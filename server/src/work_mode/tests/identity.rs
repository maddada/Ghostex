//! What a ticket is called and which tickets a PR names: the keys saved links, pins and removals
//! are stored by, the link kinds clients draw, and which ticket row a PR folds into when its branch
//! and body name tickets of more than one source.

use serde_json::{json, Value};

use super::*;

#[test]
fn ticket_keys_labels_and_link_kinds_stay_what_saved_links_hold() {
    let linear = TicketRef::new(WorkSourceId::Linear, "ACM-1");
    let github = TicketRef::new(WorkSourceId::Github, "12");
    assert_eq!(linear.key("acme/app"), "linear:ACM-1");
    assert_eq!(linear.label(), "ACM-1");
    assert_eq!(linear.untitled(), "ACM-1");
    assert_eq!(github.key("acme/app"), "issue:acme/app#12");
    assert_eq!(github.label(), "#12");
    assert_eq!(github.untitled(), "Issue #12");
    assert_eq!(WorkSourceId::Linear.as_wire(), "linear");
    assert_eq!(WorkSourceId::Github.as_wire(), "github");

    assert_eq!(
        ticket_link_value(&linear, "acme/app", &TicketRead::Unread),
        json!({ "key": "linear:ACM-1", "kind": "linear", "label": "ACM-1", "loading": true })
    );
    assert_eq!(
        ticket_link_value(&github, "acme/app", &TicketRead::NotFound),
        json!({
            "key": "issue:acme/app#12",
            "kind": "issue",
            "label": "#12",
            "state": "notFound",
            "stateName": "Not found",
            "finished": true,
        })
    );
}

/// Each source's known ticket-id prefixes: `linear` for Linear, none for GitHub.
fn known_keys(linear: &[&str]) -> Vec<(WorkSourceId, Vec<String>)> {
    vec![
        (
            WorkSourceId::Linear,
            linear.iter().map(|key| key.to_string()).collect(),
        ),
        (WorkSourceId::Github, Vec::new()),
    ]
}

#[test]
fn a_pr_branch_names_linear_tickets_by_known_keys_and_a_github_issue_by_its_number() {
    let ticket = |source, id: &str| WorkItemRef::Ticket(TicketRef::new(source, id));
    // The details page reads no body here: a "Closes #13" does not name #13.
    let pull_request = json!({ "headBranch": "leandro/12-acm-1-fix", "body": "Closes #13" });
    assert_eq!(
        pull_request_branch_tickets(&pull_request, &known_keys(&["ACM"])),
        vec![
            ticket(WorkSourceId::Linear, "ACM-1"),
            ticket(WorkSourceId::Github, "12"),
        ]
    );
    assert_eq!(
        pull_request_branch_tickets(&pull_request, &known_keys(&[])),
        vec![ticket(WorkSourceId::Github, "12")]
    );
    assert_eq!(
        pull_request_branch_tickets(&json!({ "body": "Closes #13" }), &known_keys(&["ACM"])),
        Vec::<WorkItemRef>::new()
    );
}

#[test]
fn a_pr_branch_matches_each_source_only_by_its_own_known_keys() {
    let pull_request = json!({ "headBranch": "leandro/acm-1-fix" });
    let ticket = |source, id: &str| WorkItemRef::Ticket(TicketRef::new(source, id));
    assert_eq!(
        pull_request_branch_tickets(&pull_request, &known_keys(&["ACM"])),
        vec![ticket(WorkSourceId::Linear, "ACM-1")]
    );
    // `ACM` known to another source names no Linear ticket.
    assert_eq!(
        pull_request_branch_tickets(
            &pull_request,
            &[
                (WorkSourceId::Linear, Vec::new()),
                (WorkSourceId::Github, vec!["ACM".to_string()]),
            ]
        ),
        Vec::<WorkItemRef>::new()
    );
}

/// Each row's key, the PR folded into it and its status group.
fn folded(list: &Value) -> Vec<Value> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            json!([
                item["key"],
                item.pointer("/pullRequest/number")
                    .cloned()
                    .unwrap_or(Value::Null),
                item["status"]["group"],
            ])
        })
        .collect()
}

