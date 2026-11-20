//! Lead links: an ordinary session that reports to another ordinary session, its lead, the way a
//! coordinator's thread reports to its coordinator, without the coordinator role.
//!
//! CDXC:LeadLinks 2026-11-20 DECISION:
//! GitHub #678 (WangAxerold leads work from ordinary sessions and polls the host with a watcher to
//! learn when the sessions it started finish or go quiet). From the design page the user picked the
//! minimal version (Q4277 A): `ghostex agents create --lead`, `agents link`, `agents unlink`; the
//! final message of each turn THE LEAD started is forwarded to it (turns the user starts in a linked
//! session stay between them), and the lead is told when a linked session waits on an answer, goes
//! quiet or is closed. The lead stays an ordinary session: no playbook, no reply format, it keeps
//! its own work items, and a linked session keeps its own and can still `agents send` its lead.
//! Consent (Q4282, the user's note): "agent asks but I don't need to allow; Unlink is fine to add but
//! I don't want linking sessions to be a context menu item at all; it's a niche use case, keep the
//! lead thing in the guide", and "agents shouldn't have unneeded info/context about this". So there
//! is no approval step and no Link menu item; the app offers only Unlink and Stop leading, and agents
//! learn about leads only from `ghostex agents guide leads`, which opens with "Only when the user
//! asks for it". A session may only make itself a lead; the user's own terminal or the app may link
//! any two. One level only: a lead can't be linked, a linked session can't lead, and neither side
//! may be a coordinator or a coordinator's thread (those have threads).
//! SEE-ALSO: server/src/server/lead_runtime.rs (the watcher), server/src/storage/migrations/0084_lead_links.sql,
//! server/src/ghostex_cli/agents/guide_leads.md, packages/gx-core/src/sidebar_view/rows.rs (nesting).

mod records;

pub(crate) use records::*;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::coordinators::SessionKey;
use crate::domain::{DomainRepository, DomainStateError};
use crate::ids::create_global_session_ref;
use crate::security::Principal;

pub const LEAD_LINKS_ENDPOINT: &str = "/api/leadLinks";

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn ids(
    params: &Map<String, Value>,
    project_key: &str,
    session_key: &str,
) -> Result<SessionKey, DomainStateError> {
    match (text(params, project_key), text(params, session_key)) {
        (Some(project_id), Some(session_id)) => Ok((project_id, session_id)),
        _ => Err(DomainStateError::bad_request(format!(
            "This lead link request needs {project_key} and {session_key}."
        ))),
    }
}

/// Who asked, for the link rules.
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus review of #678, finding 5 (the coordinator's decision): every caller that was not a
/// session passed as the user, so another computer's agents (`Peer`, the endpoint is
/// remote-allowed) could link, unlink or stop any two sessions here, against "another computer is
/// never the user" (236e4b3ef) and `caller_may_reopen`. Now the owner's app, a paired phone and the
/// token file (the user's own terminal) are the user; a session follows the "only itself as lead"
/// rule; Guest, Extension and Anonymous are refused; and a Peer may never link, and may Unlink or
/// Stop leading only when the desktop app issued its grant (`owner_issued`, the relay work of the
/// same day). The desktop's own menu on another computer's sessions does not come as a Peer: it
/// calls that computer's gxserver over its own connection with its owner session or token
/// (`gpui_remote_gxserver_request_target`), so it counts as the user there.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LinkCaller {
    User,
    Session(SessionKey),
    /// Another computer; `owner_issued` when the desktop app issued its grant.
    Peer {
        owner_issued: bool,
    },
    Refused,
}

fn link_caller(caller: &Principal) -> LinkCaller {
    match caller {
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => {
            LinkCaller::User
        }
        Principal::Session {
            project_id,
            session_id,
            ..
        } => LinkCaller::Session((project_id.clone(), session_id.clone())),
        Principal::Peer { server_id, .. } => LinkCaller::Peer {
            owner_issued: crate::connected_computers::owner_issued(server_id),
        },
        Principal::Guest { .. } | Principal::Extension { .. } | Principal::Anonymous => {
            LinkCaller::Refused
        }
    }
}

