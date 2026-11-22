use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};

use super::*;
use crate::{
    domain::DomainRepository,
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

pub(super) fn open_test_database() -> (tempfile::TempDir, crate::storage::GxserverDatabase) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    (temp, db)
}

pub(super) fn write_metadata_value(db: &Connection, key: &str, value: Value) {
    db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            key,
            serde_json::to_string(&value).expect("serialize metadata value"),
            "2026-06-19T00:00:00.000Z"
        ],
    )
    .expect("write metadata value");
}

pub(super) fn create_agent_session(
    repository: &DomainRepository<'_>,
    agent_id: &str,
    agent_session_id: &str,
    project_path: &Path,
) -> (LifecycleParams, Value) {
    let project = repository
        .create_project(
            json!({
                "name": "Rename Test Project",
                "path": project_path.to_string_lossy()
            })
            .as_object()
            .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": agent_id,
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": agent_id,
                    "agentSessionId": agent_session_id,
                    "titleSource": "placeholder"
                },
                "title": create_agent_session_default_title(None, Some(agent_id))
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("session project id")
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    (lifecycle, session)
}

pub(super) fn create_codex_agent_session(
    repository: &DomainRepository<'_>,
    agent_session_id: &str,
    project_path: &Path,
) -> (LifecycleParams, Value) {
    create_agent_session(repository, "codex", agent_session_id, project_path)
}

pub(super) fn create_pi_agent_session_without_launch_lock(
    repository: &DomainRepository<'_>,
) -> (LifecycleParams, Value) {
    let project = repository
        .create_project(
            json!({ "name": "Pi Lock Project", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "pi",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "pi",
                    "titleSource": "terminal-auto"
                },
                "title": "Pi Investigation"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("session project id")
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    (lifecycle, session)
}
