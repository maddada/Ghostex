//! Ghostex's own record of when a Claude process started on each conversation, in place of the
//! `SessionStart` hook lines the readers used to look for in the transcript.
//!
//! CDXC:AgentHooks 2026-11-02 DECISION:
//! User, asked how to close the gaps that keep Claude's hooks installed: "Everything, including Windows". The background-shell roster, the fleet view's turn boundary and the session details panel learned that a new Claude process took a conversation over (its old shells died with the old process) from the `SessionStart:startup|resume|clear` attachment lines Claude writes for Ghostex's hooks, so a session with no hooks never reset. gxserver now writes `<state>/claude-process-starts/<conversation>.json` itself from Claude's session records, whether or not Track Claude without its hooks is on and whether or not the hooks are installed, and the readers take both: these records and the old hook lines, so old transcripts read the same.
//!
//! CDXC:AgentHooks 2026-11-02 WHY:
//! A process seen for the first time started at its record's `startedAt`, which is before any line it writes. A process that moved to another conversation (/resume, /clear) is dated at the pass that last saw it on the old one, at most a poll earlier than the switch and so still before anything it writes there. The source follows Claude's own: a conversation on disk that does not open with /clear is a resume, an in-process move to anything else is a clear, a new process on an unwritten conversation is a startup.
//! SEE-ALSO: server/src/session_chat_claude_fleet.rs, server/src/session_chat_claude_background.rs and server/src/session_details/scan.rs (the readers).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use super::attach::ATTACH_CLIENT;
use super::claude::{opened_by_clear, text, transcript_path, LiveRecord};

/// Starts kept per conversation, newest last.
const KEPT: usize = 64;
/// A conversation's record is dropped once no process started on it for this long.
const RETAIN: Duration = Duration::from_secs(60 * 60 * 24 * 30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClaudeStartSource {
    Startup,
    Resume,
    Clear,
}

impl ClaudeStartSource {
    fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Resume => "resume",
            Self::Clear => "clear",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "startup" => Some(Self::Startup),
            "resume" => Some(Self::Resume),
            "clear" => Some(Self::Clear),
            _ => None,
        }
    }

    /// A new process (a startup or a resume) has none of the old process's shells; a /clear
    /// keeps the process.
    pub(crate) fn new_process(self) -> bool {
        !matches!(self, Self::Clear)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeProcessStart {
    /// Ms since the epoch.
    pub at: i64,
    pub source: ClaudeStartSource,
}

/// The source a `SessionStart:<source>` hook line names, for the readers that still take them.
pub(crate) fn hook_line_start(record: &Value) -> Option<ClaudeStartSource> {
    record
        .pointer("/attachment/hookName")
        .and_then(Value::as_str)?
        .strip_prefix("SessionStart:")
        .and_then(ClaudeStartSource::parse)
}

fn store_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("claude-process-starts")
}

fn store_file(state_dir: &Path, conversation: &str) -> Option<PathBuf> {
    let safe = !conversation.is_empty()
        && conversation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    safe.then(|| store_dir(state_dir).join(format!("{conversation}.json")))
}

fn read_entries(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|data| data.get("starts").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

/// Every process start recorded for a conversation, oldest first.
pub(crate) fn read_claude_process_starts(conversation: &str) -> Vec<ClaudeProcessStart> {
    if cfg!(test) {
        return Vec::new();
    }
    let state_dir = crate::paths::get_gxserver_paths(None).app_state_dir;
    store_file(&state_dir, conversation)
        .map(|path| parse_starts(&read_entries(&path)))
        .unwrap_or_default()
}

fn parse_starts(entries: &[Value]) -> Vec<ClaudeProcessStart> {
    let mut starts: Vec<_> = entries
        .iter()
        .filter_map(|entry| {
            Some(ClaudeProcessStart {
                at: entry["at"].as_i64()?,
                source: ClaudeStartSource::parse(entry["source"].as_str()?)?,
            })
        })
        .collect();
    starts.sort_by_key(|start| start.at);
    starts
}

/// How far a reader has applied a conversation's starts, so each is applied once, in transcript
/// order: before the first record written at or after it.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessStartCursor {
    through: Option<i64>,
}

impl ProcessStartCursor {
    /// The starts after the cursor and no later than `upto`, oldest first, moving the cursor on.
    pub(crate) fn due(
        &mut self,
        starts: &[ClaudeProcessStart],
        upto: i64,
    ) -> Vec<ClaudeProcessStart> {
        let due: Vec<_> = starts
            .iter()
            .filter(|start| {
                self.through.is_none_or(|through| start.at > through) && start.at <= upto
            })
            .copied()
            .collect();
        if let Some(last) = due.last() {
            self.through = Some(last.at);
        }
        due
    }
}

/// Writes a start for every `claude` that began showing a conversation since the last pass.
#[derive(Default)]
pub(super) struct ProcessStartRecorder {
    /// The conversation each live pid showed on the last pass, and when that pass ran.
    shown: HashMap<u32, (String, i64)>,
    pruned: bool,
}

