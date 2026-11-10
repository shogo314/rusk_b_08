//! Explicit semantic contracts for the embedded runtime. These describe Rusk
//! operations, never C++ names or source patterns. They are a trust boundary.
pub(crate) mod aliases;
pub(crate) mod composition;

use crate::analysis::effects::{Effect, ReturnAlias, Summary};
use std::collections::BTreeMap;

/// Parameter writes/retention and returned aliases require projection mappings;
/// until those are declared, such native dependencies remain opaque.
pub(crate) fn dependency_covered(base: &Summary, dependency: &Summary) -> bool {
    dependency.external_write == Effect::No
        && dependency.io <= base.io
        && dependency.may_fail <= base.may_fail
        && dependency.may_diverge <= base.may_diverge
        && !dependency.returned.unknown
        && dependency.returned.aliases.is_empty()
        && dependency.parameters.iter().all(|p| {
            p.write == Effect::No && p.structural == Effect::No && p.retained == Effect::No
        })
}

pub(crate) fn contract(
    specification: &str,
    parameters: &[String],
    result: &str,
    types: &BTreeMap<String, crate::ir::hir::Type>,
) -> Result<Summary, String> {
    fn builtin_value(ty: &str, types: &BTreeMap<String, crate::ir::hir::Type>) -> bool {
        fn walk(ty: &str, types: &BTreeMap<String, crate::ir::hir::Type>, depth: usize) -> bool {
            if depth >= 64 {
                return false;
            }
            match types.get(ty) {
                Some(crate::ir::hir::Type::Scalar(_)) => true,
                Some(
                    crate::ir::hir::Type::Buffer(element) | crate::ir::hir::Type::Vector(element),
                ) => walk(element, types, depth + 1),
                _ => false,
            }
        }
        walk(ty, types, 0)
    }
    let specification = match specification {
        "deep_read_fresh" | "deep_resize_first" => {
            // Generated record copies can invoke user-defined copy functions;
            // foreign data can also supply arbitrary target-language helpers.
            if parameters.iter().any(|ty| !builtin_value(ty, types)) {
                return Ok(Summary::opaque(parameters.len()));
            }
            match specification {
                "deep_read_fresh" => "checked_read_fresh",
                "deep_resize_first" => "checked_resize_first",
                _ => unreachable!("deep operation matched above"),
            }
        }
        other => other,
    };
    let mut summary = Summary::native(parameters.len());
    for parameter in &mut summary.parameters {
        parameter.read = Effect::May;
    }
    summary.returned.fresh = result != "Unit";
    match specification {
        "read_fresh" => {}
        "checked_read_fresh" => summary.may_fail = Effect::May,
        "write_first" | "checked_write_first" | "checked_resize_first" | "checked_mutate_fresh" => {
            let first = summary
                .parameters
                .first_mut()
                .ok_or("native write contract requires an argument")?;
            first.write = Effect::May;
            if specification == "checked_resize_first" {
                first.structural = Effect::May;
            }
            if specification != "write_first" {
                summary.may_fail = Effect::May;
            }
            if specification != "checked_mutate_fresh" && result != "Unit" {
                return Err("native write contract requires Unit result".into());
            }
        }
        "io_read_fresh" | "io_write" | "checked_io_write" => {
            summary.io = Effect::May;
            summary.may_diverge = Effect::May; // IO can block.
            if specification == "io_read_fresh" {
                summary.may_fail = Effect::May;
                if result == "Unit" {
                    return Err("native input contract requires a result".into());
                }
            } else {
                if result != "Unit" {
                    return Err("native output contract requires Unit result".into());
                }
                summary.may_fail = if specification == "checked_io_write" {
                    Effect::May
                } else {
                    Effect::No
                };
            }
        }
        "resize_retain_second" | "replace_element" => {
            let source = if specification == "replace_element" {
                2
            } else {
                1
            };
            if parameters.len() != source + 1 {
                return Err("native element store contract has invalid arity".into());
            }
            let Some(crate::ir::hir::Type::Vector(element)) = types.get(&parameters[0]) else {
                return Err("native element store requires a vector".into());
            };
            if *element != parameters[source]
                || (source == 2
                    && !matches!(
                        types.get(&parameters[1]),
                        Some(crate::ir::hir::Type::Scalar(crate::ir::hir::Scalar::I64))
                    ))
            {
                return Err("native element store type mismatch".into());
            }
            summary.parameters[0].write = Effect::May;
            summary.parameters[0].structural = if source == 1 { Effect::May } else { Effect::No };
            summary.parameters[source].escape = Effect::May;
            summary.parameters[source].retained = Effect::May;
            if source == 1 {
                if result != "Unit" {
                    return Err("native append requires Unit result".into());
                }
            } else {
                if result != element {
                    return Err("native element store result mismatch".into());
                }
                summary.may_fail = Effect::May;
                summary.returned.fresh = false;
                summary.returned.aliases.insert(ReturnAlias {
                    result_path: vec![],
                    parameter: source,
                    parameter_path: vec![],
                });
            }
        }
        specification if specification.starts_with("variant_constructor:") => {
            let variant = specification
                .strip_prefix("variant_constructor:")
                .and_then(|n| n.parse::<usize>().ok())
                .ok_or("invalid native enum constructor contract")?;
            let Some(crate::ir::hir::Type::Enum(variants)) = types.get(result) else {
                return Err("native enum constructor contract requires an enum result".into());
            };
            if variants
                .get(variant)
                .is_none_or(|(_, fields)| fields != parameters)
            {
                return Err("native enum constructor contract has an invalid payload".into());
            }
            for parameter in 0..parameters.len() {
                summary.parameters[parameter].escape = Effect::May;
                summary.returned.aliases.insert(ReturnAlias {
                    result_path: vec![crate::analysis::effects::Projection::VariantField {
                        variant,
                        field: parameter,
                    }],
                    parameter,
                    parameter_path: vec![],
                });
            }
        }
        specification if specification.starts_with("checked_variant_field:") => {
            let mut parts = specification.split(':').skip(1);
            let mut number = || {
                parts
                    .next()
                    .and_then(|p| p.parse::<usize>().ok())
                    .ok_or("invalid native enum field contract")
            };
            let variant = number()?;
            let field = number()?;
            if parts.next().is_some() {
                return Err("invalid native enum field contract".into());
            }
            if parameters.len() != 1 {
                return Err("native enum field contract requires one receiver".into());
            }
            let Some(crate::ir::hir::Type::Enum(variants)) = types.get(&parameters[0]) else {
                return Err("native enum field contract requires an enum".into());
            };
            if variants
                .get(variant)
                .and_then(|(_, fields)| fields.get(field))
                .is_none_or(|ty| ty != result)
            {
                return Err("native enum field contract has an invalid payload".into());
            }
            summary.returned.fresh = false;
            summary.returned.aliases.insert(ReturnAlias {
                result_path: vec![],
                parameter: 0,
                parameter_path: vec![crate::analysis::effects::Projection::VariantField {
                    variant,
                    field,
                }],
            });
            summary.parameters[0].escape = Effect::May;
            summary.may_fail = Effect::May;
        }
        "element" => {
            if parameters.len() != 2
                || !matches!(
                    types.get(&parameters[1]),
                    Some(crate::ir::hir::Type::Scalar(crate::ir::hir::Scalar::I64))
                )
            {
                return Err("native element contract requires a container and an integer index representation".into());
            }
            summary.returned.fresh = false;
            summary.returned.aliases.insert(ReturnAlias {
                result_path: vec![],
                parameter: 0,
                parameter_path: vec![crate::analysis::effects::Projection::Element],
            });
            summary.parameters[0].escape = Effect::May;
            summary.may_fail = Effect::May;
        }
        _ => return Err(format!("unknown native effect contract '{specification}'")),
    }
    summary.validate_signature(parameters, result, types, false)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
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
}
