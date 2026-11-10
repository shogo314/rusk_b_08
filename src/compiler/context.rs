//! State belonging to a single function body or lambda check.
use super::Compiler;
use crate::frontend::ast::Name;
use crate::ir::hir::{Body, Value, ValueId};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Variable {
    pub(super) id: ValueId,
    pub(super) ty: String,
    pub(super) initialized: bool,
    pub(super) binding: Option<crate::ir::hir::BindingInfo>,
    pub(super) access: Option<crate::ir::hir::AccessInfo>,
}
#[derive(Clone)]
pub(super) struct LoopContext {
    pub(super) id: usize,
    pub(super) label: Option<Name>,
    pub(super) broken: Option<Value>,
    pub(super) break_seen: bool,
    pub(super) break_states: Vec<Vec<BTreeMap<String, Variable>>>,
}

#[derive(Default)]
pub(super) struct FunctionContext {
    pub(super) current_function: Option<usize>,
    pub(super) scopes: Vec<BTreeMap<String, Variable>>,
    pub(super) body: Body,
    pub(super) loops: Vec<LoopContext>,
    pub(super) diverged: bool,
    pub(super) infer_return: bool,
    pub(super) inferred_return: Option<String>,
    pub(super) return_type: Option<String>,
    pub(super) pending_temps: Vec<ValueId>,
}
impl FunctionContext {
    pub(super) fn function(index: usize, result: String) -> Self {
        Self {
            current_function: Some(index),
            scopes: vec![BTreeMap::new()],
            return_type: Some(result),
            ..Self::default()
        }
    }
    pub(super) fn lambda(current_function: Option<usize>) -> Self {
        Self {
            current_function,
            scopes: vec![BTreeMap::new()],
            return_type: Some("Unit".into()),
            infer_return: true,
            ..Self::default()
        }
    }
}
impl Compiler {
    /// Restore the parent even when the check returns an error. Registries and
    /// globally unique value/loop IDs deliberately remain outside the context.
    pub(super) fn with_function_context<T>(
        &mut self,
        context: FunctionContext,
        check: impl FnOnce(&mut Self) -> T,
    ) -> (T, FunctionContext) {
        let parent = std::mem::replace(&mut self.context, context);
        let result = check(self);
        let completed = std::mem::replace(&mut self.context, parent);
        (result, completed)
    }
}
#[cfg(test)]
mod tests;
