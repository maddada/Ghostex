//! Pairing v2 (GitHub #495): device keys, Pending, the three match words, the signed approval,
//! the key-bound device token, renewal, revoke, and the retirement of pre-11.0.1 pairings.
//!
//! Nothing here touches the real `~/.ssh/authorized_keys`: finishing a pairing goes through a
//! [`PairingContext`] whose key-file writer records lines in memory.

use std::sync::Mutex as StdMutex;

use super::*;
use crate::remote_access::{
    authorized_key_line, begin_pairing, easy_connect_devices_to_pair_again,
    easy_connect_pairing_entry, forget_approver_key, has_active_easy_connect_devices, match_words,
    may_show_pairing_code, pairing_status, parse_pair_device_params, reload_device_api_tokens,
    remove_paired_device_using, renew_device_token, reset_pair_device_attempts,
    retire_legacy_pairings_using, EasyConnectCode, PairDeviceError, PairDeviceParams,
    PairedDeviceStatus, PairingContext, PairingStarted, RemotePairedDeviceRecord,
    RemotePairingRepository, SignedAskParams,
};
use crate::security::{test_keys::TestApprover, Principal, SensitiveAction};
use crate::storage::open_gxserver_database;
use ghostex_gx_protocol::approval::{approval_signing_payload, ApprovalStatus};
use ghostex_gx_protocol::pairing::{
    device_token_renew_payload, pairing_match_words, pairing_status_payload, pairing_words_input,
    PairingStatus,
};
use sha2::{Digest, Sha256};

const PHONE_SSH_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl ghostex-mobile";

/// Serialises these tests: `/api/pairDevice` counts attempts per process.
static PAIRING_TESTS: StdMutex<()> = StdMutex::new(());

fn temp_state() -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    (temp, test_app_state_for(paths, "S90"))
}

/// The lines a finished pairing would have written to `authorized_keys`.
#[derive(Default)]
struct KeyFile {
    lines: StdMutex<Vec<String>>,
}

