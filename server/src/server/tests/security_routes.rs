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
