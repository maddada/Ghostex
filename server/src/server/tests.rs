use super::*;
use crate::{
    config::create_default_gxserver_config,
    constants::GXSERVER_PROTOCOL_HEADER,
    session_chat_files::{sanitized_session_chat_attachment_name, session_chat_image_media_type},
    storage::{create_gxserver_migration_status, initialize_gxserver_storage},
};
use std::fs;

#[test]
fn renderer_command_actions_include_generated_title_rename() {
    /*
    CDXC:GenerateTitleSkill 2026-06-17-17:02:
    Rust gxserver must accept the same renderer `renameCommand` action as the TypeScript daemon so a full cutover keeps Claude Code generated-title renames on the native Enter path.
    */
    assert!(RENDERER_COMMAND_ACTIONS.contains(&"renameCommand"));
}

#[test]
fn title_signaled_process_identity_sync_only_targets_incomplete_live_zmx_identity() {
    assert!(terminal_title_indicates_agent_identity(
        "01a00854-13cb-7500-bde7-3d8d2b03abdd"
    ));
    assert!(terminal_title_indicates_agent_identity("Codex"));
    assert!(!terminal_title_indicates_agent_identity(
        "⠦ Fix GPUI Chat Mode Switching"
    ));

    let candidate = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "providerState": {
            "lifecycleState": "exists",
            "provider": "zmx",
        },
        "sessionId": "G9mmz",
        "surface": "terminal",
        "zmxName": "S9-P9-G9mmz",
    });
    assert!(should_probe_title_signaled_zmx_process_identity(&candidate));

    let mut promoted_agent = candidate.clone();
    promoted_agent["kind"] = json!("agent");
    promoted_agent["agentId"] = json!("codex");
    assert!(should_probe_title_signaled_zmx_process_identity(
        &promoted_agent
    ));

    promoted_agent["runtimeSettings"] = json!({
        "agentSessionId": "01a00854-13cb-7500-bde7-3d8d2b03abdd",
    });
    assert!(!should_probe_title_signaled_zmx_process_identity(
        &promoted_agent
    ));

    let mut stopped_terminal = candidate.clone();
    stopped_terminal["lifecycleState"] = json!("stopped");
    assert!(!should_probe_title_signaled_zmx_process_identity(
        &stopped_terminal
    ));

    let mut command_surface = candidate.clone();
    command_surface["surface"] = json!("commands");
    assert!(!should_probe_title_signaled_zmx_process_identity(
        &command_surface
    ));

    let mut non_zmx_terminal = candidate;
    non_zmx_terminal["providerState"]["provider"] = json!("none");
    assert!(!should_probe_title_signaled_zmx_process_identity(
        &non_zmx_terminal
    ));
}

#[test]
fn missing_zmx_agent_without_restore_plan_is_not_kept_running() {
    let project = json!({
        "path": "/tmp/ghostex-project",
        "projectId": "P9k9k",
    });
    let agent_settings = Map::new();
    let unrestorable_agent = json!({
        "agentId": "codex",
        "kind": "agent",
        "lifecycleState": "running",
        "projectId": "P9k9k",
        "providerState": {
            "lifecycleState": "missing",
            "provider": "zmx",
        },
        "runtimeSettings": {
            "sessionPersistenceProvider": "zmx",
            "titleSource": "terminal-auto",
        },
        "sessionId": "G9mmz",
        "title": "Codex Session",
    });
    assert!(should_stop_unrestorable_missing_zmx_agent(
        &project,
        &unrestorable_agent,
        &agent_settings,
    ));

    let mut restorable_agent = unrestorable_agent.clone();
    restorable_agent["runtimeSettings"]["agentSessionId"] =
        json!("019fca32-5dad-73d3-a0eb-86b6a7486fdd");
    assert!(!should_stop_unrestorable_missing_zmx_agent(
        &project,
        &restorable_agent,
        &agent_settings,
    ));

    let mut plain_terminal = unrestorable_agent;
    plain_terminal["kind"] = json!("terminal");
    assert!(!should_stop_unrestorable_missing_zmx_agent(
        &project,
        &plain_terminal,
        &agent_settings,
    ));
}

