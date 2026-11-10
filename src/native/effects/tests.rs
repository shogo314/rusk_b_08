use super::*;
use crate::{
    analysis::effects::{Projection, SummarySource},
    ir::hir::Type,
};
fn types() -> BTreeMap<String, Type> {
    [
        ("Int".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
        ("Char".into(), Type::Scalar(crate::ir::hir::Scalar::Char)),
        ("Str".into(), Type::Buffer("Char".into())),
        ("Unit".into(), Type::Scalar(crate::ir::hir::Scalar::Unit)),
        ("Vec<Int>".into(), Type::Vector("Int".into())),
    ]
    .into()
}
#[test]
fn recursive_native_shapes_do_not_imply_pure_deep_copy_contracts() {
    let types = [("Loop".into(), Type::Buffer("Loop".into()))].into();
    let effect = contract("deep_read_fresh", &["Loop".into()], "Loop", &types).unwrap();
    assert_eq!(effect.source, SummarySource::Opaque);
}
#[test]
fn element_contract_returns_a_reference_without_retaining_it_elsewhere() {
    for (container, element) in [("Vec<Int>", "Int"), ("Str", "Char")] {
        let summary = contract(
            "element",
            &[container.into(), "Int".into()],
            element,
            &types(),
        )
        .unwrap();
        assert_eq!(summary.source, SummarySource::NativeContract);
        assert_eq!(summary.parameters[0].escape, Effect::May);
        assert_eq!(summary.parameters[0].retained, Effect::No);
        assert_eq!(summary.parameters[0].write, Effect::No);
        assert_eq!(
            summary
                .returned
                .aliases
                .iter()
                .next()
                .unwrap()
                .parameter_path,
            vec![Projection::Element]
        );
        assert_eq!(summary.may_fail, Effect::May);
    }
}
#[test]
fn checked_reads_writes_and_io_have_distinct_effects() {
    let read = contract("checked_read_fresh", &["Vec<Int>".into()], "Int", &types()).unwrap();
    assert!(read.returned.fresh);
    assert_eq!(read.parameters[0].escape, Effect::No);
    assert_eq!(read.may_fail, Effect::May);
    assert_eq!(read.io, Effect::No);
    let write = contract(
        "checked_resize_first",
        &["Str".into(), "Char".into()],
        "Unit",
        &types(),
    )
    .unwrap();
    assert_eq!(write.parameters[0].write, Effect::May);
    assert_eq!(write.parameters[0].structural, Effect::May);
    assert_eq!(write.parameters[1].write, Effect::No);
    assert_eq!(write.parameters[1].retained, Effect::No);
    let input = contract("io_read_fresh", &[], "Int", &types()).unwrap();
    assert_eq!(input.io, Effect::May);
    assert_eq!(input.may_fail, Effect::May);
    assert_eq!(input.may_diverge, Effect::May);
    assert_eq!(input.external_write, Effect::No);
}
#[test]
fn malformed_contracts_and_alias_types_are_rejected() {
    for (spec, args, result) in [
        ("unknown", vec![], "Int"),
        ("write_first", vec![], "Unit"),
        ("write_first", vec!["Int".into()], "Int"),
        ("io_write", vec!["Int".into()], "Int"),
        ("checked_io_write", vec!["Int".into()], "Int"),
        ("io_read_fresh", vec![], "Unit"),
        ("element", vec!["Int".into(), "Int".into()], "Int"),
        ("element", vec!["Str".into(), "Int".into()], "Int"),
    ] {
        assert!(contract(spec, &args, result, &types()).is_err(), "{spec}");
    }
}
#[test]
fn vector_stores_retain_the_source_and_deep_operations_do_not_trust_user_helpers() {
    let mut types = types();
    let append = contract(
        "resize_retain_second",
        &["Vec<Int>".into(), "Int".into()],
        "Unit",
        &types,
    )
    .unwrap();
    assert_eq!(append.parameters[0].write, Effect::May);
    assert_eq!(append.parameters[1].write, Effect::No);
    assert_eq!(append.parameters[1].retained, Effect::May);
    let store = contract(
        "replace_element",
        &["Vec<Int>".into(), "Int".into(), "Int".into()],
        "Int",
        &types,
    )
    .unwrap();
    assert_eq!(store.parameters[0].structural, Effect::No);
    assert_eq!(store.returned.aliases.first().unwrap().parameter, 2);
    let copy = contract("deep_read_fresh", &["Vec<Int>".into()], "Vec<Int>", &types).unwrap();
    assert!(copy.returned.fresh);
    assert_eq!(copy.parameters[0].write, Effect::No);
    types.insert(
        "Custom".into(),
        Type::Record(vec![("x".into(), "Int".into())]),
    );
    types.insert("Vec<Custom>".into(), Type::Vector("Custom".into()));
    for spec in ["deep_read_fresh", "deep_resize_first"] {
        let opaque = contract(spec, &["Vec<Custom>".into()], "Vec<Custom>", &types).unwrap();
        assert_eq!(opaque.source, SummarySource::Opaque);
        assert_eq!(opaque.parameters[0].write, Effect::Unknown);
    }
    assert!(
        contract(
            "replace_element",
            &["Vec<Int>".into(), "Str".into(), "Int".into()],
            "Int",
            &types
        )
        .is_err()
    );
}
#[test]
fn enum_field_returns_a_checked_payload_alias_without_external_retention() {
    let mut types = types();
    types.insert(
        "Option<Int>".into(),
        Type::Enum(vec![
            ("Some".into(), vec!["Int".into()]),
            ("None".into(), vec![]),
        ]),
    );
    let summary = contract(
        "checked_variant_field:0:0",
        &["Option<Int>".into()],
        "Int",
        &types,
    )
    .unwrap();
    assert_eq!(summary.parameters[0].escape, Effect::May);
    assert_eq!(summary.parameters[0].retained, Effect::No);
    assert_eq!(summary.parameters[0].write, Effect::No);
    assert_eq!(
        summary.returned.aliases.first().unwrap().parameter_path,
        vec![Projection::VariantField {
            variant: 0,
            field: 0
        }]
    );
    assert_eq!(summary.may_fail, Effect::May);
    assert!(
        contract(
            "checked_variant_field:0:0",
            &["Option<Int>".into()],
            "Str",
            &types
        )
        .is_err()
    );
    types.insert(
        "Choice".into(),
        Type::Enum(vec![
            ("Empty".into(), vec![]),
            ("Pair".into(), vec!["Str".into(), "Int".into()]),
        ]),
    );
    let constructor = contract(
        "variant_constructor:1",
        &["Str".into(), "Int".into()],
        "Choice",
        &types,
    )
    .unwrap();
    assert!(constructor.returned.fresh);
    assert_eq!(constructor.returned.aliases.len(), 2);
    assert_eq!(constructor.parameters[0].retained, Effect::No);
    let field = contract(
        "checked_variant_field:1:1",
        &["Choice".into()],
        "Int",
        &types,
    )
    .unwrap();
    assert_eq!(
        field.returned.aliases.first().unwrap().parameter_path,
        vec![Projection::VariantField {
            variant: 1,
            field: 1
        }]
    );
    for specification in [
        "variant_constructor:",
        "variant_constructor:x",
        "variant_constructor:3",
    ] {
        assert!(
            contract(specification, &[], "Choice", &types).is_err(),
            "{specification}"
        );
    }
    for specification in [
        "checked_variant_field:1:x",
        "checked_variant_field:1:1:0",
        "checked_variant_field:0:0",
        "checked_variant_field:1:2",
    ] {
        assert!(
            contract(specification, &["Choice".into()], "Int", &types).is_err(),
            "{specification}"
        );
    }
    assert!(
        contract(
            "variant_constructor:1",
            &["Int".into(), "Str".into()],
            "Choice",
            &types
        )
        .is_err()
    );
    assert!(contract("checked_variant_field:0:0", &["Int".into()], "Int", &types).is_err());
}
