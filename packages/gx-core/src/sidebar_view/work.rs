//! What a session of a work-mode project is linked to, as its card and its menus read it.
//!
//! The daemon publishes `PresentationSession.work` only for sessions of projects with work mode on
//! (server/src/work_mode/), so a row with links here is exactly a row that draws a second line.

use ghostex_gx_protocol::presentation::PresentationSessionWork;

/// A session's work links, in the client's own vocabulary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionWork {
    /// The checkout's branch; absent on the default branch or a detached HEAD.
    pub branch: Option<String>,
    pub pull_request: Option<WorkPullRequest>,
    pub linear_issues: Vec<WorkLinearIssue>,
    pub github_issues: Vec<WorkGithubIssue>,
    /// A release the team works on in Linear, never a repo.
    pub linear_project: Option<WorkLinearProject>,
    /// A GitHub Project (Projects v2), when the workspace's primary tracker is GitHub.
    pub github_project: Option<WorkGithubProject>,
    /// Some link was set by hand, so the Link to menu offers "Back to automatic".
    pub hand_set: bool,
    /// The linked PR is merged and the Clean up / Keep offer for it is unanswered.
    pub offer_cleanup: bool,
    /// Its computer answers the offer for all of a coordinator's threads at once, so the offer goes
    /// in the coordinator's banner instead of on the row.
    pub offer_cleanup_all: bool,
    /// Cleaned up by itself: `Some(can_undo)` while the coordinator's banner reports it.
    pub auto_cleaned: Option<bool>,
    /// The worktree was cleaned up for the merged PR.
    pub cleaned_up: bool,
}

impl SessionWork {
    /// The work behind the row is over: its PR is merged, or its worktree was cleaned up for it.
    pub fn is_finished(&self) -> bool {
        self.cleaned_up
            || self
                .pull_request
                .as_ref()
                .is_some_and(|pull_request| pull_request.state == "merged")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkPullRequest {
    pub number: u64,
    /// `open`, `draft`, `merged` or `closed`.
    pub state: String,
    pub url: Option<String>,
    /// `passing`, `failing` or `pending`; absent until the checks were read.
    pub checks: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkLinearIssue {
    pub identifier: String,
    pub title: Option<String>,
    /// Linear's workflow state type: `triage`, `backlog`, `unstarted`, `started`, `completed` or
    /// `canceled`.
    pub state_type: Option<String>,
    /// The team's own name for the state, e.g. "In Review".
    pub state_name: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkGithubIssue {
    pub number: u64,
    pub title: Option<String>,
    /// `open` or `closed`.
    pub state: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkLinearProject {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkGithubProject {
    /// The login that owns it; `owner/number` names it.
    pub owner: String,
    pub number: u64,
    /// Absent until `gh` could read it (it needs the `read:project` scope).
    pub title: Option<String>,
    pub url: Option<String>,
    /// The item's Status on the project board.
    pub status: Option<String>,
}

impl WorkGithubProject {
    /// What a chip or a menu shows: the title, else `owner/number`.
    pub fn label(&self) -> String {
        self.title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("{}/{}", self.owner, self.number))
    }
}

impl SessionWork {
    pub(crate) fn from_presentation(work: &PresentationSessionWork) -> Self {
        Self {
            branch: work
                .branch
                .clone()
                .filter(|branch| !branch.trim().is_empty()),
            pull_request: work.pull_request.as_ref().map(|pr| WorkPullRequest {
                number: pr.number,
                state: pr.state.as_str().to_string(),
                url: pr.url.clone(),
                checks: pr.checks.as_ref().map(|checks| checks.as_str().to_string()),
            }),
            linear_issues: work
                .linear_issues
                .iter()
                .map(|issue| WorkLinearIssue {
                    identifier: issue.identifier.clone(),
                    title: issue.title.clone(),
                    state_type: issue.state_type.clone(),
                    state_name: issue.state_name.clone(),
                    url: issue.url.clone(),
                })
                .collect(),
            github_issues: work
                .github_issues
                .iter()
                .map(|issue| WorkGithubIssue {
                    number: issue.number,
                    title: issue.title.clone(),
                    state: issue.state.clone(),
                    url: issue.url.clone(),
                })
                .collect(),
            linear_project: work
                .linear_project
                .as_ref()
                .map(|project| WorkLinearProject {
                    name: project.name.clone(),
                    url: project.url.clone(),
                }),
            github_project: work
                .github_project
                .as_ref()
                .map(|project| WorkGithubProject {
                    owner: project.owner.clone(),
                    number: project.number,
                    title: project.title.clone(),
                    url: project.url.clone(),
                    status: project.status.clone(),
                }),
            hand_set: work.hand_set,
            offer_cleanup: work.offer_cleanup,
            offer_cleanup_all: work.offer_cleanup_all,
            auto_cleaned: work.auto_cleaned.as_ref().map(|line| line.undo),
            cleaned_up: work.cleaned_up,
        }
    }

    /// Whether anything is linked, which is what earns the card its second line.
    pub fn has_links(&self) -> bool {
        self.pull_request.is_some()
            || !self.linear_issues.is_empty()
            || !self.github_issues.is_empty()
            || self.linear_project.is_some()
            || self.github_project.is_some()
    }
}
