//! Which parts of Issues Workflow (formerly Work mode) a project has, and the project a request
//! names.
//!
//! CDXC:WorkMode 2026-10-24 DECISION:
//! User: Work mode is renamed "Issues Workflow", and it is "On everywhere once set up": "No switch; Linear parts appear once you add a Linear key, GitHub parts once gh is signed in." This supersedes the per-project switch (right-click the project → Work Mode, the Settings > Projects switch, `ghostex work-mode on|off`) and the Work-workspace default; a project that had it on or off keeps nothing special, and the stored `launchSettings.workMode` is left alone and no longer read. Asked whether it should still need the Workspaces extension, the user answered "Yes, no Workspaces needed" (replacing CDXC:Workspaces 2026-10-09 in packages/settings-catalog/src/data/official_extensions.rs). Internal names (`workMode`, `work_mode`, `/api/*Work*`) stay, so older clients keep working: they read `workMode` as "on".

use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

/// The parts of Issues Workflow a project has right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct IssuesWorkflowParts {
    /// A Linear key applies to the project: Create Linear Ticket, Linear statuses, Link to Linear.
    pub(crate) linear: bool,
    /// `gh` is signed in: pull requests, GitHub issues and GitHub Projects.
    pub(crate) github: bool,
    /// Create Linear Ticket is offered: a Linear key applies and either the workspace picked Linear
    /// or the key is the project's or its workspace's own (`project_linear_create`).
    pub(crate) linear_create: bool,
}

impl IssuesWorkflowParts {
    pub(crate) fn from_setup(has_linear_key: bool, gh_signed_in: bool) -> Self {
        Self {
            linear: has_linear_key,
            github: gh_signed_in,
            linear_create: false,
        }
    }

    /// Issues Workflow is on for the project: something is set up.
    pub(crate) fn any(self) -> bool {
        self.linear || self.github
    }
}

/// The parts `project` has, from what the background passes last found (the project's Linear key
/// and `gh auth status`); never blocks.
pub(crate) fn issues_workflow_parts(project: &Value) -> IssuesWorkflowParts {
    IssuesWorkflowParts {
        linear_create: project
            .get("projectId")
            .and_then(Value::as_str)
            .is_some_and(super::project_linear_create),
        ..IssuesWorkflowParts::from_setup(
            super::project_has_linear_key(project),
            crate::session_git_status::cached_gh_cli_available(),
        )
    }
}

/// What a project row last published of Issues Workflow, which the card pass compares to tell
/// whether to send the row again: its parts and its tracker.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PublishedProjectWork {
    pub(crate) parts: IssuesWorkflowParts,
    tracker: super::WorkSourceId,
}

pub(crate) fn published_project_work(project_id: &str, project: &Value) -> PublishedProjectWork {
    PublishedProjectWork {
        parts: issues_workflow_parts(project),
        tracker: super::cached_project_work_tracker(project_id),
    }
}

/// Whether Issues Workflow is on for `project`: a Linear key applies to it or `gh` is signed in.
/// The name stays from the per-project switch it replaced.
pub(crate) fn project_work_mode(project: &Value) -> bool {
    issues_workflow_parts(project).any()
}

/// The project a request names: `projectId`, else the project whose folder holds `path`
/// (the deepest one, so a worktree project wins over its parent checkout).
pub(crate) fn resolve_work_mode_project(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if let Some(project_id) = params
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
    {
        return repository.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::bad_request(format!("No project matched \"{project_id}\"."))
        });
    }
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Pass projectId or path."))?;
    let wanted = comparable_path(path);
    let mut best: Option<(usize, Value)> = None;
    for project in repository.list_projects()? {
        let Some(project_path) = project.get("path").and_then(Value::as_str) else {
            continue;
        };
        let candidate = comparable_path(project_path);
        if candidate.is_empty() {
            continue;
        }
        let inside = wanted == candidate || wanted.starts_with(&format!("{candidate}/"));
        if inside
            && best
                .as_ref()
                .is_none_or(|(length, _)| candidate.len() > *length)
        {
            best = Some((candidate.len(), project));
        }
    }
    best.map(|(_, project)| project)
        .ok_or_else(|| DomainStateError::bad_request(format!("No project holds \"{path}\".")))
}

fn comparable_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_end_matches('/').to_string();
    if cfg!(windows) || cfg!(target_os = "macos") {
        path.to_lowercase()
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_once_a_linear_key_or_gh_is_set_up() {
        assert!(!IssuesWorkflowParts::from_setup(false, false).any());
        let linear = IssuesWorkflowParts::from_setup(true, false);
        assert!(linear.any() && linear.linear && !linear.github);
        let github = IssuesWorkflowParts::from_setup(false, true);
        assert!(github.any() && github.github && !github.linear);
        assert!(IssuesWorkflowParts::from_setup(true, true).any());
    }
}
