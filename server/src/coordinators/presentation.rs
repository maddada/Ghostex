//! The coordinator fields every client reads from a presentation session.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;
use serde_json::{json, Value};

use super::records::{
    list_coordinators, list_threads, read_coordinator, read_thread, SessionKey, ThreadRecord,
};
use super::state::ThreadProgress;
use crate::domain::DomainRepository;
use crate::presentation::now_iso;

/// CDXC:Coordinators 2026-09-30 WHY:
/// Clients nest threads under their coordinator and group them by state from these fields alone. Every field is computed from the session's OWN row plus its coordinator link, so the per-row caches in gx-core stay valid and the fields refresh with every delta the session already publishes (a hook flipping it to working republishes its state too). Present-only keys: a session that is neither a coordinator nor a thread carries none of them, exactly like a daemon that predates the feature.
/// SEE-ALSO: packages/gx-protocol/src/presentation.rs (PresentationSession), packages/gx-core/src/sidebar_view/ (nesting).
fn insert_fields(
    session: &mut Value,
    is_coordinator: bool,
    thread: Option<(&ThreadRecord, Option<&Value>)>,
    generated_at: &str,
) {
    let Some(object) = session.as_object_mut() else {
        return;
    };
    if is_coordinator {
        object.insert("coordinatorRole".to_string(), json!("coordinator"));
    }
    if let Some((thread, raw_session)) = thread {
        if !is_coordinator {
            object.insert("coordinatorRole".to_string(), json!("thread"));
        }
        object.insert(
            "coordinatorProjectId".to_string(),
            json!(thread.coordinator_project_id),
        );
        object.insert(
            "coordinatorSessionId".to_string(),
            json!(thread.coordinator_session_id),
        );
        object.insert(
            "coordinatorThreadState".to_string(),
            json!(super::live::classify_thread_live(
                raw_session,
                ThreadProgress::of(thread),
                generated_at,
            )
            .as_str()),
        );
    }
}

fn session_key(session: &Value) -> SessionKey {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (text("projectId"), text("sessionId"))
}

/// Adds the coordinator fields to every session of a presentation snapshot.
pub fn insert_coordinator_presentation_payload(
    snapshot: &mut Value,
    db: &Connection,
    repository: &DomainRepository<'_>,
) {
    let Ok(coordinators) = list_coordinators(db) else {
        return;
    };
    let Ok(threads) = list_threads(db) else {
        return;
    };
    if coordinators.is_empty() && threads.is_empty() {
        return;
    }
    let coordinator_keys = coordinators
        .iter()
        .map(|record| record.key())
        .collect::<BTreeSet<_>>();
    let threads = threads
        .into_iter()
        .map(|thread| (thread.key(), thread))
        .collect::<BTreeMap<_, _>>();
    let generated_at = now_iso();
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        let key = session_key(session);
        let thread = threads.get(&key);
        let is_coordinator = coordinator_keys.contains(&key);
        if !is_coordinator && thread.is_none() {
            continue;
        }
        let raw = thread.and_then(|_| repository.get_session(&key.0, &key.1).ok().flatten());
        insert_fields(
            session,
            is_coordinator,
            thread.map(|thread| (thread, raw.as_ref())),
            &generated_at,
        );
        if is_coordinator {
            super::remote_threads::insert_coordinator_remote_threads(session, db, &key);
        }
    }
}

/// The same fields on one presentation session (a delta), from its raw row.
pub fn insert_coordinator_session_projection(
    presentation_session: &mut Value,
    db: &Connection,
    raw_session: &Value,
) {
    let (project_id, session_id) = session_key(presentation_session);
    let is_coordinator = read_coordinator(db, &project_id, &session_id)
        .ok()
        .flatten()
        .is_some();
    let thread = read_thread(db, &project_id, &session_id).ok().flatten();
    if !is_coordinator && thread.is_none() {
        return;
    }
    insert_fields(
        presentation_session,
        is_coordinator,
        thread.as_ref().map(|thread| (thread, Some(raw_session))),
        &now_iso(),
    );
    if is_coordinator {
        super::remote_threads::insert_coordinator_remote_threads(
            presentation_session,
            db,
            &(project_id, session_id),
        );
    }
}
