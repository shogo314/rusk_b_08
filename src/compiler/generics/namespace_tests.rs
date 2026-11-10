use super::*;

#[test]
fn library_type_names_are_not_reserved_without_library_declarations() {
    for name in ["Vec", "Option", "Iter"] {
        for declaration in [
            format!("struct {name}<T>{{value:T;}}"),
            format!("enum {name}<T>{{Value(T),Empty}}"),
            format!("extern type<T> {name}{{cpp_name:\"{{T}}\",}}"),
        ] {
            let source = format!("extern type Unit{{cpp_name:\"Unit\",}}{declaration}");
            Compiler::default()
                .compile(
                    &[Source::user(
                        crate::frontend::parse(&source, "type-namespace.rusk").unwrap(),
                    )],
                    crate::backend::Emit::Hir,
                )
                .unwrap_or_else(|error| panic!("{source}\n{error}"));
        }
    }
}

#[test]
fn library_type_conflicts_use_the_same_family_check_as_unrelated_types() {
    for name in ["Vec", "Option", "Iter", "Unrelated"] {
        for first in [
            format!("struct {name}<T>{{value:T;}}"),
            format!("enum {name}<T>{{Value(T)}}"),
            format!("extern type<T> {name}{{cpp_name:\"{{T}}\",}}"),
        ] {
            let second = format!("extern type {name}<Int>{{cpp_name:\"int\",}}");
            for source in [format!("{first}{second}"), format!("{second}{first}")] {
                let error = Compiler::default()
                    .compile(
                        &[Source::user(
                            crate::frontend::parse(&source, "type-namespace.rusk").unwrap(),
                        )],
                        crate::backend::Emit::Hir,
                    )
                    .unwrap_err();
                assert!(
                    error.contains(&format!("duplicate type family '{name}'")),
                    "{error}"
                );
            }
        }
    }
}

#[test]
fn language_and_internal_type_names_are_protected_for_every_declaration_form() {
    for (name, diagnostic) in [
        ("Tuple", "reserved tuple type constructor"),
        ("__RuskGenerated", "reserved internal type name"),
    ] {
        for source in [
            format!("struct {name}{{}}"),
            format!("struct {name}<T>{{value:T;}}"),
            format!("enum {name}{{Empty}}"),
            format!("enum {name}<T>{{Full(T)}}"),
            format!("extern type {name}{{cpp_name:\"int\",}}"),
            format!("extern type<T> {name}{{cpp_name:\"{{T}}\",}}"),
            format!("extern type {name}<Int>{{cpp_name:\"int\",}}"),
        ] {
            let error = Compiler::default()
                .compile(
                    &[Source::user(
                        crate::frontend::parse(&source, "type-namespace.rusk").unwrap(),
                    )],
                    crate::backend::Emit::Hir,
                )
                .unwrap_err();
            assert!(
                error.contains("type-namespace.rusk:") && error.contains(diagnostic),
                "{error}"
            );
        }
    }
}
