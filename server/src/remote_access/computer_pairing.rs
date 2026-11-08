//! The connecting computer's side of pairing a computer through Easy Connect (GitHub #495).
//!
//! CDXC:RemotePairing 2026-11-08 DECISION:
//! User (GitHub #495): "Computers connecting through Easy Connect must not need their key added by
//! hand under Advanced: give them the same flow as phones." Since an empty Easy Connect allow-list
//! is closed, a computer's tunnel only gets through once its tailcat client key is listed. This
//! gxserver pairs it the way a phone pairs:
//!
//! 1. Make sure tailcat's `client-default` key exists (every tailcat client here dials with it;
//!    without it tailcat dials with a new random key each time, which no list could hold) and read
//!    its public key.
//! 2. Use this computer's pairing device key: P-256, made once, kept in gxserver's state folder
//!    (0600). It only signs pairing asks, so a software key is enough on macOS, Windows and Linux.
//! 3. Open a tailcat tunnel to the other computer's gxserver (its Show QR sheet holds Easy Connect
//!    open to new devices) and send `/api/pairDevice` with `kind: "computer"`, the code's one-time
//!    secret, the device key and the tailcat key.
//! 4. Work the three words out here and refuse a pairing whose words differ, then ask
//!    `/api/pairingStatus`, signed, until the owner approves on that computer (or denies).
//!
//! The desktop's Remote machines dialog drives it with `/api/pairComputer`,
//! `/api/computerPairingStatus` and `/api/cancelComputerPairing`.
//! SEE-ALSO: server/src/remote_access/pair_device.rs (`PairingDeviceKind::Computer`).

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use ghostex_gx_protocol::pairing::{pairing_status_payload, DEVICE_KEY_ALGORITHM};
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
use serde_json::{json, Value};

use crate::ghostex_cli::tailcat_tunnel::{start_tailcat_tunnel, TailcatForwardPlan, TailcatTunnel};
use crate::paths::GxserverPaths;

use super::pair_device::match_words;

const DEVICE_KEY_FILE: &str = "computer-device-key.pkcs8";
/// tailcat's own name for the client key every client mode loads.
const TAILCAT_CLIENT_KEY_NAME: &str = "client-default";
const ASK_TIMEOUT: Duration = Duration::from_secs(30);
/// Approving can wait on the other computer's Windows administrator prompt.
const STATUS_TIMEOUT: Duration = Duration::from_secs(180);

/// One computer pairing in flight: the tunnel stays open until it ends.
struct OutgoingPairing {
    tunnel: TailcatTunnel,
    local_port: u16,
    key_pkcs8: Vec<u8>,
    last_timestamp_ms: u64,
    computer_name: String,
    /// The other computer's Easy Connect address, recorded once it approves.
    address: String,
}

fn outgoing() -> &'static Mutex<HashMap<String, OutgoingPairing>> {
    static OUTGOING: OnceLock<Mutex<HashMap<String, OutgoingPairing>>> = OnceLock::new();
    OUTGOING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The Easy Connect code fields the dialog read (`address`, `port`, `secret`, `name`).
#[derive(Clone, Debug)]
pub struct ComputerPairingCode {
    pub address: String,
    pub port: u16,
    pub secret: String,
    pub name: String,
}

pub fn parse_computer_pairing_code(params: &Value) -> Result<ComputerPairingCode> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let code = ComputerPairingCode {
        address: text("address"),
        port: params
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(0),
        secret: text("secret"),
        name: text("name"),
    };
    if code.address.is_empty() || code.port == 0 {
        bail!("The Easy Connect code has no address or port.");
    }
    if code.secret.is_empty() {
        bail!("This code can't pair a computer by itself. On the other computer, open Settings > Remote, click Show QR, then Copy Easy Connect code, and paste that code here.");
    }
    Ok(code)
}

