//! The furthest-along 5h / 7d reading from any Claude session on one login.
//!
//! CDXC:AgentProviders 2026-11-09 WHY:
//! GitHub #514: without a saved account a session's limits are the numbers its own Claude last piped to the statusline command. A coordinator that sits idle for hours reports nothing new, so its chat kept showing the percentages from before the window reset even while other sessions on the same login had reported fresh ones. The limits belong to the login, not to the session, so a session's reading takes each window from the furthest-along reading any session on its login stored, when that reading is further along than its own.
//!
//! CDXC:AgentProviders 2026-11-13 DECISION:
//! The user saw the chat's usage bar flip between 7d 26% and 7d 19% ("the bar keeps switching ... conflicting information"). The shared reading is chosen by its window, never by payload file time: a later `resets_at` wins, and inside the same window the higher `used_percentage` wins, because usage never falls inside a window. Idle sessions rewrite their payload every few seconds with old rate-limit headers, so file time picked a stale lower percentage whenever an idle session wrote last. A session never swaps its own reading for one that is not further along. `resets_at` is handed over as a whole number (epoch seconds) because the status mapper in `session_chat_options/selection_sources.rs` reads it as an integer and drops the reset row for a float.

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

/// One window's reading.
#[derive(Clone, Copy)]
struct WindowReading {
    used: f64,
    resets_at: f64,
}

impl WindowReading {
    /// Whether this reading is further along than `known`: a later window, or the same window with more used.
    fn supersedes(self, known: Self) -> bool {
        self.resets_at > known.resets_at
            || (self.resets_at == known.resets_at && self.used > known.used)
    }
}

/// What one payload file version says about the shared limits.
struct PayloadSummary {
    modified: SystemTime,
    login: Option<String>,
    windows: [Option<WindowReading>; 2],
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

fn window_reading(payload: &serde_json::Map<String, Value>, key: &str) -> Option<WindowReading> {
    let window = payload.get("rate_limits")?.get(key)?;
    let used = window.get("used_percentage")?.as_f64()?;
    let resets_at = window.get("resets_at")?.as_f64()?;
    (used.is_finite() && resets_at.is_finite()).then_some(WindowReading { used, resets_at })
}

/// Replaces `own`'s 5h / 7d windows with the furthest-along same-login reading, window by window,
/// from the folder index (rebuilt at most every [`RESCAN_INTERVAL`]).
pub(crate) fn adopt_same_login_rate_limits(
    hook_state_directory: &Path,
    own: &mut ClaudeStatuslinePayload,
) {
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
        let Some(reading) = reading else {
            continue;
        };
        // A window only moves forward, and usage never falls inside one: a reading that is not further along than the session's own is not news.
        if window_reading(&own.payload, key).is_some_and(|known| !reading.supersedes(known)) {
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
            // Epoch seconds as a whole number: the status mapper reads `resets_at` as an integer and drops a float.
            serde_json::json!({
                "used_percentage": reading.used,
                "resets_at": reading.resets_at.round() as i64,
            }),
        );
    }
}

