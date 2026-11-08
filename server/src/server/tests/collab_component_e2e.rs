//! Collab end to end (GitHub #495), opt-in: a gxserver served on a loopback port installs the
//! real `ghostex-collab` component with the owner's signed approval, starts it, shares a chat
//! (approved again), and a guest running the guest page's own tunnel code in Node joins through a
//! real local share relay, reads the chat, has its message held, and is refused another chat, a
//! file and the owner's calls. Uninstalling then ends it all.
//!
//! Run it with the component and the relay built:
//! ```text
//! (cd server/collab && cargo build) && (cd server/share-relay && cargo build)
//! GHOSTEX_COLLAB_E2E_BINARY=<target>/debug/ghostex-collab \
//! GHOSTEX_COLLAB_E2E_RELAY=<target>/debug/ghostex-share-relay \
//!   cargo test --lib collab_component_e2e -- --ignored
//! ```

use super::*;
use crate::collab_shim::{self, registry, sidecar};
use crate::security::{test_keys::TestApprover, ApprovalRequest};
use ghostex_gx_protocol::approval::approval_signing_payload;

async fn post_to(
    state: &Arc<AppState>,
    path: &str,
    token: &str,
    params: Value,
) -> (StatusCode, Value) {
    let routed = route_http(
        state.clone(),
        rpc_request(path, token, json!({ "params": params })),
        "e2e".to_string(),
    )
    .await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

async fn approve(state: &Arc<AppState>, owner: &str, approver: &TestApprover, approval: &Value) {
    let request: ApprovalRequest = serde_json::from_value(approval.clone()).expect("request");
    let signature = approver
        .sign(&approval_signing_payload(&state.security.server_id, &request).expect("payload"));
    let (status, body) = post_to(
        state,
        "/api/approveApprovalRequest",
        owner,
        json!({"id": request.id, "keyId": approver.key.key_id, "signature": signature}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn wait_for(what: &str, check: impl Fn() -> bool) {
    for _ in 0..300 {
        if check() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the built ghostex-collab component, the share relay and node"]
async fn collab_component_e2e() {
    let binary = std::env::var("GHOSTEX_COLLAB_E2E_BINARY").expect("GHOSTEX_COLLAB_E2E_BINARY");
    let relay_binary = std::env::var("GHOSTEX_COLLAB_E2E_RELAY").expect("GHOSTEX_COLLAB_E2E_RELAY");
    let _lock = collab_shim::test_lock();

    // gxserver on its own loopback port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("config dir");
    let state = test_app_state_on(paths, "S5e", Some(port));
    let served = state.clone();
    let app = axum::Router::new().fallback_service(tower::service_fn(move |request| {
        handle_http_request(served.clone(), request)
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let approver = TestApprover::new("mac");
    state
        .security
        .approver_keys
        .insert(approver.key.clone())
        .expect("key");
    let owner = state.security.session_tokens.mint_owner("mac");

    // A chat to share.
    let (project_id, session_id) = {
        let db = open_gxserver_database(&state.paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository
            .create_project(
                json!({ "name": "Shop", "path": temp.path() })
                    .as_object()
                    .unwrap(),
            )
            .expect("project");
        let project_id = project["projectId"].as_str().unwrap().to_string();
        let session = repository
            .create_session(
                json!({
                    "projectId": project_id, "kind": "agent", "agentId": "codex",
                    "title": "Fix login", "lifecycleState": "running",
                    "providerState": { "lifecycleState": "exists" },
                })
                .as_object()
                .unwrap(),
                false,
            )
            .expect("session");
        (
            project_id,
            session["sessionId"].as_str().unwrap().to_string(),
        )
    };

    // A local share relay; the component checks no guest page in a debug build with this set.
    let relay_port = free_port();
    let relay_url = format!("http://127.0.0.1:{relay_port}");
    let mut relay = std::process::Command::new(&relay_binary)
        .args(["--listen", &format!("127.0.0.1:{relay_port}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start the relay");
    std::env::set_var(
        "GHOSTEX_COMPONENT_STORE_DIR",
        temp.path().join("components"),
    );
    std::env::set_var("GHOSTEX_COLLAB_DEV_BINARY", &binary);
    std::env::set_var("GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE", "1");
    std::env::set_var("GHOSTEX_COLLAB_RELAY_URL", &relay_url);
    std::env::remove_var("GHOSTEX_ON_DEMAND_MANIFEST");

    crate::collab_shim::http::approvals::start(state.clone());
    sidecar::start_supervisor(state.clone());

    // Not installed: nothing answers.
    let (_, body) = post_to(&state, "/api/collab", &owner, json!({"op": "read"})).await;
    assert_eq!(body["result"]["installed"], false, "{body}");

    // Install: the owner's approval first.
    let (status, body) = post_to(
        &state,
        "/api/extensionComponents",
        &owner,
        json!({"op": "install", "name": "collab"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["approval"]["action"], "installExtensionComponent");
    approve(&state, &owner, &approver, &body["approval"]).await;
    wait_for("the component to run", sidecar::is_running).await;
    let (_, body) = post_to(&state, "/api/collab", &owner, json!({"op": "read"})).await;
    assert_eq!(body["result"]["installed"], true, "{body}");
    assert_eq!(body["result"]["config"]["enabled"], true);

    // Share: approved again; gxserver finishes it and the answer carries the link and code.
    let share = json!({"op": "share", "projectId": project_id, "sessionId": session_id});
    let (status, body) = post_to(&state, "/api/collab", &owner, share.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["approval"]["action"], "collabShareChat");
    assert!(body["approval"]["summary"]
        .as_str()
        .unwrap()
        .contains("Fix login"));
    let approval_id = body["approval"]["id"].as_str().unwrap().to_string();
    approve(&state, &owner, &approver, &body["approval"]).await;
    let mut repeated = share.clone();
    repeated["approvalId"] = json!(approval_id);
    let mut shared = Value::Null;
    for _ in 0..100 {
        let (status, body) = post_to(&state, "/api/collab", &owner, repeated.clone()).await;
        if status == StatusCode::OK {
            shared = body["result"]["share"].clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let link = shared["link"].as_str().expect("link").to_string();
    let code = shared["code"].as_str().expect("code").to_string();
    assert!(link.starts_with(&relay_url), "{link}");
    wait_for("gxserver to learn the share", || {
        registry::for_chat(&project_id, &session_id).is_some()
    })
    .await;
    wait_for("the relay link", || {
        let mut frame = serde_json::Map::new();
        collab_shim::insert_chat_collab(&mut frame, &project_id, &session_id);
        frame
            .get("collab")
            .is_some_and(|collab| collab["shared"] == true)
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // The guest, with the guest page's own tunnel code.
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("collab/e2e/guest.mjs");
    let output = tokio::process::Command::new("node")
        .arg(&script)
        .args([&relay_url, &link, &code, &project_id, &session_id])
        .output()
        .await
        .expect("run the guest");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let guest: Value =
        serde_json::from_str(stdout.lines().last().unwrap_or("{}")).expect("guest json");
    eprintln!("guest: {guest}");
    assert_eq!(guest["intro"]["title"], "Fix login", "{guest}");
    assert_eq!(guest["wrong"]["t"], "denied", "{guest}");
    assert_eq!(guest["welcome"]["t"], "welcome", "{guest}");
    assert_eq!(guest["welcome"]["sessionId"], session_id.as_str());
    assert_eq!(guest["readStatus"], 200, "{guest}");
    assert_eq!(guest["otherStatus"], 403, "{guest}");
    assert_eq!(guest["ownerStatus"], 403, "{guest}");
    assert_eq!(guest["secretStatus"], 403, "{guest}");
    assert_eq!(guest["sent"]["result"]["collab"]["held"], true, "{guest}");
    wait_for("the held message on the chat", || {
        let mut frame = serde_json::Map::new();
        collab_shim::insert_chat_collab(&mut frame, &project_id, &session_id);
        frame.get("collab").is_some_and(|collab| {
            collab["held"]
                .as_array()
                .is_some_and(|held| held.len() == 1)
        })
    })
    .await;

    // Uninstall: never asks, and everything ends.
    let (status, body) = post_to(
        &state,
        "/api/extensionComponents",
        &owner,
        json!({"op": "uninstall", "name": "collab"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!sidecar::is_running());
    assert!(registry::for_chat(&project_id, &session_id).is_none());
    assert!(!temp.path().join("components/collab").exists());
    assert!(!sidecar::data_dir(&state).exists());
    let (_, body) = post_to(&state, "/api/collab", &owner, json!({"op": "read"})).await;
    assert_eq!(body["result"]["installed"], false, "{body}");

    let _ = relay.kill();
    let _ = relay.wait();
    for key in [
        "GHOSTEX_COMPONENT_STORE_DIR",
        "GHOSTEX_COLLAB_DEV_BINARY",
        "GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE",
        "GHOSTEX_COLLAB_RELAY_URL",
    ] {
        std::env::remove_var(key);
    }
}