/// Reads tailcat's `client-default` public key, making the key first when it doesn't exist.
pub fn ensure_tailcat_client_key(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .args([
            "genkey",
            "--client",
            &format!("--key={TAILCAT_CLIENT_KEY_NAME}"),
        ])
        .output()
        .with_context(|| format!("run {} genkey", binary.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    read_tailcat_client_key(&stdout, &stderr, |path| std::fs::read_to_string(path).ok())
}

/// The public key from `tailcat genkey --client`: printed when it made the key; read from the key
/// file named in its "already exists" refusal otherwise.
pub(crate) fn read_tailcat_client_key(
    stdout: &str,
    stderr: &str,
    read_file: impl Fn(&Path) -> Option<String>,
) -> Result<String> {
    if let Some(key) = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| line.starts_with("nodekey:"))
    {
        return Ok(key.to_string());
    }
    let path = stderr
        .lines()
        .chain(stdout.lines())
        .find_map(|line| {
            let (before, _) = line.split_once(" already exists")?;
            Some(PathBuf::from(strip_go_log_time(before.trim())))
        })
        .ok_or_else(|| anyhow!("tailcat didn't print this computer's client key: {stderr}"))?;
    let text = read_file(&path).ok_or_else(|| anyhow!("can't read {}", path.display()))?;
    let json: Value = serde_json::from_str(&text).context("read the tailcat client key file")?;
    json.pointer("/Public/ServerPublic")
        .and_then(Value::as_str)
        .filter(|key| key.starts_with("nodekey:"))
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{} holds no public key", path.display()))
}

/// Go's `log` prefix, `2026/11/08 05:59:05 `, before the path tailcat names (which can hold spaces,
/// "Application Support" on macOS).
fn strip_go_log_time(line: &str) -> &str {
    let bytes = line.as_bytes();
    let stamped = bytes.len() > 20
        && bytes[4] == b'/'
        && bytes[7] == b'/'
        && bytes[10] == b' '
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b' ';
    if stamped {
        &line[20..]
    } else {
        line
    }
}

/// This computer's pairing device key (PKCS#8), made once and kept 0600 in gxserver's state.
pub fn load_or_create_device_key(paths: &GxserverPaths) -> Result<Vec<u8>> {
    let path = paths.root_dir.join("pairing").join(DEVICE_KEY_FILE);
    if let Ok(bytes) = std::fs::read(&path) {
        if EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            &bytes,
            &SystemRandom::new(),
        )
        .is_ok()
        {
            return Ok(bytes);
        }
    }
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
        .map_err(|_| anyhow!("could not make a device key"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    write_private_file(&path, pkcs8.as_ref())?;
    Ok(pkcs8.as_ref().to_vec())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("write {}", path.display()))?;
        file.write_all(bytes)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        // The state folder is the user's own profile on Windows.
        std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
    }
}

fn key_pair(pkcs8: &[u8]) -> Result<EcdsaKeyPair> {
    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8, &SystemRandom::new())
        .map_err(|_| anyhow!("this computer's device key can't be read"))
}

pub fn device_public_key(pkcs8: &[u8]) -> Result<String> {
    Ok(STANDARD.encode(key_pair(pkcs8)?.public_key().as_ref()))
}

pub fn device_sign(pkcs8: &[u8], payload: &[u8]) -> Result<String> {
    let signature = key_pair(pkcs8)?
        .sign(&SystemRandom::new(), payload)
        .map_err(|_| anyhow!("this computer's device key could not sign"))?;
    Ok(STANDARD.encode(signature.as_ref()))
}

fn platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// Posts a gxserver RPC through the tunnel; the result, or the error's message.
fn post(local_port: u16, path: &str, params: Value, timeout: Duration) -> Result<Value> {
    let body = json!({
        "requestId": format!("pair-computer-{}", uuid::Uuid::new_v4().simple()),
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
        "params": params,
    });
    let response = crate::http_agent::answering_agent()
        .post(format!("http://127.0.0.1:{local_port}{path}"))
        .header("content-type", "application/json")
        .config()
        .timeout_global(Some(timeout))
        .build()
        .send(body.to_string().as_str())
        .map_err(|error| {
            anyhow!("Reached the other computer, but its Ghostex did not answer ({error}). Keep its Show QR window open and try again.")
        })?;
    let answer: Value = crate::http_agent::read_json(response)
        .context("the other computer answered with no JSON")?;
    if answer.get("ok") == Some(&Value::Bool(false)) {
        let message = answer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The other computer refused the pairing.");
        bail!("{message}");
    }
    Ok(answer.get("result").cloned().unwrap_or(answer))
}

fn free_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("find a free local port")?;
    Ok(listener.local_addr()?.port())
}

/// `/api/pairComputer`: starts pairing this computer with the one behind `code`; answers
/// `{ pendingId, words, computerName }` once that computer shows it as Pending.
pub fn start_computer_pairing(paths: &GxserverPaths, code: ComputerPairingCode) -> Result<Value> {
    let binary = crate::tailcat::resolve_client_tailcat_binary().ok_or_else(|| {
        anyhow!("Install the Easy Connect helper first (Settings > Remote > Easy Connect).")
    })?;
    let tailcat_client_key = ensure_tailcat_client_key(&binary)?;
    let key_pkcs8 = load_or_create_device_key(paths)?;
    let public_key = device_public_key(&key_pkcs8)?;
    let local_port = free_local_port()?;
    let plan = TailcatForwardPlan {
        address_blob: code.address.clone(),
        base_url: format!("http://127.0.0.1:{local_port}"),
        local_port,
        remote_port: code.port,
    };
    let tunnel = start_tailcat_tunnel(&binary, &plan).map_err(|error| anyhow!("{error}"))?;
    let identity = super::probe_cache::cached_remote_access_identity()?;
    let started = post(
        local_port,
        "/api/pairDevice",
        json!({
            "kind": "computer",
            "secret": code.secret,
            "deviceName": identity.computer_name,
            "platform": platform_name(),
            "tailcatClientKey": tailcat_client_key,
            "deviceKey": { "algorithm": DEVICE_KEY_ALGORITHM, "publicKey": public_key },
        }),
        ASK_TIMEOUT,
    );
    let started = match started {
        Ok(started) => started,
        Err(error) => {
            tunnel.stop();
            return Err(error);
        }
    };
    let (Some(pending_id), Some(words)) = (
        started.get("pendingId").and_then(Value::as_str),
        started.get("words").and_then(Value::as_array),
    ) else {
        tunnel.stop();
        bail!("Update Ghostex on the other computer to 11.1.0 or later, then try again.");
    };
    let words: Vec<String> = words
        .iter()
        .filter_map(|word| word.as_str().map(str::to_string))
        .collect();
    // The words are worked out here too: a computer that names others isn't the one in the code.
    if words != match_words(&code.secret, &public_key) {
        tunnel.stop();
        bail!("The other computer showed different words from this one, so pairing stopped. Show a new code there and try again.");
    }
    let computer_name = started
        .get("computerName")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(&code.name)
        .to_string();
    outgoing()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            pending_id.to_string(),
            OutgoingPairing {
                tunnel,
                local_port,
                key_pkcs8,
                last_timestamp_ms: 0,
                computer_name: computer_name.clone(),
                address: code.address.clone(),
            },
        );
    Ok(json!({ "pendingId": pending_id, "words": words, "computerName": computer_name }))
}

