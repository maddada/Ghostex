//! `/api/pairOverSsh` with Easy Connect keys: a phone that signs in over SSH pairs for Easy Connect
//! too, with no click when it is the owner's own tailnet device (CDXC:RemotePairing 2026-11-12 in
//! remote_access/tailnet_owner.rs), and with the usual click otherwise.

use std::sync::Mutex;

use base64::Engine as _;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

use super::*;
use crate::remote_access::tailnet_owner::TailnetOwnerCheck;
use crate::remote_access::{
    begin_ssh_pairing, pairing_status, parse_signed_ask, parse_ssh_pairing_params, PairingContext,
    RemotePairingRepository,
};
use ghostex_gx_protocol::pairing::{pairing_status_payload, PairingStatus, DEVICE_KEY_ALGORITHM};

const TAILCAT_KEY: &str =
    "nodekey:1111111111111111111111111111111111111111111111111111111111111111";

struct Phone {
    key: EcdsaKeyPair,
    public_key: String,
    last_ask_ms: Mutex<u64>,
}

impl Phone {
    fn new() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let public_key =
            base64::engine::general_purpose::STANDARD.encode(key.public_key().as_ref());
        Self {
            key,
            public_key,
            last_ask_ms: Mutex::new(0),
        }
    }

    /// The body `sshPairing.ts` sends; `easy_connect` adds the phone's SSH and tailcat keys.
    fn body(&self, easy_connect: bool) -> Value {
        let mut body = json!({
            "deviceName": "Test iPhone",
            "platform": "ios",
            "deviceKey": { "algorithm": DEVICE_KEY_ALGORITHM, "publicKey": self.public_key },
        });
        if easy_connect {
            body["easyConnect"] = json!(true);
            body["sshPublicKey"] = json!(ed25519_line());
            body["tailcatClientKey"] = json!(TAILCAT_KEY);
        }
        body
    }

    /// A `/api/pairingStatus` ask signed with the phone's device key.
    fn ask(&self, pending_id: &str) -> Value {
        let mut last = self.last_ask_ms.lock().unwrap();
        let timestamp_ms = (crate::security::now_ms()).max(*last + 1);
        *last = timestamp_ms;
        let signature = self
            .key
            .sign(
                &ring::rand::SystemRandom::new(),
                &pairing_status_payload(pending_id, timestamp_ms),
            )
            .unwrap();
        json!({
            "pendingId": pending_id,
            "timestampMs": timestamp_ms,
            "signature": base64::engine::general_purpose::STANDARD.encode(signature.as_ref()),
        })
    }
}

/// A well-formed `ssh-ed25519` public key line.
fn ed25519_line() -> String {
    let mut blob = Vec::new();
    for part in [b"ssh-ed25519".as_slice(), &[7_u8; 32]] {
        blob.extend_from_slice(&(part.len() as u32).to_be_bytes());
        blob.extend_from_slice(part);
    }
    format!(
        "ssh-ed25519 {} ghostex-mobile",
        base64::engine::general_purpose::STANDARD.encode(blob)
    )
}

fn owned() -> TailnetOwnerCheck {
    TailnetOwnerCheck::Owned {
        peer_ip: "100.71.245.28".parse().unwrap(),
        node_name: "test-phone".to_string(),
    }
}

fn asks() -> TailnetOwnerCheck {
    TailnetOwnerCheck::Ask {
        reason: "the SSH login did not come from a tailnet address".to_string(),
    }
}

/// These tests share the process-wide pairing rate limit, so they run one at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

struct Computer {
    _temp: tempfile::TempDir,
    state: Arc<AppState>,
    /// The SSH key lines the pairing wrote (device id), in place of `~/.ssh/authorized_keys`.
    key_lines: Mutex<Vec<String>>,
}

impl Computer {
    fn new() -> (std::sync::MutexGuard<'static, ()>, Self) {
        let guard = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::remote_access::pair_device::forget_pair_device_attempts();
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        std::fs::create_dir_all(paths.config_file.parent().unwrap()).unwrap();
        (
            guard,
            Self {
                state: test_app_state(paths),
                _temp: temp,
                key_lines: Mutex::new(Vec::new()),
            },
        )
    }

