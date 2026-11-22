use super::*;
use crate::{
    protocol::{endpoint_for, ApiPermission},
    push::REGISTER_PUSH_TOKEN_ENDPOINT,
    remote_access::{
        remove_paired_device_using, replace_earlier_pairings_of_same_phone,
        AdministratorApprovalDeclined, RemotePairedDeviceRecord, RemotePairingRepository,
        StoredDeviceKey,
    },
    storage::open_gxserver_database,
};
use std::{fs, path::PathBuf};

const TOKEN_A: &str = "ExponentPushToken[phoneAtoken0001]";
const TOKEN_B: &str = "ExponentPushToken[phoneAtoken0002]";
const TOKEN_C: &str = "ExpoPushToken[phone_Q-token-0003]";
const TOKEN_D: &str = "ExpoPushToken[phoneRtoken0004]";
const ALL_TOKENS: [&str; 4] = [TOKEN_A, TOKEN_B, TOKEN_C, TOKEN_D];

fn push_fixture() -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let state = test_app_state(paths);
    (temp, state)
}

fn envelope(params: Value) -> Value {
    json!({ "protocolVersion": 1, "params": params })
}

fn register(token: &str, machine_id: &str, kinds: (bool, bool, bool)) -> Value {
    json!({
        "action": "register",
        "token": token,
        "machineId": machine_id,
        "kinds": { "finished": kinds.0, "waiting": kinds.1, "error": kinds.2 },
    })
}

fn register_paired(token: &str, machine_id: &str, device_id: &str) -> Value {
    let mut params = register(token, machine_id, (true, true, true));
    params["deviceId"] = json!(device_id);
    params
}

fn unregister(machine_id: &str) -> Value {
    json!({ "action": "unregister", "machineId": machine_id })
}

async fn post_push(state: &Arc<AppState>, params: Value) -> (StatusCode, Value) {
    send(
        state,
        rpc_request(
            REGISTER_PUSH_TOKEN_ENDPOINT,
            &state.auth_token,
            envelope(params),
        ),
    )
    .await
}

