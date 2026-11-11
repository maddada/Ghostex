//! The view each chat is shown in (Summary, Normal or Detailed, and Quiet View), kept on the
//! computer that owns the session so every device opens the chat the same way.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "btw we need to track the chat view the user set on each chat accross all devices so it's the same (doesn't need to sync instantly, some slight delay is ok)". The chat core keeps its four view records per device as before (`summary`, `simple`, `verbose`, `quietView`) and also hands each pick to this computer (`/api/setSessionChatView`), which keeps the newest pick of each record by the time it was made. Every chat read carries the kept pick (`chatView`), so a device takes it when it opens the chat, and the phone's long poll within a few seconds (the pick's times are in the read's fingerprint); nothing is pushed to clients. A device that could not reach the computer keeps its own pick and hands it over on its next read, and the newest pick wins.
//!
//! CDXC:SessionChat 2026-11-11 SEE-ALSO:
//! packages/gx-chat-core/src/transcript/view_sync.rs (what a client sends and takes),
//! packages/gx-protocol/src/chat.rs (`ChatViewPick`), server/src/session_chat_read.rs (the read
//! carries it), server/src/ghostex_cli/session_chat_rpc.rs (the phone's method list).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use axum::http::StatusCode;
use ghostex_gx_protocol::{ChatViewField, ChatViewPick};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

const STORE_FILE: &str = "session-chat-views.json";
/// The most chats kept; past it the chat picked longest ago is dropped.
const MAX_CHATS: usize = 2_000;
/// The records a client may set: the chat core's four view records.
const FIELDS: [&str; 4] = ["summary", "simple", "verbose", "quietView"];

/// The kept picks, by project and session.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    chats: BTreeMap<String, BTreeMap<String, ChatViewPick>>,
}

struct Loaded {
    path: PathBuf,
    store: Store,
}

fn loaded() -> &'static Mutex<Option<Loaded>> {
    static LOADED: OnceLock<Mutex<Option<Loaded>>> = OnceLock::new();
    LOADED.get_or_init(|| Mutex::new(None))
}

/// Runs `f` on the store for `paths`, reading the file the first time.
fn with_store<T>(paths: &crate::paths::GxserverPaths, f: impl FnOnce(&mut Loaded) -> T) -> T {
    let path = paths.app_state_dir.join(STORE_FILE);
    let mut guard = loaded()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.as_ref().is_none_or(|loaded| loaded.path != path) {
        let store = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Store>(&bytes).ok())
            .unwrap_or_default();
        *guard = Some(Loaded { path, store });
    }
    f(guard.as_mut().expect("loaded above"))
}

fn newest(pick: &ChatViewPick) -> i64 {
    pick.values().map(|field| field.at).max().unwrap_or(0)
}

fn save(loaded: &mut Loaded) {
    let total: usize = loaded.store.chats.values().map(BTreeMap::len).sum();
    if total > MAX_CHATS {
        let mut ages: Vec<(i64, String, String)> = loaded
            .store
            .chats
            .iter()
            .flat_map(|(project, sessions)| {
                sessions
                    .iter()
                    .map(move |(session, pick)| (newest(pick), project.clone(), session.clone()))
            })
            .collect();
        ages.sort();
        for (_, project, session) in ages.into_iter().take(total - MAX_CHATS) {
            if let Some(sessions) = loaded.store.chats.get_mut(&project) {
                sessions.remove(&session);
                if sessions.is_empty() {
                    loaded.store.chats.remove(&project);
                }
            }
        }
    }
    if let Some(parent) = loaded.path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_vec(&loaded.store) {
        let _ = ghostex_paths::durable_file::write_durably(&loaded.path, &text);
    }
}

/// The pick kept for one chat, empty when none was made.
pub(crate) fn read_pick(
    paths: &crate::paths::GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> ChatViewPick {
    with_store(paths, |loaded| {
        loaded
            .store
            .chats
            .get(project_id)
            .and_then(|sessions| sessions.get(session_id))
            .cloned()
            .unwrap_or_default()
    })
}

/// Keeps each record of `incoming` that is newer than the one kept; answers the kept pick.
fn merge(
    paths: &crate::paths::GxserverPaths,
    project_id: &str,
    session_id: &str,
    incoming: ChatViewPick,
) -> ChatViewPick {
    with_store(paths, |loaded| {
        let pick = loaded
            .store
            .chats
            .entry(project_id.to_string())
            .or_default()
            .entry(session_id.to_string())
            .or_default();
        let mut changed = false;
        for (name, field) in incoming {
            if !FIELDS.contains(&name.as_str()) {
                continue;
            }
            if pick.get(&name).is_none_or(|kept| field.at > kept.at) {
                pick.insert(name, field);
                changed = true;
            }
        }
        let answer = pick.clone();
        if changed {
            save(loaded);
        }
        answer
    })
}

/// `chatView` on a chat read: the kept pick, `{}` when none was made, so a client can tell this
/// computer keeps picks from an older one that never sends the field.
pub(crate) fn insert_into(
    result: &mut Map<String, Value>,
    paths: &crate::paths::GxserverPaths,
    project_id: &str,
    session_id: &str,
) {
    result.insert(
        "chatView".to_string(),
        json!(read_pick(paths, project_id, session_id)),
    );
}

/// What the read's fingerprint hashes for the pick: nothing for a chat without one, so its
/// fingerprint stays as it was.
pub(crate) fn fingerprint_part(
    paths: &crate::paths::GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> Option<Vec<(String, i64)>> {
    let pick = read_pick(paths, project_id, session_id);
    (!pick.is_empty()).then(|| {
        pick.into_iter()
            .map(|(name, field)| (name, field.at))
            .collect()
    })
}

/// `/api/setSessionChatView` `{projectId, sessionId, chatView: {<record>: {value, at}}}`: answers
/// `{chatView}`, the pick as kept after the merge.
pub(crate) fn handle_set_session_chat_view_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let incoming = params
        .get("chatView")
        .cloned()
        .and_then(|value| serde_json::from_value::<BTreeMap<String, ChatViewField>>(value).ok());
    let (false, false, Some(incoming)) = (project_id.is_empty(), session_id.is_empty(), incoming)
    else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "setSessionChatView requires projectId, sessionId and chatView."
                    .to_string(),
            },
        );
    };
    let kept = merge(&state.paths, &project_id, &session_id, incoming);
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "chatView": kept })),
    )
}
