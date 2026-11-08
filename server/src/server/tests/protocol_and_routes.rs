use super::*;
use crate::{
    constants::GXSERVER_PROTOCOL_HEADER,
    session_chat_files::{sanitized_session_chat_attachment_name, session_chat_image_media_type},
};
use std::fs;

#[tokio::test]
async fn image_body_limit_reports_actual_request_size() {
    use crate::constants::GXSERVER_IMAGE_BODY_LIMIT_BYTES;
    let size = GXSERVER_IMAGE_BODY_LIMIT_BYTES as u64 + 1;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_LENGTH, size.to_string().parse().unwrap());
    assert!(matches!(
        read_json_body(&headers, Body::empty(), GXSERVER_IMAGE_BODY_LIMIT_BYTES).await,
        Err(ReadBodyError::TooLarge { size_bytes: Some(actual) }) if actual == size
    ));
    assert_eq!(
        json_body_limit_bytes("/api/saveSessionChatImage"),
        44 * 1024 * 1024
    );
}

#[test]
fn decoded_image_over_32_mib_returns_413() {
    use base64::Engine as _;
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths);
    let bytes = vec![0u8; crate::session_chat_files::SESSION_CHAT_IMAGE_MAX_BYTES + 1];
    let response = crate::session_chat_files::handle_save_session_chat_image_http(
        &state,
        "/api/saveSessionChatImage".to_string(),
        "image-limit-test".to_string(),
        &json!({"params":{"base64Data":base64::engine::general_purpose::STANDARD.encode(&bytes)}}),
    );
    assert_eq!(response.response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let limit = response
        .response
        .extensions()
        .get::<ImageSizeLimit>()
        .unwrap();
    assert_eq!(limit.size_bytes, bytes.len());
    assert_eq!(limit.limit_bytes, 32 * 1024 * 1024);
}

#[test]
fn saved_image_above_old_preview_limit_can_be_read() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preview.png");
    let mut bytes = vec![0u8; 25_000_000];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    fs::write(&path, bytes).unwrap();
    let response = crate::session_chat_files::handle_read_session_chat_image_http(
        "/api/readSessionChatImage".to_string(),
        "preview-limit-test".to_string(),
        &json!({"params":{"path":path}}),
    );
    assert_eq!(response.response.status(), StatusCode::OK);
}

#[test]
fn typed_operation_scope_rejection_details_match_private_typescript_shape() {
    let mut params = Map::new();
    params.insert("action".to_string(), json!("board"));
    params.insert("projectId".to_string(), json!("P3a91"));
    params.insert(
        "projectPath".to_string(),
        json!("/Users/person/dev/private-project"),
    );
    let details = typed_operation_scope_rejection_details(
        "/api/runBeadsAction",
        &params,
        &TypedOperationError {
            code: "notFound",
            details: None,
            message: "projectPath does not exist".to_string(),
            scope_rejection: true,
        },
    );

    assert_eq!(details.get("action"), Some(&json!("board")));
    assert_eq!(details.get("endpoint"), Some(&json!("runBeadsAction")));
    assert_eq!(details.get("errorCode"), Some(&json!("notFound")));
    assert_eq!(
        details.get("errorType"),
        Some(&json!("GxserverProjectPathError"))
    );
    assert_eq!(details.get("hasProjectId"), Some(&json!(true)));
    assert_eq!(details.get("hasProjectPath"), Some(&json!(true)));
    assert!(!details.to_string().contains("private-project"));
}

#[test]
fn session_chat_attachment_names_are_flat_and_portable() {
    assert_eq!(
        sanitized_session_chat_attachment_name(Some("notes.pdf")),
        Some("notes.pdf".to_string())
    );
    assert_eq!(
        sanitized_session_chat_attachment_name(Some("/tmp/../etc/passwd")),
        Some("passwd".to_string())
    );
    assert_eq!(
        sanitized_session_chat_attachment_name(Some("C:\\Users\\me\\my report (v2).docx")),
        Some("my-report--v2-.docx".to_string())
    );
    // Hidden-file dots and empty results fall back to the caller default.
    assert_eq!(
        sanitized_session_chat_attachment_name(Some(".env")),
        Some("env".to_string())
    );
    assert_eq!(sanitized_session_chat_attachment_name(Some("...")), None);
    assert_eq!(sanitized_session_chat_attachment_name(None), None);
}

