//! What this computer keeps for settings sync, in one metadata row (`settingsSync`): every synced
//! entry, its clock, what the last sync left its own documents as, the recent notes and the
//! computers it has synced with.

use std::collections::BTreeMap;

use ghostex_gx_core::settings_sync::{SyncConflict, SyncEntry};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::DomainStateError;

const METADATA_KEY: &str = "settingsSync";
/// Notes kept for Settings > Sync, newest last.
const MAX_NOTES: usize = 50;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SyncStore {
    pub(crate) entries: BTreeMap<String, SyncEntry>,
    /// The newest stamp written or received here.
    pub(crate) clock: Option<String>,
    /// This computer's Spaces documents and shareable projects as the last sync left them.
    pub(crate) spaces_baseline: Option<SpacesBaseline>,
    pub(crate) notes: Vec<SyncNote>,
    /// This computer's Spaces were saved to `backups/` before sync first wrote them.
    pub(crate) spaces_backed_up: bool,
    /// Spaces sync keys projects by repository and folder here, and the Spaces an older sync
    /// folded together were put back from the backup (#488, `repair_folded_spaces`).
    pub(crate) spaces_keyed_by_folder: bool,
    /// Each value category's keys and values as the last sync left this computer (missing while
    /// the category is off, so turning it on seeds again).
    pub(crate) value_baselines: BTreeMap<String, BTreeMap<String, Value>>,
    pub(crate) peers: BTreeMap<String, PeerRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpacesBaseline {
    pub(crate) spaces: Value,
    pub(crate) collections: Value,
    pub(crate) repos: Vec<(String, String)>,
}

/// A conflict worth telling the user about, with its sentence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncNote {
    pub(crate) conflict: SyncConflict,
    pub(crate) text: String,
    pub(crate) at_ms: u64,
    #[serde(default)]
    pub(crate) undone: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PeerRecord {
    pub(crate) name: String,
    pub(crate) last_sync_ms: u64,
    /// The first sync with it happened (Spaces with the same name were merged then).
    pub(crate) met: bool,
    /// It refused this computer: the user removed this computer from the group there.
    pub(crate) refused: Option<String>,
    /// Its gxserver has no settings sync (an older version), until when it is not asked again.
    pub(crate) unsupported_until_ms: u64,
    /// The categories it synced at the last exchange, for the fingerprint a round asks with first.
    pub(crate) categories: Vec<String>,
}

impl SyncStore {
    pub(crate) fn load(db: &Connection) -> Result<Self, DomainStateError> {
        let stored = db
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                [METADATA_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?;
        Ok(stored
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default())
    }

    pub(crate) fn save(&self, db: &Connection) -> Result<(), DomainStateError> {
        let text = serde_json::to_string(self).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Settings sync serialization error: {error}"),
        })?;
        db.execute(
            r#"
            INSERT INTO metadata (key, value, updatedAt)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
            "#,
            rusqlite::params![METADATA_KEY, text, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(sql_error)?;
        Ok(())
    }

    /// Adds notes not seen before (the same conflict reaches a computer from several others).
    pub(crate) fn add_notes(&mut self, notes: Vec<SyncNote>) -> Vec<SyncNote> {
        let mut added = Vec::new();
        for note in notes {
            if self
                .notes
                .iter()
                .any(|known| known.conflict.id == note.conflict.id)
            {
                continue;
            }
            added.push(note.clone());
            self.notes.push(note);
        }
        let overflow = self.notes.len().saturating_sub(MAX_NOTES);
        self.notes.drain(..overflow);
        added
    }
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite settings sync error: {error}"),
    }
}
