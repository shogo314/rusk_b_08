//! Parsing, AST construction, and source/resource loading.
pub(crate) mod ast;
mod rusk_grammar;
pub(crate) mod source;
#[allow(warnings)]
pub(crate) mod rusk_grammar_trait {
    include!(concat!(env!("OUT_DIR"), "/rusk_grammar_trait.rs"));
}
#[allow(warnings)]
mod rusk_parser {
    include!(concat!(env!("OUT_DIR"), "/rusk_parser.rs"));
}

use parol_runtime::Report;
struct ErrorReporter;
impl Report for ErrorReporter {}

pub(crate) fn parse_cst<'t>(
    text: &'t str,
    file: &str,
) -> Result<rusk_grammar_trait::Rusk<'t>, String> {
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
pub(crate) fn parse(text: &str, file: &str) -> Result<Vec<ast::Stmt>, String> {
    ast::lower(&parse_cst(text, file)?)
}