#[test]
fn session_chat_image_media_type_prefers_magic_bytes() {
    let path = std::path::Path::new("/tmp/x.dat");
    assert_eq!(
        session_chat_image_media_type(b"\x89PNG\r\n\x1a\n....", path),
        Some("image/png")
    );
    assert_eq!(
        session_chat_image_media_type(b"\xff\xd8\xff\xe0rest", path),
        Some("image/jpeg")
    );
    assert_eq!(
        session_chat_image_media_type(b"RIFF\x00\x00\x00\x00WEBPVP8 ", path),
        Some("image/webp")
    );
    // Extension fallback for formats without a simple signature.
    assert_eq!(
        session_chat_image_media_type(b"<svg/>", std::path::Path::new("/tmp/a.svg")),
        Some("image/svg+xml")
    );
    // Non-images are refused, whatever the extension claims.
    assert_eq!(
        session_chat_image_media_type(b"plain text", std::path::Path::new("/tmp/a.txt")),
        None
    );
}

#[test]
fn foreground_classifies_selected_port_ownership_like_typescript() {
    let current = test_health("gxserver:0.1.0:current");
    let previous = test_health("gxserver:0.1.0:previous");

    assert_eq!(
        classify_existing_gxserver(Some(&current), "gxserver:0.1.0:current"),
        ExistingGxserverState::Reusable
    );
    assert_eq!(
        classify_existing_gxserver(Some(&previous), "gxserver:0.1.0:current"),
        ExistingGxserverState::Running
    );
    assert_eq!(
        classify_existing_gxserver(None, "gxserver:0.1.0:current"),
        ExistingGxserverState::Stopped
    );
}

/*
CDXC:AgentLauncher 2026-08-07:
Only the caller reads this response. The sidebar row that renders Global
Actions lives in another surface that refetches the HUD when the daemon
announces a change and never polls it, so a Global Action write has to
broadcast the way a project Action write already does through its
projectUpdated delta. Without the announcement the row kept the stale list
until an unrelated project delta happened to fire.
*/
#[tokio::test]
async fn global_sidebar_command_write_broadcasts_a_hud_change_event() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    /*
    `ensure_gxserver_storage_layout` creates the state directories but not
    the config directory, so a temp home has nowhere to write the default
    config. A real install always has one.
    */
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let state = test_app_state(paths);
    let token = state.auth_token.clone();
    let mut events = state.event_hub.subscribe();

    let saved = route_http(
        state.clone(),
        rpc_request(
            "/api/mutateSidebarHudSettings",
            &token,
            json!({
                "params": {
                    "actionType": "terminal",
                    "command": "echo global",
                    "commandId": "custom-global-action",
                    "name": "Global Action",
                    "operation": "save",
                    "showOnProjectRow": true,
                    "target": "globalCommand"
                }
            }),
        ),
        "request-save-global-sidebar-command".to_string(),
    )
    .await;
    assert_eq!(saved.response.status(), StatusCode::OK);
    let body = response_json(saved.response).await;
    assert_eq!(
        body["result"]["hud"]["globalCommands"][0]["commandId"],
        json!("custom-global-action")
    );
    assert_eq!(
        body["result"]["hud"]["globalCommands"][0]["showOnProjectRow"],
        json!(true)
    );

    let mut broadcast_types = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Some(event_type) = event["type"].as_str() {
            broadcast_types.push(event_type.to_string());
        }
    }
    assert!(
        broadcast_types
            .iter()
            .any(|event_type| event_type == "globalSidebarCommandsChanged"),
        "expected a globalSidebarCommandsChanged broadcast, saw {broadcast_types:?}"
    );
}

