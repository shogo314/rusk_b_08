use super::*;
fn types() -> BTreeMap<String, Type> {
    [
        ("Int".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
        ("Other".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
        (
            "Box".into(),
            Type::Shared(vec![("value".into(), "Int".into())]),
        ),
        (
            "Outer".into(),
            Type::Shared(vec![("child".into(), "Box".into())]),
        ),
    ]
    .into()
}
#[test]
fn mapped_calls_compose_nested_aliases_and_keep_discarded_returns_local() {
    let types = types();
    let parameters = vec!["Outer".into()];
    let base = contract("read_fresh", &parameters, "Outer", &types).unwrap();
    let mut child = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
    aliases::apply(
        &mut child,
        "return.value=0.value",
        &["Box".into()],
        "Box",
        &types,
    )
    .unwrap();
    let routed = Specification::parse("(0.child) into return.child")
        .unwrap()
        .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
        .unwrap();
    let result = compose(&base, &[routed], &[child.clone()]).unwrap();
    result
        .validate_signature(&parameters, "Outer", &types, false)
        .unwrap();
    assert!(result.returned.fresh);
    assert_eq!(result.parameters[0].retained, Effect::No);
    assert!(result.returned.aliases.contains(&ReturnAlias {
        result_path: vec![Projection::Retained(0), Projection::Retained(0)],
        parameter: 0,
        parameter_path: vec![Projection::Retained(0), Projection::Retained(0)]
    }));
    let discard = Specification::parse("(0.child) into discard")
        .unwrap()
        .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
        .unwrap();
    let result = compose(&base, &[discard], &[child]).unwrap();
    assert_eq!(result.parameters[0].escape, Effect::No);
    assert!(result.returned.aliases.is_empty());
}
#[test]
fn mapped_calls_propagate_mutation_retention_io_failure_and_uncertainty() {
    let types = types();
    let parameters = vec!["Outer".into()];
    let base = contract("read_fresh", &parameters, "Outer", &types).unwrap();
    let routed = Specification::parse("(0.child) into return.child")
        .unwrap()
        .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
        .unwrap();
    let mut child = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
    child.parameters[0].write = Effect::May;
    child.parameters[0].structural = Effect::May;
    child.parameters[0].retained = Effect::May;
    child.parameters[0].escape = Effect::May;
    child.io = Effect::May;
    child.may_fail = Effect::May;
    child.may_diverge = Effect::May;
    let result = compose(&base, std::slice::from_ref(&routed), &[child]).unwrap();
    result
        .validate_signature(&parameters, "Outer", &types, false)
        .unwrap();
    assert_eq!(result.parameters[0].retained, Effect::May);
    assert_eq!(result.parameters[0].write, Effect::May);
    assert_eq!(result.parameters[0].structural, Effect::May);
    assert_eq!(result.io, Effect::May);
    assert_eq!(result.may_fail, Effect::May);
    assert!(result.returned.unknown);
    assert!(result.returned.aliases.is_empty());
    assert!(compose(&base, std::slice::from_ref(&routed), &[Summary::opaque(1)]).is_none());
    assert!(compose(&base, &[routed], &[Summary::unanalyzed(1)]).is_none());
}
#[test]
fn mapped_base_writes_widen_later_effects_across_all_input_roots() {
    let types = types();
    let parameters = vec!["Outer".into(), "Outer".into()];
    let base = contract("checked_mutate_fresh", &parameters, "Outer", &types).unwrap();
    let mut child = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
    child.parameters[0].retained = Effect::May;
    child.parameters[0].escape = Effect::May;
    let route = Specification::parse("(0.child) into return.child")
        .unwrap()
        .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
        .unwrap();
    let result = compose(&base, &[route], &[child]).unwrap();
    result
        .validate_signature(&parameters, "Outer", &types, false)
        .unwrap();
    assert_eq!(result.parameters[1].write, Effect::May);
    assert_eq!(result.parameters[1].retained, Effect::May);
    assert!(result.returned.unknown);
    assert_eq!(compose(&base, &[], &[]), Some(base));
}

#[test]
fn mapped_stored_results_retain_aliases_but_independent_copies_do_not() {
    let types = types();
    let parameters = vec!["Outer".into(), "Box".into()];
    let base = contract("read_fresh", &parameters, "Unit", &types).unwrap();
    let mut shared = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
    aliases::apply(
        &mut shared,
        "return.value=0.value",
        &["Box".into()],
        "Box",
        &types,
    )
    .unwrap();
    let route = Specification::parse("(1) into 0.child")
        .unwrap()
        .resolve(0, &parameters, "Unit", &["Box".into()], "Box", &types)
        .unwrap();
    let result = compose(&base, std::slice::from_ref(&route), &[shared]).unwrap();
    result
        .validate_signature(&parameters, "Unit", &types, false)
        .unwrap();
    assert!(result.parameters.iter().all(|p| p.retained == Effect::May));
    assert!(result.parameters.iter().all(|p| p.write == Effect::May));
    assert!(result.returned.aliases.is_empty());

    let independent = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
    let result = compose(&base, std::slice::from_ref(&route), &[independent]).unwrap();
    assert!(result.parameters.iter().all(|p| p.retained == Effect::No));
    assert_eq!(result.parameters[0].structural, Effect::May);

    let mut unknown = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
    unknown.returned = crate::analysis::effects::ReturnOrigins {
        unknown: true,
        ..Default::default()
    };
    let result = compose(&base, std::slice::from_ref(&route), &[unknown]).unwrap();
    assert!(result.parameters.iter().all(|p| p.retained == Effect::May));
    assert!(compose(&base, &[route], &[Summary::opaque(1)]).is_none());
}

#[test]
fn mapped_storage_validates_root_contents_nested_fields_and_logical_types() {
    let types = types();
    let parameters = vec!["Outer".into(), "Box".into()];
    Specification::parse("(1) into 1")
        .unwrap()
        .resolve(0, &parameters, "Unit", &["Box".into()], "Box", &types)
        .unwrap();
    Specification::parse("(1.value) into 0.child.value")
        .unwrap()
        .resolve(0, &parameters, "Unit", &["Int".into()], "Int", &types)
        .unwrap();
    for target in ["2.child", "0.missing", "0", "0.child.value"] {
        assert!(
            Specification::parse(&format!("(1) into {target}"))
                .unwrap()
                .resolve(0, &parameters, "Unit", &["Box".into()], "Box", &types)
                .is_err(),
            "{target}"
        );
    }
    assert!(
        Specification::parse("(1.value) into 0.child.value")
            .unwrap()
            .resolve(0, &parameters, "Unit", &["Int".into()], "Other", &types)
            .is_err()
    );
    for target in ["0.", "-1.child", "0..child"] {
        assert!(Specification::parse(&format!("(1) into {target}")).is_err());
    }
    assert!(
        Specification::parse("(0) into 1")
            .unwrap()
            .validate_bounds(1)
            .is_err()
    );
}

#[test]
fn mapped_call_routes_validate_shapes_logical_types_arity_and_limits() {
    let types = types();
    let parameters = vec!["Outer".into()];
    for text in [
        "0.child into return.child",
        "(0.child)",
        "(0.child) into value",
        "(0.) into return",
        "(0.child,) into return",
    ] {
        assert!(Specification::parse(text).is_err(), "{text}");
    }
    for text in [
        "(1.child) into return.child",
        "() into return.child",
        "(0.missing) into return.child",
        "(0.child) into return",
    ] {
        assert!(
            Specification::parse(text)
                .unwrap()
                .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
                .is_err(),
            "{text}"
        );
    }
    assert!(Specification::parse(&format!("({}) into return", vec!["0"; 65].join(","))).is_err());
    assert!(
        Specification::parse("(0.child.value) into return")
            .unwrap()
            .resolve(0, &parameters, "Other", &["Int".into()], "Int", &types)
            .is_err()
    );
}
