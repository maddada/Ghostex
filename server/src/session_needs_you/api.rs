//! `/api/sessionNeedsYou` (the `ghostex needs-you` verbs), the user's Done, answer, Reply and
//! Dismiss from a to-do's dock row (which arrive as the coordinator ask's own call, see
//! [`route_board_answer`]), the `needsYou` key every chat frame of the session carries, and the
//! alert a turn end gives for a new to-do.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use super::store::{self, NeedsYouRecord, NewItem};
use crate::domain::{DomainRepository, DomainStateError};
use crate::security::Principal;
use crate::server::AppState;

pub(crate) const SESSION_NEEDS_YOU_ENDPOINT: &str = "/api/sessionNeedsYou";

fn text(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn bad(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message)
}

/// The frames' copy of each session's open to-dos, by `project\u{1f}session`, so a frame never
/// reads the database: loaded on a session's first frame and replaced on every write.
fn frames() -> &'static Mutex<HashMap<String, Value>> {
    static FRAMES: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
    FRAMES.get_or_init(Default::default)
}

fn frame_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}\u{1f}{session_id}")
}

fn remember(project_id: &str, session_id: &str, record: &NeedsYouRecord) {
    if let Ok(mut frames) = frames().lock() {
        frames.insert(
            frame_key(project_id, session_id),
            store::frame_value(record),
        );
    }
}

/// Adds `needsYou` (`{ items: [{ tag, title, detail, command, answers, askedAt }] }`) to a chat
/// frame or read result. Always present, so an empty list clears a row on every client; a client
/// from before 2026-11-16 ignores the key.
pub(crate) fn insert_needs_you(frame: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    let key = frame_key(project_id, session_id);
    let cached = frames()
        .lock()
        .ok()
        .and_then(|frames| frames.get(&key).cloned());
    let mut value = cached.unwrap_or_else(|| {
        let paths = crate::paths::get_gxserver_paths(None);
        let record = crate::storage::open_gxserver_database(&paths)
            .ok()
            .and_then(|db| {
                DomainRepository::new(&db, "")
                    .get_session(project_id, session_id)
                    .ok()
                    .flatten()
            })
            .map(|session| NeedsYouRecord::of(&session))
            .unwrap_or_default();
        let value = store::frame_value(&record);
        if let Ok(mut frames) = frames().lock() {
            frames.insert(key, value.clone());
        }
        value
    });
    let command_runs = super::runner::runs_for_session(project_id, session_id);
    if !command_runs.is_empty() {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("commandRuns".to_string(), Value::Object(command_runs));
        }
    }
    frame.insert("needsYou".to_string(), value);
}

/// The tags of the open to-dos, in memory: wakes an SSH-only long-poller when a row opens or closes.
pub(crate) fn needs_you_identity(project_id: &str, session_id: &str) -> String {
    let items = frames()
        .lock()
        .ok()
        .and_then(|frames| frames.get(&frame_key(project_id, session_id)).cloned())
        .map(|value| value["items"].to_string())
        .unwrap_or_default();
    let runs = super::runner::identity_for_session(project_id, session_id);
    if runs.is_empty() {
        items
    } else {
        format!("{items}#{runs}")
    }
}

/// The highest tag this session's to-dos ever took (`Q61` is 61), 0 for none, so a coordinator's
/// own asks and `ghostex coordinator next-tag` never give one of them again.
pub(crate) fn highest_tag(db: &rusqlite::Connection, key: &crate::coordinators::SessionKey) -> u64 {
    DomainRepository::new(db, "")
        .get_session(&key.0, &key.1)
        .ok()
        .flatten()
        .map_or(0, |session| NeedsYouRecord::of(&session).last)
}

/// Who may keep a session's to-dos: the session's own agent, or the CLI in the user's own terminal
/// (the token file, the desktop's owner session) naming it with `--session`. Any other session,
/// a connected computer, a phone, a guest or an extension may not.
fn require_writer(
    caller: &Principal,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    match caller {
        Principal::Session {
            project_id: own_project,
            session_id: own_session,
            ..
        } if own_project == project_id && own_session == session_id => Ok(()),
        Principal::LocalAgent | Principal::Owner { .. } => Ok(()),
        _ => Err(DomainStateError {
            code: "forbidden",
            message: "Only the agent of that session can keep its to-dos for the user.".to_string(),
        }),
    }
}

