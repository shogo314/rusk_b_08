//! C++ runtime provider. Rusk HIR/LIR bodies never contain these native sources.
use super::*;
impl Compiler {
    pub(super) fn cpp_context(&self) -> crate::backend::Context {
        let functions = self
            .functions
            .iter()
            .enumerate()
            .map(|(id, f)| crate::backend::FunctionInfo {
                cpp: f.cpp.clone(),
                result: f.result.clone(),
                unit_object: self.native_unit_objects.contains(&id),
                trusted: f.body.is_none()
                    && (f.origin == Origin::StandardLibrary
                        || self.native_lowerings.contains_key(&id)),
                lowering: self.native_lowerings.get(&id).cloned(),
            })
            .collect();
        let mut prelude = String::new();
        for code in self.symbols.values() {
            prelude += code;
            prelude += "\n";
        }
        prelude += &self.enum_forwards();
        prelude += &self.container_cpp();
        prelude += &self.enum_cpp();
        prelude += &self.lazy_cpp();
        prelude += &self.record_cpp();
        prelude += &self.key_equal_cpp();
        prelude += &self.key_hash_cpp();
        prelude += &self.lazy_copy_declarations();
        prelude += &self.scalar_representation_checks();
        let (concrete_foreign_definitions, foreign_definitions): (Vec<_>, Vec<_>) = self
            .used
            .iter()
            .filter(|&&id| self.functions[id].checked.is_none())
            .partition(|&&id| {
                self.functions[id].origin != Origin::Generated
                    && !self.generic_calls.contains_key(&id)
                    && !self.native_derived_functions.contains(&id)
            });
        let definitions = |ids: Vec<&usize>| {
            ids.into_iter()
                .map(|&id| self.functions[id].code.clone())
                .collect()
        };
        let epilogue = self.enum_copy_cpp() + &self.record_copy_cpp() + &self.lazy_copy_cpp();
        crate::backend::Context {
            types: self.types.clone(),
            functions,
            prelude,
            concrete_foreign_definitions: definitions(concrete_foreign_definitions),
            foreign_definitions: definitions(foreign_definitions),
            native_unit_bridges: self.native_unit_bridges.clone(),
            native_value_references: self.native_value_references.clone(),
            native_owned_references: self.native_owned_references.clone(),
            native_rooted_references: self.native_rooted_references.clone(),
            native_effect_references: self.native_effect_references.clone(),
            native_declarations: self
                .native_references
                .iter()
                .copied()
                .chain(
                    self.generic_instances
                        .values()
                        .copied()
                        .filter(|&id| self.functions[id].body.is_none() && self.used.contains(&id)),
                )
                .collect(),
            epilogue,
        }
    }
}
