//! gxserver's side of Ghostex Collab: a thin, inert shim. All of Collab's working code (shares,
//! codes, the relay connection, the end-to-end lock, the guest page pin, held guest messages,
//! the owner's rules) is the separately downloaded `ghostex-collab` component (server/collab),
//! which this shim installs, starts and talks to over a local channel
//! (packages/gx-protocol/src/collab_component.rs).
//!
//! What stays here:
//! - `install.rs`: install (with the owner's signed approval), update and uninstall
//!   (`/api/extensionComponents`).
//! - `sidecar.rs`: starts and supervises the component while it is installed.
//! - `http.rs`: `/api/collab` forwarded to the component (the gate stays here), the component's
//!   calls (`/api/collabHost`), and a guest's replayed reads of their one chat.
//! - `registry.rs`: the live shares the component reported, which let a share token read its
//!   chat; empty when the component isn't running.
//! - this file: each shared chat's Collab part of its frame, as the component reported it.
//!
//! Without the component every Collab call is refused, no share token opens anything and chat
//! frames carry no `collab` part, so no client shows Share.
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495, Q1730): "i'm fine to not include the code of collab in the app unless you enable it in extensions (when enabled, we need to show touch id / auth so we make sure it's the user enabling this feature and installing it not an agent). We need to do exact same system that we have for vscode extension". Full sidecar split in 11.0.1.

pub(crate) mod http;
pub(crate) mod install;
pub(crate) mod registry;
pub(crate) mod sidecar;

use std::collections::HashMap;
use std::sync::{OnceLock, PoisonError, RwLock};

use serde_json::{json, Map, Value};

use crate::server::AppState;

fn extras() -> &'static RwLock<HashMap<(String, String), Value>> {
    static EXTRAS: OnceLock<RwLock<HashMap<(String, String), Value>>> = OnceLock::new();
    EXTRAS.get_or_init(Default::default)
}

/// Keeps a chat's Collab part as the component reported it (`{shared: false}` forgets it).
pub(crate) fn set_chat_extras(project_id: &str, session_id: &str, value: Value) {
    let key = (project_id.to_string(), session_id.to_string());
    let mut extras = extras().write().unwrap_or_else(PoisonError::into_inner);
    if value.get("shared") == Some(&Value::Bool(true)) {
        extras.insert(key, value);
    } else {
        extras.remove(&key);
    }
}

/// Forgets every chat's Collab part (the component stopped); the chats it named.
pub(crate) fn clear_chat_extras() -> Vec<(String, String)> {
    extras()
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .drain()
        .map(|(key, _)| key)
        .collect()
}

/// `collab` on a chat frame and on `readSessionChat`, while Collab runs: the chat's part as the
/// component reported it, or `{shared: false}`, both with `available: true` (clients show Share
/// only then). `{shared: false, available: false}` while Collab isn't installed or isn't running:
/// a frame without `collab` leaves a client's last one in place.
pub(crate) fn insert_chat_collab(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    if !sidecar::is_running() {
        frame.insert(
            "collab".to_string(),
            json!({
                "shared": false,
                "available": false,
                "installed": install::installed().is_some(),
                "hideShare": share_hidden(),
            }),
        );
        return;
    }
    let mut value = extras()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(project_id.to_string(), session_id.to_string()))
        .cloned()
        .unwrap_or_else(|| json!({ "shared": false }));
    if let Some(object) = value.as_object_mut() {
        object.insert("available".to_string(), Value::Bool(true));
    }
    frame.insert("collab".to_string(), value);
}

/// Republishes chats whose Collab part changed.
pub(crate) fn republish(state: &AppState, chats: &[(String, String)]) {
    for (project_id, session_id) in chats {
        crate::server::republish_session_chat(state, project_id, session_id);
    }
}

/// Republishes every chat someone follows now (the component started or stopped, so every
/// chat gains or loses its Collab part and its Share button).
pub(crate) fn republish_followed(state: &AppState) {
    let keys: Vec<String> = state
        .session_chat_followers
        .lock()
        .map(|followers| followers.keys().cloned().collect())
        .unwrap_or_default();
    let chats: Vec<(String, String)> = keys
        .iter()
        .filter_map(|key| key.rsplit_once('/'))
        .map(|(project_id, session_id)| (project_id.to_string(), session_id.to_string()))
        .collect();
    republish(state, &chats);
}

/// A chat was deleted: its share (if any) ends with it.
pub(crate) fn chat_deleted(project_id: &str, session_id: &str) {
    if registry::for_chat(project_id, session_id).is_none() {
        return;
    }
    sidecar::send_event_later(
        ghostex_gx_protocol::collab_component::SidecarEvent::ChatDeleted {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        },
    );
}

/// The owner's "Don't show Share" (the not-installed Share popover, Q1782): Share stays out of
/// chats on this computer until Collab is installed or the owner shows it again (Settings >
/// Extensions > Collab). Kept in gxserver's state so it works without the component.
const SHARE_HIDDEN_FILE: &str = "collab-share-hidden";

static SHARE_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn share_hidden() -> bool {
    SHARE_HIDDEN.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn set_share_hidden(
    paths: &crate::paths::GxserverPaths,
    hidden: bool,
) -> std::io::Result<()> {
    let file = paths.app_state_dir.join(SHARE_HIDDEN_FILE);
    if hidden {
        std::fs::create_dir_all(&paths.app_state_dir)?;
        std::fs::write(&file, "1")?;
    } else if let Err(error) = std::fs::remove_file(&file) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error);
        }
    }
    SHARE_HIDDEN.store(hidden, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Files 11.0.0's Collab (and 11.0.1's before the split) kept in gxserver's state folder.
const LEGACY_FILES: &[&str] = &[
    "collab.json",
    "collab.json.tmp",
    "collab-shares.json",
    "collab-shares.json.tmp",
    "collab-held.json",
    "collab-held.json.tmp",
];

/// The 11.1.0 move (GitHub #495): Collab starts not installed. 11.0.0's choices and shares in
/// gxserver's state folder are removed once, at start, so no old share or switch survives; the
/// component keeps its own files in its own folder (sidecar.rs `data_dir`).
pub(crate) fn migrate_at_start(paths: &crate::paths::GxserverPaths) {
    SHARE_HIDDEN.store(
        paths.app_state_dir.join(SHARE_HIDDEN_FILE).exists(),
        std::sync::atomic::Ordering::Relaxed,
    );
    let mut removed = false;
    for name in LEGACY_FILES {
        removed |= std::fs::remove_file(paths.app_state_dir.join(name)).is_ok();
    }
    if removed {
        eprintln!("[collab] 11.1.0: removed the Collab settings and shares kept before Collab became an extension");
    }
}
