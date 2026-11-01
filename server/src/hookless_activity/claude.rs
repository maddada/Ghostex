//! Claude Code's own session records (`<config>/sessions/<pid>.json`), turned into the hook events
//! Claude's hooks send.
//!
//! CDXC:AgentHooks 2026-11-01 WHY:
//! Claude Code writes one record per running `claude` (since 2.1.172) with its conversation id and a `status`: `busy` while a turn runs (`isLoading` or delegated work), `idle` at the prompt, and since 2.1.212 `waiting` with a `waitingFor` while it needs the user (`permission prompt`, `sandbox request`, `worker request` and `goal proposal` are approvals, `input needed` is a question or MCP form, `dialog open` is a local picker such as /model that needs nothing). A terminal showing a conversation hosted by Claude's daemon names it as `parkedJobId`, and the daemon's `kind: bg` record with that `jobId` carries the status. The record says nothing about tools, prompts, subagents, compaction or interrupts, so a finished turn reads the transcript's last entry to tell an interrupt or a manual /compact from a completed turn, and the idle reminder Claude's Notification hook sends a minute after a turn comes from `statusUpdatedAt`.
//! SEE-ALSO: server/src/agent_hooks/claude_daemon.rs (the same records for daemon-hosted hooks), server/src/accounts/claude_resets.rs `ClaudeSlot::session_dir` (cswap profiles, whose `sessions/` hold their own records).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use super::comparison::ActivityEvent;

pub(crate) const OBSERVABLE_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PermissionRequest",
    "Stop",
    "PostCompact",
];

/// A just-started `claude` (younger than this) reports its SessionStart; older ones seen for the
/// first time (gxserver restarted, the comparison turned on) are only a baseline.
const FRESH_START: Duration = Duration::from_secs(20);
/// Claude's Notification hook reminds that it waits for input once it has sat idle this long
/// after a turn (the `idleInput` notification).
const IDLE_REMINDER_MS: i64 = 60_000;
/// A finished turn waits this long for its transcript lines before the last one is read.
const TURN_END_SETTLE: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaneState {
    pid: u32,
    status: String,
    waiting_for: Option<String>,
    conversation: Option<String>,
}

struct TurnEnd {
    event: ActivityEvent,
    conversation: Option<String>,
    config_dir: PathBuf,
    cwd: Option<String>,
    due: Instant,
}

#[derive(Default)]
pub(super) struct ClaudeObserver {
    files: HashMap<PathBuf, (SystemTime, Value)>,
    owners: HashMap<u32, Option<(String, String)>>,
    panes: HashMap<(String, String), PaneState>,
    turn_ends: Vec<TurnEnd>,
    /// When each pane idle after a turn gets Claude's idle reminder (ms since the epoch).
    reminders: HashMap<(String, String), i64>,
}

fn session_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![home.join(".claude").join("sessions")];
    let profiles = crate::accounts::claude_resets::swap_root(home).join("sessions");
    if let Ok(entries) = std::fs::read_dir(profiles) {
        dirs.extend(entries.flatten().map(|entry| entry.path().join("sessions")));
    }
    dirs
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    pid > 1 && unsafe { libc::kill(pid as libc::pid_t, 0) } == 0
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

