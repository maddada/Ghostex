//! The Git state one project shows: the facts gxserver read (`/api/readProjectGitState`) plus the
//! three preferences the project's `gitConfig` carries.
//!
//! The field names are the TypeScript `SidebarGitState`'s (packages/shared/sidebar-git.ts), so a
//! state serializes to the same JSON the old runtime published and the parity harness can diff
//! the two byte for byte.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One row of the Git menu, and the action a Git button or hotkey runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitAction {
    Commit,
    Push,
    Pr,
    SyncRemote,
    SyncMain,
    MultiRelease,
    Release,
}

impl GitAction {
    pub const ALL: [GitAction; 7] = [
        GitAction::Commit,
        GitAction::Push,
        GitAction::Pr,
        GitAction::SyncMain,
        GitAction::SyncRemote,
        GitAction::MultiRelease,
        GitAction::Release,
    ];

    pub fn selector(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::Pr => "pr",
            Self::SyncRemote => "syncRemote",
            Self::SyncMain => "syncMain",
            Self::MultiRelease => "multiRelease",
            Self::Release => "release",
        }
    }

    pub fn from_selector(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.selector() == value)
    }

    /// `normalizeSidebarGitAction`: the primary action a project may store is Push or PR, anything
    /// else (including the agent workflows) reads as Commit.
    pub fn normalize_primary(candidate: Option<&str>) -> Self {
        match candidate {
            Some("push") => Self::Push,
            Some("pr") => Self::Pr,
            _ => Self::Commit,
        }
    }
}

/// The three review-dialog actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitReviewAction {
    Commit,
    Push,
    Pr,
}

impl GitReviewAction {
    pub fn selector(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::Pr => "pr",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitChangedFile {
    pub additions: i64,
    pub deletions: i64,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitPullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPullRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
    pub state: GitPullRequestState,
    pub title: String,
    pub url: String,
}

/// What the GitHub CLI said about a project: kept apart from the local Git facts because it has
/// its own, much longer lease (`CDXC:Git 2026-07-29`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubState {
    pub has_git_hub_cli: bool,
    pub pr: Option<GitPullRequest>,
}

/// A project's three Git preferences (`gitConfig`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitPreferences {
    pub confirm_commit: bool,
    pub generate_commit_body: bool,
    pub primary_action: GitAction,
}

impl Default for GitPreferences {
    fn default() -> Self {
        Self {
            confirm_commit: false,
            generate_commit_body: true,
            primary_action: GitAction::Commit,
        }
    }
}

impl GitPreferences {
    /// `gitPreferencesForProject`: a missing or non-boolean key reads as its default.
    pub fn from_git_config(git_config: Option<&Value>) -> Self {
        let boolean = |key: &str| git_config.and_then(|config| config.get(key)?.as_bool());
        let primary = git_config
            .and_then(|config| config.get("primaryAction")?.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        Self {
            confirm_commit: boolean("confirmCommit").unwrap_or(false),
            generate_commit_body: boolean("generateCommitBody").unwrap_or(true),
            primary_action: GitAction::normalize_primary(primary),
        }
    }
}

/// `SidebarGitState`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitState {
    pub additions: i64,
    pub ahead_count: i64,
    pub behind_count: i64,
    pub branch: Option<String>,
    pub confirm_suggested_commit: bool,
    pub deletions: i64,
    pub generate_commit_body: bool,
    pub has_checked_git_hub_remote: bool,
    pub has_git_hub_cli: bool,
    pub has_git_hub_remote: bool,
    pub has_origin_remote: bool,
    pub has_upstream: bool,
    pub has_working_tree_changes: bool,
    pub is_busy: bool,
    pub is_repo: bool,
    pub files: Vec<GitChangedFile>,
    pub is_worktree: bool,
    pub pr: Option<GitPullRequest>,
    pub primary_action: GitAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_name: Option<String>,
}

impl Default for GitState {
    fn default() -> Self {
        Self::with_preferences(GitPreferences::default())
    }
}

impl GitState {
    /// `createDefaultSidebarGitState(primaryAction, confirmCommit, generateCommitBody)`.
    pub fn with_preferences(preferences: GitPreferences) -> Self {
        Self {
            additions: 0,
            ahead_count: 0,
            behind_count: 0,
            branch: None,
            confirm_suggested_commit: preferences.confirm_commit,
            deletions: 0,
            generate_commit_body: preferences.generate_commit_body,
            has_checked_git_hub_remote: false,
            has_git_hub_cli: false,
            has_git_hub_remote: false,
            has_origin_remote: false,
            has_upstream: false,
            has_working_tree_changes: false,
            is_busy: false,
            is_repo: false,
            files: Vec::new(),
            is_worktree: false,
            pr: None,
            primary_action: preferences.primary_action,
            worktree_name: None,
        }
    }

    /// The same state with `preferences` laid over it (`gitStateForHud`,
    /// `applyLiveGitStateOverlays`): preferences change without a refresh, so a stored state never
    /// decides them.
    pub fn with_overlaid_preferences(mut self, preferences: GitPreferences) -> Self {
        self.confirm_suggested_commit = preferences.confirm_commit;
        self.generate_commit_body = preferences.generate_commit_body;
        self.primary_action = preferences.primary_action;
        self
    }

    /// The same state with a GitHub answer laid over it.
    pub fn with_git_hub(mut self, git_hub: &GitHubState) -> Self {
        self.has_git_hub_cli = git_hub.has_git_hub_cli;
        self.pr = git_hub.pr.clone();
        self
    }

    /// A project that is not a repository, or that could not be read: the preferences and
    /// nothing else, marked as checked.
    pub fn not_a_repository(preferences: GitPreferences) -> Self {
        Self {
            has_checked_git_hub_remote: true,
            is_repo: false,
            ..Self::with_preferences(preferences)
        }
    }
}