#[test]
fn renderer_command_actions_keep_only_renderer_owned_mobile_timer_actions() {
    /*
    CDXC:GxserverDelayedSends 2026-08-17:
    Delayed Send now enters first-class daemon endpoints. Keeping the old
    renderer actions would arm a second timer in whichever desktop client
    happened to be connected.
    */
    assert!(!RENDERER_COMMAND_ACTIONS.contains(&"scheduleDelayedSend"));
    assert!(!RENDERER_COMMAND_ACTIONS.contains(&"cancelDelayedSend"));
    assert!(RENDERER_COMMAND_ACTIONS.contains(&"toggleCloseAfterDone"));
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

#[test]
fn project_status_agent_title_polling_predicate_matches_typescript() {
    let pending = json!({
        "kind": "agent",
        "projectId": "P1abc",
        "runtimeSettings": {
            "pendingAgentTitleRequestStatus": "pending"
        },
        "sessionId": "G1abc",
        "title": "Trusted user title"
    });
    assert!(should_check_agent_metadata_title_for_project_status(
        &pending
    ));

    let trusted = json!({
        "kind": "agent",
        "projectId": "P1abc",
        "runtimeSettings": {},
        "sessionId": "G1abc",
        "title": "Investigate renderer state"
    });
    assert!(!should_check_agent_metadata_title_for_project_status(
        &trusted
    ));

    let placeholder = json!({
        "kind": "agent",
        "projectId": "P1abc",
        "runtimeSettings": { "titleSource": "placeholder" },
        "sessionId": "G1abc",
        "title": "Codex Session"
    });
    assert!(should_check_agent_metadata_title_for_project_status(
        &placeholder
    ));

    let reconciled = json!({
        "kind": "agent",
        "projectId": "P1abc",
        "runtimeSettings": { "titleMetadataSource": "agent-metadata" },
        "sessionId": "G1abc",
        "title": "Codex Session"
    });
    assert!(!should_check_agent_metadata_title_for_project_status(
        &reconciled
    ));

    let terminal = json!({
        "kind": "terminal",
        "projectId": "P1abc",
        "runtimeSettings": {},
        "sessionId": "G1abc",
        "title": "Codex Session"
    });
    assert!(!should_check_agent_metadata_title_for_project_status(
        &terminal
    ));
}

#[test]
fn renderer_command_payload_adds_structured_session_target() {
    /*
    CDXC:GxserverRendererCommands 2026-06-21-19:22:
    Rust gxserver must normalize renderer-command payloads from any client so macOS receives a project-scoped session target and does not have to match raw G ids against combined sidebar presentation ids.
    */
    let payload = Map::from_iter([
        ("globalRef".to_string(), json!("S90:P1a:G9a")),
        ("projectId".to_string(), json!("P1a")),
        ("sessionId".to_string(), json!("G9a")),
        ("title".to_string(), json!("GPUI Sidebar Resize Parity")),
    ]);

    let normalized = with_renderer_session_target(payload);

    assert_eq!(
        normalized.get("sessionTarget"),
        Some(&json!({
            "globalRef": "S90:P1a:G9a",
            "projectId": "P1a",
            "sessionId": "G9a",
        }))
    );
    assert_eq!(normalized.get("sessionId"), Some(&json!("G9a")));
}

#[test]
fn first_prompt_auto_title_decides_provider_strategy_and_filters_meta_prompts() {
    let codex = json!({
        "agentId": "codex",
        "runtimeSettings": {},
        "title": "Codex Session",
    });
    let decision = decide_first_prompt_auto_title(
        &codex,
        Some("Please can you help me fix flaky tests."),
        false,
    );
    assert!(!decision.should_run);
    assert_eq!(
        decision.normalized_prompt.as_deref(),
        Some("fix flaky tests")
    );
    assert_eq!(decision.reason, "agentAutoTitle");
    assert_eq!(decision.strategy, Some("agentAutoTitle"));

    let claude = json!({
        "agentId": "claude",
        "runtimeSettings": {},
        "title": "Claude Code",
    });
    let decision = decide_first_prompt_auto_title(&claude, Some("Summarize the logs"), false);
    assert!(decision.should_run);
    assert_eq!(decision.strategy, Some("sendBareRenameCommand"));

    let meta = decide_first_prompt_auto_title(&codex, Some("# AGENTS.md instructions"), false);
    assert!(!meta.should_run);
    assert_eq!(meta.reason, "metaPrompt");

    let slash = decide_first_prompt_auto_title(
        &codex,
        Some("notes before command\n  /status please"),
        false,
    );
    assert!(!slash.should_run);
    assert_eq!(slash.reason, "slashCommand");
}

#[test]
fn agent_session_title_command_uses_provider_specific_slash_command() {
    assert_eq!(
        agent_session_title_command(Some("hermes-agent"), "Investigate hooks"),
        "/title Investigate hooks"
    );
    assert_eq!(
        agent_session_title_command(Some("Hermes Agent"), "Investigate hooks"),
        "/title Investigate hooks"
    );
    assert_eq!(
        agent_session_title_command(Some("pi"), "Investigate hooks"),
        "/name Investigate hooks"
    );
    assert_eq!(
        agent_session_title_command(Some("codex"), "Investigate hooks"),
        "/rename Investigate hooks"
    );
}

#[test]
fn requested_agent_title_command_submission_requires_opt_in_and_agent_rename() {
    let mut params = Map::new();
    params.insert("submitAgentRenameCommand".to_string(), Value::Bool(true));
    params.insert("title".to_string(), json!("Investigate hooks"));
    let result = json!({
        "session": {
            "agentId": "hermes-agent",
            "projectId": "P1abc",
            "sessionId": "G1abc"
        },
        "shouldSendAgentRenameCommand": true
    });

    assert_eq!(
        requested_agent_title_command_submission("/api/requestSessionRename", &params, &result),
        Some((
            "P1abc".to_string(),
            "G1abc".to_string(),
            "/title Investigate hooks".to_string()
        ))
    );

    params.remove("submitAgentRenameCommand");
    assert_eq!(
        requested_agent_title_command_submission("/api/requestSessionRename", &params, &result),
        None
    );
}

#[test]
fn first_prompt_auto_title_attempt_rejects_stale_same_prompt_job() {
    let session = json!({
        "runtimeSettings": {
            "firstUserMessage": "Please fix the sidebar",
            "gxserverFirstPromptAutoTitleAttemptId": "replacement-attempt",
            "gxserverFirstPromptAutoTitleStatus": "running"
        }
    });

    assert!(is_current_first_prompt_auto_title_attempt_for_prompt(
        &session,
        "replacement-attempt",
        Some("fix the sidebar")
    ));
    assert!(!is_current_first_prompt_auto_title_attempt_for_prompt(
        &session,
        "cancelled-attempt",
        Some("fix the sidebar")
    ));
}

#[test]
fn generated_first_prompt_titles_are_sanitized_and_clamped() {
    let title = parse_generated_session_title_text(
        "```text\n\"Investigate Sidebar Resize Regression With Extra Words\"\n```",
    )
    .expect("title");
    assert_eq!(title, "Investigate Sidebar Resize Regression");
    assert!(js_string_length(&title) <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH);
}

#[test]
fn first_prompt_title_caps_use_javascript_utf16_length() {
    let rocket = "\u{1F680}";
    let exact = js_string_slice_prefix(
        &rocket.repeat(126),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
    );
    assert_eq!(exact, rocket.repeat(125));
    assert_eq!(
        js_string_length(&exact),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH
    );

    let split = js_string_slice_prefix(
        &format!(
            "{}{}",
            "a".repeat(GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH - 1),
            rocket
        ),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
    );
    assert_eq!(
        split,
        format!(
            "{}{}",
            "a".repeat(GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH - 1),
            char::REPLACEMENT_CHARACTER
        )
    );
    assert_eq!(
        js_string_length(&split),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH
    );
}

#[test]
fn generated_title_clamp_counts_non_bmp_as_javascript_utf16() {
    let rocket = "\u{1F680}";
    let title = parse_generated_session_title_text(&rocket.repeat(20)).expect("title");
    assert_eq!(
        title,
        format!("{}{}", rocket.repeat(19), char::REPLACEMENT_CHARACTER)
    );
    assert_eq!(
        js_string_length(&title),
        GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH
    );
}

#[tokio::test]
async fn read_project_status_route_returns_project_sessions_and_missing_errors() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let token = state.auth_token.clone();

    let created_project = route_http(
        state.clone(),
        rpc_request(
            "/api/createProject",
            &token,
            json!({
                "params": {
                    "name": "Status Project",
                    "path": temp.path().to_string_lossy(),
                    "runtimeSettings": { "defaultPromptAgentId": "codex" }
                }
            }),
        ),
        "request-create-project".to_string(),
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
                    "projectId": project_id.clone(),
                    "title": "Status Session"
                }
            }),
        ),
        "request-create-session".to_string(),
    )
    .await;
    assert_eq!(created_session.response.status(), StatusCode::OK);
    let body = response_json(created_session.response).await;
    let session_id = body["result"]["session"]["sessionId"]
        .as_str()
        .expect("session id")
        .to_string();

    let status = route_http(
        state.clone(),
        rpc_request(
            "/api/readProjectStatus",
            &token,
            json!({ "params": { "projectId": project_id } }),
        ),
        "request-read-project-status".to_string(),
    )
    .await;
    assert_eq!(status.response.status(), StatusCode::OK);
    let body = response_json(status.response).await;
    assert_eq!(body["result"]["project"]["projectId"], json!(project_id));
    assert_eq!(
        body["result"]["project"]["runtimeSettings"]["defaultPromptAgentId"],
        json!("codex")
    );
    assert_eq!(body["result"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["result"]["sessions"][0]["sessionId"],
        json!(session_id)
    );

    let missing = route_http(
        state,
        rpc_request(
            "/api/readProjectStatus",
            &token,
            json!({ "params": { "projectId": "P9zzz" } }),
        ),
        "request-read-missing-project-status".to_string(),
    )
    .await;
    assert_eq!(missing.response.status(), StatusCode::NOT_FOUND);
    let body = response_json(missing.response).await;
    assert_eq!(body["error"], json!("notFound"));
    assert_eq!(body["message"], json!("Project P9zzz does not exist."));
}

