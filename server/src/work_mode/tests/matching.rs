//! How a branch, a PR body and a request name a ticket.

use serde_json::{json, Map, Value};

use super::*;

fn params(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn a_branch_names_linear_tickets_only_by_real_team_keys() {
    let teams = vec!["ACM".to_string(), "Web".to_string()];
    assert_eq!(
        linear_identifiers_in_branch("leandro/acm-12-and-web-3_ACM-12", &teams),
        vec!["ACM-12".to_string(), "WEB-3".to_string()]
    );
    assert!(linear_identifiers_in_branch("fix-utf-8-paste", &teams).is_empty());
    assert!(linear_identifiers_in_branch("spx-1245-x", &teams).is_empty());
}

#[test]
fn a_branch_names_a_github_issue_by_its_last_segment() {
    assert_eq!(
        github_issue_in_branch("yahia/218-arabic-plan-cards"),
        Some(218)
    );
    assert_eq!(github_issue_in_branch("218-x"), Some(218));
    assert_eq!(github_issue_in_branch("218/arabic"), None);
    assert_eq!(github_issue_in_branch("v2-218"), None);
    assert_eq!(github_issue_in_branch("0-zero"), None);
    assert_eq!(github_issue_in_branch("1234"), None);
}

#[test]
fn a_pr_body_names_the_issues_it_closes() {
    // Numbers come back in keyword order, not text order; a fix is expected to change this assertion.
    assert_eq!(
        closing_issue_numbers("Fixes #6, closes #9 and resolves #12. Refs #40."),
        vec![9, 6, 12]
    );
}

#[test]
fn linear_identifiers_are_normalized() {
    assert_eq!(
        normalize_linear_identifier(" spx-1245 "),
        Some("SPX-1245".to_string())
    );
    assert_eq!(normalize_linear_identifier("1ab-2"), None);
    assert_eq!(normalize_linear_identifier("ABC-x"), None);
    assert_eq!(normalize_linear_identifier("ABCDEFGHIJK-1"), None);
}

#[test]
fn a_request_names_a_ticket_of_a_source_or_a_pr() {
    let linear = |id: &str| {
        Ok(WorkItemRef::Ticket(TicketRef::new(
            WorkSourceId::Linear,
            id,
        )))
    };
    let github = |id: &str| {
        Ok(WorkItemRef::Ticket(TicketRef::new(
            WorkSourceId::Github,
            id,
        )))
    };
    let pull_request = |selector: &str| Ok(WorkItemRef::PullRequest(selector.to_string()));
    let cases = [
        (json!({ "source": "linear", "ticketId": "acm-1" }), linear("ACM-1")),
        (json!({ "source": "github", "ticketId": 218 }), github("218")),
        (json!({ "source": "github", "ticketId": "#218" }), github("218")),
        (json!({ "pullRequest": 41 }), pull_request("41")),
        (json!({ "pullRequest": " #41 " }), pull_request("41")),
        (
            json!({ "pullRequest": "https://github.com/acme/app/pull/41" }),
            pull_request("https://github.com/acme/app/pull/41"),
        ),
        // The ticket wins when a request names a PR too.
        (
            json!({ "source": "linear", "ticketId": "ACM-1", "pullRequest": 3 }),
            linear("ACM-1"),
        ),
        (
            json!({ "source": "linear", "ticketId": "not a ticket" }),
            Err("\"not a ticket\" is not a Linear issue ID like SPX-1245.".to_string()),
        ),
        (
            json!({ "source": "jira", "ticketId": "X-1" }),
            Err("source must be linear or github.".to_string()),
        ),
        (
            json!({ "source": "github", "ticketId": 0 }),
            Err("ticketId must be a GitHub issue number.".to_string()),
        ),
        (
            json!({ "pullRequest": "abc" }),
            Err("pullRequest must be a PR number or URL.".to_string()),
        ),
        (
            json!({}),
            Err("Pass source and ticketId (linear SPX-1245, github 218) or pullRequest (412 or its URL).".to_string()),
        ),
    ];
    for (request, expected) in cases {
        assert_eq!(
            WorkItemRef::from_params(&params(request.clone())),
            expected,
            "{request}"
        );
    }
}

#[test]
fn linear_states_map_to_the_work_page_groups() {
    let cases = [
        (Some("started"), Some("In Review"), "review", "In Review"),
        (
            Some("started"),
            Some("In Progress"),
            "progress",
            "In Progress",
        ),
        (Some("unstarted"), Some("Todo"), "todo", "Todo"),
        (Some("backlog"), Some("Backlog"), "backlog", "Backlog"),
        (Some("triage"), Some("Triage"), "backlog", "Triage"),
        (Some("completed"), Some("Done"), "done", "Done"),
        (Some("canceled"), Some("Canceled"), "canceled", "Canceled"),
        (None, None, "todo", "Open"),
    ];
    for (state_type, state_name, group, name) in cases {
        let status = linear_status(state_type, state_name);
        assert_eq!((status.group, status.name.as_str()), (group, name));
    }
}

#[test]
fn github_project_statuses_map_to_the_work_page_groups() {
    let project = |status: Option<&str>| GithubListProject {
        title: "Roadmap".to_string(),
        status: status.map(str::to_string),
        url: None,
    };
    let cases = [
        (Some("In Review"), "review", "In Review"),
        (Some("In Progress"), "progress", "In Progress"),
        (Some("Doing"), "progress", "Doing"),
        (Some("Done"), "done", "Done"),
        (Some("Complete"), "done", "Complete"),
        (Some("Shipped"), "done", "Shipped"),
        (Some("Backlog"), "backlog", "Backlog"),
        (Some("Triage"), "backlog", "Triage"),
        (Some("Icebox"), "backlog", "Icebox"),
        (Some("Ready"), "todo", "Ready"),
        (None, "todo", "Open"),
    ];
    for (status, group, name) in cases {
        let status = github_issue_status(Some(&project(status)));
        assert_eq!((status.group, status.name.as_str()), (group, name));
    }
    let status = github_issue_status(None);
    assert_eq!((status.group, status.name.as_str()), ("todo", "Open"));
}
