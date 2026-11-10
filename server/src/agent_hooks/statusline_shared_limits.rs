//! The newest 5h / 7d reading from any Claude session on one login.
//!
//! CDXC:AgentProviders 2026-11-09 WHY:
//! GitHub #514: without a saved account a session's limits are the numbers its own Claude last piped to the statusline command. A coordinator that sits idle for hours reports nothing new, so its chat kept showing the percentages from before the window reset even while other sessions on the same login had reported fresh ones. The limits belong to the login, not to the session, so a session's reading takes each window from the newest payload any session on its login stored, when that payload is newer than its own.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

use serde_json::Value;

use super::statusline::{
    read_statusline_payload_file, ClaudeStatuslinePayload, CLAUDE_STATUSLINE_STATE_DIRECTORY,
};

const RATE_LIMIT_WINDOWS: [&str; 2] = ["five_hour", "seven_day"];

/// The longest window is 7 days; a payload older than that cannot carry a reading worth taking.
const NEWER_PAYLOAD_MAX_AGE: Duration = Duration::from_secs(8 * 24 * 60 * 60);

/// How long the newest reading per login is reused before the folder is listed again.
const RESCAN_INTERVAL: Duration = Duration::from_secs(10);

/// A temporary payload is renamed into place within milliseconds of being written; one this old was
/// left by a statusline command that died between the write and the rename.
const TEMP_LEFTOVER_AGE: Duration = Duration::from_secs(10 * 60);

/// One window's reading and when the payload that carried it was written.
#[derive(Clone, Copy)]
struct WindowReading {
    used: f64,
    resets_at: f64,
    modified: SystemTime,
}

/// What one payload file version says about the shared limits.
struct PayloadSummary {
    modified: SystemTime,
    login: Option<String>,
    windows: [Option<(f64, f64)>; 2],
}

/*
CDXC:AgentProviders 2026-11-10 WHY:
The #514 lookup was 15% of gxserver's busy time: every working chat's status probe ran it each
second, listing and stat-ing the whole payload folder (954 files here) and parsing account files
for logins on every call. The folder is now indexed in memory: each payload file version is read
once (by path and modification time), and the newest reading per login is rebuilt at most every
RESCAN_INTERVAL. A session's own payload is still read fresh on every probe, so its own limits show
at once; a newer reading another session on the same login wrote shows up to about 10 seconds late.
*/
#[derive(Default)]
struct FolderIndex {
    scanned_at: Option<Instant>,
    files: HashMap<PathBuf, PayloadSummary>,
    newest_by_login: HashMap<String, [Option<WindowReading>; 2]>,
}

static FOLDER_INDEXES: OnceLock<Mutex<HashMap<PathBuf, FolderIndex>>> = OnceLock::new();

/// The login a payload's session ran on: the config folder its transcript lives in (the folder that holds `projects/`) names it.
fn payload_login(
    payload: &serde_json::Map<String, Value>,
    home: &Path,
    cache: &mut HashMap<PathBuf, Option<String>>,
) -> Option<String> {
    let transcript = Path::new(payload.get("transcript_path")?.as_str()?);
    if !transcript.is_absolute() {
        return None;
    }
    let root = transcript
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "projects"))?
        .parent()?
        .to_path_buf();
    cache
        .entry(root.clone())
        .or_insert_with(|| {
            crate::accounts::session_identity::login_identity(
                crate::accounts::model::Provider::Claude,
                &root,
                home,
            )
        })
        .clone()
}

fn window_reading(payload: &serde_json::Map<String, Value>, key: &str) -> Option<(f64, f64)> {
    let window = payload.get("rate_limits")?.get(key)?;
    let used = window.get("used_percentage")?.as_f64()?;
    let resets_at = window.get("resets_at")?.as_f64()?;
    (used.is_finite() && resets_at.is_finite()).then_some((used, resets_at))
}

