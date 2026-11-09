//! This computer's own Easy Connect keys for each computer it paired with (the asking side).
//!
//! CDXC:RemotePairing 2026-11-10 DECISION:
//! User (Q2106: "must not break older iPhone versions and must not require re-pairing"): a computer
//! that is already paired gets keys of its own for each computer it reaches over Easy Connect, on
//! its own, with nothing to approve or click. Every tailcat client here dialed with tailcat's one
//! `client-default` key, and a relay delivers a key's traffic only to that key's newest connection,
//! so the desktop app's forward and gxserver's tunnel to one computer, or the tunnels to two
//! computers on one relay, starved each other (GitHub #526; measured 2026-11-10). On first need this
//! makes one key per purpose (`app` for the desktop app's forward, `server` for gxserver's tunnels),
//! registers both in one request signed with this computer's pairing device key
//! (`/api/registerComputerEasyConnectKeys`, one allow-list change there), and from then on those
//! clients dial with them. Until then, and for a computer whose Ghostex has no such route (11.2.1 and
//! older), they keep `client-default`, which stays on that computer's allow-list.
//! SEE-ALSO: server/src/remote_access/pair_device.rs (`register_computer_keys_signed`, the other
//! side), apps/desktop/src/app/helpers/remote/easy_connect_forward.rs, server/src/ghostex_cli/tailcat_tunnel.rs.

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::ghostex_cli::tailcat_tunnel::{start_tailcat_tunnel, TailcatForwardPlan};
use crate::paths::GxserverPaths;

use super::computer_pairing::{device_public_key, device_sign, read_tailcat_client_key};
use super::paired_devices::{COMPUTER_EASY_CONNECT_KEYS_ENDPOINT, COMPUTER_KEY_PURPOSES};

/// The desktop app asks for its key for a computer here (the local listener only).
pub const EASY_CONNECT_LINK_KEY_ENDPOINT: &str = "/api/easyConnectLinkKey";

/// The desktop app's forward to a computer.
pub const LINK_KEY_PURPOSE_APP: &str = "app";
/// gxserver's own tunnels to a computer (`--computer` commands, connected computers).
pub const LINK_KEY_PURPOSE_SERVER: &str = "server";

