//! `/api/listWorkItems`: the rows a Linear and a GitHub workspace list, with their PRs folded in.

use serde_json::{json, Value};

use super::*;

fn linear_workspace_list() -> Value {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let cwd = "/gx-char/list/linear-app";
    let api_key = "lin-key-work-list";
    let project = project("PCharLinList", cwd);
    let sessions = vec![session(
        "PCharLinList",
        "S1",
        json!({ "linearIssues": ["acm-7"], "pullRequest": 41 }),
    )];
    let input = work_project(
        project,
        sessions,
        Some(api_key),
        "acme/app",
        WorkSourceId::Linear,
    );
    let projects = vec![input];
    let plan = work_feed_plan(&projects);

    let mut in_review = linear_node(
        "ACM-1",
        ("started", "In Review"),
        "2026-10-20T09:00:00.000Z",
    );
    in_review["assignee"] = json!({ "name": "leandro", "displayName": "Leandro", "avatarUrl": "https://avatars/l.png", "isMe": true });
    in_review["project"] =
        json!({ "name": "Checkout revamp", "url": "https://linear.app/acme/project/checkout" });
    in_review["cycle"] = json!({ "name": null, "number": 12 });
    in_review["labels"] = json!({ "nodes": [{ "name": "Bug" }, { "name": "Web" }] });
    in_review["attachments"] =
        json!({ "nodes": [{ "url": "https://github.com/acme/app/pull/40" }] });
    let backlog = linear_node("ACM-2", ("backlog", "Backlog"), "2026-10-21T09:00:00.000Z");
    let feed = LinearFeed {
        issues: [in_review, backlog]
            .iter()
            .filter_map(parse_linear_list_issue)
            .collect(),
    };
    seed_linear_issue(
        linear_key_fingerprint(api_key),
        "ACM-7",
        Some(linear_issue_info("ACM-7", ("unstarted", "Todo"))),
    );
    let github = GithubFeed {
        repo: Some("acme/app".to_string()),
        pull_requests: vec![
            list_pull_request(
                40,
                "acme/app",
                PullRequestState::Open,
                "acm-1-review",
                "",
                "2026-10-22T09:00:00.000Z",
            ),
            list_pull_request(
                42,
                "acme/app",
                PullRequestState::Draft,
                "acm-2-retry",
                "",
                "2026-10-19T09:00:00.000Z",
            ),
            list_pull_request(
                43,
                "acme/app",
                PullRequestState::Open,
                "chore/bump",
                "",
                "2026-10-18T09:00:00.000Z",
            ),
        ],
        // A Linear workspace lists its repo's PRs but not its GitHub issues.
        issues: vec![list_issue(
            9,
            "acme/app",
            VIEWER_LOGIN,
            None,
            "2026-10-23T09:00:00.000Z",
        )],
    };
    seed_work_feeds(
        &[(&plan.linear[0], feed)],
        &[(cwd, github)],
        Some(VIEWER_LOGIN),
    );
    seed_work_github(
        cwd,
        Some("acme/app"),
        Vec::new(),
        vec![WorkPullRequestInfo {
            number: 41,
            state: PullRequestState::Open,
            url: Some("https://github.com/acme/app/pull/41".to_string()),
            checks: Some("failing"),
            title: Some("PR 41".to_string()),
            review: None,
            closing_issues: Vec::new(),
            head_ref: Some("leandro/acm-7-work".to_string()),
        }],
    );
    stable_list(build_work_list(&projects, &plan))
}

fn github_workspace_list() -> Value {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let cwd = "/gx-char/list/github-app";
    let project = project("PCharGhList", cwd);
    let sessions = vec![session("PCharGhList", "S3", json!({ "githubIssues": [5] }))];
    let input = work_project(project, sessions, None, "acme/web", WorkSourceId::Github);
    let projects = vec![input];
    let plan = work_feed_plan(&projects);
    let roadmap = GithubListProject {
        title: "Roadmap".to_string(),
        status: Some("In progress".to_string()),
        url: Some("https://github.com/orgs/acme/projects/3".to_string()),
    };
    let github = GithubFeed {
        repo: Some("acme/web".to_string()),
        pull_requests: vec![
            list_pull_request(
                7,
                "acme/web",
                PullRequestState::Open,
                "fix-thing",
                "Fixes #6",
                "2026-10-24T09:00:00.000Z",
            ),
            list_pull_request(
                8,
                "acme/web",
                PullRequestState::Draft,
                "5-speedup",
                "",
                "2026-10-19T09:00:00.000Z",
            ),
            list_pull_request(
                10,
                "acme/web",
                PullRequestState::Open,
                "chore/deps",
                "",
                "2026-10-17T09:00:00.000Z",
            ),
        ],
        issues: vec![
            list_issue(
                5,
                "acme/web",
                VIEWER_LOGIN,
                Some(roadmap),
                "2026-10-20T09:00:00.000Z",
            ),
            list_issue(6, "acme/web", "bob", None, "2026-10-21T09:00:00.000Z"),
        ],
    };
    seed_work_feeds(&[], &[(cwd, github)], Some(VIEWER_LOGIN));
    stable_list(build_work_list(&projects, &plan))
}

