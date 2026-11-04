//! A session's Link to submenu (Issues Workflow): pick the PR, Linear issues, Linear project or
//! GitHub issue the session is about, unlink one kind, or hand the links back to the branch.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: start any session the normal way and link it later: right-click → Link to → Pull request…, Linear issue…, Linear project…, GitHub issue…. Something already linked shows its ID in the menu; picking another one replaces it and Unlink removes it. Only sessions of projects with work mode on get the submenu. Slack threads come later and get no row yet.
//!
//! SEE-ALSO: `link_work` / `set_session_work_links` in commands.rs (the payloads),
//! sidebar_actions/modals.rs (`linkWork` opens the picker), server/src/work_mode/links.rs (what
//! Unlink and Back to automatic write).

use serde_json::json;

use crate::sidebar_view::view::SessionRow;

use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::item::MenuItem;

/// The submenu, or `None` for a session of a project with nothing set up (no Linear key, `gh` not
/// signed in), a remote machine's row or a faded one (`/api/setSessionWorkLinks` and the picker
/// answer for this computer's gxserver).
pub(crate) fn link_submenu(id: &str, row: &SessionRow, group: &MenuGroup<'_>) -> Option<MenuItem> {
    let project = group.project?;
    if !project.work_mode || group.is_remote || group.is_stale || row.key.is_none() {
        return None;
    }
    let work = row.work.clone().unwrap_or_default();
    let pull_request = work
        .pull_request
        .as_ref()
        .map(|pr| format!("#{}", pr.number));
    let linear_issues = (!work.linear_issues.is_empty()).then(|| {
        work.linear_issues
            .iter()
            .map(|issue| issue.identifier.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let linear_project = work
        .linear_project
        .as_ref()
        .map(|project| project.name.clone());
    let github_issues = (!work.github_issues.is_empty()).then(|| {
        work.github_issues
            .iter()
            .map(|issue| format!("#{}", issue.number))
            .collect::<Vec<_>>()
            .join(", ")
    });

    let github_project = work.github_project.as_ref().map(|project| project.label());

    let pick = |label: &str, icon: &str, kind: &str, linked: &Option<String>| MenuItem {
        suffix: linked.clone(),
        ..MenuItem::row(label, icon, MenuCommand::link_work(id, kind))
    };
    // CDXC:WorkMode 2026-10-09 DECISION:
    // User: Link to offers the primary tracker's tickets and projects ("Linear Tickets & Projects or Github Issues & Projects - Need to pick just 1"); links of the other kind that are already set can still be unlinked below.
    // CDXC:WorkMode 2026-10-24 DECISION: User: "Linear parts appear once you add a Linear key, GitHub parts once gh is signed in": Pull request… and the GitHub rows need `gh`, the Linear rows a Linear key.
    let mut children = Vec::new();
    if project.work_pull_requests {
        children.push(pick(
            "Pull request…",
            "git-pull-request",
            "pullRequest",
            &pull_request,
        ));
    }
    if project.work_github {
        children.push(pick(
            "GitHub issue…",
            "circle-dot",
            "githubIssue",
            &github_issues,
        ));
        children.push(pick(
            "GitHub project…",
            "box",
            "githubProject",
            &github_project,
        ));
    } else if project.work_linear && !project.work_tracker_github {
        children.push(pick("Linear issue…", "hash", "linearIssue", &linear_issues));
        children.push(pick(
            "Linear project…",
            "box",
            "linearProject",
            &linear_project,
        ));
    }

    // Unlink writes "explicitly none" for that kind, so the branch cannot bring it back.
    let unlink_rows: Vec<MenuItem> = [
        (
            pull_request.is_some(),
            "Unlink pull request",
            json!({ "pullRequest": "none" }),
        ),
        (
            linear_issues.is_some(),
            if work.linear_issues.len() > 1 {
                "Unlink Linear issues"
            } else {
                "Unlink Linear issue"
            },
            json!({ "linearIssues": [] }),
        ),
        (
            linear_project.is_some(),
            "Unlink Linear project",
            json!({ "linearProject": "" }),
        ),
        (
            github_issues.is_some(),
            if work.github_issues.len() > 1 {
                "Unlink GitHub issues"
            } else {
                "Unlink GitHub issue"
            },
            json!({ "githubIssues": [] }),
        ),
        (
            github_project.is_some(),
            "Unlink GitHub project",
            json!({ "githubProject": "none" }),
        ),
    ]
    .into_iter()
    .filter(|(linked, _, _)| *linked)
    .map(|(_, label, links)| {
        MenuItem::row(
            label,
            "unlink",
            MenuCommand::command(message::set_session_work_links(id, links)),
        )
    })
    .collect();
    if !unlink_rows.is_empty() {
        if !children.is_empty() {
            children.push(MenuItem::separator());
        }
        children.extend(unlink_rows);
    }
    if work.hand_set {
        if !children.is_empty() {
            children.push(MenuItem::separator());
        }
        children.push(MenuItem::row(
            "Back to automatic",
            "refresh",
            MenuCommand::command(message::set_session_work_links(
                id,
                json!({ "clear": true }),
            )),
        ));
    }
    (!children.is_empty()).then(|| MenuItem::submenu("Link to", "link", children))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{MachineId, SessionKey};
    use crate::sidebar_view::view::ProjectContextView;

    fn row() -> SessionRow {
        SessionRow {
            key: Some(SessionKey {
                machine: MachineId::Local,
                project_id: "P1".to_string(),
                session_id: "S1".to_string(),
            }),
            ..SessionRow::default()
        }
    }

    fn labels(project: &ProjectContextView) -> Option<Vec<String>> {
        let group = MenuGroup {
            group_id: "P1",
            storage_id: "P1",
            title: "Project",
            is_remote: false,
            remote_machine_name: None,
            is_stale: false,
            can_create_session_group: false,
            can_focus_mode: false,
            project: Some(project),
            can_remove_project: false,
            git_remote_origin_url: None,
            server_id: None,
            agent_place: Default::default(),
        };
        link_submenu("S1", &row(), &group).map(|menu| {
            menu.children
                .unwrap_or_default()
                .into_iter()
                .filter_map(|child| child.label)
                .collect()
        })
    }

    #[test]
    fn nothing_set_up_has_no_link_to() {
        assert_eq!(labels(&ProjectContextView::default()), None);
    }

    #[test]
    fn gh_signed_in_without_a_linear_key_offers_no_linear_rows() {
        let project = ProjectContextView {
            work_mode: true,
            work_pull_requests: true,
            ..ProjectContextView::default()
        };
        assert_eq!(labels(&project), Some(vec!["Pull request…".to_string()]));
    }

    #[test]
    fn a_linear_key_without_gh_offers_only_linear_rows() {
        let project = ProjectContextView {
            work_mode: true,
            work_linear: true,
            ..ProjectContextView::default()
        };
        assert_eq!(
            labels(&project),
            Some(vec![
                "Linear issue…".to_string(),
                "Linear project…".to_string()
            ])
        );
    }

    #[test]
    fn a_github_tracker_offers_github_rows_and_hides_linear_ones() {
        let project = ProjectContextView {
            work_mode: true,
            work_linear: true,
            work_pull_requests: true,
            work_github: true,
            work_tracker_github: true,
            ..ProjectContextView::default()
        };
        assert_eq!(
            labels(&project),
            Some(vec![
                "Pull request…".to_string(),
                "GitHub issue…".to_string(),
                "GitHub project…".to_string(),
            ])
        );
    }
}