#[tokio::test]
async fn protocol_contract_gate_edges_match_typescript() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let token = state.auth_token.clone();

    let unknown_options = route_http(
        state.clone(),
        Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/missing")
            .body(Body::empty())
            .expect("request"),
        "request-options".to_string(),
    )
    .await;
    assert_eq!(unknown_options.response.status(), StatusCode::NOT_FOUND);
    let body = response_json(unknown_options.response).await;
    assert_eq!(body["error"], json!("notFound"));
    assert_eq!(
        body["message"],
        json!("/api/missing is not a gxserver HTTP endpoint.")
    );

    let http_events = route_http(
        state.clone(),
        Request::builder()
            .method(Method::GET)
            .uri("/api/events")
            .body(Body::empty())
            .expect("request"),
        "request-events".to_string(),
    )
    .await;
    assert_eq!(http_events.response.status(), StatusCode::NOT_FOUND);
    let body = response_json(http_events.response).await;
    assert_eq!(body["error"], json!("notFound"));
    assert_eq!(
        body["message"],
        json!("No gxserver endpoint for GET /api/events.")
    );

    let header_wins = route_http(
        state,
        Request::builder()
            .method(Method::POST)
            .uri("/api/listSessions")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(GXSERVER_PROTOCOL_HEADER, "999")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "params": {}, "protocolVersion": GXSERVER_PROTOCOL_VERSION }).to_string(),
            ))
            .expect("request"),
        "request-protocol".to_string(),
    )
    .await;
    assert_eq!(header_wins.response.status(), StatusCode::UPGRADE_REQUIRED);
    let body = response_json(header_wins.response).await;
    assert_eq!(body["error"], json!("protocolMismatch"));
    assert_eq!(
        body["message"],
        json!(
            "gxserver protocol mismatch. Expected protocol 1, got 999. Update Ghostex and gxserver so their protocol versions match."
        )
    );
}

#[tokio::test]
async fn protocol_query_parsing_matches_typescript_edges() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let token = state.auth_token.clone();

    let empty_query = route_http(
        state.clone(),
        Request::builder()
            .method(Method::POST)
            .uri("/api/listSessions?protocolVersion=")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "params": {} }).to_string()))
            .expect("request"),
        "request-empty-query".to_string(),
    )
    .await;
    assert_eq!(empty_query.response.status(), StatusCode::UPGRADE_REQUIRED);
    let body = response_json(empty_query.response).await;
    assert_eq!(
        body["message"],
        json!(
            "gxserver protocol mismatch. Expected protocol 1, got undefined. Update Ghostex and gxserver so their protocol versions match."
        )
    );

    let plus_query = route_http(
        state,
        Request::builder()
            .method(Method::POST)
            .uri("/api/listSessions?protocolVersion=%2B1")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "params": {} }).to_string()))
            .expect("request"),
        "request-plus-query".to_string(),
    )
    .await;
    assert_eq!(plus_query.response.status(), StatusCode::UPGRADE_REQUIRED);
    let body = response_json(plus_query.response).await;
    assert_eq!(
        body["message"],
        json!(
            "gxserver protocol mismatch. Expected protocol 1, got +1. Update Ghostex and gxserver so their protocol versions match."
        )
    );
}

#[test]
fn request_id_preserves_non_empty_header_value() {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static(" request-1 "));
    assert_eq!(request_id(&headers), " request-1 ");
}

