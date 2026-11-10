mod ast;
mod backend;
mod bindings;
mod compiler;
mod effects;
mod hir;
mod hir_flow;
mod hir_validate;
mod lir;
mod lir_cleanup;
mod native_abi;
mod native_effects;
mod native_lowering;
mod readonly;
mod rusk_grammar;
mod source;
mod stdlib;
#[allow(warnings)]
mod rusk_grammar_trait {
    include!(concat!(env!("OUT_DIR"), "/rusk_grammar_trait.rs"));
}
#[allow(warnings)]
mod rusk_parser {
    include!(concat!(env!("OUT_DIR"), "/rusk_parser.rs"));
}

use parol_runtime::Report;
struct ErrorReporter;
impl Report for ErrorReporter {}

fn parse_cst<'t>(text: &'t str, file: &str) -> Result<rusk_grammar_trait::Rusk<'t>, String> {
    let mut grammar = rusk_grammar::RuskGrammar::default();
    rusk_parser::parse(text, file, &mut grammar).map_err(|e| {
        if ErrorReporter::report_error(&e, file).is_err() {
            eprintln!("{e:?}");
        }
        format!("{file}: syntax error")
    })?;
    grammar
        .root
        .ok_or_else(|| format!("{file}: missing parse result"))
}
fn parse(text: &str, file: &str) -> Result<Vec<ast::Stmt>, String> {
    ast::lower(&parse_cst(text, file)?)
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or(
        "usage: rusk compile FILE [-o OUTPUT] [--no-opt] [--emit cst|ast|hir|lir|cpp] | dependencies FILE",
    )?;
    if command == "--help" || command == "-h" {
        println!(
            "usage: rusk compile FILE [-o OUTPUT] [--no-opt] [--emit cst|ast|hir|lir|cpp] | dependencies FILE\nGenerate C++20 source (default: a.cpp), or list source dependencies."
        );
        return Ok(());
    }
    if command != "compile" && command != "dependencies" {
        return Err(format!("unknown command '{command}'"));
    }
    let file = args.next().ok_or("missing source file")?;
    if command == "dependencies" {
        if let Some(arg) = args.next() {
            return Err(format!("unknown argument '{arg}'"));
        }
        for path in source::load(std::path::Path::new(&file))?.dependencies {
            println!("{}", path.display());
        }
        return Ok(());
    }
    let mut output = "a.cpp".to_string();
    let mut optimize = true;
    let mut emit = "cpp".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-opt" => optimize = false,
            "--emit" => {
                emit = args
                    .next()
                    .ok_or("--emit requires cst, ast, hir, lir, or cpp")?;
                if !matches!(emit.as_str(), "cst" | "ast" | "hir" | "lir" | "cpp") {
                    return Err("--emit requires cst, ast, hir, lir, or cpp".into());
                }
            }
            "-o" | "--output" => output = args.next().ok_or("missing output path")?,
            _ => return Err(format!("unknown argument '{arg}'")),
        }
    }
    if matches!(emit.as_str(), "cst" | "ast") {
        let result = if emit == "cst" {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
            format!("{:#?}\n", parse_cst(&text, &file)?)
        } else {
            format!(
                "{:#?}\n",
                source::load(std::path::Path::new(&file))?.statements
            )
        };
        std::fs::write(&output, result).map_err(|e| format!("{output}: {e}"))?;
        return Ok(());
    }
    let emit = match emit.as_str() {
        "hir" => backend::Emit::Hir,
        "lir" => backend::Emit::Lir,
        _ => backend::Emit::Cpp,
    };
    let source = source::load(std::path::Path::new(&file))?.statements;
    let cpp = compiler::Compiler::with_optimization(optimize)
        .compile(&[stdlib::load()?, compiler::Source::user(source)], emit)?;
    std::fs::write(&output, cpp).map_err(|e| format!("{output}: {e}"))?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
