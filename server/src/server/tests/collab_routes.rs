//! Ghostex Collab's owner calls through the real gate (GitHub #495): turning Collab on, Send mode
//! and widening a share need the owner's signed approval, gxserver finishes an approved change
//! itself, a repeated call gets that answer back, and no read hands out a link or a code.

use super::*;
use crate::collab::{self, shares};
use crate::security::{test_keys::TestApprover, ApprovalRequest, SessionRole};
use ghostex_gx_protocol::approval::approval_signing_payload;

/// Collab keeps one config and one share list per process, so its route tests run one at a time.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Setup {
    _temp: tempfile::TempDir,
    state: Arc<AppState>,
    approver: TestApprover,
    owner: String,
    agent: String,
}

fn setup() -> Setup {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let state = test_app_state(paths);
    let approver = TestApprover::new("mac");
    state
        .security
        .approver_keys
        .insert(approver.key.clone())
        .expect("key");
    let owner = state.security.session_tokens.mint_owner("mac");
    let agent = state
        .security
        .session_tokens
        .mint_session("p", "s", SessionRole::Agent);
    crate::server::collab_http::approvals::start(state.clone());
    // Off, with no shares, whatever an earlier test left.
    let _ = collab::update_config(&state.paths, |config| config.enabled = false);
    shares::stop_all();
    Setup {
        _temp: temp,
        state,
        approver,
        owner,
        agent,
    }
}

async fn post(state: &Arc<AppState>, token: &str, params: Value) -> (StatusCode, Value) {
    let routed = route_http(
        state.clone(),
        rpc_request("/api/collab", token, json!({ "params": params })),
        "test".to_string(),
    )
    .await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

async fn approve(setup: &Setup, approval: &Value) {
    let request: ApprovalRequest = serde_json::from_value(approval.clone()).expect("request");
    let signature = setup.approver.sign(
        &approval_signing_payload(&setup.state.security.server_id, &request).expect("payload"),
    );
    let routed = route_http(
        setup.state.clone(),
        rpc_request(
            "/api/approveApprovalRequest",
            &setup.owner,
            json!({ "params": {
                "id": request.id,
                "keyId": setup.approver.key.key_id,
                "signature": signature,
            }}),
        ),
        "test".to_string(),
    )
    .await;
    let status = routed.response.status();
    let body = response_json(routed.response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn eventually(check: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn turning_collab_on_waits_for_the_owners_signed_approval() {
    let _serial = serial();
    let setup = setup();
    let state = &setup.state;

    // The main token (the CLI, any agent today) and a session's own token only ask.
    for token in [state.auth_token.clone(), setup.agent.clone()] {
        let (status, body) = post(
            state,
            &token,
            json!({"op": "set", "patch": {"enabled": true}}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["error"], "approvalRequired");
        assert_eq!(body["approval"]["action"], "collabEnable");
    }
    // The owner's own app asks too.
    let (status, body) = post(
        state,
        &setup.owner,
        json!({"op": "set", "patch": {"enabled": true}}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!collab::read_config(&state.paths).enabled);

    let (_, asked) = post(
        state,
        &setup.agent,
        json!({"op": "set", "patch": {"enabled": true}}),
    )
    .await;
    let approval = asked["approval"].clone();
    let id = approval["id"].as_str().expect("id").to_string();
    approve(&setup, &approval).await;
    // gxserver turns it on itself once the signature arrives.
    assert!(eventually(|| collab::read_config(&state.paths).enabled).await);

    // The agent repeating its call gets the answer, not a second request.
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "set", "patch": {"enabled": true}, "approvalId": id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["config"]["enabled"], true);
    assert!(state
        .security
        .approvals
        .list_pending(crate::security::now_ms())
        .iter()
        .all(|request| request.requested_by.session_id.as_deref() != Some("s")));
    // Another caller can't collect it.
    let (status, body) = post(
        state,
        &state.auth_token,
        json!({"op": "set", "patch": {"enabled": true}, "approvalId": id}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "already on, nothing to approve: {body}"
    );

    // Turning it off never needs an approval, and ends every share.
    shares::share_session(&collab::read_config(&state.paths), "p", "s");
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "set", "patch": {"enabled": false}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(shares::list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_never_weakens_collabs_rules() {
    let _serial = serial();
    let setup = setup();
    for patch in [
        json!({"relayUrl": "https://evil.example"}),
        json!({"requireCode": false}),
        json!({"expiry": "never"}),
        json!({"mode": "send"}),
    ] {
        let (status, body) = post(
            &setup.state,
            &setup.owner,
            json!({"op": "set", "patch": patch}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{patch}: {body}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_never_hand_out_a_link_or_code_and_send_mode_needs_approval() {
    let _serial = serial();
    let setup = setup();
    let state = &setup.state;
    let _ = collab::update_config(&state.paths, |config| config.enabled = true);
    let share = shares::share_session(&collab::read_config(&state.paths), "p", "s");
    assert_eq!(share.mode, collab::GuestMode::Approve);
    assert_eq!(share.code.len(), shares::CODE_DIGITS);

    let (status, read) = post(state, &setup.owner, json!({"op": "read"})).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    let listed = &read["result"]["shares"][0];
    assert_eq!(listed["shareId"], share.id.as_str());
    let text = read.to_string();
    assert!(
        !text.contains(&share.key),
        "the link's key never leaves in a read"
    );
    assert!(!text.contains(&share.code), "nor the code");
    assert!(listed.get("link").is_none() && listed.get("code").is_none());

    // Narrowing is free; Send needs the owner's approval, which gxserver then carries out.
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "setMode", "shareId": share.id, "mode": "view"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "setMode", "shareId": share.id, "mode": "approve"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "view to approve widens: {body}"
    );
    assert_eq!(body["approval"]["action"], "collabChangeShare");
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "setMode", "shareId": share.id, "mode": "send"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["approval"]["action"], "collabEnableSendMode");
    assert_eq!(body["approval"]["target"], "s");
    assert_eq!(
        shares::get(&share.id).expect("share").mode,
        collab::GuestMode::View
    );
    approve(&setup, &body["approval"]).await;
    assert!(
        eventually(
            || shares::get(&share.id).is_some_and(|share| share.mode == collab::GuestMode::Send)
        )
        .await
    );

    // A guest message can't be waved through by an agent either.
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "resolve", "shareId": share.id, "heldId": "held_missing", "action": "send"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let viewer = shares::join(&share.id, "Sam", "dev1").ok().expect("joined");
    let held = shares::hold(&share.id, &viewer, "please run rm -rf", &[]);
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "resolve", "shareId": share.id, "heldId": held.id, "action": "send"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["approval"]["action"], "collabApproveGuestMessage");
    assert!(body["approval"]["detail"]
        .as_str()
        .unwrap_or_default()
        .contains("please run rm -rf"));
    assert!(
        shares::peek_held(&share.id, &held.id).is_some(),
        "still waiting"
    );
    // Decline is always allowed.
    let (status, body) = post(
        state,
        &setup.agent,
        json!({"op": "resolve", "shareId": share.id, "heldId": held.id, "action": "decline"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A chat that is deleted takes its share with it.
    crate::server::collab_http::end_share_of_deleted_chat("p", "s");
    assert!(shares::get(&share.id).is_none());
    let _ = collab::update_config(&state.paths, |config| config.enabled = false);
}
