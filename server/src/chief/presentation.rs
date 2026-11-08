//! What Chief adds to other parts of gxserver: `chiefEarlier` on its earlier chats in every
//! presentation (so clients list them under Chief's row), and the folder-trust answer for Chief's
//! own sessions at the home folder.

use rusqlite::Connection;
use serde_json::{json, Value};

use super::record::read_record;
use crate::domain::DomainRepository;

fn key_of(session: &Value) -> (String, String) {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (text("projectId"), text("sessionId"))
}

fn mark(session: &mut Value) {
    if let Some(object) = session.as_object_mut() {
        object.insert("chiefEarlier".to_string(), json!(true));
    }
}

/// Flags Chief's earlier chats in a presentation snapshot.
pub(crate) fn insert_chief_presentation_payload(snapshot: &mut Value, db: &Connection) {
    let Some(record) = read_record(db).filter(|record| !record.earlier.is_empty()) else {
        return;
    };
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        let key = key_of(session);
        if key != record.key() && record.owns(&key) {
            mark(session);
        }
    }
}

/// The same flag on one presentation session (a delta).
pub(crate) fn insert_chief_session_projection(session: &mut Value, db: &Connection) {
    let key = key_of(session);
    if read_record(db).is_some_and(|record| key != record.key() && record.owns(&key)) {
        mark(session);
    }
}

/// CDXC:Chief 2026-11-08 DECISION:
/// User (Q1914): Chief's own chat runs at ~, and "Claude Code never saves trust for the home folder, so Ghostex must answer that trust prompt itself for Chief's own sessions at ~ (and only those), so New Chat never stops at it." Chief's current chat and its earlier chats (which wake at ~ too) are answered like a remembered folder; any other session at the home folder keeps the prompt and the chat's folder-trust card.
/// SEE-ALSO: server/src/session_chat_trust_memory.rs (`session_folders_remembered`, the funnel that answers it), server/src/agent_folder_trust/launch.rs (never writes trust for the home folder).
pub(crate) fn chief_home_session_trusted(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> bool {
    let key = (project_id.to_string(), session_id.to_string());
    if !read_record(repository.connection()).is_some_and(|record| record.owns(&key)) {
        return false;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let folder = session
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(str::to_string)
        .or_else(|| {
            repository
                .get_project(project_id)
                .ok()
                .flatten()
                .and_then(|project| {
                    project
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
        });
    folder.is_some_and(|folder| super::is_home_folder(&folder))
}
