//! Synced entries and how two computers' entries merge.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One synced key as a computer holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEntry {
    pub key: String,
    /// `None` is a tombstone: the thing was deleted.
    #[serde(default)]
    pub value: Option<Value>,
    /// When it was written (`next_hlc`); the newest stamp of a key wins.
    pub hlc: String,
    /// The stamp of the value the writer replaced, so a later write that never saw another
    /// computer's write reads as a conflict rather than an ordinary edit.
    #[serde(default)]
    pub parent: Option<String>,
    /// The server id of the computer that wrote it, and that computer's name.
    pub origin: String,
    #[serde(default)]
    pub origin_name: String,
}

/// Two computers changed one key apart: `kept` won (the newer stamp) and `lost` was replaced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    /// The same on every computer that sees this conflict: the key and the lost stamp.
    pub id: String,
    pub kept: SyncEntry,
    pub lost: SyncEntry,
}

/// What a merge did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeOutcome {
    /// Keys whose winning entry changed here.
    pub changed: Vec<String>,
    pub conflicts: Vec<SyncConflict>,
}

/// Merges `incoming` into `store`: per key, the newer stamp wins. A key both sides wrote without
/// one having seen the other's write is reported as a conflict, whichever side won.
pub fn merge_entries(
    store: &mut BTreeMap<String, SyncEntry>,
    incoming: impl IntoIterator<Item = SyncEntry>,
) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();
    for entry in incoming {
        match store.get(&entry.key) {
            Some(current) if current.hlc >= entry.hlc => {
                if current.hlc != entry.hlc && concurrent(current, &entry) {
                    outcome.conflicts.push(conflict(current.clone(), entry));
                }
            }
            current => {
                if let Some(current) = current.filter(|current| concurrent(&entry, current)) {
                    outcome
                        .conflicts
                        .push(conflict(entry.clone(), current.clone()));
                }
                outcome.changed.push(entry.key.clone());
                store.insert(entry.key.clone(), entry);
            }
        }
    }
    outcome
}

/// `newer` was written without seeing `older`, and they say different things.
fn concurrent(newer: &SyncEntry, older: &SyncEntry) -> bool {
    newer.value != older.value
        && newer.origin != older.origin
        && newer
            .parent
            .as_deref()
            .is_none_or(|parent| parent < older.hlc.as_str())
}

fn conflict(kept: SyncEntry, lost: SyncEntry) -> SyncConflict {
    SyncConflict {
        id: format!("{}@{}", lost.key, lost.hlc),
        kept,
        lost,
    }
}

/// A write made on this computer, or `None` when the key already says `value`.
pub fn local_edit(
    store: &BTreeMap<String, SyncEntry>,
    key: &str,
    value: Option<Value>,
    hlc: String,
    origin: &str,
    origin_name: &str,
) -> Option<SyncEntry> {
    let current = store.get(key);
    if current.is_some_and(|current| current.value == value) {
        return None;
    }
    // Deleting what was never written is nothing to tell the others.
    if current.is_none() && value.is_none() {
        return None;
    }
    Some(SyncEntry {
        key: key.to_string(),
        value,
        hlc,
        parent: current.map(|current| current.hlc.clone()),
        origin: origin.to_string(),
        origin_name: origin_name.to_string(),
    })
}

/// A short fingerprint of which stamp every key holds, so two computers that hold the same
/// entries can tell without sending them (FNV-1a over the sorted keys and stamps).
pub fn entries_digest<'a>(entries: impl IntoIterator<Item = &'a SyncEntry>) -> String {
    let mut sorted: Vec<(&str, &str)> = entries
        .into_iter()
        .map(|entry| (entry.key.as_str(), entry.hlc.as_str()))
        .collect();
    sorted.sort_unstable();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (key, hlc) in sorted {
        for byte in key.bytes().chain([0]).chain(hlc.bytes()).chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
