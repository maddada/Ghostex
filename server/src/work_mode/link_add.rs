//! Adding one ticket to a session's links without touching the others (the Work page's "Link to
//! current session"), and the `setSessionWorkLinks` request that takes exactly that back.
//!
//! CDXC:WorkMode 2026-10-10 WHY:
//! Hand-set keys replace what the branch says (links.rs), so "add SPX-2" cannot be sent as
//! `linearIssues: [SPX-2]`: a session automatically linked to SPX-1 by its branch would lose it.
//! The added ticket joins the session's effective list here, where the branch's links are known,
//! and the undo restores each touched key as it was stored (absent = automatic again).

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::*;

/// The keys a request may carry to add instead of set.
const ADD_KEYS: &[&str] = &["addLinearIssues", "addGithubIssues", "addPullRequest"];

/// What a request with `add…` keys becomes: the `setSessionWorkLinks` request to merge, and the
/// one that undoes it.
pub(crate) struct AddedWorkLinks {
    pub(crate) request: Map<String, Value>,
    pub(crate) undo: Map<String, Value>,
}

/// `None` when the request adds nothing (a plain set).
pub(crate) fn expand_added_work_links(
    project: &Value,
    session: &Value,
    existing: Option<&Map<String, Value>>,
    params: &Map<String, Value>,
) -> Result<Option<AddedWorkLinks>, DomainStateError> {
    if !ADD_KEYS.iter().any(|key| params.contains_key(*key)) {
        return Ok(None);
    }
    let targets = work_targets(project, session);
    let mut request: Map<String, Value> = params
        .iter()
        .filter(|(key, _)| !ADD_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut undo = Map::new();
    let previous = |key: &str| {
        existing
            .and_then(|links| links.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    };

    if let Some(added) = params.get("addLinearIssues") {
        let mut identifiers = targets.linear_issues.clone();
        for item in list_values(added) {
            let text = item.as_str().unwrap_or_default();
            let identifier = normalize_linear_identifier(text).ok_or_else(|| {
                DomainStateError::bad_request(format!(
                    "\"{text}\" is not a Linear issue ID like SPX-1245."
                ))
            })?;
            if !identifiers.contains(&identifier) {
                identifiers.push(identifier);
            }
        }
        if identifiers != targets.linear_issues {
            undo.insert("linearIssues".to_string(), previous("linearIssues"));
            request.insert("linearIssues".to_string(), json!(identifiers));
        }
    }
    if let Some(added) = params.get("addGithubIssues") {
        let mut numbers = targets.github_issues.clone();
        for item in list_values(added) {
            let number = item
                .as_u64()
                .or_else(|| {
                    item.as_str()
                        .and_then(|text| text.trim().trim_start_matches('#').parse().ok())
                })
                .filter(|number| *number > 0)
                .ok_or_else(|| {
                    DomainStateError::bad_request("addGithubIssues takes issue numbers.")
                })?;
            if !numbers.contains(&number) {
                numbers.push(number);
            }
        }
        if numbers != targets.github_issues {
            undo.insert("githubIssues".to_string(), previous("githubIssues"));
            request.insert("githubIssues".to_string(), json!(numbers));
        }
    }
    if let Some(added) = params.get("addPullRequest") {
        let wanted = added
            .as_u64()
            .map(|number| number.to_string())
            .or_else(|| {
                added
                    .as_str()
                    .map(|text| text.trim().trim_start_matches('#').to_string())
            })
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request("addPullRequest takes a PR number or URL.")
            })?;
        let current = targets
            .pull_request
            .clone()
            .or_else(|| targets.branch_pull_request.map(|number| number.to_string()));
        match current {
            None => {
                undo.insert("pullRequest".to_string(), previous("pullRequest"));
                request.insert("pullRequest".to_string(), json!(wanted));
            }
            Some(current) if same_pull_request(&current, &wanted) => {}
            Some(current) => {
                let title = session
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or("This session");
                return Err(DomainStateError::bad_request(format!(
                    "'{title}' is linked to #{}.",
                    pull_request_label(&current)
                )));
            }
        }
    }
    Ok(Some(AddedWorkLinks { request, undo }))
}

fn list_values(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

/// `412` or a PR link, as the number the chip shows.
fn pull_request_label(selector: &str) -> String {
    pull_request_url_parts(selector)
        .map(|(_, number)| number.to_string())
        .unwrap_or_else(|| selector.trim_start_matches('#').to_string())
}

fn same_pull_request(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right) || pull_request_label(left) == pull_request_label(right)
}
