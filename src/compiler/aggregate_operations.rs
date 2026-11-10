//! Source templates and metadata for automatically derived aggregate operations.
use super::*;
impl Compiler {
    pub(super) fn native_operation_needed(&self, index: usize) -> bool {
        self.used.contains(&index)
            || self
                .native_effect_references
                .iter()
                .any(|(id, _)| *id == index)
    }

    pub(super) fn render_derived_copy(
        &mut self,
        kind: &str,
        ty: &str,
        index: usize,
        span: &Span,
    ) -> Result<String, String> {
        self.native_effect_references
            .insert((index, crate::effects::Query::Writes));
        let code = self.render_native_derivation(kind, ty, index, "deep_copy", span)?;
        let dependencies = self
            .native_contract_dependencies
            .get(&index)
            .cloned()
            .unwrap_or_default();
        self.set_derived_copy_dependencies(index, ty, dependencies)?;
        Ok(code)
    }

    pub(super) fn prepare_derived_functions(&mut self) -> Result<(), String> {
        let tasks = self
            .native_derivation_tasks
            .iter()
            .filter(|(id, _)| {
                self.native_operation_needed(**id) && !self.native_derived_functions.contains(id)
            })
            .map(|(&id, (kind, ty))| (id, kind.clone(), ty.clone()))
            .collect::<Vec<_>>();
        for (index, kind, ty) in tasks {
            let function = self.functions[index].clone();
            self.functions[index].code = self.render_native_derivation(
                &kind,
                &ty,
                index,
                &function.cpp,
                &function.name.span,
            )?;
        }
        Ok(())
    }

    fn render_native_derivation(
        &mut self,
        kind: &str,
        ty: &str,
        index: usize,
        cpp: &str,
        span: &Span,
    ) -> Result<String, String> {
        let definition = self
            .native_derivations
            .get(kind)
            .cloned()
            .ok_or_else(|| span.error(format!("missing standard {kind} derivation")))?;
        self.functions[index].origin = definition.origin;
        self.native_derived_functions.insert(index);
        let pattern = if !definition.arguments.is_empty() {
            &definition.arguments[0].1.text
        } else {
            &definition.result.as_ref().unwrap().text
        };
        let mut bindings = BTreeMap::new();
        if !generics::unify(pattern, ty, &definition.parameters, &mut bindings) {
            return Err(span.error(format!("standard {kind} derivation does not match '{ty}'")));
        }
        let code = self
            .render_native(
                &definition.foreign.as_ref().unwrap()["code"],
                &bindings,
                Some(cpp),
                // Derived children inherit the original request site. Otherwise a
                // deferred child reader loses the user's location at each level.
                span,
                true,
                Some(index),
            )
            .map_err(|error| {
                format!(
                    "{error}\nwhile expanding standard {kind} declared at {}:{}:{}",
                    definition.name.span.file,
                    definition.name.span.line,
                    definition.name.span.column,
                )
            })?;
        let mut properties = definition.foreign.as_ref().unwrap().clone();
        let params = self.functions[index].params.clone();
        let result = self.functions[index].result.clone();
        if let Some(contract) = self.declared_native_effects(
            &definition.name,
            &params,
            &result,
            &properties,
            definition.origin,
        )? {
            self.native_contracts.insert(index, contract);
        }
        for key in ["lower", "lower_owned", "lower_borrowed", "lower_rooted"] {
            if let Some(expression) = properties.get(key).cloned() {
                let expression =
                    self.render_native(&expression, &bindings, Some(cpp), span, true, Some(index))?;
                properties.insert(key.into(), expression);
            }
        }
        self.register_native_lowering(
            index,
            &definition.name,
            &params,
            &result,
            &properties,
            definition.origin,
        )?;
        Ok(code)
    }

    pub(super) fn register_derived_copy(&mut self, index: usize) {
        self.native_lowerings.insert(index,crate::native_lowering::Lowering {
            rooted: Some(crate::native_lowering::Template::parse("deep_copy($0,$loc)",1).expect("rooted aggregate copy expression")),
            owned: Some(crate::native_lowering::Template::parse(&format!("([&](){{if constexpr(rusk_writes_{index}){{auto owner=$0;return deep_copy(owner,$loc);}}else return deep_copy($0,$loc);}}())"),1).expect("derived aggregate copy expression")),
            ..Default::default()
        });
    }
    pub(super) fn set_derived_copy_dependencies(
        &mut self,
        index: usize,
        ty: &str,
        dependencies: impl IntoIterator<Item = usize>,
    ) -> Result<(), String> {
        self.native_contracts.insert(
            index,
            crate::native_effects::contract(
                "checked_read_fresh",
                &[ty.into()],
                ty,
                &self.ir_types(),
            )?,
        );
        self.native_contract_dependencies
            .insert(index, dependencies.into_iter().collect());
        Ok(())
    }
}