/*
CDXC:GlobalActions 2026-08-07:
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

#[test]
fn session_state_sidecar_parser_matches_legacy_env_fields() {
    let raw = [
        "agent=codex",
        "agentSessionId=019eebdb-ba5a-7282-ac09-b926a9c09863",
        "agentSessionPath=/Users/example/.codex/sessions/2026/06/21/thread.jsonl",
        "firstUserMessageBase64=UGxlYXNlIGZpeCB0aGUgc2lkZWJhcg==",
        "lastActivityAt=2026-06-21T20:25:05.171Z",
        "status=working",
        "statusUpdatedAt=2026-06-21T20:25:06.000Z",
        "title=  GPUI Sidebar Resize Parity  ",
    ]
    .join("\n");
    let sidecar = parse_session_state_sidecar(&raw).expect("sidecar");

    assert_eq!(sidecar.agent_name.as_deref(), Some("codex"));
    assert_eq!(
        sidecar.agent_session_id.as_deref(),
        Some("019eebdb-ba5a-7282-ac09-b926a9c09863")
    );
    assert_eq!(
        sidecar.first_user_message.as_deref(),
        Some("Please fix the sidebar")
    );
    assert_eq!(sidecar.status.as_deref(), Some("working"));
    assert_eq!(
        sidecar.status_updated_at.as_deref(),
        Some("2026-06-21T20:25:06.000Z")
    );
    assert_eq!(sidecar.title.as_deref(), Some("GPUI Sidebar Resize Parity"));
    assert!(has_session_state_sidecar_payload(&sidecar));
    assert_eq!(
        sanitize_session_state_sidecar_path_part("P3lv0/../../G01q0"),
        "P3lv0-..-..-G01q0"
    );
}

#[test]
fn session_state_sidecar_reader_uses_typescript_one_mib_cap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let sidecar_path = build_session_state_sidecar_path(&paths, "P3lv0", "G01q0");
    fs::create_dir_all(sidecar_path.parent().expect("sidecar parent")).expect("sidecar dir");

    fs::write(
        &sidecar_path,
        format!("agent=codex\npadding={}", "x".repeat(70 * 1024)),
    )
    .expect("write sidecar under cap");
    let sidecar = read_session_state_sidecar(&paths, "P3lv0", "G01q0").expect("sidecar");
    assert_eq!(sidecar.agent_name.as_deref(), Some("codex"));

    fs::write(
        &sidecar_path,
        format!(
            "agent=codex\npadding={}",
            "x".repeat(GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES as usize + 1)
        ),
    )
    .expect("write sidecar over cap");
    assert!(read_session_state_sidecar(&paths, "P3lv0", "G01q0").is_none());
}

#[test]
fn zmx_title_observer_parses_lines_and_filters_observable_sessions() {
    assert_eq!(
        parse_zmx_title_line(r#"{"title":"  Codex Session  "}"#).as_deref(),
        Some("Codex Session")
    );
    assert!(parse_zmx_title_line(r#"{"title":"   "}"#).is_none());
    assert!(parse_zmx_title_line("not-json").is_none());

    let observable = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "providerState": { "lifecycleState": "exists", "provider": "zmx" },
        "projectId": "P1",
        "runtimeSettings": {},
        "sessionId": "G1",
        "zmxName": "S1-P1-G1"
    });
    assert!(is_zmx_title_observable_session(&observable));

    let missing_provider = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "providerState": { "lifecycleState": "missing", "provider": "zmx" },
        "projectId": "P1",
        "sessionId": "G1",
        "zmxName": "S1-P1-G1"
    });
    assert!(!is_zmx_title_observable_session(&missing_provider));
}

#[test]
fn live_zmx_process_identity_sync_accepts_provider_state_only_sessions() {
    let provider_state_only = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "providerState": { "lifecycleState": "exists", "provider": "zmx" },
        "runtimeSettings": {},
        "surface": "workspace",
        "zmxName": "S1-P1-G1"
    });
    assert!(should_sync_live_zmx_process_identity(&provider_state_only));

    let runtime_provider = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "providerState": { "lifecycleState": "exists" },
        "runtimeSettings": { "sessionPersistenceProvider": "zmx" },
        "surface": "workspace",
        "zmxName": "S1-P1-G1"
    });
    assert!(should_sync_live_zmx_process_identity(&runtime_provider));

    let command_surface = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "providerState": { "lifecycleState": "exists", "provider": "zmx" },
        "runtimeSettings": {},
        "surface": "commands",
        "zmxName": "S1-P1-G1"
    });
    assert!(!should_sync_live_zmx_process_identity(&command_surface));
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
async fn browse_project_directories_route_filters_directory_entries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("picker-parent");
    fs::create_dir_all(parent.join("alpha")).expect("alpha");
    fs::create_dir_all(parent.join("alpine")).expect("alpine");
    fs::create_dir_all(parent.join("beta")).expect("beta");
    fs::create_dir_all(parent.join(".hidden")).expect("hidden");
    fs::write(parent.join("alphabet.txt"), "not a directory\n").expect("file");
    let parent_path = path_to_string(&parent);

    let filtered = route_http(
        state.clone(),
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({
                "params": {
                    "limit": 5,
                    "partialPath": format!("{parent_path}/al")
                }
            }),
        ),
        "request-browse-filtered".to_string(),
    )
    .await;
    assert_eq!(filtered.response.status(), StatusCode::OK);
    let body = response_json(filtered.response).await;
    assert_eq!(body["result"]["parentPath"], json!(parent_path));
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha", "alpine"]);

    let hidden = route_http(
        state.clone(),
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({
                "params": {
                    "partialPath": format!("{parent_path}/.h")
                }
            }),
        ),
        "request-browse-hidden".to_string(),
    )
    .await;
    assert_eq!(hidden.response.status(), StatusCode::OK);
    let body = response_json(hidden.response).await;
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec![".hidden"]);

    let relative = route_http(
        state,
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({
                "params": {
                    "cwd": parent_path,
                    "partialPath": "./a"
                }
            }),
        ),
        "request-browse-relative".to_string(),
    )
    .await;
    assert_eq!(relative.response.status(), StatusCode::OK);
    let body = response_json(relative.response).await;
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha", "alpine"]);
}

#[tokio::test]
async fn browse_project_directories_sorts_case_insensitively_and_swallows_permission_errors() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("browse-order");
    for name in ["Zebra", "apple", "Banana", "cherry"] {
        fs::create_dir_all(parent.join(name)).expect("dir");
    }
    let parent_path = path_to_string(&parent);

    let sorted = route_http(
        state.clone(),
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({ "params": { "partialPath": format!("{parent_path}/") } }),
        ),
        "request-browse-order".to_string(),
    )
    .await;
    assert_eq!(sorted.response.status(), StatusCode::OK);
    let body = response_json(sorted.response).await;
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["apple", "Banana", "cherry", "Zebra"]);
    assert!(body["result"]["entries"][0]
        .as_object()
        .unwrap()
        .get("sortKey")
        .is_none());

    let unreadable = paths.root_dir.join("browse-unreadable");
    fs::create_dir_all(unreadable.join("child")).expect("child");
    let mut permissions = fs::metadata(&unreadable).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o000);
    }
    fs::set_permissions(&unreadable, permissions).expect("chmod");
    let denied = route_http(
        state,
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({ "params": { "partialPath": format!("{}/", path_to_string(&unreadable)) } }),
        ),
        "request-browse-denied".to_string(),
    )
    .await;
    let denied_status = denied.response.status();
    let denied_body = response_json(denied.response).await;
    let mut restored = fs::metadata(&unreadable).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        restored.set_mode(0o755);
    }
    fs::set_permissions(&unreadable, restored).expect("chmod restore");
    assert_eq!(denied_status, StatusCode::OK);
    assert_eq!(denied_body["result"]["entries"], json!([]));
    assert_eq!(
        denied_body["result"]["parentPath"],
        json!(path_to_string(&unreadable))
    );
}

#[tokio::test]
async fn discover_source_control_reports_every_provider_with_a_hint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths);
    let token = state.auth_token.clone();

    let response = route_http(
        state,
        rpc_request(
            "/api/discoverSourceControl",
            &token,
            json!({ "params": {} }),
        ),
        "request-discover-source-control".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    let providers = body["result"]["discovery"]["providers"]
        .as_array()
        .expect("providers")
        .clone();
    let names = providers
        .iter()
        .map(|entry| entry["provider"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["github", "gitlab", "bitbucket", "azure-devops"]);
    for provider in &providers {
        assert!(provider["installHint"]
            .as_str()
            .is_some_and(|hint| !hint.is_empty()));
        assert!(provider["label"]
            .as_str()
            .is_some_and(|label| !label.is_empty()));
        assert!(provider["auth"]["status"].as_str().is_some());
        assert!(matches!(
            provider["status"].as_str(),
            Some("available") | Some("missing") | Some("unsupported")
        ));
    }
    for provider in providers.iter().filter(|entry| {
        matches!(
            entry["provider"].as_str(),
            Some("bitbucket") | Some("azure-devops")
        )
    }) {
        assert_eq!(provider["status"], json!("unsupported"));
    }
    assert!(body["result"]["discovery"]["checkedAt"]
        .as_str()
        .is_some_and(|value| value.ends_with('Z')));
}

#[tokio::test]
async fn lookup_repository_rejects_unsupported_providers_and_blank_repositories() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths);
    let token = state.auth_token.clone();

    let unsupported = route_http(
        state.clone(),
        rpc_request(
            "/api/lookupRepository",
            &token,
            json!({ "params": { "provider": "bitbucket", "repository": "team/app" } }),
        ),
        "request-lookup-unsupported".to_string(),
    )
    .await;
    assert_eq!(unsupported.response.status(), StatusCode::BAD_REQUEST);

    let blank = route_http(
        state,
        rpc_request(
            "/api/lookupRepository",
            &token,
            json!({ "params": { "provider": "github", "repository": "  " } }),
        ),
        "request-lookup-blank".to_string(),
    )
    .await;
    assert_eq!(blank.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(blank.response).await;
    assert_eq!(body["error"], json!("badRequest"));
    assert_eq!(
        body["message"],
        json!("repository must be a non-empty string.")
    );
}

#[tokio::test]
async fn resolve_git_root_route_does_not_register_projects() {
    let git_available = StdCommand::new("git")
        .arg("--version")
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !git_available {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let repo = paths.root_dir.join("open-path-repo");
    let nested = repo.join("src").join("feature");
    let outside = paths.root_dir.join("outside-repo");
    fs::create_dir_all(&nested).expect("nested");
    fs::create_dir_all(&outside).expect("outside");
    assert!(StdCommand::new("git")
        .arg("init")
        .current_dir(&repo)
        .status()
        .expect("git init")
        .success());

    let resolved = route_http(
        state.clone(),
        rpc_request(
            "/api/resolveGitRootForPath",
            &token,
            json!({
                "params": {
                    "path": path_to_string(&nested)
                }
            }),
        ),
        "request-resolve-git-root".to_string(),
    )
    .await;
    let resolved_status = resolved.response.status();
    let body = response_json(resolved.response).await;
    assert_eq!(resolved_status, StatusCode::OK, "response body: {body}");
    assert_eq!(
        body["result"]["gitRoot"],
        json!(path_to_string(
            &fs::canonicalize(&repo).expect("canonical repo")
        ))
    );

    let projects = route_http(
        state.clone(),
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-projects".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert_eq!(body["result"]["projects"], json!([]));

    let outside = route_http(
        state,
        rpc_request(
            "/api/resolveGitRootForPath",
            &token,
            json!({
                "params": {
                    "path": path_to_string(&outside)
                }
            }),
        ),
        "request-resolve-outside".to_string(),
    )
    .await;
    assert_eq!(outside.response.status(), StatusCode::OK);
    let body = response_json(outside.response).await;
    assert_eq!(body["result"], json!({}));
}

#[tokio::test]
async fn delete_worktree_project_route_removes_clean_checkout_and_local_branch() {
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("delete-worktree-parent");
    let worktree = paths.root_dir.join("delete-worktree-parent-feature");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "feature-clean"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feature-clean",
        ],
    );

    let parent_project = add_project_path_for_server_test(
        state.clone(),
        &token,
        &parent,
        Some("Delete Worktree Parent"),
    )
    .await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;
    assert_eq!(
        worktree_project["worktree"]["parentProjectId"],
        parent_project["projectId"]
    );

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/deleteWorktreeProject",
            &token,
            json!({
                "params": {
                    "deleteLocalBranch": true,
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-delete-clean-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    assert_eq!(
        body["result"]["checkoutRemoval"],
        json!({ "forced": false, "retriedForSubmodules": false })
    );
    assert_eq!(body["result"]["warnings"], json!([]));
    assert_eq!(
        body["result"]["project"]["projectId"],
        worktree_project["projectId"]
    );
    assert!(!worktree.exists());
    assert_eq!(
        run_git_status_for_server_test(&parent, &["rev-parse", "--verify", "feature-clean"])
            .status
            .code(),
        Some(128)
    );

    let projects = route_http(
        state,
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-after-delete-clean".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert!(!body["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["projectId"] == worktree_project["projectId"]));
}

#[tokio::test]
async fn renaming_a_worktree_updates_the_project_path_and_every_session_cwd() {
    /*
    CDXC:WorktreeRename 2026-08-09-18:40:
    The lockstep contract. A rename that moves the folder but leaves the
    database describing the old one is worse than no feature: the sidebar row
    points at a dead path, `start_session_provider` refuses to start anything
    there, and the V2 worktree marker silently stops being renameable. Assert
    the whole set in one pass — project path, derived label, re-detected
    worktree metadata, every session cwd, and the marker path — because they
    only have value together.
    */
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("rename-worktree-parent");
    let worktree = paths.root_dir.join("rename-worktree-parent-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "ghostex/0123abcd"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "ghostex/0123abcd",
        ],
    );

    let parent_project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;
    assert_eq!(
        worktree_project["worktree"]["parentProjectId"],
        parent_project["projectId"]
    );

    let marker = worktree_sessions::worktree_session_marker_value(
        "ghostex/0123abcd",
        &path_to_string(&worktree),
        "Codex Session",
        "2026-07-29T00:00:00.000Z",
    );
    let created = route_http(
        state.clone(),
        rpc_request(
            "/api/createSession",
            &token,
            json!({
                "params": {
                    "cwd": path_to_string(&worktree.join("packages/app")),
                    "kind": "terminal",
                    "projectId": worktree_project["projectId"],
                    "runtimeSettings": {
                        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker,
                    },
                    "title": "Codex Session",
                }
            }),
        ),
        "request-create-rename-session".to_string(),
    )
    .await;
    assert_eq!(created.response.status(), StatusCode::OK);
    let session = response_json(created.response).await["result"]["session"].clone();

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "feat/kanban-assignee",
                    "projectId": worktree_project["projectId"],
                    "renameBranch": true
                }
            }),
        ),
        "request-rename-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    let renamed = paths
        .root_dir
        .join("rename-worktree-parent-feat-kanban-assignee");

    assert_eq!(body["result"]["movedFolder"], json!(true));
    assert_eq!(
        body["result"]["renamedBranch"],
        json!("feat/kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["path"],
        json!(path_to_string(&renamed))
    );
    assert_eq!(
        body["result"]["project"]["name"],
        json!("Parent-feat-kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["worktree"]["name"],
        json!("rename-worktree-parent-feat-kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["worktree"]["branch"],
        json!("feat/kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["worktree"]["createdAt"],
        worktree_project["worktree"]["createdAt"],
        "a rename is not a new checkout"
    );
    assert!(renamed.is_dir());
    assert!(!worktree.exists());
    assert_eq!(
        run_git_for_server_test(&renamed, &["branch", "--show-current"]).trim(),
        "feat/kanban-assignee"
    );

    let sessions = route_http(
        state,
        rpc_request(
            "/api/listSessions",
            &token,
            json!({ "params": { "projectId": worktree_project["projectId"] } }),
        ),
        "request-list-renamed-sessions".to_string(),
    )
    .await;
    let sessions = response_json(sessions.response).await;
    let moved = sessions["result"]["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|candidate| candidate["sessionId"] == session["sessionId"])
        .cloned()
        .expect("renamed session");
    assert_eq!(
        moved["cwd"],
        json!(path_to_string(&renamed.join("packages/app"))),
        "a cwd inside the moved folder follows it"
    );
    let moved_marker = &moved["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY];
    assert_eq!(moved_marker["path"], json!(path_to_string(&renamed)));
    assert_eq!(moved_marker["branch"], json!("feat/kanban-assignee"));
    assert_eq!(moved_marker["initialTitle"], json!("Codex Session"));
}

#[test]
fn a_session_cwd_recorded_through_the_resolved_path_form_still_follows_the_move() {
    /*
    CDXC:WorktreeRename 2026-08-10:
    Nothing forces a session's stored `cwd` to use the same spelling of a
    folder that the project row happens to carry, and on macOS every path
    under `/tmp` or `/var` has two: `/tmp/rt-old` and `/private/tmp/rt-old`.
    Compared lexically against the project row's spelling alone, a cwd
    recorded through the other one matched no prefix, was left untouched, and
    pointed into a folder that had just moved — which breaks that session's
    next cold start, because `start_session_provider` bakes the cwd into the
    generated run script. Both spellings of the old folder rebase; anything
    genuinely outside it still does not.
    */
    let plan = RenameWorktreeProjectPlan {
        destination_path: "/tmp/rt-new".to_string(),
        moves_folder: true,
        params: RenameWorktreeProjectParams {
            name: "new".to_string(),
            project_id: "project-1".to_string(),
            rename_branch: false,
        },
        parent_path: "/tmp/rt".to_string(),
        parent_project_name: "Parent".to_string(),
        projects: Vec::new(),
        worktree_branch: None,
        worktree_path: "/tmp/rt-old".to_string(),
        worktree_path_resolved: Some("/private/tmp/rt-old".to_string()),
    };

    assert_eq!(
        rebase_renamed_worktree_path("/tmp/rt-old/packages/app", &plan).as_deref(),
        Some("/tmp/rt-new/packages/app"),
        "the spelling the project row carries"
    );
    assert_eq!(
        rebase_renamed_worktree_path("/private/tmp/rt-old/packages/app", &plan).as_deref(),
        Some("/tmp/rt-new/packages/app"),
        "the spelling the filesystem resolves to"
    );
    assert_eq!(
        rebase_renamed_worktree_path("/private/tmp/rt-old", &plan).as_deref(),
        Some("/tmp/rt-new")
    );
    assert_eq!(
        rebase_renamed_worktree_path("/private/tmp/rt-older/src", &plan),
        None,
        "a sibling that merely shares a prefix is not inside the moved folder"
    );
    assert_eq!(
        rebase_renamed_worktree_path("/tmp/somewhere-else", &plan),
        None
    );
}

#[tokio::test]
async fn renaming_a_worktree_refuses_a_taken_folder_and_a_taken_branch() {
    /*
    CDXC:WorktreeRename 2026-08-09-18:40:
    Both refusals must land BEFORE anything is touched, and both must say
    which name is in the way. The folder case is the important one: with the
    destination already present, `git worktree move` exits 0 and nests the
    checkout one level deeper, so "no error" would otherwise mean "the folder
    is somewhere nobody asked for".
    */
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("rename-guard-parent");
    let worktree = paths.root_dir.join("rename-guard-parent-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "ghostex/0123abcd"]);
    run_git_for_server_test(&parent, &["branch", "feat/taken"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "ghostex/0123abcd",
        ],
    );
    fs::create_dir_all(paths.root_dir.join("rename-guard-parent-busy")).expect("busy dir");

    add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let taken_folder = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "busy",
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-rename-taken-folder".to_string(),
    )
    .await;
    assert_eq!(taken_folder.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(taken_folder.response).await;
    assert_eq!(
        body["message"],
        json!("A folder named \"rename-guard-parent-busy\" already exists next to the project.")
    );
    assert!(worktree.is_dir(), "the worktree never moved");

    let taken_branch = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "feat/taken",
                    "projectId": worktree_project["projectId"],
                    "renameBranch": true
                }
            }),
        ),
        "request-rename-taken-branch".to_string(),
    )
    .await;
    assert_eq!(taken_branch.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(taken_branch.response).await;
    assert_eq!(
        body["message"],
        json!("Branch \"feat/taken\" already exists.")
    );
    assert!(
        worktree.is_dir(),
        "a refused branch rename never moves the folder"
    );
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/0123abcd"
    );

    let nothing = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "old",
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-rename-nothing".to_string(),
    )
    .await;
    assert_eq!(nothing.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(nothing.response).await;
    assert_eq!(body["message"], json!("Nothing to rename."));

    /*
    CDXC:WorktreeRename 2026-08-10:
    Asking to rename the branch to the name it already carries, on a folder
    that is already correct, changes nothing — and reporting success for it
    tells the user something happened. The checkbox being ticked is not
    enough to call it a rename.
    */
    run_git_for_server_test(&parent, &["branch", "-m", "ghostex/0123abcd", "old"]);
    let no_op_branch = route_http(
        state,
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "old",
                    "projectId": worktree_project["projectId"],
                    "renameBranch": true
                }
            }),
        ),
        "request-rename-noop-branch".to_string(),
    )
    .await;
    assert_eq!(no_op_branch.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(no_op_branch.response).await;
    assert_eq!(body["message"], json!("Nothing to rename."));
}