const NOT_ALLOWED: &str = "Only the user, or a session on this computer, can change lead links.";

/// The calling session, when an agent session's own token made the call.
fn calling_session(caller: &Principal) -> Option<SessionKey> {
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => Some((project_id.clone(), session_id.clone())),
        _ => None,
    }
}

fn is_coordinator_or_thread(db: &Connection, key: &SessionKey) -> bool {
    crate::coordinators::read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()
        .is_some()
        || crate::coordinators::read_thread(db, &key.0, &key.1)
            .ok()
            .flatten()
            .is_some()
}

fn session_title(repository: &DomainRepository<'_>, key: &SessionKey) -> String {
    repository
        .get_session(&key.0, &key.1)
        .ok()
        .flatten()
        .and_then(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// The link rules and the write: `linked` reports to `lead` from now on. Returns the lead it had
/// before, if any. `by_session` says an agent session asked (it may only make itself the lead,
/// checked by the caller); the user (the app, the phone, their own terminal) may also move a
/// session from one lead to another.
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus review of #678, finding 1: a session could take a session already linked to another lead
/// with `agents link X --lead self`, and the report owed to that lead then went to it. A session
/// already linked elsewhere is now refused to an agent ("only the user can move it"), and any
/// change of lead starts the link afresh (records.rs `write_lead_link`).
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus review of #678, finding 8: the "one level deep" checks and the write ran separately, so A
/// linking B under itself while B linked A under itself could both pass and leave each leading
/// the other. They run in one `BEGIN IMMEDIATE` transaction now, so the second waits for the first
/// and then sees it.
pub(crate) fn link_checked(
    db: &Connection,
    by_session: bool,
    linked: &SessionKey,
    lead: &SessionKey,
    owed: bool,
) -> Result<Option<SessionKey>, DomainStateError> {
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)
            .map_err(crate::coordinators::sql_error)?;
    let before = link_rules_and_write(&transaction, by_session, linked, lead, owed)?;
    transaction
        .commit()
        .map_err(crate::coordinators::sql_error)?;
    Ok(before)
}

fn link_rules_and_write(
    db: &Connection,
    by_session: bool,
    linked: &SessionKey,
    lead: &SessionKey,
    owed: bool,
) -> Result<Option<SessionKey>, DomainStateError> {
    if linked == lead {
        return Err(DomainStateError::bad_request(
            "A session can't be its own lead.",
        ));
    }
    if is_coordinator_or_thread(db, lead) {
        return Err(DomainStateError::bad_request(
            "A coordinator or a coordinator's thread can't be a lead: a coordinator has threads (ghostex coordinator start-thread).",
        ));
    }
    if is_coordinator_or_thread(db, linked) {
        return Err(DomainStateError::bad_request(
            "That session is a coordinator or a coordinator's thread, so it can't be linked to a lead.",
        ));
    }
    if read_lead_link(db, &lead.0, &lead.1)?.is_some() {
        return Err(DomainStateError::bad_request(
            "The lead is itself linked to a lead; links are one level deep.",
        ));
    }
    if is_lead(db, &linked.0, &linked.1) {
        return Err(DomainStateError::bad_request(
            "That session leads other sessions, so it can't be linked to a lead; links are one level deep.",
        ));
    }
    let before = read_lead_link(db, &linked.0, &linked.1)?.map(|link| link.lead_key());
    if by_session && before.as_ref().is_some_and(|previous| previous != lead) {
        return Err(DomainStateError::bad_request(
            "That session is linked to another lead; only the user can move it.",
        ));
    }
    write_lead_link(db, linked, lead, owed)?;
    Ok(before)
}

/// One transcript row as `lead_turn_reply` reads it: a message someone typed (`user`) or the
/// agent's words, with its time (ms) when the transcript has one.
#[derive(Clone, Debug)]
pub(crate) struct TurnRow {
    pub user: bool,
    pub text: String,
    pub at: Option<i64>,
}

/// A lead's message reaches its transcript a moment after the send that marked the link owed.
const LEAD_MESSAGE_SLACK_MS: i64 = 30_000;

/// The reply to the lead's newest message: the agent's last words after that message and before
/// the next message the user typed there. `None` until there is one. `rows` are oldest first;
/// `lead_ref` is the lead's Global Ref, which the sender block of each of its messages names.
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus review of #678, finding 3: the report took the newest reply newer than `owedSince`, read
/// again on every try, so a retried report (the lead busy) or a lead message that reached a
/// session busy with the user's turn could send the lead the user's own turn, and then count the
/// lead's turn as reported. The reply is now found from the lead's own message (its sender block),
/// so a retry gives the same reply and a turn the user starts afterwards is never sent.
#[cfg(test)]
pub(crate) fn lead_turn_reply(rows: &[TurnRow], lead_ref: &str, owed_ms: i64) -> Option<String> {
    lead_turn(rows, lead_ref, owed_ms)?.reply
}

/// Where the lead's turn stands in a linked session's transcript: found from the lead's newest
/// message (`lead_turn_reply`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeadTurn {
    /// The agent's last words after the lead's message, before the user typed anything there.
    pub reply: Option<String>,
    /// The user typed a message after the lead's: the session is on the user's own turn now.
    pub user_after: bool,
}

