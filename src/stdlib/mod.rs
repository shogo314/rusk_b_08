//! Embedded resources use the same include/code_file loader as user sources.
const FILES: &[(&str, &str)] = &[
    ("prelude.rusk", include_str!("../../lib/prelude.rusk")),
    ("runtime.rusk", include_str!("../../lib/runtime.rusk")),
    ("core.rusk", include_str!("../../lib/core.rusk")),
    (
        "collections.rusk",
        include_str!("../../lib/collections.rusk"),
    ),
    ("vec.rusk", include_str!("../../lib/vec.rusk")),
    (
        "hash_collections.rusk",
        include_str!("../../lib/hash_collections.rusk"),
    ),
    (
        "lazy_iterators.rusk",
        include_str!("../../lib/lazy_iterators.rusk"),
    ),
    ("algorithms.rusk", include_str!("../../lib/algorithms.rusk")),
    ("base.hpp", include_str!("../../lib/base.hpp")),
    ("io.hpp", include_str!("../../lib/io.hpp")),
    ("token_io.hpp", include_str!("../../lib/token_io.hpp")),
    ("scalar.hpp", include_str!("../../lib/scalar.hpp")),
    (
        "sequence_runtime.hpp",
        include_str!("../../lib/sequence_runtime.hpp"),
    ),
    ("containers.hpp", include_str!("../../lib/containers.hpp")),
    (
        "lazy_iterators.hpp",
        include_str!("../../lib/lazy_iterators.hpp"),
    ),
];
pub(crate) fn load() -> Result<crate::compiler::Source, String> {
    Ok(crate::compiler::Source::standard(
        crate::frontend::source::embedded("prelude.rusk", FILES)?.statements,
    ))
}
#[cfg(test)]
pub(crate) fn runtime() -> Vec<crate::frontend::ast::Stmt> {
    crate::frontend::source::embedded("runtime.rusk", FILES)
        .unwrap()
        .statements
}
