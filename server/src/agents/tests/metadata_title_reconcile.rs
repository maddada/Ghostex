use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;

#[test]
fn terminal_title_capture_reconciles_codex_metadata_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Metadata", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let agent_session_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    let session = repository
        .create_session(
            json!({
                "agentId": "codex",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "codex",
                    "titleSource": "placeholder"
                },
                "title": "Codex Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Captured Metadata Title\"}}\n"),
    )
    .expect("write session index");

    let output = ingest_terminal_title_event_with_home(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "rawTitle": agent_session_id,
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
        temp.path(),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
    assert_eq!(result.get("agentSessionId"), Some(&json!(agent_session_id)));
    let session = result.get("session").expect("result session");
    assert_eq!(
        session.get("title"),
        Some(&json!("Captured Metadata Title"))
    );
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentSessionId")),
        Some(&json!(agent_session_id))
    );
}

#[test]
fn zmx_status_title_reconciles_codex_rename_metadata() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-zmx-status-title";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Renamed From Agent CLI\"}}\n"),
    )
    .expect("write session index");

    let output = ingest_terminal_title_event_with_home(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "⣸ ghostex",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
        temp.path(),
    )
    .expect("terminal title result");

    assert!(output.schedule_presentation_delta);
    assert_eq!(output.result.get("changed"), Some(&json!(true)));
    assert_eq!(
        output.result.get("reason"),
        Some(&json!("metadata-title-applied"))
    );
    assert_eq!(
        output
            .result
            .get("session")
            .and_then(|session| session.get("title")),
        Some(&json!("Renamed From Agent CLI"))
    );
}

#[test]
fn request_session_rename_reconciles_codex_metadata_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-thread-rename";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Old title\"}}\n{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Renamed Investigation\",\"updated_at\":\"2026-06-21T15:35:00.000Z\"}}\n"
            ),
        )
        .expect("write session index");

    let result = request_session_rename(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "title": "Renamed Investigation",
            "titleSource": "user"
        })
        .as_object()
        .expect("rename params"),
        temp.path(),
    )
    .expect("rename result");

    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(result.get("pendingAgentMetadata"), Some(&json!(true)));
    assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
    assert_eq!(
        result.get("shouldSendAgentRenameCommand"),
        Some(&json!(true))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Renamed Investigation")));
    let runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestStatus"),
        Some(&json!("confirmed"))
    );
    assert_eq!(
        runtime_settings.get("titleMetadataProvider"),
        Some(&json!("codex-session-index"))
    );
    assert_eq!(
        runtime_settings.get("titleMetadataSource"),
        Some(&json!("agent-metadata"))
    );
    assert_eq!(
        runtime_settings.get("titleMetadataUpdatedAt"),
        Some(&json!("2026-06-21T15:35:00.000Z"))
    );
    assert_eq!(
        runtime_settings.get("titleSource"),
        Some(&json!("terminal-auto"))
    );
    assert!(runtime_settings.get("titleMetadataCheckedAt").is_some());
    let stored = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("get stored session")
        .expect("stored session");
    assert_eq!(stored.get("title"), Some(&json!("Renamed Investigation")));
}

#[test]
fn request_session_rename_keeps_pending_when_codex_metadata_is_missing() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-thread-missing";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());

    let result = request_session_rename(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "title": "Requested Missing Title",
            "titleSource": "user"
        })
        .as_object()
        .expect("rename params"),
        temp.path(),
    )
    .expect("rename result");

    assert_eq!(
        result.get("reason"),
        Some(&json!("agent-rename-request-pending-metadata"))
    );
    assert_eq!(
        result.get("shouldSendAgentRenameCommand"),
        Some(&json!(true))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Codex Session")));
    let runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestStatus"),
        Some(&json!("pending"))
    );
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestTitle"),
        Some(&json!("Requested Missing Title"))
    );
    assert!(runtime_settings.get("titleMetadataSource").is_none());
}

#[test]
fn trailing_agent_metadata_reconcile_marks_request_mismatch() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-thread-trailing";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let _pending = request_session_rename(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "title": "Requested Title",
            "titleSource": "user"
        })
        .as_object()
        .expect("rename params"),
        temp.path(),
    )
    .expect("pending rename");
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Accepted Different Title\"}}\n"),
    )
    .expect("write session index");

    let changed = reconcile_agent_metadata_title_for_session(
        &repository,
        &lifecycle.project_id,
        &lifecycle.session_id,
        temp.path(),
        "metadata-mismatch",
    )
    .expect("trailing reconcile");

    assert!(changed);
    let stored = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("get stored session")
        .expect("stored session");
    assert_eq!(
        stored.get("title"),
        Some(&json!("Accepted Different Title"))
    );
    let runtime_settings = stored
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestStatus"),
        Some(&json!("metadata-mismatch"))
    );
}
