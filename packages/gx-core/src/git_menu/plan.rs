//! What a Git menu row, the project header's Git button or a Git hotkey does
//! (`runSidebarGitAction`), as a decision the host performs.
//!
//! The decision is taken twice: once before the project's Git state is read (the two release
//! workflows start an agent without looking at it) and once after, over the fresh state.

use super::menu::has_remote_commit_delta;
use super::state::{GitAction, GitPullRequestState, GitReviewAction, GitState};
use super::text::{multicommit_release_prompt, release_only_prompt, sync_with_main_prompt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl GitToastLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// A toast a decision ends with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitToast {
    pub level: GitToastLevel,
    pub title: &'static str,
    pub description: Option<&'static str>,
}

const fn toast(
    level: GitToastLevel,
    title: &'static str,
    description: Option<&'static str>,
) -> GitToast {
    GitToast {
        level,
        title,
        description,
    }
}

/// A direct Git write the host runs inside the started/finished toasts (`runGitMutation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitMutation {
    /// Pull fast-forward when the branch has an upstream, then push what is ahead.
    Sync,
    Push,
}

/// What the host does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitActionStep {
    Toast(GitToast),
    /// Start a prompt agent on the project with this title and prompt.
    PromptWorkflow {
        title: &'static str,
        prompt: String,
    },
    /// Open the commit review dialog.
    Review(GitReviewAction),
    /// Open the project's open pull request in the browser.
    OpenExistingPullRequest,
    /// The pull request agent workflow with nothing to commit.
    PullRequestAgentWorkflow,
    Mutation {
        mutation: GitMutation,
        started: &'static str,
        finished: &'static str,
    },
}

/// The step taken before the state is read, if there is one.
pub fn plan_git_action_before_read(action: GitAction) -> Option<GitActionStep> {
    match action {
        GitAction::MultiRelease => Some(GitActionStep::PromptWorkflow {
            title: "Multicommit & Release",
            prompt: multicommit_release_prompt(),
        }),
        GitAction::Release => Some(GitActionStep::PromptWorkflow {
            title: "Release",
            prompt: release_only_prompt(),
        }),
        _ => None,
    }
}

/// The step taken over the fresh `state`. `is_worktree` is whether the project row carries
/// worktree metadata; `remote` picks the remote wording the old runtime used on a remote row.
pub fn plan_git_action_after_read(
    action: GitAction,
    state: &GitState,
    is_worktree: bool,
    remote: bool,
) -> Option<GitActionStep> {
    use GitToastLevel::{Info, Warning};
    if !state.is_repo {
        return Some(GitActionStep::Toast(if remote {
            toast(
                Warning,
                "Remote Git unavailable",
                Some("Open a Git repository on the remote machine to use Git actions."),
            )
        } else {
            toast(
                Warning,
                "Git unavailable",
                Some("Open a Git repository to use Git actions."),
            )
        }));
    }
    match action {
        GitAction::SyncMain => Some(if !is_worktree {
            GitActionStep::Toast(if remote {
                toast(
                    Warning,
                    "Remote worktree unavailable",
                    Some("Open a remote worktree project to sync with main."),
                )
            } else {
                toast(
                    Warning,
                    "Worktree unavailable",
                    Some("Open a worktree project to sync with main."),
                )
            })
        } else {
            GitActionStep::PromptWorkflow {
                title: "Sync with Main",
                prompt: sync_with_main_prompt(),
            }
        }),
        GitAction::SyncRemote => Some(if !has_remote_commit_delta(state) {
            GitActionStep::Toast(toast(Info, "Remote already synced", None))
        } else {
            GitActionStep::Mutation {
                mutation: GitMutation::Sync,
                started: "Syncing remote",
                finished: "Remote sync complete",
            }
        }),
        GitAction::Commit | GitAction::Push | GitAction::Pr if is_worktree => {
            Some(GitActionStep::Review(review_action(action)))
        }
        GitAction::Pr => Some(
            if state
                .pr
                .as_ref()
                .is_some_and(|pr| pr.state == GitPullRequestState::Open)
            {
                GitActionStep::OpenExistingPullRequest
            } else if !state.has_git_hub_cli {
                GitActionStep::Toast(if remote {
                    toast(
                        Warning,
                        "Remote GitHub CLI unavailable",
                        Some("Install GitHub CLI on the remote machine before creating a pull request."),
                    )
                } else {
                    toast(
                        Warning,
                        "GitHub CLI unavailable",
                        Some("Install GitHub CLI before creating a pull request."),
                    )
                })
            } else if state.has_working_tree_changes {
                GitActionStep::Review(GitReviewAction::Pr)
            } else {
                GitActionStep::PullRequestAgentWorkflow
            },
        ),
        GitAction::Commit => Some(if !state.has_working_tree_changes {
            GitActionStep::Toast(toast(
                Info,
                if remote {
                    "No remote changes to commit"
                } else {
                    "No changes to commit"
                },
                None,
            ))
        } else {
            GitActionStep::Review(GitReviewAction::Commit)
        }),
        GitAction::Push => Some(if state.has_working_tree_changes {
            GitActionStep::Review(GitReviewAction::Push)
        } else {
            GitActionStep::Mutation {
                mutation: GitMutation::Push,
                started: "Pushing",
                finished: if remote {
                    "Remote push complete"
                } else {
                    "Push complete"
                },
            }
        }),
        // The release workflows were decided before the read.
        GitAction::MultiRelease | GitAction::Release => None,
    }
}

fn review_action(action: GitAction) -> GitReviewAction {
    match action {
        GitAction::Push => GitReviewAction::Push,
        GitAction::Pr => GitReviewAction::Pr,
        _ => GitReviewAction::Commit,
    }
}