impl KeyFile {
    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

fn with_context<T>(
    state: &AppState,
    keys: &KeyFile,
    run: impl FnOnce(&PairingContext<'_>) -> T,
) -> T {
    let db = open_gxserver_database(&state.paths).expect("db");
    let append = |key: &crate::remote_access::ParsedSshPublicKey, id: &str, name: &str| {
        keys.lines
            .lock()
            .unwrap()
            .push(authorized_key_line(key, id, name));
        Ok(())
    };
    let remove = |id: &str| {
        let marker = format!("ghostex-paired:{id}");
        let mut lines = keys.lines.lock().unwrap();
        let before = lines.len();
        lines.retain(|line| !line.split_ascii_whitespace().any(|field| field == marker));
        Ok(lines.len() != before)
    };
    let context = PairingContext {
        paths: &state.paths,
        db: &db,
        tailcat_runtime: &state.tailcat_runtime,
        pairing_runtime: &state.remote_pairing_runtime,
        device_tokens: &state.device_api_tokens,
        security: &state.security,
        logger: &state.logger,
        append_key: &append,
        remove_key: &remove,
    };
    run(&context)
}

fn pair_params(
    secret: &str,
    phone: &TestApprover,
    approver: Option<&TestApprover>,
) -> PairDeviceParams {
    let mut body = json!({
        "secret": secret,
        "deviceName": "Pixel 8",
        "platform": "android",
        "sshPublicKey": PHONE_SSH_KEY,
        "tailcatClientKey": format!("nodekey:{}", "ab".repeat(32)),
        "deviceKey": { "algorithm": "ecdsaP256Sha256", "publicKey": phone.key.public_key },
    });
    if let Some(approver) = approver {
        body["approverKey"] = json!({
            "algorithm": "ecdsaP256Sha256",
            "publicKey": approver.key.public_key,
            "presence": "biometric",
        });
    }
    parse_pair_device_params(&body).expect("valid pairDevice params")
}

fn live_secret(state: &AppState) -> String {
    let db = open_gxserver_database(&state.paths).expect("db");
    state
        .remote_pairing_runtime
        .ensure_secret(&db)
        .expect("secret")
        .secret
}

fn begin(state: &AppState, params: PairDeviceParams) -> Result<PairingStarted, PairDeviceError> {
    let db = open_gxserver_database(&state.paths).expect("db");
    begin_pairing(&db, &state.remote_pairing_runtime, &state.security, params)
}

fn signed_status(phone: &TestApprover, pending_id: &str, timestamp_ms: u64) -> SignedAskParams {
    SignedAskParams {
        id: pending_id.to_string(),
        timestamp_ms,
        signature: phone.sign(&pairing_status_payload(pending_id, timestamp_ms)),
    }
}

/// The owner's desktop approves the pending request with its approver key.
fn approve_as_owner(state: &AppState, approval_id: &str) {
    let owner = TestApprover::new("mac");
    state
        .security
        .approver_keys
        .insert(owner.key.clone())
        .unwrap();
    let request = state
        .security
        .approvals
        .read(
            approval_id,
            &Principal::Owner {
                device_id: "mac".into(),
            },
            now_ms(),
        )
        .expect("request");
    let signature =
        owner.sign(&approval_signing_payload(&state.security.server_id, &request).unwrap());
    state
        .security
        .approvals
        .approve(
            approval_id,
            &owner.key.key_id,
            &signature,
            &state.security.approver_keys,
            &state.security.server_id,
            now_ms(),
        )
        .expect("approve");
}

fn now_ms() -> u64 {
    crate::security::now_ms()
}

#[test]
fn a_phone_is_trusted_only_after_matching_words_and_a_signed_approval() {
    let _serial = PAIRING_TESTS.lock().unwrap_or_else(|p| p.into_inner());
    reset_pair_device_attempts();
    let (_temp, state) = temp_state();
    let keys = KeyFile::default();
    let phone = TestApprover::new("phone");
    let secret = live_secret(&state);

    let started = begin(&state, pair_params(&secret, &phone, None)).expect("pending");
    // The phone works the words out on its own from the QR's nonce and its key: the same three.
    let digest = Sha256::digest(pairing_words_input(&secret, &phone.key.public_key));
    let own_words: Vec<String> = pairing_match_words(&digest)
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(started.words, own_words);
    assert_eq!(started.words.len(), 3);
    assert_eq!(started.words, match_words(&secret, &phone.key.public_key));

    // Pending: listed with the words, an approval request waits, nothing is granted yet.
    let waiting = state.remote_pairing_runtime.list_waiting(now_ms());
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].words, started.words);
    let request = state
        .security
        .approvals
        .list_pending(now_ms())
        .into_iter()
        .find(|request| request.id == waiting[0].approval_id)
        .expect("approval request");
    assert_eq!(request.action, SensitiveAction::PairingApproveDevice);
    assert!(request
        .detail
        .as_deref()
        .unwrap()
        .contains(&started.words.join(" · ")));
    assert!(keys.lines().is_empty());
    let db = open_gxserver_database(&state.paths).unwrap();
    assert!(RemotePairingRepository::new(&db)
        .list_devices()
        .unwrap()
        .is_empty());

    // Asking needs the phone's own key, a fresh clock, and a newer timestamp each time.
    let t0 = now_ms();
    let answer = with_context(&state, &keys, |context| {
        pairing_status(context, signed_status(&phone, &started.pending_id, t0))
    })
    .expect("status");
    assert_eq!(answer.status, PairingStatus::Pending);
    let stranger = TestApprover::new("stranger");
    for ask in [
        signed_status(&stranger, &started.pending_id, t0 + 1),
        signed_status(&phone, &started.pending_id, t0),
        signed_status(&phone, &started.pending_id, t0 + 10 * 60 * 1000),
    ] {
        assert!(matches!(
            with_context(&state, &keys, |context| pairing_status(context, ask)),
            Err(PairDeviceError::BadSignature(_))
        ));
    }

    approve_as_owner(&state, &waiting[0].approval_id);
    let answer = with_context(&state, &keys, |context| {
        pairing_status(context, signed_status(&phone, &started.pending_id, t0 + 2))
    })
    .expect("approved");
    assert_eq!(answer.status, PairingStatus::Approved);
    let approved = answer.approved.expect("approved payload");
    let device_id = approved["deviceId"].as_str().unwrap().to_string();
    let token = approved["deviceApiToken"].as_str().unwrap().to_string();

