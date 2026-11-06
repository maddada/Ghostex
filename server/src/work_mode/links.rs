//! The links a person set by hand on a session, stored as `runtimeSettings.workLinks`.
//!
//! A key that is present overrides what the branch says; a key that is absent leaves that kind
//! of link automatic. So `linearIssues: []` means "no Linear issue, whatever the branch says",
//! while no `linearIssues` key means "whatever the branch says".

use std::collections::BTreeMap;

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

use super::{
    hold_work_source_writer, parse_github_project_reference, stored_values, work_source,
    workspace_command, CommandSourceId, WorkSourceError, WorkSourceId,
};

/// Why a request's `tickets` names a ticket command the session's workspace doesn't read. Quotes
/// nothing it was sent: a typed id can be a credential the command keeps.
pub(crate) const TICKET_LINKS_REFUSED: &str = "Only the ticket command of the session's workspace can be linked, and not in a workspace connected to a team.";

/// A session's hand-set links. `None` = automatic, `Some(empty)` = explicitly none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ManualWorkLinks {
    pub(crate) pull_request: Option<Option<ManualPullRequest>>,
    pub(crate) linear_issues: Option<Vec<String>>,
    pub(crate) github_issues: Option<Vec<u64>>,
    pub(crate) linear_project: Option<Option<String>>,
    /// A GitHub Project as `owner/number`.
    pub(crate) github_project: Option<Option<String>>,
    /// Each ticket command's hand-set ticket ids, by the command's id (`workLinks.tickets`); an id
    /// absent here is automatic.
    pub(crate) tickets: BTreeMap<String, Vec<String>>,
}

impl ManualWorkLinks {
    /// One source's hand-set ticket ids; `None` = automatic.
    pub(crate) fn ticket_ids(&self, source: WorkSourceId) -> Option<Vec<String>> {
        match source {
            WorkSourceId::Linear => self.linear_issues.clone(),
            WorkSourceId::Github => self
                .github_issues
                .as_ref()
                .map(|numbers| numbers.iter().map(u64::to_string).collect()),
            WorkSourceId::Command(id) => self.tickets.get(id.as_str()).cloned(),
        }
    }
}

/// The request key that adds tickets of a source to the ones a session already links; none for a
/// ticket command, whose additions go under `addTickets`, keyed by the command's id.
pub(crate) fn added_work_link_field(source: WorkSourceId) -> Option<&'static str> {
    match source {
        WorkSourceId::Linear => Some("addLinearIssues"),
        WorkSourceId::Github => Some("addGithubIssues"),
        WorkSourceId::Command(_) => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ManualPullRequest {
    Number(u64),
    Url(String),
}

pub(crate) fn manual_work_links(session: &Value) -> ManualWorkLinks {
    let Some(links) = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("workLinks"))
        .and_then(Value::as_object)
    else {
        return ManualWorkLinks::default();
    };
    ManualWorkLinks {
        pull_request: links.get("pullRequest").map(parse_pull_request),
        linear_issues: links.get("linearIssues").map(|value| {
            value
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .filter_map(normalize_linear_identifier)
                        .collect()
                })
                .unwrap_or_default()
        }),
        github_issues: links.get("githubIssues").map(|value| {
            value
                .as_array()
                .map(|items| items.iter().filter_map(issue_number).collect())
                .unwrap_or_default()
        }),
        linear_project: links.get("linearProject").map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        }),
        github_project: links.get("githubProject").map(|value| {
            value
                .as_str()
                .and_then(parse_github_project_reference)
                .map(|(owner, number)| format!("{owner}/{number}"))
        }),
        tickets: links
            .get("tickets")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(id, ids)| {
                let ids = ids
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                (id.clone(), ids)
            })
            .collect(),
    }
}

/// Why a ticket id can't be linked: it holds a value the workspace keeps, which the session's
/// links would keep and show as it is. Quotes nothing it was sent.
pub(crate) const TICKET_LINK_SPELLS_CREDENTIAL: &str =
    "That ticket ID can't be linked: it contains the value of a credential this workspace keeps.";