fn text(record: &Value, key: &str) -> Option<String> {
    record[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The Ghostex pane `S..-P..-G..` a `claude` process hangs below.
fn pane_of(pid: u32) -> Option<(String, String)> {
    let name = crate::agent_hooks::zmx_session_owning_agent(pid)?;
    let mut parts = name.split('-');
    let (Some(_server), Some(project), Some(session), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    (project.starts_with('P') && session.starts_with('G'))
        .then(|| (project.to_string(), session.to_string()))
}

/// What a waiting record waits for, in hook terms.
/// Claude's hooks send PermissionRequest for an approval and for an AskUserQuestion alike (the
/// Notification that follows a few seconds later only repeats it).
fn waiting_event(waiting_for: Option<&str>) -> Option<&'static str> {
    match waiting_for {
        Some(
            "permission prompt" | "sandbox request" | "worker request" | "goal proposal"
            | "input needed",
        ) => Some("PermissionRequest"),
        _ => None,
    }
}

impl ClaudeObserver {
    pub(super) fn poll(&mut self, home: &Path) -> Vec<ActivityEvent> {
        let mut seen = HashSet::new();
        for dir in session_dirs(home) {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "json")
                    || !path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .is_some_and(|stem| stem.bytes().all(|b| b.is_ascii_digit()))
                {
                    continue;
                }
                let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
                    continue;
                };
                seen.insert(path.clone());
                if self
                    .files
                    .get(&path)
                    .is_some_and(|(known, _)| *known == modified)
                {
                    continue;
                }
                // A torn read mid-rewrite fails to parse; the next pass sees the finished file.
                if let Some(record) = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                {
                    self.files.insert(path, (modified, record));
                }
            }
        }
        self.files.retain(|path, _| seen.contains(path));

        let live: Vec<(&PathBuf, &Value)> = self
            .files
            .iter()
            .map(|(path, (_, record))| (path, record))
            .filter(|(_, record)| {
                record["pid"]
                    .as_u64()
                    .and_then(|pid| u32::try_from(pid).ok())
                    .is_some_and(process_alive)
            })
            .collect();
        let jobs: HashMap<String, &Value> = live
            .iter()
            .filter(|(_, record)| record["kind"] != "interactive")
            .filter_map(|(_, record)| Some((text(record, "jobId")?, *record)))
            .collect();

        let mut events = Vec::new();
        let mut live_panes = HashSet::new();
        let mut live_pids = HashSet::new();
        for (path, record) in &live {
            if record["kind"] != "interactive" {
                continue;
            }
            let Some(pid) = record["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok())
            else {
                continue;
            };
            live_pids.insert(pid);
            let Some(pane) = self
                .owners
                .entry(pid)
                .or_insert_with(|| pane_of(pid))
                .clone()
            else {
                continue;
            };
            // A terminal showing a daemon-hosted conversation: that conversation's record speaks.
            let shown = text(record, "parkedJobId")
                .and_then(|job| jobs.get(&job).copied())
                .unwrap_or(record);
            let Some(status) = text(shown, "status") else {
                continue;
            };
            let next = PaneState {
                pid,
                status,
                waiting_for: text(shown, "waitingFor"),
                conversation: text(shown, "sessionId"),
            };
            live_panes.insert(pane.clone());
            let config_dir = path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_default();
            let previous = self.panes.insert(pane.clone(), next.clone());
            let make = |name: &str| {
                ActivityEvent::new(&pane.0, &pane.1, "claude", name, next.conversation.clone())
            };
            let Some(previous) = previous else {
                let started = record["startedAt"].as_i64().unwrap_or_default();
                let age = chrono::Utc::now().timestamp_millis() - started;
                if (0..FRESH_START.as_millis() as i64).contains(&age) {
                    events.push(make("SessionStart").with("source", json!("startup")));
                } else {
                    let name = match next.status.as_str() {
                        "busy" => "UserPromptSubmit",
                        "waiting" => {
                            waiting_event(next.waiting_for.as_deref()).unwrap_or("SessionStart")
                        }
                        _ => "SessionStart",
                    };
                    events.push(make(name).baseline());
                }
                continue;
            };
            if previous.pid != next.pid || previous.conversation != next.conversation {
                // A new `claude` in the pane, /clear, /resume, or the daemon view moving on. The
                // same process leaving a conversation ends it, as Claude's SessionEnd hook says.
                if previous.pid == next.pid && previous.conversation.is_some() {
                    events.push(ActivityEvent::new(
                        &pane.0,
                        &pane.1,
                        "claude",
                        "SessionEnd",
                        previous.conversation.clone(),
                    ));
                }
                events.push(make("SessionStart"));
                continue;
            }
            if previous.status == next.status && previous.waiting_for == next.waiting_for {
                continue;
            }
            self.reminders.remove(&pane);
            match next.status.as_str() {
                // Back to work after an answer: no hook fires at that moment (PostToolUse comes
                // when the approved tool ends), so only the activity is compared.
                "busy" if previous.status == "waiting" => {
                    events.push(make("SessionBusy").activity_only())
                }
                "busy" => events.push(make("UserPromptSubmit")),
                "waiting" => {
                    if let Some(name) = waiting_event(next.waiting_for.as_deref()) {
                        events.push(make(name).with("waitingFor", json!(next.waiting_for)));
                    }
                }
                "idle" => {
                    let from_turn = previous.status == "busy"
                        || waiting_event(previous.waiting_for.as_deref()).is_some();
                    if from_turn {
                        let since = shown["statusUpdatedAt"]
                            .as_i64()
                            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                        self.reminders
                            .insert(pane.clone(), since + IDLE_REMINDER_MS);
                        self.turn_ends.push(TurnEnd {
                            event: make("Stop"),
                            conversation: next.conversation.clone(),
                            config_dir,
                            cwd: text(shown, "cwd"),
                            due: Instant::now() + TURN_END_SETTLE,
                        });
                    }
                }
                _ => {}
            }
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let due: Vec<_> = self
            .reminders
            .iter()
            .filter(|(_, at)| **at <= now_ms)
            .map(|(pane, _)| pane.clone())
            .collect();
        for pane in due {
            self.reminders.remove(&pane);
            if let Some(state) = self.panes.get(&pane).filter(|state| state.status == "idle") {
                events.push(
                    ActivityEvent::new(
                        &pane.0,
                        &pane.1,
                        "claude",
                        "Notification",
                        state.conversation.clone(),
                    )
                    .with("notificationKind", json!("idleInput")),
                );
            }
        }
        // A `claude` that exited removes its record.
        let ended: Vec<_> = self
            .panes
            .iter()
            .filter(|(pane, _)| !live_panes.contains(*pane))
            .map(|(pane, state)| (pane.clone(), state.conversation.clone()))
            .collect();
        for (pane, conversation) in ended {
            self.panes.remove(&pane);
            self.reminders.remove(&pane);
            events.push(ActivityEvent::new(
                &pane.0,
                &pane.1,
                "claude",
                "SessionEnd",
                conversation,
            ));
        }
        self.owners.retain(|pid, _| live_pids.contains(pid));

        let now = Instant::now();
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.turn_ends)
            .into_iter()
            .partition(|end| end.due <= now);
        self.turn_ends = waiting;
        for end in due {
            events.push(settle_turn_end(end));
        }
        events
    }
}

