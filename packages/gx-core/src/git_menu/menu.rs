//! The Git menu: its rows, their labels and why a row is disabled. Ported from
//! packages/shared/sidebar-git.ts (`buildSidebarGitMenuItems`, `getSidebarGitDisabledReason`,
//! `resolveSidebarGitPrimaryActionState`) and the old runtime's
//! `createGpuiTitlebarGitMenuStatePayload`, which the titlebar Git menu and the work area's Commit
//! button draw verbatim.

use serde::Serialize;

use super::state::{GitAction, GitPullRequestState, GitState};

/// One row of the Git menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitMenuItem {
    pub action: GitAction,
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub label: String,
}

/// The split button's primary half.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPrimaryActionState {
    pub action: GitAction,
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub label: String,
}

/// One row of the titlebar Git menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TitlebarGitMenuRow {
    pub action: GitAction,
    pub disabled: bool,
    pub label: String,
    pub primary: bool,
}

/// What the titlebar Git menu and the work area's Commit button draw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitlebarGitMenu {
    pub additions: i64,
    pub ahead_count: i64,
    pub behind_count: i64,
    pub branch: Option<String>,
    pub deletions: i64,
    pub has_working_tree_changes: bool,
    pub is_busy: bool,
    pub is_repo: bool,
    pub primary_action: GitAction,
    pub rows: Vec<TitlebarGitMenuRow>,
    pub sync_remote_disabled: bool,
}

/// `hasSidebarGitRemoteCommitDelta`.
///
/// CDXC:Git 2026-06-16-18:41:
/// Remote sync availability and titlebar copy share one normalized ahead/behind check so a synced
/// branch cannot run the sync action while the Commits row shows ↑0 ↓0.
pub fn has_remote_commit_delta(state: &GitState) -> bool {
    state.behind_count.max(0) > 0 || state.ahead_count.max(0) > 0
}

/// `getSidebarGitDisabledReason`.
pub fn disabled_reason(state: &GitState, action: GitAction) -> Option<&'static str> {
    if state.is_busy {
        return Some("Git action already running.");
    }
    if !state.is_repo {
        return Some("Open a Git repository to use Git actions.");
    }
    if action == GitAction::Commit {
        return (!state.has_working_tree_changes).then_some("No working tree changes to commit.");
    }
    if matches!(action, GitAction::MultiRelease | GitAction::Release) {
        return None;
    }
    if state.branch.is_none() {
        return Some(if action == GitAction::SyncRemote {
            "Create and checkout a branch before syncing."
        } else {
            "Create and checkout a branch before pushing or creating a PR."
        });
    }
    if action == GitAction::SyncMain {
        return (!state.is_worktree).then_some("Open a worktree project to sync with main.");
    }
    if action == GitAction::SyncRemote {
        // CDXC:Git 2026-06-16-07:31:
        // The macOS titlebar sync row is a direct remote branch sync, not the worktree-only Sync
        // with Main agent workflow. Enable it for any checked-out branch with an upstream or an
        // origin remote so normal projects can pull and push from the titlebar menu.
        if state.has_upstream || state.has_origin_remote {
            return None;
        }
        return Some("Add an \"origin\" remote or set an upstream before syncing.");
    }
    if state.behind_count > 0 {
        return Some("Branch is behind upstream. Pull or rebase first.");
    }
    if action == GitAction::Push {
        if state.has_working_tree_changes || state.ahead_count > 0 {
            return None;
        }
        if !state.has_upstream && state.has_origin_remote {
            return None;
        }
        if !state.has_origin_remote {
            return Some("Add an \"origin\" remote before pushing.");
        }
        return Some("No local commits to push.");
    }
    if !state.has_git_hub_cli {
        return Some("Install GitHub CLI to create or view pull requests.");
    }
    if state.has_working_tree_changes {
        return None;
    }
    if pr_is_open(state) {
        return None;
    }
    if state.ahead_count > 0 {
        return None;
    }
    if !state.has_upstream && state.has_origin_remote {
        return None;
    }
    if !state.has_origin_remote {
        return Some("Add an \"origin\" remote before creating a PR.");
    }
    if state.has_upstream {
        return None;
    }
    Some("No branch state available for PR creation.")
}

