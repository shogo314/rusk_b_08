use super::*;
use crate::ir::hir::Instruction;
use std::collections::BTreeSet;

#[test]
fn a_failed_child_check_restores_the_parent_scope_and_return_state() {
    let mut compiler = Compiler {
        context: FunctionContext::function(7, "Str".into()),
        ..Compiler::default()
    };
    compiler.context.diverged = true;
    compiler.context.pending_temps.push(ValueId::Local(90));
    compiler.context.body.push(Instruction::Unreachable);
    let (result, child) =
        compiler.with_function_context(FunctionContext::function(8, "Int".into()), |compiler| {
            compiler.context.inferred_return = Some("Bool".into());
            compiler.context.scopes.push(BTreeMap::new());
            compiler.context.pending_temps.push(ValueId::Local(91));
            Err::<(), _>("child error")
        });
    assert_eq!(result, Err("child error"));
    assert_eq!(compiler.context.current_function, Some(7));
    assert_eq!(compiler.context.return_type.as_deref(), Some("Str"));
    assert!(compiler.context.diverged);
    assert_eq!(compiler.context.scopes.len(), 1);
    assert_eq!(compiler.context.pending_temps, [ValueId::Local(90)]);
    assert!(matches!(
        compiler.context.body.instructions.as_slice(),
        [Instruction::Unreachable]
    ));
    assert_eq!(child.inferred_return.as_deref(), Some("Bool"));
    assert_eq!(child.pending_temps, [ValueId::Local(91)]);
}

#[test]
fn nested_checks_preserve_global_registrations_and_unique_value_ids() {
    let mut compiler = Compiler::default();
    let outer = compiler.temp();
    let ((first, second), _) =
        compiler.with_function_context(FunctionContext::function(3, "Unit".into()), |compiler| {
            let first = compiler.temp();
            let lambda = FunctionContext::lambda(compiler.context.current_function);
            let (second, _) = compiler.with_function_context(lambda, |compiler| {
                compiler.types.insert("Registered".into(), "Native".into());
                compiler.temp()
            });
            assert_eq!(compiler.context.current_function, Some(3));
            assert!(!compiler.context.infer_return);
            (first, second)
        });
    let after = compiler.temp();
    assert_eq!(
        compiler.types.get("Registered").map(String::as_str),
        Some("Native")
    );
    assert_eq!(BTreeSet::from([outer, first, second, after]).len(), 4);
    assert_eq!(compiler.context.current_function, None);
}
