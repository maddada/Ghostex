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
//! To install the way a release does, add `GHOSTEX_COLLAB_E2E_MANIFEST` (a sealed
//! on-demand-resources.json naming a `tooling/collab/build-component.sh` archive),
//! `GHOSTEX_COLLAB_E2E_ASSETS` (the archive's folder) and `GHOSTEX_COLLAB_E2E_PAGES` (the relay's
//! pinned guest pages folder, since a release build checks it).

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

/// Serves `folder`'s files at `/<tag>/<file>` (what a components release download URL looks like).
async fn serve_assets(folder: std::path::PathBuf) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    let app = axum::Router::new().fallback(move |uri: axum::http::Uri| {
        let folder = folder.clone();
        async move {
            let name = uri
                .path()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            match std::fs::read(folder.join(&name)) {
                Ok(bytes) if !name.contains("..") => (StatusCode::OK, bytes),
                _ => (StatusCode::NOT_FOUND, Vec::new()),
            }
        }
    });
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://127.0.0.1:{port}")
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
    let mut relay_args = vec!["--listen".to_string(), format!("127.0.0.1:{relay_port}")];
    // The pinned guest pages, for a release-built component, which checks the relay serves them.
    if let Ok(pages) = std::env::var("GHOSTEX_COLLAB_E2E_PAGES") {
        relay_args.extend(["--pages".to_string(), pages]);
    }
    let mut relay = std::process::Command::new(&relay_binary)
        .args(&relay_args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start the relay");
    // It checks its pinned pages before it listens.
    for _ in 0..300 {
        if tokio::net::TcpStream::connect(("127.0.0.1", relay_port))
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    std::env::set_var(
        "GHOSTEX_COMPONENT_STORE_DIR",
        temp.path().join("components"),
    );
    // With a sealed manifest and its archive folder, install the way a release does: download the
    // archive from a local "components repository" and check it; else the development binary.
    if let (Ok(manifest), Ok(assets)) = (
        std::env::var("GHOSTEX_COLLAB_E2E_MANIFEST"),
        std::env::var("GHOSTEX_COLLAB_E2E_ASSETS"),
    ) {
        let base = serve_assets(std::path::PathBuf::from(assets)).await;
        std::env::set_var("GHOSTEX_ON_DEMAND_MANIFEST", manifest);
        std::env::set_var("GHOSTEX_ON_DEMAND_BASE_URL", base);
        std::env::remove_var("GHOSTEX_COLLAB_DEV_BINARY");
    } else {
        std::env::set_var("GHOSTEX_COLLAB_DEV_BINARY", &binary);
        std::env::remove_var("GHOSTEX_ON_DEMAND_MANIFEST");
    }
    std::env::set_var("GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE", "1");
    std::env::set_var("GHOSTEX_COLLAB_RELAY_URL", &relay_url);

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
        "GHOSTEX_ON_DEMAND_MANIFEST",
        "GHOSTEX_ON_DEMAND_BASE_URL",
        "GHOSTEX_COMPONENT_STORE_DIR",
        "GHOSTEX_COLLAB_DEV_BINARY",
        "GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE",
        "GHOSTEX_COLLAB_RELAY_URL",
    ] {
        std::env::remove_var(key);
    }
}
