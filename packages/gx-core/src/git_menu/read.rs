//! gxserver's answer to `/api/readProjectGitState`, and the state a project shows from it.

use serde::Deserialize;

use super::state::{GitChangedFile, GitHubState, GitPreferences, GitState};
use crate::sidebar_view::ProjectDiffStats;

pub const READ_PROJECT_GIT_STATE_PATH: &str = "/api/readProjectGitState";

/// The header numbers a diff-stats read answers with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffStatsRead {
    #[serde(default)]
    pub additions: i64,
    #[serde(default)]
    pub deletions: i64,
    #[serde(default)]
    pub files: i64,
    #[serde(default)]
    pub is_repo: bool,
}

impl GitDiffStatsRead {
    pub fn into_stats(self) -> ProjectDiffStats {
        ProjectDiffStats {
            additions: self.additions,
            deletions: self.deletions,
            files: self.files,
            is_loading: false,
            is_repo: self.is_repo,
        }
    }
}

/// `/api/readProjectGitState`'s result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitRead {
    #[serde(default)]
    pub is_repo: bool,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub files: Vec<GitChangedFile>,
    #[serde(default)]
    pub additions: i64,
    #[serde(default)]
    pub deletions: i64,
    #[serde(default)]
    pub ahead_count: i64,
    #[serde(default)]
    pub behind_count: i64,
    #[serde(default)]
    pub has_upstream: bool,
    #[serde(default)]
    pub has_origin_remote: bool,
    #[serde(default)]
    pub has_git_hub_remote: bool,
    #[serde(default)]
    pub has_working_tree_changes: bool,
    /// Present when the read asked for the GitHub CLI probe.
    #[serde(default)]
    pub git_hub: Option<GitHubState>,
    /// Present on a diff-stats read: `null` when the project is a repository whose numbers could
    /// not be read this time (keep the last ones), the numbers otherwise.
    #[serde(default)]
    pub diff_stats: Option<GitDiffStatsRead>,
}

/// The project facts a state is composed with, beside the read.
#[derive(Clone, Debug, Default)]
pub struct GitStateProject {
    pub preferences: GitPreferences,
    /// The row carries worktree metadata (a parent project id).
    pub is_worktree: bool,
    pub worktree_name: Option<String>,
}

/// The state a project shows from a full read, with `git_hub` laid over it (the read's own probe,
/// or the memoized answer when the read left the probe out).
pub fn git_state_from_read(
    read: &ProjectGitRead,
    project: &GitStateProject,
    git_hub: &GitHubState,
) -> GitState {
    if !read.is_repo {
        return GitState::not_a_repository(project.preferences);
    }
    GitState {
        additions: read.additions,
        ahead_count: read.ahead_count,
        behind_count: read.behind_count,
        branch: read.branch.clone().filter(|branch| !branch.is_empty()),
        deletions: read.deletions,
        has_checked_git_hub_remote: true,
        has_git_hub_remote: read.has_git_hub_remote,
        has_origin_remote: read.has_origin_remote,
        has_upstream: read.has_upstream,
        has_working_tree_changes: read.has_working_tree_changes,
        is_busy: false,
        is_repo: true,
        files: read.files.clone(),
        is_worktree: project.is_worktree,
        worktree_name: project.worktree_name.clone(),
        ..GitState::with_preferences(project.preferences)
    }
    .with_git_hub(git_hub)
}