// Symlinks are the whole subject of this test, and `std::os::unix::fs` does
// not exist off unix — without the gate the module stops compiling there.
#[cfg(unix)]
#[tokio::test]
async fn renaming_explains_a_worktree_registered_through_a_different_path_form() {
    /*
    CDXC:WorktreeRename 2026-08-09-18:40:
    Reproduces a real failure from manual testing on macOS: the project was
    registered as `/tmp/rt` while its worktree resolved to
    `/private/tmp/rt-old`, because `git worktree list` reports the symlink-
    resolved path and the project kept the typed one. The typed operation
    compares paths lexically by design, so it refused with a sentence about
    `worktreePath` that meant nothing to the user. The rename must explain
    which two things disagree instead of forwarding that.
    */
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let real_root = paths.root_dir.join("symlink-family");
    fs::create_dir_all(&real_root).expect("real root");
    let parent = real_root.join("rt");
    let worktree = real_root.join("rt-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "feat/old"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feat/old",
        ],
    );

    // The parent is registered through a symlinked alias of the same folder,
    // exactly as `/tmp/rt` aliases `/private/tmp/rt`.
    let alias_root = paths.root_dir.join("symlink-alias");
    std::os::unix::fs::symlink(&real_root, &alias_root).expect("symlink");
    let aliased_parent = alias_root.join("rt");

    add_project_path_for_server_test(state.clone(), &token, &aliased_parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let response = route_http(
        state,
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "renamed",
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-rename-symlinked-family".to_string(),
    )
    .await;

    assert_eq!(response.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response.response).await;
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("registered under different paths"),
        "expected the mismatch explained, got: {message}"
    );
    assert!(
        !message.contains("worktreePath"),
        "the internal typed-operation sentence must not reach the user: {message}"
    );
    assert!(worktree.is_dir(), "nothing was touched");
}

