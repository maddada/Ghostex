//! Publishes, on each session's presentation row (`switchingHostMode`), that gxserver is switching
//! the session between Terminal-based and Native right now.
//!
//! CDXC:SessionChat 2026-11-25 WHY:
//! A switch to Native sleeps and wakes the session, which ends the terminal a client's tab is
//! attached to. A desktop that did not start the switch itself (another device, the bulk "Turn on
//! Native mode", a queued switch) closed the tab and the sidebar reopened it as a plain terminal tab
//! on the sleeping pill and its Resume button. Every switch (`execute_switch_to_acp`, `switch_to_cli`
//! in channel.rs) raises this flag before it touches the session and lowers it when it returns, so
//! each client keeps the tab and its chat view through the restart. The flag lives in memory, so a
//! gxserver restart mid-switch drops it and clients get a fresh snapshot without it when they
//! reconnect; a failed switch lowers it like a finished one; a switch still running after
//! [`MAX_PUBLISHED`] stops being published, so a stuck switch never pins a tab for good.
//! SEE-ALSO: apps/desktop/src/app/session_chat/drafts_and_attachments.rs
//! (`agents_session_switching_host_mode`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::domain::DomainRepository;
use crate::logging::LogLevel;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Longest a switch is published; a switch to Native sleeps and wakes in seconds.
const MAX_PUBLISHED: Duration = Duration::from_secs(120);

/// The raised flags, each with the raise that owns it.
static RAISED: LazyLock<Mutex<HashMap<(String, String), u64>>> = LazyLock::new(Mutex::default);
static NEXT_RAISE: AtomicU64 = AtomicU64::new(1);
/// Bumped on every raise and lower; see [`changes`].
static CHANGES: AtomicU64 = AtomicU64::new(0);

/// How many times a flag was raised or lowered. The phone's pushed sessions list
/// (server/src/server/sessions_list_stream.rs) rebuilds when it moves: the flag lives in memory,
/// so the database stamp that list watches never changes for it.
pub(crate) fn changes() -> u64 {
    CHANGES.load(Ordering::Relaxed)
}

pub(crate) fn is_raised(project_id: &str, session_id: &str) -> bool {
    RAISED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&(project_id.to_string(), session_id.to_string()))
}

/// One switch's flag: raised by [`HostModeSwitchFlag::raise`], lowered when dropped.
pub(crate) struct HostModeSwitchFlag {
    state: AppState,
    key: (String, String),
    raise: u64,
}

impl HostModeSwitchFlag {
    /// Raises the flag and returns once clients have been sent the row carrying it.
    pub(crate) async fn raise(state: &AppState, project_id: &str, session_id: &str) -> Self {
        let key = (project_id.to_string(), session_id.to_string());
        let raise = NEXT_RAISE.fetch_add(1, Ordering::Relaxed);
        RAISED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.clone(), raise);
        CHANGES.fetch_add(1, Ordering::Relaxed);
        // Made before the first await, so a cancelled caller still lowers it.
        let flag = Self {
            state: state.clone(),
            key: key.clone(),
            raise,
        };
        let expiry_state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(MAX_PUBLISHED).await;
            if lower(&key, raise) {
                super::channel::log(
                    LogLevel::Warn,
                    "acp.switchingHostModeExpired",
                    json!({ "projectId": key.0, "sessionId": key.1 }),
                );
                publish(expiry_state, key).await;
            }
        });
        publish(state.clone(), flag.key.clone()).await;
        flag
    }
}

impl Drop for HostModeSwitchFlag {
    fn drop(&mut self) {
        if !lower(&self.key, self.raise) {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(publish(self.state.clone(), self.key.clone()));
        }
    }
}

/// Lowers the flag if `raise` still owns it.
fn lower(key: &(String, String), raise: u64) -> bool {
    let mut raised = RAISED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if raised.get(key) != Some(&raise) {
        return false;
    }
    raised.remove(key);
    CHANGES.fetch_add(1, Ordering::Relaxed);
    true
}

/// Sends clients the session's presentation row, which reads the flag ([`is_raised`]).
async fn publish(state: AppState, (project_id, session_id): (String, String)) {
    let _ = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        crate::server::schedule_presentation_session_delta(
            &state,
            &db,
            &repository,
            &project_id,
            &session_id,
        )
        .ok()
    })
    .await;
}