#[tokio::test]
async fn query_logs_route_returns_filtered_logs_and_bad_request_errors() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    fs::write(
        &paths.log_file,
        [
            serde_json::to_string(&json!({
                "client": "cli",
                "event": "agent.detected",
                "level": "info",
                "projectId": "P3a91",
                "serverId": "S7k",
                "sessionId": "G8v20",
                "ts": "2026-05-30T10:00:00.000Z"
            }))
            .unwrap(),
            serde_json::to_string(&json!({
                "client": "api",
                "event": "zmx.kill.failed",
                "level": "error",
                "projectId": "P3a91",
                "serverId": "S7k",
                "sessionId": "G8v20",
                "ts": "2026-05-30T10:01:00.000Z"
            }))
            .unwrap(),
        ]
        .join("\n")
            + "\n",
    )
    .expect("write logs");
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();

    let filtered = route_http(
        state.clone(),
        rpc_request(
            "/api/queryLogs",
            &token,
            json!({
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                "params": {
                    "eventPrefix": "agent.",
                    "limit": 1,
                    "order": "desc"
                }
            }),
        ),
        "request-1".to_string(),
    )
    .await;
    assert_eq!(filtered.response.status(), StatusCode::OK);
    let body = response_json(filtered.response).await;
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["result"]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["result"]["entries"][0]["event"],
        json!("agent.detected")
    );
    assert_eq!(body["result"]["malformedLineCount"], json!(0));

    let bad_params = route_http(
        state,
        rpc_request(
            "/api/queryLogs",
            &token,
            json!({
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                "params": { "limit": 0 }
            }),
        ),
        "request-2".to_string(),
    )
    .await;
    assert_eq!(bad_params.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(bad_params.response).await;
    assert_eq!(body["error"], json!("badRequest"));
}

#[tokio::test]
async fn agent_hook_route_matches_typescript_bad_params_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let token = state.auth_token.clone();

    let response = route_http(
        state,
        rpc_request(
            "/api/readAgentHookStatus",
            &token,
            json!({
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                "params": []
            }),
        ),
        "request-hook-bad-params".to_string(),
    )
    .await;

    assert_eq!(
        response.response.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let body = response_json(response.response).await;
    assert_eq!(body["error"], json!("internalError"));
    assert_eq!(body["message"], json!("RPC params must be an object."));
}

#[tokio::test]
async fn agent_hook_conflict_response_strips_private_log_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let log_file = paths.log_file.clone();
    let state = test_app_state(paths);
    let token = state.auth_token.clone();
    let current_codex_session_id = "019e7af5-c610-7f62-a129-db7bb510b48d";
    let incoming_codex_session_id = "019e7c39-7ba7-7ac3-b79c-02757e299516";

    let created_project = route_http(
        state.clone(),
        rpc_request(
            "/api/createProject",
            &token,
            json!({
                "params": {
                    "name": "Hook Conflict",
                    "path": temp.path().to_string_lossy()
                }
            }),
        ),
        "request-create-hook-conflict-project".to_string(),
    )
    .await;
    assert_eq!(created_project.response.status(), StatusCode::OK);
    let body = response_json(created_project.response).await;
    let project_id = body["result"]["project"]["projectId"]
        .as_str()
        .expect("project id")
        .to_string();

    let created_session = route_http(
        state.clone(),
        rpc_request(
            "/api/createSession",
            &token,
            json!({
                "params": {
                    "agentId": "codex",
                    "kind": "agent",
                    "projectId": project_id.clone(),
                    "runtimeSettings": {
                        "agentActivity": { "activity": "idle", "isAcknowledged": true },
                        "agentName": "codex",
                        "agentSessionId": current_codex_session_id,
                        "titleSource": "terminal-auto"
                    },
                    "title": "Target Codex Thread"
                }
            }),
        ),
        "request-create-hook-conflict-session".to_string(),
    )
    .await;
    assert_eq!(created_session.response.status(), StatusCode::OK);
    let body = response_json(created_session.response).await;
    let session_id = body["result"]["session"]["sessionId"]
        .as_str()
        .expect("session id")
        .to_string();

    let ingested = route_http(
        state,
        rpc_request(
            "/api/ingestAgentHookEvent",
            &token,
            json!({
                "params": {
                    "agentName": "codex",
                    "agentSessionId": incoming_codex_session_id,
                    "eventName": "Stop",
                    "firstUserMessage": "private prompt text",
                    "projectId": project_id,
                    "rawEventName": "Stop",
                    "sessionId": session_id.clone(),
                    "status": "attention",
                    "statusUpdatedAt": "2026-06-09T18:08:19.857Z",
                    "title": "Wrong Codex Thread"
                }
            }),
        ),
        "request-ingest-hook-conflict".to_string(),
    )
    .await;

    assert_eq!(ingested.response.status(), StatusCode::OK);
    let body = response_json(ingested.response).await;
    assert_eq!(
        body["result"]["reason"],
        json!("passive-session-identity-conflict")
    );
    assert!(body["result"].get("identityConflict").is_none());
    assert_eq!(body["result"]["activity"]["activity"], json!("idle"));
    let logs = fs::read_to_string(log_file).expect("read hook conflict log");
    assert!(logs.contains("sessionIdentity.passiveEventRejected"));
    assert!(!logs.contains(current_codex_session_id));
    assert!(!logs.contains(incoming_codex_session_id));
    assert!(!logs.contains("private prompt text"));
    assert!(!logs.contains("Wrong Codex Thread"));
    assert!(!logs.contains("Target Codex Thread"));
}

