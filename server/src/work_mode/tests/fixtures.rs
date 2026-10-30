use serde_json::{json, Value};

use crate::session_git_status::{set_cached_session_git_status_for_test, SessionGitStatus};

/// The `gh` login every test seeds, so tests that run at the same time agree on "me".
pub(super) const VIEWER_LOGIN: &str = "octo-me";

/// Fails with the whole published value pretty-printed, so a changed field is easy to spot.
#[track_caller]
pub(super) fn assert_published(actual: &Value, expected: Value) {
    assert!(
        *actual == expected,
        "published value changed:\n{}",
        serde_json::to_string_pretty(actual).unwrap()
    );
}

pub(super) fn project(project_id: &str, path: &str) -> Value {
    json!({ "projectId": project_id, "name": format!("{project_id} app"), "path": path })
}

pub(super) fn session(project_id: &str, session_id: &str, work_links: Value) -> Value {
    json!({
        "projectId": project_id,
        "sessionId": session_id,
        "title": format!("Session {session_id}"),
        "lifecycleState": "running",
        "agentId": "claude",
        "runtimeSettings": { "workLinks": work_links },
    })
}

pub(super) fn set_branch(cwd: &str, branch: &str) {
    set_cached_session_git_status_for_test(
        cwd,
        Some(SessionGitStatus {
            branch: Some(branch.to_string()),
            additions: 0,
            deletions: 0,
            pull_request: None,
            updated_at: "2026-10-27T10:00:00.000Z".to_string(),
        }),
    );
}

/// An issue as Linear's `issues` query answers it (`LINEAR_LIST_FIELDS` in feeds.rs).
pub(super) fn linear_node(identifier: &str, state: (&str, &str), updated_at: &str) -> Value {
    let team = identifier.split('-').next().unwrap();
    json!({
        "identifier": identifier,
        "title": format!("Title of {identifier}"),
        "url": format!("https://linear.app/acme/issue/{identifier}"),
        "updatedAt": updated_at,
        "branchName": format!("leandro/{}-work", identifier.to_ascii_lowercase()),
        "state": { "name": state.1, "type": state.0 },
        "assignee": null,
        "team": { "key": team },
        "project": null,
        "cycle": null,
        "labels": { "nodes": [] },
        "attachments": { "nodes": [] },
    })
}

pub(super) fn linear_issue_info(identifier: &str, state: (&str, &str)) -> LinearIssueInfo {
    LinearIssueInfo {
        identifier: identifier.to_string(),
        title: Some(format!("Title of {identifier}")),
        url: Some(format!("https://linear.app/acme/issue/{identifier}")),
        state_type: Some(state.0.to_string()),
        state_name: Some(state.1.to_string()),
        project_name: Some("Checkout revamp".to_string()),
        project_url: Some("https://linear.app/acme/project/checkout".to_string()),
        pull_request_urls: Vec::new(),
    }
}

pub(super) fn list_pull_request(
    number: u64,
    repo: &str,
    state: PullRequestState,
    head_branch: &str,
    body: &str,
    updated_at: &str,
) -> GithubListPullRequest {
    GithubListPullRequest {
        number,
        title: format!("PR {number}"),
        url: Some(format!("https://github.com/{repo}/pull/{number}")),
        state,
        author: Some(VIEWER_LOGIN.to_string()),
        assignees: Vec::new(),
        updated_at: Some(updated_at.to_string()),
        head_branch: Some(head_branch.to_string()),
        checks: Some("passing"),
        review_decision: Some("REVIEW_REQUIRED".to_string()),
        body: body.to_string(),
        project: None,
    }
}

pub(super) fn list_issue(
    number: u64,
    repo: &str,
    assignee: &str,
    project: Option<GithubListProject>,
    updated_at: &str,
) -> GithubListIssue {
    GithubListIssue {
        number,
        title: format!("Issue {number}"),
        url: Some(format!("https://github.com/{repo}/issues/{number}")),
        author: Some("someone".to_string()),
        assignees: vec![assignee.to_string()],
        updated_at: Some(updated_at.to_string()),
        labels: vec!["bug".to_string()],
        project,
    }
}

pub(super) fn work_project(
    project: Value,
    sessions: Vec<Value>,
    linear_api_key: Option<&str>,
    repo: &str,
    tracker: WorkSourceId,
) -> WorkProjectInput {
    let project_id = project["projectId"].as_str().unwrap().to_string();
    WorkProjectInput {
        credentials: Arc::new(WorkCredentials::with_project_linear_key(
            &project_id,
            linear_api_key,
        )),
        project_id,
        name: project["name"].as_str().unwrap().to_string(),
        path: project["path"].as_str().map(str::to_string),
        project,
        sessions,
        repo: Some(repo.to_string()),
        workspace_id: crate::workspaces::DEFAULT_WORKSPACE_ID.to_string(),
        tracker,
    }
}

/// Whether any of the plan's Work list feeds is missing or stale.
pub(super) fn work_feeds_stale_for(plan: &WorkFeedPlan) -> bool {
    work_feeds_stale(plan)
}

/// The list without what depends on the clock (`generatedAt`).
pub(super) fn stable_list(mut list: Value) -> Value {
    let object = list.as_object_mut().unwrap();
    object.remove("generatedAt");
    list
}

use std::sync::Arc;

use super::*;
