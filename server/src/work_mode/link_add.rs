//! Adding one ticket to a session's links without touching the others (the Work page's "Link to
//! current session"), and the `setSessionWorkLinks` request that takes exactly that back; and the
//! request saved.
//!
//! CDXC:WorkMode 2026-10-10 WHY:
//! Hand-set keys replace what the branch says (links.rs), so "add SPX-2" cannot be sent as
//! `linearIssues: [SPX-2]`: a session automatically linked to SPX-1 by its branch would lose it.
//! The added ticket joins the session's effective list here, where the branch's links are known,
//! and the undo restores each touched key as it was stored (absent = automatic again).

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use rusqlite::Connection;

use crate::domain::{DomainRepository, DomainStateError};

use super::*;

/// The keys a request may carry to add instead of set, besides a ticket named by `source` +
/// `ticketId` (the Work page): `ghostex link-session`, the link picker and the phone send these.
const ADD_KEYS: &[&str] = &["addLinearIssues", "addGithubIssues", "addPullRequest"];

/// What a saved `/api/setSessionWorkLinks` request answers: the links written, and for an add the
/// request that takes it back.
pub(crate) type SavedWorkLinks = (Map<String, Value>, Option<Map<String, Value>>);

/// Saves a `/api/setSessionWorkLinks` request on the session and answers the links written and,
/// for an add, the request that takes it back. The session's links are read, an add expanded into
/// them and the links written under `hold_work_links_lock`.
pub(crate) fn save_session_work_links(
    repository: &DomainRepository,
    db: &Connection,
    project_id: &str,
    session_id: &str,
    params: &Map<String, Value>,
) -> Result<SavedWorkLinks, DomainStateError> {
    let _lock = hold_work_links_lock();
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::bad_request("No such session."))?;
    let existing = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("workLinks"))
        .and_then(Value::as_object);
    // `add…` keys (the Work page's Link to current session) join the session's effective links
    // and answer with the request that takes them back.
    let project = repository.get_project(project_id)?.unwrap_or(Value::Null);
    let added = expand_added_work_links(&project, &session, existing, params)?;
    let request = added.as_ref().map_or(params, |added| &added.request);
    let links = merge_work_links(existing, request)?;
    write_work_links(db, project_id, session_id, &links)?;
    Ok((links, added.map(|added| added.undo)))
}

/// Merges Start's links into the session it just created, read and written under
/// `hold_work_links_lock` like a `/api/setSessionWorkLinks` save, so neither writes over the other.
pub(crate) fn link_started_session(
    repository: &DomainRepository,
    db: &Connection,
    project_id: &str,
    session_id: &str,
    request: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let _lock = hold_work_links_lock();
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::bad_request("The new session is gone."))?;
    let existing = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("workLinks"))
        .and_then(Value::as_object);
    let links = merge_work_links(existing, request)?;
    write_work_links(db, project_id, session_id, &links)?;
    Ok(())
}

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
    let ticket = TicketRef::from_params(params)
        .transpose()
        .map_err(DomainStateError::bad_request)?;
    let mut added: HashMap<WorkSourceId, Vec<Value>> = WorkSourceId::BUILT_IN
        .into_iter()
        .filter_map(|source| {
            params
                .get(added_work_link_field(source))
                .map(|value| (source, list_values(value)))
        })
        .collect();
    match ticket {
        Some(ticket) => added
            .entry(ticket.source)
            .or_default()
            .push(json!(ticket.id)),
        None if !ADD_KEYS.iter().any(|key| params.contains_key(*key)) => return Ok(None),
        None => {}
    }
    let targets = work_targets(project, session);
    let mut request: Map<String, Value> = params
        .iter()
        .filter(|(key, _)| {
            !ADD_KEYS.contains(&key.as_str()) && !matches!(key.as_str(), "source" | "ticketId")
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut undo = Map::new();
    let previous = |key: &str| {
        existing
            .and_then(|links| links.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    };

    if let Some(added) = added.remove(&WorkSourceId::Linear) {
        let linked = targets.ids_of(WorkSourceId::Linear);
        let mut identifiers = linked.clone();
        for item in added {
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
        if identifiers != linked {
            undo.insert("linearIssues".to_string(), previous("linearIssues"));
            request.insert("linearIssues".to_string(), json!(identifiers));
        }
    }
    if let Some(added) = added.remove(&WorkSourceId::Github) {
        let linked = targets.github_issue_numbers();
        let mut numbers = linked.clone();
        for item in added {
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
        if numbers != linked {
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
