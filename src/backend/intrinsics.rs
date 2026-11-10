//! Declaration-driven native expression lowering.
use super::FunctionInfo;
use crate::ir::hir::Call;

fn render(
    call: &Call,
    info: &FunctionInfo,
    select: impl FnOnce(
        &crate::native::lowering::Lowering,
    ) -> Option<&crate::native::lowering::Template>,
) -> Option<String> {
    if !info.trusted {
        return None;
    }
    let template = select(info.lowering.as_ref()?)?;
    let args = call
        .args
        .iter()
        .map(|a| a.id.to_string())
        .collect::<Vec<_>>();
    Some(template.render(&args, &super::cpp::call_location(call)))
}

pub(super) fn lower(call: &Call, info: &FunctionInfo) -> Option<String> {
    render(call, info, |l| l.scalar.as_ref())
}

// Ownership and borrowed-slot selection still require the existing lifetime proof.
pub(super) fn lower_owned(call: &Call, info: &FunctionInfo) -> Option<String> {
    render(call, info, |l| l.owned.as_ref())
}
pub(super) fn lower_borrowed(call: &Call, info: &FunctionInfo) -> Option<String> {
    render(call, info, |l| l.borrowed.as_ref())
}
