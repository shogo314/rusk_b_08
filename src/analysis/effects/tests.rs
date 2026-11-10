use super::*;
use crate::ir::hir::{self, Body, Instruction as I, Literal};

fn param(i: usize, ty: &str) -> Value {
    Value {
        id: ValueId::Parameter(i),
        ty: ty.into(),
    }
}
fn local(i: usize, ty: &str) -> Value {
    Value {
        id: ValueId::Local(i),
        ty: ty.into(),
    }
}
fn function(params: &[&str], result: &str, body: Option<Vec<I>>) -> hir::Function {
    hir::Function {
        id: 0,
        name: "test".into(),
        params: params.iter().map(|s| (*s).into()).collect(),
        result: result.into(),
        effects: Summary::unanalyzed(params.len()),
        access: crate::analysis::readonly::Summary::default(),
        body: body.map(|instructions| Body { instructions }),
    }
}
fn program(mut functions: Vec<hir::Function>) -> lir::Program {
    for (id, f) in functions.iter_mut().enumerate() {
        f.id = id;
    }
    let mut hir = hir::Program {
        types: [
            ("Int".into(), hir::Type::Scalar(crate::ir::hir::Scalar::I64)),
            (
                "Bool".into(),
                hir::Type::Scalar(crate::ir::hir::Scalar::Bool),
            ),
            (
                "Unit".into(),
                hir::Type::Scalar(crate::ir::hir::Scalar::Unit),
            ),
            (
                "Maybe".into(),
                hir::Type::Enum(vec![
                    ("Empty".into(), vec![]),
                    ("Full".into(), vec!["Int".into()]),
                ]),
            ),
            (
                "Box".into(),
                hir::Type::Record(vec![("value".into(), "Int".into())]),
            ),
        ]
        .into(),
        functions,
        main: Body {
            instructions: vec![I::Return {
                value: None,
                span: None,
            }],
        },
    };
    hir.prune_unreachable();
    hir.validate().unwrap();
    let lir = lir::lower_program(&hir);
    lir.validate().unwrap();
    lir
}
fn analyze_checked(functions: Vec<hir::Function>) -> Vec<Summary> {
    let mut program = program(functions);
    let results = analyze(&program);
    for (f, summary) in program.functions.iter_mut().zip(&results) {
        f.effects = summary.clone();
    }
    program.validate().unwrap();
    results
}
fn call(id: usize, args: Vec<Value>, result: Value) -> I {
    I::Call(hir::Call {
        function: id,
        args,
        result,
        inherit_location: false,
        span: crate::frontend::ast::Span {
            file: "effects-test".into(),
            line: 1,
            column: 1,
        },
    })
}
fn alias(
    parameter: usize,
    parameter_path: Vec<Projection>,
    result_path: Vec<Projection>,
) -> ReturnAlias {
    ReturnAlias {
        parameter,
        parameter_path,
        result_path,
    }
}

