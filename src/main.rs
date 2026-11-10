mod analysis;
mod backend;
mod cli;
mod compiler;
mod frontend;
mod ir;
mod native;
mod stdlib;

use clap::Parser;
use cli::{Cli, Command, CompileArgs, EmitStage};
use frontend::{parse_cst, source};

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Compile(args) => compile(args),
        Command::Dependencies { file } => {
            for path in source::load(&file)?.dependencies {
                println!("{}", path.display());
            }
            Ok(())
        }
    }
}

fn compile(args: CompileArgs) -> Result<(), String> {
    let CompileArgs {
        file,
        output,
        no_opt,
        emit,
    } = args;
    if matches!(emit, EmitStage::Cst | EmitStage::Ast) {
        let result = if matches!(emit, EmitStage::Cst) {
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            format!("{:#?}\n", parse_cst(&text, &file.to_string_lossy())?)
        } else {
            format!("{:#?}\n", source::load(&file)?.statements)
        };
        std::fs::write(&output, result).map_err(|e| format!("{}: {e}", output.display()))?;
        return Ok(());
    }
    let emit = match emit {
        EmitStage::Hir => backend::Emit::Hir,
        EmitStage::Lir => backend::Emit::Lir,
        EmitStage::Cpp => backend::Emit::Cpp,
        EmitStage::Cst | EmitStage::Ast => unreachable!(),
    };
    let source = source::load(&file)?.statements;
    let cpp = compiler::Compiler::with_optimization(!no_opt)
        .compile(&[stdlib::load()?, compiler::Source::user(source)], emit)?;
    std::fs::write(&output, cpp).map_err(|e| format!("{}: {e}", output.display()))?;
    Ok(())
}
fn main() {
    if let Err(e) = run(Cli::parse().command) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
