use serde_json::{json, Value};

use crate::{
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

pub(super) fn project(project_id: &str, name: &str, is_pinned: bool, is_favorite: bool) -> Value {
    json!({
        "createdAt": "2026-06-15T09:55:00.000Z",
        "isFavorite": is_favorite,
        "isPinned": is_pinned,
        "name": name,
        "projectId": project_id,
        "updatedAt": "2026-06-15T09:55:00.000Z",
    })
}

pub(super) fn session(
    project_id: &str,
    session_id: &str,
    title: &str,
    lifecycle_state: &str,
    sidebar_order: f64,
) -> Value {
    json!({
        "createdAt": "2026-06-15T09:55:00.000Z",
        "isFavorite": false,
        "isPinned": false,
        "kind": "terminal",
        "lifecycleState": lifecycle_state,
        "projectId": project_id,
        "providerState": { "lifecycleState": "missing", "provider": "zmx" },
        "runtimeSettings": {},
        "sessionId": session_id,
        "sidebarOrder": sidebar_order,
        "surface": "workspace",
        "title": title,
        "updatedAt": "2026-06-15T09:55:00.000Z",
        "zmxName": format!("S7k-{project_id}-{session_id}"),
    })
}

pub(super) fn previous_session(
    session_id: &str,
    title: &str,
    lifecycle_state: &str,
    surface: &str,
    probed_at: Option<&str>,
    updated_at: &str,
    last_active_at: &str,
) -> Value {
    let provider_state = match probed_at {
        Some(probed_at) => json!({
            "lifecycleState": "missing",
            "probedAt": probed_at,
            "provider": "zmx",
            "zmxName": format!("S7k-P100-{session_id}"),
        }),
        None => json!({
            "lifecycleState": "missing",
            "provider": "zmx",
            "zmxName": format!("S7k-P100-{session_id}"),
        }),
    };
    json!({
        "agentId": "codex",
        "createdAt": "2026-06-01T08:00:00.000Z",
        "isFavorite": false,
        "isPinned": false,
        "kind": "agent",
        "lastActiveAt": last_active_at,
        "lifecycleState": lifecycle_state,
        "projectId": "P100",
        "providerState": provider_state,
        "runtimeSettings": {
            "titleSource": if title == "Search by Text" { "placeholder" } else { "terminal-auto" },
        },
        "sessionId": session_id,
        "surface": surface,
        "title": title,
        "updatedAt": updated_at,
        "zmxName": format!("S7k-P100-{session_id}"),
    })
}

pub(super) fn open_test_database() -> (tempfile::TempDir, crate::storage::GxserverDatabase) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    (temp, db)
}
