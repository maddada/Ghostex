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

#[derive(Clone, Debug, PartialEq, Eq)]
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

fn store() -> &'static Mutex<HashMap<String, SessionChatPendingRewind>> {
    static STORE: OnceLock<Mutex<HashMap<String, SessionChatPendingRewind>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
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
    }
}

pub fn session_chat_pending_rewind(transcript_path: &Path) -> Option<SessionChatPendingRewind> {
    store()
        .lock()
        .ok()
        .and_then(|entries| entries.get(&store_key(transcript_path)).cloned())
}

pub fn clear_session_chat_pending_rewind(transcript_path: &Path) {
    if let Ok(mut entries) = store().lock() {
        entries.remove(&store_key(transcript_path));
    }
}
