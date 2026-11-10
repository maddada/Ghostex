//! The coordinator fields every client reads from a presentation session.

use std::collections::{BTreeSet, HashMap};

use rusqlite::Connection;
use serde_json::{json, Value};

use super::records::{list_coordinators, read_coordinator, read_thread, SessionKey, ThreadRecord};
use super::state::ThreadProgress;
use crate::domain::DomainRepository;
use crate::presentation::now_iso;

/// CDXC:Coordinators 2026-09-30 WHY:
/// Clients nest threads under their coordinator and group them by state from these fields alone. Every field is computed from the session's OWN row plus its coordinator link, so the per-row caches in gx-core stay valid and the fields refresh with every delta the session already publishes (a hook flipping it to working republishes its state too). Present-only keys: a session that is neither a coordinator nor a thread carries none of them, exactly like a daemon that predates the feature.
/// SEE-ALSO: packages/gx-protocol/src/presentation.rs (PresentationSession), packages/gx-core/src/sidebar_view/ (nesting).
fn insert_fields(
    session: &mut Value,
    is_coordinator: bool,
    is_chief: bool,
    is_main: bool,
    thread: Option<(&ThreadRecord, Option<&Value>)>,
    generated_at: &str,
) {
    let Some(object) = session.as_object_mut() else {
        return;
    };
    if is_coordinator {
        object.insert("coordinatorRole".to_string(), json!("coordinator"));
    }
    // Its project's main coordinator on this computer (main_coordinator.rs); additive, so older
    // clients ignore it.
    if is_coordinator && is_main {
        object.insert("isMainCoordinator".to_string(), json!(true));
    }
    // CDXC:Chief 2026-11-08 DECISION: User (Q1734): Chief is "its own row but under spaces bar", so its session is flagged here for every client to lift it out of its project's list.
    if is_chief {
        object.insert("isChief".to_string(), json!(true));
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
    // CDXC:Coordinators 2026-11-10 WHY: every snapshot read all 1,104 thread rows (3.9 MB of
    // text) for the few its sessions are; the resolved ones now come from memory unless one
    // changed (thread_reads.rs).
    let Ok(all_threads) = super::read_all_threads(db) else {
        return;
    };
    if coordinators.is_empty() && all_threads.is_empty() {
        return;
    }
    let coordinator_keys = coordinators
        .iter()
        .map(|record| record.key())
        .collect::<BTreeSet<_>>();
    let threads = all_threads
        .ordered()
        .into_iter()
        .map(|thread| {
            (
                (thread.project_id.as_str(), thread.session_id.as_str()),
                thread,
            )
        })
        .collect::<HashMap<_, _>>();
    let generated_at = now_iso();
    let chief = crate::chief::read_record(db).map(|record| record.key());
    let mains = super::main_coordinator::main_coordinators(db, repository);
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        let key = session_key(session);
        let thread = threads.get(&(key.0.as_str(), key.1.as_str())).copied();
        let is_coordinator = coordinator_keys.contains(&key);
        if !is_coordinator && thread.is_none() {
            continue;
        }
        let raw = thread.and_then(|_| repository.get_session(&key.0, &key.1).ok().flatten());
        let is_main = super::main_coordinator::is_main_in(&mains, session);
        insert_fields(
            session,
            is_coordinator,
            chief.as_ref() == Some(&key),
            is_main,
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
    repository: &DomainRepository<'_>,
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
    let is_chief = crate::chief::read_record(db)
        .is_some_and(|record| record.key() == (project_id.clone(), session_id.clone()));
    let is_main = is_coordinator
        && super::main_coordinator::main_coordinator_of(db, repository, &project_id)
            .is_some_and(|main| main.1 == session_id);
    insert_fields(
        presentation_session,
        is_coordinator,
        is_chief,
        is_main,
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
