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