#[tokio::test]
async fn export_session_transcript_includes_coordinator_goal_and_instructions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let transcript_path = temp.path().join("coordinator-transcript.jsonl");
    fs::write(
        &transcript_path,
        concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"Ship the coordinator handoff\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Kicked off thread 1.\"}]}}\n"
        ),
    )
    .expect("write transcript");

    let db = crate::storage::open_gxserver_database(&paths).expect("open db");
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({
                "name": "Ghostex",
                "path": temp.path(),
            })
            .as_object()
            .expect("project params"),
        )
        .expect("project created");
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let coordinator_session = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "agentSessionPath": transcript_path,
                    "coordinatorRole": "skill",
                },
                "title": "Coordinator",
            })
            .as_object()
            .expect("session params"),
            true,
        )
        .expect("coordinator session created");
    let coordinator_session_id = coordinator_session["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    crate::coordinators::insert_coordinator(
        &db,
        &project_id,
        &coordinator_session_id,
        "Finish remote coordinator handoff",
        "Keep every thread scoped to one module.",
    )
    .expect("insert coordinator");

    let plain_session = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "agentSessionPath": transcript_path,
                },
                "title": "Plain Session",
            })
            .as_object()
            .expect("session params"),
            true,
        )
        .expect("plain session created");
    let plain_session_id = plain_session["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    drop(db);

    let exported_coordinator = route_http(
        state.clone(),
        rpc_request(
            "/api/exportSessionTranscript",
            &token,
            json!({
                "params": {
                    "includeMarkdown": true,
                    "projectId": project_id,
                    "sessionId": coordinator_session_id,
                }
            }),
        ),
        "export-coordinator".to_string(),
    )
    .await;
    assert_eq!(exported_coordinator.response.status(), StatusCode::OK);
    let body = response_json(exported_coordinator.response).await;
    assert_eq!(
        body["result"]["coordinator"],
        json!({
            "goal": "Finish remote coordinator handoff",
            "instructions": "Keep every thread scoped to one module.",
        })
    );
    assert!(body["result"]["markdown"]
        .as_str()
        .is_some_and(|markdown| markdown.contains("Ship the coordinator handoff")));

    let exported_plain = route_http(
        state,
        rpc_request(
            "/api/exportSessionTranscript",
            &token,
            json!({
                "params": {
                    "projectId": project_id,
                    "sessionId": plain_session_id,
                }
            }),
        ),
        "export-plain".to_string(),
    )
    .await;
    assert_eq!(exported_plain.response.status(), StatusCode::OK);
    let plain_body = response_json(exported_plain.response).await;
    assert!(plain_body["result"].get("coordinator").is_none());
}