/// A repo whose open PR 50 is on `leandro/12-acm-1-fix` and says "Closes #12", so its branch names
/// Linear's ACM-1 and GitHub's #12, and its body names #12 again.
fn fold_list(
    name: &str,
    tracker: WorkSourceId,
    linear_api_key: Option<&str>,
    session_links: Value,
) -> Value {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let cwd = format!("/gx-char/identity/{name}");
    let repo = format!("acme/{name}");
    let project_id = format!("PIdentity{name}");
    let input = work_project(
        project(&project_id, &cwd),
        vec![session(&project_id, "S1", session_links)],
        linear_api_key,
        &repo,
        tracker,
    );
    let projects = vec![input];
    let plan = work_feed_plan(&projects);
    let github = GithubFeed {
        repo: Some(repo.clone()),
        pull_requests: vec![list_pull_request(
            50,
            &repo,
            PullRequestState::Open,
            "leandro/12-acm-1-fix",
            "Closes #12",
            "2026-10-25T09:00:00.000Z",
        )],
        issues: vec![list_issue(
            12,
            &repo,
            VIEWER_LOGIN,
            None,
            "2026-10-20T09:00:00.000Z",
        )],
    };
    let linear: Vec<(&LinearFeedRequest, LinearFeed)> = plan
        .linear
        .iter()
        .map(|request| {
            let issues = [
                linear_node("ACM-1", ("unstarted", "Todo"), "2026-10-21T09:00:00.000Z"),
                linear_node("ACM-2", ("unstarted", "Todo"), "2026-10-22T09:00:00.000Z"),
            ];
            (
                request,
                LinearFeed {
                    issues: issues.iter().filter_map(parse_linear_list_issue).collect(),
                },
            )
        })
        .collect();
    seed_work_feeds(&linear, &[(cwd.as_str(), github)], Some(VIEWER_LOGIN));
    stable_list(build_work_list(&projects, &plan))
}

#[test]
fn a_pr_folds_into_the_linear_ticket_its_branch_names_in_a_linear_workspace() {
    let list = fold_list(
        "linear",
        WorkSourceId::Linear,
        Some("lin-key-identity-linear"),
        Value::Null,
    );
    assert_eq!(
        folded(&list),
        vec![
            json!(["linear:ACM-1", 50, "todo"]),
            json!(["linear:ACM-2", null, "todo"]),
        ]
    );
}

#[test]
fn a_pr_folds_into_the_issue_its_branch_names_in_a_github_workspace() {
    let list = fold_list("github", WorkSourceId::Github, None, Value::Null);
    assert_eq!(
        folded(&list),
        vec![json!(["issue:acme/github#12", 50, "review"])]
    );
}

#[test]
fn a_session_linking_a_pr_and_a_ticket_folds_the_pr_into_that_ticket_first() {
    let list = fold_list(
        "session",
        WorkSourceId::Linear,
        Some("lin-key-identity-session"),
        json!({ "linearIssues": ["ACM-2"], "pullRequest": 50 }),
    );
    assert_eq!(
        folded(&list),
        vec![
            json!(["linear:ACM-2", 50, "todo"]),
            json!(["linear:ACM-1", null, "todo"]),
        ]
    );
}

/// A window showing a Linear workspace's project and a GitHub workspace's project lists both
/// sources' rows at once. PR 50 in the GitHub project's repo is on `leandro/12-acm-1-fix`, so it
/// names the Linear project's ACM-1 and the GitHub project's #12, and only one row can take it.
#[test]
fn a_pr_naming_tickets_of_both_listed_sources_folds_into_the_linear_one() {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let api_key = "lin-key-identity-both";
    let linear_cwd = "/gx-char/identity/both-linear";
    let github_cwd = "/gx-char/identity/both-github";
    let projects = vec![
        work_project(
            project("PIdentityBothLinear", linear_cwd),
            Vec::new(),
            Some(api_key),
            "acme/both-linear",
            WorkSourceId::Linear,
        ),
        work_project(
            project("PIdentityBothGithub", github_cwd),
            Vec::new(),
            None,
            "acme/both-github",
            WorkSourceId::Github,
        ),
    ];
    let plan = work_feed_plan(&projects);
    let linear = LinearFeed {
        issues: [linear_node(
            "ACM-1",
            ("unstarted", "Todo"),
            "2026-10-21T09:00:00.000Z",
        )]
        .iter()
        .filter_map(parse_linear_list_issue)
        .collect(),
    };
    let linear_repo = GithubFeed {
        repo: Some("acme/both-linear".to_string()),
        pull_requests: Vec::new(),
        issues: Vec::new(),
    };
    let github_repo = GithubFeed {
        repo: Some("acme/both-github".to_string()),
        pull_requests: vec![list_pull_request(
            50,
            "acme/both-github",
            PullRequestState::Open,
            "leandro/12-acm-1-fix",
            "",
            "2026-10-25T09:00:00.000Z",
        )],
        issues: vec![list_issue(
            12,
            "acme/both-github",
            VIEWER_LOGIN,
            None,
            "2026-10-20T09:00:00.000Z",
        )],
    };
    seed_work_feeds(
        &[(&plan.linear[0], linear)],
        &[(linear_cwd, linear_repo), (github_cwd, github_repo)],
        Some(VIEWER_LOGIN),
    );
    let list = stable_list(build_work_list(&projects, &plan));
    assert_eq!(
        folded(&list),
        vec![
            json!(["linear:ACM-1", 50, "todo"]),
            json!(["issue:acme/both-github#12", null, "todo"]),
        ]
    );
}
