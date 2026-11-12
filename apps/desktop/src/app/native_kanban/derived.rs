//! What the board draws from its tickets, computed once per change instead of once per frame.

/// Cached per `NativeKanbanState::revision`; see `NativeKanbanState::derived`.
pub(crate) struct KanbanDerived {
    pub(crate) revision: u64,
    /// The lanes drawn, as indices into `columns` (a hidden lane is left out while it is empty).
    pub(crate) shown: Vec<usize>,
    /// Per shown lane, in order: indices into `tickets`, filtered and sorted.
    pub(crate) lanes: Vec<Vec<usize>>,
    pub(crate) filter_count: usize,
    pub(crate) nothing_matches: bool,
}
