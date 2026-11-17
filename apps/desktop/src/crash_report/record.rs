//! The crash record a dying Ghostex leaves on disk, and the small state file that says which
//! crashes were already handled.
//!
//! Records live in `<logs>/crash-reports/`: one `pending-<ms>.json` per crash until the crash
//! window (or the rollback message) sends or drops it, `sent.json` with the crashes this computer
//! already reported (so the same crash is not asked about twice), and `state.json` with how far
//! the Ghostty crash folder was read.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Lines of Ghostex's own log a record keeps.
pub(crate) const LOG_TAIL_LINES: usize = 40;
/// Longest log line kept; the rest of the line is cut.
const LOG_LINE_MAX_CHARS: usize = 300;
/// Frames of a stack a record keeps.
pub(crate) const STACK_MAX_FRAMES: usize = 40;
/// Crashes `sent.json` remembers.
const SENT_MAX_ENTRIES: usize = 50;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CrashKind {
    /// A Rust panic on the main thread, which ends the app.
    #[default]
    Panic,
    /// An Objective-C exception nothing caught (macOS).
    UncaughtException,
    /// The process was stopped by the system (a bad memory access, an abort): read back from the
    /// crash file Ghostty's crash handler wrote (macOS).
    Native,
}

/// One crash, as saved by the crashing process (or, for a native crash, at the next open).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct CrashRecord {
    pub(crate) kind: CrashKind,
    /// The Ghostex version that crashed.
    pub(crate) version: String,
    pub(crate) commit: Option<String>,
    /// When it crashed, in milliseconds since 1970 (UTC).
    pub(crate) at_ms: u64,
    /// How long after opening, in milliseconds; `None` when unknown (a native crash).
    pub(crate) uptime_ms: Option<u64>,
    /// `src/app/native_chat/list.rs:488`, `NSInternalInconsistencyException`, `Ghostex+0x723e61c`.
    pub(crate) location: String,
    /// The panic message, the exception reason, or what the system stopped it for.
    pub(crate) message: String,
    pub(crate) stack: Vec<String>,
    pub(crate) log_tail: Vec<String>,
}

impl CrashRecord {
    /// What makes two crashes the same one: the version, the kind and where it happened (line
    /// numbers included; the message is left out because it often holds ids or sizes).
    pub(crate) fn fingerprint(&self) -> String {
        let key = format!("{}|{:?}|{}", self.version, self.kind, self.location);
        // FNV-1a: stable across builds and platforms, unlike std's hasher.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in key.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("gxc-{:012x}", hash & 0xffff_ffff_ffff)
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

pub(crate) fn crash_reports_dir() -> PathBuf {
    super::logs_directory().join("crash-reports")
}

fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(path)?, text.as_bytes())
}

/// Saves `record` as a pending crash. Called from the panic hook, so it never panics itself.
pub(crate) fn save_pending(record: &CrashRecord) {
    let Ok(text) = serde_json::to_string_pretty(record) else {
        return;
    };
    let path = crash_reports_dir().join(format!("pending-{}.json", record.at_ms));
    if let Err(error) = write_private(&path, &text) {
        eprintln!("Ghostex could not save the crash record: {error}");
    }
}

/// The pending crashes, newest first, with their files.
pub(crate) fn pending() -> Vec<(PathBuf, CrashRecord)> {
    let Ok(entries) = fs::read_dir(crash_reports_dir()) else {
        return Vec::new();
    };
    let mut records: Vec<(PathBuf, CrashRecord)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("pending-") && name.ends_with(".json"))
        })
        .filter_map(|path| {
            let record = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
            Some((path, record))
        })
        .collect();
    records.sort_by_key(|(_, record)| std::cmp::Reverse(record.at_ms));
    records
}

/// Drops every pending crash (after Send report or Don't send) and remembers them as handled.
pub(crate) fn clear_pending() {
    let pending = pending();
    if pending.is_empty() {
        return;
    }
    let mut state = reader_state();
    for (path, record) in pending {
        state.handled_at_ms.insert(0, record.at_ms);
        let _ = fs::remove_file(path);
    }
    state.handled_at_ms.truncate(HANDLED_MAX_ENTRIES);
    save_reader_state(&state);
}

/// A crash this computer already reported.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SentCrash {
    pub(crate) fingerprint: String,
    pub(crate) issue_number: u64,
    pub(crate) issue_url: String,
    pub(crate) at_ms: u64,
}

fn sent_path() -> PathBuf {
    crash_reports_dir().join("sent.json")
}

pub(crate) fn sent() -> Vec<SentCrash> {
    fs::read_to_string(sent_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn sent_issue(fingerprint: &str) -> Option<SentCrash> {
    sent()
        .into_iter()
        .find(|sent| sent.fingerprint == fingerprint)
}

pub(crate) fn remember_sent(entry: SentCrash) {
    let mut all = sent();
    all.retain(|sent| sent.fingerprint != entry.fingerprint);
    all.insert(0, entry);
    all.truncate(SENT_MAX_ENTRIES);
    if let Ok(text) = serde_json::to_string_pretty(&all) {
        let _ = write_private(&sent_path(), &text);
    }
}

/// How far the Ghostty crash folder was read.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ReaderState {
    /// Crash files written at or before this time (ms since 1970) were already looked at.
    pub(crate) native_crashes_read_until_ms: Option<u64>,
    /// When the crashes already sent or dismissed happened. Ghostty's handler writes its file for
    /// a crash at the next launch, after the panic record of that same crash may have been
    /// handled, so this is what keeps it from being offered a second time.
    pub(crate) handled_at_ms: Vec<u64>,
}

/// Crash times `handled_at_ms` keeps.
const HANDLED_MAX_ENTRIES: usize = 50;

fn state_path() -> PathBuf {
    crash_reports_dir().join("state.json")
}

pub(crate) fn reader_state() -> ReaderState {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn save_reader_state(state: &ReaderState) {
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = write_private(&state_path(), &text);
    }
}

/// The last [`LOG_TAIL_LINES`] lines of the app's own log (`gpui-host-lifecycle.log`, the
/// warnings and errors it always keeps). Its writer already replaced paths and long values.
pub(crate) fn log_tail() -> Vec<String> {
    let path = super::logs_directory().join("gpui-host-lifecycle.log");
    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    // The tail of a 25 MB file: the last 64 KB holds far more than 40 lines.
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = length.saturating_sub(64 * 1024);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    // The first line may start mid-way.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(LOG_TAIL_LINES);
    lines[skip..]
        .iter()
        .map(|line| {
            let mut kept: String = line.chars().take(LOG_LINE_MAX_CHARS).collect();
            if line.chars().count() > LOG_LINE_MAX_CHARS {
                kept.push_str(" [cut]");
            }
            kept
        })
        .collect()
}
