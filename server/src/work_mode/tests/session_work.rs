//! `PresentationSession.work` and `PresentationSession.links` for a Linear and a GitHub session.

use serde_json::{json, Value};

use crate::session_git_status::{
    set_cached_session_git_status_for_test, SessionGitStatus, SessionPullRequest,
};
use crate::session_links::presentation_session_links;

use super::*;

/// A Linear workspace session: `branch` names its tickets by the team keys Linear answered;
/// `manual` sets them by hand instead.
fn linear_session(session_id: &str, cwd: &str, branch: &str, manual: Value) -> (Value, Value) {
    let project_id = format!("PCharLin{session_id}");
    let fingerprint = linear_key_fingerprint(&format!("lin-key-{session_id}"));
    remember_project_linear_key(&project_id, Some(fingerprint));
    remember_project_work_tracker(&project_id, WorkSourceId::Linear);
    seed_linear_team_keys(fingerprint, &["ACM"]);
    seed_linear_issue(
        fingerprint,
        "ACM-12",
        Some(linear_issue_info("ACM-12", ("started", "In Progress"))),
    );
    seed_linear_issue(fingerprint, "ACM-13", None);
    set_branch(cwd, branch);
    seed_work_github(
        cwd,
        Some("acme/app"),
        vec![WorkGithubIssueInfo {
            number: 3,
            title: Some("Crash on start".to_string()),
            state: Some("open".to_string()),
            url: Some("https://github.com/acme/app/issues/3".to_string()),
        }],
        Vec::new(),
    );
    (
        project(&project_id, cwd),
        session(&project_id, session_id, manual),
    )
}

#[test]
fn a_linear_branch_links_its_ticket_and_the_card_shows_its_linear_project() {
    let (project, session) = linear_session(
        "Branch",
        "/gx-char/session/linear-branch",
        "leandro/acm-12-checkout",
        Value::Null,
    );
    assert_published(
        &presentation_session_work(&project, &session).unwrap(),
        json!({
          "branch": "leandro/acm-12-checkout",
          "linearIssues": [
            {
              "identifier": "ACM-12",
              "title": "Title of ACM-12",
              "stateType": "started",
              "stateName": "In Progress",
              "url": "https://linear.app/acme/issue/ACM-12"
            }
          ],
          "tickets": [
            {
              "key": "linear:ACM-12",
              "kind": "linear",
              "label": "ACM-12",
              "title": "Title of ACM-12",
              "state": "started",
              "stateName": "In Progress",
              "stateGroup": "progress",
              "url": "https://linear.app/acme/issue/ACM-12",
              "source": "branch",
              "sourceLabel": "From branch"
            }
          ],
          "linearProject": {
            "name": "Checkout revamp",
            "url": "https://linear.app/acme/project/checkout"
          }
        }),
    );
    assert_published(
        &presentation_session_links(&project, &session).unwrap(),
        json!({
          "items": [
            {
              "key": "linear:ACM-12",
              "kind": "linear",
              "label": "ACM-12",
              "source": "branch",
              "sourceLabel": "From branch",
              "identifier": "ACM-12",
              "state": "started",
              "stateName": "In Progress",
              "title": "Title of ACM-12",
              "url": "https://linear.app/acme/issue/ACM-12"
            }
          ]
        }),
    );
}