#[test]
fn a_linear_workspace_lists_its_tickets_with_their_prs_folded_in() {
    assert_published(
        &linear_workspace_list(),
        json!({
          "tracker": "linear",
          "ghAvailable": true,
          "items": [
            {
              "key": "linear:ACM-1",
              "kind": "ticket",
              "source": "linear",
              "id": "ACM-1",
              "title": "Title of ACM-1",
              "url": "https://linear.app/acme/issue/ACM-1",
              "updatedAt": "2026-10-22T09:00:00.000Z",
              "status": {
                "group": "review",
                "name": "In Review"
              },
              "projectId": "PCharLinList",
              "projectName": "PCharLinList app",
              "linearProject": {
                "name": "Checkout revamp",
                "url": "https://linear.app/acme/project/checkout"
              },
              "cycle": "Cycle 12",
              "labels": [
                "Bug",
                "Web"
              ],
              "assignee": {
                "name": "Leandro",
                "avatarUrl": "https://avatars/l.png",
                "isMe": true
              },
              "assignedToMe": true,
              "pullRequest": {
                "number": 40,
                "url": "https://github.com/acme/app/pull/40",
                "state": "open",
                "checks": "passing",
                "reviewDecision": "REVIEW_REQUIRED",
                "title": "PR 40"
              },
              "noTicket": false,
              "branchName": "leandro/acm-1-work",
              "sessions": [],
              "ticketId": "ACM-1"
            },
            {
              "key": "linear:ACM-2",
              "kind": "ticket",
              "source": "linear",
              "id": "ACM-2",
              "title": "Title of ACM-2",
              "url": "https://linear.app/acme/issue/ACM-2",
              "updatedAt": "2026-10-21T09:00:00.000Z",
              "status": {
                "group": "backlog",
                "name": "Backlog"
              },
              "projectId": "PCharLinList",
              "projectName": "PCharLinList app",
              "labels": [],
              "assignedToMe": true,
              "pullRequest": {
                "number": 42,
                "url": "https://github.com/acme/app/pull/42",
                "state": "draft",
                "checks": "passing",
                "reviewDecision": "REVIEW_REQUIRED",
                "title": "PR 42"
              },
              "noTicket": false,
              "branchName": "leandro/acm-2-work",
              "sessions": [],
              "ticketId": "ACM-2"
            },
            {
              "key": "pr:acme/app#43",
              "kind": "pullRequest",
              "id": "#43",
              "title": "PR 43",
              "url": "https://github.com/acme/app/pull/43",
              "updatedAt": "2026-10-18T09:00:00.000Z",
              "status": {
                "group": "open",
                "name": "Open"
              },
              "projectId": "PCharLinList",
              "projectName": "PCharLinList app",
              "labels": [],
              "assignee": {
                "name": "octo-me",
                "isMe": true
              },
              "assignedToMe": true,
              "pullRequest": {
                "number": 43,
                "url": "https://github.com/acme/app/pull/43",
                "state": "open",
                "checks": "passing",
                "reviewDecision": "REVIEW_REQUIRED",
                "title": "PR 43"
              },
              "noTicket": true,
              "branchName": "chore/bump",
              "sessions": [],
              "pullRequestRef": "https://github.com/acme/app/pull/43"
            },
            {
              "key": "linear:ACM-7",
              "kind": "ticket",
              "source": "linear",
              "id": "ACM-7",
              "title": "Title of ACM-7",
              "url": "https://linear.app/acme/issue/ACM-7",
              "status": {
                "group": "todo",
                "name": "Todo"
              },
              "projectId": "PCharLinList",
              "projectName": "PCharLinList app",
              "linearProject": {
                "name": "Checkout revamp",
                "url": "https://linear.app/acme/project/checkout"
              },
              "labels": [],
              "assignedToMe": false,
              "pullRequest": {
                "number": 41,
                "url": "https://github.com/acme/app/pull/41",
                "state": "open",
                "checks": "failing"
              },
              "noTicket": false,
              "sessions": [
                {
                  "projectId": "PCharLinList",
                  "sessionId": "S1",
                  "title": "Session S1",
                  "working": false,
                  "lifecycle": "running",
                  "agentId": "claude"
                }
              ],
              "ticketId": "ACM-7"
            }
          ],
          "projects": [
            {
              "projectId": "PCharLinList",
              "name": "PCharLinList app",
              "repo": "acme/app"
            }
          ],
          "viewer": {
            "githubLogin": "octo-me"
          },
          "linearConfigured": true,
          "errors": []
        }),
    );
}

