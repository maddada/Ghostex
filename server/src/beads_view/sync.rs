//! Pull and Push (`bd dolt pull`, `bd dolt push --no-adopt`) as background operations whose
//! progress and result the Beads view reads with its polls.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use ghostex_gx_protocol::beads_view::{
    BeadsSyncInfo, BeadsSyncRemote, BeadsSyncResult, BeadsSyncRunning,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::typed_operations::{
    beads_action_command, run_process_command_with_timeout, TypedOperationContext,
    TypedOperationError,
};

use super::cache::WorkspaceEntry;
use super::discovery::WorkspaceInfo;

/// One deadline for the whole operation, waiting for the workspace lock included.
const SYNC_DEADLINE: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedSync {
    #[serde(default)]
    last_push_at: Option<String>,
    #[serde(default)]
    last_pull_at: Option<String>,
    #[serde(default)]
    last_result: Option<BeadsSyncResult>,
}

#[derive(Default)]
pub(crate) struct SyncState {
    loaded: bool,
    saved: SavedSync,
    running: Option<BeadsSyncRunning>,
}

impl SyncState {
    pub(crate) fn info(&self, remote: Option<BeadsSyncRemote>) -> BeadsSyncInfo {
        BeadsSyncInfo {
            remote,
            running: self.running.clone(),
            last_push_at: self.saved.last_push_at.clone(),
            last_pull_at: self.saved.last_pull_at.clone(),
            last_result: self.saved.last_result.clone(),
        }
    }
}

fn saved_file() -> std::path::PathBuf {
    crate::paths::get_gxserver_paths(None)
        .app_state_dir
        .join("beads-sync.json")
}

fn file_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Default::default)
}

fn read_saved_file() -> HashMap<String, SavedSync> {
    std::fs::read_to_string(saved_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// CDXC:ProjectBoard 2026-11-11 DECISION:
/// User (settled with the Beads view plan): the last push and pull times are saved, so the header's "Synced 4 min ago" survives a gxserver restart instead of starting over at "Not synced yet".
pub(crate) fn load_saved(entry: &WorkspaceEntry) {
    let mut state = entry.state();
    if state.sync.loaded {
        return;
    }
    let _file = file_lock().lock().expect("beads sync file");
    state.sync.saved = read_saved_file().remove(&entry.path).unwrap_or_default();
    state.sync.loaded = true;
}

fn save(entry: &WorkspaceEntry, saved: &SavedSync) {
    let _file = file_lock().lock().expect("beads sync file");
    let mut all = read_saved_file();
    all.insert(entry.path.clone(), saved.clone());
    let path = saved_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&all) {
        Ok(text) => {
            if let Err(error) = std::fs::write(&path, text) {
                super::log::warn("beadsView.syncSaveFailed", &error.to_string());
            }
        }
        Err(error) => super::log::warn("beadsView.syncSaveFailed", &error.to_string()),
    }
}

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// A push can take longer than a client's one-minute call limit while gxserver allows two minutes, so the client reported a failure for a push that went through. Pull and Push now answer at once and run in the background under one two-minute deadline that includes waiting for the workspace; clients follow `sync.running` and `sync.lastResult` in their `/api/beadsView` polls, which never wait on the lock a sync holds.
pub(crate) fn start_sync(
    entry: Arc<WorkspaceEntry>,
    info: WorkspaceInfo,
    action: &str,
    context: TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    load_saved(&entry);
    let kind = if action == "doltPush" { "push" } else { "pull" };
    let command = beads_action_command(action, &Map::new(), &context)?;
    {
        let mut state = entry.state();
        if state.sync.running.is_some() {
            return Ok(
                json!({ "action": action, "alreadyRunning": true, "exitCode": 0, "stderr": "", "stdout": "" }),
            );
        }
        state.sync.running = Some(BeadsSyncRunning {
            kind: kind.to_string(),
            started_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        });
    }
    let kind = kind.to_string();
    tokio::spawn(async move {
        let started = Instant::now();
        let result = match entry.lock_within(SYNC_DEADLINE).await {
            None => timeout_result(&kind),
            Some(guard) => {
                let left = SYNC_DEADLINE.saturating_sub(started.elapsed());
                super::log::bd_call("sync", &kind);
                let output = run_process_command_with_timeout(&command, &context, left).await;
                drop(guard);
                match output {
                    Ok(output) => classify(
                        &kind,
                        output.exit_code,
                        &output.stdout,
                        &output.stderr,
                        output.error.as_ref(),
                    ),
                    Err(error) => BeadsSyncResult {
                        kind: kind.clone(),
                        ok: false,
                        code: "other".to_string(),
                        message: error.message.clone(),
                        output: error.message,
                        at: now(),
                    },
                }
            }
        };
        let _ = &info;
        let saved = {
            let mut state = entry.state();
            state.sync.running = None;
            if result.ok {
                if kind == "push" {
                    state.sync.saved.last_push_at = Some(result.at.clone());
                } else {
                    state.sync.saved.last_pull_at = Some(result.at.clone());
                }
            }
            state.sync.saved.last_result = Some(result);
            state.reads.clear();
            state.written = true;
            state.remote = None;
            state.sync.saved.clone()
        };
        save(&entry, &saved);
    });
    Ok(json!({ "action": action, "started": true, "exitCode": 0, "stderr": "", "stdout": "" }))
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn timeout_result(kind: &str) -> BeadsSyncResult {
    BeadsSyncResult {
        kind: kind.to_string(),
        ok: false,
        code: "timeout".to_string(),
        message: "Beads stayed busy, so the sync did not start in time.".to_string(),
        output: String::new(),
        at: now(),
    }
}

/// Names what went wrong from bd's own words; the card copy lives in the client.
fn classify(
    kind: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    error: Option<&Value>,
) -> BeadsSyncResult {
    let output = [stderr.trim(), stdout.trim()]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let error_code = error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str);
    let lower = output.to_lowercase();
    // bd exits 0 with "No remote is configured — skipping." when there is nothing to sync with.
    let no_remote = lower.contains("no remote is configured")
        || lower.contains("no remote")
        || lower.contains("remote not found")
        || lower.contains("no remotes")
        || lower.contains("no dolt remote");
    let code = if no_remote {
        "noRemote"
    } else if exit_code == 0 && error.is_none() {
        "ok"
    } else if error_code == Some("timeout") {
        "timeout"
    } else if lower.contains("conflict") {
        "conflict"
    } else if lower.contains("non-fast-forward")
        || lower.contains("not fast forward")
        || lower.contains("fetch first")
        || lower.contains("rejected")
        || lower.contains("behind")
        || lower.contains("diverged")
    {
        "notFastForward"
    } else if lower.contains("permission denied")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
    {
        "auth"
    } else if lower.contains("could not resolve")
        || lower.contains("connection refused")
        || lower.contains("network")
        || lower.contains("no such host")
        || lower.contains("timed out")
    {
        "network"
    } else {
        "other"
    };
    BeadsSyncResult {
        kind: kind.to_string(),
        ok: code == "ok",
        code: code.to_string(),
        message: [stderr, stdout]
            .into_iter()
            .find_map(|text| {
                text.lines()
                    .rev()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
            })
            .unwrap_or_default()
            .to_string(),
        output,
        at: now(),
    }
}
