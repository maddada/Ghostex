//! The retained transcript cache: the record a reopened chat draws before its snapshot arrives.
//!
//! The port of `persistence.ts` from the deleted `apps/desktop/sidebar/session-chat-runtime/`. The
//! RECORD is the core's to encode, decode and judge fresh; what is left here is building the key,
//! which only the host can do because it knows the machine id, and the write's one rule.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! The key is the RETENTION key, `JSON.stringify([machineId, projectId, sessionId])`, and not the
//! storage session key every other chat record is suffixed with. Those two disagree for a remote
//! session (`remote-<machineId>:<projectId>:<sessionId>` against the three-element array), so a
//! cache written under the wrong one is a record the reader never finds and a remote chat
//! that draws empty every time it is reopened.

use ghostex_gx_chat_core::StorageKey;
use serde::Deserialize;

use super::storage;

/// `managedStore('chatSnapshots')`.
const STORE: &str = "chatSnapshots";

/// `readPersistedSessionChat(key)`: the stored record, verbatim.
///
/// The freshness window is the core's (`session::persistence::decode` drops a record older than
/// seven days), so this hands over whatever is on disk and lets the core refuse it.
pub(super) fn read(key: &str, now_ms: i64) -> Result<Option<String>, &'static str> {
    storage::read(&record_key(key), now_ms).map(|raw| raw.filter(|raw| !raw.is_empty()))
}

fn record_key(key: &str) -> StorageKey {
    StorageKey {
        store: STORE.to_string(),
        suffix: key.to_string(),
    }
}

/// `persistSessionChat(key, snapshot, savedAt)`: writes the core's record, or deletes it for `None`.
///
/// It keeps the stored record when that one is NEWER, which is `storage.update`'s transform in the
/// TypeScript: a second writer of the same conversation (a second page of the web build) must not
/// roll the tail back. Only the stored `savedAt` is read, so the check does not build the record.
pub(super) fn write(key: &str, value: Option<&str>, now_ms: i64) -> Result<(), &'static str> {
    let record = record_key(key);
    if let Some(raw) = value {
        let incoming = saved_at(raw);
        let stored = storage::read(&record, now_ms)?;
        if stored
            .as_deref()
            .and_then(saved_at)
            .zip(incoming)
            .is_some_and(|(stored, incoming)| stored > incoming)
        {
            return Ok(());
        }
    }
    storage::write(&record, value, now_ms)
}

fn saved_at(raw: &str) -> Option<i64> {
    #[derive(Deserialize)]
    struct Stamp {
        #[serde(rename = "savedAt")]
        saved_at: i64,
    }
    serde_json::from_str::<Stamp>(raw).ok().map(|stamp| stamp.saved_at)
}