#[test]
fn a_github_workspace_lists_its_issues_with_their_prs_folded_in() {
    assert_published(
        &github_workspace_list(),
        json!({
          "tracker": "github",
          "ghAvailable": true,
          "items": [
            {
              "key": "issue:acme/web#6",
              "kind": "ticket",
              "source": "github",
              "id": "#6",
              "title": "Issue 6",
              "url": "https://github.com/acme/web/issues/6",
              "updatedAt": "2026-10-24T09:00:00.000Z",
              "status": {
                "group": "review",
                "name": "Open"
              },
              "projectId": "PCharGhList",
              "projectName": "PCharGhList app",
              "labels": [
                "bug"
              ],
              "assignee": {
                "name": "bob",
                "isMe": false
              },
              "assignedToMe": false,
              "pullRequest": {
                "number": 7,
                "url": "https://github.com/acme/web/pull/7",
                "state": "open",
                "checks": "passing",
                "reviewDecision": "REVIEW_REQUIRED",
                "title": "PR 7"
              },
              "noTicket": false,
              "sessions": [],
              "ticketId": "6"
            },
            {
              "key": "issue:acme/web#5",
              "kind": "ticket",
              "source": "github",
              "id": "#5",
              "title": "Issue 5",
              "url": "https://github.com/acme/web/issues/5",
              "updatedAt": "2026-10-20T09:00:00.000Z",
              // The Roadmap status is "In progress", but folding PR 8 in replaces it with "Open"; a fix is expected to change this assertion.
              "status": {
                "group": "progress",
                "name": "Open"
              },
              "projectId": "PCharGhList",
              "projectName": "PCharGhList app",
              "githubProject": {
                "name": "Roadmap",
                "url": "https://github.com/orgs/acme/projects/3"
              },
              "labels": [
                "bug"
              ],
              "assignee": {
                "name": "octo-me",
                "isMe": true
              },
              "assignedToMe": true,
              "pullRequest": {
                "number": 8,
                "url": "https://github.com/acme/web/pull/8",
                "state": "draft",
                "checks": "passing",
                "reviewDecision": "REVIEW_REQUIRED",
                "title": "PR 8"
              },
              "noTicket": false,
              "sessions": [
                {
                  "projectId": "PCharGhList",
                  "sessionId": "S3",
                  "title": "Session S3",
                  "working": false,
                  "lifecycle": "running",
                  "agentId": "claude"
                }
              ],
              "ticketId": "5"
            },
            {
              "key": "pr:acme/web#10",
              "kind": "pullRequest",
              "id": "#10",
              "title": "PR 10",
              "url": "https://github.com/acme/web/pull/10",
              "updatedAt": "2026-10-17T09:00:00.000Z",
              "status": {
                "group": "open",
                "name": "Open"
              },
              "projectId": "PCharGhList",
              "projectName": "PCharGhList app",
              "labels": [],
              "assignee": {
                "name": "octo-me",
                "isMe": true
              },
              "assignedToMe": true,
              "pullRequest": {
                "number": 10,
                "url": "https://github.com/acme/web/pull/10",
                "state": "open",
                "checks": "passing",
                "reviewDecision": "REVIEW_REQUIRED",
                "title": "PR 10"
              },
              "noTicket": true,
              "branchName": "chore/deps",
              "sessions": [],
              "pullRequestRef": "https://github.com/acme/web/pull/10"
            }
          ],
          "projects": [
            {
              "projectId": "PCharGhList",
              "name": "PCharGhList app",
              "repo": "acme/web"
            }
          ],
          "viewer": {
            "githubLogin": "octo-me"
          },
          "linearConfigured": true,
          "errors": []
        }),
    );
}
