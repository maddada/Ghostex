//! gxserver's Collab shim (GitHub #495): without the component nothing of Collab answers; with
//! it, the owner's calls go to the component and the gate stays in gxserver (gxserver repeats an
//! approved call with the grant itself), only the component may call `/api/collabHost`, a share
//! token reads only its one chat, and installing needs the owner's signed approval while
//! uninstalling never asks.
//!
//! A small fake component (an axum server on a loopback port) stands in for `ghostex-collab`.

use super::*;
use crate::collab_shim::{self, registry, sidecar};
use crate::security::{test_keys::TestApprover, ApprovalRequest, SessionRole};
use ghostex_gx_protocol::approval::approval_signing_payload;
use ghostex_gx_protocol::collab_component::{OwnerCall, OwnerReply, Performed, PresenceNeed};

const HOST_TOKEN: &str = "host-token-0123456789-0123456789-0123456789";
const SIDECAR_TOKEN: &str = "sidecar-token-0123456789-0123456789-01234567";

struct Setup {
    _temp: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
    state: Arc<AppState>,
    approver: TestApprover,
    owner: String,
    agent: String,
}

fn setup() -> Setup {
    let lock = collab_shim::test_lock();
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
    crate::collab_shim::http::approvals::start(state.clone());
    sidecar::test_set_running(None, "", "");
    registry::clear();
    collab_shim::clear_chat_extras();
    crate::collab_shim::install::test_reset_phase();
    Setup {
        _temp: temp,
        _lock: lock,
        state,
        approver,
        owner,
        agent,
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        sidecar::test_set_running(None, "", "");
        registry::clear();
        collab_shim::clear_chat_extras();
    }
}

