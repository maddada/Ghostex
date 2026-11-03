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
    /// The project's workspace has a ticket command and isn't in a team: its tickets on cards.
    pub(crate) command: bool,
}

impl IssuesWorkflowParts {
    pub(crate) fn from_setup(has_linear_key: bool, gh_signed_in: bool) -> Self {
        Self {
            linear: has_linear_key,
            github: gh_signed_in,
            command: false,
        }
    }

    /// Issues Workflow is on for the project: something is set up.
    pub(crate) fn any(self) -> bool {
        self.linear || self.github || self.command
    }
}

/// The parts `project` has, from what the background passes last found (the project's Linear key
/// and `gh auth status`) and its workspace's ticket command as last placed; never blocks.
///
/// CDXC:WorkMode 2026-11-02 WHY:
/// A pinned ticket command alone turns Issues Workflow on for its workspace's projects, as a Linear key or a signed-in `gh` does, so a workspace that tracks only through its command still gets its tickets on cards. Not in a team workspace, where the command is stopped and keeps its settings.
pub(crate) fn issues_workflow_parts(project: &Value) -> IssuesWorkflowParts {
    IssuesWorkflowParts {
        command: project_ticket_command(project).is_some(),
        ..IssuesWorkflowParts::from_setup(
            super::project_has_linear_key(project),
            crate::session_git_status::cached_gh_cli_available(),
        )
    }
}

/// `PresentationProject.work_source`: the ticket command of the project's workspace, not its
/// tracker (`isTracker: false`): a command can't be picked as the tracker yet.
///
/// CDXC:WorkMode 2026-11-03 WHY:
/// The row carries no icon: an icon can be 16 KB (about 22 KB as a `data:` URL), and on every project of the workspace that was about 650 KB per full snapshot for 30 projects, sent to every client for a field none drew. When a client draws it, carry it once per workspace, not on each row; Settings reads it from `/api/readWorkSource`.
pub(crate) fn presentation_work_source(project: &Value) -> Option<Value> {
    let command = project_ticket_command(project)?;
    let mut source = Map::new();
    source.insert("id".to_string(), Value::from(command.source_id.clone()));
    source.insert("name".to_string(), Value::from(command.info.name.clone()));
    source.insert("isTracker".to_string(), Value::Bool(false));
    Some(Value::Object(source))
}

/// The ticket command of `project`'s workspace as last placed (`cached_project_workspace_id`),
/// unless that workspace is in a team.
pub(crate) fn project_ticket_command(
    project: &Value,
) -> Option<std::sync::Arc<super::WorkspaceCommand>> {
    let workspace_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .and_then(super::cached_project_workspace_id)?;
    if super::cached_workspace_in_team(&workspace_id) {
        return None;
    }
    super::workspace_command(&workspace_id)
}

/// Whether Issues Workflow is on for `project`: a Linear key applies to it, `gh` is signed in, or
/// its workspace has a ticket command outside a team. The name stays from the per-project switch
/// it replaced.
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
