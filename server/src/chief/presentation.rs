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

/// While Main Hub is off nothing of Chief of Staff shows: its current and earlier chats are left
/// out of every list (CDXC:Chief 2026-11-08 in packages/settings-catalog/src/data/
/// official_extensions.rs).
fn hide(session: &mut Value) {
    if let Some(object) = session.as_object_mut() {
        object.insert("visibleInSidebarByDefault".to_string(), json!(false));
        // The phone's summary drops these rows by this flag (ghostex_cli/sessions/mobile_summary.rs).
        object.insert("mainHubHidden".to_string(), json!(true));
        object.remove("isChief");
    }
}

/// Flags Chief's earlier chats in a presentation snapshot, or hides all its chats while Main Hub
/// is off.
pub(crate) fn insert_chief_presentation_payload(snapshot: &mut Value, db: &Connection) {
    let Some(record) = read_record(db) else {
        return;
    };
    let shown = super::chief_shown();
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        let key = key_of(session);
        if !shown && record.owns(&key) {
            hide(session);
        } else if key != record.key() && record.owns(&key) {
            mark(session);
        }
    }
}

/// The same on one presentation session (a delta).
pub(crate) fn insert_chief_session_projection(session: &mut Value, db: &Connection) {
    let key = key_of(session);
    let Some(record) = read_record(db).filter(|record| record.owns(&key)) else {
        return;
    };
    if !super::chief_shown() {
        hide(session);
    } else if key != record.key() {
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
