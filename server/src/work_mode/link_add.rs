//! Adding one ticket to a session's links without touching the others (the Work page's "Link to
//! current session"), and the `setSessionWorkLinks` request that takes exactly that back; and the
//! request saved, an added ticket command's ticket checked under the same guard as a set one.
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
use crate::paths::GxserverPaths;

use super::*;

/// The keys a request may carry to add instead of set, besides a ticket named by `source` +
/// `ticketId` (the Work page): `ghostex link-session`, the link picker and the phone send these.
/// `addTickets` maps a ticket command's id to the ticket IDs to add.
const ADD_KEYS: &[&str] = &[
    "addLinearIssues",
    "addGithubIssues",
    "addPullRequest",
    "addTickets",
];

/// What a saved `/api/setSessionWorkLinks` request answers: the links written, and for an add the
/// request that takes it back.
pub(crate) type SavedWorkLinks = (Map<String, Value>, Option<Map<String, Value>>);

/// Saves a `/api/setSessionWorkLinks` request on the session and answers the links written and,
/// for an add, the request that takes it back. The session's links are read, an add expanded into
/// them, the request's `tickets` checked (`checked_ticket_links`) and the links written, all under
/// `TicketLinkLocks`: so a ticket command's ticket an add names is checked like a set one, and two
/// saves on one session never both read the same links, the second writing over the first.
pub(crate) fn save_session_work_links(
    repository: &DomainRepository,
    db: &Connection,
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    params: &Map<String, Value>,
) -> Result<SavedWorkLinks, DomainStateError> {
    let locks = hold_ticket_link_locks()?;
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
    // A ticket command's links name only the command of the session's workspace, read under the
    // locks: a move rewrites the project's workspace.
    let session_workspace = || {
        let project = repository.get_project(project_id)?.unwrap_or(Value::Null);
        Ok(crate::workspaces::project_workspace_id(
            &crate::workspaces::read_sidebar_workspaces(db)?,
            &project,
            &repository.list_projects_without_session_history()?,
        ))
    };
    let mut request = checked_ticket_links(
        &locks,
        added.as_ref().map_or(params, |added| &added.request),
        session_workspace,
        db,
        paths,
    )?;
    let undo = added.map(|mut added| {
        drop_unchanged_tickets(&mut request, &mut added.undo, &added.linked_tickets);
        added.undo
    });
    let links = merge_work_links(existing, &request)?;
    write_work_links(db, project_id, session_id, &links)?;
    Ok((links, undo))
}

