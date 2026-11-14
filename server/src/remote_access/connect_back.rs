//! Connect both ways: a computer that just connected to this one asks it to connect back.
//!
//! CDXC:RemotePairing 2026-11-14 DECISION:
//! User, on the Easy Connect page (Q2 "Connect back after", named "Connect both ways"): the Done
//! screen of the computer that entered the code offers Connect both ways, and one click makes the
//! other computer connect back to it, with no second code to carry. It runs as two calls:
//! 1. On the computer that entered the code (A), its desktop asks its own gxserver to sign the
//!    ask (`/api/signComputerConnectBack`): A's pairing device key signs A's own fresh code's
//!    secret and the time, so the ask can only come from a computer B already paired.
//! 2. A's desktop, which is connected to B, sends the ask to B's gxserver
//!    (`/api/connectBackToComputer`). B checks the signature against the computers paired with it,
//!    then pairs with A exactly as if B's owner had pasted A's code (`computer_pairing.rs`), with
//!    B's own SSH key. A's desktop approves the request on A (the click was the owner's; the
//!    approvals rule still applies there), and once A approves, B's gxserver has B's desktop add
//!    A to its sidebar (renderer command `addRemoteMachine`).
//! An older B answers the second call as unknown, and A says to update Ghostex there. The same
//! two steps are what a phone paired with both computers will run later (Q5: "let a phone paired
//! with both computers link them to each other, behind an 'are you sure?' confirmation"): it asks
//! one computer for a code and hands it to the other; the signed ask here is only the computer's
//! way of proving who asks, and a phone proves it with its device token instead.
//! SEE-ALSO: server/src/remote_access/computer_pairing.rs, apps/desktop/src/app/connect_computer_lifecycle.rs,
//! packages/gx-core/src/renderer_commands/verbs.rs (`addRemoteMachine`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, bail, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use ghostex_gx_protocol::pairing::DEVICE_KEY_ALGORITHM;

use crate::paths::GxserverPaths;

use super::computer_pairing::{
    device_public_key, device_sign, load_or_create_device_key, start_computer_pairing, AddMachine,
    ComputerPairingCode,
};
use super::pair_device::verify_device_signature;
use super::repository::{PairedDeviceStatus, RemotePairingRepository};

pub const SIGN_CONNECT_BACK_ENDPOINT: &str = "/api/signComputerConnectBack";
pub const CONNECT_BACK_ENDPOINT: &str = "/api/connectBackToComputer";

/// How old a signed ask may be when it arrives.
const ASK_MAX_AGE_MS: u64 = 5 * 60 * 1000;

/// What the device key signs: the code's secret (hashed, so the signature never carries it) and
/// the time.
fn connect_back_payload(secret: &str, timestamp_ms: u64) -> Vec<u8> {
    let secret_hash = hex::encode(Sha256::digest(secret.as_bytes()));
    format!("ghostex-connect-back-v1\n{secret_hash}\n{timestamp_ms}").into_bytes()
}

/// `/api/signComputerConnectBack` on the computer that wants the other to connect back: signs its
/// own code's `secret` with this computer's pairing device key.
pub fn sign_connect_back(paths: &GxserverPaths, params: &Value) -> Result<Value> {
    let secret = params
        .get("secret")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| anyhow!("secret is required."))?;
    let key = load_or_create_device_key(paths)?;
    let timestamp_ms = crate::security::now_ms();
    Ok(json!({
        "deviceKey": { "algorithm": DEVICE_KEY_ALGORITHM, "publicKey": device_public_key(&key)? },
        "timestampMs": timestamp_ms,
        "signature": device_sign(&key, &connect_back_payload(secret, timestamp_ms))?,
    }))
}

fn last_timestamps() -> &'static Mutex<HashMap<String, u64>> {
    static LAST: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

fn text(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn port(params: &Value, key: &str) -> Option<u16> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
}

/// `/api/connectBackToComputer`: a computer paired with this one asks it to pair back with that
/// computer's code and add it to this computer's sidebar once approved there. Answers what
/// `/api/pairComputer` answers, plus `addsMachine`.
pub fn connect_back(db: &Connection, paths: &GxserverPaths, params: &Value) -> Result<Value> {
    let public_key = params
        .pointer("/deviceKey/publicKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let timestamp_ms = params
        .get("timestampMs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let signature = text(params, "signature");
    let code = ComputerPairingCode {
        address: text(params, "address"),
        port: port(params, "port").unwrap_or(0),
        secret: text(params, "secret"),
        name: text(params, "name"),
    };
    let user = text(params, "user");
    let ssh_port = port(params, "sshPort").unwrap_or(22);
    if code.address.is_empty() || code.port == 0 || code.secret.is_empty() || user.is_empty() {
        bail!("The other computer's code is missing its address, user or secret.");
    }
    if code.name.is_empty()
        || code.name.chars().count() > 80
        || code.name.chars().any(char::is_control)
    {
        bail!("The other computer's name is not valid.");
    }
    if user.starts_with('-') || user.chars().any(char::is_control) {
        bail!("The other computer's user is not valid.");
    }
    // Only a computer paired with this one may ask, proven by its pairing device key.
    let asker = RemotePairingRepository::new(db)
        .list_devices()?
        .into_iter()
        .find(|device| {
            device.status == PairedDeviceStatus::Paired
                && device.api_token_hash.is_none()
                && device.tailcat_client_key.is_some()
                && device
                    .device_key
                    .as_ref()
                    .is_some_and(|key| !public_key.is_empty() && key.public_key == public_key)
        })
        .ok_or_else(|| {
            anyhow!("Only a computer connected to this one through Easy Connect can ask it to connect back.")
        })?;
    let now = crate::security::now_ms();
    let fresh = timestamp_ms + ASK_MAX_AGE_MS >= now && timestamp_ms <= now + ASK_MAX_AGE_MS;
    {
        let mut last = last_timestamps()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let seen = last.get(&public_key).copied().unwrap_or(0);
        if !fresh
            || timestamp_ms <= seen
            || !verify_device_signature(
                &public_key,
                &connect_back_payload(&code.secret, timestamp_ms),
                &signature,
            )
        {
            bail!(
                "This ask isn't signed by {}, or its clock is more than five minutes off.",
                asker.name
            );
        }
        last.insert(public_key.clone(), timestamp_ms);
    }
    let mut answer = start_computer_pairing(paths, code, Some(AddMachine { user, ssh_port }))?;
    answer["addsMachine"] = json!(true);
    Ok(answer)
}
