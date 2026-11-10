//! Per-function native semantics, separate from whole-program output requests.
use super::*;
use crate::analysis::effects::Summary;
use crate::native::{abi::UnitBridge, effects::composition::Dependency, lowering::Lowering};

#[derive(Default)]
pub(super) struct NativeFunctionInfo {
    pub(super) contract: Option<Summary>,
    // None means there was no dependency registration. Some(empty) still marks
    // a conditional contract whose mapped calls must be proven before use.
    pub(super) dependencies: Option<BTreeSet<usize>>,
    pub(super) mapped_dependencies: Vec<Dependency>,
    pub(super) lowering: Option<Lowering>,
    pub(super) enum_operation: Option<enum_operations::Operation>,
    pub(super) unit_object: bool,
}
#[derive(Default)]
pub(super) struct NativeRequests {
    pub(super) declarations: BTreeSet<usize>,
    pub(super) values: BTreeSet<usize>,
    pub(super) owned: BTreeSet<usize>,
    pub(super) rooted: BTreeSet<usize>,
    pub(super) effects: BTreeSet<(usize, crate::analysis::effects::Query)>,
    pub(super) unit_bridges: BTreeSet<(usize, UnitBridge)>,
}
impl Compiler {
    pub(super) fn native_info_mut(&mut self, index: usize) -> &mut NativeFunctionInfo {
        self.native_info.entry(index).or_default()
    }
    pub(super) fn native_contract(&self, index: usize) -> Option<&Summary> {
        self.native_info.get(&index)?.contract.as_ref()
    }
    pub(super) fn native_dependencies(&self, index: usize) -> Option<&BTreeSet<usize>> {
        self.native_info.get(&index)?.dependencies.as_ref()
    }
    pub(super) fn native_mappings(&self, index: usize) -> &[Dependency] {
        self.native_info
            .get(&index)
            .map_or(&[], |info| info.mapped_dependencies.as_slice())
    }
    pub(super) fn native_lowering(&self, index: usize) -> Option<&Lowering> {
        self.native_info.get(&index)?.lowering.as_ref()
    }
    pub(super) fn native_unit_object(&self, index: usize) -> bool {
        self.native_info
            .get(&index)
            .is_some_and(|info| info.unit_object)
    }
}
