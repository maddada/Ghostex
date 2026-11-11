//! One entry per Beads workspace: the lock every Ghostex bd call for it takes, the cached read
//! replies, the published snapshot and the sync state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_protocol::beads_view::{BeadsReadiness, BeadsSyncRemote, BeadsViewRow};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::change::ChangeToken;
use super::sync::SyncState;

/// A server-mode workspace has no change token, so its reads count as current for this long.
pub(crate) const SERVER_MODE_REFRESH: Duration = Duration::from_secs(10);

pub(crate) struct CachedRead {
    pub(crate) token: Option<ChangeToken>,
    pub(crate) at: Instant,
    pub(crate) reply: Value,
}

pub(crate) struct Published {
    pub(crate) token: Option<ChangeToken>,
    pub(crate) at: Instant,
    pub(crate) read_at: String,
    pub(crate) revision: String,
    pub(crate) content_hash: [u8; 32],
    pub(crate) rows: Vec<BeadsViewRow>,
    pub(crate) readiness: BeadsReadiness,
    pub(crate) custom_statuses: Vec<String>,
    /// The earliest dated defer still in the future: once it passes, a read wakes it.
    pub(crate) next_wake_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Default)]
pub(crate) struct EntryState {
    pub(crate) reads: HashMap<String, CachedRead>,
    pub(crate) published: Option<Published>,
    pub(crate) counter: u64,
    pub(crate) settling: bool,
    /// A Ghostex write ran; a server-mode workspace re-reads on the next poll.
    pub(crate) written: bool,
    /// A Ghostex write ran and the published snapshot has not been read since. Apart from
    /// `written`, which a cached read clears, so a Board read never hides a write from the Tree.
    pub(crate) snapshot_stale: bool,
    /// When the last refresh failed and the change token it saw, so a failing bd is asked again
    /// on a pause, not on every poll.
    pub(crate) refresh_failed: Option<(Instant, Option<ChangeToken>)>,
    pub(crate) actor: Option<Option<String>>,
    pub(crate) remote: Option<Option<BeadsSyncRemote>>,
    /// When reading the remotes last failed, so a failure is retried, but not on every poll.
    pub(crate) remote_failed_at: Option<Instant>,
    pub(crate) sync: SyncState,
    /// A refresh is running in the background; polls answer with what is published meanwhile.
    pub(crate) refreshing: bool,
    /// The header's Refresh: read everything again, whatever the change token says.
    pub(crate) force: bool,
    /// Why the last refresh failed, until one succeeds.
    pub(crate) last_error: Option<String>,
}

pub(crate) struct WorkspaceEntry {
    pub(crate) path: String,
    pub(crate) key: String,
    /// Chosen when the entry is made: a new gxserver run or another workspace never reuses a
    /// revision a client remembers.
    pub(crate) epoch: u64,
    lock: Arc<tokio::sync::Mutex<()>>,
    pub(crate) state: Mutex<EntryState>,
}

impl WorkspaceEntry {
    pub(crate) async fn lock_owned(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.lock.clone().lock_owned().await
    }

    pub(crate) async fn lock_within(
        &self,
        limit: Duration,
    ) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        tokio::time::timeout(limit, self.lock.clone().lock_owned())
            .await
            .ok()
    }

    pub(crate) fn state(&self) -> std::sync::MutexGuard<'_, EntryState> {
        self.state.lock().expect("beads workspace state")
    }

    /// Whether another Ghostex bd call holds the workspace right now.
    pub(crate) fn lock_busy(&self) -> bool {
        self.lock.try_lock().is_err()
    }

    /// The header's Refresh: forget every cached read and side fact and read the workspace again.
    pub(crate) fn invalidate(&self) {
        let mut state = self.state();
        state.reads.clear();
        state.force = true;
        state.refresh_failed = None;
        state.remote = None;
        state.remote_failed_at = None;
        state.actor = None;
    }

    /// After a write: every cached read is stale, and server mode re-reads.
    pub(crate) fn forget_reads(&self) {
        let mut state = self.state();
        state.reads.clear();
        state.written = true;
        state.snapshot_stale = true;
    }
}

fn entries() -> &'static Mutex<HashMap<String, Arc<WorkspaceEntry>>> {
    static ENTRIES: OnceLock<Mutex<HashMap<String, Arc<WorkspaceEntry>>>> = OnceLock::new();
    ENTRIES.get_or_init(Default::default)
}

pub(crate) fn existing_entry(path: &str) -> Option<Arc<WorkspaceEntry>> {
    entries()
        .lock()
        .expect("beads workspaces")
        .get(path)
        .cloned()
}

pub(crate) fn entry(path: &str) -> Arc<WorkspaceEntry> {
    let mut entries = entries().lock().expect("beads workspaces");
    entries
        .entry(path.to_string())
        .or_insert_with(|| {
            let digest = Sha256::digest(path.as_bytes());
            let key = digest[..8]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            Arc::new(WorkspaceEntry {
                path: path.to_string(),
                key,
                epoch: rand::random::<u64>() >> 16,
                lock: Arc::new(tokio::sync::Mutex::new(())),
                state: Mutex::new(EntryState::default()),
            })
        })
        .clone()
}

/// Whether a cached reply or snapshot read at `cached_token`/`at` still describes the workspace.
pub(crate) fn still_current(
    cached_token: &Option<ChangeToken>,
    at: Instant,
    current_token: &Option<ChangeToken>,
    written: bool,
) -> bool {
    match (cached_token, current_token) {
        (Some(cached), Some(current)) => cached == current,
        (None, None) => !written && at.elapsed() < SERVER_MODE_REFRESH,
        _ => false,
    }
}
