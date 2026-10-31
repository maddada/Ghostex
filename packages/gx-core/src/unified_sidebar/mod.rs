//! The Unified sidebar: one list of every session on every machine, Space and project, with the
//! sessions that need you on top and Working, Snoozed and Parked folded into shelves below.
//!
//! It is built from the per-machine lists the status indicators already keep with no Space,
//! filter, collapse or hide applied (`indicators::neutral_indicator_inputs`), so the Project
//! sidebar's own list model is untouched by it.

mod build;
#[cfg(test)]
mod fixture_tests;
mod repo;
mod spaces;
#[cfg(test)]
mod tests;

pub use build::{
    build_unified_sidebar, came_back_ms, is_unified_draft, repo_of_project, space_filter_key,
    unified_shelf, unified_status, UnifiedFilter, UnifiedFilterOption, UnifiedFilterOptions,
    UnifiedMachineInput, UnifiedRow, UnifiedSettings, UnifiedShelf, UnifiedSidebarView,
    UnifiedStatus,
};
pub use repo::{normalize_git_remote, repo_identity, RepoIdentity};
pub use spaces::unified_machine_spaces;