#[tokio::test]
async fn delete_worktree_project_route_force_removes_dirty_checkout_and_warns_for_remote() {
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let remote = paths.root_dir.join("delete-worktree-origin.git");
    let parent = paths.root_dir.join("delete-worktree-remote-parent");
    let worktree = paths.root_dir.join("delete-worktree-remote-parent-feature");
    run_git_for_server_test(
        &paths.root_dir,
        &["init", "--bare", path_to_string(&remote).as_str()],
    );
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &["remote", "add", "origin", path_to_string(&remote).as_str()],
    );
    run_git_for_server_test(&parent, &["push", "-u", "origin", "HEAD:main"]);
    run_git_for_server_test(&parent, &["branch", "feature-remote"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feature-remote",
        ],
    );
    fs::write(worktree.join("dirty.txt"), "not committed\n").expect("dirty file");

    add_project_path_for_server_test(
        state.clone(),
        &token,
        &parent,
        Some("Delete Worktree Remote Parent"),
    )
    .await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/deleteWorktreeProject",
            &token,
            json!({
                "params": {
                    "deleteRemoteBranch": true,
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-delete-dirty-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    assert_eq!(
        body["result"]["checkoutRemoval"],
        json!({ "forced": true, "retriedForSubmodules": false })
    );
    assert!(body["result"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "remoteBranchDeleteFailed"));
    assert!(!worktree.exists());

    let projects = route_http(
        state,
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-after-delete-dirty".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert!(!body["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["projectId"] == worktree_project["projectId"]));
}

#[tokio::test]
async fn delete_worktree_project_route_retries_clean_initialized_submodule_with_force() {
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let submodule = paths.root_dir.join("delete-worktree-submodule-source");
    let parent = paths.root_dir.join("delete-worktree-submodule-parent");
    let worktree = paths
        .root_dir
        .join("delete-worktree-submodule-parent-feature");
    create_git_repository_for_server_test(&submodule);
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            path_to_string(&submodule).as_str(),
            "deps/submodule",
        ],
    );
    run_git_for_server_test(&parent, &["commit", "-m", "add submodule"]);
    run_git_for_server_test(&parent, &["branch", "feature-submodule"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feature-submodule",
        ],
    );
    run_git_for_server_test(
        &worktree,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
            "--recursive",
        ],
    );

    add_project_path_for_server_test(
        state.clone(),
        &token,
        &parent,
        Some("Delete Worktree Submodule Parent"),
    )
    .await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let response = route_http(
        state,
        rpc_request(
            "/api/deleteWorktreeProject",
            &token,
            json!({
                "params": {
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-delete-submodule-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    assert_eq!(
        body["result"]["checkoutRemoval"],
        json!({ "forced": true, "retriedForSubmodules": true })
    );
    assert!(!worktree.exists());
}

// Sidebar V2 worktree sessions
// -----------------------------------------------------------------------

async fn worktree_session_context_for_test(
    state: Arc<AppState>,
    project_id: &str,
) -> ProjectWorktreeOperationContext {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    match resolve_project_worktree_operation_context(&state, &params) {
        Ok(context) => context,
        Err(_) => panic!("worktree operation context"),
    }
}

async fn set_worktree_command_for_test(
    state: Arc<AppState>,
    token: &str,
    project_id: &str,
    command: &str,
) {
    let response = route_http(
        state,
        rpc_request(
            "/api/updateProject",
            token,
            json!({
                "params": {
                    "gitConfig": { "worktreeCommand": command },
                    "projectId": project_id,
                }
            }),
        ),
        "request-set-worktree-command".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
}

#[tokio::test]
async fn worktree_session_checkout_creates_a_temp_branch_and_runs_the_setup_command() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("worktree-session-parent");
    create_git_repository_for_server_test(&parent);
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    set_worktree_command_for_test(
        state.clone(),
        &token,
        &project_id,
        "printf 'setup\\n' > setup-ran.txt",
    )
    .await;

    let context = worktree_session_context_for_test(state.clone(), &project_id).await;
    let request = normalize_worktree_session_create_request(&Map::new()).expect("request");
    let prepared = match prepare_worktree_session_checkout(&state, &context, &request).await {
        Ok(prepared) => prepared,
        Err(_) => panic!("prepare worktree checkout"),
    };

    assert!(prepared.created);
    assert!(
        worktree_sessions::is_worktree_temp_branch(&prepared.branch),
        "unexpected branch {}",
        prepared.branch
    );
    assert!(Path::new(&prepared.path).is_dir());
    assert!(
        Path::new(&prepared.path).join("setup-ran.txt").is_file(),
        "the project's worktree setup command runs inside the new checkout"
    );
    assert_eq!(
        run_git_for_server_test(Path::new(&prepared.path), &["branch", "--show-current"]).trim(),
        prepared.branch
    );
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}", prepared.branch)
            ]
        )
        .status
        .code(),
        Some(0)
    );
    // The worktree is a session attribute, never a registered project.
    let projects = route_http(
        state.clone(),
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-after-worktree-session".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert_eq!(body["result"]["projects"].as_array().unwrap().len(), 1);

    // An explicit base branch seeds the checkout from that branch's tip.
    run_git_for_server_test(&parent, &["checkout", "--quiet", "-b", "seed-branch"]);
    fs::write(parent.join("seed.txt"), "seed\n").expect("seed file");
    run_git_for_server_test(&parent, &["add", "seed.txt"]);
    run_git_for_server_test(&parent, &["commit", "-m", "seed"]);
    run_git_for_server_test(&parent, &["checkout", "--quiet", "-"]);
    let mut base_params = Map::new();
    base_params.insert("baseBranch".to_string(), json!("seed-branch"));
    let base_request = normalize_worktree_session_create_request(&base_params).expect("request");
    let based = match prepare_worktree_session_checkout(&state, &context, &base_request).await {
        Ok(prepared) => prepared,
        Err(_) => panic!("prepare worktree checkout from base branch"),
    };
    assert!(Path::new(&based.path).join("seed.txt").is_file());

    // Without a remote there is nothing to start from, and the refusal is
    // explicit instead of silently falling back to the local branch.
    let mut origin_params = Map::new();
    origin_params.insert("baseBranch".to_string(), json!("seed-branch"));
    origin_params.insert("startFromOrigin".to_string(), json!(true));
    let origin_request =
        normalize_worktree_session_create_request(&origin_params).expect("request");
    let error = prepare_worktree_session_checkout(&state, &context, &origin_request)
        .await
        .err()
        .expect("origin failure");
    match error {
        ProjectWorktreeOperationError::Domain(error) => {
            assert!(error.message.contains("origin/seed-branch"));
        }
        _ => panic!("expected a domain failure"),
    }
}

#[tokio::test]
async fn worktree_session_checkout_rolls_back_when_the_setup_command_fails() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("worktree-session-rollback-parent");
    create_git_repository_for_server_test(&parent);
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    set_worktree_command_for_test(state.clone(), &token, &project_id, "exit 3").await;

    let context = worktree_session_context_for_test(state.clone(), &project_id).await;
    let request = normalize_worktree_session_create_request(&Map::new()).expect("request");
    let error = prepare_worktree_session_checkout(&state, &context, &request)
        .await
        .err()
        .expect("setup failure");
    match error {
        ProjectWorktreeOperationError::Typed(error) => {
            assert!(error.message.contains("Worktree setup command failed."));
        }
        _ => panic!("expected a typed operation failure"),
    }

    let worktrees =
        run_git_for_server_test(&parent, &["worktree", "list", "--porcelain"]).to_string();
    assert_eq!(
        worktrees.matches("worktree ").count(),
        1,
        "the failed checkout is removed again: {worktrees}"
    );
    let branches = run_git_for_server_test(&parent, &["branch", "--list", "ghostex/*"]);
    assert!(
        branches.trim().is_empty(),
        "the temp branch is deleted too: {branches}"
    );
    let siblings = fs::read_dir(&paths.root_dir)
        .expect("root dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("worktree-session-rollback-parent-")
        })
        .count();
    assert_eq!(siblings, 0, "no stray worktree directory survives");
}