#[test]
fn hand_set_linear_and_github_links_publish_what_is_known_about_each() {
    let (project, session) = linear_session(
        "Manual",
        "/gx-char/session/linear-manual",
        "main",
        json!({ "linearIssues": ["ACM-12", "acm-13", "ACM-14"], "githubIssues": [3] }),
    );
    assert_published(
        &presentation_session_work(&project, &session).unwrap(),
        json!({
          "handSet": true,
          "linearIssues": [
            {
              "identifier": "ACM-12",
              "title": "Title of ACM-12",
              "stateType": "started",
              "stateName": "In Progress",
              "url": "https://linear.app/acme/issue/ACM-12"
            },
            {
              "identifier": "ACM-13",
              "stateType": "notFound",
              "stateName": "Not found"
            },
            {
              "identifier": "ACM-14"
            }
          ],
          "githubIssues": [
            {
              "number": 3,
              "title": "Crash on start",
              "state": "open",
              "url": "https://github.com/acme/app/issues/3"
            }
          ],
          "tickets": [
            {
              "key": "linear:ACM-12",
              "kind": "linear",
              "label": "ACM-12",
              "title": "Title of ACM-12",
              "state": "started",
              "stateName": "In Progress",
              "stateGroup": "progress",
              "url": "https://linear.app/acme/issue/ACM-12",
              "source": "user",
              "sourceLabel": "You added"
            },
            {
              "key": "linear:ACM-13",
              "kind": "linear",
              "label": "ACM-13",
              "state": "notFound",
              "stateName": "Not found",
              "finished": true,
              "source": "user",
              "sourceLabel": "You added"
            },
            {
              "key": "linear:ACM-14",
              "kind": "linear",
              "label": "ACM-14",
              "loading": true,
              "source": "user",
              "sourceLabel": "You added"
            },
            {
              "key": "issue:acme/app#3",
              "kind": "issue",
              "label": "#3",
              "title": "Crash on start",
              "state": "open",
              "stateName": "Open",
              "stateGroup": "todo",
              "url": "https://github.com/acme/app/issues/3",
              "source": "user",
              "sourceLabel": "You added"
            }
          ],
          "linearProject": {
            "name": "Checkout revamp",
            "url": "https://linear.app/acme/project/checkout"
          }
        }),
    );
    assert_published(
        &presentation_session_links(&project, &session).unwrap(),
        json!({
          "items": [
            {
              "key": "issue:acme/app#3",
              "kind": "issue",
              "label": "#3",
              "source": "user",
              "sourceLabel": "You added",
              "number": 3,
              "state": "open",
              "title": "Crash on start",
              "url": "https://github.com/acme/app/issues/3"
            },
            {
              "key": "linear:ACM-12",
              "kind": "linear",
              "label": "ACM-12",
              "source": "user",
              "sourceLabel": "You added",
              "identifier": "ACM-12",
              "state": "started",
              "stateName": "In Progress",
              "title": "Title of ACM-12",
              "url": "https://linear.app/acme/issue/ACM-12"
            },
            {
              "key": "linear:ACM-14",
              "kind": "linear",
              "label": "ACM-14",
              "source": "user",
              "sourceLabel": "You added",
              "identifier": "ACM-14",
              "loading": true
            },
            {
              "key": "linear:ACM-13",
              "kind": "linear",
              "label": "ACM-13",
              "source": "user",
              "sourceLabel": "You added",
              "identifier": "ACM-13",
              "state": "notFound",
              "stateName": "Not found",
              "finished": true
            }
          ]
        }),
    );
}

#[test]
fn a_github_branch_links_its_issue_and_no_linear_project() {
    let cwd = "/gx-char/session/github-branch";
    let project_id = "PCharGhBranch";
    // A remembered Linear key turns Issues Workflow on without asking whether `gh` is signed in.
    remember_project_linear_key(
        project_id,
        Some(linear_key_fingerprint("lin-key-gh-branch")),
    );
    remember_project_work_tracker(project_id, WorkSourceId::Github);
    set_branch(cwd, "leandro/7-dark-mode");
    seed_work_github(
        cwd,
        Some("acme/web"),
        vec![WorkGithubIssueInfo {
            number: 7,
            title: Some("Dark mode".to_string()),
            state: Some("closed".to_string()),
            url: Some("https://github.com/acme/web/issues/7".to_string()),
        }],
        Vec::new(),
    );
    let project = project(project_id, cwd);
    let session = session(
        project_id,
        "S7",
        json!({ "linearProject": "Ignored for GitHub" }),
    );
    assert_published(
        &presentation_session_work(&project, &session).unwrap(),
        json!({
          "branch": "leandro/7-dark-mode",
          "handSet": true,
          "githubIssues": [
            {
              "number": 7,
              "title": "Dark mode",
              "state": "closed",
              "url": "https://github.com/acme/web/issues/7"
            }
          ],
          "tickets": [
            {
              "key": "issue:acme/web#7",
              "kind": "issue",
              "label": "#7",
              "title": "Dark mode",
              "state": "closed",
              "stateName": "Closed",
              "stateGroup": "closed",
              "url": "https://github.com/acme/web/issues/7",
              "finished": true,
              "source": "branch",
              "sourceLabel": "From branch"
            }
          ]
        }),
    );
    assert_published(
        &presentation_session_links(&project, &session).unwrap(),
        json!({
          "items": [
            {
              "key": "issue:acme/web#7",
              "kind": "issue",
              "label": "#7",
              "source": "branch",
              "sourceLabel": "From branch",
              "number": 7,
              "state": "closed",
              "title": "Dark mode",
              "url": "https://github.com/acme/web/issues/7",
              "finished": true
            }
          ]
        }),
    );
}

#[test]
fn a_hand_set_github_issue_nobody_has_read_yet_publishes_its_number_and_a_loading_link() {
    let cwd = "/gx-char/session/github-unread";
    let project_id = "PCharGhUnread";
    remember_project_work_tracker(project_id, WorkSourceId::Github);
    set_branch(cwd, "main");
    // The repo is known, but the issue was never read.
    seed_work_github(cwd, Some("acme/web"), Vec::new(), Vec::new());
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let project = project(project_id, cwd);
    let session = session(project_id, "S9", json!({ "githubIssues": [9] }));
    assert_published(
        &presentation_session_work(&project, &session).unwrap(),
        json!({
          "handSet": true,
          "githubIssues": [{ "number": 9 }],
          "tickets": [
            {
              "key": "issue:acme/web#9",
              "kind": "issue",
              "label": "#9",
              "loading": true,
              "source": "user",
              "sourceLabel": "You added"
            }
          ]
        }),
    );
    assert_published(
        &presentation_session_links(&project, &session).unwrap(),
        json!({
          "items": [
            {
              "key": "issue:acme/web#9",
              "kind": "issue",
              "label": "#9",
              "source": "user",
              "sourceLabel": "You added",
              "number": 9,
              "url": "https://github.com/acme/web/issues/9",
              "loading": true
            }
          ]
        }),
    );
}