/// The lead's turn, or `None` while its message is not in `rows` (not typed yet, or older than
/// what is owed).
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus re-review of #678: the waiting and quiet notices fired for whatever the session was doing
/// while a turn was owed, so a turn left owed passed the user's own later turns (and the waiting
/// prompt's summary) to the lead. They are sent only while the lead's turn is open: its message is
/// in the transcript and the user has typed nothing since (`user_after`).
pub(crate) fn lead_turn(rows: &[TurnRow], lead_ref: &str, owed_ms: i64) -> Option<LeadTurn> {
    let from_lead = |row: &TurnRow| {
        row.user && crate::coordinators::find_own_agent_message_block(&row.text, lead_ref).is_some()
    };
    let start = rows.iter().rposition(|row| {
        from_lead(row)
            && row
                .at
                .is_none_or(|at| at + LEAD_MESSAGE_SLACK_MS >= owed_ms)
    })?;
    let after = &rows[start + 1..];
    let reply = after
        .iter()
        .take_while(|row| !row.user || from_lead(row))
        .filter(|row| !row.user && !row.text.trim().is_empty())
        .last()
        .map(|row| row.text.clone());
    Some(LeadTurn {
        reply,
        user_after: after.iter().any(|row| row.user && !from_lead(row)),
    })
}

/// How far a lead's message may come before `owedSince` (the send is marked after it is typed).
pub(crate) const LEAD_TURN_SLACK_MS: i64 = LEAD_MESSAGE_SLACK_MS;

/// How new a session may be for a link to say its first task is the lead's (`owed`).
const FRESH_SESSION_MS: i64 = 60_000;

/// Whether a link's `owed` is honoured: only for a session just created (`agents create --lead
/// --task`, which links before it queues the task) that has not replied to anything yet.
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus review of #678, finding 2: `owed` came from the request, so a session could link an
/// existing session under itself with `"owed": true` and get the user's own next turn there
/// forwarded to it. A session that already exists starts owing nothing; its first reported turn
/// is the one the lead starts with a message.
pub(crate) fn honours_owed(session: &Value, now_ms: i64, has_reply: bool) -> bool {
    let created_ms = session
        .get("createdAt")
        .and_then(Value::as_str)
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis());
    !has_reply && created_ms.is_some_and(|created| now_ms - created <= FRESH_SESSION_MS)
}

/// What an endpoint answered, plus the sessions whose sidebar rows changed.
pub struct LeadLinksOutput {
    pub result: Value,
    pub changed_sessions: Vec<SessionKey>,
}

