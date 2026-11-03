//! The Unified sidebar: one list of every session on every machine, Space and project, with the
//! sessions that need you on top and Working, Snoozed and Parked folded into shelves below.
//!
//! It is built from the per-machine lists the status indicators already keep with no Space,
//! filter, collapse or hide applied (`indicators::neutral_indicator_inputs`), so the Project
//! sidebar's own list model is untouched by it.

mod build;
mod choices;
#[cfg(test)]
mod fixture_tests;
mod grouping;
mod lanes;
mod repo;
mod saved;
#[cfg(test)]
mod scale_tests;
mod spaces;
#[cfg(test)]
mod tests;

pub use build::{
    build_unified_sidebar, came_back_ms, is_unified_draft, repo_of_project, space_filter_key,
    unified_shelf, unified_status, UnifiedFilter, UnifiedFilterOption, UnifiedFilterOptions,
    UnifiedMachineInput, UnifiedRow, UnifiedSettings, UnifiedShelf, UnifiedSidebarView,
    UnifiedStatus,
};
pub use choices::{
    default_machine, filter_project_choices, move_project_choice_machine,
    unified_new_session_target, unified_project_choices, widen_filter_for_new_session,
    ActiveProject, ProjectChoiceInput, UnifiedChoiceMachine, UnifiedChoiceMachineInput,
    UnifiedNewSessionTarget, UnifiedProjectChoice,
};
pub use grouping::{group_unified_sidebar, move_unified_project, ordered_repo_keys, UnifiedGroup};
pub use lanes::{unified_lane_inputs, unified_lane_menu_inputs};
pub use repo::{normalize_git_remote, repo_identity, RepoIdentity};
pub use saved::{UnifiedSavedState, RECENT_REPOS_MAX, UNIFIED_SAVED_STATE_KEY};
pub use spaces::unified_machine_spaces;