/// A component that asks for approval to share and shares once it has a grant for exactly that.
async fn start_fake_component() -> u16 {
    use axum::routing::post;
    async fn owner(
        headers: axum::http::HeaderMap,
        axum::Json(call): axum::Json<OwnerCall>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        if headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            != Some(&format!("Bearer {SIDECAR_TOKEN}"))
        {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        let op = call
            .params
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or("read");
        let reply = match op {
            "share" => match &call.grant {
                Some(grant)
                    if grant.action
                        == ghostex_gx_protocol::approval::SensitiveAction::CollabShareChat
                        && grant.target == "s" =>
                {
                    let mut reply = OwnerReply::ok(
                        json!({"share": {"link": "https://x/s/1/#k", "code": "12345678"}}),
                    );
                    reply.performed.push(Performed {
                        approval_id: grant.approval_id.clone(),
                        ok: true,
                    });
                    reply
                }
                _ => OwnerReply::needs_presence(PresenceNeed {
                    action: ghostex_gx_protocol::approval::SensitiveAction::CollabShareChat,
                    target: "s".into(),
                    summary: "Share \u{201c}Fix login\u{201d} through a link".into(),
                    detail: "Anyone with the link and its code can follow this chat.".into(),
                }),
            },
            _ => OwnerReply::ok(json!({"installed": true, "config": {"enabled": true}})),
        };
        axum::Json(reply).into_response()
    }
    let app = axum::Router::new().route("/owner", post(owner));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    port
}

async fn post_to(
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

async fn approve(setup: &Setup, approval: &Value) {
    let request: ApprovalRequest = serde_json::from_value(approval.clone()).expect("request");
    let signature = setup.approver.sign(
        &approval_signing_payload(&setup.state.security.server_id, &request).expect("payload"),
    );
    let (status, body) = post_to(
        &setup.state,
        "/api/approveApprovalRequest",
        &setup.owner,
        json!({"id": request.id, "keyId": setup.approver.key.key_id, "signature": signature}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_component_collab_answers_nothing() {
    let setup = setup();
    let state = &setup.state;
    let (status, body) = post_to(
        state,
        "/api/collab",
        &state.auth_token,
        json!({"op": "read"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["installed"], false);
    assert_eq!(body["result"]["running"], false);
    let (status, body) = post_to(
        state,
        "/api/collab",
        &setup.owner,
        json!({"op": "share", "projectId": "p", "sessionId": "s"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "collabNotInstalled");
    // No share token opens anything.
    let token = format!("gxshare_{}", "x".repeat(43));
    let (status, _) = post_to(
        state,
        "/api/readSessionChat",
        &token,
        json!({"projectId": "p", "sessionId": "s"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Nobody may call the component's endpoint, the main token included.
    for token in [state.auth_token.as_str(), HOST_TOKEN] {
        let (status, _) = post_to(
            state,
            "/api/collabHost",
            token,
            json!({"op": "setShares", "shares": []}),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sharing_waits_for_the_owners_approval_and_gxserver_finishes_it() {
    let setup = setup();
    let state = &setup.state;
    let port = start_fake_component().await;
    sidecar::test_set_running(Some(port), HOST_TOKEN, SIDECAR_TOKEN);

    let share = json!({"op": "share", "projectId": "p", "sessionId": "s"});
    // An agent only asks; the card's words are the component's, not the caller's.
    let (status, body) = post_to(state, "/api/collab", &setup.agent, share.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "approvalRequired");
    assert_eq!(body["approval"]["action"], "collabShareChat");
    assert!(body["approval"]["summary"]
        .as_str()
        .unwrap()
        .contains("Fix login"));

    // The owner approves; gxserver repeats the call with the grant itself.
    approve(&setup, &body["approval"]).await;
    let id = body["approval"]["id"].as_str().unwrap().to_string();
    let mut answered = None;
    for _ in 0..100 {
        if let Some(answer) = crate::collab_shim::http::approvals::done_answer(&id) {
            answered = Some(answer);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let answer = answered
        .expect("gxserver finished the approved share")
        .expect("ok");
    assert_eq!(answer["share"]["code"], "12345678");

    // The agent repeating its call gets that same answer, not a second share or request.
    let mut repeated = share.clone();
    repeated["approvalId"] = json!(id);
    let (status, body) = post_to(state, "/api/collab", &setup.agent, repeated).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["share"]["code"], "12345678");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_component_reports_shares_and_a_share_token_reads_only_its_chat() {
    let setup = setup();
    let state = &setup.state;
    sidecar::test_set_running(Some(1), HOST_TOKEN, SIDECAR_TOKEN);
    let token = format!("gxshare_{}", "y".repeat(43));
    let scope = json!({"shareId": "share_1", "projectId": "p", "sessionId": "s", "token": token, "viewerIds": ["guest_1"]});
    let (status, body) = post_to(
        state,
        "/api/collabHost",
        &state.auth_token,
        json!({"op": "setShares", "shares": [scope]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(registry::authorize_token(&token).is_none());
    let (status, body) = post_to(
        state,
        "/api/collabHost",
        HOST_TOKEN,
        json!({"op": "setShares", "shares": [scope]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(registry::authorize_token(&token).is_some());

    // The component may not describe or write into a chat it doesn't share.
    let (status, _) = post_to(
        state,
        "/api/collabHost",
        HOST_TOKEN,
        json!({"op": "deliver", "projectId": "p", "sessionId": "other", "text": "hi"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = post_to(state, "/api/collabHost", HOST_TOKEN, json!({"op": "setChatExtras", "projectId": "p", "sessionId": "other", "extras": {"shared": true}})).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A guest's token: no other chat, no write, no other endpoint, and only a current guest.
    let read = |session: &str, viewer: Option<&str>| {
        let mut request = rpc_request(
            "/api/readSessionChat",
            &token,
            json!({"params": {"projectId": "p", "sessionId": session}}),
        );
        if let Some(viewer) = viewer {
            request.headers_mut().insert(
                ghostex_gx_protocol::collab_component::COLLAB_VIEWER_HEADER,
                viewer.parse().unwrap(),
            );
        }
        request
    };
    for (request, why) in [
        (read("other", Some("guest_1")), "another chat"),
        (read("s", None), "no guest named"),
        (read("s", Some("guest_9")), "a removed guest"),
        (
            rpc_request(
                "/api/sendSessionChatMessage",
                &token,
                json!({"params": {"projectId": "p", "sessionId": "s", "text": "x"}}),
            ),
            "a write",
        ),
        (
            rpc_request("/api/collab", &token, json!({"params": {"op": "read"}})),
            "Collab's owner calls",
        ),
    ] {
        let routed = route_http(state.clone(), request, "test".to_string()).await;
        assert_eq!(routed.response.status(), StatusCode::FORBIDDEN, "{why}");
    }
    // The chat's frame carries the component's Collab part only while it runs.
    let (status, _) = post_to(state, "/api/collabHost", HOST_TOKEN, json!({"op": "setChatExtras", "projectId": "p", "sessionId": "s", "extras": {"shared": true, "viewers": []}})).await;
    assert_eq!(status, StatusCode::OK);
    let mut frame = serde_json::Map::new();
    collab_shim::insert_chat_collab(&mut frame, "p", "s");
    assert_eq!(frame["collab"]["shared"], true);
    assert_eq!(frame["collab"]["available"], true);
    sidecar::stop_now();
    assert!(
        registry::authorize_token(&token).is_none(),
        "tokens die with the component"
    );
    let mut frame = serde_json::Map::new();
    collab_shim::insert_chat_collab(&mut frame, "p", "s");
    assert_eq!(frame["collab"]["available"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn installing_needs_the_owners_approval_and_uninstalling_never_asks() {
    let setup = setup();
    let state = &setup.state;
    let fake = setup._temp.path().join("ghostex-collab");
    std::fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
    let store = setup._temp.path().join("components");
    std::env::set_var("GHOSTEX_COLLAB_DEV_BINARY", &fake);
    std::env::set_var("GHOSTEX_COMPONENT_STORE_DIR", &store);

    let install = json!({"op": "install", "name": "collab"});
    for token in [
        state.auth_token.clone(),
        setup.agent.clone(),
        setup.owner.clone(),
    ] {
        let (status, body) =
            post_to(state, "/api/extensionComponents", &token, install.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["approval"]["action"], "installExtensionComponent");
        assert_eq!(body["approval"]["target"], "collab");
    }
    assert!(
        crate::collab_shim::install::installed().is_none(),
        "nothing installed before the approval"
    );
    let (_, body) = post_to(
        state,
        "/api/extensionComponents",
        &setup.agent,
        install.clone(),
    )
    .await;
    approve(&setup, &body["approval"]).await;
    let mut installed = false;
    for _ in 0..200 {
        if crate::collab_shim::install::installed().is_some() {
            installed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(installed, "gxserver installed it once the owner approved");

    // Anyone may uninstall, and it asks nothing; the files go.
    let (status, body) = post_to(
        state,
        "/api/extensionComponents",
        &setup.agent,
        json!({"op": "uninstall", "name": "collab"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["component"]["installed"], false);
    assert!(!store.join("collab").exists());
    std::env::remove_var("GHOSTEX_COLLAB_DEV_BINARY");
    std::env::remove_var("GHOSTEX_COMPONENT_STORE_DIR");
}

/// Serves `files` (name, bytes) at `/<tag>/<name>`.
async fn serve_files(files: Vec<(String, Vec<u8>)>) -> String {
    let files = std::sync::Arc::new(files);
    let app = axum::Router::new().fallback(move |uri: axum::http::Uri| {
        let files = files.clone();
        async move {
            let name = uri
                .path()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            match files.iter().find(|(file, _)| *file == name) {
                Some((_, bytes)) => (StatusCode::OK, bytes.clone()),
                None => (StatusCode::NOT_FOUND, Vec::new()),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://127.0.0.1:{port}")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sealed_install_refuses_an_archive_without_ghostexs_signature() {
    use base64::Engine as _;
    use ring::signature::KeyPair as _;
    use sha2::Digest as _;

    let setup = setup();
    let temp = &setup._temp;
    // A Collab archive (a stand-in binary), as the release publishes it.
    let stage = temp.path().join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("ghostex-collab"), "#!/bin/sh\nexit 0\n").unwrap();
    let archive = temp.path().join("archive.tar.gz");
    assert!(std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .arg(".")
        .status()
        .unwrap()
        .success());
    let bytes = std::fs::read(&archive).unwrap();
    let sha256 = hex::encode(sha2::Sha256::digest(&bytes));
    let platform = crate::components::platform_dir();
    let version = "1.0.0-test";
    let asset = format!("collab-{version}-{platform}.tar.gz");

    // A test signing key stands in for Ghostex's (debug builds read GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY).
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    std::env::set_var(
        "GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY",
        hex::encode(pair.public_key().as_ref()),
    );
    let statement = ghostex_gx_protocol::component_signing::component_signing_statement(
        "collab",
        version,
        &platform,
        &asset,
        &sha256,
        bytes.len() as u64,
    );
    let good =
        base64::engine::general_purpose::STANDARD.encode(pair.sign(statement.as_bytes()).as_ref());
    let other_statement = statement.replace(version, "1.0.0-other");
    let replayed = base64::engine::general_purpose::STANDARD
        .encode(pair.sign(other_statement.as_bytes()).as_ref());

    let base = serve_files(vec![(asset.clone(), bytes.clone())]).await;
    let store = temp.path().join("components");
    std::env::set_var("GHOSTEX_ON_DEMAND_BASE_URL", &base);
    std::env::set_var("GHOSTEX_COMPONENT_STORE_DIR", &store);
    std::env::remove_var("GHOSTEX_COLLAB_DEV_BINARY");
    let manifest_path = temp.path().join("on-demand-resources.json");
    let install = |signature: &str| {
        std::fs::write(
            &manifest_path,
            json!({
                "schemaVersion": 2, "version": "11.0.1", "githubRepo": "maddada/Ghostex", "assets": {},
                "components": {"collab": {
                    "name": "collab", "componentVersion": version, "downloadTag": format!("collab-{version}"),
                    "githubRepo": "maddada/ghostex-components",
                    "platforms": {platform.clone(): {"assetName": asset, "sha256": sha256, "sizeBytes": bytes.len(), "signature": signature}}
                }}
            })
            .to_string(),
        )
        .unwrap();
        std::env::set_var("GHOSTEX_ON_DEMAND_MANIFEST", &manifest_path);
        let (tx, rx) = std::sync::mpsc::channel();
        crate::collab_shim::install::start_install(
            setup.state.clone(),
            Box::new(move |ok| {
                let _ = tx.send(ok);
            }),
        )
        .expect("starts");
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .expect("finishes")
    };

    // A signature for another version of the archive (a replay), or none of Ghostex's: refused,
    // and nothing is installed.
    assert!(!install(&replayed));
    let item = crate::collab_shim::install::item();
    assert!(
        item["error"]
            .as_str()
            .unwrap_or_default()
            .contains("signature"),
        "{item}"
    );
    assert!(crate::collab_shim::install::installed().is_none());
    // Ghostex's signature over this exact archive: installed.
    assert!(install(&good));
    assert!(crate::collab_shim::install::installed().is_some());

    sidecar::stop_now();
    for key in [
        "GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY",
        "GHOSTEX_ON_DEMAND_BASE_URL",
        "GHOSTEX_ON_DEMAND_MANIFEST",
        "GHOSTEX_COMPONENT_STORE_DIR",
    ] {
        std::env::remove_var(key);
    }
}