#[test]
fn identity_literal_and_local_rebinding_have_distinct_origins() {
    let summaries = analyze_checked(vec![
        function(
            &["Int"],
            "Int",
            Some(vec![I::Return {
                value: Some(param(0, "Int")),
                span: None,
            }]),
        ),
        function(
            &["Int"],
            "Int",
            Some(vec![
                I::Declare {
                    result: local(0, "Int"),
                    initial: Some(param(0, "Int")),
                },
                I::Literal {
                    result: local(1, "Int"),
                    value: Literal::Int(7),
                },
                I::Rebind {
                    span: None,
                    target: Place::Local(local(0, "Int").id),
                    value: local(1, "Int"),
                },
                I::Return {
                    value: Some(local(0, "Int")),
                    span: None,
                },
            ]),
        ),
    ]);
    assert_eq!(summaries[0].returned, ReturnOrigins::parameter(0));
    assert_eq!(summaries[0].parameters[0].write, Effect::No);
    assert_eq!(summaries[0].parameters[0].escape, Effect::May);
    assert_eq!(summaries[1].returned, ReturnOrigins::fresh());
    assert_eq!(summaries[1].parameters[0].escape, Effect::No);
    assert_eq!(summaries[1].parameters[0].write, Effect::No);
}
#[test]
fn returned_sharing_only_escapes_again_when_the_caller_exports_it() {
    let summaries = analyze_checked(vec![
        function(
            &["Bool"],
            "Unit",
            Some(vec![
                call(3, vec![param(0, "Bool")], local(0, "Bool")),
                I::Return {
                    value: None,
                    span: None,
                },
            ]),
        ),
        function(
            &["Bool"],
            "Bool",
            Some(vec![
                call(3, vec![param(0, "Bool")], local(0, "Bool")),
                I::Return {
                    value: Some(local(0, "Bool")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Bool"],
            "Bool",
            Some(vec![
                call(3, vec![param(0, "Bool")], local(0, "Bool")),
                I::Object {
                    result: local(1, "Bool"),
                    expression: Rvalue::BoolCopy(local(0, "Bool")),
                },
                I::Return {
                    value: Some(local(1, "Bool")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Bool"],
            "Bool",
            Some(vec![I::Return {
                value: Some(param(0, "Bool")),
                span: None,
            }]),
        ),
    ]);
    assert_eq!(summaries[0].parameters[0].escape, Effect::No);
    assert_eq!(summaries[1].parameters[0].escape, Effect::May);
    assert_eq!(summaries[2].parameters[0].escape, Effect::No);
    assert!(summaries[2].returned.fresh);
    for summary in summaries {
        assert_eq!(summary.parameters[0].retained, Effect::No);
    }
}
#[test]
fn fields_and_contained_references_survive_function_calls() {
    let summaries = analyze_checked(vec![
        function(
            &["Int"],
            "Int",
            Some(vec![
                call(1, vec![param(0, "Int")], local(0, "Box")),
                I::Object {
                    result: local(1, "Int"),
                    expression: Rvalue::Field {
                        owner: local(0, "Box"),
                        index: 0,
                    },
                },
                I::Return {
                    value: Some(local(1, "Int")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Int"],
            "Box",
            Some(vec![
                I::Object {
                    result: local(0, "Box"),
                    expression: Rvalue::Aggregate(vec![param(0, "Int")]),
                },
                I::Return {
                    value: Some(local(0, "Box")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Box"],
            "Int",
            Some(vec![
                I::Object {
                    result: local(0, "Int"),
                    expression: Rvalue::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                },
                I::Return {
                    value: Some(local(0, "Int")),
                    span: None,
                },
            ]),
        ),
    ]);
    assert_eq!(
        summaries[0].returned.aliases,
        [alias(0, vec![], vec![])].into()
    );
    assert_eq!(
        summaries[1].returned.aliases,
        [alias(0, vec![], vec![Projection::Field(0)])].into()
    );
    assert_eq!(
        summaries[2].returned.aliases,
        [alias(0, vec![Projection::Field(0)], vec![])].into()
    );
    assert_eq!(summaries[2].parameters[0].read, Effect::May);
}
#[test]
fn enum_payload_paths_are_composed_and_type_checked() {
    let projection = Projection::VariantField {
        variant: 1,
        field: 0,
    };
    let summaries = analyze_checked(vec![
        function(
            &["Int"],
            "Int",
            Some(vec![
                call(1, vec![param(0, "Int")], local(0, "Maybe")),
                call(2, vec![local(0, "Maybe")], local(1, "Int")),
                I::Return {
                    value: Some(local(1, "Int")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Int"],
            "Maybe",
            Some(vec![
                I::Object {
                    result: local(0, "Maybe"),
                    expression: Rvalue::Variant {
                        index: 1,
                        fields: vec![param(0, "Int")],
                    },
                },
                I::Return {
                    value: Some(local(0, "Maybe")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Maybe"],
            "Int",
            Some(vec![
                I::Object {
                    result: local(0, "Int"),
                    expression: Rvalue::VariantField {
                        owner: param(0, "Maybe"),
                        variant: 1,
                        index: 0,
                    },
                },
                I::Return {
                    value: Some(local(0, "Int")),
                    span: None,
                },
            ]),
        ),
    ]);
    assert_eq!(
        summaries[0].returned.aliases,
        [alias(0, vec![], vec![])].into()
    );
    assert_eq!(
        summaries[1].returned.aliases,
        [alias(0, vec![], vec![projection.clone()])].into()
    );
    assert_eq!(
        summaries[2].returned.aliases,
        [alias(0, vec![projection], vec![])].into()
    );
}
#[test]
fn branch_returns_and_recursive_calls_reach_a_fixed_point() {
    let summaries = analyze_checked(vec![
        function(
            &["Bool", "Int", "Int"],
            "Int",
            Some(vec![I::If {
                condition: param(0, "Bool"),
                yes: Body {
                    instructions: vec![I::Return {
                        value: Some(param(1, "Int")),
                        span: None,
                    }],
                },
                no: Body {
                    instructions: vec![I::Return {
                        value: Some(param(2, "Int")),
                        span: None,
                    }],
                },
            }]),
        ),
        function(
            &["Bool"],
            "Bool",
            Some(vec![I::If {
                condition: param(0, "Bool"),
                yes: Body {
                    instructions: vec![I::Return {
                        value: Some(param(0, "Bool")),
                        span: None,
                    }],
                },
                no: Body {
                    instructions: vec![
                        call(2, vec![param(0, "Bool")], local(0, "Bool")),
                        I::Return {
                            value: Some(local(0, "Bool")),
                            span: None,
                        },
                    ],
                },
            }]),
        ),
        function(
            &["Bool"],
            "Bool",
            Some(vec![
                call(1, vec![param(0, "Bool")], local(0, "Bool")),
                I::Return {
                    value: Some(local(0, "Bool")),
                    span: None,
                },
            ]),
        ),
    ]);
    assert_eq!(
        summaries[0].returned.aliases,
        [alias(1, vec![], vec![]), alias(2, vec![], vec![])].into()
    );
    assert_eq!(summaries[0].may_diverge, Effect::No);
    for s in &summaries[1..] {
        assert_eq!(s.returned, ReturnOrigins::parameter(0));
        assert_eq!(s.parameters[0].write, Effect::No);
        assert_eq!(s.may_diverge, Effect::May);
    }
}
#[test]
fn mutations_and_retention_propagate_through_calls() {
    let summaries = analyze_checked(vec![
        function(
            &["Bool"],
            "Unit",
            Some(vec![
                call(1, vec![param(0, "Bool")], local(0, "Unit")),
                I::Return {
                    value: None,
                    span: None,
                },
            ]),
        ),
        function(
            &["Bool"],
            "Unit",
            Some(vec![
                I::Literal {
                    result: local(0, "Bool"),
                    value: Literal::Bool(true),
                },
                I::StoreBool {
                    target: param(0, "Bool"),
                    value: local(0, "Bool"),
                },
                I::Return {
                    value: None,
                    span: None,
                },
            ]),
        ),
        function(
            &["Box", "Int"],
            "Unit",
            Some(vec![
                I::Rebind {
                    span: None,
                    target: Place::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                    value: param(1, "Int"),
                },
                I::Return {
                    value: None,
                    span: None,
                },
            ]),
        ),
    ]);
    assert_eq!(summaries[0].parameters[0].write, Effect::May);
    assert_eq!(summaries[0].parameters[0].escape, Effect::No);
    assert_eq!(summaries[2].parameters[0].write, Effect::May);
    assert_eq!(summaries[2].parameters[1].escape, Effect::May);
    assert_eq!(summaries[2].parameters[1].retained, Effect::May);
}
#[test]
fn opaque_names_are_not_trusted_and_unknown_global_writes_invalidate_fields() {
    let mut opaque = function(&[], "Unit", None);
    opaque.name = "copy".into();
    let summaries = analyze_checked(vec![
        function(
            &["Box"],
            "Int",
            Some(vec![
                call(1, vec![], local(0, "Unit")),
                I::Object {
                    result: local(1, "Int"),
                    expression: Rvalue::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                },
                I::Return {
                    value: Some(local(1, "Int")),
                    span: None,
                },
            ]),
        ),
        opaque,
    ]);
    assert_eq!(summaries[1].source, SummarySource::Opaque);
    assert_eq!(summaries[0].external_write, Effect::Unknown);
    assert!(summaries[0].returned.unknown);
    assert_eq!(summaries[0].parameters[0].escape, Effect::Unknown);
}
#[test]
fn overwritten_heap_fields_are_not_reported_as_old_aliases() {
    let summaries = analyze_checked(vec![function(
        &["Box", "Int"],
        "Int",
        Some(vec![
            I::Rebind {
                span: None,
                target: Place::Field {
                    owner: param(0, "Box"),
                    index: 0,
                },
                value: param(1, "Int"),
            },
            I::Object {
                result: local(0, "Int"),
                expression: Rvalue::Field {
                    owner: param(0, "Box"),
                    index: 0,
                },
            },
            I::Return {
                value: Some(local(0, "Int")),
                span: None,
            },
        ]),
    )]);
    assert!(summaries[0].returned.unknown);
    assert_eq!(summaries[0].parameters[1].escape, Effect::Unknown);
}
#[test]
fn callee_projections_respect_heap_changes_but_old_snapshots_survive() {
    let summaries = analyze_checked(vec![
        function(
            &["Box", "Int"],
            "Int",
            Some(vec![
                I::Rebind {
                    span: None,
                    target: Place::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                    value: param(1, "Int"),
                },
                call(2, vec![param(0, "Box")], local(0, "Int")),
                I::Return {
                    value: Some(local(0, "Int")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Box", "Int"],
            "Int",
            Some(vec![
                I::Object {
                    result: local(0, "Int"),
                    expression: Rvalue::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                },
                I::Rebind {
                    span: None,
                    target: Place::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                    value: param(1, "Int"),
                },
                I::Return {
                    value: Some(local(0, "Int")),
                    span: None,
                },
            ]),
        ),
        function(
            &["Box"],
            "Int",
            Some(vec![
                I::Object {
                    result: local(0, "Int"),
                    expression: Rvalue::Field {
                        owner: param(0, "Box"),
                        index: 0,
                    },
                },
                I::Return {
                    value: Some(local(0, "Int")),
                    span: None,
                },
            ]),
        ),
    ]);
    assert!(summaries[0].returned.unknown);
    assert_eq!(
        summaries[1].returned.aliases,
        [alias(0, vec![Projection::Field(0)], vec![])].into()
    );
}
#[test]
fn loop_join_keeps_both_initial_and_rebound_origins() {
    let summaries = analyze_checked(vec![function(
        &["Bool", "Int", "Int"],
        "Int",
        Some(vec![
            I::Declare {
                result: local(0, "Int"),
                initial: Some(param(1, "Int")),
            },
            I::Loop {
                id: 0,
                body: Body {
                    instructions: vec![
                        I::If {
                            condition: param(0, "Bool"),
                            yes: Body {
                                instructions: vec![I::Break(0)],
                            },
                            no: Body::default(),
                        },
                        I::Rebind {
                            span: None,
                            target: Place::Local(local(0, "Int").id),
                            value: param(2, "Int"),
                        },
                        I::Continue(0),
                    ],
                },
            },
            I::Return {
                value: Some(local(0, "Int")),
                span: None,
            },
        ]),
    )]);
    assert_eq!(
        summaries[0].returned.aliases,
        [alias(1, vec![], vec![]), alias(2, vec![], vec![])].into()
    );
    assert_eq!(summaries[0].may_diverge, Effect::May);
}
#[test]
fn provenance_widens_and_invalid_summaries_are_rejected() {
    let mut value = ReturnOrigins::parameter(0);
    for _ in 0..=MAX_PATH {
        value = value.project(Projection::Field(0));
    }
    assert_eq!(value, ReturnOrigins::unknown());
    value.join(&ReturnOrigins::fresh());
    assert_eq!(value, ReturnOrigins::unknown());
    let mut summary = Summary::inferred(1);
    summary.returned = ReturnOrigins::parameter(0);
    assert!(summary.validate(1).unwrap_err().contains("return escape"));
    summary.parameters[0].escape = Effect::May;
    summary.validate(1).unwrap();
    assert!(summary.validate(0).is_err());
    summary.returned.aliases = [alias(2, vec![], vec![])].into();
    assert!(
        summary
            .validate(1)
            .unwrap_err()
            .contains("return parameter")
    );
    let types = [(
        "Int".into(),
        crate::ir::hir::Type::Scalar(crate::ir::hir::Scalar::I64),
    )]
    .into();
    let mut invalid_path = Summary::inferred(1);
    invalid_path.parameters[0].escape = Effect::May;
    invalid_path.returned.aliases = [alias(0, vec![Projection::Field(0)], vec![])].into();
    assert!(
        invalid_path
            .validate_signature(&["Int".into()], "Int", &types, true)
            .unwrap_err()
            .contains("projection")
    );
    assert!(
        Summary::inferred(0)
            .validate_signature(&[], "Int", &types, false)
            .unwrap_err()
            .contains("opaque body")
    );
    let mut opaque = Summary::unknown(1, SummarySource::Opaque);
    opaque.parameters[0].write = Effect::No;
    assert!(opaque.validate(1).unwrap_err().contains("must be unknown"));
}
