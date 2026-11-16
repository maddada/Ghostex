//! A coordinator's thread's to-dos: they become its coordinator's Needs you asks, and the user's
//! answer there reaches the thread's agent too.
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! User (Q62 "thread to coordinator"): `ghostex needs-you add` run inside a coordinator's thread
//! becomes that coordinator's Needs you ask, so the user sees one list per coordinator, and Done
//! must still reach the thread's agent. A session the user started themselves keeps its own, even
//! in a project with a coordinator. The ask goes on the line of work the thread is attached to (or
//! a new one named after the thread) and under the coordinator's own key: a thread's
//! `coordinator item add` was once stored under the thread's session, where no list showed it
//! (W32), which must not happen here.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::store::NewItem;
use crate::coordinators::{
    handle_work_request, list_work_items, mark_answer_list, SessionKey, ThreadRecord, WorkThread,
};
use crate::domain::{DomainRepository, DomainStateError};

/// The coordinator a running thread of `key` reports to, or `None` for any other session.
pub(crate) fn coordinator_of(db: &Connection, key: &SessionKey) -> Option<ThreadRecord> {
    crate::coordinators::read_thread(db, &key.0, &key.1)
        .ok()
        .flatten()
        .filter(|thread| !thread.is_resolved())
}

fn is_this_thread(thread: &WorkThread, key: &SessionKey) -> bool {
    thread.server_id.is_empty() && thread.project_id == key.0 && thread.session_id == key.1
}