/// Merges Start's links into the session it just created, read and written under
/// `TicketLinkLocks` like a `/api/setSessionWorkLinks` save, so neither writes over the other.
/// Start never links a ticket command's ticket (`WorkTicket::of` refuses one), so nothing is
/// checked.
pub(crate) fn link_started_session(
    repository: &DomainRepository,
    db: &Connection,
    project_id: &str,
    session_id: &str,
    request: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let _locks = hold_ticket_link_locks()?;
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

/// Leaves out of the checked request, and out of its undo, each ticket command whose checked list
/// is the one the session linked before the add: re-adding a linked ticket changes nothing, once
/// it has passed the check.
fn drop_unchanged_tickets(
    request: &mut Map<String, Value>,
    undo: &mut Map<String, Value>,
    linked_tickets: &std::collections::BTreeMap<String, Value>,
) {
    for (id, before) in linked_tickets {
        if request.get("tickets").and_then(|tickets| tickets.get(id)) != Some(before) {
            continue;
        }
        for map in [&mut *request, &mut *undo] {
            if let Some(Value::Object(tickets)) = map.get_mut("tickets") {
                tickets.remove(id);
                if tickets.is_empty() {
                    map.remove("tickets");
                }
            }
        }
    }
}

/// What a request with `add…` keys becomes: the `setSessionWorkLinks` request to merge, and the
/// one that undoes it.
pub(crate) struct AddedWorkLinks {
    pub(crate) request: Map<String, Value>,
    pub(crate) undo: Map<String, Value>,
    /// Each ticket command the add names, with the ids the session linked before it
    /// (`drop_unchanged_tickets`).
    pub(crate) linked_tickets: std::collections::BTreeMap<String, Value>,
}

/// `None` when the request adds nothing (a plain set).
pub(crate) fn expand_added_work_links(
    project: &Value,
    session: &Value,
    existing: Option<&Map<String, Value>>,
    params: &Map<String, Value>,
) -> Result<Option<AddedWorkLinks>, DomainStateError> {
    // A ticket command's ticket, named by `source` + `ticketId` (the Work page's Link to), stays
    // as typed: `checked_ticket_links` checks it as typed and as the command reads it, and
    // refuses a source the session's workspace doesn't pin in its fixed words.
    let command_ticket = command_ticket_param(params);
    let ticket = match command_ticket {
        Some(_) => None,
        None => TicketRef::from_params(params)
            .transpose()
            .map_err(DomainStateError::bad_request)?,
    };
    let mut added_tickets = added_command_tickets(params.get("addTickets"))?;
    let names_command_ticket = command_ticket.is_some();
    if let Some((source, id)) = command_ticket {
        added_tickets.entry(source).or_default().push(id);
    }
    let mut added: HashMap<WorkSourceId, Vec<Value>> = WorkSourceId::BUILT_IN
        .into_iter()
        .filter_map(|source| {
            added_work_link_field(source)
                .and_then(|field| params.get(field))
                .map(|value| (source, list_values(value)))
        })
        .collect();
    match ticket {
        Some(ticket) => added
            .entry(ticket.source)
            .or_default()
            .push(json!(ticket.id)),
        None if !names_command_ticket && !ADD_KEYS.iter().any(|key| params.contains_key(*key)) => {
            return Ok(None)
        }
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
    // A ticket command's: each id the request adds joins the ones the session links now (the
    // branch's, or the hand-set list). Each is sent exactly as typed, one already linked included:
    // `checked_ticket_links` checks it as typed and as the command reads it, against what the
    // workspace stores now, and drops a duplicate. A command whose list comes out unchanged is then
    // left as it was (`drop_unchanged_tickets`).
    let mut tickets = Map::new();
    let mut undo_tickets = Map::new();
    let mut linked_tickets = std::collections::BTreeMap::new();
    for (id, typed) in added_tickets {
        let source = CommandSourceId::known(&id).map(WorkSourceId::Command);
        let linked = source
            .map(|source| targets.ids_of(source))
            .unwrap_or_default();
        let mut ids = linked.clone();
        ids.extend(typed);
        undo_tickets.insert(
            id.clone(),
            existing
                .and_then(|links| links.get("tickets"))
                .and_then(|tickets| tickets.get(&id))
                .cloned()
                .unwrap_or(Value::Null),
        );
        tickets.insert(id.clone(), json!(ids));
        linked_tickets.insert(id, json!(linked));
    }
    if !tickets.is_empty() {
        request.insert("tickets".to_string(), Value::Object(tickets));
        undo.insert("tickets".to_string(), Value::Object(undo_tickets));
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
    Ok(Some(AddedWorkLinks {
        request,
        undo,
        linked_tickets,
    }))
}

/// The `source` + `ticketId` a request names when its source isn't Linear or GitHub: a ticket
/// command's id, read as `WorkSourceId::parse` reads one, and the ticket ID exactly as typed.
fn command_ticket_param(params: &Map<String, Value>) -> Option<(String, String)> {
    let id = params.get("ticketId").filter(|value| !value.is_null())?;
    let source = params.get("source").and_then(Value::as_str)?.trim();
    if WorkSourceId::parse_built_in(source).is_some() {
        return None;
    }
    let id = id
        .as_u64()
        .map(|number| number.to_string())
        .or_else(|| id.as_str().map(str::to_string))
        .unwrap_or_default();
    Some((source.to_string(), id))
}

/// `addTickets`: a ticket command's id to the ticket IDs to add, each as typed.
fn added_command_tickets(
    value: Option<&Value>,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, DomainStateError> {
    let mut added = std::collections::BTreeMap::new();
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(added);
    };
    let refused = || {
        DomainStateError::bad_request(
            "addTickets must map a ticket command's id to its ticket IDs.",
        )
    };
    for (id, ids) in value.as_object().ok_or_else(refused)? {
        let ids: Vec<String> = list_values(ids)
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .or_else(|| item.as_u64().map(|number| number.to_string()))
                    .ok_or_else(refused)
            })
            .collect::<Result<_, _>>()?;
        added.insert(id.clone(), ids);
    }
    Ok(added)
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