const REGISTRY_FILE: &str = "link-keys.json";
const KEYS_DIR: &str = "link-keys";
const DEVICE_KEY_FILE: &str = "computer-device-key.pkcs8";
/// A computer whose Ghostex has no such route (or doesn't know this computer's device key) is asked
/// again after this long: it may have been updated, or paired again.
const UNSUPPORTED_RETRY_MS: i64 = 24 * 60 * 60 * 1000;
/// A computer that could not be reached is asked again after this long.
const FAILED_RETRY_MS: i64 = 30 * 60 * 1000;
/// One registration attempt, per port tried: the tunnel's first rendezvous plus the answer.
const REGISTER_TIMEOUT: Duration = Duration::from_secs(15);
/// How a sent request whose answer never came reads (`register` asks again once).
const ANSWER_LOST: &str = "no answer";
/// How long after a lost answer the request is sent again: that computer's helper restart.
const ANSWER_LOST_RETRY_AFTER: Duration = Duration::from_secs(4);
/// The ports a computer's Easy Connect serves its gxserver on: its usual local API port, then
/// native Windows Ghostex's.
const GXSERVER_PORTS: [u16; 2] = [crate::constants::GXSERVER_LOCAL_API_PORT, 58_746];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkKeyEntry {
    /// `registered`, `unsupported` or `failed`.
    state: String,
    at_ms: i64,
    /// Purpose → key file, once registered.
    #[serde(default)]
    keys: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LinkKeyRegistry {
    /// Keyed by `computer_id` (the computer's Easy Connect address).
    #[serde(default)]
    computers: BTreeMap<String, LinkKeyEntry>,
}

fn registry_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

fn pairing_dir(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("pairing")
}

fn read_registry(paths: &GxserverPaths) -> LinkKeyRegistry {
    std::fs::read_to_string(pairing_dir(paths).join(REGISTRY_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_registry(paths: &GxserverPaths, registry: &LinkKeyRegistry) {
    let path = pairing_dir(paths).join(REGISTRY_FILE);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &path,
        serde_json::to_vec_pretty(registry).unwrap_or_default(),
    );
}

/// A short stable name for the computer behind `address`.
fn computer_id(address: &str) -> String {
    hex::encode(&Sha256::digest(address.trim().as_bytes())[..8])
}

/// The key file `purpose` dials the computer at `address` with, when this computer registered keys
/// of its own there; `None` means dial with `client-default`. Registers them the first time (and
/// again after a failure's wait), which takes up to a few seconds and restarts that computer's Easy
/// Connect helper once. `port_hint` is the computer's gxserver port when the caller knows it.
pub fn computer_link_key(
    paths: &GxserverPaths,
    address: &str,
    purpose: &str,
    port_hint: Option<u16>,
) -> Option<PathBuf> {
    if !COMPUTER_KEY_PURPOSES.contains(&purpose) || address.trim().is_empty() {
        return None;
    }
    // Only a computer this one paired with (pairing v2) knows its device key; anything else (a
    // Cloud Box, an address typed in by hand) is never asked.
    let paired = super::computer_pairing::read_paired_computers(paths)
        .into_iter()
        .find(|computer| computer.address.trim() == address.trim())?;
    let port_known = paired.gxserver_port.is_some();
    let port_hint = paired.gxserver_port.or(port_hint);
    // A computer that never paired with pairing v2 has no device key to sign with.
    let device_key = std::fs::read(pairing_dir(paths).join(DEVICE_KEY_FILE)).ok()?;
    let _guard = registry_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let id = computer_id(address);
    let mut registry = read_registry(paths);
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(entry) = registry.computers.get(&id) {
        match entry.state.as_str() {
            "registered" => {
                if let Some(path) = entry.keys.get(purpose).filter(|path| path.is_file()) {
                    return Some(path.clone());
                }
            }
            "unsupported" if now - entry.at_ms < UNSUPPORTED_RETRY_MS => return None,
            "failed" if now - entry.at_ms < FAILED_RETRY_MS => return None,
            _ => {}
        }
    }
    let outcome = register(paths, &device_key, &id, address, port_hint, port_known);
    let entry = match outcome {
        Ok(keys) => LinkKeyEntry {
            state: "registered".to_string(),
            at_ms: now,
            keys,
        },
        Err(Registration::Unsupported(reason)) => {
            eprintln!("[gxserver] Easy Connect keys for {id}: kept client-default ({reason})");
            LinkKeyEntry {
                state: "unsupported".to_string(),
                at_ms: now,
                keys: BTreeMap::new(),
            }
        }
        Err(Registration::Failed(reason)) => {
            eprintln!("[gxserver] Easy Connect keys for {id}: not registered yet ({reason})");
            LinkKeyEntry {
                state: "failed".to_string(),
                at_ms: now,
                keys: BTreeMap::new(),
            }
        }
    };
    let chosen = entry.keys.get(purpose).cloned();
    registry.computers.insert(id, entry);
    write_registry(paths, &registry);
    chosen
}

enum Registration {
    /// The computer answered but won't take them (no such route, or it doesn't know this device key).
    Unsupported(String),
    /// The computer could not be reached or the keys could not be made.
    Failed(String),
}

fn register(
    paths: &GxserverPaths,
    device_key: &[u8],
    id: &str,
    address: &str,
    port_hint: Option<u16>,
    port_known: bool,
) -> std::result::Result<BTreeMap<String, PathBuf>, Registration> {
    let binary = crate::tailcat::resolve_client_tailcat_binary()
        .ok_or_else(|| Registration::Failed("no Easy Connect helper".to_string()))?;
    let keys_dir = pairing_dir(paths).join(KEYS_DIR);
    std::fs::create_dir_all(&keys_dir).map_err(|error| Registration::Failed(error.to_string()))?;
    let mut files = BTreeMap::new();
    let mut public = Vec::new();
    for purpose in COMPUTER_KEY_PURPOSES {
        let file = keys_dir.join(format!("{id}-{purpose}.json"));
        let key = make_client_key(&binary, &file)
            .map_err(|error| Registration::Failed(format!("{error:#}")))?;
        public.push((purpose.to_string(), key));
        files.insert(purpose.to_string(), file);
    }
    let device_public = device_public_key(device_key)
        .map_err(|error| Registration::Failed(format!("{error:#}")))?;
    let signed = || -> std::result::Result<Value, Registration> {
        let timestamp_ms = crate::security::now_ms();
        let signature = device_sign(
            device_key,
            &ghostex_gx_protocol::pairing::easy_connect_client_keys_payload(
                &device_public,
                timestamp_ms,
                &public,
            ),
        )
        .map_err(|error| Registration::Failed(format!("{error:#}")))?;
        Ok(json!({
            "deviceKey": device_public,
            "timestampMs": timestamp_ms,
            "signature": signature,
            "keys": public.iter().cloned().collect::<BTreeMap<String, String>>(),
        }))
    };
    // A recorded port is the one; an older pairing record has none, so the usual ports are tried.
    let mut ports: Vec<u16> = port_hint.into_iter().collect();
    if !port_known {
        ports.extend(
            GXSERVER_PORTS
                .iter()
                .copied()
                .filter(|port| Some(*port) != port_hint),
        );
    }
    let mut last = Registration::Failed("no port answered".to_string());
    for port in ports {
        match post_signed(&binary, address, port, &signed()?) {
            Ok(()) => return Ok(files),
            Err(outcome @ Registration::Unsupported(_)) => return Err(outcome),
            Err(Registration::Failed(reason)) if reason.starts_with(ANSWER_LOST) => {
                /*
                The keys reached that computer and changed its allow-list, which restarts its Easy
                Connect helper and with it the tunnel the answer was on. Asking again once the
                helper is back changes nothing there (the keys are listed) and its answer gets
                through.
                */
                std::thread::sleep(ANSWER_LOST_RETRY_AFTER);
                match post_signed(&binary, address, port, &signed()?) {
                    Ok(()) => return Ok(files),
                    Err(outcome @ Registration::Unsupported(_)) => return Err(outcome),
                    Err(failed) => last = failed,
                }
            }
            Err(failed) => last = failed,
        }
    }
    Err(last)
}

/// `tailcat genkey --client --key=<file>`: the key's public half, made the first time.
fn make_client_key(binary: &Path, file: &Path) -> Result<String> {
    let output = Command::new(binary)
        .args(["genkey", "--client", &format!("--key={}", file.display())])
        .output()
        .with_context(|| format!("run {} genkey", binary.display()))?;
    read_tailcat_client_key(
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        |path| std::fs::read_to_string(path).ok(),
    )
}

/// Sends the signed keys to the computer's gxserver on `port`, through a tunnel on the key this
/// computer paired with.
fn post_signed(
    binary: &Path,
    address: &str,
    port: u16,
    params: &Value,
) -> std::result::Result<(), Registration> {
    let local_port = TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .map_err(|error| Registration::Failed(error.to_string()))?;
    let plan = TailcatForwardPlan {
        address_blob: address.to_string(),
        base_url: format!("http://127.0.0.1:{local_port}"),
        local_port,
        remote_port: port,
        client_key: None,
    };
    let tunnel = start_tailcat_tunnel(binary, &plan)
        .map_err(|error| Registration::Failed(error.to_string()))?;
    let body = json!({
        "requestId": format!("easy-connect-keys-{}", uuid::Uuid::new_v4().simple()),
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
        "params": params,
    });
    let sent = crate::http_agent::answering_agent()
        .post(format!(
            "http://127.0.0.1:{local_port}{COMPUTER_EASY_CONNECT_KEYS_ENDPOINT}"
        ))
        .header("content-type", "application/json")
        .header(
            "x-gxserver-protocol-version",
            crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(REGISTER_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .send(body.to_string().as_str());
    tunnel.stop();
    let response = sent.map_err(|error| Registration::Failed(format!("{ANSWER_LOST}: {error}")))?;
    let status = response.status().as_u16();
    let answer: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
    if answer.get("ok") == Some(&Value::Bool(true)) {
        return Ok(());
    }
    let message = answer
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("no answer")
        .to_string();
    // An older Ghostex has no such route (404); one that doesn't know this device key refuses the
    // signature (401). Either way it keeps `client-default` until asked again tomorrow.
    if matches!(status, 401 | 404 | 405) {
        Err(Registration::Unsupported(format!(
            "HTTP {status}: {message}"
        )))
    } else {
        Err(Registration::Failed(format!("HTTP {status}: {message}")))
    }
}

/// `/api/easyConnectLinkKey` for the desktop app: `{ keyPath }` (a file) or `{ keyPath: null }`.
pub fn easy_connect_link_key_answer(paths: &GxserverPaths, params: &Value) -> Result<Value> {
    let address = params
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("address is required."))?;
    let purpose = params
        .get("purpose")
        .and_then(Value::as_str)
        .unwrap_or(LINK_KEY_PURPOSE_APP);
    let port_hint = params
        .get("gxserverPort")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok());
    let path = computer_link_key(paths, address, purpose, port_hint);
    Ok(json!({ "keyPath": path.map(|path| path.display().to_string()) }))
}