#[tokio::test]
async fn export_session_transcript_fails_when_the_coordinator_read_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let transcript_path = temp.path().join("transcript.jsonl");
    fs::write(
        &transcript_path,
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"Hello\"}}\n",
    )
    .expect("write transcript");
    let db = crate::storage::open_gxserver_database(&paths).expect("open db");
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({ "name": "Ghostex", "path": temp.path() })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "agentSessionPath": transcript_path,
                },
                "title": "Session",
            })
            .as_object()
            .expect("session params"),
            true,
        )
        .expect("session created");
    let session_id = session["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    // An unreadable coordinators table: the export must not answer as if the session were no
    // orchestrator, which would hand an orchestrator off as a plain session.
    db.execute_batch("DROP TABLE coordinators")
        .expect("drop coordinators");
    drop(db);

    let exported = route_http(
        state,
        rpc_request(
            "/api/exportSessionTranscript",
            &token,
            json!({ "params": { "projectId": project_id, "sessionId": session_id } }),
        ),
        "export-unreadable-coordinator".to_string(),
    )
    .await;
    assert_ne!(exported.response.status(), StatusCode::OK);
}

#[tokio::test]
async fn export_session_transcript_counts_turns_and_keeps_the_last_ones() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let transcript_path = temp.path().join("transcript.jsonl");
    let mut lines = String::new();
    for turn in 1..=3 {
        lines.push_str(&format!(
            "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"Question {turn}\"}}}}\n\
             {{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"Answer {turn}\"}}]}}}}\n"
        ));
    }
    fs::write(&transcript_path, lines).expect("write transcript");
    let db = crate::storage::open_gxserver_database(&paths).expect("open db");
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({ "name": "Ghostex", "path": temp.path() })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "agentSessionPath": transcript_path,
                },
                "title": "Session",
            })
            .as_object()
            .expect("session params"),
            true,
        )
        .expect("session created");
    let session_id = session["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    drop(db);

    let counted = route_http(
        state.clone(),
        rpc_request(
            "/api/countSessionTranscriptTurns",
            &token,
            json!({ "params": { "projectId": project_id, "sessionId": session_id } }),
        ),
        "count".to_string(),
    )
    .await;
    assert_eq!(counted.response.status(), StatusCode::OK);
    let counts = response_json(counted.response).await["result"].clone();
    assert_eq!(counts["turns"], json!(3));
    assert!(counts.get("path").is_none(), "a count writes no file");
    let sizes = counts["lastTurnsBytes"].as_array().expect("sizes").clone();
    assert_eq!(sizes.len(), 3);
    assert_eq!(
        sizes[2], counts["bytes"],
        "all three turns is the whole file"
    );

    let exported = route_http(
        state,
        rpc_request(
            "/api/exportSessionTranscript",
            &token,
            json!({
                "params": {
                    "includeMarkdown": true,
                    "lastTurns": 1,
                    "projectId": project_id,
                    "sessionId": session_id,
                }
            }),
        ),
        "export-last-turn".to_string(),
    )
    .await;
    assert_eq!(exported.response.status(), StatusCode::OK);
    let result = response_json(exported.response).await["result"].clone();
    assert_eq!(result["turns"], json!(3));
    assert_eq!(result["keptTurns"], json!(1));
    assert_eq!(
        result["bytes"], sizes[0],
        "the estimate is the written size"
    );
    let markdown = result["markdown"].as_str().expect("markdown");
    assert!(markdown.contains("Answer 3"));
    assert!(!markdown.contains("Answer 2"));
    assert!(
        markdown.contains("- Question 1\n- Question 2\n"),
        "{markdown}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn omp_tool_approval_post_answer_probe_publishes_the_next_command() {
    omp_tool_approval_probe_after_http_action(OmpApprovalAction::Row).await;
}

#[cfg(unix)]
#[tokio::test]
async fn omp_tool_approval_post_key_probe_publishes_the_next_command() {
    omp_tool_approval_probe_after_http_action(OmpApprovalAction::Key).await;
}

/// The row answer names the approval its card showed (`noticeDetectedAt`), as the chat core sends
/// it: the check that the answer targets that approval reads OMP's screen as OMP, so the answer is
/// applied and the check leaves the shown approval in place.
#[cfg(unix)]
#[tokio::test]
async fn omp_tool_approval_row_answer_to_the_shown_approval_is_applied() {
    omp_tool_approval_probe_after_http_action(OmpApprovalAction::ShownRow).await;
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq)]
enum OmpApprovalAction {
    /// A key through `sendSessionChatMessage`.
    Key,
    /// A picker row through `answerSessionChatPrompt` without the shown notice's identity.
    Row,
    /// A picker row with the `noticeDetectedAt` of the approval the card showed.
    ShownRow,
}