#[tokio::test]
async fn create_worktree_session_route_rejects_a_foreign_existing_worktree_path() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("worktree-session-foreign-parent");
    let foreign = paths.root_dir.join("worktree-session-foreign-other");
    create_git_repository_for_server_test(&parent);
    create_git_repository_for_server_test(&foreign);
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/createWorktreeSession",
            &token,
            json!({
                "params": {
                    "existingWorktree": { "path": path_to_string(&foreign) },
                    "projectId": project["projectId"],
                }
            }),
        ),
        "request-create-worktree-session-foreign".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response.response).await;
    assert_eq!(body["error"], json!("badRequest"));
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("is not a worktree of this project"));

    let missing = route_http(
        state,
        rpc_request(
            "/api/createWorktreeSession",
            &token,
            json!({
                "params": {
                    "existingWorktree": { "path": path_to_string(&paths.root_dir.join("nope")) },
                    "projectId": project["projectId"],
                }
            }),
        ),
        "request-create-worktree-session-missing".to_string(),
    )
    .await;
    assert_eq!(missing.response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn remove_session_worktree_route_answers_dirty_before_removing_and_force_overrides() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("remove-session-worktree-dirty-parent");
    let worktree = paths
        .root_dir
        .join("remove-session-worktree-dirty-parent-0123abcd");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&worktree).as_str(),
        ],
    );
    fs::write(worktree.join("README.md"), "dirty\n").expect("dirty file");
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

    let dirty = route_http(
        state.clone(),
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&worktree),
                }
            }),
        ),
        "request-remove-session-worktree-dirty".to_string(),
    )
    .await;
    assert_eq!(dirty.response.status(), StatusCode::OK);
    let body = response_json(dirty.response).await;
    assert_eq!(body["result"]["removed"], json!(false));
    assert_eq!(body["result"]["dirty"], json!(true));
    assert_eq!(
        body["result"]["warnings"],
        json!(["This worktree has uncommitted changes."])
    );
    assert!(
        worktree.is_dir(),
        "a dirty worktree is never removed silently"
    );

    let forced = route_http(
        state,
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "force": true,
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&worktree),
                }
            }),
        ),
        "request-remove-session-worktree-force".to_string(),
    )
    .await;
    assert_eq!(forced.response.status(), StatusCode::OK);
    let body = response_json(forced.response).await;
    assert_eq!(body["result"]["removed"], json!(true));
    assert_eq!(body["result"]["dirty"], json!(true));
    assert_eq!(body["result"]["warnings"], json!([]));
    assert!(!worktree.exists());
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
        )
        .status
        .code(),
        Some(128),
        "force deletes the managed temp branch too"
    );
}

