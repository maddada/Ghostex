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
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use ghostex_gx_protocol::pairing::server_link_payload;

use crate::paths::GxserverPaths;
use crate::tailcat::TailcatRuntime;

use super::pair_device::verify_device_signature;
use super::paired_devices::remove_paired_device;
use super::repository::{PairedDeviceStatus, RemotePairingRepository};
use crate::auth::DeviceApiTokens;

pub const SERVER_LINK_ENDPOINT: &str = "/api/serverLink";
/// This computer's own clients (CLI, desktop) list, add and remove server links here.
pub const SERVER_LINKS_ENDPOINT: &str = "/api/serverLinks";

const FILE_NAME: &str = "server-links.json";
/// How far an ask's time may be from this computer's clock.
const CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
/// Asks `/api/serverLink` takes a minute, from everyone together (a linked server asks about once a
/// minute; this is its own bucket, so asks never use up the pairing attempts).
const ASKS_PER_MINUTE: usize = 600;
/// What one linked computer may ask a minute (it asks about once a minute).
const ASKS_PER_KEY_PER_MINUTE: usize = 20;

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

/// Whether this computer keeps any server link (a pin or its own link): the dialer sweeps only
/// then (review of #674, finding 10).
static HAS_LINKS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn has_links() -> bool {
    HAS_LINKS.load(std::sync::atomic::Ordering::SeqCst)
}

fn note_links(file: &LinksFile) {
    HAS_LINKS.store(
        !file.door.is_empty() || !file.outgoing.is_empty(),
        std::sync::atomic::Ordering::SeqCst,
    );
}

/// The links this computer keeps. A missing file reads as none; an unreadable one reads as none
/// too, and is kept as it is (`update` won't write over it).
pub fn read(paths: &GxserverPaths) -> LinksFile {
    let _guard = file_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let file = read_unlocked(paths).unwrap_or_default();
    note_links(&file);
    file
}

/// `Err` when the file is there but can't be read or parsed.
fn read_unlocked(paths: &GxserverPaths) -> Result<LinksFile, String> {
    match std::fs::read_to_string(file_path(paths)) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("{error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LinksFile::default()),
        Err(error) => Err(format!("{error}")),
    }
}

/// Changes the file under its lock and writes it back (0600, atomically) when `change` changed
/// it; returns what `change` returned. An unreadable file is never written over (review of #674,
/// finding 7): the change runs on an empty copy and nothing is saved.
pub fn update<T>(paths: &GxserverPaths, change: impl FnOnce(&mut LinksFile) -> T) -> T {
    let _guard = file_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (mut file, writable) = match read_unlocked(paths) {
        Ok(file) => (file, true),
        Err(error) => {
            eprintln!(
                "[gxserver] {} can't be read ({error}); server links are off until it is fixed or removed, and it is not written over",
                file_path(paths).display()
            );
            (LinksFile::default(), false)
        }
    };
    let before = serde_json::to_vec(&file).unwrap_or_default();
    let answer = change(&mut file);
    if writable {
        let after = serde_json::to_vec_pretty(&file).unwrap_or_default();
        if serde_json::to_vec(&file).unwrap_or_default() != before {
            write_private(&file_path(paths), &after);
        }
        note_links(&file);
    }
    answer
}

