//! Unit has a void call ABI and an owning object ABI when stored as data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UnitBridge {
    Discard,
    Box,
}
impl UnitBridge {
    pub(crate) fn name(self, id: usize) -> String {
        format!(
            "rusk_unit_{}_{id}",
            match self {
                Self::Discard => "discard",
                Self::Box => "box",
            }
        )
    }
}