impl ProcessStartRecorder {
    pub(super) fn record(&mut self, state_dir: &Path, live: &[LiveRecord]) {
        if !self.pruned {
            self.pruned = true;
            prune(state_dir);
        }
        let now = chrono::Utc::now().timestamp_millis();
        let mut seen = HashMap::new();
        for LiveRecord { config_dir, record } in live {
            if record.get(ATTACH_CLIENT).is_some() || record["spare"] == true {
                continue;
            }
            let (Some(pid), Some(conversation)) = (
                record["pid"]
                    .as_u64()
                    .and_then(|pid| u32::try_from(pid).ok()),
                text(record, "sessionId"),
            ) else {
                continue;
            };
            let previous = self.shown.get(&pid).cloned();
            let first_sight = previous.is_none();
            seen.insert(pid, (conversation.clone(), now));
            let transcript =
                || transcript_path(config_dir, text(record, "cwd").as_deref(), &conversation);
            let start = match previous {
                Some((shown, _)) if shown == conversation => continue,
                // The same process moved on: /resume to a written conversation, else /clear.
                Some((_, last_seen)) => ClaudeProcessStart {
                    at: last_seen + 1,
                    source: match transcript() {
                        Some(path) if !opened_by_clear(&path) => ClaudeStartSource::Resume,
                        _ => ClaudeStartSource::Clear,
                    },
                },
                None => ClaudeProcessStart {
                    at: record["startedAt"].as_i64().unwrap_or(now),
                    source: if transcript().is_some() {
                        ClaudeStartSource::Resume
                    } else {
                        ClaudeStartSource::Startup
                    },
                },
            };
            write_start(state_dir, &conversation, pid, start, first_sight);
        }
        self.shown = seen;
    }
}

/// Adds one start to a conversation's record. A process seen for the first time that is already
/// recorded on it (gxserver restarted under a running `claude`) is not added again.
fn write_start(
    state_dir: &Path,
    conversation: &str,
    pid: u32,
    start: ClaudeProcessStart,
    first_sight: bool,
) {
    let Some(path) = store_file(state_dir, conversation) else {
        return;
    };
    let mut entries = read_entries(&path);
    if entries.iter().any(|entry| {
        entry["pid"].as_u64() == Some(u64::from(pid))
            && (first_sight || entry["at"].as_i64() == Some(start.at))
    }) {
        return;
    }
    entries.push(json!({"at": start.at, "pid": pid, "source": start.source.name()}));
    entries.sort_by_key(|entry| entry["at"].as_i64().unwrap_or_default());
    if entries.len() > KEPT {
        entries.drain(..entries.len() - KEPT);
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if std::fs::write(&temp, json!({"version": 1, "starts": entries}).to_string()).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

fn prune(state_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(store_dir(state_dir)) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > RETAIN);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pid: u32, conversation: &str, started_at: i64, cwd: &Path) -> Value {
        json!({"pid": pid, "sessionId": conversation, "kind": "interactive", "status": "idle", "startedAt": started_at, "cwd": cwd})
    }

    fn starts(dir: &Path, conversation: &str) -> Vec<(i64, ClaudeStartSource)> {
        parse_starts(&read_entries(&store_file(dir, conversation).unwrap()))
            .into_iter()
            .map(|start| (start.at, start.source))
            .collect()
    }

    #[test]
    fn each_process_start_on_a_conversation_is_recorded_once() {
        let dir = std::env::temp_dir().join(format!("gx-starts-{}", uuid::Uuid::new_v4()));
        let cwd = dir.join("project");
        let config = dir.join("config");
        let (first, second) = (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        let live = |pid: u32, conversation: &str, started_at: i64| {
            vec![LiveRecord {
                config_dir: config.clone(),
                record: record(pid, conversation, started_at, &cwd),
            }]
        };
        let mut recorder = ProcessStartRecorder::default();
        recorder.record(&dir, &live(100, &first, 1_000));
        recorder.record(&dir, &live(100, &first, 1_000));
        assert_eq!(
            starts(&dir, &first),
            vec![(1_000, ClaudeStartSource::Startup)]
        );

        // gxserver restarted under the same process: nothing new.
        let mut restarted = ProcessStartRecorder::default();
        restarted.record(&dir, &live(100, &first, 1_000));
        assert_eq!(starts(&dir, &first).len(), 1);

        // /clear in the same process, onto a conversation not written yet.
        restarted.record(&dir, &live(100, &second, 1_000));
        let cleared = starts(&dir, &second);
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].1, ClaudeStartSource::Clear);
        assert!(cleared[0].0 > 1_000);

        // A new process resumes the first conversation, now on disk.
        let folder = cwd
            .to_string_lossy()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let transcript = config
            .join("projects")
            .join(folder)
            .join(format!("{first}.jsonl"));
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "{\"type\":\"user\"}\n").unwrap();
        restarted.record(&dir, &live(200, &first, 9_000));
        assert_eq!(
            starts(&dir, &first),
            vec![
                (1_000, ClaudeStartSource::Startup),
                (9_000, ClaudeStartSource::Resume)
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_cursor_hands_each_start_out_once_in_order() {
        let start = |at, source| ClaudeProcessStart { at, source };
        let starts = [
            start(10, ClaudeStartSource::Startup),
            start(20, ClaudeStartSource::Clear),
            start(30, ClaudeStartSource::Resume),
        ];
        let mut cursor = ProcessStartCursor::default();
        assert_eq!(cursor.due(&starts, 5), vec![]);
        assert_eq!(cursor.due(&starts, 20), starts[..2].to_vec());
        assert_eq!(cursor.due(&starts, 25), vec![]);
        assert_eq!(cursor.due(&starts, i64::MAX), starts[2..].to_vec());
        assert_eq!(cursor.due(&starts, i64::MAX), vec![]);
    }

    #[test]
    fn hook_lines_still_name_their_source() {
        let line = |name: &str| json!({"type": "attachment", "attachment": {"hookName": name}});
        assert_eq!(
            hook_line_start(&line("SessionStart:resume")),
            Some(ClaudeStartSource::Resume)
        );
        assert_eq!(
            hook_line_start(&line("SessionStart:clear")),
            Some(ClaudeStartSource::Clear)
        );
        assert_eq!(hook_line_start(&line("PostToolUse:Bash")), None);
    }
}
