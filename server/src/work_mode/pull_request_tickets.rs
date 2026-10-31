//! The tickets a pull request belongs to, so its Work details (opened from a session's PR chip)
//! show the same ticket and team-flow steps as the ticket's own details.
//!
//! CDXC:WorkMode 2026-10-10 WHY:
//! A PR opened from its chip read only the PR, so the team flow's ticket step said "no ticket"
//! for a PR whose session also links the ticket (PR #6591 on SPX-13408, ShortPoint live test).
//! Its tickets come from three places, in this order: the sessions that link the PR, the PR's
//! branch (Linear team keys or a GitHub issue number), and Linear's GitHub integration, which
//! attaches the PR's URL to its issue.

use serde_json::{json, Value};

use super::*;

/// What the details already know without a network call: the list row the PR folded into (or the
/// ticket its own row names) and the sessions on this computer that link the PR. `repo` is the
/// requesting project's `owner/repo`, for a PR named by number.
pub(crate) fn pull_request_tickets_known_locally(
    selector: &str,
    repo: Option<&str>,
    item: Option<&Value>,
    projects: &[WorkProjectInput],
) -> Vec<WorkItemRef> {
    let mut tickets = Vec::new();
    if let Some(item) = item {
        // The ticket row the PR folded into, or the Linear ticket its own row names.
        let ticket = match item.get("ticket").and_then(Value::as_str) {
            Some(identifier) => normalize_linear_identifier(identifier)
                .map(|identifier| Ok(TicketRef::new(WorkSourceId::Linear, identifier))),
            None => item.as_object().and_then(TicketRef::from_params),
        };
        if let Some(Ok(ticket)) = ticket {
            push_ticket(&mut tickets, WorkItemRef::Ticket(ticket));
        }
    }
    let Some(wanted) = pull_request_identity(selector, repo) else {
        return tickets;
    };
    for input in projects {
        let session_repo = input.repo.as_deref().or(Some(input.project_id.as_str()));
        for session in &input.sessions {
            let targets = work_targets(&input.project, session);
            let links_this_pr = targets
                .pull_request
                .as_deref()
                .and_then(|linked| pull_request_identity(linked, session_repo))
                .is_some_and(|linked| linked == wanted);
            if !links_this_pr {
                continue;
            }
            // A session's GitHub issues are in its own repo, which is the PR's.
            for linked in targets.tickets {
                push_ticket(&mut tickets, WorkItemRef::Ticket(linked.ticket));
            }
        }
    }
    tickets
}

/// The tickets the PR's head branch names, each source's in turn with the ticket-id prefixes it
/// knows: Linear identifiers with a known team key, or the GitHub issue number a
/// `<user>/<number>-<slug>` branch starts with.
pub(crate) fn pull_request_branch_tickets(
    pull_request: &Value,
    known_keys: &[(WorkSourceId, Vec<String>)],
) -> Vec<WorkItemRef> {
    let Some(head) = pull_request.get("headBranch").and_then(Value::as_str) else {
        return Vec::new();
    };
    let text = PullRequestText { head, body: None };
    let mut tickets = Vec::new();
    for (source, keys) in known_keys {
        for id in work_source(*source).tickets_in_pull_request(keys, &text) {
            push_ticket(
                &mut tickets,
                WorkItemRef::Ticket(TicketRef::new(*source, id)),
            );
        }
    }
    tickets
}

/// The Linear issues whose attachments include the PR's URL (Linear's documented
/// `attachmentsForURL` query; its GitHub integration attaches every linked PR).
pub(crate) fn linear_issues_attached_to_url(
    api_key: &str,
    url: &str,
) -> Result<Vec<String>, String> {
    let body = linear_graphql(
        api_key,
        "query($url: String!) { attachmentsForURL(url: $url) { nodes { issue { identifier } } } }",
        json!({ "url": url }),
    )?;
    let mut identifiers: Vec<String> = Vec::new();
    for node in body
        .pointer("/data/attachmentsForURL/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(identifier) = node
            .pointer("/issue/identifier")
            .and_then(Value::as_str)
            .and_then(normalize_linear_identifier)
        {
            if !identifiers.contains(&identifier) {
                identifiers.push(identifier);
            }
        }
    }
    Ok(identifiers)
}

pub(crate) fn push_ticket(tickets: &mut Vec<WorkItemRef>, ticket: WorkItemRef) {
    if !tickets.contains(&ticket) {
        tickets.push(ticket);
    }
}

/// `owner/repo` (lowercase) and number of a PR named by URL, or by number within `repo`.
fn pull_request_identity(selector: &str, repo: Option<&str>) -> Option<(String, u64)> {
    let selector = selector.trim().trim_start_matches('#');
    match selector.parse::<u64>() {
        Ok(number) => Some((repo?.to_ascii_lowercase(), number)),
        Err(_) => pull_request_url_parts(selector),
    }
}