    // The SSH line is restricted, the row carries the device key, the token names the device.
    let lines = keys.lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("restrict,pty,port-forwarding ssh-ed25519 "));
    assert!(lines[0].contains(&format!("ghostex-paired:{device_id}")));
    let device = RemotePairingRepository::new(&db)
        .find_device(&device_id)
        .unwrap()
        .expect("device row");
    assert_eq!(device.status, PairedDeviceStatus::Paired);
    assert_eq!(
        device.device_key.as_ref().unwrap().key_id,
        crate::security::approver_key_id(
            &base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &phone.key.public_key
            )
            .unwrap()
        )
    );
    assert!(device.api_token_expires_at.is_some());
    assert_eq!(
        state
            .device_api_tokens
            .entry_for(&token)
            .unwrap()
            .device_id
            .as_deref(),
        Some(device_id.as_str())
    );

    // A lost reply is asked for again and gets the same answer; the approval itself is spent.
    let again = with_context(&state, &keys, |context| {
        pairing_status(context, signed_status(&phone, &started.pending_id, t0 + 3))
    })
    .unwrap();
    assert_eq!(again.approved.unwrap()["deviceId"], json!(device_id));
    assert_eq!(keys.lines().len(), 1);
    let spent = state
        .security
        .approvals
        .read(&waiting[0].approval_id, &Principal::Anonymous, now_ms())
        .unwrap();
    assert_eq!(spent.status, ApprovalStatus::Consumed);
}

#[test]
fn a_used_code_and_an_old_app_are_refused() {
    let _serial = PAIRING_TESTS.lock().unwrap_or_else(|p| p.into_inner());
    reset_pair_device_attempts();
    let (_temp, state) = temp_state();
    let phone = TestApprover::new("phone");
    let secret = live_secret(&state);
    begin(&state, pair_params(&secret, &phone, None)).expect("first scan");
    // Single use: the same code can't make a second Pending entry.
    assert!(matches!(
        begin(
            &state,
            pair_params(&secret, &TestApprover::new("other"), None)
        ),
        Err(PairDeviceError::CodeExpired)
    ));
    // An app from before pairing v2 sends no device key.
    let old_app = json!({
        "secret": "x",
        "deviceName": "iPhone",
        "platform": "ios",
        "sshPublicKey": PHONE_SSH_KEY,
    });
    assert!(matches!(
        parse_pair_device_params(&old_app),
        Err(PairDeviceError::NeedsAppUpdate)
    ));
    let bad_key = json!({
        "secret": "x",
        "deviceName": "iPhone",
        "platform": "ios",
        "sshPublicKey": PHONE_SSH_KEY,
        "deviceKey": { "algorithm": "ecdsaP256Sha256", "publicKey": "AAAA" },
    });
    assert!(matches!(
        parse_pair_device_params(&bad_key),
        Err(PairDeviceError::BadRequest(_))
    ));
}

#[tokio::test]
async fn denying_a_pending_phone_tells_it_and_grants_nothing() {
    let started_and_phone = {
        let _serial = PAIRING_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        reset_pair_device_attempts();
        let (temp, state) = temp_state();
        let phone = TestApprover::new("phone");
        let secret = live_secret(&state);
        let started = begin(&state, pair_params(&secret, &phone, None)).expect("pending");
        (temp, state, phone, started)
    };
    let (_temp, state, phone, started) = started_and_phone;

    // The desktop and the phones already paired see it under `pending`, with the words.
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/pairedDevices",
            &state.auth_token.clone(),
            json!({ "params": {} }),
        ),
        "list".to_string(),
    )
    .await;
    let body = response_json(routed.response).await;
    assert_eq!(body["result"]["pending"][0]["words"], json!(started.words));

    // Remove on a Pending row denies it (closing a door needs no approval).
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/removePairedDevice",
            &state.auth_token.clone(),
            json!({ "params": { "deviceId": started.pending_id } }),
        ),
        "deny".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);
    let keys = KeyFile::default();
    let answer = with_context(&state, &keys, |context| {
        pairing_status(
            context,
            signed_status(&phone, &started.pending_id, now_ms()),
        )
    })
    .unwrap();
    // The entry is gone with the denial, so the phone hears that its pairing is over.
    assert!(matches!(
        answer.status,
        PairingStatus::Denied | PairingStatus::Expired
    ));
    assert!(keys.lines().is_empty());
    assert!(state
        .remote_pairing_runtime
        .list_waiting(now_ms())
        .is_empty());
}

#[tokio::test]
async fn the_status_and_renewal_routes_take_no_token_and_refuse_a_bad_signature() {
    let (_temp, state) = temp_state();
    for path in ["/api/pairingStatus", "/api/renewDeviceToken"] {
        let routed = route_http(
            state.clone(),
            rpc_request(
                path,
                "",
                json!({ "params": { "pendingId": "pd_x", "deviceId": "d", "timestampMs": now_ms(), "signature": "AAAA" } }),
            ),
            "ask".to_string(),
        )
        .await;
        let status = routed.response.status();
        let body = response_json(routed.response).await;
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "{path} must not need a bearer token: {body}"
        );
        assert!(
            body.to_string().find("deviceApiToken").is_none(),
            "{path}: {body}"
        );
    }
}

