use super::*;
fn int(id: usize) -> Value {
    Value {
        id: ValueId::Local(id),
        ty: "Int".into(),
    }
}
#[test]
fn an_outer_loop_exit_cleans_inner_scopes_but_preserves_the_returned_owner() {
    let body = Body {
        instructions: vec![
            Instruction::Literal {
                result: int(1),
                value: hir::Literal::Int(42),
            },
            Instruction::Loop {
                id: 10,
                body: Body {
                    instructions: vec![
                        Instruction::Literal {
                            result: int(2),
                            value: hir::Literal::Int(2),
                        },
                        Instruction::Loop {
                            id: 20,
                            body: Body {
                                instructions: vec![
                                    Instruction::Literal {
                                        result: int(3),
                                        value: hir::Literal::Int(3),
                                    },
                                    Instruction::Break(10),
                                ],
                            },
                        },
                    ],
                },
            },
            Instruction::Return {
                value: Some(int(1)),
                span: None,
            },
        ],
    };
    let function = lower(&body);
    let end = BlockId(
        function
            .blocks
            .iter()
            .position(|b| matches!(b.terminator, Terminator::Return { .. }))
            .unwrap(),
    );
    let exit = function
        .blocks
        .iter()
        .find(|b| matches!(b.terminator,Terminator::Jump(id) if id==end))
        .unwrap();
    let Operation::Release(ids) = exit.instructions.last().unwrap() else {
        panic!("exit must release its scopes")
    };
    assert_eq!(ids, &vec![ValueId::Local(3), ValueId::Local(2)]);
    let Terminator::Return { cleanup, .. } = &function.blocks[end.0].terminator else {
        unreachable!()
    };
    assert_eq!(cleanup, &vec![ValueId::Local(1)]);
}
#[test]
fn corrupt_jump_targets_are_rejected() {
    let mut program = Program {
        types: BTreeMap::new(),
        functions: vec![],
        main: Function {
            slots: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Jump(BlockId(9)),
            }],
        },
    };
    assert!(program.validate().unwrap_err().contains("jump target"));
    program.main.blocks[0].terminator = Terminator::Return {
        span: None,
        value: None,
        cleanup: vec![],
    };
    assert!(program.validate().is_ok());
}
#[test]
fn escape_metadata_requires_unique_parameter_identities_and_preserves_return_locations() {
    let span = crate::frontend::ast::Span {
        file: "test".into(),
        line: 4,
        column: 2,
    };
    let info = hir::EscapeInfo {
        name: "x".into(),
        declaration: span.clone(),
        annotation: crate::frontend::ast::EscapeAnnotation {
            mode: crate::frontend::ast::EscapeMode::Noescape,
            span: span.clone(),
        },
    };
    let body = Body {
        instructions: vec![
            Instruction::AnnotateEscape {
                parameter: 0,
                info: info.clone(),
            },
            Instruction::Return {
                value: None,
                span: Some(span.clone()),
            },
        ],
    };
    let function = lower(&body);
    assert!(function.slots.is_empty());
    let Terminator::Return {
        span: Some(location),
        ..
    } = &function.blocks[0].terminator
    else {
        panic!("return location was lost")
    };
    assert_eq!(location, &span);
    let types = BTreeMap::from([
        ("Int".into(), hir::Type::Scalar(crate::ir::hir::Scalar::I64)),
        (
            "Unit".into(),
            hir::Type::Scalar(crate::ir::hir::Scalar::Unit),
        ),
    ]);
    let mut program = Program {
        types,
        functions: vec![],
        main: function,
    };
    assert!(
        program
            .validate_body(&program.main, &["Int".into()], "Unit", true)
            .is_ok()
    );
    assert!(
        program
            .validate()
            .unwrap_err()
            .contains("escape annotation")
    );
    program.main.blocks[0]
        .instructions
        .push(Operation::AnnotateEscape { parameter: 0, info });
    assert!(
        program
            .validate_body(&program.main, &["Int".into()], "Unit", true)
            .unwrap_err()
            .contains("duplicate")
    );
}