/// Lists the payload folder once: reads only payload files that are new or changed since the last
/// listing, forgets the ones that are gone or older than [`NEWER_PAYLOAD_MAX_AGE`], removes
/// temporary files a dead write left behind, and rebuilds the furthest-along reading per login.
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
            let Some(reading) = window else {
                continue;
            };
            if slot.is_none_or(|known| reading.supersedes(known)) {
                *slot = Some(reading);
            }
        }
    }
    index.files = files;
    index.newest_by_login = newest_by_login;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    const RESET: i64 = 1_795_078_800;

    /// Writes a login's account file and one session payload with the given 7d reading.
    fn write_payload(
        hooks: &Path,
        root: &Path,
        session: &str,
        used: f64,
        resets_at: i64,
        age: Duration,
    ) {
        let folder = hooks.join(CLAUDE_STATUSLINE_STATE_DIRECTORY);
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("{session}.json"));
        let payload = json!({
            "transcript_path": root.join("projects/p/t.jsonl"),
            "rate_limits": {"seven_day": {"used_percentage": used, "resets_at": resets_at}},
        });
        fs::write(
            &path,
            json!({"payload": payload, "updatedAt": ""}).to_string(),
        )
        .unwrap();
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    fn own_payload(hooks: &Path, session: &str) -> ClaudeStatuslinePayload {
        read_statusline_payload_file(
            &hooks
                .join(CLAUDE_STATUSLINE_STATE_DIRECTORY)
                .join(format!("{session}.json")),
        )
        .unwrap()
    }

    fn seven_day(payload: &ClaudeStatuslinePayload) -> Value {
        payload.payload["rate_limits"]["seven_day"].clone()
    }

    fn login_root(base: &Path) -> PathBuf {
        let root = base.join("claude-login");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join(".claude.json"),
            json!({"oauthAccount": {"emailAddress": "a@example.test", "organizationUuid": "org"}})
                .to_string(),
        )
        .unwrap();
        root
    }

    #[test]
    fn a_later_window_wins_then_the_higher_percentage() {
        let reading = |used, resets_at| WindowReading { used, resets_at };
        assert!(reading(1.0, 200.0).supersedes(reading(90.0, 100.0)));
        assert!(!reading(90.0, 100.0).supersedes(reading(1.0, 200.0)));
        assert!(reading(26.0, 100.0).supersedes(reading(19.0, 100.0)));
        assert!(!reading(19.0, 100.0).supersedes(reading(26.0, 100.0)));
        assert!(!reading(26.0, 100.0).supersedes(reading(26.0, 100.0)));
    }

    #[test]
    fn an_idle_session_written_later_does_not_lower_a_busy_sessions_reading() {
        let base = tempfile::tempdir().unwrap();
        let root = login_root(base.path());
        let hooks = base.path().join("hooks");
        // The busy session wrote 26% ten minutes ago; the idle one wrote 19% just now.
        write_payload(&hooks, &root, "busy", 26.0, RESET, Duration::from_secs(600));
        write_payload(&hooks, &root, "idle", 19.0, RESET, Duration::from_secs(1));
        write_payload(&hooks, &root, "mine", 5.0, RESET, Duration::from_secs(3600));

        let mut mine = own_payload(&hooks, "mine");
        adopt_same_login_rate_limits(&hooks, &mut mine);
        let window = seven_day(&mine);
        assert_eq!(window["used_percentage"], json!(26.0));
        // A whole number, so the status mapper keeps the reset row.
        assert_eq!(window["resets_at"].as_i64(), Some(RESET));

        // A session already at 26% is not pulled down to the idle session's 19%.
        let mut busy = own_payload(&hooks, "busy");
        adopt_same_login_rate_limits(&hooks, &mut busy);
        assert_eq!(seven_day(&busy)["used_percentage"], json!(26.0));
    }

    #[test]
    fn a_later_window_replaces_the_own_reading_whatever_the_percentage() {
        let base = tempfile::tempdir().unwrap();
        let root = login_root(base.path());
        let hooks = base.path().join("hooks");
        write_payload(
            &hooks,
            &root,
            "fresh",
            2.0,
            RESET + 604_800,
            Duration::from_secs(5),
        );
        write_payload(&hooks, &root, "mine", 80.0, RESET, Duration::from_secs(5));

        let mut mine = own_payload(&hooks, "mine");
        adopt_same_login_rate_limits(&hooks, &mut mine);
        let window = seven_day(&mine);
        assert_eq!(window["used_percentage"], json!(2.0));
        assert_eq!(window["resets_at"].as_i64(), Some(RESET + 604_800));
    }

    #[test]
    fn a_handed_over_reading_keeps_its_reset_row_in_the_chat_status() {
        let base = tempfile::tempdir().unwrap();
        let root = login_root(base.path());
        let hooks = base.path().join("hooks");
        write_payload(&hooks, &root, "busy", 26.0, RESET, Duration::from_secs(600));
        write_payload(&hooks, &root, "mine", 5.0, RESET, Duration::from_secs(3600));

        // The chat's status goes through the handover and then the status mapper.
        let (selection, _) =
            crate::session_chat_options::read_claude_statusline_reading(&hooks, "mine", true)
                .expect("a stored payload reads as a selection");
        let window = &selection.claude_status.expect("status")["rateLimits"]["sevenDay"];
        assert_eq!(window["usedPercentage"], json!(26.0));
        assert_eq!(window["resetsAt"], json!(RESET));
    }
}
