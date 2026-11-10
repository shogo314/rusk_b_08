use super::*;
fn types() -> BTreeMap<String, Type> {
    [
        ("Int".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
        ("Other".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
        ("Unit".into(), Type::Scalar(crate::ir::hir::Scalar::Unit)),
        (
            "Source".into(),
            Type::Shared(vec![("value".into(), "Int".into())]),
        ),
        (
            "Cursor".into(),
            Type::Iterator {
                element: "Int".into(),
                retained: vec![("owner".into(), "Source".into())],
            },
        ),
    ]
    .into()
}
#[test]
fn aliases_preserve_nested_owner_identity_and_distinguish_return_escape_from_retention() {
    let types = types();
    let parameters = vec!["Cursor".into()];
    let mut summary = contract("read_fresh", &parameters, "Cursor", &types).unwrap();
    apply(
        &mut summary,
        "return.owner=0.owner",
        &parameters,
        "Cursor",
        &types,
    )
    .unwrap();
    assert!(summary.returned.fresh);
    assert_eq!(summary.parameters[0].escape, Effect::May);
    assert_eq!(summary.parameters[0].retained, Effect::No);
    assert!(summary.returned.aliases.contains(&ReturnAlias {
        result_path: vec![Projection::Retained(0)],
        parameter: 0,
        parameter_path: vec![Projection::Retained(0)]
    }));
    let mut identity = contract("read_fresh", &parameters, "Cursor", &types).unwrap();
    apply(&mut identity, "return=0", &parameters, "Cursor", &types).unwrap();
    assert!(!identity.returned.fresh);
    let mut value = contract("read_fresh", &parameters, "Int", &types).unwrap();
    apply(
        &mut value,
        "return=0.owner.value",
        &parameters,
        "Int",
        &types,
    )
    .unwrap();
    assert_eq!(
        value.returned.aliases.first().unwrap().parameter_path,
        vec![Projection::Retained(0), Projection::Retained(0)]
    );
}
#[test]
fn aliases_validate_logical_types_paths_bounds_and_unknown_effects() {
    let types = types();
    let parameters = vec!["Cursor".into()];
    for text in ["return.owner=0", "return.missing=0.owner", "return=0.owner"] {
        let mut summary = contract("read_fresh", &parameters, "Cursor", &types).unwrap();
        assert!(
            apply(&mut summary, text, &parameters, "Cursor", &types).is_err(),
            "{text}"
        );
    }
    for text in [
        "",
        "return.=0",
        "return=0.",
        "return.owner=1",
        "value=0",
        "return=+0",
        "return=0..owner",
    ] {
        assert!(parse(text, 1).is_err(), "{text}");
    }
    assert!(parse(&vec!["return=0"; 65].join(";"), 1).is_err());
    assert!(parse(&format!("return=0.{}", vec!["value"; 17].join(".")), 1).is_err());
    let mut summary = contract("read_fresh", &parameters, "Other", &types).unwrap();
    assert!(
        apply(
            &mut summary,
            "return=0.owner.value",
            &parameters,
            "Other",
            &types
        )
        .is_err()
    );
    let mut opaque = Summary::opaque(1);
    apply(&mut opaque, "return=0", &parameters, "Cursor", &types).unwrap();
    assert_eq!(opaque, Summary::opaque(1));
}
