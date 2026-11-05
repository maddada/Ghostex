//! One sync step on this computer: take its own edits into the entries, merge another computer's
//! entries, and write back what the entries now say.

use std::sync::{Mutex, MutexGuard, OnceLock};

use ghostex_gx_core::settings_sync::{
    apply_spaces_entries, category_enabled, derive_spaces_edits, describe_spaces_key,
    entries_digest, key_category, local_edit, member_key, merge_entries, merge_spaces_by_name,
    next_hlc, removed_key, spaces_named_apart, LocalSpaces, SyncEntry, GROUP_CATEGORY,
    SYNC_CATEGORIES,
};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

use super::spaces_bridge::{read_local_spaces, write_local_spaces, LocalSpacesRead};
use super::store::{SpacesBaseline, SyncNote, SyncStore};

pub(super) struct Engine<'a> {
    state: &'a AppState,
    db: &'a Connection,
    pub(super) store: SyncStore,
    settings: Option<Value>,
    pub(super) server_id: String,
    pub(super) name: String,
    /// A first sync in this step found Spaces named differently on the two computers.
    pub(super) spaces_named_apart: bool,
}

/// Runs `step` on this computer's entries, one step at a time, and saves them after it.
///
/// CDXC:SettingsSync 2026-11-06 WHY:
/// The periodic exchange, an exchange another computer starts and a Settings action all read, change and save the one stored row; one at a time, so none of them writes back entries another just changed. Nothing waits on the network while holding it.
pub(super) fn with_engine<T>(
    state: &AppState,
    step: impl FnOnce(&mut Engine<'_>) -> Result<T, DomainStateError>,
) -> Result<T, DomainStateError> {
    let _guard = lock();
    let db =
        crate::storage::open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    let mut engine = Engine {
        state,
        db: &db,
        store: SyncStore::load(&db)?,
        settings: crate::session_lifecycle::read_sidebar_settings(&state.paths),
        server_id: state.metadata.server_id.clone(),
        name: crate::connected_computers::this_computer_name(),
        spaces_named_apart: false,
    };
    let result = step(&mut engine)?;
    engine.store.save(&db)?;
    Ok(result)
}

fn lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

impl Engine<'_> {
    /// This computer's settings file, as read for this step.
    pub(super) fn settings(&self) -> Option<&Value> {
        self.settings.as_ref()
    }

    /// A key of this category syncs here (the group's own keys always do).
    pub(super) fn enabled(&self, key: &str) -> bool {
        let prefix = key_category(key);
        prefix == GROUP_CATEGORY
            || SYNC_CATEGORIES
                .iter()
                .find(|category| category.prefix == prefix)
                .is_some_and(|category| category_enabled(category, self.settings.as_ref()))
    }

    /// Writes `value` (`None` deletes) as an edit made on this computer, when it differs.
    pub(super) fn edit(&mut self, key: &str, value: Option<Value>) -> bool {
        let stamp = next_hlc(self.store.clock.as_deref(), now_ms(), &self.server_id);
        let Some(entry) = local_edit(
            &self.store.entries,
            key,
            value,
            stamp.clone(),
            &self.server_id,
            &self.name,
        ) else {
            return false;
        };
        self.store.clock = Some(stamp);
        self.store.entries.insert(entry.key.clone(), entry);
        true
    }

    /// The user removed `server_id` from the group (and didn't add it back).
    pub(super) fn removed(&self, server_id: &str) -> bool {
        self.store
            .entries
            .get(&removed_key(server_id))
            .is_some_and(|entry| entry.value.is_some())
    }

    /// Adds `server_id` to the group the first time it syncs here.
    pub(super) fn welcome(&mut self, server_id: &str, name: &str) {
        let key = member_key(server_id);
        let known = self
            .store
            .entries
            .get(&key)
            .and_then(|entry| entry.value.as_ref())
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str);
        if known != Some(name) {
            self.edit(&key, Some(json!({ "name": name })));
        }
    }

    /// Takes what changed on this computer since the last sync into the entries.
    pub(super) fn export(&mut self) -> Result<(), DomainStateError> {
        let (server_id, name) = (self.server_id.clone(), self.name.clone());
        self.welcome(&server_id, &name);
        if !self.enabled("spaces/") {
            return Ok(());
        }
        let read = read_local_spaces(self.db, &self.server_id, &self.store.entries)?;
        let baseline = self.store.spaces_baseline.clone();
        let edits = derive_spaces_edits(
            local(&read),
            baseline.as_ref().map(|baseline| LocalSpaces {
                spaces: &baseline.spaces,
                collections: &baseline.collections,
                repos: &baseline.repos,
            }),
            &self.store.entries,
        );
        for (key, value) in edits {
            self.edit(&key, value);
        }
        self.store.spaces_baseline = Some(baseline_of(read));
        Ok(())
    }

    /// Merges another computer's entries, returning the new notes. On the first sync with that
    /// computer (`first`), Spaces and groups with the same name become one.
    pub(super) fn merge(&mut self, incoming: Vec<SyncEntry>, first: bool) -> Vec<SyncNote> {
        let incoming: Vec<SyncEntry> = incoming
            .into_iter()
            .filter(|entry| self.enabled(&entry.key))
            .collect();
        if let Some(newest) = incoming.iter().map(|entry| entry.hlc.clone()).max() {
            if self
                .store
                .clock
                .as_ref()
                .is_none_or(|clock| *clock < newest)
            {
                self.store.clock = Some(newest);
            }
        }
        if first && self.enabled("spaces/") {
            self.spaces_named_apart |= spaces_named_apart(&self.store.entries, &incoming);
        }
        let outcome = merge_entries(&mut self.store.entries, incoming);
        if first && self.enabled("spaces/") {
            for (key, value) in merge_spaces_by_name(&self.store.entries) {
                self.edit(&key, value);
            }
        }
        let at_ms = now_ms();
        let notes = outcome
            .conflicts
            .into_iter()
            .filter_map(|conflict| {
                let text = describe_spaces_key(&conflict, &self.store.entries)?;
                Some(SyncNote {
                    conflict,
                    text,
                    at_ms,
                    undone: false,
                })
            })
            .collect();
        self.store.add_notes(notes)
    }

    /// Writes this computer's documents from the entries, after taking any edit made here since
    /// the last step (so a move made a moment ago is not undone).
    pub(super) fn import(&mut self) -> Result<(), DomainStateError> {
        self.export()?;
        if !self.enabled("spaces/") {
            return Ok(());
        }
        let read = read_local_spaces(self.db, &self.server_id, &self.store.entries)?;
        let applied = apply_spaces_entries(&self.store.entries, local(&read));
        if applied.spaces != read.spaces || applied.collections != read.collections {
            if !self.store.spaces_backed_up {
                back_up_spaces(self.state, &read)?;
                self.store.spaces_backed_up = true;
            }
            write_local_spaces(self.state, self.db, &applied.spaces, &applied.collections)?;
            let written = read_local_spaces(self.db, &self.server_id, &self.store.entries)?;
            self.store.spaces_baseline = Some(baseline_of(written));
        }
        Ok(())
    }

    /// The entries this computer sends: the categories it syncs.
    pub(super) fn outgoing(&self) -> Vec<SyncEntry> {
        self.store
            .entries
            .values()
            .filter(|entry| self.enabled(&entry.key))
            .cloned()
            .collect()
    }

    pub(super) fn digest(&self) -> String {
        entries_digest(
            self.store
                .entries
                .values()
                .filter(|entry| self.enabled(&entry.key)),
        )
    }
}

fn local(read: &LocalSpacesRead) -> LocalSpaces<'_> {
    LocalSpaces {
        spaces: &read.spaces,
        collections: &read.collections,
        repos: &read.repos,
    }
}

fn baseline_of(read: LocalSpacesRead) -> SpacesBaseline {
    SpacesBaseline {
        spaces: read.spaces,
        collections: read.collections,
        repos: read.repos,
    }
}

/// This computer's Spaces and groups as they were before sync first touched them, in
/// `<gxserver state>/backups/settings-sync-spaces-<time>.json`. Sync writes nothing when this fails.
fn back_up_spaces(state: &AppState, read: &LocalSpacesRead) -> Result<(), DomainStateError> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = state
        .paths
        .root_dir
        .join("backups")
        .join(format!("settings-sync-spaces-{stamp}.json"));
    let bytes = serde_json::to_vec_pretty(&json!({
        "sidebarSpaces": read.spaces,
        "sidebarProjectCollections": read.collections,
    }))
    .unwrap_or_default();
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| ghostex_paths::durable_file::write_durably(&path, &bytes))
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Settings sync could not back up the Spaces: {error}"),
        })
}
