use super::*;
fn info() -> AccessInfo {
    AccessInfo {
        name: "x".into(),
        declaration: Span {
            file: "test".into(),
            line: 1,
            column: 1,
        },
        annotation: crate::frontend::ast::AccessAnnotation {
            mode: AccessMode::Readonly,
            span: Span {
                file: "test".into(),
                line: 1,
                column: 1,
            },
        },
    }
}
#[test]
fn ancestor_restrictions_dominate_descendants_and_widening_is_stable() {
    let root = AccessPaths::root(info());
    let mut nested = root.contain(Projection::Field(0));
    assert!(nested.project(Projection::Field(1)).0.is_empty());
    assert_eq!(nested.project(Projection::Field(0)), root);
    nested.join(&root);
    assert_eq!(nested, root);
    let mut wide = root.clone();
    for _ in 0..17 {
        wide = wide.contain(Projection::Field(0));
    }
    assert_eq!(wide.0, root.0);
    assert!(wide.1);
    let mut combined = wide.clone();
    combined.join(&wide.contain(Projection::Field(0)));
    assert_eq!(combined, wide);
}
#[test]
fn returned_and_retained_restrictions_have_valid_paths_and_readonly_sources() {
    let types = [(
        "Int".into(),
        crate::ir::hir::Type::Scalar(crate::ir::hir::Scalar::I64),
    )]
    .into();
    let mut summary = Summary {
        returned: AccessPaths::root(info()),
        retained: AccessPaths::default(),
    };
    assert!(summary.validate("Int", &types).is_ok());
    summary.returned = summary.returned.contain(Projection::Field(0));
    assert!(summary.validate("Int", &types).is_err());
    summary.returned = AccessPaths::default();
    let mut mutable = info();
    mutable.annotation.mode = AccessMode::Mutable;
    summary.retained = AccessPaths::root(mutable);
    assert!(summary.validate("Int", &types).is_err());
}
