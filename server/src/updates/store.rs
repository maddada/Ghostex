//! What Settings > Updates remembers: each item's "Update automatically" switch (in
//! `updates.json`, so it survives restarts), the updates running now, and how the last one of
//! each item ended.
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdatesRecord {
    /// Item id to its switch; an item that is not here is off.
    #[serde(default)]
    auto: BTreeMap<String, bool>,
}

/// How an item's last update here ended, while it still matters (a success clears it).
#[derive(Clone, Debug)]
pub(super) struct Outcome {
    /// `failed`, or `needsYou` for an update only the user can finish (an extension's new permissions).
    pub(super) state: &'static str,
    pub(super) message: String,
}

static RUNNING: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static OUTCOMES: LazyLock<Mutex<HashMap<String, Outcome>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static RECORD_LOCK: Mutex<()> = Mutex::new(());

fn record_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join("updates.json")
}

fn read_record(paths: &GxserverPaths) -> UpdatesRecord {
    std::fs::read_to_string(record_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(super) fn auto_enabled(paths: &GxserverPaths, id: &str) -> bool {
    read_record(paths).auto.get(id).copied().unwrap_or(false)
}

pub(super) fn auto_ids(paths: &GxserverPaths) -> Vec<String> {
    read_record(paths)
        .auto
        .into_iter()
        .filter_map(|(id, on)| on.then_some(id))
        .collect()
}

pub(super) fn set_auto(paths: &GxserverPaths, id: &str, enabled: bool) -> Result<(), String> {
    let _guard = RECORD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut record = read_record(paths);
    if enabled {
        record.auto.insert(id.to_string(), true);
    } else {
        record.auto.remove(id);
    }
    let path = record_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(&record).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| error.to_string())
}

/// Marks `id` as running with `phase` ("Waiting…", "Updating…"); `false` when it already runs.
pub(super) fn begin(id: &str, phase: &str) -> bool {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if running.contains_key(id) {
        return false;
    }
    running.insert(id.to_string(), phase.to_string());
    true
}

pub(super) fn set_phase(id: &str, phase: &str) {
    if let Some(entry) = RUNNING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(id)
    {
        *entry = phase.to_string();
    }
}

pub(super) fn running_phase(id: &str) -> Option<String> {
    RUNNING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
}

pub(super) fn any_running() -> bool {
    !RUNNING.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
}

/// Ends `id`'s run with `outcome` (`None` for a success).
pub(super) fn finish(id: &str, outcome: Option<Outcome>) {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    let mut outcomes = OUTCOMES.lock().unwrap_or_else(|e| e.into_inner());
    match outcome {
        Some(outcome) => {
            outcomes.insert(id.to_string(), outcome);
        }
        None => {
            outcomes.remove(id);
        }
    }
}

pub(super) fn outcome(id: &str) -> Option<Outcome> {
    OUTCOMES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
}
