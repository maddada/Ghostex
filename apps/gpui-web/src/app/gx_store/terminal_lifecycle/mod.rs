//! The desktop's `terminal_lifecycle/` files the page compiles: a session's rename, note and Delayed Send edits, and the single calls a session's own controls make, Close After Done and a user-made group's Sleep and Wake. The rest of that folder answers the desktop's pane workspace (tab close, sleep and wake from a tab, terminal events), which the page does not have.
mod close_after_done;
mod group_sleep;
pub(crate) mod session_calls;
mod session_edits;
pub(crate) mod shown_sessions;
mod shown_sessions_report;