    fn begin(
        &self,
        phone: &Phone,
        easy_connect: bool,
        owner: &TailnetOwnerCheck,
    ) -> crate::remote_access::SshPairingStarted {
        let db = crate::storage::open_gxserver_database(&self.state.paths).unwrap();
        let params = parse_ssh_pairing_params(&phone.body(easy_connect)).expect("params");
        begin_ssh_pairing(
            &db,
            &self.state.remote_pairing_runtime,
            &self.state.security,
            params,
            owner,
        )
        .expect("begin")
    }

    fn status(&self, phone: &Phone, pending_id: &str) -> crate::remote_access::PairingStatusAnswer {
        let db = crate::storage::open_gxserver_database(&self.state.paths).unwrap();
        let append = |_key: &crate::remote_access::ParsedSshPublicKey, id: &str, _name: &str| {
            self.key_lines.lock().unwrap().push(id.to_string());
            Ok(())
        };
        let remove = |id: &str| {
            let mut lines = self.key_lines.lock().unwrap();
            let before = lines.len();
            lines.retain(|line| line != id);
            Ok(lines.len() != before)
        };
        let context = PairingContext {
            paths: &self.state.paths,
            db: &db,
            tailcat_runtime: &self.state.tailcat_runtime,
            pairing_runtime: &self.state.remote_pairing_runtime,
            device_tokens: &self.state.device_api_tokens,
            security: &self.state.security,
            logger: &self.state.logger,
            append_key: &append,
            remove_key: &remove,
        };
        pairing_status(&context, parse_signed_ask(&phone.ask(pending_id)).unwrap()).expect("status")
    }

    fn devices(&self) -> Vec<crate::remote_access::RemotePairedDeviceRecord> {
        let db = crate::storage::open_gxserver_database(&self.state.paths).unwrap();
        RemotePairingRepository::new(&db).list_devices().unwrap()
    }

    fn waiting_requests(&self) -> usize {
        crate::remote_access::waiting_devices(
            &self.state.remote_pairing_runtime,
            &self.state.security,
        )
        .len()
    }
}

#[test]
fn the_owners_own_tailnet_phone_pairs_for_easy_connect_without_a_click() {
    let (_one_at_a_time, computer) = Computer::new();
    let phone = Phone::new();
    let started = computer.begin(&phone, true, &owned());
    assert!(started.auto_approved);
    // Nothing waits on the computer: no card, no request.
    assert_eq!(computer.waiting_requests(), 0);

    let answer = computer.status(&phone, &started.started.pending_id);
    assert_eq!(answer.status, PairingStatus::Approved);
    let approved = answer.approved.expect("approved answer");
    assert!(approved["deviceApiToken"]
        .as_str()
        .is_some_and(|token| !token.is_empty()));

    let devices = computer.devices();
    assert_eq!(devices.len(), 1);
    let device = &devices[0];
    // What a QR's phone gets: the restricted SSH key line and the Easy Connect key.
    assert_eq!(device.tailcat_client_key.as_deref(), Some(TAILCAT_KEY));
    assert!(device.ssh_key_fingerprint.starts_with("SHA256:"));
    assert_eq!(*computer.key_lines.lock().unwrap(), vec![device.id.clone()]);

    // A lost reply is answered again from the kept answer, without pairing twice.
    let again = computer.status(&phone, &started.started.pending_id);
    assert_eq!(again.status, PairingStatus::Approved);
    assert_eq!(computer.devices().len(), 1);
}

#[test]
fn a_phone_that_is_not_the_owners_tailnet_device_waits_for_the_click() {
    let (_one_at_a_time, computer) = Computer::new();
    let phone = Phone::new();
    let started = computer.begin(&phone, true, &asks());
    assert!(!started.auto_approved);
    assert_eq!(computer.waiting_requests(), 1);

    let answer = computer.status(&phone, &started.started.pending_id);
    assert_eq!(answer.status, PairingStatus::Pending);
    assert!(computer.devices().is_empty());
    assert!(computer.key_lines.lock().unwrap().is_empty());
}

#[test]
fn an_owned_check_never_approves_a_pairing_without_easy_connect() {
    // The trust-only ask of an older flow keeps its click even from the owner's own device.
    let (_one_at_a_time, computer) = Computer::new();
    let phone = Phone::new();
    let started = computer.begin(&phone, false, &owned());
    assert!(!started.auto_approved);
    assert_eq!(computer.waiting_requests(), 1);
    let answer = computer.status(&phone, &started.started.pending_id);
    assert_eq!(answer.status, PairingStatus::Pending);
}

