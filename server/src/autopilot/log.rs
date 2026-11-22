//! Autopilot's decision log: one line per thing it saw and what it did, kept 30 days, read by
//! Settings > Autopilot (the log and the summary) and `ghostex autopilot log`.

use std::io::Write;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::paths::GxserverPaths;

const LOG_FILE: &str = "autopilot-log.jsonl";
const KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// The log is trimmed of old lines when it grows past this many bytes.
const TRIM_OVER_BYTES: u64 = 4 * 1024 * 1024;

/// What happened after a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// It acted.
    Done,
    /// It tried and the action failed.
    Failed,
    /// It left the session for the user, with a note.
    Parked,
    /// It held everything while this computer was offline, or carried on once it was back (a
    /// line about the computer, not a session: CDXC:OfflineHold in network_status).
    Waited,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub at: String,
    pub project_id: String,
    pub session_id: String,
    /// The session's title and its project's name when it was logged.
    pub title: String,
    pub project: String,
    /// What Autopilot saw, in plain words.
    pub saw: String,
    /// What it did.
    pub did: String,
    /// The rule that decided it (`parked-job-finished`, `hung`, `frozen`, `limit`, `ended`).
    pub rule: String,
    pub outcome: Outcome,
}

fn lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

fn at_ms(entry: &LogEntry) -> i64 {
    chrono::DateTime::parse_from_rfc3339(&entry.at)
        .map(|at| at.timestamp_millis())
        .unwrap_or_default()
}

/// Adds a line, and trims lines older than 30 days once the file grows.
pub fn append(paths: &GxserverPaths, entry: &LogEntry) {
    let _guard = lock().lock().unwrap_or_else(PoisonError::into_inner);
    let _ = std::fs::create_dir_all(&paths.app_state_dir);
    let path = paths.app_state_dir.join(LOG_FILE);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{}", serde_json::to_string(entry).unwrap_or_default());
    }
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > TRIM_OVER_BYTES) {
        let cutoff = chrono::Utc::now().timestamp_millis() - KEEP_MS;
        let kept: Vec<String> = read_all_unlocked(paths)
            .into_iter()
            .filter(|entry| at_ms(entry) >= cutoff)
            .map(|entry| serde_json::to_string(&entry).unwrap_or_default())
            .collect();
        let _ = std::fs::write(&path, kept.join("\n") + "\n");
    }
}

fn read_all_unlocked(paths: &GxserverPaths) -> Vec<LogEntry> {
    std::fs::read_to_string(paths.app_state_dir.join(LOG_FILE))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<LogEntry>(line).ok())
        .collect()
}

/// Every line of the last 30 days, newest first.
pub fn read_recent(paths: &GxserverPaths, limit: usize) -> Vec<LogEntry> {
    let _guard = lock().lock().unwrap_or_else(PoisonError::into_inner);
    let cutoff = chrono::Utc::now().timestamp_millis() - KEEP_MS;
    let mut entries: Vec<LogEntry> = read_all_unlocked(paths)
        .into_iter()
        .filter(|entry| at_ms(entry) >= cutoff)
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}

/// The summary since `since_ms`: actions taken, sessions kept moving, failures and parked ones.
pub fn summary(paths: &GxserverPaths, since_ms: i64) -> Value {
    let _guard = lock().lock().unwrap_or_else(PoisonError::into_inner);
    let entries: Vec<LogEntry> = read_all_unlocked(paths)
        .into_iter()
        .filter(|entry| at_ms(entry) >= since_ms)
        .collect();
    let count = |outcome: Outcome| entries.iter().filter(|e| e.outcome == outcome).count();
    let mut moved: Vec<(String, String)> = entries
        .iter()
        .filter(|e| e.outcome == Outcome::Done)
        .map(|e| (e.project_id.clone(), e.session_id.clone()))
        .collect();
    moved.sort();
    moved.dedup();
    let parked: Vec<Value> = entries
        .iter()
        .rev()
        .filter(|e| e.outcome == Outcome::Parked)
        .take(20)
        .map(|e| json!({ "title": e.title, "project": e.project, "note": e.saw, "at": e.at, "projectId": e.project_id, "sessionId": e.session_id }))
        .collect();
    json!({
        "actions": count(Outcome::Done),
        "failed": count(Outcome::Failed),
        "parkedCount": count(Outcome::Parked),
        "keptMoving": moved.len(),
        "parked": parked,
    })
}
