//! The chats shared through Ghostex Collab: each share's link key, route, code, expiry and mode
//! (saved, so a gxserver restart keeps links working), and while it runs, who is watching, who is
//! typing and the guest messages waiting for the owner (memory only).
//!
//! A share's token (`gxshare_…`) is what the relay connector (server/src/server/collab_connector.rs)
//! calls this gxserver with on a guest's behalf. It never leaves gxserver and opens only that one
//! chat: the routes in [`GUEST_ENDPOINTS`] for its own `projectId`/`sessionId`, and that chat's
//! stream on `/api/events?stream=sessionChat`.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1612, Q1617): the guest's page is "the GPUI web build in a guest mode that shows only that one chat (never the main token: a new share-scoped token, modelled on connected-computer peer grants that only allow listed endpoints)", and "a guest can never answer permission prompts or see anything but that chat".

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use subtle::ConstantTimeEq;

use super::config::{CollabConfig, GuestMode};
use crate::paths::GxserverPaths;

const SHARES_FILE: &str = "collab-shares.json";
/// Guests one share lets in at once.
pub const MAX_VIEWERS: usize = 10;
/// How long one typing signal shows "is typing".
pub const TYPING_MS: i64 = 4_000;
/// Wrong codes one guest connection may type before it is closed.
pub const MAX_CODE_TRIES: u32 = 5;
/// Wrong codes one share takes, from every connection together, before it stops taking codes for
/// [`CODE_LOCK_MS`]: guessing a six-digit code is then out of reach.
pub const MAX_SHARE_CODE_MISSES: u32 = 20;
pub const CODE_LOCK_MS: i64 = 15 * 60_000;
/// Messages one guest may send a minute, and guest messages one share may hold at once.
pub const GUEST_MESSAGES_PER_MINUTE: usize = 6;
pub const MAX_HELD: usize = 20;
/// Pictures one guest may upload to a share.
pub const MAX_GUEST_IMAGES: usize = 20;

/// What a share token may call, always for its own chat only.
///
/// CDXC:Collab 2026-11-07 WHY: `readSessionChatImage` takes a file path, so a guest could read any picture on the owner's computer through it; a guest's read is answered only for a path the shared chat itself shows (`session_files_links::session_shows_path`) or one that guest uploaded (`saveSessionChatImage`). `readVisualPageThumbnail` stays out.
pub const GUEST_ENDPOINTS: &[&str] = &[
    "/api/readSessionChat",
    "/api/readSessionChatImage",
    "/api/readSessionChatResource",
    "/api/saveSessionChatImage",
    // Answered by Ghostex Collab, never by the owner's composer: a guest message, and the guest's
    // typing (`setSessionChatDraft` from a guest never touches the owner's draft).
    "/api/sendSessionChatMessage",
    "/api/setSessionChatDraft",
    "/api/collabTyping",
];

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
    /// Six digits; empty when this share asks for no code.
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

#[derive(Clone, Debug)]
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

fn paths() -> GxserverPaths {
    crate::paths::get_gxserver_paths(None)
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
    format!("{:06}", u32::from_le_bytes(random_bytes::<4>()) % 1_000_000)
}

fn with_registry<T>(change: impl FnOnce(&mut Registry) -> T) -> T {
    let mut registry = registry().lock().unwrap_or_else(PoisonError::into_inner);
    if !registry.loaded {
        registry.loaded = true;
        let stored: Vec<StoredShare> =
            std::fs::read_to_string(paths().app_state_dir.join(SHARES_FILE))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
        registry.shares = stored
            .into_iter()
            .map(|share| (share.id.clone(), share))
            .collect();
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
    }
    answer
}

fn save(registry: &Registry) {
    let paths = paths();
    let _ = std::fs::create_dir_all(&paths.app_state_dir);
    let target = paths.app_state_dir.join(SHARES_FILE);
    let temporary = paths.app_state_dir.join(format!("{SHARES_FILE}.tmp"));
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
    super::notify_changed();
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
            route: super::tunnel::route_id(&key),
            host_secret: hex::encode(random_bytes::<32>()),
            token: format!("gxshare_{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>())),
            code: if config.require_code {
                new_code()
            } else {
                String::new()
            },
            mode: config.mode,
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

/// What a share token opens: its share, when the token is one and still live.
pub fn authorize_token(token: &str) -> Option<StoredShare> {
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
        held
    })
}

/// Takes a held message out (the owner sent, edited or declined it).
pub fn take_held(share_id: &str, held_id: &str) -> Option<HeldMessage> {
    with_registry(|registry| {
        let live = registry.live.get_mut(share_id)?;
        let index = live.held.iter().position(|held| held.id == held_id)?;
        Some(live.held.remove(index))
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

/// The link guests open.
pub fn link(share: &StoredShare, relay_url: &str) -> String {
    format!("{relay_url}/s#{}", share.key)
}

/// `collab` on a chat frame and on `readSessionChat`: `{ shared: false }`, or who is watching,
/// who is typing and the guest messages waiting. Never the link, the code or the token.
pub fn insert_chat_collab(frame: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    let config = super::config::read_config(&paths());
    let value = with_registry(|registry| {
        let Some(share) = registry
            .shares
            .values()
            .find(|share| share.project_id == project_id && share.session_id == session_id)
            .filter(|_| config.enabled)
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
        json!({
            "shared": true,
            "shareId": share.id,
            "mode": share.mode.as_str(),
            "owner": super::config::owner_display_name(&config),
            "ownerTyping": live.is_some_and(|live| live.owner_typing_until_ms > now),
            "viewers": viewers,
            "held": held,
        })
    });
    frame.insert("collab".to_string(), value);
}

/// One share as the owner's Share popover and Settings > Collab show it.
pub fn owner_view(share: &StoredShare, relay_url: &str, title: Option<String>) -> Value {
    let (viewers, held) = with_registry(|registry| {
        let live = registry.live.get(&share.id);
        let viewers: Vec<Value> = live
            .map(|live| {
                live.viewers
                    .values()
                    .map(|viewer| json!({ "id": viewer.id, "name": viewer.name, "joinedAtMs": viewer.joined_at_ms }))
                    .collect()
            })
            .unwrap_or_default();
        (viewers, live.map(|live| live.held.len()).unwrap_or(0))
    });
    json!({
        "shareId": share.id,
        "projectId": share.project_id,
        "sessionId": share.session_id,
        "title": title,
        "link": link(share, relay_url),
        "code": share.code,
        "mode": share.mode.as_str(),
        "createdAtMs": share.created_at_ms,
        "expiresAtMs": share.expires_at_ms,
        "viewers": viewers,
        "held": held,
    })
}
