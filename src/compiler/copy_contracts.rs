//! Common metadata for automatically derived aggregate copies.
use super::*;
impl Compiler {
    pub(super) fn register_derived_copy(&mut self, index: usize) {
        self.native_effect_references
            .insert((index, crate::effects::Query::Writes));
        self.native_lowerings.insert(index,crate::native_lowering::Lowering {
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
