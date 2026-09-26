//! Git: the state a project shows, the menu drawn from it, what each Git action does, the commit
//! review's trusted file list, the poll's schedule and the worktree dialogs' drafts.
//!
//! CDXC:Git 2026-09-25 WHY:
//! The app runtime port (docs/2026-09-25/app-runtime-port/PLAN.md, family F5) moves the Git menu,
//! the Git actions, the diff-stats poll and the worktree dialogs out of the desktop's QuickJS
//! runtime. gxserver reads a project's Git state in one call (`/api/readProjectGitState`) and runs
//! the commit and push writes (`/api/runGitShipWorkflow`); this module holds the rules that are
//! neither I/O nor drawing, so the desktop, the web build and mobile decide the same way. Nothing
//! here reads a clock or performs I/O.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/git/ (the host), server/src/project_git_state.rs,
//! server/src/git_ship_workflow.rs, tooling/gx-core/git-menu-parity.ts (the gate).

mod memo;
mod menu;
mod plan;
mod poll;
mod read;
mod review;
mod state;
mod text;
mod worktree;

pub use memo::{GitTtlMemo, GIT_HUB_MEMO_TTL_MS, GIT_MEMO_MAX_ENTRIES, GIT_STATE_MEMO_TTL_MS};
pub use menu::{
    disabled_reason, has_remote_commit_delta, menu_items, primary_action_state, titlebar_menu,
    GitMenuItem, GitPrimaryActionState, TitlebarGitMenu, TitlebarGitMenuRow,
};
pub use plan::{
    plan_git_action_after_read, plan_git_action_before_read, GitActionStep, GitMutation, GitToast,
    GitToastLevel,
};
pub use poll::{
    plan_git_poll_cycle, GitPollCycle, GitPollTarget, GIT_HUB_DEFERRED_PROBE_DELAY_MS,
    GIT_POLL_INTERVAL_MS, GIT_POLL_MIN_PROBE_SPACING_MS,
};
pub use read::{
    git_state_from_read, GitDiffStatsRead, GitStateProject, ProjectGitRead,
    READ_PROJECT_GIT_STATE_PATH,
};
pub use review::{
    normalize_relative_git_file_path, parse_commit_message, resolve_trusted_file_selection,
    review_modal_draft, GitRemoteProject, PendingGitReview, ReviewDraftInput, TrustedFileSelection,
};
pub use state::{
    GitAction, GitChangedFile, GitHubState, GitPreferences, GitPullRequest, GitPullRequestState,
    GitReviewAction, GitState,
};
pub use text::{
    agent_workflow_title, confirm_label, finished_title, merge_conflict_prompt,
    multicommit_release_prompt, prompt_description, pull_request_agent_prompt, release_only_prompt,
    started_title, sync_with_main_prompt, user_visible_git_error, DEFAULT_PROMPT_AGENT_ID,
    GIT_MULTIPLE_COMMITS_PROMPT, REMOTE_MERGE_CONFLICT_PROMPT,
};
pub use worktree::{
    is_managed_worktree_branch, normalize_existing_worktree_options, normalize_project_path,
    normalize_worktree_base_branches, project_name_from_path, short_status_has_changes,
    worktree_branch_metadata, worktree_branch_name, worktree_delete_warning_title,
    worktree_folder_suffix, worktree_list_error_text, worktree_rename_user_visible_error,
    worktree_slug_from_prompt, worktree_user_visible_error,
};
