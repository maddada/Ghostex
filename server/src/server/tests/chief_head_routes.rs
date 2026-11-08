//! Chief's head role through the real HTTP gate: a head token against an ordinary agent token on
//! every route that asks for the owner's approval (chief/head.rs, security/capability.rs).

use super::*;
use crate::security::SessionRole;

fn temp_state() -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    std::fs::create_dir_all(&paths.app_config_dir).expect("app config directory");
    (temp, test_app_state(paths))
}

async fn post(
    state: &Arc<AppState>,
    path: &str,
    token: &str,
    params: Value,
) -> (StatusCode, Value) {
    let routed = route_http(
        state.clone(),
        rpc_request(path, token, json!({ "params": params })),
        "test".to_string(),
    )
    .await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

fn error_code(body: &Value) -> String {
    body["error"]
        .as_str()
        .or_else(|| body["error"]["code"].as_str())
        .unwrap_or_default()
        .to_string()
}

fn set_chief_hidden(state: &AppState, hidden: bool) {
    std::fs::write(
        state
            .paths
            .app_config_dir
            .join("native-sidebar-settings.json"),
        json!({ "chiefHidden": hidden }).to_string(),
    )
    .expect("settings file");
}

/// The three "steer another session" routes: Chief skips their approval, an agent asks.
fn steering_routes() -> Vec<(&'static str, Value)> {
    let target = |extra: Value| {
        let mut params = json!({ "projectId": "P1", "sessionId": "OTHER" });
        for (key, value) in extra.as_object().expect("object") {
            params[key] = value.clone();
        }
        params
    };
    vec![
        ("/api/sendSessionText", target(json!({ "text": "hello" }))),
        ("/api/sendSessionEnter", target(json!({}))),
        (
            "/api/sendSessionMessage",
            target(json!({ "message": "hello" })),
        ),
        ("/api/interruptSessionChat", target(json!({}))),
        (
            "/api/answerSessionChatPrompt",
            target(json!({ "answer": { "optionIndex": 0 } })),
        ),
    ]
}

/// Routes whose action stays the owner's (#495): Chief gets exactly an agent's answer.
fn owner_only_routes() -> Vec<(&'static str, Value)> {
    vec![
        (
            "/api/updateTailcatState",
            json!({ "kind": "setEnabled", "enabled": true }),
        ),
        (
            "/api/autopilot",
            json!({ "op": "set", "patch": { "answerApprovals": true } }),
        ),
        (
            "/api/autopilot",
            json!({ "op": "set", "patch": { "limits": {} } }),
        ),
        ("/api/gbots", json!({ "action": "create", "name": "bot" })),
        (
            "/api/dispatchRendererCommand",
            json!({ "action": "updateSettingsPatch", "payload": { "patch": { "gbotsEnabled": true } } }),
        ),
        (
            "/api/dispatchRendererCommand",
            json!({ "action": "updateSettingsPatch", "payload": { "patch": { "chiefHidden": false } } }),
        ),
        (
            "/api/registerConnectedComputer",
            json!({ "serverId": "S9new", "name": "Server" }),
        ),
        (
            "/api/issueComputerGrant",
            json!({ "serverId": "S9new", "name": "Server", "agentAccess": true }),
        ),
        ("/api/listApprovalRequests", json!({})),
        ("/api/approveApprovalRequest", json!({ "id": "apr_x" })),
        ("/api/denyApprovalRequest", json!({ "id": "apr_x" })),
        ("/api/listApproverKeys", json!({})),
        ("/api/removeApproverKey", json!({ "keyId": "k" })),
        ("/api/enrollApproverKey", json!({ "key": {} })),
        ("/api/chief", json!({ "op": "open", "agentId": "claude" })),
        ("/api/chief", json!({ "op": "newChat" })),
        (
            "/api/chief",
            json!({ "op": "setAgent", "agentId": "codex" }),
        ),
    ]
}

#[tokio::test]
async fn a_head_token_steers_any_session_but_every_owner_action_still_stops_it() {
    let (_temp, state) = temp_state();
    let tokens = &state.security.session_tokens;
    let head = tokens.mint_session("P0", "CHIEF", SessionRole::Coordinator);
    let agent = tokens.mint_session("P1", "AGENT", SessionRole::Agent);
    let coordinator = tokens.mint_session("P1", "COORD", SessionRole::Coordinator);
    crate::chief::record_chief_for_tests(&state.paths, "P0", "CHIEF");

    for (path, params) in steering_routes() {
        let (_, body) = post(&state, path, &head, params.clone()).await;
        assert_ne!(error_code(&body), "approvalRequired", "head {path}: {body}");
        assert_ne!(error_code(&body), "forbidden", "head {path}: {body}");
        for token in [&agent, &coordinator] {
            let (status, body) = post(&state, path, token, params.clone()).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "agent {path}: {body}");
            assert_eq!(
                error_code(&body),
                "approvalRequired",
                "agent {path}: {body}"
            );
        }
    }

    for (path, params) in owner_only_routes() {
        let (head_status, head_body) = post(&state, path, &head, params.clone()).await;
        let (agent_status, agent_body) = post(&state, path, &agent, params.clone()).await;
        assert!(
            matches!(
                error_code(&head_body).as_str(),
                "approvalRequired" | "forbidden"
            ),
            "head {path} {params}: {head_status} {head_body}"
        );
        assert_eq!(
            (head_status, error_code(&head_body)),
            (agent_status, error_code(&agent_body)),
            "head and agent differ on {path} {params}: {head_body} / {agent_body}"
        );
    }
}