/// Adds the thread's to-do as an ask of its coordinator's. Returns what `needs-you add` prints:
/// `{ tag, coordinator: true, item }`.
pub(crate) fn add_for_thread(
    db: &Connection,
    repository: &DomainRepository<'_>,
    thread: &ThreadRecord,
    new: NewItem,
) -> Result<Value, DomainStateError> {
    let thread_key = thread.key();
    let coordinator = thread.coordinator_key();
    let threads = crate::coordinators::cached_thread_rows(&coordinator);
    let items = list_work_items(db, &coordinator)?;
    let attached = items
        .iter()
        .filter(|item| item.dropped_at.is_none() && item.status != "finished")
        .filter(|item| item.threads.iter().any(|t| is_this_thread(t, &thread_key)))
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .map(|item| item.number);
    let number = match attached {
        Some(number) => number,
        None => {
            let title = repository
                .get_session(&thread_key.0, &thread_key.1)?
                .and_then(|session| {
                    session
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| new.title.clone());
            let mut params = Map::new();
            params.insert("op".into(), json!("add"));
            params.insert("title".into(), json!(title));
            params.insert("threadProjectId".into(), json!(thread_key.0));
            params.insert("threadSessionId".into(), json!(thread_key.1));
            let (added, _) =
                handle_work_request(db, &coordinator, &threads, &params, 0, &Default::default())?;
            added["number"].as_i64().unwrap_or_default()
        }
    };
    let ask_floor =
        crate::session_chat_open_questions::highest_question_tag(&coordinator.0, &coordinator.1)
            .max(super::api::highest_tag(db, &coordinator));
    let chat_tags =
        crate::session_chat_open_questions::asked_question_tags(&coordinator.0, &coordinator.1);
    let mut params = Map::new();
    params.insert("op".into(), json!("ask"));
    params.insert("item".into(), json!(format!("W{number}")));
    params.insert("text".into(), json!(new.title));
    if !new.detail.is_empty() {
        params.insert("detail".into(), json!(new.detail));
    }
    if !new.command.is_empty() {
        params.insert("command".into(), json!(new.command));
    }
    if !new.answers.is_empty() {
        params.insert("answers".into(), json!(mark_answer_list(&new.answers, "")));
    }
    params.insert(
        "fromThread".into(),
        json!(WorkThread {
            server_id: String::new(),
            project_id: thread_key.0.clone(),
            session_id: thread_key.1.clone(),
        }),
    );
    let (item, _) =
        handle_work_request(db, &coordinator, &threads, &params, ask_floor, &chat_tags)?;
    let tag = item["asks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|ask| ask["open"] == true)
        .filter_map(|ask| ask["tag"].as_str())
        .max_by_key(|tag| tag.trim_start_matches('Q').parse::<u64>().unwrap_or(0))
        .unwrap_or_default()
        .to_string();
    Ok(json!({
        "tag": tag,
        "title": params["text"],
        "coordinator": true,
        "coordinatorProjectId": coordinator.0,
        "coordinatorSessionId": coordinator.1,
        "item": item["ref"],
    }))
}

/// The thread's to-dos still open on its coordinator, as `needs-you list` prints them.
pub(crate) fn list_for_thread(
    db: &Connection,
    thread: &ThreadRecord,
) -> Result<Vec<Value>, DomainStateError> {
    let thread_key = thread.key();
    let items = list_work_items(db, &thread.coordinator_key())?;
    Ok(items
        .iter()
        .filter(|item| item.dropped_at.is_none())
        .flat_map(|item| {
            item.asks
                .iter()
                .filter(|ask| ask.is_open())
                .filter(|ask| {
                    ask.from_thread
                        .as_ref()
                        .is_some_and(|from| is_this_thread(from, &thread_key))
                })
                .map(|ask| {
                    json!({
                        "tag": ask.tag,
                        "title": ask.text,
                        "detail": ask.detail,
                        "command": ask.command,
                        "askedAt": ask.asked_at,
                        "item": item.reference(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect())
}

/// The thread's own `needs-you done` / `drop`: settles its ask on the coordinator (done, or
/// withdrawn for a drop). Returns the item's reference.
pub(crate) fn settle_for_thread(
    db: &Connection,
    thread: &ThreadRecord,
    tag: &str,
    withdraw: bool,
) -> Result<Option<String>, DomainStateError> {
    let open = list_for_thread(db, thread)?;
    let Some(row) = open.iter().find(|row| {
        row["tag"]
            .as_str()
            .is_some_and(|own| own.eq_ignore_ascii_case(tag))
    }) else {
        return Ok(None);
    };
    let reference = row["item"].as_str().unwrap_or_default().to_string();
    let coordinator = thread.coordinator_key();
    let mut params = Map::new();
    params.insert("op".into(), json!("settle"));
    params.insert("item".into(), json!(reference));
    params.insert("ask".into(), row["tag"].clone());
    params.insert("withdraw".into(), json!(withdraw));
    handle_work_request(
        db,
        &coordinator,
        &crate::coordinators::cached_thread_rows(&coordinator),
        &params,
        0,
        &Default::default(),
    )?;
    Ok(Some(reference))
}

/// An open ask of the coordinator `coordinator` that came from a thread, before the user's answer
/// (`answer`, `reply` or `cancel` on `/api/coordinatorBoard` or `/api/coordinatorWork`) settles it.
pub(crate) struct ThreadAsk {
    thread: SessionKey,
    tag: String,
}

/// The thread ask an answer request names, if it is one and still open.
pub(crate) fn thread_ask_of(
    db: &Connection,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<ThreadAsk> {
    if !matches!(
        endpoint_path,
        "/api/coordinatorBoard" | "/api/coordinatorWork"
    ) {
        return None;
    }
    let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or_default();
    if !matches!(text("op"), "answer" | "reply" | "cancel") || text("ask").is_empty() {
        return None;
    }
    let coordinator = (text("projectId").to_string(), text("sessionId").to_string());
    let number = crate::coordinators::item_number_of(text("item"))?;
    let items = list_work_items(db, &coordinator).ok()?;
    let item = items.iter().find(|item| item.number == number)?;
    let ask = item
        .asks
        .iter()
        .find(|ask| ask.is_open() && ask.tag.eq_ignore_ascii_case(text("ask")))?;
    let from = ask.from_thread.as_ref()?;
    Some(ThreadAsk {
        thread: (from.project_id.clone(), from.session_id.clone()),
        tag: ask.tag.clone(),
    })
}

/// After the answer request went through: tells the thread what the user said, as a message
/// queued after its current turn ("Q61: Done"), the way the coordinator hears it.
pub(crate) fn forward_answer(
    db: &Connection,
    ask: &ThreadAsk,
    params: &Map<String, Value>,
) -> Option<SessionKey> {
    let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or_default();
    let message = if text("op") == "cancel" {
        format!(
            "{}: The user dismissed this to-do; it is no longer needed.",
            ask.tag
        )
    } else {
        let answer = text("answer").trim();
        if answer.is_empty() {
            return None;
        }
        format!("{}: {answer}", ask.tag)
    };
    crate::session_chat_queue::enqueue_session_chat_prompt(
        db,
        &ask.thread.0,
        &ask.thread.1,
        &message,
        false,
    )
    .ok()?;
    Some(ask.thread.clone())
}