/// The send worker uses its scripted terminal; the burst's blocking reader receives the next
/// approval through the existing zmx capture socket, so both real HTTP scheduling sites are tested.
/// For `ShownRow` the socket serves the shown approval to the detection that painted the card and
/// to the answer's check, and holds the first later capture until the test has read what the check
/// left in the detection cache.
#[cfg(unix)]
async fn omp_tool_approval_probe_after_http_action(action: OmpApprovalAction) {
    use crate::session_chat_send::test_terminal::{self, TestTerminal};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicBool, Ordering};

    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let db = open_gxserver_database(&state.paths).expect("db");
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({ "name": "Approvals", "path": temp.path() })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap();
    let session = repository
        .create_session(
            json!({
                "projectId": project_id, "kind": "agent", "agentId": "omp",
                "title": "Approve a command", "lifecycleState": "running",
                "runtimeSettings": { "agentName": "omp" }
            })
            .as_object()
            .unwrap(),
            false,
        )
        .expect("session");
    assert_eq!(
        crate::session_chat_follower::session_chat_display_agent_for_session(&session).as_deref(),
        Some("omp")
    );
    let session_id = session["sessionId"].as_str().unwrap();
    let zmx_name = session["zmxName"].as_str().expect("terminal name");
    drop(db);
    let first = include_str!("../../session_chat_tests/fixtures/omp-tool-approval-pi.txt");
    let next = first.replace("echo ghostex-approval-test", "echo next-command");
    test_terminal::open(
        &zmx_name,
        TestTerminal {
            screen: Some(Some(first.to_string())),
            ..TestTerminal::default()
        },
    );

    let socket = crate::zmx::zmx_session_socket_path(&zmx_name);
    fs::create_dir_all(socket.parent().unwrap()).expect("socket directory");
    let listener = UnixListener::bind(&socket).expect("capture socket");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let shown_captures = if action == OmpApprovalAction::ShownRow {
        2
    } else {
        0
    };
    let release = Arc::new(AtomicBool::new(action != OmpApprovalAction::ShownRow));
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let releasing = release.clone();
    let capture = std::thread::spawn(move || {
        let mut served = 0;
        while !stopping.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // macOS hands the accepted stream the listener's nonblocking flag.
                    stream.set_nonblocking(false).expect("blocking stream");
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 24];
                    // Other readers may connect only to inspect liveness or request Info.
                    if stream.read_exact(&mut request[..8]).is_err() || request[0] != 22 {
                        continue;
                    }
                    stream
                        .read_exact(&mut request[8..])
                        .expect("capture request");
                    assert_eq!(request[16], 6, "Info barrier");
                    served += 1;
                    let screen = if served <= shown_captures {
                        first
                    } else {
                        let held = std::time::Instant::now();
                        while !releasing.load(Ordering::SeqCst)
                            && held.elapsed() < std::time::Duration::from_secs(2)
                        {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        next.as_str()
                    };
                    stream
                        .write_all(&crate::zmx::ipc_frame::zmx_ipc_header(
                            22,
                            screen.len() as u32,
                        ))
                        .unwrap();
                    stream.write_all(screen.as_bytes()).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("capture listener: {error}"),
            }
        }
    });
    let observer_key = session_observer_key(project_id, session_id);
    let follower = tokio::spawn(std::future::pending::<()>());
    let follower_abort = follower.abort_handle();
    state.session_chat_followers.lock().unwrap().insert(
        observer_key.clone(),
        SessionChatFollowerEntry {
            subscribers: 1,
            fingerprint: String::new(),
            limit: 20,
            task: Some(follower),
            stream: Arc::new(crate::session_chat::SessionChatStream::new()),
            resnapshot: Arc::new(tokio::sync::Notify::new()),
            heartbeat: Arc::new(crate::session_chat::SessionChatFollowerHeartbeat::new()),
        },
    );
    let mut events = state.event_hub.subscribe();
    let detector = crate::session_chat_options::SessionChatOptionDetector::new(&state);
    // The approval the card shows: what the chat's detection, which reads OMP's screen as OMP,
    // published.
    let shown = if action == OmpApprovalAction::ShownRow {
        let detected = detector
            .detect(project_id, session_id, Some("omp"), true)
            .await;
        let notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            project_id,
            session_id,
            detected.notice,
        )
        .expect("the shown OMP approval");
        assert_eq!(
            notice.kind,
            crate::session_chat_notice::SESSION_CHAT_NOTICE_PERMISSION_PROMPT
        );
        assert_eq!(
            notice.approval_command.as_deref(),
            Some("echo ghostex-approval-test")
        );
        Some(notice.detected_at)
    } else {
        None
    };
    let endpoint = if action == OmpApprovalAction::Key {
        "/api/sendSessionChatMessage"
    } else {
        "/api/answerSessionChatPrompt"
    };
    let mut params = json!({ "projectId": project_id, "sessionId": session_id });
    if action == OmpApprovalAction::Key {
        params["key"] = json!("shift-tab");
    } else {
        params["kind"] = json!("terminalChoice");
        params["choiceIndex"] = json!(0);
    }
    if let Some(shown) = &shown {
        params["noticeDetectedAt"] = json!(shown);
    }
    // The user's own app answers (an agent answering another session's prompt asks first,
    // GitHub #495).
    let owner = state.security.session_tokens.mint_owner("test-desktop");
    let routed = route_http(
        state.clone(),
        rpc_request(endpoint, &owner, json!({ "params": params })),
        "approval-probe".to_string(),
    )
    .await;
    // What the answer's check left for every reader of the detection cache, before the
    // post-answer probes read the next approval.
    let after_check = detector.cached(project_id, session_id).notice;
    release.store(true, Ordering::SeqCst);
    let status = routed.response.status();
    let body = response_json(routed.response).await;
    let published = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.expect("event hub");
            if event["sessionId"] == session_id && event.get("terminalNotice").is_some() {
                break event;
            }
        }
    })
    .await;
    stop.store(true, Ordering::SeqCst);
    let captured = capture.join();
    fs::remove_file(socket).expect("remove capture socket");
    follower_abort.abort();
    state
        .session_chat_followers
        .lock()
        .unwrap()
        .remove(&observer_key);
    crate::session_chat_options::forget_session_chat_options(&state, project_id, session_id);
    captured.expect("capture thread");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["queued"], true, "{body}");
    if let Some(shown) = shown {
        let after_check = after_check.expect("the answer's check keeps the shown OMP approval");
        assert_eq!(after_check.detected_at, shown);
        assert_eq!(
            after_check.approval_command.as_deref(),
            Some("echo ghostex-approval-test")
        );
    }
    let published = published.expect("the HTTP action must publish the next OMP approval");
    assert_eq!(
        published["terminalNotice"]["kind"],
        crate::session_chat_notice::SESSION_CHAT_NOTICE_PERMISSION_PROMPT
    );
    assert_eq!(
        published["terminalNotice"]["approvalCommand"],
        "echo next-command"
    );
    assert!(!test_terminal::terminal(&zmx_name).writes.is_empty());
}
