//! Server links (GitHub #674): two computers' gxservers keep the link their agents use between
//! them themselves, with no desktop app holding it. This file is the store both sides keep and
//! the door side: what this computer pinned for each computer it let link here, and its answer to
//! that computer's signed asks (`/api/serverLink`). The dialing side is `server_link_dialer.rs`.
//!
//! CDXC:ServerLinks 2026-11-19 DECISION:
//! User (issue #674, "make sure to build in a way that doesn't open any new security holes in our
//! pairing and connecting flow"), with their answers 1A (both ways, a second approval card for the
//! other server's door), 1A on Fable 5.2's review (no SSH key: the Easy Connect tunnel and the
//! agents' grant only), 2A (all of Fable 5.2's tighter rules), 3A (removing on one server ends both
//! directions) and 4B (a Paired computers list in Settings > Remote). How it works:
//! - `ghostex computers add <code>` on B pairs B with A like any computer (`/api/pairDevice`, three
//!   words, A's owner approves on the app or a phone), with `serverLink: { serverId }`. The owner
//!   approves the link and its server id together (they are in the approval's params).
//! - A pins B's server id to B's paired-device row only when one of the owner's devices approved
//!   (never the token file, which an agent can read, `decided_by_owner_device`), and refuses an id
//!   that is its own, pinned to another key, or connected here through the app.
//! - B dials A over Easy Connect with a key of its own and asks `/api/serverLink` `grant` every
//!   minute, signed with its pairing device key over A's server id, its own, the time and whether
//!   to rotate. A answers with the same `gxpeer_` grant the app's link gets (`issue_grant`: the
//!   agent allowlist, 180 s), never more.
//! - Then B asks A to pair back (`connectBack`) with a code that only A's device key can spend;
//!   B's owner approves that second card.
//! - Removing either side's row, or `ghostex computers remove`, drops the pin, the grant and this
//!   side's own link, and tells the other side (`unlink`, signed), which drops only the row whose
//!   key signed it. Agent access off is kept on disk (`access_off`).
//! Why a signature, not a token: the asking server holds no token here (a computer never gets
//! one), like `/api/pairingStatus`, `/api/renewDeviceToken` and `/api/registerComputerEasyConnectKeys`,
//! the three routes this follows; the receiver's server id is signed so one linked server can't
//! replay another's ask. Same-user limit (for the reviewer): any program running as the user on B
//! can read B's pairing device key (0600 in gxserver's state), as it can today for B's computer
//! SSH key, so it can ask A for B's grant; removing B on A cuts it off.
//! SEE-ALSO: server/src/remote_access/server_link_dialer.rs, server/src/connected_computers/registry.rs,
//! packages/gx-protocol/src/pairing.rs (`server_link_payload`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use ghostex_gx_protocol::pairing::server_link_payload;

use crate::paths::GxserverPaths;
use crate::tailcat::TailcatRuntime;

use super::pair_device::verify_device_signature;
use super::paired_devices::remove_paired_device;
use crate::auth::DeviceApiTokens;
use super::repository::{PairedDeviceStatus, RemotePairedDeviceRecord, RemotePairingRepository};

pub const SERVER_LINK_ENDPOINT: &str = "/api/serverLink";
/// This computer's own clients (CLI, desktop) list, add and remove server links here.
pub const SERVER_LINKS_ENDPOINT: &str = "/api/serverLinks";

const FILE_NAME: &str = "server-links.json";
/// How far an ask's time may be from this computer's clock.
const CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
/// Asks `/api/serverLink` takes a minute, from everyone together (a linked server asks about once a
/// minute; this is its own bucket, so asks never use up the pairing attempts).
const ASKS_PER_MINUTE: usize = 120;

/// What a refusal says to the asking server. `notPaired` ends its link (only from inside the
/// tunnel to the address it recorded); the others never do.
pub const NOT_PAIRED: &str = "serverLinkNotPaired";
pub const WRONG_COMPUTER: &str = "serverLinkWrongComputer";
pub const CLOCK_OFF: &str = "serverLinkClockOff";
pub const RATE_LIMITED: &str = "serverLinkRateLimited";

/// A computer this one let link here: pinned to its paired-device row (by id) and its device key.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoorPin {
    pub server_id: String,
    pub name: String,
    pub device_public_key: String,
    pub pinned_at_ms: u64,
}

/// Where this side's own link to a computer stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkState {
    /// Waiting for that computer's owner to approve.
    Pairing,
    /// Approved: the dialer keeps it up.
    Linked,
    /// It ended (denied, lapsed, refused): kept so `ghostex computers add` can say why, then dropped.
    Ended,
}

