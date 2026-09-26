//! Git, worktrees and transcript export, in Rust (app runtime port family F5). Per-concern files:
//! `calls.rs` holds the gxserver calls themselves and nothing else (only `gx_rpc` and gx-core, so
//! the web build can take it unchanged); `scope.rs` resolves which project, on which computer, a
//! Git request is about; `hud.rs` keeps the active project's Git state and draws the titlebar Git
//! menu and the Commit button from it; `poll.rs` is the background poll behind the project
//! headers' +/- numbers; `toasts.rs` the toasts every flow shows; `actions.rs` what a Git menu row,
//! the header's Git button or a Git hotkey does; `review.rs` the commit review dialog and its
//! confirm; `merge.rs` the review's direct merge into main; `prompt_agent.rs` starts the agent a
//! Git workflow hands its work to; the `worktree_*.rs` files are the Add, Delete and Rename
//! Worktree dialogs; `export_transcript.rs` is Handoff / Export.
//!
//! CDXC:Git 2026-09-25 WHY:
//! These used to run in the desktop's QuickJS app runtime (`gxserver-runtime/git/`,
//! `worktrees.ts`, `export-transcript.ts`), which read Git state with about ten calls per refresh
//! and ran the commit and push as a client-side sequence. gxserver now answers the read in one
//! call and runs the writes (server/src/project_git_state.rs, server/src/git_ship_workflow.rs);
//! the rules in between are gx-core's (packages/gx-core/src/git_menu/). What is left here is the
//! host: which project is active, the toasts, the dialogs, the timers and the tunnel.

mod actions;
mod calls;
mod export_transcript;
mod hud;
mod merge;
mod modal_commands;
mod poll;
mod prompt_agent;
mod review;
mod scope;
mod toasts;
mod worktree_create;
mod worktree_delete;
mod worktree_list;
mod worktree_rename;

use std::collections::{HashMap, HashSet};

use ghostex_gx_core::ProjectDiffStats;
use ghostex_gx_core::git_menu::{
    GIT_HUB_MEMO_TTL_MS, GIT_STATE_MEMO_TTL_MS, GitHubState, GitState, GitTtlMemo,
    PendingGitReview,
};

pub(crate) use export_transcript::{ExportTranscriptHost, HandoffTarget};
pub(crate) use prompt_agent::PromptAgentLaunch;
pub(crate) use worktree_list::TrustedWorktreeList;

/// Everything this family keeps between calls.
pub(crate) struct GitHost {
    /// The scoped project id (a raw local id, or `remote:<machine>:project:<id>`) the titlebar
    /// Git state describes, and the one the last switch-driven refresh ran for.
    pub(super) active_scope: Option<String>,
    pub(super) last_refresh_scope: Option<String>,
    /// The active project's state, as read. Preferences are laid over it when it is drawn.
    pub(super) state: GitState,
    /// Local Git states by project id, and GitHub answers by project id (`CDXC:Git 2026-07-29`).
    pub(super) state_memo: GitTtlMemo<GitState>,
    pub(super) git_hub_memo: GitTtlMemo<GitHubState>,
    pub(super) pending_git_hub_probes: HashSet<String>,
    /// The background poll: its generation (a new cycle cancels the old one's timers), the
    /// header numbers it found by the id the sidebar looks them up by, and the projects whose
    /// probe is in flight.
    pub(super) poll_generation: u64,
    pub(super) poll_started: bool,
    pub(super) diff_stats: HashMap<String, ProjectDiffStats>,
    pub(super) pending_diff_probes: HashSet<String>,
    /// Open review dialogs by request id.
    pub(super) reviews: HashMap<String, PendingGitReview>,
    pub(super) trusted_worktrees: Option<TrustedWorktreeList>,
    pub(super) export: ExportTranscriptHost,
    pub(super) counters: GitCounters,
}

impl Default for GitHost {
    fn default() -> Self {
        Self {
            active_scope: None,
            last_refresh_scope: None,
            state: GitState::default(),
            state_memo: GitTtlMemo::new(GIT_STATE_MEMO_TTL_MS),
            git_hub_memo: GitTtlMemo::new(GIT_HUB_MEMO_TTL_MS),
            pending_git_hub_probes: HashSet::new(),
            poll_generation: 0,
            poll_started: false,
            diff_stats: HashMap::new(),
            pending_diff_probes: HashSet::new(),
            reviews: HashMap::new(),
            trusted_worktrees: None,
            export: ExportTranscriptHost::default(),
            counters: GitCounters::default(),
        }
    }
}

/// What happened since the app started. Memory only.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GitCounters {
    pub(crate) state_reads: u64,
    pub(crate) memo_hits: u64,
    pub(crate) git_hub_probes: u64,
    pub(crate) poll_cycles: u64,
    pub(crate) poll_probes: u64,
    pub(crate) actions: u64,
    pub(crate) workflows: u64,
}
