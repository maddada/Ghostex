//! Hands what a ticket's Work details just read to the caches the sidebar cards read, so a card
//! never lags behind the details for the same ticket (a PR Linear attached a moment ago).

use serde_json::Value;

use crate::session_git_status::PullRequestState;

use super::*;

/// Shares the details' Linear PR URLs and their PR with the card caches. Returns whether a cache
/// changed, so the caller republishes the cards now instead of on the next pass.
pub(crate) fn share_work_item_with_sidebar(projects: &[WorkProjectInput], details: &Value) -> bool {
    let mut changed = false;
    if let Some(linear) = details
        .get("ticket")
        .filter(|ticket| ticket.get("source").and_then(Value::as_str) == Some("linear"))
    {
        let identifier = linear.get("id").and_then(Value::as_str);
        let urls: Vec<String> = linear
            .get("pullRequestUrls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if let (Some(identifier), false) = (identifier, urls.is_empty()) {
            // Only an issue a card already caches gains the URLs, so trying every key is harmless.
            let mut fingerprints: Vec<u64> = projects
                .iter()
                .filter_map(|input| {
                    input.credentials.linear_api_key(
                        Some(&input.project_id),
                        crate::workspaces::stored_project_workspace_id(&input.project),
                    )
                })
                .map(|key| linear_key_fingerprint(&key))
                .collect();
            fingerprints.sort_unstable();
            fingerprints.dedup();
            for fingerprint in fingerprints {
                changed |= remember_linear_issue_pull_requests(fingerprint, identifier, &urls);
            }
        }
    }
    if let Some(info) = details.get("pullRequest").and_then(pull_request_info) {
        changed |= remember_work_pull_request(info);
    }
    changed
}

/// The card's PR from the details' PR (`fetch_pull_request_detail`).
fn pull_request_info(pull_request: &Value) -> Option<WorkPullRequestInfo> {
    let state = match pull_request.get("state").and_then(Value::as_str)? {
        "open" => PullRequestState::Open,
        "draft" => PullRequestState::Draft,
        "merged" => PullRequestState::Merged,
        "closed" => PullRequestState::Closed,
        _ => return None,
    };
    let count = |key: &str| {
        pull_request
            .pointer(&format!("/checksSummary/{key}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    // The same word `checks_state` gives the rollup.
    let checks = if count("failed") > 0 {
        Some("failing")
    } else if count("pending") > 0 {
        Some("pending")
    } else if count("total") > 0 {
        Some("passing")
    } else {
        None
    };
    Some(WorkPullRequestInfo {
        number: pull_request.get("number").and_then(Value::as_u64)?,
        state,
        url: pull_request
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string),
        checks,
        title: pull_request
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        review: None,
        closing_issues: Vec::new(),
        head_ref: pull_request
            .get("headBranch")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}
