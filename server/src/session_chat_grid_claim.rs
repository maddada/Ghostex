//! The chat grid claim gxserver holds for a chat shown without a terminal.
//!
//! CDXC:Zmx 2026-10-08 DECISION:
//! User: "How about if we don't connect the terminal until user switches to it" and chose "Chat-only, no terminal" for Chat View, with the hard rule "pls dont break things for users who set default to terminal at all". A chat that is on screen needs the agent at the 200-column resting width (CDXC:Zmx 2026-09-05), which only an attached terminal typing `ZMX_CHAT` used to claim. gxserver now holds that claim itself: a client reports the chats it shows as leases (`/api/holdSessionChatGrid`, renewed like `/api/holdSessionsAwake`), and while any live lease names a running session gxserver keeps one `ChatClaim` connection open to its daemon (zmx tag 210, wmx `chat-claim`). The connection closes when the last lease ends, expires, or the session stops, and a dropped claim never narrows the grid. A visible terminal still owns the grid.
//! WHY: the claim follows chats a client reports as shown, not chat subscriptions: the desktop keeps hidden and prewarmed chats subscribed, and claiming for those would widen a parked terminal to 200 columns, which reflows it on the next switch back (the regression the hard rule forbids).
//! SEE-ALSO: .dependencies/zmx/src/ipc.zig (ChatClaim), .dependencies/wmx/src/display.rs (claim_chat), apps/desktop/src/app/gx_store/terminal_lifecycle/chat_grid_claims.rs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chrono::Utc;
use serde_json::{json, Map, Value};

use crate::domain::{read_project_id, read_session_id};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{read_session_text, AppState};
use crate::storage::open_gxserver_database;

/// Default lease when a client sends none; a client renews well inside it.
pub const DEFAULT_CHAT_GRID_TTL_MS: i64 = 45_000;
const MIN_CHAT_GRID_TTL_MS: i64 = 5_000;
const MAX_CHAT_GRID_TTL_MS: i64 = 300_000;
/// How often expired leases are dropped and lost claims (a daemon that restarted) are retaken.
const SYNC_INTERVAL: Duration = Duration::from_secs(5);
/// A claim that ended this soon after it was taken waits before the next try (an old wmx
/// daemon answers `chat-claim` with an error and exits).
const QUICK_FAILURE: Duration = Duration::from_secs(2);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60);

type Key = (String, String);

#[derive(Default)]
struct Registry {
    /// Leases per session: holder -> expiry (ms since the epoch).
    leases: HashMap<Key, HashMap<String, i64>>,
    /// The open claim per session, with the daemon it was taken on.
    claims: HashMap<Key, (String, tokio::task::JoinHandle<()>)>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// `/api/holdSessionChatGrid`: `{ holderId, sessions: [{projectId, sessionId}], ttlMs?, release? }`.
/// The same shape as `/api/holdSessionsAwake`; ids that do not resolve come back as `unknownSessions`.
pub(crate) fn hold_session_chat_grid(
    state: &AppState,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let holder_id = crate::session_keep_awake::normalize_holder_id(
        params.get("holderId").and_then(Value::as_str),
    );
    let ttl_ms = params
        .get("ttlMs")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_CHAT_GRID_TTL_MS)
        .clamp(MIN_CHAT_GRID_TTL_MS, MAX_CHAT_GRID_TTL_MS);
    let release = params.get("release").and_then(Value::as_bool) == Some(true);
    let requested = params
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if requested.is_empty() {
        return Err(DomainStateError::bad_request(
            "holdSessionChatGrid requires a non-empty sessions list.",
        ));
    }
    let mut held = Vec::new();
    let mut unknown = Vec::new();
    let expires_at = now_ms().saturating_add(ttl_ms);
    {
        let mut registry = registry()
            .lock()
            .expect("chat grid claim registry poisoned");
        for entry in requested.iter().filter_map(Value::as_object) {
            let project_id = read_project_id(entry)?;
            let session_id = read_session_id(entry)?;
            if repository.get_session(&project_id, &session_id)?.is_none() {
                unknown.push(json!({ "projectId": project_id, "sessionId": session_id }));
                continue;
            }
            let key = (project_id.clone(), session_id.clone());
            if release {
                if let Some(holders) = registry.leases.get_mut(&key) {
                    holders.remove(&holder_id);
                    if holders.is_empty() {
                        registry.leases.remove(&key);
                    }
                }
            } else {
                registry
                    .leases
                    .entry(key)
                    .or_default()
                    .insert(holder_id.clone(), expires_at);
            }
            held.push(json!({ "projectId": project_id, "sessionId": session_id }));
        }
    }
    schedule_sync(state);
    Ok(json!({
        "holderId": holder_id,
        "released": release,
        "sessions": held,
        "ttlMs": ttl_ms,
        "unknownSessions": unknown,
    }))
}

/// The holder-id prefix the phone's chat leases carry (`resolveHolderId` in
/// apps/mobile/app/src/terminal/chatGridClaim.ts), which leases only the one chat in the foreground.
pub(crate) const PHONE_CHAT_HOLDER_PREFIX: &str = "mobile-chat-";