/// `/api/computerPairingStatus`: asks the other computer, signed, where the pairing stands;
/// `{ status: "pending" | "approved" | "denied" | "expired", computerName }`. The tunnel closes
/// once it is over.
pub fn computer_pairing_status(paths: &GxserverPaths, pending_id: &str) -> Result<Value> {
    let (local_port, key_pkcs8, timestamp_ms, computer_name, address) = {
        let mut map = outgoing()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(pairing) = map.get_mut(pending_id) else {
            return Ok(json!({ "status": "expired" }));
        };
        let timestamp_ms = crate::security::now_ms().max(pairing.last_timestamp_ms + 1);
        pairing.last_timestamp_ms = timestamp_ms;
        (
            pairing.local_port,
            pairing.key_pkcs8.clone(),
            timestamp_ms,
            pairing.computer_name.clone(),
            pairing.address.clone(),
        )
    };
    let signature = device_sign(
        &key_pkcs8,
        &pairing_status_payload(pending_id, timestamp_ms),
    )?;
    let answer = post(
        local_port,
        "/api/pairingStatus",
        json!({ "pendingId": pending_id, "timestampMs": timestamp_ms, "signature": signature }),
        STATUS_TIMEOUT,
    );
    let status = match &answer {
        Ok(answer) => answer
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("expired")
            .to_string(),
        Err(_) => "failed".to_string(),
    };
    if status == "approved" {
        record_paired_computer(paths, &address, &computer_name);
    }
    if status != "pending" {
        cancel_computer_pairing(pending_id);
    }
    let answer = answer?;
    Ok(json!({
        "status": status,
        "computerName": computer_name,
        "deviceId": answer.get("deviceId"),
    }))
}

/// The computers this one paired with through Easy Connect (pairing v2), kept in
/// `<gxserver state>/security/computer-pairings.json`.
const PAIRED_COMPUTERS_FILE: &str = "computer-pairings.json";

/// One computer this one paired with: its Easy Connect address, its name, and when (ms).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedComputer {
    pub address: String,
    pub computer_name: String,
    pub paired_at_ms: i64,
}

/*
CDXC:Security 2026-11-08 FIX:
User (GitHub #495, 11.1.1): "a re-paired computer must count as paired: decide 'paired again' from
the pairing itself ..., not from an address change". Pairing again with a computer keeps its Easy
Connect address (the address follows that computer's own key), so the pair-again notice, which
looked for a new address, showed it as Needs pairing again. This computer now records each pairing
the other computer approved, by that address, and the notice counts the entry as paired again
when a pairing with its address was recorded after the update.
*/
fn paired_computers_path(paths: &GxserverPaths) -> std::path::PathBuf {
    paths.root_dir.join("security").join(PAIRED_COMPUTERS_FILE)
}

/// The pairings recorded so far (an unreadable file reads as none).
pub fn read_paired_computers(paths: &GxserverPaths) -> Vec<PairedComputer> {
    std::fs::read_to_string(paired_computers_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Records that the computer at `address` approved this one, replacing an older record for it.
fn record_paired_computer(paths: &GxserverPaths, address: &str, computer_name: &str) {
    let mut paired: Vec<PairedComputer> = read_paired_computers(paths)
        .into_iter()
        .filter(|entry| entry.address != address)
        .collect();
    paired.push(PairedComputer {
        address: address.to_string(),
        computer_name: computer_name.to_string(),
        paired_at_ms: chrono::Utc::now().timestamp_millis(),
    });
    let path = paired_computers_path(paths);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &path,
        serde_json::to_vec_pretty(&paired).unwrap_or_default(),
    );
}

/// `/api/cancelComputerPairing`: stops waiting and closes the tunnel (the other computer's request
/// lapses by itself).
pub fn cancel_computer_pairing(pending_id: &str) {
    if let Some(pairing) = outgoing()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(pending_id)
    {
        pairing.tunnel.stop();
    }
}