fn pair_fully(
    state: &AppState,
    keys: &KeyFile,
    phone: &TestApprover,
    approver: Option<&TestApprover>,
) -> (String, String) {
    let secret = live_secret(state);
    let started = begin(state, pair_params(&secret, phone, approver)).expect("pending");
    let approval_id = state.remote_pairing_runtime.list_waiting(now_ms())[0]
        .approval_id
        .clone();
    approve_as_owner(state, &approval_id);
    let answer = with_context(state, keys, |context| {
        pairing_status(context, signed_status(phone, &started.pending_id, now_ms()))
    })
    .expect("approved");
    let approved = answer.approved.unwrap();
    (
        approved["deviceId"].as_str().unwrap().to_string(),
        approved["deviceApiToken"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn the_device_token_is_bound_to_the_key_and_renewal_replaces_it() {
    let (_temp, state, phone, device_id, token) = {
        let _serial = PAIRING_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        reset_pair_device_attempts();
        let (temp, state) = temp_state();
        let phone = TestApprover::new("phone");
        let (device_id, token) = pair_fully(&state, &KeyFile::default(), &phone, None);
        (temp, state, phone, device_id, token)
    };

    // The token opens the API as this device.
    let routed = route_http(
        state.clone(),
        rpc_request("/api/easyConnectAddresses", &token, json!({ "params": {} })),
        "as-device".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);

    let db = open_gxserver_database(&state.paths).unwrap();
    let renew = |key: &TestApprover, timestamp_ms: u64| {
        renew_device_token(
            &db,
            &state.remote_pairing_runtime,
            &state.device_api_tokens,
            SignedAskParams {
                id: device_id.clone(),
                timestamp_ms,
                signature: key.sign(&device_token_renew_payload(&device_id, timestamp_ms)),
            },
        )
    };
    let t0 = now_ms();
    // Another key, a stale clock: refused, and the old token still works.
    assert!(matches!(
        renew(&TestApprover::new("thief"), t0),
        Err(PairDeviceError::BadSignature(_))
    ));
    assert!(matches!(
        renew(&phone, t0 - 10 * 60 * 1000),
        Err(PairDeviceError::BadSignature(_))
    ));
    assert!(state.device_api_tokens.accepts(&token));

    let fresh = renew(&phone, t0).expect("renewed");
    let fresh_token = fresh["deviceApiToken"].as_str().unwrap();
    assert!(state.device_api_tokens.accepts(fresh_token));
    assert!(
        !state.device_api_tokens.accepts(&token),
        "the old token stops working"
    );
    // The same signed renewal can't be replayed.
    assert!(matches!(
        renew(&phone, t0),
        Err(PairDeviceError::BadSignature(_))
    ));
}

#[test]
fn revoking_a_device_removes_its_key_line_token_and_approver_key() {
    let _serial = PAIRING_TESTS.lock().unwrap_or_else(|p| p.into_inner());
    reset_pair_device_attempts();
    let (_temp, state) = temp_state();
    let keys = KeyFile::default();
    let phone = TestApprover::new("phone");
    let phone_approver = TestApprover::new("phone-face-id");
    let (device_id, token) = pair_fully(&state, &keys, &phone, Some(&phone_approver));
    // Approving the pairing also enrolled the phone's approver key, once.
    let enrolled = state
        .security
        .approver_keys
        .get(&phone_approver.key.key_id)
        .expect("phone approver key");
    assert_eq!(enrolled.device_id, device_id);

    let db = open_gxserver_database(&state.paths).unwrap();
    let removed = remove_paired_device_using(
        &state.paths,
        &db,
        &state.tailcat_runtime,
        &state.device_api_tokens,
        &device_id,
        |id| {
            let marker = format!("ghostex-paired:{id}");
            let mut lines = keys.lines.lock().unwrap();
            lines.retain(|line| !line.contains(&marker));
            Ok(true)
        },
    )
    .unwrap()
    .expect("removed");
    forget_approver_key(&state.security, &removed, &Principal::LocalAgent);

    assert!(keys.lines().is_empty());
    assert!(!state.device_api_tokens.accepts(&token));
    assert!(state
        .security
        .approver_keys
        .get(&phone_approver.key.key_id)
        .is_none());
    assert!(RemotePairingRepository::new(&db)
        .find_device(&device_id)
        .unwrap()
        .is_none());
}

#[test]
fn pairings_from_before_11_0_1_stop_working_and_are_listed_to_pair_again() {
    let (_temp, state) = temp_state();
    let db = open_gxserver_database(&state.paths).unwrap();
    assert!(!has_active_easy_connect_devices(&db).unwrap());
    let old_token = crate::auth::mint_device_api_token();
    RemotePairingRepository::new(&db)
        .insert_device(&RemotePairedDeviceRecord {
            id: "old".into(),
            name: "iPhone".into(),
            platform: "ios".into(),
            ssh_key_fingerprint: "SHA256:old".into(),
            api_token_hash: Some(crate::auth::hash_device_api_token(&old_token)),
            paired_at: "2026-10-01T00:00:00.000Z".into(),
            status: PairedDeviceStatus::Legacy,
            ..Default::default()
        })
        .unwrap();
    reload_device_api_tokens(&db, &state.device_api_tokens).unwrap();
    assert!(!state.device_api_tokens.accepts(&old_token));

    let mut asked = Vec::new();
    let removed = retire_legacy_pairings_using(&db, |id| {
        asked.push(id.to_string());
        Ok(Some(true))
    })
    .unwrap();
    assert_eq!((removed, asked), (1, vec!["old".to_string()]));
    // Done once: the next start doesn't touch the file again.
    assert_eq!(
        retire_legacy_pairings_using(&db, |_| panic!("asked twice")).unwrap(),
        0
    );
    assert!(has_active_easy_connect_devices(&db).unwrap());
    assert_eq!(
        easy_connect_devices_to_pair_again(&db).unwrap(),
        vec!["iPhone".to_string()]
    );

    // A line that needs a Windows administrator prompt waits for the user's Remove.
    RemotePairingRepository::new(&db)
        .insert_device(&RemotePairedDeviceRecord {
            id: "admin".into(),
            name: "Pixel".into(),
            platform: "android".into(),
            ssh_key_fingerprint: "SHA256:admin".into(),
            paired_at: "2026-10-02T00:00:00.000Z".into(),
            status: PairedDeviceStatus::Legacy,
            ..Default::default()
        })
        .unwrap();
    retire_legacy_pairings_using(&db, |_| Ok(None)).unwrap();
    let admin = RemotePairingRepository::new(&db)
        .find_device("admin")
        .unwrap()
        .unwrap();
    assert!(!admin.legacy_key_line_removed);
}

#[tokio::test]
async fn no_api_hands_out_a_pairing_code_except_to_the_owner_with_the_sheet_open() {
    assert!(may_show_pairing_code(&Principal::Owner {
        device_id: "mac".into()
    }));
    for caller in [
        Principal::LocalAgent,
        Principal::Device {
            device_id: "phone".into(),
        },
        Principal::Session {
            project_id: "p".into(),
            session_id: "s".into(),
            role: crate::security::SessionRole::Coordinator,
        },
        Principal::Anonymous,
    ] {
        assert!(!may_show_pairing_code(&caller), "{caller:?}");
    }
    let code = EasyConnectCode {
        v: 1,
        address: "tcAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJw".into(),
        name: "Studio".into(),
        user: "madda".into(),
        port: 58744,
        ssh_port: 22,
        secret: None,
        expires_at: None,
    };
    let entry = easy_connect_pairing_entry(code.clone(), code.address.clone());
    assert_eq!(entry["payload"], "");
    assert!(entry["code"].get("secret").is_none());
    // The bare code another computer connects with stays, without a secret.
    let text = entry["text"].as_str().unwrap();
    let body = text.strip_prefix("ghostex-ec1:").unwrap();
    let decoded: Value = serde_json::from_slice(
        &base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, body).unwrap(),
    )
    .unwrap();
    assert!(decoded.get("secret").is_none());
    let shown = easy_connect_pairing_entry(
        EasyConnectCode {
            secret: Some("ZGVmZ2hpamtsbW5vcHFycw".into()),
            ..code.clone()
        },
        code.address,
    );
    assert!(shown["payload"]
        .as_str()
        .unwrap()
        .starts_with("https://ghostex.dev/pair#"));

    // Through the gate: the main token (agents, the CLI) asks with the sheet "open" and gets no code.
    let (_temp, state) = temp_state();
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/remotePairingCode",
            &state.auth_token.clone(),
            json!({ "params": { "openTunnel": true } }),
        ),
        "code".to_string(),
    )
    .await;
    let body = response_json(routed.response).await;
    assert!(!body.to_string().contains("\"secret\""), "{body}");
    assert!(
        RemotePairingRepository::new(&open_gxserver_database(&state.paths).unwrap())
            .read_secret()
            .unwrap()
            .is_none(),
        "no code was minted"
    );
}