/// A finished turn as the transcript tells it: an interrupt, a manual or automatic compaction,
/// or a completed turn.
fn settle_turn_end(end: TurnEnd) -> ActivityEvent {
    let TurnEnd {
        mut event,
        conversation,
        config_dir,
        cwd,
        ..
    } = end;
    let Some(path) = conversation
        .as_deref()
        .and_then(|id| transcript_path(&config_dir, cwd.as_deref(), id))
    else {
        return event;
    };
    match last_turn_entry(&path) {
        Some(TurnTail::Interrupted) => event.event_name = "Interrupt".to_string(),
        Some(TurnTail::Compacted(trigger)) => {
            event.event_name = "PostCompact".to_string();
            event.payload.insert("trigger".to_string(), json!(trigger));
        }
        _ => {}
    }
    event
}

fn transcript_path(config_dir: &Path, cwd: Option<&str>, conversation: &str) -> Option<PathBuf> {
    if let Some(cwd) = cwd {
        let folder: String = cwd
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let path = config_dir
            .join("projects")
            .join(folder)
            .join(format!("{conversation}.jsonl"));
        if path.is_file() {
            return Some(path);
        }
    }
    crate::agent_transcripts::find_claude_transcript(conversation)
}

enum TurnTail {
    Interrupted,
    Compacted(String),
    Other,
}

/// The newest turn-shaping entry of a Claude transcript.
fn last_turn_entry(path: &Path) -> Option<TurnTail> {
    const TAIL: u64 = 256 * 1024;
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines().rev() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match entry["type"].as_str() {
            Some("system") if entry["subtype"] == "compact_boundary" => {
                let trigger = entry["compactMetadata"]["trigger"]
                    .as_str()
                    .unwrap_or("auto")
                    .to_string();
                return Some(TurnTail::Compacted(trigger));
            }
            Some("assistant") => return Some(TurnTail::Other),
            Some("user") => {
                if entry["isMeta"] == true || entry["isCompactSummary"] == true {
                    continue;
                }
                let content = &entry["message"]["content"];
                let texts: Vec<&str> = match content {
                    Value::String(text) => vec![text.as_str()],
                    Value::Array(parts) => {
                        parts.iter().filter_map(|p| p["text"].as_str()).collect()
                    }
                    _ => Vec::new(),
                };
                if texts
                    .iter()
                    .any(|text| text.starts_with("[Request interrupted by user"))
                {
                    return Some(TurnTail::Interrupted);
                }
                // Local command echoes (`<command-name>/compact`, its stdout) sit after the boundary.
                if texts.iter().any(|text| {
                    text.starts_with("<command-name>") || text.starts_with("<local-command")
                }) {
                    continue;
                }
                return Some(TurnTail::Other);
            }
            _ => continue,
        }
    }
    None
}