#[tokio::test]
async fn remove_session_worktree_route_keeps_branches_it_does_not_manage() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("remove-session-worktree-clean-parent");
    let managed = paths
        .root_dir
        .join("remove-session-worktree-clean-parent-0123abcd");
    let foreign = paths
        .root_dir
        .join("remove-session-worktree-clean-parent-feature");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&managed).as_str(),
        ],
    );
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "feature-work",
            path_to_string(&foreign).as_str(),
        ],
    );
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

    let removed = route_http(
        state.clone(),
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&managed),
                }
            }),
        ),
        "request-remove-session-worktree-clean".to_string(),
    )
    .await;
    assert_eq!(removed.response.status(), StatusCode::OK);
    let body = response_json(removed.response).await;
    assert_eq!(body["result"]["removed"], json!(true));
    assert_eq!(body["result"]["dirty"], json!(false));
    assert_eq!(body["result"]["warnings"], json!([]));
    assert!(!managed.exists());
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
        )
        .status
        .code(),
        Some(128)
    );

    let untouched = route_http(
        state.clone(),
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&foreign),
                }
            }),
        ),
        "request-remove-session-worktree-foreign-branch".to_string(),
    )
    .await;
    assert_eq!(untouched.response.status(), StatusCode::OK);
    let body = response_json(untouched.response).await;
    assert_eq!(body["result"]["removed"], json!(true));
    assert!(!foreign.exists());
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/feature-work"]
        )
        .status
        .code(),
        Some(0),
        "a branch gxserver did not mint survives the worktree removal"
    );

    let outside = route_http(
        state,
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&parent),
                }
            }),
        ),
        "request-remove-session-worktree-main".to_string(),
    )
    .await;
    assert_eq!(
        outside.response.status(),
        StatusCode::BAD_REQUEST,
        "the project's own checkout is not a removable worktree"
    );
}

#[tokio::test]
async fn remove_session_worktree_route_refuses_a_registered_worktree_project() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths
        .root_dir
        .join("remove-session-worktree-registered-parent");
    let registered = paths
        .root_dir
        .join("remove-session-worktree-registered-parent-0123abcd");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&registered).as_str(),
        ],
    );
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    // The V1 flow's registration: the worktree is a project in its own right.
    add_project_path_for_server_test(state.clone(), &token, &registered, Some("Worktree")).await;

    let refused = route_http(
        state,
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&registered),
                }
            }),
        ),
        "request-remove-session-worktree-registered".to_string(),
    )
    .await;
    assert_eq!(refused.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(refused.response).await;
    assert_eq!(body["error"], json!("badRequest"));
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("registered as its own project"),
        "the refusal points at the project delete flow: {}",
        body["message"]
    );
    assert!(
        registered.is_dir(),
        "a registered worktree project's checkout survives the refusal"
    );
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
        )
        .status
        .code(),
        Some(0),
        "its branch survives too"
    );
}