/// The sessions a live lease from a holder whose id starts with `holder_prefix` shows right now.
pub(crate) fn chats_shown_by(holder_prefix: &str) -> Vec<(String, String)> {
    let now = now_ms();
    let registry = registry()
        .lock()
        .expect("chat grid claim registry poisoned");
    registry
        .leases
        .iter()
        .filter(|(_, holders)| {
            holders
                .iter()
                .any(|(holder, expires_at)| holder.starts_with(holder_prefix) && *expires_at > now)
        })
        .map(|(key, _)| key.clone())
        .collect()
}

/// Brings the open claims in line with the leases now, and keeps a slow sweep running for
/// expiries and daemons that restarted under a held claim.
fn schedule_sync(state: &AppState) {
    static SWEEPING: AtomicBool = AtomicBool::new(false);
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let now_state = state.clone();
    runtime.spawn(async move { sync(&now_state) });
    if !SWEEPING.swap(true, Ordering::AcqRel) {
        let state = state.clone();
        runtime.spawn(async move {
            loop {
                tokio::time::sleep(SYNC_INTERVAL).await;
                sync(&state);
            }
        });
    }
}

/// One sync at a time: a sync that read the leases before a hold landed would otherwise abort
/// the claim the hold's own sync had just taken (seen live: a release followed by a hold).
fn sync(state: &AppState) {
    static SYNCING: Mutex<()> = Mutex::new(());
    let _serial = SYNCING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = now_ms();
    let wanted: Vec<Key> = {
        let mut registry = registry()
            .lock()
            .expect("chat grid claim registry poisoned");
        registry.leases.retain(|_, holders| {
            holders.retain(|_, expiry| *expiry > now);
            !holders.is_empty()
        });
        if registry.leases.is_empty() && registry.claims.is_empty() {
            return;
        }
        registry.leases.keys().cloned().collect()
    };
    // Only a running session has a daemon to claim on.
    let mut desired: HashMap<Key, String> = HashMap::new();
    if !wanted.is_empty() {
        if let Ok(db) = open_gxserver_database(&state.paths) {
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            for key in wanted {
                let Ok(Some(session)) = repository.get_session(&key.0, &key.1) else {
                    continue;
                };
                if read_session_text(&session, "lifecycleState").as_deref() != Some("running") {
                    continue;
                }
                if let Ok(name) = crate::zmx::provider_zmx_session_name(&session) {
                    desired.insert(key, name);
                }
            }
        }
    }
    let mut registry = registry()
        .lock()
        .expect("chat grid claim registry poisoned");
    registry.claims.retain(|key, (name, task)| {
        let keep = desired.get(key) == Some(name) && !task.is_finished();
        if !keep {
            task.abort();
        }
        keep
    });
    for (key, name) in desired {
        if registry.claims.contains_key(&key) {
            continue;
        }
        let task = tokio::spawn(hold_claim(name.clone()));
        registry.claims.insert(key, (name, task));
    }
}

/// Holds one claim on the session's daemon for as long as this task runs; aborting the task
/// closes the connection, which is what releases the claim.
async fn hold_claim(zmx_name: String) {
    loop {
        let started = std::time::Instant::now();
        let _ = claim_once(&zmx_name).await;
        // The daemon went away (sleep, restart) or refused: the sweep retakes it while wanted.
        if started.elapsed() < QUICK_FAILURE {
            tokio::time::sleep(RETRY_AFTER_FAILURE).await;
        } else {
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    }
}

/// `ipc.Tag.ChatClaim` with an empty payload: a bare frame header.
#[cfg(unix)]
async fn claim_once(zmx_name: &str) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    const ZMX_IPC_TAG_CHAT_CLAIM: u8 = 210;
    let mut stream =
        tokio::net::UnixStream::connect(crate::zmx::zmx_session_socket_path(zmx_name)).await?;
    let request = crate::zmx::ipc_frame::zmx_ipc_header(ZMX_IPC_TAG_CHAT_CLAIM, 0);
    stream.write_all(&request).await?;
    // The daemon never writes to a claim; a read returns when the session ends.
    let mut sink = [0_u8; 256];
    while stream.read(&mut sink).await? > 0 {}
    Ok(())
}

/// `wmx chat-claim <name>` holds the claim until its stdin closes, which `kill_on_drop` does
/// when this task is aborted.
#[cfg(windows)]
async fn claim_once(zmx_name: &str) -> std::io::Result<()> {
    let zmx = crate::toolchain::require_bundled_zmx().map_err(std::io::Error::other)?;
    use crate::platform::process::NoConsoleWindow;
    let mut child = tokio::process::Command::new(&zmx.executable_path)
        .no_console_window()
        .env("WMX_DIR", crate::zmx::session_directory())
        .args(["chat-claim", zmx_name])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    // `wait` closes a stdin the child still owns, and stdin EOF is what releases the claim.
    let _held = child.stdin.take();
    child.wait().await?;
    Ok(())
}