/// The other computer pairing back to this one (`connectBack`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseAsk {
    #[serde(default)]
    pub words: Vec<String>,
    #[serde(default)]
    pub asked_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// This side's own link to a computer (the dialing side).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingLink {
    /// Its Easy Connect address, from the code: the only address this link ever dials.
    pub address: String,
    pub gxserver_port: u16,
    pub name: String,
    /// The Easy Connect client key this link pairs and dials with (gxserver's own, never tailcat's
    /// shared `client-default`).
    pub key_file: PathBuf,
    /// Its server id, pinned from its answer to the pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// Its pairing device key, from the same answer: its `unlink` must be signed with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_device_key: Option<String>,
    pub state: LinkState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_id: Option<String>,
    #[serde(default)]
    pub words: Vec<String>,
    /// Started by `ghostex computers add` here: once linked, ask that computer to pair back.
    #[serde(default)]
    pub connect_back: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse: Option<ReverseAsk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinksFile {
    /// By paired-device id.
    #[serde(default)]
    pub door: BTreeMap<String, DoorPin>,
    #[serde(default)]
    pub outgoing: Vec<OutgoingLink>,
    /// Server ids whose Agent access this computer turned off (kept on disk: a headless server has
    /// no app to keep renewing an in-memory deny).
    #[serde(default)]
    pub access_off: BTreeSet<String>,
}

impl LinksFile {
    pub fn pin_for_server(&self, server_id: &str) -> Option<(&String, &DoorPin)> {
        self.door.iter().find(|(_, pin)| pin.server_id == server_id)
    }

    pub fn outgoing_for_server(&self, server_id: &str) -> Option<&OutgoingLink> {
        self.outgoing
            .iter()
            .find(|link| link.server_id.as_deref() == Some(server_id))
    }
}

fn file_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

fn file_path(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("security").join(FILE_NAME)
}

/// The links this computer keeps (an unreadable file reads as none).
pub fn read(paths: &GxserverPaths) -> LinksFile {
    let _guard = file_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read_unlocked(paths)
}