#[test]
fn a_branch_with_a_pr_shows_the_pr_in_the_sessions_work_and_links() {
    let cwd = "/gx-char/session/github-pr";
    let project_id = "PCharGhPr";
    let pull_request_url = "https://github.com/acme/web/pull/12";
    // A remembered Linear key turns Issues Workflow on without asking whether `gh` is signed in.
    remember_project_linear_key(project_id, Some(linear_key_fingerprint("lin-key-gh-pr")));
    remember_project_work_tracker(project_id, WorkSourceId::Github);
    set_cached_session_git_status_for_test(
        cwd,
        Some(SessionGitStatus {
            branch: Some("leandro/7-dark-mode".to_string()),
            additions: 0,
            deletions: 0,
            pull_request: Some(SessionPullRequest {
                number: 12,
                state: PullRequestState::Open,
                url: Some(pull_request_url.to_string()),
            }),
            updated_at: "2026-10-27T10:00:00.000Z".to_string(),
        }),
    );
    seed_work_github(
        cwd,
        Some("acme/web"),
        vec![WorkGithubIssueInfo {
            number: 7,
            title: Some("Dark mode".to_string()),
            state: Some("open".to_string()),
            url: Some("https://github.com/acme/web/issues/7".to_string()),
        }],
        vec![WorkPullRequestInfo {
            number: 12,
            state: PullRequestState::Open,
            url: Some(pull_request_url.to_string()),
            checks: Some("passing"),
            title: Some("Dark mode".to_string()),
            review: None,
            closing_issues: vec!["https://github.com/acme/web/issues/7".to_string()],
            head_ref: Some("leandro/7-dark-mode".to_string()),
        }],
    );
    let project = project(project_id, cwd);
    let session = session(project_id, "S12", json!({}));
    assert_published(
        &presentation_session_work(&project, &session).unwrap(),
        json!({
          "branch": "leandro/7-dark-mode",
          "pullRequest": {
            "number": 12,
            "state": "open",
            "url": "https://github.com/acme/web/pull/12",
            "checks": "passing"
          },
          "githubIssues": [
            {
              "number": 7,
              "title": "Dark mode",
              "state": "open",
              "url": "https://github.com/acme/web/issues/7"
            }
          ],
          "tickets": [
            {
              "key": "issue:acme/web#7",
              "kind": "issue",
              "label": "#7",
              "title": "Dark mode",
              "state": "open",
              "stateName": "Open",
              "stateGroup": "todo",
              "url": "https://github.com/acme/web/issues/7",
              "source": "branch",
              "sourceLabel": "From branch"
            }
          ]
        }),
    );
    assert_published(
        &presentation_session_links(&project, &session).unwrap(),
        json!({
          "items": [
            {
              "key": "pr:acme/web#12",
              "kind": "pullRequest",
              "label": "#12",
              "source": "branch",
              "sourceLabel": "From branch",
              "number": 12,
              "state": "open",
              "checks": "passing",
              "title": "Dark mode",
              "branch": "leandro/7-dark-mode",
              "url": "https://github.com/acme/web/pull/12"
            },
            {
              "key": "issue:acme/web#7",
              "kind": "issue",
              "label": "#7",
              "source": "branch",
              "sourceLabel": "From branch",
              "number": 7,
              "state": "open",
              "title": "Dark mode",
              "url": "https://github.com/acme/web/issues/7"
            }
          ]
        }),
    );
}

#[test]
fn a_session_without_a_pr_of_its_own_takes_its_tickets_pr_in_its_repo() {
    let (project, session) = linear_session(
        "TicketPr",
        "/gx-char/session/linear-ticket-pr",
        "leandro/acm-12-checkout",
        Value::Null,
    );
    // Linear attached PRs from two repos; the session works in acme/app.
    let mut issue = linear_issue_info("ACM-12", ("started", "In Progress"));
    issue.pull_request_urls = vec![
        "https://github.com/acme/other/pull/3".to_string(),
        "https://github.com/acme/app/pull/7".to_string(),
    ];
    seed_linear_issue(
        linear_key_fingerprint("lin-key-TicketPr"),
        "ACM-12",
        Some(issue),
    );
    assert_eq!(
        work_targets(&project, &session).pull_request.as_deref(),
        Some("https://github.com/acme/app/pull/7")
    );
}