fn session_of(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| bad("No such session."))
}

/// The floor a new to-do's tag follows: the highest Q tag the session's chat asked, and a
/// coordinator's own asks.
fn tag_floor(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> u64 {
    let key = (project_id.to_string(), session_id.to_string());
    let asks = crate::coordinators::list_work_items(db, &key)
        .unwrap_or_default()
        .iter()
        .flat_map(|item| item.asks.iter())
        .filter_map(|ask| ask.tag.strip_prefix('Q')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    crate::session_chat_open_questions::highest_question_tag(project_id, session_id).max(asks)
}

fn item_json(item: &store::NeedsYouItem) -> Value {
    let mut value = json!({
        "tag": item.tag,
        "title": item.title,
        "detail": item.detail,
        "command": item.command,
        "answers": item.answers,
        "askedAt": item.asked_at,
    });
    if store::is_snooze_active(item.snoozed_until.as_deref()) {
        if let Some(until) = &item.snoozed_until {
            value["snoozedUntil"] = json!(until);
        }
    }
    value
}

/// `/api/sessionNeedsYou {projectId, sessionId, op}`: `add` (`title`, `detail`, `command`,
/// `answers`), `list` (`all` for the settled ones too), `done` or `drop` (`tag`). Returns the
/// answer and whether the session's to-dos changed.
pub(crate) fn handle_needs_you_request(
    state: &AppState,
    caller: &Principal,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (project_id, session_id) = (text(params, "projectId"), text(params, "sessionId"));
    if project_id.is_empty() || session_id.is_empty() {
        return Err(bad("projectId and sessionId are required."));
    }
    require_writer(caller, &project_id, &session_id)?;
    let op = text(params, "op");
    let key = (project_id.clone(), session_id.clone());
    let thread = super::thread::coordinator_of(db, &key);
    match op.as_str() {
        "add" => {
            let new = NewItem {
                title: text(params, "title"),
                detail: text(params, "detail"),
                command: text(params, "command"),
                answers: params
                    .get("answers")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            }
            .checked()?;
            if let Some(thread) = thread {
                let added = super::thread::add_for_thread(db, repository, &thread, new)?;
                let coordinator = thread.coordinator_key();
                for (project_id, session_id) in
                    crate::coordinators::refresh_coordinator_panels(db, repository)
                {
                    crate::server::republish_session_chat(state, &project_id, &session_id);
                }
                let _ = crate::server::schedule_presentation_session_delta(
                    state,
                    db,
                    repository,
                    &coordinator.0,
                    &coordinator.1,
                );
                return Ok(added);
            }
            let _lock = store::write_lock();
            let session = session_of(repository, &project_id, &session_id)?;
            let mut record = NeedsYouRecord::of(&session);
            let floor = tag_floor(db, &project_id, &session_id);
            let item = store::add(&mut record, new, floor, &crate::coordinators::now_iso())?;
            store::write(db, &project_id, &session_id, &record)?;
            changed(state, db, repository, &project_id, &session_id, &record);
            Ok(json!({ "tag": item.tag, "title": item.title, "coordinator": false }))
        }
        "list" => {
            if let Some(thread) = thread {
                return Ok(json!({
                    "items": super::thread::list_for_thread(db, &thread)?,
                    "coordinator": true,
                }));
            }
            let record = NeedsYouRecord::of(&session_of(repository, &project_id, &session_id)?);
            let mut out = json!({
                "items": record.open.iter().map(item_json).collect::<Vec<_>>(),
                "coordinator": false,
            });
            if params.get("all").and_then(Value::as_bool) == Some(true) {
                out["settled"] = json!(record.settled);
            }
            Ok(out)
        }
        "done" | "drop" => {
            let tag = text(params, "tag").to_ascii_uppercase();
            if tag.is_empty() {
                return Err(bad("Name the to-do: Q61."));
            }
            if let Some(thread) = thread {
                let Some(item) = super::thread::settle_for_thread(db, &thread, &tag, op == "drop")?
                else {
                    return Err(bad(format!(
                        "{tag} is not an open to-do of this thread. `ghostex needs-you list` shows them."
                    )));
                };
                for (project_id, session_id) in
                    crate::coordinators::refresh_coordinator_panels(db, repository)
                {
                    crate::server::republish_session_chat(state, &project_id, &session_id);
                }
                return Ok(json!({ "tag": tag, "settled": true, "item": item }));
            }
            let _lock = store::write_lock();
            let session = session_of(repository, &project_id, &session_id)?;
            let mut record = NeedsYouRecord::of(&session);
            let outcome = if op == "drop" { "dropped" } else { "done" };
            if record
                .settle(&tag, outcome, "", &crate::coordinators::now_iso())
                .is_none()
            {
                let how = record
                    .settled_of(&tag)
                    .map(|settled| match settled.outcome.as_str() {
                        "answered" if !settled.answer.is_empty() => {
                            format!(" The user already answered it: {}.", settled.answer)
                        }
                        "dismissed" => " The user dismissed it.".to_string(),
                        _ => " It was already cleared.".to_string(),
                    })
                    .unwrap_or_default();
                return Err(bad(format!("{tag} is not an open to-do.{how}")));
            }
            store::write(db, &project_id, &session_id, &record)?;
            changed(state, db, repository, &project_id, &session_id, &record);
            Ok(json!({ "tag": tag, "settled": true }))
        }
        other => Err(bad(format!(
            "Unknown needs-you request {other}. Use add, list, done or drop."
        ))),
    }
}

/// After a write: the frames' copy, the chat on every client, and the sidebar row's count.
fn changed(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    record: &NeedsYouRecord,
) {
    remember(project_id, session_id, record);
    crate::server::republish_session_chat(state, project_id, session_id);
    let _ = crate::server::schedule_presentation_session_delta(
        state, db, repository, project_id, session_id,
    );
}

/// Whether `reference` names a to-do (`Q61`) rather than a work item (`W7`) or board card (`B7`).
fn is_todo_reference(reference: &str) -> bool {
    reference
        .strip_prefix(['Q', 'q'])
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// The user's answer on a to-do's dock row. The row reuses the coordinator ask's Done, answer,
/// Reply and Dismiss (gx-chat-core `coordinator_board::answer`, the desktop and phone Reply boxes),
/// which record it with `/api/coordinatorBoard {op, item, answer}` and then send its message
/// ("Q61: Done") themselves; a to-do names itself by its tag as `item`. Recorded once: a second
/// answer from any client is refused, as an ask's is. `None` when the call is not about a to-do.
///
/// CDXC:SessionChat 2026-11-16 DECISION:
/// User (Q61 "tell agent"): Done clears the to-do and sends "Q<n>: Done" to the chat, and a busy
/// agent gets it after its turn like any queued message. The message is the chat's own send, the
/// one a coordinator ask's answer uses, so it queues behind a working agent the same way.
pub(crate) fn route_board_answer(
    state: &AppState,
    endpoint_path: &str,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Option<Value>, DomainStateError> {
    if endpoint_path != "/api/coordinatorBoard" || !is_todo_reference(&text(params, "item")) {
        return Ok(None);
    }
    let (project_id, session_id) = (text(params, "projectId"), text(params, "sessionId"));
    let tag = text(params, "item").to_ascii_uppercase();
    let op = text(params, "op");
    let answer = text(params, "answer");
    let _lock = store::write_lock();
    let session = session_of(repository, &project_id, &session_id)?;
    let mut record = NeedsYouRecord::of(&session);
    match op.as_str() {
        "snooze" => {
            let hours = params
                .get("hours")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 24 * 14);
            let until = (chrono::Utc::now() + chrono::Duration::hours(hours as i64))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            if !record.set_snoozed(&tag, Some(until.clone())) {
                return Err(bad(format!("{tag} is not an open to-do.")));
            }
            store::write(db, &project_id, &session_id, &record)?;
            changed(state, db, repository, &project_id, &session_id, &record);
            return Ok(Some(json!({ "item": tag, "snoozedUntil": until })));
        }
        "unsnooze" => {
            if !record.set_snoozed(&tag, None) {
                return Err(bad(format!("{tag} is not an open to-do.")));
            }
            store::write(db, &project_id, &session_id, &record)?;
            changed(state, db, repository, &project_id, &session_id, &record);
            return Ok(Some(json!({ "item": tag, "snoozedUntil": Value::Null })));
        }
        "restore" => {
            if record.restore(&tag) {
                store::write(db, &project_id, &session_id, &record)?;
                changed(state, db, repository, &project_id, &session_id, &record);
            }
            return Ok(Some(json!({ "item": tag, "restored": true })));
        }
        _ => {}
    }
    let (outcome, words) = match op.as_str() {
        "cancel" => ("dismissed", String::new()),
        "answer" | "reply" if !answer.is_empty() => ("answered", answer.clone()),
        "answer" | "reply" => return Err(bad("Say the answer.")),
        other => return Err(bad(format!("A to-do takes no {other}."))),
    };
    if record
        .settle(&tag, outcome, &words, &crate::coordinators::now_iso())
        .is_none()
    {
        let given = record
            .settled_of(&tag)
            .map(|settled| {
                if settled.answer.is_empty() {
                    settled.outcome.clone()
                } else {
                    settled.answer.clone()
                }
            })
            .unwrap_or_else(|| "cleared".to_string());
        return Err(bad(format!("{tag} was already answered: {given}.")));
    }
    store::write(db, &project_id, &session_id, &record)?;
    changed(state, db, repository, &project_id, &session_id, &record);
    // Mark as read (the to-do's Dismiss) sends the agent nothing now: its next message says so
    // (session_chat_dismissed.rs, where the user's decision lives).
    if outcome == "dismissed" {
        let title: String = record
            .settled_of(&tag)
            .map(|settled| settled.title.replace('"', "'").chars().take(80).collect())
            .unwrap_or_default();
        let name = if title.trim().is_empty() {
            format!("{tag} (marked read)")
        } else {
            format!("{tag} \"{}\" (marked read)", title.trim())
        };
        crate::session_chat_dismissed::record(db, &project_id, &session_id, &name);
    }
    Ok(Some(
        json!({ "item": tag, "answered": words, "settled": true }),
    ))
}

/// A turn ended (`ended_idle` when it ended without attention): the to-dos it added alert once,
/// as a turn's new open questions do (its Finished row turns into Needs input, and a turn that
/// ended idle is raised to attention). Blocking.
///
/// CDXC:Notifications 2026-11-16 DECISION:
/// User (Q63 "like question"): an open to-do counts like an open question: the pink sidebar dot,
/// and an alert on the computer and the phone under the user's Notifications settings. The count
/// is in `pendingQuestionCount`, which gxserver works out, so phones from 11.2.1 on show the dot
/// without an update.
pub(crate) fn turn_ended(state: &AppState, project_id: &str, session_id: &str, ended_idle: bool) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _lock = store::write_lock();
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let mut record = NeedsYouRecord::of(&session);
    let newly: Vec<String> = record
        .open
        .iter()
        .filter(|item| !item.notified)
        .map(|item| item.title.clone())
        .collect();
    if newly.is_empty() {
        return;
    }
    for item in &mut record.open {
        item.notified = true;
    }
    if store::write(&db, project_id, session_id, &record).is_err() {
        return;
    }
    remember(project_id, session_id, &record);
    let body = match newly.as_slice() {
        [one] => format!("{}: {one}", super::INBOX_FULL_NAME),
        [first, rest @ ..] => format!(
            "{} {} in {}: {first}",
            rest.len() + 1,
            super::ASKS_GROUP_NAME.to_ascii_lowercase(),
            super::INBOX_FULL_NAME
        ),
        [] => return,
    };
    crate::session_chat_open_questions::notify_waiting(
        state,
        &db,
        &repository,
        project_id,
        session_id,
        &body,
        ended_idle,
    );
}