fn read_unlocked(paths: &GxserverPaths) -> LinksFile {
    std::fs::read_to_string(file_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Changes the file under its lock and writes it back (0600); returns what `change` returned.
pub fn update<T>(paths: &GxserverPaths, change: impl FnOnce(&mut LinksFile) -> T) -> T {
    let _guard = file_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = read_unlocked(paths);
    let answer = change(&mut file);
    write_private(
        &file_path(paths),
        &serde_json::to_vec_pretty(&file).unwrap_or_default(),
    );
    answer
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let temp = path.with_extension("json.tmp");
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .and_then(|mut file| file.write_all(bytes));
        if written.is_ok() {
            let _ = std::fs::rename(&temp, path);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::write(path, bytes);
    }
}

/// Whether a server link names `server_id` here, either way: then only this computer's own
/// clients (never an agent's session token) may mint, register or drop a grant for it through
/// the connected-computer routes (Fable 5.2's review: an agent could otherwise take that
/// computer's grant slot, rotate it or lift its deny).
pub fn is_linked_server_id(paths: &GxserverPaths, server_id: &str) -> bool {
    let file = read(paths);
    file.pin_for_server(server_id).is_some() || file.outgoing_for_server(server_id).is_some()
}

/// Whether Agent access for `server_id` is turned off here (kept on disk).
pub fn access_off(paths: &GxserverPaths, server_id: &str) -> bool {
    read(paths).access_off.contains(server_id)
}

/// Whether an approval was decided on one of the owner's devices: the app's owner key or a paired
/// phone record their device id; every other caller records its kind (`approve_plain_as`).
pub fn decided_by_owner_device(decided_by: &str) -> bool {
    !matches!(
        decided_by,
        "" | "owner"
            | "device"
            | "session"
            | "localAgent"
            | "peer"
            | "guest"
            | "extension"
            | "anonymous"
            | "policy"
    )
}

/// What `/api/pairDevice` offers a computer asking for a server link.
pub struct DoorLinkOffer {
    /// What this computer already calls that server id, for the card.
    pub known_as: Option<String>,
    /// `{ serverLink: true, serverId, deviceKey }`: this computer's id and pairing device key,
    /// which the asking computer pins.
    pub answer: Value,
}

/// Checks a computer's server link ask once its code is spent: refused for this computer's own
/// id, an id another device key holds, or one connected here through the app.
pub fn door_link_offer(
    paths: &GxserverPaths,
    db: &Connection,
    own_server_id: &str,
    server_id: &str,
    device_public_key: &str,
) -> Result<DoorLinkOffer, String> {
    if server_id == own_server_id {
        return Err("A computer can't link to itself.".to_string());
    }
    let live_devices = live_device_ids(db);
    let file = read(paths);
    if file.door.iter().any(|(device_id, pin)| {
        pin.server_id == server_id
            && pin.device_public_key != device_public_key
            && live_devices.contains(device_id)
    }) {
        return Err(
            "Another computer is already linked here under that server id. Remove it in Settings › Remote first."
                .to_string(),
        );
    }
    let linked_here =
        file.pin_for_server(server_id).is_some() || file.outgoing_for_server(server_id).is_some();
    if !linked_here
        && (crate::connected_computers::find(server_id).is_some()
            || crate::connected_computers::has_grant(server_id))
    {
        return Err(
            "That computer is already connected here through the Ghostex app, so it can't link as a server too."
                .to_string(),
        );
    }
    let known_as = crate::connected_computers::read_known_computers(&paths.app_state_dir)
        .into_iter()
        .find(|(id, _, _)| id == server_id)
        .map(|(_, name, _)| name);
    let device_key = super::computer_pairing::load_or_create_device_key(paths)
        .and_then(|key| super::computer_pairing::device_public_key(&key))
        .map_err(|error| format!("{error:#}"))?;
    Ok(DoorLinkOffer {
        known_as,
        answer: json!({
            "serverLink": true,
            "serverId": own_server_id,
            "deviceKey": device_key,
        }),
    })
}

fn live_device_ids(db: &Connection) -> BTreeSet<String> {
    RemotePairingRepository::new(db)
        .list_devices()
        .map(|devices| {
            devices
                .into_iter()
                .filter(|device| device.status == PairedDeviceStatus::Paired)
                .map(|device| device.id)
                .collect()
        })
        .unwrap_or_default()
}

/// A finished pairing that asked for a server link: pins it when `allowed` (approved on an owner
/// device, not a tunnel-only join), and says so in the pairing's answer either way.
pub fn settle_door_link(
    paths: &GxserverPaths,
    answer: &mut Value,
    server_id: &str,
    pending: &super::pairing_code::PendingPairing,
    allowed: bool,
) {
    let device_id = answer
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !allowed || device_id.is_empty() {
        answer["serverLink"] = json!("notLinked");
        answer["serverLinkReason"] = json!(
            "It was approved in a terminal, not in the Ghostex app or on your phone, so the computers are paired without a server link. Approve it in the app or on your phone to link them."
        );
        return;
    }
    let key = pending.device_key.public_key.clone();
    let name = pending.name.clone();
    update(paths, |file| {
        // A computer pairing again (same key) moves its pin to its new row.
        file.door.retain(|_, pin| pin.device_public_key != key);
        file.door.insert(
            device_id,
            DoorPin {
                server_id: server_id.to_string(),
                name,
                device_public_key: key,
                pinned_at_ms: crate::security::now_ms(),
            },
        );
    });
    answer["serverLink"] = json!("linked");
    crate::audit::record(
        crate::audit::Action::new(
            "computers.serverLink",
            format!("Let the agents on {} link with this computer", pending.name),
        )
        .target(pending.name.clone()),
        crate::audit::Actor::current(),
        crate::audit::Outcome::Done,
        None,
    );
}

/// A paired-device row was removed here (Settings, a phone, the bell, `computers remove`, or the
/// other computer's `unlink`): its pin, its grant and this side's own link to it go too (the
/// user's 3A), and the other computer hears it when `tell` (best effort, over this side's own
/// tunnel, signed).
pub fn on_device_removed(paths: &GxserverPaths, device: &RemotePairedDeviceRecord, tell: bool) {
    let key = device
        .device_key
        .as_ref()
        .map(|key| key.public_key.clone())
        .unwrap_or_default();
    let (pin, outgoing) = update(paths, |file| {
        let pin = file.door.remove(&device.id);
        let mut outgoing = Vec::new();
        file.outgoing.retain(|link| {
            let theirs = !key.is_empty() && link.peer_device_key.as_deref() == Some(key.as_str());
            if theirs {
                outgoing.push(link.clone());
            }
            !theirs
        });
        if let Some(pin) = &pin {
            file.access_off.remove(&pin.server_id);
        }
        (pin, outgoing)
    });
    let mut server_ids: BTreeSet<String> = BTreeSet::new();
    if let Some(pin) = &pin {
        server_ids.insert(pin.server_id.clone());
    }
    for link in &outgoing {
        if let Some(id) = &link.server_id {
            server_ids.insert(id.clone());
        }
    }
    for server_id in &server_ids {
        crate::connected_computers::revoke_grant(server_id, false, Duration::from_secs(10));
        crate::connected_computers::unregister(server_id);
    }
    for link in outgoing {
        super::server_link_dialer::forget_link(paths, &link, tell);
    }
}

/// Drops pins whose paired-device row is gone (a pairing replaced or removed by another route).
pub fn prune_pins(paths: &GxserverPaths, db: &Connection) {
    let live = live_device_ids(db);
    let stale = read(paths)
        .door
        .keys()
        .any(|device_id| !live.contains(device_id));
    if stale {
        update(paths, |file| file.door.retain(|id, _| live.contains(id)));
    }
}

/// `/api/serverLink`'s params.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerLinkAsk {
    pub op: String,
    pub device_key: String,
    pub to_server_id: String,
    pub from_server_id: String,
    #[serde(default)]
    pub name: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub rotate: bool,
    pub signature: String,
    /// `connectBack`: the asking computer's code.
    #[serde(default)]
    pub code: Option<ConnectBackCode>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectBackCode {
    pub address: String,
    pub port: u16,
    pub secret: String,
    pub name: String,
}

impl ServerLinkAsk {
    /// The extra lines the signature covers for this op.
    pub fn extra(&self) -> Vec<(String, String)> {
        let mut extra = vec![("name".to_string(), self.name.clone())];
        if let Some(code) = &self.code {
            extra.push(("address".to_string(), code.address.clone()));
            extra.push(("port".to_string(), code.port.to_string()));
            extra.push(("codeName".to_string(), code.name.clone()));
            extra.push((
                "secretSha256".to_string(),
                hex::encode(Sha256::digest(code.secret.as_bytes())),
            ));
        }
        extra
    }

    pub fn payload(&self) -> Vec<u8> {
        server_link_payload(
            &self.op,
            &self.to_server_id,
            &self.from_server_id,
            &self.device_key,
            self.timestamp_ms,
            self.rotate,
            &self.extra(),
        )
    }
}

/// A refusal: its code, its words and its HTTP status.
#[derive(Debug)]
pub struct Refusal {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

fn refusal(status: u16, code: &'static str, message: impl Into<String>) -> Refusal {
    Refusal {
        status,
        code,
        message: message.into(),
    }
}

fn take_ask_slot() -> bool {
    static ASKS: Mutex<Vec<Instant>> = Mutex::new(Vec::new());
    let mut asks = ASKS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    asks.retain(|at| now.duration_since(*at) < Duration::from_secs(60));
    if asks.len() >= ASKS_PER_MINUTE {
        return false;
    }
    asks.push(now);
    true
}

fn last_timestamps() -> &'static Mutex<HashMap<String, u64>> {
    static LAST: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What the door side needs to answer an ask.
pub struct DoorContext<'a> {
    pub paths: &'a GxserverPaths,
    pub db: &'a Connection,
    pub tailcat_runtime: &'a TailcatRuntime,
    pub device_tokens: &'a DeviceApiTokens,
    pub own_server_id: &'a str,
}

/// `/api/serverLink`: a linked computer's signed ask. The signature is checked before anything
/// else is done for it, and an unknown key and a bad signature hear the same refusal.
pub fn answer_ask(context: &DoorContext<'_>, ask: ServerLinkAsk) -> Result<Value, Refusal> {
    if !take_ask_slot() {
        return Err(refusal(
            429,
            RATE_LIMITED,
            "Too many server link asks here just now. Try again in a minute.",
        ));
    }
    let not_paired = || {
        refusal(
            401,
            NOT_PAIRED,
            "This computer has no server link with the computer that asked.",
        )
    };
    let device = RemotePairingRepository::new(context.db)
        .list_devices()
        .map_err(|error| refusal(500, "internal", format!("{error:#}")))?
        .into_iter()
        .find(|device| {
            device.status == PairedDeviceStatus::Paired
                && device.api_token_hash.is_none()
                && device.tailcat_client_key.is_some()
                && device.device_key.as_ref().is_some_and(|key| {
                    !ask.device_key.is_empty() && key.public_key == ask.device_key
                })
        });
    let pin = device.as_ref().and_then(|device| {
        read(context.paths)
            .door
            .get(&device.id)
            .filter(|pin| pin.device_public_key == ask.device_key)
            .cloned()
    });
    let (Some(device), Some(pin)) = (device, pin) else {
        return Err(not_paired());
    };
    if !verify_device_signature(&ask.device_key, &ask.payload(), &ask.signature) {
        return Err(not_paired());
    }
    // Signed for another computer (replayed from one it is also linked with), or not by the
    // server this key was pinned to: refused, without ending anything.
    if ask.to_server_id != context.own_server_id || ask.from_server_id != pin.server_id {
        return Err(refusal(
            403,
            WRONG_COMPUTER,
            "This ask was signed for another computer.",
        ));
    }
    let now = crate::security::now_ms();
    if ask.timestamp_ms + CLOCK_SKEW_MS < now || ask.timestamp_ms > now + CLOCK_SKEW_MS {
        return Err(refusal(
            400,
            CLOCK_OFF,
            "The asking computer's clock is more than five minutes off this one's.",
        ));
    }
    {
        let mut last = last_timestamps()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let seen = last.entry(ask.device_key.clone()).or_insert(0);
        if ask.timestamp_ms <= *seen {
            return Err(refusal(400, CLOCK_OFF, "This ask was already answered."));
        }
        *seen = ask.timestamp_ms;
    }
    let this = crate::connected_computers::this_computer_name();
    match ask.op.as_str() {
        "grant" => {
            if access_off(context.paths, &pin.server_id) {
                return Ok(
                    json!({ "agentAccess": false, "serverId": context.own_server_id, "name": this }),
                );
            }
            let had_grant = crate::connected_computers::has_grant(&pin.server_id);
            match crate::connected_computers::issue_grant(
                &pin.server_id,
                &pin.name,
                true,
                ask.rotate,
                crate::connected_computers::DEFAULT_REGISTRATION_TTL,
                false,
            ) {
                Ok(Some(token)) => {
                    if !had_grant {
                        crate::audit::record(
                            crate::audit::Action::new(
                                "computers.grant",
                                format!(
                                    "Let agents on {} use this computer (server link)",
                                    pin.name
                                ),
                            )
                            .target(pin.name.clone()),
                            crate::audit::Actor::PairedDevice {
                                id: Some(device.id.clone()),
                                name: device.name.clone(),
                            },
                            crate::audit::Outcome::Done,
                            None,
                        );
                    }
                    Ok(json!({
                        "agentAccess": true,
                        "peerToken": token,
                        "serverId": context.own_server_id,
                        "name": this,
                    }))
                }
                // This computer's own app turned Agent access off for it (in memory).
                Ok(None) | Err(_) => Ok(
                    json!({ "agentAccess": false, "serverId": context.own_server_id, "name": this }),
                ),
            }
        }
        "connectBack" => {
            let Some(code) = ask.code.clone() else {
                return Err(refusal(
                    400,
                    "badRequest",
                    "connectBack needs the asking computer's code.",
                ));
            };
            if code.address.trim().is_empty() || code.port == 0 || code.secret.trim().is_empty() {
                return Err(refusal(
                    400,
                    "badRequest",
                    "The asking computer's code is missing its address or secret.",
                ));
            }
            if code.name.chars().count() > 80 || code.name.chars().any(char::is_control) {
                return Err(refusal(
                    400,
                    "badRequest",
                    "The asking computer's name is not valid.",
                ));
            }
            // Already linked to it from here: nothing to pair.
            if read(context.paths).outgoing.iter().any(|link| {
                link.peer_device_key.as_deref() == Some(ask.device_key.as_str())
                    && link.state != LinkState::Ended
            }) {
                return Ok(json!({ "alreadyLinked": true }));
            }
            super::server_link_dialer::start_link_pairing(
                context.paths,
                context.own_server_id,
                super::computer_pairing::ComputerPairingCode {
                    address: code.address,
                    port: code.port,
                    secret: code.secret,
                    name: code.name,
                },
                false,
                Some(ask.device_key.clone()),
            )
            .map_err(|error| refusal(400, "badRequest", format!("{error:#}")))
        }
        "unlink" => {
            if let Some(removed) = remove_paired_device(
                context.paths,
                context.db,
                context.tailcat_runtime,
                context.device_tokens,
                &device.id,
            )
            .map_err(|error| refusal(500, "internal", format!("{error:#}")))?
            {
                on_device_removed(context.paths, &removed, false);
                crate::audit::record(
                    crate::audit::Action::new(
                        "pairing.remove",
                        "A linked computer ended its server link with this computer",
                    )
                    .target(removed.name.clone()),
                    crate::audit::Actor::PairedDevice {
                        id: Some(removed.id.clone()),
                        name: removed.name.clone(),
                    },
                    crate::audit::Outcome::Done,
                    None,
                );
            }
            Ok(json!({ "unlinked": true }))
        }
        _ => Err(refusal(400, "badRequest", "Unknown server link op.")),
    }
}
