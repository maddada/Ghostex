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
mod machines;
pub(crate) mod repo;
mod reveal;
mod saved;
#[cfg(test)]
mod scale_tests;
mod spaces;
#[cfg(test)]
mod tests;
mod tree;

pub use build::{
    build_unified_sidebar, build_unified_sidebar_folded, build_unified_sidebar_scoped, came_back_ms, is_unified_draft,
    repo_of_project, space_filter_key, unified_shelf, unified_space_of_project, unified_status,
    UnifiedFilter, UnifiedFilterOption, UnifiedFilterOptions, UnifiedMachineInput, UnifiedRow,
    UnifiedSettings, UnifiedShelf, UnifiedSidebarView, UnifiedStatus, UnifiedTreeLine,
};
pub use tree::{unified_coordinator_key, UnifiedCrew, UnifiedFolds};
pub use choices::{
    default_machine, filter_project_choices, move_project_choice_machine, project_query_match,
    session_activity_ms, unified_new_session_target, unified_project_choices,
    widen_filter_for_new_session, ActiveProject, ProjectChoiceInput, UnifiedChoiceMachine,
    UnifiedChoiceMachineInput, UnifiedNewSessionTarget, UnifiedProjectChoice,
};
pub use grouping::{group_unified_sidebar, move_unified_project, ordered_repo_keys, UnifiedGroup};
pub use lanes::{unified_lane_inputs, unified_lane_menu_inputs};
pub use machines::{unified_branch_line, unified_shows_machine, UnifiedBranchLine};
pub use repo::{
    folder_in_repository, merge_repo_identities, normalize_git_remote, origin_keys_of,
    project_repo_key, repo_identity, repository_of_project_key, resolve_repo_identity,
    RepoIdentity,
};
pub use reveal::{
    find_unified_row, open_unified_reveal, UnifiedFound, unified_shelf_key, unified_shelf_label,
    unified_shelf_open, UnifiedRevealSpot,
};
pub use saved::{UnifiedSavedState, RECENT_REPOS_MAX, UNIFIED_SAVED_STATE_KEY};
pub use spaces::{unified_machine_spaces, unified_space_scope, UnifiedSpaceScope};