/// The locks a session's links are read, checked and written under: the ticket command store's
/// writer lock, then the placement lock, which every change to a project's workspace takes
/// (`DomainRepository::update_project`). Held from the read of the session's links through their
/// write, no clear, Switch, team join, pruning, credential change or project move lands between
/// the check and the write, and no other save of the session's links between its read and its
/// write.
pub(crate) struct TicketLinkLocks {
    _writer: std::sync::MutexGuard<'static, ()>,
    _placement: crate::domain::repository::project::PlacementGuard,
}

#[cfg(test)]
thread_local! {
    static LINK_LOCKS_WANTED: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Says on `wanted` when this thread next asks for `TicketLinkLocks`, just before it waits for
/// them: a test knows another save of the session waits behind the one it paused.
#[cfg(test)]
pub(crate) fn notify_ticket_link_locks_wanted(wanted: std::sync::mpsc::Sender<()>) {
    LINK_LOCKS_WANTED.with(|slot| *slot.borrow_mut() = Some(wanted));
}

pub(crate) fn hold_ticket_link_locks() -> Result<TicketLinkLocks, DomainStateError> {
    #[cfg(test)]
    LINK_LOCKS_WANTED.with(|wanted| {
        if let Some(wanted) = wanted.borrow_mut().take() {
            let _ = wanted.send(());
        }
    });
    let writer = hold_work_source_writer()?;
    let placement = crate::domain::repository::project::hold_project_placement()?;
    Ok(TicketLinkLocks {
        _writer: writer,
        _placement: placement,
    })
}

/// The request with its `tickets` checked, which a remote client may send; one that names no
/// tickets is answered as it is. The caller holds `TicketLinkLocks` until it has written the
/// answer, and the session's workspace (`session_workspace`) is read under them. The check:
/// - what the workspace stores is read strictly (`stored_values`): a file that can't be read
///   refuses the request;
/// - a clear (`null`, or no ids) of a command the session's links (`session_links`, read under
///   the locks) already store passes whatever the workspace pins now, so a link left by a cleared
///   command or from before a team join can still be cleared; it writes no id;
/// - otherwise each key must be the ticket command the session's workspace pins, outside a team;
/// - each id must be one of its tickets, kept as its `parse_ref` reads it, and neither as typed
///   nor as read may it hold a stored value of 3 characters or more;
/// - `null` puts the command back to automatic.
///
/// Links stored under any other id are never written by a request but to clear them, so a link
/// under a command since cleared or switched from stays as it was until then, and no request adds
/// a key the session doesn't store for a command the workspace doesn't pin. Every refusal is in
/// fixed words.
pub(crate) fn checked_ticket_links(
    _locks: &TicketLinkLocks,
    request: &Map<String, Value>,
    session_links: Option<&Map<String, Value>>,
    session_workspace: impl FnOnce() -> Result<String, DomainStateError>,
    db: &Connection,
    paths: &GxserverPaths,
) -> Result<Map<String, Value>, DomainStateError> {
    let Some(requested) = request.get("tickets") else {
        return Ok(request.clone());
    };
    let session_tickets = session_links
        .and_then(|links| links.get("tickets"))
        .and_then(Value::as_object);
    let requested = requested.as_object().ok_or_else(|| {
        DomainStateError::bad_request("tickets must map a ticket command's id to its ticket IDs.")
    })?;
    let workspace_id = &session_workspace()?;
    let stored = stored_values(db, paths, workspace_id).map_err(|error| match error {
        WorkSourceError::Storage(error) => error,
        WorkSourceError::Refused(text) => DomainStateError::bad_request(text),
        WorkSourceError::Failed(failure) => DomainStateError::bad_request(failure.message),
    })?;
    // Read once the file is known to be readable, so a damaged file never reads as "no team".
    let pinned = workspace_command(workspace_id)
        .filter(|_| !crate::team_sync::workspace_has_team(paths, workspace_id))
        .and_then(|command| CommandSourceId::known(&command.source_id));
    let mut checked = Map::new();
    for (id, ids) in requested {
        let clears = ids.is_null() || list_of(ids).is_empty();
        if clears && session_tickets.is_some_and(|tickets| tickets.contains_key(id)) {
            let cleared = if ids.is_null() {
                Value::Null
            } else {
                json!([])
            };
            checked.insert(id.clone(), cleared);
            continue;
        }
        let Some(source) = pinned.filter(|pinned| pinned.as_str() == id) else {
            return Err(DomainStateError::bad_request(TICKET_LINKS_REFUSED));
        };
        if ids.is_null() {
            checked.insert(id.clone(), Value::Null);
            continue;
        }
        let rules = work_source(WorkSourceId::Command(source));
        let mut canonical: Vec<String> = Vec::new();
        // The refusal says whether a typed id contains a stored value. That is fine while only
        // remote-allowed clients reach this route, which are fully trusted; if
        // `/api/setSessionWorkLinks` ever joined `PEER_ALLOWED_ENDPOINTS`, it would let another
        // computer guess short secrets one try at a time.
        for item in list_of(ids) {
            let text = item
                .as_str()
                .map(str::to_string)
                .or_else(|| item.as_u64().map(|number| number.to_string()))
                .unwrap_or_default();
            if stored.found_in(&text) {
                return Err(DomainStateError::bad_request(TICKET_LINK_SPELLS_CREDENTIAL));
            }
            let ticket = rules
                .parse_ref(&text)
                .ok_or_else(|| DomainStateError::bad_request(rules.invalid_ref(&text)))?;
            if stored.found_in(&ticket) {
                return Err(DomainStateError::bad_request(TICKET_LINK_SPELLS_CREDENTIAL));
            }
            if !canonical.contains(&ticket) {
                canonical.push(ticket);
            }
        }
        checked.insert(id.clone(), json!(canonical));
    }
    let mut request = request.clone();
    request.insert("tickets".to_string(), Value::Object(checked));
    // A test pauses here, after the check and before the write, as it pauses a job before its
    // command starts (`hook_before_spawn`).
    #[cfg(test)]
    super::before_spawn(workspace_id);
    Ok(request)
}

fn parse_pull_request(value: &Value) -> Option<ManualPullRequest> {
    if let Some(number) = issue_number(value) {
        return Some(ManualPullRequest::Number(number));
    }
    let text = value.as_str()?.trim();
    if text.starts_with("https://") || text.starts_with("http://") {
        return Some(ManualPullRequest::Url(text.to_string()));
    }
    None
}

fn issue_number(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().trim_start_matches('#').parse().ok())
        })
        .filter(|number| *number > 0)
}

