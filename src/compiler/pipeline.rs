//! Body completion, intermediate representations, and analysis phases.
use super::*;

impl Compiler {
    pub(super) fn prepare_bodies(&mut self, sources: &[Source]) -> Result<(), String> {
        // Check every body, including unused functions, in an isolated variable scope.
        for i in 0..self.functions.len() {
            if self.functions[i].body.is_some() {
                self.compile_function(i)?;
            }
        }
        self.context.scopes.push(BTreeMap::new());
        for source in sources {
            for s in source {
                self.statement(s)?;
            }
        }
        let main_body = std::mem::take(&mut self.context.body);
        self.context.diverged = false;
        self.complete_instantiations()?;
        self.context.body = main_body;
        self.context.body.push(Instruction::Return {
            value: None,
            span: None,
        });
        Ok(())
    }
    fn complete_instantiations(&mut self) -> Result<(), String> {
        let mut index = 0;
        loop {
            while index < self.functions.len() {
                if self.functions[index].body.is_some()
                    && self.functions[index].checked.is_none()
                    && let Err(error) = self.compile_function(index)
                {
                    if let Some(span) = self.generic_calls.get(&index) {
                        return Err(format!(
                            "{error}\nwhile instantiating '{}' at {}:{}:{}",
                            self.generic_instances
                                .iter()
                                .find(|(_, value)| **value == index)
                                .map(|((template, arguments), _)| format!(
                                    "{}<{}>",
                                    self.generic_functions[*template].name.text,
                                    arguments.join(",")
                                ))
                                .unwrap_or_else(|| self.functions[index].name.text.clone()),
                            span.file,
                            span.line,
                            span.column
                        ));
                    }
                    return Err(error);
                }
                index += 1;
            }
            self.prepare_record_copies()?;
            self.prepare_enum_copies()?;
            self.prepare_derived_functions()?;
            self.prepare_native_types()?;
            if index == self.functions.len()
                && self
                    .native_type_bindings
                    .values()
                    .all(|binding| binding.native.is_some())
                && self.record_copy_functions.iter().all(|(ty, id)| {
                    !self.native_operation_needed(*id) || self.record_copies.contains_key(ty)
                })
                && self.enum_copy_functions.iter().all(|(ty, id)| {
                    !self.native_operation_needed(*id) || self.enum_copies.contains_key(ty)
                })
                && self.native_derivation_tasks.keys().all(|id| {
                    !self.native_operation_needed(*id) || self.native_derived_functions.contains(id)
                })
            {
                break;
            }
        }
        Ok(())
    }
    pub(super) fn build_hir(&mut self) -> crate::ir::hir::Program {
        let mut hir = crate::ir::hir::Program {
            types: self.ir_types(),
            functions: self
                .functions
                .iter()
                .enumerate()
                .map(|(id, f)| crate::ir::hir::Function {
                    id,
                    name: f.name.text.clone(),
                    params: f.params.clone(),
                    result: f.result.clone(),
                    body: f.checked.clone(),
                    access: crate::analysis::readonly::Summary::default(),
                    effects: self.native_contracts.get(&id).cloned().unwrap_or_else(|| {
                        crate::analysis::effects::Summary::unanalyzed(f.params.len())
                    }),
                })
                .collect(),
            main: std::mem::take(&mut self.context.body),
        };
        hir.prune_unreachable();
        hir
    }
    pub(super) fn analyze_program(
        &self,
        hir: &mut crate::ir::hir::Program,
    ) -> Result<crate::ir::lir::Program, String> {
        let conditional = self
            .native_contract_dependencies
            .keys()
            .copied()
            .filter(|id| {
                self.native_contracts.get(id).is_some_and(|s| {
                    s.source == crate::analysis::effects::SummarySource::NativeContract
                })
            })
            .collect::<BTreeSet<_>>();
        for &id in &conditional {
            hir.functions[id].effects =
                crate::analysis::effects::Summary::unanalyzed(hir.functions[id].params.len());
        }
        hir.validate()?;
        let mut lir = crate::ir::lir::lower_program(hir);
        lir.validate()?;
        self.resolve_effects(hir, &mut lir, conditional)?;
        hir.validate()?;
        lir.validate()?;
        crate::analysis::effects::validate_noescape(&lir)?;
        let access = crate::analysis::readonly::analyze(&lir)?;
        for (id, summary) in access.into_iter().enumerate() {
            hir.functions[id].access = summary.clone();
            lir.functions[id].access = summary;
        }
        hir.validate()?;
        lir.validate()?;
        Ok(lir)
    }
    fn resolve_effects(
        &self,
        hir: &mut crate::ir::hir::Program,
        lir: &mut crate::ir::lir::Program,
        mut conditional: BTreeSet<usize>,
    ) -> Result<(), String> {
        let mut summaries = crate::analysis::effects::analyze(lir);
        loop {
            let proven = conditional
                .iter()
                .copied()
                .filter_map(|id| {
                    let base = &self.native_contracts[&id];
                    if !self.native_contract_dependencies[&id]
                        .iter()
                        .all(|dependency| {
                            crate::native::effects::dependency_covered(
                                base,
                                &summaries[*dependency],
                            )
                        })
                    {
                        return None;
                    }
                    let composed = crate::native::effects::composition::compose(
                        base,
                        self.native_mapped_dependencies
                            .get(&id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        &summaries,
                    )?;
                    Some((id, composed))
                })
                .collect::<Vec<_>>();
            if proven.is_empty() {
                break;
            }
            for (id, composed) in proven {
                composed.validate_signature(
                    &lir.functions[id].params,
                    &lir.functions[id].result,
                    &lir.types,
                    false,
                )?;
                conditional.remove(&id);
                lir.functions[id].effects = composed;
            }
            summaries = crate::analysis::effects::analyze(lir);
        }
        for (id, summary) in summaries.into_iter().enumerate() {
            hir.functions[id].effects = summary.clone();
            lir.functions[id].effects = summary;
        }
        Ok(())
    }
}