/// Replaces `own`'s 5h / 7d windows with the newest same-login reading, window by window, from the
/// folder index (rebuilt at most every [`RESCAN_INTERVAL`]).
pub(crate) fn adopt_same_login_rate_limits(
    hook_state_directory: &Path,
    own: &mut ClaudeStatuslinePayload,
) {
    let Some(own_modified) = own.modified else {
        return;
    };
    let home = ghostex_paths::user_home_dir();
    let Some(own_login) = payload_login(&own.payload, &home, &mut HashMap::new()) else {
        return;
    };
    let newest = {
        let Ok(mut indexes) = FOLDER_INDEXES.get_or_init(Default::default).lock() else {
            return;
        };
        let index = indexes
            .entry(hook_state_directory.to_path_buf())
            .or_default();
        if index
            .scanned_at
            .is_none_or(|scanned_at| scanned_at.elapsed() >= RESCAN_INTERVAL)
        {
            rescan_folder(
                index,
                &hook_state_directory.join(CLAUDE_STATUSLINE_STATE_DIRECTORY),
                &home,
            );
        }
        index.newest_by_login.get(&own_login).copied()
    };
    let Some(newest) = newest else {
        return;
    };
    for (key, reading) in RATE_LIMIT_WINDOWS.iter().zip(newest) {
        let Some(reading) = reading.filter(|reading| reading.modified > own_modified) else {
            continue;
        };
        // A window only moves forward: a reading of an earlier window than the one this session already knows is not newer news.
        if window_reading(&own.payload, key).is_some_and(|(_, known)| reading.resets_at < known) {
            continue;
        }
        let Some(limits) = own
            .payload
            .entry("rate_limits")
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
        else {
            continue;
        };
        limits.insert(
            key.to_string(),
            serde_json::json!({"used_percentage": reading.used, "resets_at": reading.resets_at}),
        );
    }
}

/// Lists the payload folder once: reads only payload files that are new or changed since the last
/// listing, forgets the ones that are gone or older than [`NEWER_PAYLOAD_MAX_AGE`], removes
/// temporary files a dead write left behind, and rebuilds the newest reading per login.
fn rescan_folder(index: &mut FolderIndex, folder: &Path, home: &Path) {
    index.scanned_at = Some(Instant::now());
    let Ok(entries) = fs::read_dir(folder) else {
        index.files.clear();
        index.newest_by_login.clear();
        return;
    };
    let now = SystemTime::now();
    let age = |modified: SystemTime| now.duration_since(modified).unwrap_or_default();
    let mut logins = HashMap::new();
    let mut files = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("tmp") => {
                if age(modified) > TEMP_LEFTOVER_AGE {
                    let _ = fs::remove_file(&path);
                }
                continue;
            }
            Some("json") if age(modified) <= NEWER_PAYLOAD_MAX_AGE => {}
            _ => continue,
        }
        let summary = match index.files.remove(&path) {
            Some(known) if known.modified == modified => known,
            _ => {
                let Some(payload) = read_statusline_payload_file(&path) else {
                    continue;
                };
                PayloadSummary {
                    // The listing's time, taken before the text was read: a write renamed into
                    // place in between pairs newer text with the older time, which only makes the
                    // next rescan read the file again. The time looked up after the read could pair
                    // the old text with the new time and keep it until the session writes again.
                    modified,
                    login: payload_login(&payload.payload, home, &mut logins),
                    windows: RATE_LIMIT_WINDOWS.map(|key| window_reading(&payload.payload, key)),
                }
            }
        };
        files.insert(path, summary);
    }
    let mut newest_by_login: HashMap<String, [Option<WindowReading>; 2]> = HashMap::new();
    for summary in files.values() {
        let Some(login) = &summary.login else {
            continue;
        };
        let newest = newest_by_login.entry(login.clone()).or_default();
        for (slot, window) in newest.iter_mut().zip(summary.windows) {
            let Some((used, resets_at)) = window else {
                continue;
            };
            if slot.is_none_or(|known| summary.modified > known.modified) {
                *slot = Some(WindowReading {
                    used,
                    resets_at,
                    modified: summary.modified,
                });
            }
        }
    }
    index.files = files;
    index.newest_by_login = newest_by_login;
}
