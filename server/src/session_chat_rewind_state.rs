/*
CDXC:SessionChat 2026-09-02:
A Claude TUI rewind (`/rewind` → "Restore conversation") truncates the agent's
IN-MEMORY conversation only; the transcript on disk is untouched until the next
prompt is appended, at which point that prompt's `parentUuid` names the rewound
leaf and the abandoned rows become a dead branch. When Ghostex itself drove the
rewind it knows the target the instant the TUI accepts it, so it records that
knowledge here and the chat readers (tail page, follower snapshot, export) hide
every message row after the leaf as if the transcript already carried it.

Keyed by the transcript path because the readers are path-based and never see a
session id. The entry retires by itself: the first message row appended past
`cutoff_offset` either proves the rewind (it descends from `leaf_id`, so the
subtree rule takes over for good) or refutes it (the agent went on from the old
leaf), and in both cases the reader clears the entry.
*/
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChatPendingRewind {
    /// `uuid` of the row that is the active leaf after the rewind. `None` means
    /// the conversation was rewound to before its first message.
    pub leaf_id: Option<String>,
    /// Transcript length in bytes when the rewind was accepted. Rows at or past
    /// this offset were written AFTER the rewind and are the agent's answer to
    /// whether it took.
    pub cutoff_offset: u64,
    pub set_at_ms: i64,
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: the rewound messages must not come back when gxserver restarts before the next prompt. The entries are also written to a small file in gxserver's state folder and read back on start, keeping only those whose transcript has not grown past the rewind; tests never touch the file.
#[cfg(not(test))]
fn persisted_path() -> std::path::PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("session-chat-pending-rewinds.json")
}

#[cfg(not(test))]
fn load() -> HashMap<String, SessionChatPendingRewind> {
    let Ok(text) = std::fs::read_to_string(persisted_path()) else {
        return HashMap::new();
    };
    let mut entries: HashMap<String, SessionChatPendingRewind> =
        serde_json::from_str(&text).unwrap_or_default();
    // A transcript that grew past the cutoff has already answered the rewind; a missing one has
    // nothing left to hide; a month-old entry belongs to a conversation nobody continued.
    let oldest_ms = chrono::Utc::now().timestamp_millis() - 30 * 24 * 60 * 60 * 1000;
    entries.retain(|path, pending| {
        pending.set_at_ms >= oldest_ms
            && std::fs::metadata(path).is_ok_and(|metadata| metadata.len() <= pending.cutoff_offset)
    });
    entries
}

#[cfg(test)]
fn load() -> HashMap<String, SessionChatPendingRewind> {
    HashMap::new()
}

/// The rewind recorded for `transcript_path`, read from the file each time instead of from this
/// process's copy: `ghostex acp-host` is its own process and must see a rewind gxserver recorded
/// after it started (SEE-ALSO: acp/driver_claude_rewind.rs).
pub(crate) fn persisted_session_chat_pending_rewind(
    transcript_path: &Path,
) -> Option<SessionChatPendingRewind> {
    load().remove(&store_key(transcript_path))
}

#[cfg(not(test))]
fn save(entries: &HashMap<String, SessionChatPendingRewind>) {
    let path = persisted_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(text) = serde_json::to_string(entries) else {
        return;
    };
    let staged = path.with_extension("json.tmp");
    if std::fs::write(&staged, text).is_ok() {
        let _ = std::fs::rename(&staged, &path);
    }
}

#[cfg(test)]
fn save(_entries: &HashMap<String, SessionChatPendingRewind>) {}

fn store() -> &'static Mutex<HashMap<String, SessionChatPendingRewind>> {
    static STORE: OnceLock<Mutex<HashMap<String, SessionChatPendingRewind>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(load()))
}

fn store_key(transcript_path: &Path) -> String {
    transcript_path
        .canonicalize()
        .unwrap_or_else(|_| transcript_path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub fn set_session_chat_pending_rewind(transcript_path: &Path, pending: SessionChatPendingRewind) {
    if let Ok(mut entries) = store().lock() {
        entries.insert(store_key(transcript_path), pending);
        // Under the store's lock, before the save: a read that sees the rewind sees the new number.
        PENDING_REWIND_GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        save(&entries);
    }
}

/// Moves each time a pending rewind is recorded or cleared: tail reads skip the rewound rows
/// (`session_chat_branch`), so a page kept from before is not the page a read gives now
/// (session_chat_tail_cache.rs).
static PENDING_REWIND_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn session_chat_pending_rewind_generation() -> u64 {
    PENDING_REWIND_GENERATION.load(std::sync::atomic::Ordering::Acquire)
}

pub fn session_chat_pending_rewind(transcript_path: &Path) -> Option<SessionChatPendingRewind> {
    store()
        .lock()
        .ok()
        .and_then(|entries| entries.get(&store_key(transcript_path)).cloned())
}

pub fn clear_session_chat_pending_rewind(transcript_path: &Path) {
    if let Ok(mut entries) = store().lock() {
        if entries.remove(&store_key(transcript_path)).is_some() {
            PENDING_REWIND_GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            save(&entries);
        }
    }
}