async fn send(state: &Arc<AppState>, request: Request<Body>) -> (StatusCode, Value) {
    let routed = route_http(state.clone(), request, "push-test".to_string()).await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

fn store_file(state: &AppState) -> PathBuf {
    state.paths.auth_dir.join("push-registrations.json")
}

fn store_bytes(state: &AppState) -> Option<Vec<u8>> {
    fs::read(store_file(state)).ok()
}

fn stored(state: &AppState) -> Vec<Value> {
    let Some(bytes) = store_bytes(state) else {
        return Vec::new();
    };
    let store: Value = serde_json::from_slice(&bytes).expect("store file is JSON");
    store["registrations"]
        .as_array()
        .expect("registrations array")
        .clone()
}

fn stored_entry(state: &AppState, machine_id: &str) -> Option<Value> {
    stored(state)
        .into_iter()
        .find(|entry| entry["machineId"] == json!(machine_id))
}

fn insert_paired_device(state: &AppState, device_id: &str) {
    let db = open_gxserver_database(&state.paths).expect("db");
    RemotePairingRepository::new(&db)
        .insert_device(&RemotePairedDeviceRecord {
            id: device_id.to_string(),
            name: format!("Phone {device_id}"),
            platform: "ios".to_string(),
            ssh_key_fingerprint: format!("SHA256:{device_id}"),
            tailcat_client_key: None,
            api_token_hash: None,
            paired_at: "2026-10-31T10:00:00.000Z".to_string(),
            last_seen_at: None,
            ..Default::default()
        })
        .expect("insert paired device");
}

/// Pairs `device_id` with its own API token, as `/api/pairDevice` stores it, and returns the token.
fn insert_phone_with_api_token(state: &AppState, device_id: &str) -> String {
    let token = format!("{}{}", crate::auth::DEVICE_API_TOKEN_PREFIX, "p".repeat(43));
    let db = open_gxserver_database(&state.paths).expect("db");
    RemotePairingRepository::new(&db)
        .insert_device(&RemotePairedDeviceRecord {
            id: device_id.to_string(),
            name: format!("Phone {device_id}"),
            platform: "ios".to_string(),
            ssh_key_fingerprint: format!("SHA256:{device_id}"),
            tailcat_client_key: None,
            api_token_hash: Some(crate::auth::hash_device_api_token(&token)),
            paired_at: "2026-11-07T10:00:00.000Z".to_string(),
            last_seen_at: None,
            ..Default::default()
        })
        .expect("insert paired device");
    crate::remote_access::reload_device_api_tokens(&db, &state.device_api_tokens)
        .expect("reload device tokens");
    token
}

fn paired_device_exists(state: &AppState, device_id: &str) -> bool {
    let db = open_gxserver_database(&state.paths).expect("db");
    RemotePairingRepository::new(&db)
        .find_device(device_id)
        .expect("find device")
        .is_some()
}

fn assert_no_token_in(text: &str) {
    for token in ALL_TOKENS {
        assert!(!text.contains(token), "a push token leaked into: {text}");
    }
}

#[tokio::test]
async fn push_token_store_route_rejects_invalid_auth_and_fields() {
    let (_temp, state) = push_fixture();

    // The route exists only on the authenticated loopback listener and its tunnels.
    let endpoint = endpoint_for(REGISTER_PUSH_TOKEN_ENDPOINT).expect("descriptor");
    assert!(matches!(endpoint.permission, ApiPermission::FullLocal));
    assert!(endpoint.requires_auth && endpoint.requires_protocol_version);
    // Extension API tokens open only their own allow-listed endpoints, never this one.
    let mut extension_headers = axum::http::HeaderMap::new();
    extension_headers.insert(
        header::AUTHORIZATION,
        "Bearer extension-token".parse().expect("header"),
    );
    assert_eq!(
        state
            .extension_registry
            .authorize_api_token(&extension_headers, REGISTER_PUSH_TOKEN_ENDPOINT),
        None
    );

    // The phone's envelope without the protocol header is accepted.
    let phone_request = Request::builder()
        .method(Method::POST)
        .uri(REGISTER_PUSH_TOKEN_ENDPOINT)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.auth_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            envelope(register(TOKEN_A, "computer-P", (true, true, true))).to_string(),
        ))
        .expect("request");
    let (status, body) = send(&state, phone_request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["result"], json!({}));
    let before = store_bytes(&state).expect("store written");

    let wrong_bearer = rpc_request(
        REGISTER_PUSH_TOKEN_ENDPOINT,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        envelope(register(TOKEN_B, "computer-P", (true, true, true))),
    );
    let (status, body) = send(&state, wrong_bearer).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], json!("unauthorized"));

    let get = Request::builder()
        .method(Method::GET)
        .uri(REGISTER_PUSH_TOKEN_ENDPOINT)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.auth_token),
        )
        .body(Body::empty())
        .expect("request");
    assert_eq!(send(&state, get).await.0, StatusCode::METHOD_NOT_ALLOWED);

    let old_protocol = Request::builder()
        .method(Method::POST)
        .uri(REGISTER_PUSH_TOKEN_ENDPOINT)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.auth_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "protocolVersion": 2,
                "params": register(TOKEN_B, "computer-P", (true, true, true)),
            })
            .to_string(),
        ))
        .expect("request");
    assert_eq!(
        send(&state, old_protocol).await.0,
        StatusCode::UPGRADE_REQUIRED
    );

    let not_json = Request::builder()
        .method(Method::POST)
        .uri(REGISTER_PUSH_TOKEN_ENDPOINT)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.auth_token),
        )
        .header(GXSERVER_PROTOCOL_HEADER, "1")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            "{{\"params\":{{\"token\":\"{TOKEN_B}\""
        )))
        .expect("request");
    let (status, body) = send(&state, not_json).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_no_token_in(&body.to_string());

    let kinds = json!({ "finished": true, "waiting": true, "error": true });
    let long_id = "m".repeat(65);
    let long_device = "d".repeat(129);
    let invalid_bodies = [
        json!({ "protocolVersion": 1 }),
        json!({ "protocolVersion": 1, "params": TOKEN_B }),
        json!({ "protocolVersion": 1, "params": {} }),
        envelope(
            json!({ "action": "replace", "token": TOKEN_B, "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(json!({ "action": "register", "machineId": "computer-P", "kinds": kinds })),
        envelope(
            json!({ "action": "register", "token": 7, "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": "ExponentPushToken[short]", "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": format!("ExponentPushToken[{}]", "x".repeat(65)), "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": format!("{TOKEN_B}\n"), "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": "ApplePushToken[phoneAtoken0002]", "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": "ExponentPushToken[phone token02]", "machineId": "computer-P", "kinds": kinds }),
        ),
        envelope(json!({ "action": "register", "token": TOKEN_B, "kinds": kinds })),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer P", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-\u{e9}", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-\u{7f}", "kinds": kinds }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": long_id, "kinds": kinds }),
        ),
        envelope(json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P" })),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": null }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": { "finished": true, "error": true } }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": { "finished": true, "waiting": "true", "error": true } }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": { "finished": true, "waiting": true, "error": true, "bell": true } }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": kinds, "deviceId": "" }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": kinds, "deviceId": null }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": kinds, "deviceId": long_device }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": kinds, "registeredAt": "2026-10-31T10:00:00.000Z" }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_B, "machineId": "computer-P", "kinds": kinds, "url": "https://example.invalid/push" }),
        ),
        envelope(
            json!({ "action": "register", "token": TOKEN_C, "machineId": "computer-P", "kinds": kinds, TOKEN_B: true }),
        ),
        envelope(json!({ "action": "unregister" })),
        envelope(json!({ "action": "unregister", "machineId": "computer-P", "token": TOKEN_A })),
        envelope(json!({ "action": "unregister", "machineId": "computer-P", "kinds": kinds })),
    ];
    for invalid in invalid_bodies {
        let request = rpc_request(
            REGISTER_PUSH_TOKEN_ENDPOINT,
            &state.auth_token,
            invalid.clone(),
        );
        let (status, body) = send(&state, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "accepted {invalid}");
        assert_eq!(body["ok"], json!(false));
        assert_eq!(body["error"], json!("badRequest"));
        assert_no_token_in(&body.to_string());
        assert!(!body.to_string().contains("example.invalid"));
        assert_eq!(store_bytes(&state).as_deref(), Some(before.as_slice()));
    }
    let entry = stored_entry(&state, "computer-P").expect("first registration kept");
    assert_eq!(entry["token"], json!(TOKEN_A));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_token_store_rotation_and_unregistration_are_phone_scoped() {
    let (temp, state) = push_fixture();

    let (status, body) =
        post_push(&state, register(TOKEN_A, "computer-P", (true, true, true))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"], json!({}));
    let (status, _) = post_push(
        &state,
        register(TOKEN_C, "computer-Q", (false, true, false)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Rotation and a switch change replace only P's entry.
    let (status, body) =
        post_push(&state, register(TOKEN_B, "computer-P", (true, false, true))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_no_token_in(&body.to_string());
    let entries = stored(&state);
    assert_eq!(entries.len(), 2);
    let p = stored_entry(&state, "computer-P").expect("P");
    assert_eq!(p["token"], json!(TOKEN_B));
    assert_eq!(
        p["kinds"],
        json!({ "finished": true, "waiting": false, "error": true })
    );
    let q = stored_entry(&state, "computer-Q").expect("Q");
    assert_eq!(q["token"], json!(TOKEN_C));
    assert_eq!(
        q["kinds"],
        json!({ "finished": false, "waiting": true, "error": false })
    );
    for entry in &entries {
        let mut keys: Vec<&str> = entry
            .as_object()
            .expect("entry")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["kinds", "machineId", "registeredAt", "token"]);
        chrono::DateTime::parse_from_rfc3339(entry["registeredAt"].as_str().expect("time"))
            .expect("host-generated registeredAt");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file_mode = fs::metadata(store_file(&state))
            .expect("store")
            .permissions()
            .mode();
        assert_eq!(file_mode & 0o777, 0o600);
        let dir_mode = fs::metadata(&state.paths.auth_dir)
            .expect("auth dir")
            .permissions()
            .mode();
        assert_eq!(dir_mode & 0o077, 0, "auth folder is open to other users");
    }

    // A refused takeover changes nothing.
    let before = store_bytes(&state).expect("store");
    let (status, body) =
        post_push(&state, register(TOKEN_C, "computer-R", (true, true, true))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_no_token_in(&body.to_string());
    assert_eq!(store_bytes(&state), Some(before));

    // After a restart the accepted registrations are still there; unregistering P leaves Q.
    let reopened = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    assert_eq!(stored(&reopened).len(), 2);
    let (status, body) = post_push(&reopened, unregister("computer-P")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"], json!({}));
    assert!(stored_entry(&reopened, "computer-P").is_none());
    assert_eq!(
        stored_entry(&reopened, "computer-Q").expect("Q kept")["token"],
        json!(TOKEN_C)
    );
    let (status, _) = post_push(&reopened, unregister("computer-P")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored(&reopened).len(), 1);

    // Concurrent registrations of different phones all land.
    let requests = (0..12).map(|index| {
        let state = reopened.clone();
        tokio::spawn(async move {
            post_push(
                &state,
                register(
                    &format!("ExponentPushToken[concurrent{index:04}]"),
                    &format!("computer-{index}"),
                    (true, index % 2 == 0, false),
                ),
            )
            .await
            .0
        })
    });
    for status in futures_util::future::join_all(requests).await {
        assert_eq!(status.expect("task"), StatusCode::OK);
    }
    assert_eq!(stored(&reopened).len(), 13);
    assert!(!state
        .paths
        .auth_dir
        .join("push-registrations.json.tmp")
        .exists());
}

#[tokio::test]
async fn push_token_store_rejects_takeover_of_a_held_id_or_token() {
    let (_temp, state) = push_fixture();
    insert_paired_device(&state, "device-one");
    insert_paired_device(&state, "device-two");

    let (status, body) =
        post_push(&state, register_paired(TOKEN_A, "computer-P", "device-one")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        stored_entry(&state, "computer-P").expect("P")["deviceId"],
        json!("device-one")
    );
    let before = store_bytes(&state).expect("store");

    let refused = [
        // P's token under another computer id.
        register(TOKEN_A, "computer-Q", (true, true, true)),
        // P's paired device under another computer id.
        register_paired(TOKEN_C, "computer-Q", "device-one"),
        // P without the device it was registered with.
        register(TOKEN_B, "computer-P", (true, true, true)),
        // P moved to another paired device.
        register_paired(TOKEN_B, "computer-P", "device-two"),
    ];
    for params in refused {
        let (status, body) = post_push(&state, params.clone()).await;
        assert_eq!(status, StatusCode::CONFLICT, "accepted {params}");
        assert_eq!(body["error"], json!("conflict"));
        assert_no_token_in(&body.to_string());
        assert_eq!(store_bytes(&state).as_deref(), Some(before.as_slice()));
    }

    let (status, body) = post_push(
        &state,
        register_paired(TOKEN_C, "computer-Q", "device-unknown"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.to_string().contains("device-unknown"));
    assert_eq!(store_bytes(&state).as_deref(), Some(before.as_slice()));

    // Rotation keeps the same pairing and is accepted.
    let (status, _) = post_push(&state, register_paired(TOKEN_B, "computer-P", "device-one")).await;
    assert_eq!(status, StatusCode::OK);
    // Once P is unregistered, its old token and device are free.
    let (status, _) = post_push(&state, unregister("computer-P")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) =
        post_push(&state, register_paired(TOKEN_B, "computer-Q", "device-one")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn push_token_store_refuses_a_phone_removed_while_its_request_arrives() {
    let (_temp, state) = push_fixture();
    let phone_token = insert_phone_with_api_token(&state, "device-one");
    let (body_polled, polled) = tokio::sync::oneshot::channel::<()>();
    let (release_body, released) = tokio::sync::oneshot::channel::<()>();
    let bytes = envelope(register(TOKEN_A, "computer-P", (true, true, true))).to_string();
    let delayed_body = Body::from_stream(futures_util::stream::once(async move {
        let _ = body_polled.send(());
        let _ = released.await;
        Ok::<_, std::io::Error>(bytes)
    }));
    let (parts, _) =
        rpc_request(REGISTER_PUSH_TOKEN_ENDPOINT, &phone_token, json!({})).into_parts();
    let request = Request::from_parts(parts, delayed_body);
    let pending = tokio::spawn({
        let state = state.clone();
        async move { send(&state, request).await }
    });

    // The token passed the auth gate and the body is being read when the phone is removed.
    tokio::time::timeout(Duration::from_secs(5), polled)
        .await
        .expect("body polled")
        .expect("body polled");
    let db = open_gxserver_database(&state.paths).expect("db");
    let removed = remove_paired_device_using(
        &state.paths,
        &db,
        &state.tailcat_runtime,
        &state.device_api_tokens,
        "device-one",
        |_: &str| Ok(true),
    )
    .expect("unpair");
    assert!(removed.is_some());
    assert!(!state.device_api_tokens.accepts(&phone_token));
    drop(db);
    release_body.send(()).expect("release body");

    // Refused as no longer paired, not stored as an unlinked registration of the main token.
    let (status, body) = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("answered")
        .expect("request task");
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], json!("forbidden"));
    assert_eq!(store_bytes(&state), None);
}

#[tokio::test]
async fn push_token_store_never_exposes_tokens() {
    let (temp, state) = push_fixture();
    // Every routine request log on, so the scan below covers what a debugging user would ship.
    fs::create_dir_all(&state.paths.app_config_dir).expect("config dir");
    fs::write(
        state
            .paths
            .app_config_dir
            .join("native-sidebar-settings.json"),
        json!({
            "debuggingMode": true,
            "diagnosticLogging": { "scenarios": { "gxserver.requests": true } },
        })
        .to_string(),
    )
    .expect("settings");

    let requests = [
        register(TOKEN_A, "computer-P", (true, true, true)),
        register(TOKEN_B, "computer-P", (false, true, true)),
        register(TOKEN_C, "computer-Q", (true, true, true)),
        register(TOKEN_C, "computer-R", (true, true, true)),
        register(TOKEN_B, "computer-R", (true, true, true)),
        register(
            "ExponentPushToken[phoneDtoken]x",
            "computer-R",
            (true, true, true),
        ),
        json!({ "action": "register", "token": TOKEN_D, "machineId": "computer-R", "kinds": { "finished": true, "waiting": true, "error": TOKEN_D } }),
        json!({ "action": "register", "machineId": "computer-R", TOKEN_D: true }),
        unregister("computer-Q"),
    ];
    let request_count = requests.len();
    for params in requests {
        let request = rpc_request(
            REGISTER_PUSH_TOKEN_ENDPOINT,
            &state.auth_token,
            envelope(params),
        );
        let response = handle_http_request(state.clone(), request)
            .await
            .expect("response");
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_no_token_in(&String::from_utf8_lossy(&bytes));
    }

    // Every request, accepted or refused, left a log line, so the scan below covers them.
    let log = fs::read_to_string(&state.paths.log_file).expect("request log written");
    assert_eq!(
        log.matches("\"event\":\"apiRequest\"").count(),
        request_count,
        "request log is off: {log}"
    );

    // Only the private store file may hold a token, and only the current ones.
    let store = store_file(&state);
    let mut pending = vec![temp.path().to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(&path).expect("read dir") {
                pending.push(entry.expect("entry").path());
            }
            continue;
        }
        let text = String::from_utf8_lossy(&fs::read(&path).expect("read file")).into_owned();
        if path == store {
            assert!(
                !text.contains(TOKEN_A),
                "a replaced token stayed in the store"
            );
            assert!(
                !text.contains(TOKEN_C),
                "an unregistered token stayed in the store"
            );
            assert!(!text.contains(TOKEN_D));
            assert!(text.contains(TOKEN_B));
        } else {
            for token in ALL_TOKENS {
                assert!(!text.contains(token), "token found in {}", path.display());
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn push_token_store_failed_write_preserves_registration() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, state) = push_fixture();
    // No store yet: unregistering succeeds and creates nothing.
    let (status, body) = post_push(&state, unregister("computer-P")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(store_bytes(&state).is_none());

    let (status, _) = post_push(&state, register(TOKEN_A, "computer-P", (true, true, true))).await;
    assert_eq!(status, StatusCode::OK);
    let saved = store_bytes(&state).expect("store");
    let auth_dir = state.paths.auth_dir.clone();

    // A folder that refuses the new file keeps the previous registration.
    fs::set_permissions(&auth_dir, fs::Permissions::from_mode(0o500)).expect("chmod");
    let rotated = post_push(&state, register(TOKEN_B, "computer-P", (true, true, true))).await;
    let removed = post_push(&state, unregister("computer-P")).await;
    fs::set_permissions(&auth_dir, fs::Permissions::from_mode(0o700)).expect("chmod");
    for (status, body) in [rotated, removed] {
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("could not be saved"), "{message}");
        assert_no_token_in(message);
    }
    assert_eq!(store_bytes(&state), Some(saved.clone()));
    assert!(!auth_dir.join("push-registrations.json.tmp").exists());

    // A damaged file is reported and never overwritten.
    let damaged_files: [&[u8]; 3] = [
        b"{\"version\":1,\"registrations\":[",
        br#"{"version":1,"registrations":[{"token":"not-a-token","machineId":"computer-P","kinds":{"finished":true,"waiting":true,"error":true},"registeredAt":"2026-10-31T10:00:00.000Z"}]}"#,
        br#"{"version":9,"registrations":[]}"#,
    ];
    for damaged in damaged_files {
        fs::write(store_file(&state), damaged).expect("damage store");
        for params in [
            register(TOKEN_C, "computer-Q", (true, true, true)),
            unregister("computer-P"),
        ] {
            let (status, body) = post_push(&state, params).await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
            let message = body["message"].as_str().expect("message");
            assert!(message.contains("is damaged"), "{message}");
            assert_eq!(store_bytes(&state).as_deref(), Some(damaged));
        }
    }

    // An unreadable file is reported as unreadable, not as damaged or empty.
    fs::write(store_file(&state), &saved).expect("restore store");
    fs::set_permissions(store_file(&state), fs::Permissions::from_mode(0o000)).expect("chmod");
    let (status, body) =
        post_push(&state, register(TOKEN_C, "computer-Q", (true, true, true))).await;
    fs::set_permissions(store_file(&state), fs::Permissions::from_mode(0o600)).expect("chmod");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("could not be read"), "{message}");
    assert_eq!(store_bytes(&state), Some(saved));
    assert_eq!(
        stored_entry(&state, "computer-P").expect("P kept")["token"],
        json!(TOKEN_A)
    );
}

#[tokio::test]
async fn push_token_store_unpair_revokes_only_linked_phone() {
    let (_temp, state) = push_fixture();
    insert_paired_device(&state, "device-one");
    insert_paired_device(&state, "device-two");
    for params in [
        register_paired(TOKEN_A, "computer-P", "device-one"),
        register_paired(TOKEN_C, "computer-Q", "device-two"),
        register(TOKEN_D, "computer-S", (true, true, true)),
    ] {
        let (status, body) = post_push(&state, params).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let db = open_gxserver_database(&state.paths).expect("db");
    let unpair = |remove_key: &dyn Fn(&str) -> anyhow::Result<bool>| {
        remove_paired_device_using(
            &state.paths,
            &db,
            &state.tailcat_runtime,
            &state.device_api_tokens,
            "device-one",
            remove_key,
        )
    };

    // A declined or failed key removal keeps the phone paired and registered, ready for another Remove.
    let declined = unpair(&|_| {
        Err(AdministratorApprovalDeclined {
            keys_file: PathBuf::from("administrators_authorized_keys"),
        }
        .into())
    })
    .expect_err("declined");
    assert!(declined.to_string().contains("still paired"));
    let failed =
        unpair(&|_| Err(anyhow::anyhow!("authorized_keys is read-only"))).expect_err("failed");
    assert!(failed.to_string().contains("read-only"));
    assert!(stored_entry(&state, "computer-P").is_some());
    assert!(paired_device_exists(&state, "device-one"));

    // A push store that cannot be cleaned keeps the device listed so Remove can be retried.
    let saved = store_bytes(&state).expect("store");
    fs::write(store_file(&state), b"{").expect("damage store");
    let damaged = unpair(&|_| Ok(true)).expect_err("damaged store");
    assert!(damaged.to_string().contains("is damaged"));
    assert_no_token_in(&format!("{damaged:#}"));
    assert!(paired_device_exists(&state, "device-one"));
    fs::write(store_file(&state), &saved).expect("restore store");

    // The key goes first, while the registration still exists; then the registration and the row.
    let removed = unpair(&|device_id| {
        assert_eq!(device_id, "device-one");
        assert!(stored_entry(&state, "computer-P").is_some());
        Ok(true)
    })
    .expect("unpair")
    .expect("device removed");
    assert_eq!(removed.id, "device-one");
    assert!(stored_entry(&state, "computer-P").is_none());
    assert_eq!(
        stored_entry(&state, "computer-Q").expect("other phone kept")["token"],
        json!(TOKEN_C)
    );
    assert!(stored_entry(&state, "computer-S").is_some());
    assert!(!paired_device_exists(&state, "device-one"));
    assert!(paired_device_exists(&state, "device-two"));

    // The removed device can no longer be linked, and an unknown device unpairs nothing.
    let (status, _) = post_push(&state, register_paired(TOKEN_A, "computer-P", "device-one")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let before = store_bytes(&state);
    assert!(unpair(&|_| Ok(true)).expect("unknown device").is_none());
    assert_eq!(store_bytes(&state), before);
}

#[tokio::test]
async fn push_token_store_repairing_a_phone_revokes_the_replaced_pairing() {
    let (_temp, state) = push_fixture();
    let db = open_gxserver_database(&state.paths).expect("db");
    let pair = |device_id: &str, client_key: &str| {
        RemotePairingRepository::new(&db)
            .insert_device(&RemotePairedDeviceRecord {
                id: device_id.to_string(),
                name: "iPhone".to_string(),
                platform: "ios".to_string(),
                ssh_key_fingerprint: format!("SHA256:{device_id}"),
                tailcat_client_key: Some(client_key.to_string()),
                api_token_hash: None,
                paired_at: "2026-11-05T10:00:00.000Z".to_string(),
                last_seen_at: None,
                // One device key per device (CDXC:RemotePairing 2026-11-11 SECURITY in
                // paired_devices.rs): the same phone's rows share it.
                device_key: Some(StoredDeviceKey {
                    algorithm: "ecdsaP256Sha256".to_string(),
                    public_key: format!("device-key-of-{client_key}"),
                    key_id: format!("key-id-of-{client_key}"),
                }),
                ..Default::default()
            })
            .expect("insert paired device");
    };
    pair("phone-old", "tailcat-phone");
    pair("other-phone", "tailcat-other");
    for params in [
        register_paired(TOKEN_A, "computer-P", "phone-old"),
        register_paired(TOKEN_C, "computer-Q", "other-phone"),
    ] {
        let (status, body) = post_push(&state, params).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    // The same phone pairs again: its new row replaces the earlier one.
    pair("phone-new", "tailcat-phone");
    let replace = |on_kept: &mut dyn FnMut(&RemotePairedDeviceRecord, &anyhow::Error)| {
        replace_earlier_pairings_of_same_phone(
            &state.paths,
            &db,
            "phone-new",
            Some("tailcat-phone"),
            "device-key-of-tailcat-phone",
            |_| Ok(true),
            on_kept,
        )
    };

    // A push store that cannot be cleaned keeps the earlier entry listed for a later Remove,
    // and the new pairing is not failed by it.
    let saved = store_bytes(&state).expect("store");
    fs::write(store_file(&state), b"{").expect("damage store");
    let mut kept = Vec::new();
    let replaced =
        replace(&mut |device, error| kept.push((device.id.clone(), format!("{error:#}"))))
            .expect("replace with a damaged store");
    assert!(replaced.is_empty());
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].0, "phone-old");
    assert!(kept[0].1.contains("is damaged"), "{}", kept[0].1);
    assert_no_token_in(&kept[0].1);
    assert!(paired_device_exists(&state, "phone-old"));
    fs::write(store_file(&state), &saved).expect("restore store");

    let replaced =
        replace(&mut |device, error| panic!("{} kept: {error:#}", device.id)).expect("replace");
    assert_eq!(
        replaced
            .iter()
            .map(|device| device.id.as_str())
            .collect::<Vec<_>>(),
        ["phone-old"]
    );
    assert!(!paired_device_exists(&state, "phone-old"));
    assert!(
        stored_entry(&state, "computer-P").is_none(),
        "the replaced pairing's registration stayed"
    );

    // The phone registers again under its new pairing; the other phone keeps its registration.
    let (status, body) =
        post_push(&state, register_paired(TOKEN_A, "computer-P", "phone-new")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        stored_entry(&state, "computer-P").expect("registered again")["deviceId"],
        json!("phone-new")
    );
    assert_eq!(
        stored_entry(&state, "computer-Q").expect("other phone kept")["token"],
        json!(TOKEN_C)
    );
    assert!(paired_device_exists(&state, "other-phone"));
}