#[tokio::test]
async fn the_head_role_vanishes_when_chief_is_turned_off_or_moves_to_a_new_chat() {
    let (_temp, state) = temp_state();
    let tokens = &state.security.session_tokens;
    let head = tokens.mint_session("P0", "CHIEF", SessionRole::Coordinator);
    crate::chief::record_chief_for_tests(&state.paths, "P0", "CHIEF");
    let (path, params) = steering_routes().remove(0);

    set_chief_hidden(&state, true);
    let (status, body) = post(&state, path, &head, params.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(error_code(&body), "approvalRequired", "off: {body}");

    set_chief_hidden(&state, false);
    let (_, body) = post(&state, path, &head, params.clone()).await;
    assert_ne!(error_code(&body), "approvalRequired", "back on: {body}");

    // New Chat points the record at another session: the old token is an ordinary one again.
    crate::chief::record_chief_for_tests(&state.paths, "P0", "NEWCHIEF");
    let (_, body) = post(&state, path, &head, params).await;
    assert_eq!(error_code(&body), "approvalRequired", "old chat: {body}");
}

#[tokio::test]
async fn only_the_owner_and_the_users_terminal_may_open_chief() {
    let (_temp, state) = temp_state();
    let session = state
        .security
        .session_tokens
        .mint_session("P1", "AGENT", SessionRole::Agent);
    let (status, body) = post(
        &state,
        "/api/chief",
        &session,
        json!({ "op": "open", "agentId": "claude" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // Status is open to any caller and says whether Chief is on.
    let (status, body) = post(&state, "/api/chief", &session, json!({ "op": "status" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["enabled"], true);
    assert_eq!(body["result"]["needsAgent"], true);
    // The user's own terminal gets past the gate (and then needs an agent pick).
    let main = state.auth_token.clone();
    let (_, body) = post(&state, "/api/chief", &main, json!({ "op": "open" })).await;
    assert_eq!(error_code(&body), "chiefNeedsAgent", "{body}");
    set_chief_hidden(&state, true);
    let (_, body) = post(
        &state,
        "/api/chief",
        &main,
        json!({ "op": "open", "agentId": "claude" }),
    )
    .await;
    assert_eq!(error_code(&body), "turnedOff", "{body}");
}