fn pr_is_open(state: &GitState) -> bool {
    state
        .pr
        .as_ref()
        .is_some_and(|pr| pr.state == GitPullRequestState::Open)
}

fn menu_item(action: GitAction, label: &str, state: &GitState) -> GitMenuItem {
    let reason = disabled_reason(state, action);
    GitMenuItem {
        action,
        disabled: reason.is_some(),
        disabled_reason: reason.map(str::to_string),
        label: label.to_string(),
    }
}

/// `buildSidebarGitMenuItems`.
///
/// CDXC:Worktrees 2026-05-30-05:13:
/// Sync with Main is a worktree-only Git workflow. Show it beside Create PR in the Git dropdown
/// only when the active project is a worktree, because main projects do not need to pull main into
/// themselves before worktree merge.
pub fn menu_items(state: &GitState) -> Vec<GitMenuItem> {
    let mut items = vec![
        menu_item(GitAction::Commit, "Commit", state),
        menu_item(GitAction::Push, "Push", state),
        menu_item(
            GitAction::Pr,
            if pr_is_open(state) {
                "View PR"
            } else {
                "Create PR"
            },
            state,
        ),
    ];
    if state.is_worktree {
        items.push(menu_item(GitAction::SyncMain, "Sync with Main", state));
    }
    items.push(menu_item(
        GitAction::MultiRelease,
        "Multicommit & Release",
        state,
    ));
    items.push(menu_item(GitAction::Release, "Release", state));
    items
}

fn pr_primary_label(state: &GitState) -> &'static str {
    let needs_push = state.has_working_tree_changes || state.ahead_count > 0 || !state.has_upstream;
    if state.has_working_tree_changes {
        return "Commit, Push & PR";
    }
    if pr_is_open(state) && !needs_push {
        return "View PR";
    }
    if needs_push {
        return if pr_is_open(state) {
            "Push & View PR"
        } else {
            "Push & Create PR"
        };
    }
    if pr_is_open(state) {
        "View PR"
    } else {
        "Create PR"
    }
}

/// `resolveSidebarGitPrimaryActionState`.
pub fn primary_action_state(state: &GitState) -> GitPrimaryActionState {
    let action = GitAction::normalize_primary(Some(state.primary_action.selector()));
    let reason = disabled_reason(state, action);
    let label = match action {
        GitAction::Push if state.has_working_tree_changes => "Commit & Push",
        GitAction::Push => "Push",
        GitAction::Pr => pr_primary_label(state),
        _ => "Commit",
    };
    GitPrimaryActionState {
        action,
        disabled: reason.is_some(),
        disabled_reason: reason.map(str::to_string),
        label: label.to_string(),
    }
}

/// `createGpuiTitlebarGitMenuStatePayload`: the native titlebar renders this projection verbatim,
/// so the menu builders above stay the single owner of row order, labels and disabled gating. The
/// primary row carries the resolved split-primary label, since a native menu cannot express the
/// split control.
pub fn titlebar_menu(state: &GitState) -> TitlebarGitMenu {
    let primary = primary_action_state(state);
    TitlebarGitMenu {
        additions: state.additions,
        ahead_count: state.ahead_count,
        behind_count: state.behind_count,
        branch: state.branch.clone(),
        deletions: state.deletions,
        has_working_tree_changes: state.has_working_tree_changes,
        is_busy: state.is_busy,
        is_repo: state.is_repo,
        primary_action: primary.action,
        rows: menu_items(state)
            .into_iter()
            .map(|item| {
                let is_primary = item.action == primary.action;
                TitlebarGitMenuRow {
                    action: item.action,
                    disabled: if is_primary {
                        primary.disabled
                    } else {
                        item.disabled
                    },
                    label: if is_primary {
                        primary.label.clone()
                    } else {
                        item.label
                    },
                    primary: is_primary,
                }
            })
            .collect(),
        sync_remote_disabled: disabled_reason(state, GitAction::SyncRemote).is_some()
            || !has_remote_commit_delta(state),
    }
}
