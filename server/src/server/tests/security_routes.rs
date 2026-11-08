//! The approval API through the real HTTP gate (crate::security, GitHub #495).

use super::*;
use crate::security::{test_keys::TestApprover, ActionRequest, SensitiveAction, SessionRole};
use ghostex_gx_protocol::approval::approval_signing_payload;

fn temp_state() -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
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

fn easy_connect() -> ActionRequest {
    ActionRequest::new(SensitiveAction::EasyConnectEnable, "Turn on Easy Connect")
        .params(json!({ "enabled": true }))
}

#[tokio::test]
async fn agents_cannot_list_or_decide_approvals() {
    let (_temp, state) = temp_state();
    let main = state.auth_token.clone();
    let session = state
        .security
        .session_tokens
        .mint_session("p", "s", SessionRole::Coordinator);
    for token in [main.as_str(), session.as_str()] {
        for path in [
            "/api/listApprovalRequests",
            "/api/approveApprovalRequest",
            "/api/denyApprovalRequest",
            "/api/listApproverKeys",
        ] {
            let (status, body) = post(&state, path, token, json!({ "id": "apr_x" })).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        }
    }
    let (status, _) = post(
        &state,
        "/api/listApprovalRequests",
        "gxs_unknown",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_agent_asks_the_owner_approves_with_a_signature_and_the_retry_is_granted() {
    let (_temp, state) = temp_state();
    let approver = TestApprover::new("mac");
    state
        .security
        .approver_keys
        .insert(approver.key.clone())
        .expect("key");
    let owner = state.security.session_tokens.mint_owner("mac");
    let agent_token = state
        .security
        .session_tokens
        .mint_session("p", "s", SessionRole::Agent);
    let agent = state
        .security
        .session_tokens
        .resolve(&agent_token)
        .expect("agent");

    let body = json!({ "params": { "enabled": true } });
    let pending = require_owner_presence(&state, &agent, easy_connect(), &body, "/x", "r")
        .expect_err("needs approval");
    assert_eq!(pending.response.status(), StatusCode::FORBIDDEN);
    let pending = response_json(pending.response).await;
    assert_eq!(pending["error"], "approvalRequired");
    let id = pending["approval"]["id"].as_str().expect("id").to_string();

    // The asking agent can read its own request; the owner sees it in the list.
    let (status, read) = post(
        &state,
        "/api/readApprovalRequest",
        &agent_token,
        json!({ "id": id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["result"]["request"]["status"], "pending");
    let (_, list) = post(&state, "/api/listApprovalRequests", &owner, json!({})).await;
    assert_eq!(list["result"]["requests"][0]["id"], id.as_str());

    let request: crate::security::ApprovalRequest =
        serde_json::from_value(pending["approval"].clone()).expect("request");
    let signature = approver.sign(&approval_signing_payload("S1", &request).expect("payload"));
    let (status, approved) = post(
        &state,
        "/api/approveApprovalRequest",
        &owner,
        json!({ "id": id, "keyId": approver.key.key_id, "signature": signature }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["result"]["request"]["status"], "approved");

    let retry = json!({ "params": { "enabled": true, "approvalId": id } });
    assert!(require_owner_presence(&state, &agent, easy_connect(), &retry, "/x", "r").is_ok());
    assert!(require_owner_presence(&state, &agent, easy_connect(), &retry, "/x", "r").is_err());
}

#[tokio::test]
async fn the_desktop_enrolls_its_keys_and_a_second_approver_needs_an_approval() {
    use ghostex_gx_protocol::approval::owner_session_signing_payload;
    let (_temp, state) = temp_state();
    let main = state.auth_token.clone();
    let owner_key = TestApprover::new("owner");
    let mac = TestApprover::new("mac");
    let phone = TestApprover::new("phone");

    // First owner key: trusted from the token file; a different one afterwards is refused.
    let (status, body) = post(
        &state,
        "/api/enrollOwnerKey",
        &main,
        json!({ "key": owner_key.key }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = post(
        &state,
        "/api/enrollOwnerKey",
        &main,
        json!({ "key": phone.key }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Owner session from a signed challenge.
    let (_, challenge) = post(&state, "/api/ownerChallenge", &main, json!({})).await;
    let challenge = challenge["result"]["challenge"]
        .as_str()
        .unwrap()
        .to_string();
    let signature = owner_key
        .sign(&owner_session_signing_payload("S1", &challenge, &owner_key.key.key_id).unwrap());
    let (status, opened) = post(
        &state,
        "/api/openOwnerSession",
        &main,
        json!({ "keyId": owner_key.key.key_id, "challenge": challenge, "signature": signature }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    let owner = opened["result"]["token"].as_str().unwrap().to_string();

    // The token file can't add an approver key; the owner session adds the first one directly.
    let (status, _) = post(
        &state,
        "/api/enrollApproverKey",
        &main,
        json!({ "key": mac.key }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = post(
        &state,
        "/api/enrollApproverKey",
        &owner,
        json!({ "key": mac.key }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A second approver key needs the first key's signed approval.
    let (status, pending) = post(
        &state,
        "/api/enrollApproverKey",
        &owner,
        json!({ "key": phone.key }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(pending["error"], "approvalRequired", "{pending}");
    assert_eq!(pending["approval"]["action"], "addApproverKey");
    let request: crate::security::ApprovalRequest =
        serde_json::from_value(pending["approval"].clone()).unwrap();
    let signature = mac.sign(&approval_signing_payload("S1", &request).unwrap());
    let (status, _) = post(
        &state,
        "/api/approveApprovalRequest",
        &owner,
        json!({ "id": request.id, "keyId": mac.key.key_id, "signature": signature }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(
        &state,
        "/api/enrollApproverKey",
        &owner,
        json!({ "key": phone.key, "approvalId": request.id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(state.security.approver_keys.list().len(), 2);
}

/// The gates in server/security_gates.rs, through the real dispatcher.
#[tokio::test]
async fn sensitive_routes_ask_for_approval_and_closing_doors_never_does() {
    let (_temp, state) = temp_state();
    let main = state.auth_token.clone();
    let asks = |body: &Value| body["error"] == "approvalRequired";

    let (status, body) = post(
        &state,
        "/api/updateTailcatState",
        &main,
        json!({ "kind": "setEnabled", "enabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(asks(&body), "{body}");
    assert_eq!(body["approval"]["action"], "easyConnectEnable");
    let (_, body) = post(
        &state,
        "/api/updateTailcatState",
        &main,
        json!({ "kind": "setEnabled", "enabled": false }),
    )
    .await;
    assert!(!asks(&body), "{body}");

    let (_, body) = post(
        &state,
        "/api/issueComputerGrant",
        &main,
        json!({ "serverId": "S4n", "name": "New", "agentAccess": true }),
    )
    .await;
    assert!(asks(&body), "{body}");
    assert_eq!(body["approval"]["action"], "computerGrant");
    let (_, body) = post(
        &state,
        "/api/issueComputerGrant",
        &main,
        json!({ "serverId": "S4n", "name": "New", "agentAccess": false }),
    )
    .await;
    assert!(!asks(&body), "{body}");

    let (_, body) = post(
        &state,
        "/api/autopilot",
        &main,
        json!({ "op": "set", "patch": { "answerApprovals": true } }),
    )
    .await;
    assert_eq!(
        body["approval"]["action"], "autopilotAnswerApprovals",
        "{body}"
    );

    let (_, body) = post(
        &state,
        "/api/gbots",
        &main,
        json!({ "action": "create", "name": "Helper" }),
    )
    .await;
    assert_eq!(body["approval"]["action"], "gbotsAddBot", "{body}");

    let (_, body) = post(
        &state,
        "/api/dispatchRendererCommand",
        &main,
        json!({ "action": "updateSettingsPatch", "payload": { "patch": { "syncCoordinator": true } } }),
    )
    .await;
    assert_eq!(
        body["approval"]["action"], "settingsWeakenSecurity",
        "{body}"
    );

    // A coordinator answering its own thread's prompt asks too (user decision 2).
    let coordinator =
        state
            .security
            .session_tokens
            .mint_session("P1", "G1", SessionRole::Coordinator);
    let (_, body) = post(
        &state,
        "/api/answerSessionChatPrompt",
        &coordinator,
        json!({ "projectId": "P1", "sessionId": "G2", "kind": "terminalChoice", "choiceIndex": 0 }),
    )
    .await;
    assert_eq!(
        body["approval"]["action"], "answerOtherSessionPrompt",
        "{body}"
    );

    // A session links threads only to itself.
    let (status, body) = post(
        &state,
        "/api/linkCoordinatorThread",
        &coordinator,
        json!({ "coordinatorProjectId": "P1", "coordinatorSessionId": "G9", "projectId": "P1", "sessionId": "G2" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "forbidden");
}

/// Raw terminal input into another session asks (user decision of #495's follow-up); a
/// session's own terminal doesn't.
#[tokio::test]
async fn typing_into_another_sessions_terminal_asks_and_ones_own_does_not() {
    let (_temp, state) = temp_state();
    let coordinator =
        state
            .security
            .session_tokens
            .mint_session("P1", "G1", SessionRole::Coordinator);
    let asks = |body: &Value| body["error"] == "approvalRequired";

    let (_, body) = post(
        &state,
        "/api/sendSessionText",
        &coordinator,
        json!({ "projectId": "P1", "sessionId": "G2", "text": "1" }),
    )
    .await;
    assert!(asks(&body), "{body}");
    assert_eq!(body["approval"]["action"], "typeIntoOtherSession");
    // Escape to a session that is not one of its threads asks too.
    let (_, body) = post(
        &state,
        "/api/sendSessionText",
        &coordinator,
        json!({ "projectId": "P1", "sessionId": "G2", "text": "\u{1b}" }),
    )
    .await;
    assert!(asks(&body), "{body}");
    let (_, body) = post(
        &state,
        "/api/sendSessionEnter",
        &state.auth_token.clone(),
        json!({ "projectId": "P1", "sessionId": "G2" }),
    )
    .await;
    assert!(asks(&body), "{body}");
    let (_, body) = post(
        &state,
        "/api/sendSessionText",
        &coordinator,
        json!({ "projectId": "P1", "sessionId": "G1", "text": "ls" }),
    )
    .await;
    assert!(!asks(&body), "{body}");
    let owner = state.security.session_tokens.mint_owner("mac");
    let (_, body) = post(
        &state,
        "/api/sendSessionText",
        &owner,
        json!({ "projectId": "P1", "sessionId": "G2", "text": "1" }),
    )
    .await;
    assert!(!asks(&body), "{body}");
}