pub fn handle_lead_links_endpoint(
    caller: &Principal,
    db: &Connection,
    repository: &DomainRepository<'_>,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<LeadLinksOutput, DomainStateError> {
    let who = link_caller(caller);
    let caller_session = match &who {
        LinkCaller::Session(key) => Some(key.clone()),
        _ => None,
    };
    // Unlink and Stop leading only end links, so the user's own other computer may use them.
    let may_unlink = match &who {
        LinkCaller::User | LinkCaller::Session(_) => true,
        LinkCaller::Peer { owner_issued } => *owner_issued,
        LinkCaller::Refused => false,
    };
    let global_ref = |key: &SessionKey| create_global_session_ref(server_id, &key.0, &key.1);
    match text(params, "op").as_deref().unwrap_or("list") {
        "link" => {
            let linked = ids(params, "projectId", "sessionId")?;
            let lead = ids(params, "leadProjectId", "leadSessionId")?;
            if matches!(who, LinkCaller::Peer { .. } | LinkCaller::Refused) {
                return Err(DomainStateError::bad_request(NOT_ALLOWED));
            }
            if let Some(caller) = &caller_session {
                if caller != &lead {
                    return Err(DomainStateError::bad_request(
                        "A session can only link sessions to itself (--lead self). Only the user can link a session to another lead.",
                    ));
                }
            }
            for (key, what) in [(&linked, "session to link"), (&lead, "lead")] {
                if repository.get_session(&key.0, &key.1)?.is_none() {
                    return Err(DomainStateError::not_found(format!(
                        "The {what} was not found on this computer."
                    )));
                }
            }
            let owed = params.get("owed").and_then(Value::as_bool) == Some(true)
                && repository
                    .get_session(&linked.0, &linked.1)?
                    .is_some_and(|session| {
                        honours_owed(
                            &session,
                            chrono::Utc::now().timestamp_millis(),
                            crate::notification_feed::body::last_assistant_message(&session)
                                .is_some(),
                        )
                    });
            let previous = link_checked(db, caller_session.is_some(), &linked, &lead, owed)?;
            let mut changed = vec![linked.clone(), lead.clone()];
            if let Some(previous) = previous.filter(|previous| *previous != lead) {
                changed.push(previous);
            }
            Ok(LeadLinksOutput {
                result: json!({
                    "linked": true,
                    "globalRef": global_ref(&linked),
                    "leadGlobalRef": global_ref(&lead),
                    "leadTitle": session_title(repository, &lead),
                    "owed": owed,
                }),
                changed_sessions: changed,
            })
        }
        "unlink" => {
            let linked = ids(params, "projectId", "sessionId")?;
            if !may_unlink {
                return Err(DomainStateError::bad_request(NOT_ALLOWED));
            }
            let Some(link) = read_lead_link(db, &linked.0, &linked.1)? else {
                return Err(DomainStateError::not_found(
                    "That session is not linked to a lead.",
                ));
            };
            if let Some(caller) = &caller_session {
                if caller != &link.lead_key() && caller != &linked {
                    return Err(DomainStateError::bad_request(
                        "Only its lead, the session itself or the user can unlink it.",
                    ));
                }
            }
            remove_lead_link(db, &linked.0, &linked.1)?;
            Ok(LeadLinksOutput {
                result: json!({
                    "unlinked": true,
                    "globalRef": global_ref(&linked),
                    "leadGlobalRef": global_ref(&link.lead_key()),
                }),
                changed_sessions: vec![linked, link.lead_key()],
            })
        }
        "stopLeading" => {
            let lead = ids(params, "leadProjectId", "leadSessionId")?;
            if !may_unlink {
                return Err(DomainStateError::bad_request(NOT_ALLOWED));
            }
            if let Some(caller) = &caller_session {
                if caller != &lead {
                    return Err(DomainStateError::bad_request(
                        "Only the lead itself or the user can end its links.",
                    ));
                }
            }
            let mut changed = remove_links_of_lead(db, &lead.0, &lead.1)?;
            let count = changed.len();
            changed.push(lead);
            Ok(LeadLinksOutput {
                result: json!({ "unlinked": count }),
                changed_sessions: changed,
            })
        }
        "list" => {
            let links = match (text(params, "leadProjectId"), text(params, "leadSessionId")) {
                (Some(project_id), Some(session_id)) => {
                    links_of_lead(db, &project_id, &session_id)?
                }
                _ => list_lead_links(db)?,
            };
            let rows = links
                .iter()
                .map(|link| {
                    json!({
                        "globalRef": global_ref(&link.key()),
                        "title": session_title(repository, &link.key()),
                        "leadGlobalRef": global_ref(&link.lead_key()),
                        "leadTitle": session_title(repository, &link.lead_key()),
                        "reportOwed": link.owed_since.is_some(),
                        "linkedAt": link.created_at,
                    })
                })
                .collect::<Vec<_>>();
            Ok(LeadLinksOutput {
                result: json!({ "links": rows }),
                changed_sessions: Vec::new(),
            })
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown lead link op \"{other}\": use link, unlink, stopLeading or list."
        ))),
    }
}