#[test]
fn another_device_cannot_finish_an_auto_approved_pairing() {
    let (_one_at_a_time, computer) = Computer::new();
    let phone = Phone::new();
    let started = computer.begin(&phone, true, &owned());
    let stranger = Phone::new();
    let db = crate::storage::open_gxserver_database(&computer.state.paths).unwrap();
    let context = PairingContext {
        paths: &computer.state.paths,
        db: &db,
        tailcat_runtime: &computer.state.tailcat_runtime,
        pairing_runtime: &computer.state.remote_pairing_runtime,
        device_tokens: &computer.state.device_api_tokens,
        security: &computer.state.security,
        logger: &computer.state.logger,
        append_key: &|_, _, _| panic!("no key line for a stranger"),
        remove_key: &|_| Ok(false),
    };
    let ask = parse_signed_ask(&stranger.ask(&started.started.pending_id)).unwrap();
    assert!(pairing_status(&context, ask).is_err());
    assert!(computer.devices().is_empty());
}

#[test]
fn pairing_for_easy_connect_replaces_the_phones_ssh_only_entry() {
    let (_one_at_a_time, computer) = Computer::new();
    let phone = Phone::new();
    // The phone was trusted over SSH earlier (an entry with its device key, no keys of its own).
    let db = crate::storage::open_gxserver_database(&computer.state.paths).unwrap();
    RemotePairingRepository::new(&db)
        .insert_device(&crate::remote_access::RemotePairedDeviceRecord {
            id: "earlier".to_string(),
            name: "Test iPhone".to_string(),
            platform: "ios".to_string(),
            ssh_key_fingerprint: String::new(),
            tailcat_client_key: None,
            api_token_hash: None,
            paired_at: crate::remote_access::now_iso(),
            last_seen_at: None,
            status: crate::remote_access::PairedDeviceStatus::Paired,
            device_key: Some(crate::remote_access::StoredDeviceKey {
                algorithm: DEVICE_KEY_ALGORITHM.to_string(),
                public_key: phone.public_key.clone(),
                key_id: "earlier-key".to_string(),
            }),
            api_token_expires_at: None,
            approver_key_id: None,
            legacy_key_line_removed: false,
        })
        .unwrap();
    let started = computer.begin(&phone, true, &owned());
    assert_eq!(
        computer.status(&phone, &started.started.pending_id).status,
        PairingStatus::Approved
    );
    let devices = computer.devices();
    assert_eq!(devices.len(), 1, "{devices:?}");
    assert_ne!(devices[0].id, "earlier");
    assert_eq!(devices[0].tailcat_client_key.as_deref(), Some(TAILCAT_KEY));
}

#[test]
fn easy_connect_needs_both_keys_and_an_older_body_keeps_its_meaning() {
    let phone = Phone::new();
    let mut body = phone.body(true);
    body.as_object_mut().unwrap().remove("tailcatClientKey");
    assert!(parse_ssh_pairing_params(&body).is_err());
    let mut body = phone.body(true);
    body["sshPublicKey"] = json!("not a key");
    assert!(parse_ssh_pairing_params(&body).is_err());
    // An older phone's body (no `easyConnect`) never pairs keys, even if it names some.
    let mut body = phone.body(true);
    body.as_object_mut().unwrap().remove("easyConnect");
    let params = parse_ssh_pairing_params(&body).unwrap();
    assert!(params.ssh_public_key.is_empty());
    assert!(params.tailcat_client_key.is_none());
}

#[tokio::test]
async fn the_route_asks_for_the_click_when_it_cannot_trace_the_login() {
    // Through `route_http` no listener recorded the address, so the check can't be made.
    let (_one_at_a_time, computer) = Computer::new();
    let phone = Phone::new();
    let routed = route_http(
        computer.state.clone(),
        rpc_request(
            "/api/pairOverSsh",
            &computer.state.auth_token.clone(),
            json!({ "params": phone.body(true) }),
        ),
        "pair-over-ssh".to_string(),
    )
    .await;
    let status = routed.response.status();
    let body = response_json(routed.response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["autoApproved"], json!(false));
    assert_eq!(body["result"]["status"], json!("pending"));
    assert_eq!(computer.waiting_requests(), 1);
}