/// Writes `bytes` to a temporary file beside `path` and renames it over `path` (atomic on every
/// OS: `std::fs::rename` replaces the target on Windows too), 0600 on Unix.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temp = path.with_extension("json.tmp");
    let written = {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&temp)
            .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
    };
    if written.is_ok() {
        let _ = std::fs::rename(&temp, path);
    } else {
        let _ = std::fs::remove_file(&temp);
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

/// Whether a server link's approval lets the link in, or why not (review of #674, findings 4 and
/// 9): only an approval on one of the owner's own devices (an allow list of caller kinds: the
/// app's owner session or key, a paired phone), from an app whose card worded the ask as a server
/// link (`understands`; an older app showed it as a plain connection). Otherwise the pairing is
/// undone and the asking computer is told why.
pub fn link_approval(
    request: &ghostex_gx_protocol::approval::ApprovalRequest,
) -> Result<(), &'static str> {
    let kind = request.decided_by_kind.as_deref().unwrap_or_default();
    if !matches!(kind, "owner" | "device") {
        return Err(
            "It was approved in a terminal, not in the Ghostex app or on your phone, so the computers were not linked. Approve it in the app or on your phone to link them.",
        );
    }
    if !request
        .decided_understanding
        .iter()
        .any(|word| word == ghostex_gx_protocol::approval::UNDERSTANDS_SERVER_LINK)
    {
        return Err(
            "It was approved in an older Ghostex app or phone app, which shows it as a plain connection, so the computers were not linked. Update Ghostex there (or approve on a device that is up to date), then run ghostex computers add again.",
        );
    }
    Ok(())
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
    let live_devices =
        live_device_ids(db).ok_or("This computer couldn't read its paired devices. Try again.")?;
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

/// The paired rows' ids; `None` when they couldn't be read (never taken for "no rows").
fn live_device_ids(db: &Connection) -> Option<BTreeSet<String>> {
    RemotePairingRepository::new(db)
        .list_devices()
        .ok()
        .map(|devices| {
            devices
                .into_iter()
                .filter(|device| device.status == PairedDeviceStatus::Paired)
                .map(|device| device.id)
                .collect()
        })
}

/// The paired-device row this computer pinned for `device_public_key` (a server link's own row,
/// never another pairing of the same computer, review of #674 finding 1), with its pin.
pub fn pinned_row_for_key(
    paths: &GxserverPaths,
    device_public_key: &str,
) -> Option<(String, DoorPin)> {
    if device_public_key.is_empty() {
        return None;
    }
    read(paths)
        .door
        .into_iter()
        .find(|(_, pin)| pin.device_public_key == device_public_key)
}

/// The paired rows that are server links' own rows.
pub fn pinned_device_ids(paths: &GxserverPaths) -> BTreeSet<String> {
    read(paths).door.into_keys().collect()
}

/// A finished pairing that asked for a server link: pins it when `allowed` (approved on an owner
/// device, not a tunnel-only join), and says so in the pairing's answer either way.
pub fn settle_door_link(
    paths: &GxserverPaths,
    answer: &mut Value,
    server_id: &str,
    pending: &super::pairing_code::PendingPairing,
    allowed: Result<(), &str>,
) -> bool {
    let device_id = answer
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Err(why) = allowed {
        answer["serverLink"] = json!("notLinked");
        answer["serverLinkReason"] = json!(why);
        return false;
    }
    if device_id.is_empty() {
        answer["serverLink"] = json!("notLinked");
        answer["serverLinkReason"] = json!("The pairing finished without a row to link.");
        return false;
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
    true
}

/// A paired-device row was removed here (Settings, a phone, the bell, `computers remove`, or the
/// other computer's `unlink`): its pin, its grant and this side's own link to it go too (the
/// user's 3A), and the other computer hears it when `tell` (best effort, over this side's own
/// tunnel, signed).
///
/// Only a server link's own row counts (review of #674, finding 1): removing another pairing of the
/// same computer (Connect a Computer, its own row and key) leaves the link alone, and nothing is
/// written for a row no link pinned.
///
/// `pin` is the row's pin as it was read before the row went: the sweep may drop a pin whose row
/// is gone at any moment, so it is never looked up only afterwards.
pub fn on_device_removed(paths: &GxserverPaths, device_id: &str, pin: Option<DoorPin>, tell: bool) {
    let Some(pin) = pin.or_else(|| read(paths).door.get(device_id).cloned()) else {
        return;
    };
    let device_id = device_id.to_string();
    let (pin, outgoing) = update(paths, |file| {
        file.door.remove(&device_id);
        let pin = Some(pin);
        let key = pin
            .as_ref()
            .map(|pin| pin.device_public_key.clone())
            .unwrap_or_default();
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
    // A failed read is never taken for "no rows" (review of #674, finding 7).
    let Some(live) = live_device_ids(db) else {
        return;
    };
    let stale: Vec<(String, DoorPin)> = read(paths)
        .door
        .into_iter()
        .filter(|(device_id, _)| !live.contains(device_id))
        .collect();
    // A link whose row went by another route ends like any removed link (its own link here, its
    // grant), so the pin never goes without the rest.
    for (device_id, pin) in stale {
        on_device_removed(paths, &device_id, Some(pin), false);
    }
}

/// The pin of paired row `device_id`, read before the row is removed.
pub fn pin_of(paths: &GxserverPaths, device_id: &str) -> Option<DoorPin> {
    read(paths).door.get(device_id).cloned()
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

/// A cheap limit on every ask, before any work (a key lookup and a signature check).
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

/// Each linked computer's own limit, taken once its signature checked out (review of #674,
/// finding 5): one computer asking too often slows only itself.
fn take_key_slot(device_public_key: &str) -> bool {
    static ASKS: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();
    let mut asks = ASKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let key_asks = asks.entry(device_public_key.to_string()).or_default();
    key_asks.retain(|at| now.duration_since(*at) < Duration::from_secs(60));
    if key_asks.len() >= ASKS_PER_KEY_PER_MINUTE {
        return false;
    }
    key_asks.push(now);
    true
}

/// When this gxserver started answering asks: an ask signed before it is refused, so one caught
/// on its way can't be replayed after a restart, which forgets the last timestamps (review of
/// #674, finding 8). Unset in tests.
static SERVING_SINCE_MS: OnceLock<u64> = OnceLock::new();

pub fn note_serving_since(now_ms: u64) {
    let _ = SERVING_SINCE_MS.set(now_ms);
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
    // The row this computer pinned for that key, never another pairing of the same computer
    // (review of #674, finding 1).
    let pinned = pinned_row_for_key(context.paths, &ask.device_key);
    let device = match &pinned {
        Some((device_id, _)) => RemotePairingRepository::new(context.db)
            .find_device(device_id)
            .map_err(|error| refusal(500, "internal", format!("{error:#}")))?
            .filter(|device| {
                device.status == PairedDeviceStatus::Paired
                    && device.api_token_hash.is_none()
                    && device
                        .device_key
                        .as_ref()
                        .is_some_and(|key| key.public_key == ask.device_key)
            }),
        None => None,
    };
    let (Some(device), Some((_, pin))) = (device, pinned) else {
        return Err(not_paired());
    };
    if !verify_device_signature(&ask.device_key, &ask.payload(), &ask.signature) {
        return Err(not_paired());
    }
    if !take_key_slot(&ask.device_key) {
        return Err(refusal(
            429,
            RATE_LIMITED,
            "This computer asked too often just now. It asks again in a minute.",
        ));
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
    let signed_before_start = SERVING_SINCE_MS
        .get()
        .is_some_and(|since| ask.timestamp_ms < *since);
    if signed_before_start {
        return Err(refusal(
            400,
            CLOCK_OFF,
            "This ask was signed before this computer last started. It asks again in a minute.",
        ));
    }
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
                // A server link the user approved, asked for with its pinned key: the desktop
                // app's grant, so a renewal here never clears what the app said of it.
                true,
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
            // Checked first, before anything else is answered (review of #674, finding 11).
            if !super::server_link_dialer::is_easy_connect_address(&code.address) {
                return Err(refusal(
                    400,
                    "badRequest",
                    "The asking computer's Easy Connect address isn't valid.",
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
                // It must answer as the server this key was pinned to (review of #674, finding 3).
                Some(pin.server_id.clone()),
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
                on_device_removed(context.paths, &removed.id, Some(pin.clone()), false);
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