/// The lead sent one of its linked sessions a message (`/api/sendSessionChatMessage` from the
/// lead's own session): the turn that message starts is owed a report.
pub fn note_lead_send(db: &Connection, caller: &Principal, target: &SessionKey) {
    let Some(caller) = calling_session(caller) else {
        return;
    };
    let Ok(Some(link)) = read_lead_link(db, &target.0, &target.1) else {
        return;
    };
    if link.lead_key() == caller {
        let _ = mark_lead_send(db, target);
    }
}

/// `note_lead_send` for a message route's body (its `projectId` and `sessionId`), from the
/// routes that take a message without typing it now: `agents send --queue`
/// (`/api/queueSessionChatPrompt`) and `--with-next` (held for the session's next message).
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus review of #678, finding 4: only a plain send marked the lead's turn owed, so a lead that
/// queued work for a busy linked session never got its report. The turn is owed from the moment
/// the queued or held message is submitted; `lead_turn_reply` then reports the reply that follows
/// it once it is typed.
pub fn note_lead_send_body(paths: &crate::paths::GxserverPaths, caller: &Principal, body: &Value) {
    let read = |field: &str| {
        body.pointer(&format!("/params/{field}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (read("projectId"), read("sessionId")) else {
        return;
    };
    if calling_session(caller).is_none() {
        return;
    }
    if let Ok(db) = crate::storage::open_gxserver_database(paths) {
        note_lead_send(&db, caller, &(project_id, session_id));
    }
}

fn session_key(object: &Map<String, Value>) -> SessionKey {
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (text("projectId"), text("sessionId"))
}

/// Present-only keys, like the coordinator fields: `leadProjectId` / `leadSessionId` on a linked
/// session (clients nest it under its lead), and `isLead` with `leadLinkCount` on a lead.
fn insert_projection(session: &mut Value, links: &[LeadLink]) {
    let Some(object) = session.as_object_mut() else {
        return;
    };
    let key = session_key(object);
    if let Some(link) = links.iter().find(|link| link.key() == key) {
        object.insert("leadProjectId".to_string(), json!(link.lead_project_id));
        object.insert("leadSessionId".to_string(), json!(link.lead_session_id));
    }
    let count = links.iter().filter(|link| link.lead_key() == key).count();
    if count > 0 {
        object.insert("isLead".to_string(), json!(true));
        object.insert("leadLinkCount".to_string(), json!(count));
    }
}

/// Adds the lead link fields to every session of a presentation snapshot.
pub fn insert_lead_presentation_payload(snapshot: &mut Value, db: &Connection) {
    let Ok(links) = list_lead_links(db) else {
        return;
    };
    if links.is_empty() {
        return;
    }
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        insert_projection(session, &links);
    }
}

/// The same fields on one presentation session (a delta).
pub fn insert_lead_session_projection(session: &mut Value, db: &Connection) {
    if let Ok(links) = list_lead_links(db) {
        if !links.is_empty() {
            insert_projection(session, &links);
        }
    }
}