/// `spx-1245` → `SPX-1245`; anything that is not `<key>-<number>` → `None`.
pub(crate) fn normalize_linear_identifier(text: &str) -> Option<String> {
    let text = text.trim();
    let (key, number) = text.split_once('-')?;
    let valid = !key.is_empty()
        && key.len() <= 10
        && key.chars().all(|c| c.is_ascii_alphanumeric())
        && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit());
    valid.then(|| format!("{}-{}", key.to_ascii_uppercase(), number))
}

/// Applies a request to the stored links. Each kind the request names is set (`null` = back to
/// automatic); `clear: true` first puts every kind back to automatic.
pub(crate) fn merge_work_links(
    existing: Option<&Map<String, Value>>,
    request: &Map<String, Value>,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut links = if request.get("clear") == Some(&Value::Bool(true)) {
        Map::new()
    } else {
        existing.cloned().unwrap_or_default()
    };
    if let Some(value) = request.get("pullRequest") {
        match value {
            Value::Null => {
                links.remove("pullRequest");
            }
            Value::String(text) if text.trim().eq_ignore_ascii_case("none") => {
                links.insert("pullRequest".to_string(), Value::Null);
            }
            other => {
                let parsed = parse_pull_request(other).ok_or_else(|| {
                    DomainStateError::bad_request("pullRequest must be a PR number or URL.")
                })?;
                links.insert(
                    "pullRequest".to_string(),
                    match parsed {
                        ManualPullRequest::Number(number) => json!(number),
                        ManualPullRequest::Url(url) => json!(url),
                    },
                );
            }
        }
    }
    if let Some(value) = request.get("linearIssues") {
        match value {
            Value::Null => {
                links.remove("linearIssues");
            }
            other => {
                let items = list_of(other);
                let mut identifiers = Vec::new();
                for item in items {
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
                links.insert("linearIssues".to_string(), json!(identifiers));
            }
        }
    }
    if let Some(value) = request.get("githubIssues") {
        match value {
            Value::Null => {
                links.remove("githubIssues");
            }
            other => {
                let mut numbers = Vec::new();
                for item in list_of(other) {
                    let number = issue_number(&item).ok_or_else(|| {
                        DomainStateError::bad_request("githubIssues must be issue numbers.")
                    })?;
                    if !numbers.contains(&number) {
                        numbers.push(number);
                    }
                }
                links.insert("githubIssues".to_string(), json!(numbers));
            }
        }
    }
    if let Some(value) = request.get("linearProject") {
        match value {
            Value::Null => {
                links.remove("linearProject");
            }
            Value::String(name) if !name.trim().is_empty() => {
                links.insert("linearProject".to_string(), json!(name.trim()));
            }
            _ => {
                links.insert("linearProject".to_string(), Value::Null);
            }
        }
    }
    // Ticket command links, as `checked_ticket_links` wrote them: each id's list, or `null` for
    // automatic.
    if let Some(value) = request.get("tickets") {
        let requested = value.as_object().ok_or_else(|| {
            DomainStateError::bad_request(
                "tickets must map a ticket command's id to its ticket IDs.",
            )
        })?;
        let mut tickets = links
            .get("tickets")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (id, ids) in requested {
            match ids {
                Value::Null => {
                    tickets.remove(id);
                }
                Value::Array(items) if items.iter().all(Value::is_string) => {
                    tickets.insert(id.clone(), ids.clone());
                }
                _ => {
                    return Err(DomainStateError::bad_request(
                        "tickets must map a ticket command's id to its ticket IDs.",
                    ))
                }
            }
        }
        if tickets.is_empty() {
            links.remove("tickets");
        } else {
            links.insert("tickets".to_string(), Value::Object(tickets));
        }
    }
    // `owner/number` or the project's URL; `"none"` or `""` = explicitly none.
    if let Some(value) = request.get("githubProject") {
        match value {
            Value::Null => {
                links.remove("githubProject");
            }
            Value::String(text)
                if text.trim().is_empty() || text.trim().eq_ignore_ascii_case("none") =>
            {
                links.insert("githubProject".to_string(), Value::Null);
            }
            other => {
                let (owner, number) = other
                    .as_str()
                    .and_then(parse_github_project_reference)
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "githubProject must be owner/number (like acme/12) or the project's link.",
                        )
                    })?;
                links.insert(
                    "githubProject".to_string(),
                    json!(format!("{owner}/{number}")),
                );
            }
        }
    }
    Ok(links)
}

/// A scalar becomes a one-item list; `"none"` and `[]` become an empty list.
fn list_of(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::String(text) if text.trim().eq_ignore_ascii_case("none") => Vec::new(),
        Value::String(text) => text
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| json!(part))
            .collect(),
        other => vec![other.clone()],
    }
}

/// Writes `runtimeSettings.workLinks` in one statement.
///
/// CDXC:WorkMode 2026-10-09 WHY:
/// Hooks, titles and activity rewrite the same runtime settings JSON all the time, so the links
/// are patched in place (as agentbox does) instead of reading and rewriting the whole object.
pub(crate) fn write_work_links(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    links: &Map<String, Value>,
) -> Result<bool, DomainStateError> {
    let changed = if links.is_empty() {
        db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.workLinks') \
             WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
        )
    } else {
        db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.workLinks', json(?1)) \
             WHERE projectId = ?2 AND sessionId = ?3",
            params![Value::Object(links.clone()).to_string(), project_id, session_id],
        )
    }
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite work links error: {error}"),
    })?;
    Ok(changed > 0)
}