#[tokio::test]
async fn worktree_branch_rename_pass_renames_only_a_titled_temp_branch() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("rename-parent");
    let worktree = paths.root_dir.join("rename-parent-0123abcd");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&worktree).as_str(),
        ],
    );
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let marker = worktree_sessions::worktree_session_marker_value(
        "ghostex/0123abcd",
        &path_to_string(&worktree),
        "Codex Session",
        "2026-07-29T00:00:00.000Z",
    );
    let created = route_http(
        state.clone(),
        rpc_request(
            "/api/createSession",
            &token,
            json!({
                "params": {
                    "cwd": path_to_string(&worktree),
                    "kind": "terminal",
                    "projectId": project["projectId"],
                    "runtimeSettings": {
                        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker.clone(),
                    },
                    "title": "Codex Session",
                }
            }),
        ),
        "request-create-worktree-session-row".to_string(),
    )
    .await;
    assert_eq!(created.response.status(), StatusCode::OK);
    let session = response_json(created.response).await["result"]["session"].clone();

    // A row still carrying its creation title is not due a rename.
    run_worktree_branch_rename_once(&state).expect("rename pass");
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/0123abcd"
    );

    let renamed = route_http(
        state.clone(),
        rpc_request(
            "/api/updateSession",
            &token,
            json!({
                "params": {
                    "projectId": session["projectId"],
                    "runtimeSettings": {
                        "titleSource": "generated",
                        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker,
                    },
                    "sessionId": session["sessionId"],
                    "title": "Fix the flaky login test",
                }
            }),
        ),
        "request-title-worktree-session-row".to_string(),
    )
    .await;
    assert_eq!(renamed.response.status(), StatusCode::OK);

    run_worktree_branch_rename_once(&state).expect("rename pass");
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/fix-the-flaky-login-test"
    );

    // The marker now records the new branch, so the next pass is a no-op.
    let listed = route_http(
        state.clone(),
        rpc_request(
            "/api/listSessions",
            &token,
            json!({ "params": { "projectId": session["projectId"] } }),
        ),
        "request-list-renamed-worktree-session".to_string(),
    )
    .await;
    let body = response_json(listed.response).await;
    let stored = body["result"]["sessions"][0].clone();
    assert_eq!(
        stored["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY]["branch"],
        json!("ghostex/fix-the-flaky-login-test")
    );
    assert!(
        stored["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY]["renamedAt"]
            .is_string()
    );
    run_worktree_branch_rename_once(&state).expect("rename pass");
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/fix-the-flaky-login-test"
    );
}

fn test_app_state(paths: GxserverPaths) -> Arc<AppState> {
    let storage = initialize_gxserver_storage(&paths).expect("storage");
    let config = create_default_gxserver_config().expect("config");
    let metadata = RuntimeMetadata {
        build_identity: "test-build".to_string(),
        pid: std::process::id(),
        port: config.listeners.local.port,
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        server_id: "S7k".to_string(),
        started_at: "2026-05-30T10:00:00.000Z".to_string(),
        version: "0.0.0-test".to_string(),
    };
    let (shutdown_tx, _) = broadcast::channel(8);
    let automation_runtime = AutomationRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        format!(
            "http://{}:{}",
            config.listeners.local.host, config.listeners.local.port
        ),
    );
    let event_hub = GxserverEventHub::new(metadata.server_id.clone());
    let presentation_event_sequence = Arc::new(Mutex::new(()));
    let delayed_send_runtime = DelayedSendRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        event_hub.clone(),
        presentation_event_sequence.clone(),
    );
    let extension_registry = ExtensionRegistry::new_with_api_url(
        &paths,
        format!(
            "http://{}:{}",
            config.listeners.local.host, config.listeners.local.port
        ),
    );
    Arc::new(AppState {
        auth_token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        automation_runtime,
        delayed_send_runtime,
        board_start_work_gate: Arc::new(Mutex::new(())),
        build_identity: "test-build".to_string(),
        config,
        event_hub,
        extension_registry,
        logger: Arc::new(GxserverLogger::new(paths.clone())),
        metadata,
        migration: create_gxserver_migration_status(&storage),
        paths,
        presentation_event_sequence,
        repository_clone_jobs: RepositoryCloneJobManager::default(),
        session_chat_followers: Arc::new(Mutex::new(HashMap::new())),
        session_chat_option_cache: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx,
        stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
        tailcat_runtime: crate::tailcat::TailcatRuntime::new(),
        version: "0.0.0-test".to_string(),
        zmx_title_observers: Arc::new(Mutex::new(HashMap::new())),
    })
}

async fn add_project_path_for_server_test(
    state: Arc<AppState>,
    token: &str,
    project_path: &Path,
    name: Option<&str>,
) -> Value {
    let mut params = Map::new();
    params.insert(
        "path".to_string(),
        Value::String(path_to_string(project_path)),
    );
    if let Some(name) = name {
        params.insert("name".to_string(), Value::String(name.to_string()));
    }
    let response = route_http(
        state,
        rpc_request(
            "/api/addProjectPath",
            token,
            json!({ "params": Value::Object(params) }),
        ),
        "request-add-project-path".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    response_json(response.response).await["result"]["project"].clone()
}

fn git_available() -> bool {
    StdCommand::new("git")
        .arg("--version")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn create_git_repository_for_server_test(repository_path: &Path) {
    fs::create_dir_all(repository_path).expect("repo dir");
    run_git_for_server_test(repository_path, &["init"]);
    run_git_for_server_test(
        repository_path,
        &["config", "user.email", "ghostex-tests@example.invalid"],
    );
    run_git_for_server_test(repository_path, &["config", "user.name", "Ghostex Tests"]);
    fs::write(repository_path.join("README.md"), "initial\n").expect("readme");
    run_git_for_server_test(repository_path, &["add", "README.md"]);
    run_git_for_server_test(repository_path, &["commit", "-m", "initial"]);
}

fn run_git_for_server_test(cwd: &Path, args: &[&str]) -> String {
    let output = run_git_status_for_server_test(cwd, args);
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_git_status_for_server_test(cwd: &Path, args: &[&str]) -> std::process::Output {
    StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git command")
}

fn test_health(build_identity: &str) -> ServerHealthResponse {
    let config = create_default_gxserver_config().expect("config");
    ServerHealthResponse {
        ok: true,
        product: GXSERVER_PRODUCT.to_string(),
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        version: "0.1.0".to_string(),
        build_identity: build_identity.to_string(),
        capabilities: vec![],
        listeners: config.listeners.clone(),
        migration: MigrationStatus {
            applied_migrations: vec![],
            current_version: 0,
            state_db_file: String::new(),
            state_imports: None,
        },
        pid: 123,
        portless: crate::portless::unavailable_portless_status_payload(),
        port: config.listeners.local.port,
        server_id: "S7k".to_string(),
        started_at: "2026-05-30T10:00:00.000Z".to_string(),
        tools: vec![],
    }
}

fn rpc_request(path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(
            GXSERVER_PROTOCOL_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

async fn response_json(response: Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}
