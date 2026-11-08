//! The chats shared through Ghostex Collab: each share's link key, route, code, expiry and mode
//! (saved, so a gxserver restart keeps links working), and while it runs, who is watching, who is
//! typing and the guest messages waiting for the owner (memory only).
//!
//! A share's token (`gxshare_…`) is what the relay connector (connector.rs) replays a guest's
//! reads to gxserver with. It never leaves this computer and opens only that one chat: gxserver
//! lets it read that chat (`readSessionChat`, its resources, the pictures it shows, its pages) and
//! follow its stream, and nothing else (server/src/collab_shim/registry.rs). Everything a guest
//! writes is answered here, in the component (guest.rs).
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1612, Q1617): the guest's page is "the GPUI web build in a guest mode that shows only that one chat (never the main token: a new share-scoped token, modelled on connected-computer peer grants that only allow listed endpoints)", and "a guest can never answer permission prompts or see anything but that chat".

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use crate::config::{CollabConfig, GuestMode};

pub const SHARES_FILE: &str = "collab-shares.json";
/// Guest messages waiting for the owner, per share, kept across restarts like the owner's own
/// queue keeps its rows.
pub const HELD_FILE: &str = "collab-held.json";
/// What happened to a held message, kept this long so a guest's page can follow it.
const RESOLVED_KEEP_MS: i64 = 10 * 60_000;
const RESOLVED_KEEP: usize = 40;
/// Guests one share lets in at once.
pub const MAX_VIEWERS: usize = 10;
/// How long one typing signal shows "is typing".
pub const TYPING_MS: i64 = 4_000;
/// Wrong codes one guest connection may type before it is closed.
pub const MAX_CODE_TRIES: u32 = 5;
/// Digits in a share's code. Every share has one (GitHub #495).
pub const CODE_DIGITS: usize = 8;
/// Wrong codes one share takes, from every connection together, before it stops taking codes for
/// [`CODE_LOCK_MS`]: guessing an eight-digit code is then out of reach.
pub const MAX_SHARE_CODE_MISSES: u32 = 20;
pub const CODE_LOCK_MS: i64 = 15 * 60_000;
/// Messages one guest may send a minute, and guest messages one share may hold at once.
pub const GUEST_MESSAGES_PER_MINUTE: usize = 6;
pub const MAX_HELD: usize = 20;
/// Pictures one guest may upload to a share.
pub const MAX_GUEST_IMAGES: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredShare {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    /// The link's key, 43 URL-safe base64 characters.
    pub key: String,
    pub route: String,
    /// Proves to the relay that this computer registered `route` first.
    pub host_secret: String,
    pub token: String,
    /// [`CODE_DIGITS`] digits; a share without one is dropped when the file is read.
    pub code: String,
    pub mode: GuestMode,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    /// Browsers the owner removed from this share (their page's device ids): they can't rejoin.
    #[serde(default)]
    pub blocked: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Viewer {
    pub id: String,
    pub name: String,
    /// The guest page's own random id, kept in its browser, for the block list.
    pub device: String,
    pub joined_at_ms: i64,
    pub typing_until_ms: i64,
    /// Pictures this guest uploaded, which only they and the owner may read back.
    pub images: Vec<String>,
    /// When their last messages went, for the per-minute limit.
    pub sent_at_ms: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldMessage {
    pub id: String,
    pub viewer_id: String,
    pub name: String,
    pub text: String,
    pub image_paths: Vec<String>,
    pub at_ms: i64,
}

#[derive(Default)]
struct Live {
    viewers: BTreeMap<String, Viewer>,
    held: Vec<HeldMessage>,
    /// Held messages the owner answered lately: `(held id, "sent" | "edited" | "declined", when)`.
    resolved: Vec<(String, &'static str, i64)>,
    owner_typing_until_ms: i64,
    code_misses: u32,
    code_locked_until_ms: i64,
}

#[derive(Default)]
struct Registry {
    loaded: bool,
    shares: BTreeMap<String, StoredShare>,
    live: BTreeMap<String, Live>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

fn new_code() -> String {
    format!(
        "{:08}",
        u64::from_le_bytes(random_bytes::<8>()) % 100_000_000
    )
}

/// A share as the rules allow it: a code, and either no end (it lasts until the owner stops it,
/// Q1785) or an end at most a day after it was made. A file edited to say otherwise (no code, a
/// longer end) is not trusted: that share is dropped.
fn share_is_valid(share: &StoredShare) -> bool {
    share.code.len() == CODE_DIGITS
        && share.code.bytes().all(|byte| byte.is_ascii_digit())
        && share
            .expires_at_ms
            .is_none_or(|at| at <= share.created_at_ms + crate::config::MAX_SHARE_MS)
}

fn with_registry<T>(change: impl FnOnce(&mut Registry) -> T) -> T {
    let mut registry = registry().lock().unwrap_or_else(PoisonError::into_inner);
    if !registry.loaded {
        registry.loaded = true;
        let stored: Vec<StoredShare> = std::fs::read_to_string(crate::data_dir().join(SHARES_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        registry.shares = stored
            .into_iter()
            .filter(share_is_valid)
            .map(|share| (share.id.clone(), share))
            .collect();
        for (share_id, messages) in read_held(&crate::data_dir()) {
            if registry.shares.contains_key(&share_id) && !messages.is_empty() {
                registry.live.entry(share_id).or_default().held = messages;
            }
        }
    }
    let now = now_ms();
    let expired: Vec<String> = registry
        .shares
        .values()
        .filter(|share| share.expires_at_ms.is_some_and(|at| at <= now))
        .map(|share| share.id.clone())
        .collect();
    let mut changed = false;
    for id in expired {
        registry.shares.remove(&id);
        registry.live.remove(&id);
        changed = true;
    }
    let before = registry.shares.len();
    let answer = change(&mut registry);
    if changed || before != registry.shares.len() {
        save(&registry);
        save_held(&registry);
    }
    answer
}

/// Writes the held messages of every live share.
fn save_held(registry: &Registry) {
    let held: BTreeMap<String, Vec<HeldMessage>> = registry
        .live
        .iter()
        .filter(|(share_id, live)| !live.held.is_empty() && registry.shares.contains_key(*share_id))
        .map(|(share_id, live)| (share_id.clone(), live.held.clone()))
        .collect();
    write_held(&crate::data_dir(), &held);
}

/// The held messages kept in `folder`, per share.
fn read_held(folder: &std::path::Path) -> BTreeMap<String, Vec<HeldMessage>> {
    std::fs::read_to_string(folder.join(HELD_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keeps `held` in `folder` (owner-only, written whole), or removes the file once none wait.
fn write_held(folder: &std::path::Path, held: &BTreeMap<String, Vec<HeldMessage>>) {
    let target = folder.join(HELD_FILE);
    if held.is_empty() {
        let _ = std::fs::remove_file(&target);
        return;
    }
    let _ = std::fs::create_dir_all(folder);
    let temporary = folder.join(format!("{HELD_FILE}.tmp"));
    if std::fs::write(&temporary, serde_json::to_string(held).unwrap_or_default()).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&temporary, &target);
    }
}

fn save(registry: &Registry) {
    let folder = crate::data_dir();
    let _ = std::fs::create_dir_all(&folder);
    let target = folder.join(SHARES_FILE);
    let temporary = folder.join(format!("{SHARES_FILE}.tmp"));
    let shares: Vec<&StoredShare> = registry.shares.values().collect();
    if std::fs::write(
        &temporary,
        serde_json::to_string_pretty(&shares).unwrap_or_default(),
    )
    .is_ok()
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&temporary, &target);
    }
    crate::notify_changed();
}

/// The share of a chat, made with the config's choices when it has none.
pub fn share_session(config: &CollabConfig, project_id: &str, session_id: &str) -> StoredShare {
    with_registry(|registry| {
        if let Some(share) = registry
            .shares
            .values()
            .find(|share| share.project_id == project_id && share.session_id == session_id)
        {
            return share.clone();
        }
        let key: [u8; 32] = random_bytes();
        let now = now_ms();
        let share = StoredShare {
            id: format!("share_{}", hex::encode(random_bytes::<8>())),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            key: URL_SAFE_NO_PAD.encode(key),
            route: crate::tunnel::route_id(&key),
            host_secret: hex::encode(random_bytes::<32>()),
            token: format!("gxshare_{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>())),
            code: new_code(),
            // Never Send: that is turned on per share, with the owner's approval.
            mode: if config.mode == GuestMode::View {
                GuestMode::View
            } else {
                GuestMode::Approve
            },
            created_at_ms: now,
            expires_at_ms: config.expiry.millis().map(|ms| now + ms),
            blocked: Vec::new(),
        };
        registry.shares.insert(share.id.clone(), share.clone());
        share
    })
}

pub fn list() -> Vec<StoredShare> {
    with_registry(|registry| registry.shares.values().cloned().collect())
}

pub fn get(share_id: &str) -> Option<StoredShare> {
    with_registry(|registry| registry.shares.get(share_id).cloned())
}

pub fn for_session(project_id: &str, session_id: &str) -> Option<StoredShare> {
    with_registry(|registry| {
        registry
            .shares
            .values()
            .find(|share| share.project_id == project_id && share.session_id == session_id)
            .cloned()
    })
}

/// Ends a share at once: its key is forgotten, so the link is dead for good.
pub fn stop(share_id: &str) -> Option<StoredShare> {
    with_registry(|registry| {
        registry.live.remove(share_id);
        registry.shares.remove(share_id)
    })
}

/// Ends the share of a chat that was deleted (or is gone from the database).
pub fn stop_for_session(project_id: &str, session_id: &str) -> Option<StoredShare> {
    with_registry(|registry| {
        let id = registry
            .shares
            .values()
            .find(|share| share.project_id == project_id && share.session_id == session_id)?
            .id
            .clone();
        registry.live.remove(&id);
        registry.shares.remove(&id)
    })
}

pub fn stop_all() -> Vec<StoredShare> {
    with_registry(|registry| {
        registry.live.clear();
        std::mem::take(&mut registry.shares).into_values().collect()
    })
}

pub fn set_mode(share_id: &str, mode: GuestMode) -> Option<StoredShare> {
    with_registry(|registry| {
        let share = registry.shares.get_mut(share_id)?;
        share.mode = mode;
        let share = share.clone();
        save(registry);
        Some(share)
    })
}

/// Every live share as gxserver needs it to let that share's guests read their one chat.
pub fn scopes() -> Vec<ghostex_gx_protocol::collab_component::ShareScope> {
    with_registry(|registry| {
        registry
            .shares
            .values()
            .map(|share| {
                let live = registry.live.get(&share.id);
                ghostex_gx_protocol::collab_component::ShareScope {
                    share_id: share.id.clone(),
                    project_id: share.project_id.clone(),
                    session_id: share.session_id.clone(),
                    token: share.token.clone(),
                    viewer_ids: live
                        .map(|live| live.viewers.keys().cloned().collect())
                        .unwrap_or_default(),
                    images: live
                        .map(|live| {
                            live.viewers
                                .values()
                                .flat_map(|viewer| viewer.images.iter().cloned())
                                .chain(
                                    live.held
                                        .iter()
                                        .flat_map(|held| held.image_paths.iter().cloned()),
                                )
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            })
            .collect()
    })
}

/// A share token, compared in constant time (tests and the connector's own checks).
pub fn by_token(token: &str) -> Option<StoredShare> {
    if !token.starts_with("gxshare_") {
        return None;
    }
    with_registry(|registry| {
        registry
            .shares
            .values()
            .find(|share| bool::from(share.token.as_bytes().ct_eq(token.as_bytes())))
            .cloned()
    })
}

pub enum JoinRefusal {
    Ended,
    Full,
    Blocked,
}

pub fn join(share_id: &str, name: &str, device: &str) -> Result<Viewer, JoinRefusal> {
    with_registry(|registry| {
        let Some(share) = registry.shares.get(share_id) else {
            return Err(JoinRefusal::Ended);
        };
        if !device.is_empty() && share.blocked.iter().any(|blocked| blocked == device) {
            return Err(JoinRefusal::Blocked);
        }
        let live = registry.live.entry(share_id.to_string()).or_default();
        if live.viewers.len() >= MAX_VIEWERS {
            return Err(JoinRefusal::Full);
        }
        let viewer = Viewer {
            id: format!("guest_{}", hex::encode(random_bytes::<6>())),
            name: name.to_string(),
            device: device.to_string(),
            joined_at_ms: now_ms(),
            typing_until_ms: 0,
            images: Vec::new(),
            sent_at_ms: Vec::new(),
        };
        live.viewers.insert(viewer.id.clone(), viewer.clone());
        Ok(viewer)
    })
}

/// Whether the share takes codes right now; a wrong one counts toward its lock.
pub fn code_locked(share_id: &str) -> bool {
    with_registry(|registry| {
        registry
            .live
            .get(share_id)
            .is_some_and(|live| live.code_locked_until_ms > now_ms())
    })
}

/// A wrong code; `true` when the share now stops taking codes for a while.
pub fn note_wrong_code(share_id: &str) -> bool {
    with_registry(|registry| {
        let live = registry.live.entry(share_id.to_string()).or_default();
        live.code_misses += 1;
        if live.code_misses >= MAX_SHARE_CODE_MISSES {
            live.code_misses = 0;
            live.code_locked_until_ms = now_ms() + CODE_LOCK_MS;
            return true;
        }
        false
    })
}

/// Removes a guest; `block` keeps their browser out of this share for good.
pub fn remove_viewer(share_id: &str, viewer_id: &str, block: bool) -> Option<Viewer> {
    with_registry(|registry| {
        let viewer = registry.live.get_mut(share_id)?.viewers.remove(viewer_id)?;
        if block && !viewer.device.is_empty() {
            if let Some(share) = registry.shares.get_mut(share_id) {
                if !share.blocked.contains(&viewer.device) {
                    share.blocked.push(viewer.device.clone());
                }
            }
            save(registry);
        }
        Some(viewer)
    })
}

/// A guest may send another message now (and it counts), within [`GUEST_MESSAGES_PER_MINUTE`].
pub fn allow_message(share_id: &str, viewer_id: &str) -> bool {
    with_registry(|registry| {
        let now = now_ms();
        let Some(viewer) = registry
            .live
            .get_mut(share_id)
            .and_then(|live| live.viewers.get_mut(viewer_id))
        else {
            return false;
        };
        viewer.sent_at_ms.retain(|at| now - at < 60_000);
        if viewer.sent_at_ms.len() >= GUEST_MESSAGES_PER_MINUTE {
            return false;
        }
        viewer.sent_at_ms.push(now);
        true
    })
}

pub fn held_count(share_id: &str) -> usize {
    with_registry(|registry| {
        registry
            .live
            .get(share_id)
            .map_or(0, |live| live.held.len())
    })
}

/// A picture a guest uploaded; `false` past [`MAX_GUEST_IMAGES`].
pub fn note_guest_image(share_id: &str, viewer_id: &str, path: &str) -> bool {
    with_registry(|registry| {
        let Some(viewer) = registry
            .live
            .get_mut(share_id)
            .and_then(|live| live.viewers.get_mut(viewer_id))
        else {
            return false;
        };
        if viewer.images.len() >= MAX_GUEST_IMAGES {
            return false;
        }
        viewer.images.push(path.to_string());
        true
    })
}

/// A picture some guest of this share uploaded.
pub fn guest_uploaded(share_id: &str, path: &str) -> bool {
    with_registry(|registry| {
        registry.live.get(share_id).is_some_and(|live| {
            live.viewers
                .values()
                .any(|viewer| viewer.images.iter().any(|image| image == path))
                || live
                    .held
                    .iter()
                    .any(|held| held.image_paths.iter().any(|image| image == path))
        })
    })
}

pub fn leave(share_id: &str, viewer_id: &str) -> bool {
    with_registry(|registry| {
        registry
            .live
            .get_mut(share_id)
            .is_some_and(|live| live.viewers.remove(viewer_id).is_some())
    })
}

pub fn viewer(share_id: &str, viewer_id: &str) -> Option<Viewer> {
    with_registry(|registry| registry.live.get(share_id)?.viewers.get(viewer_id).cloned())
}

pub fn viewer_ids(share_id: &str) -> Vec<String> {
    with_registry(|registry| {
        registry
            .live
            .get(share_id)
            .map(|live| live.viewers.keys().cloned().collect())
            .unwrap_or_default()
    })
}

/// Someone typed into the chat box: the owner (`viewer_id` `None`) or a guest. `true` when they
/// were not typing a moment ago, so the chat needs a new frame.
pub fn note_typing(share_id: &str, viewer_id: Option<&str>, typing: bool) -> bool {
    with_registry(|registry| {
        if !registry.shares.contains_key(share_id) {
            return false;
        }
        let now = now_ms();
        let until = if typing { now + TYPING_MS } else { 0 };
        let live = registry.live.entry(share_id.to_string()).or_default();
        let slot = match viewer_id {
            None => &mut live.owner_typing_until_ms,
            Some(id) => match live.viewers.get_mut(id) {
                Some(viewer) => &mut viewer.typing_until_ms,
                None => return false,
            },
        };
        let was = *slot > now;
        *slot = until;
        was != typing
    })
}

pub fn hold(share_id: &str, viewer: &Viewer, text: &str, image_paths: &[String]) -> HeldMessage {
    with_registry(|registry| {
        let held = HeldMessage {
            id: format!("held_{}", hex::encode(random_bytes::<6>())),
            viewer_id: viewer.id.clone(),
            name: viewer.name.clone(),
            text: text.to_string(),
            image_paths: image_paths.to_vec(),
            at_ms: now_ms(),
        };
        let live = registry.live.entry(share_id.to_string()).or_default();
        live.held.push(held.clone());
        if let Some(guest) = live.viewers.get_mut(&viewer.id) {
            guest.typing_until_ms = 0;
        }
        save_held(registry);
        held
    })
}

/// A held message, still waiting.
pub fn peek_held(share_id: &str, held_id: &str) -> Option<HeldMessage> {
    with_registry(|registry| {
        registry
            .live
            .get(share_id)?
            .held
            .iter()
            .find(|held| held.id == held_id)
            .cloned()
    })
}

/// Takes a held message out (the owner sent, edited or declined it).
///
/// `outcome` (`sent`, `edited` or `declined`) stays on the chat frame for a while, so the guest's
/// page can tell its form whether its answers went on.
pub fn take_held(share_id: &str, held_id: &str, outcome: &'static str) -> Option<HeldMessage> {
    with_registry(|registry| {
        let live = registry.live.get_mut(share_id)?;
        let index = live.held.iter().position(|held| held.id == held_id)?;
        let held = live.held.remove(index);
        let now = now_ms();
        live.resolved
            .retain(|(_, _, at)| now - *at < RESOLVED_KEEP_MS);
        live.resolved.push((held.id.clone(), outcome, now));
        if live.resolved.len() > RESOLVED_KEEP {
            let extra = live.resolved.len() - RESOLVED_KEEP;
            live.resolved.drain(..extra);
        }
        save_held(registry);
        Some(held)
    })
}

/// The soonest moment a typing line runs out, so a timer can clear it.
pub fn next_typing_deadline(share_id: &str) -> Option<i64> {
    with_registry(|registry| {
        let now = now_ms();
        let live = registry.live.get(share_id)?;
        std::iter::once(live.owner_typing_until_ms)
            .chain(live.viewers.values().map(|viewer| viewer.typing_until_ms))
            .filter(|until| *until > now)
            .min()
    })
}

/// The link guests open: the guest page this release pinned (`page_pin`), by its id, and the
/// share's key after `#`, which browsers never send to the relay.
pub fn link(share: &StoredShare, relay_url: &str) -> String {
    format!(
        "{relay_url}/s/{}/#{}",
        crate::page_pin::page_id(),
        share.key
    )
}

/// `collab` on a chat frame and on `readSessionChat` (gxserver puts it there, sync.rs pushes it):
/// `{ shared: false }`, or who is watching, who is typing and the guest messages waiting. Never
/// the link, the code or the token.
pub fn chat_extras(project_id: &str, session_id: &str) -> Value {
    let config = crate::config::read_config();
    with_registry(|registry| {
        let Some(share) = registry
            .shares
            .values()
            .find(|share| share.project_id == project_id && share.session_id == session_id)
        else {
            return json!({ "shared": false });
        };
        let now = now_ms();
        let live = registry.live.get(&share.id);
        let viewers: Vec<Value> = live
            .map(|live| {
                live.viewers
                    .values()
                    .map(|viewer| {
                        json!({
                            "id": viewer.id,
                            "name": viewer.name,
                            "typing": viewer.typing_until_ms > now,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let held: Vec<Value> = live
            .map(|live| {
                live.held
                    .iter()
                    .map(|held| {
                        json!({
                            "id": held.id,
                            "viewerId": held.viewer_id,
                            "name": held.name,
                            "text": held.text,
                            "imagePaths": held.image_paths,
                            "atMs": held.at_ms,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let resolved: Vec<Value> = live
            .map(|live| {
                live.resolved
                    .iter()
                    .filter(|(_, _, at)| now - *at < RESOLVED_KEEP_MS)
                    .map(|(id, outcome, _)| json!({ "id": id, "outcome": outcome }))
                    .collect()
            })
            .unwrap_or_default();
        json!({
            "shared": true,
            "shareId": share.id,
            "mode": share.mode.as_str(),
            "owner": crate::config::owner_display_name(&config),
            "ownerTyping": live.is_some_and(|live| live.owner_typing_until_ms > now),
            "viewers": viewers,
            "held": held,
            "resolved": resolved,
        })
    })
}

/// One share's link and code: only in the answer to an approved `share` call, never in a read.
pub fn owner_secrets(share: &StoredShare, relay_url: &str) -> Value {
    json!({ "link": link(share, relay_url), "code": share.code })
}

/// One share as the owner's Share popover and Settings > Collab show it. No link or code: those
/// come once, with the approval that shared the chat (`owner_secrets`).
pub fn owner_view(share: &StoredShare, relay_url: &str, title: Option<String>) -> Value {
    let (viewers, held, waiting_viewer_ids) = with_registry(|registry| {
        let live = registry.live.get(&share.id);
        // Whose messages wait, for the popover's "Message waiting" and Review.
        let mut waiting: Vec<String> = live
            .map(|live| {
                live.held
                    .iter()
                    .map(|held| held.viewer_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        waiting.sort();
        waiting.dedup();
        let viewers: Vec<Value> = live
            .map(|live| {
                live.viewers
                    .values()
                    .map(|viewer| json!({ "id": viewer.id, "name": viewer.name, "joinedAtMs": viewer.joined_at_ms }))
                    .collect()
            })
            .unwrap_or_default();
        (
            viewers,
            live.map(|live| live.held.len()).unwrap_or(0),
            waiting,
        )
    });
    json!({
        "shareId": share.id,
        "projectId": share.project_id,
        "sessionId": share.session_id,
        "title": title,
        "relay": relay_url,
        "mode": share.mode.as_str(),
        "createdAtMs": share.created_at_ms,
        "expiresAtMs": share.expires_at_ms,
        "viewers": viewers,
        "held": held,
        "waitingViewerIds": waiting_viewer_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(code: &str, created: i64, expires: Option<i64>) -> StoredShare {
        StoredShare {
            id: "share_1".into(),
            project_id: "p".into(),
            session_id: "s".into(),
            key: String::new(),
            route: String::new(),
            host_secret: String::new(),
            token: "gxshare_x".into(),
            code: code.into(),
            mode: GuestMode::Send,
            created_at_ms: created,
            expires_at_ms: expires,
            blocked: Vec::new(),
        }
    }

    #[test]
    fn shares_without_a_code_or_past_a_day_are_dropped() {
        let day = crate::config::MAX_SHARE_MS;
        assert!(share_is_valid(&share("12345678", 0, Some(day))));
        assert!(!share_is_valid(&share("", 0, Some(day))));
        assert!(!share_is_valid(&share("123456", 0, Some(day))));
        // No end: it lasts until the owner stops sharing.
        assert!(share_is_valid(&share("12345678", 0, None)));
        assert!(!share_is_valid(&share("12345678", 0, Some(day + 1))));
    }

    #[test]
    fn codes_are_eight_digits() {
        for _ in 0..50 {
            let code = new_code();
            assert_eq!(code.len(), CODE_DIGITS);
            assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
        }
    }

    #[test]
    fn held_messages_survive_a_restart() {
        let folder = tempfile::tempdir().unwrap();
        let message = HeldMessage {
            id: "held_1".to_string(),
            viewer_id: "viewer_1".to_string(),
            name: "Sam".to_string(),
            text: "Can you check the total?".to_string(),
            image_paths: vec!["/tmp/shot.png".to_string()],
            at_ms: 1,
        };
        let held = BTreeMap::from([("share_1".to_string(), vec![message])]);
        write_held(folder.path(), &held);
        let read = read_held(folder.path());
        assert_eq!(read["share_1"][0].text, "Can you check the total?");
        assert_eq!(
            read["share_1"][0].image_paths,
            vec!["/tmp/shot.png".to_string()]
        );
        write_held(folder.path(), &BTreeMap::new());
        assert!(!folder.path().join(HELD_FILE).exists());
        assert!(read_held(folder.path()).is_empty());
    }
}
