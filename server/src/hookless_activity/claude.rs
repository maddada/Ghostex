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

use super::claude_turns::{TurnCursor, TurnMark, CLOCK_SLACK_MS};
use super::comparison::ActivityEvent;

pub(crate) const OBSERVABLE_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PermissionRequest",
    "Stop",
    "StopFailure",
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
/// How often a followed pane whose transcript is not on disk yet looks for it again.
const TRANSCRIPT_LOOKUP_MS: i64 = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PaneState {
    pub(super) pid: u32,
    pub(super) status: String,
    waiting_for: Option<String>,
    pub(super) conversation: Option<String>,
    /// The pane's zmx name `S..-P..-G..`, the hook store's `surfaceId`.
    pub(super) surface: String,
    /// Where the conversation's transcript is looked for.
    pub(super) config_dir: PathBuf,
    pub(super) cwd: Option<String>,
    /// Claude sits at its prompt with background shells still running (record status `shell`).
    pub(super) shells_running: bool,
}

struct TurnEnd {
    event: ActivityEvent,
    conversation: Option<String>,
    config_dir: PathBuf,
    cwd: Option<String>,
    due: Instant,
}

/// The session records on disk, each read again only when its file changed, and the
/// `claude attach` clients that write none.
#[derive(Default)]
pub(super) struct SessionRecords {
    files: HashMap<PathBuf, (SystemTime, Value)>,
    attach: super::attach::AttachClients,
}

#[derive(Default)]
pub(super) struct ClaudeObserver {
    /// The zmx pane each `claude` process hangs below, by pid.
    owners: HashMap<u32, Option<String>>,
    panes: HashMap<(String, String), PaneState>,
    turn_ends: Vec<TurnEnd>,
    /// When each pane idle after a turn gets Claude's idle reminder (ms since the epoch).
    reminders: HashMap<(String, String), i64>,
    /// Where each followed pane's transcript was last read, for the turns the record skips
    /// (claude_turns.rs).
    cursors: HashMap<(String, String), TurnCursor>,
}

pub(super) fn session_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![ghostex_paths::agent_homes::claude_default_home(&home).join("sessions")];
    let profiles = crate::accounts::claude_resets::swap_root(home).join("sessions");
    if let Ok(entries) = std::fs::read_dir(profiles) {
        dirs.extend(entries.flatten().map(|entry| entry.path().join("sessions")));
    }
    dirs
}

pub(super) fn text(record: &Value, key: &str) -> Option<String> {
    record[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The zmx name of the Ghostex pane a `claude` process hangs below.
fn pane_of(pid: u32) -> Option<String> {
    crate::agent_hooks::zmx_session_owning_agent(pid)
}

/// The (project id, session id) of a Ghostex pane's zmx name `S..-P..-G..`.
fn pane_from_zmx_name(name: &str) -> Option<(String, String)> {
    let mut parts = name.split('-');
    let (Some(_server), Some(project), Some(session), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    (project.starts_with('P') && session.starts_with('G'))
        .then(|| (project.to_string(), session.to_string()))
}

/// The `background_tasks` Claude's Stop hook sends, as far as the record tells: a turn that ended
/// with a background shell still running (record status `shell`) lists one, which keeps the
/// session working instead of ringing (`activity_for_hook_event` in agent_hooks/event_mapping.rs).
fn background_tasks(shells_running: bool) -> Value {
    if shells_running {
        json!([{"type": "shell", "status": "running"}])
    } else {
        json!([])
    }
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

/// One live `claude` process's session record and the config folder it was found in.
pub(super) struct LiveRecord {
    pub(super) config_dir: PathBuf,
    pub(super) record: Value,
}

impl SessionRecords {
    /// The records of every live `claude` process, in `~/.claude` and every cswap profile. With
    /// `attach_clients`, also one made up for each `claude attach` client (attach.rs).
    pub(super) fn read(&mut self, home: &Path, attach_clients: bool) -> Vec<LiveRecord> {
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
        let mut live: Vec<LiveRecord> = self
            .files
            .iter()
            // A record left behind by a killed `claude` whose pid a later process took is not live
            // (CDXC:SessionIdentity 2026-11-05 in agent_hooks/claude_session_record.rs).
            .filter(|(_, (_, record))| crate::agent_hooks::claude_session_record_live(record))
            .map(|(path, (_, record))| LiveRecord {
                // `<config>/sessions/<pid>.json`
                config_dir: path
                    .parent()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
                record: record.clone(),
            })
            .collect();
        if attach_clients {
            let made_up = self.attach.records(home, &live);
            live.extend(made_up);
        }
        live
    }
}

impl ClaudeObserver {
    /// Only panes of `server_id` (the `S..` of their zmx name `S..-P..-G..`) are followed.
    ///
    /// CDXC:AgentHooks 2026-11-08 FIX:
    /// Two threads on 8 Nov saw a throwaway test gxserver (`cargo xtask test-server`) log `/api/ingestAgentHookEvent failed: Session P3lv0/G7d9n does not exist` for the user's real sessions. Its Claude observer reads every live `claude` record under `~/.claude` (a test server's own Claude sessions use that folder too), found the real server's panes above them, dropped the server id from their zmx names and drove their events into itself. Every gxserver on the computer did the same with every other server's panes, the real one included. The real server lost nothing: the hooks post to the base URL in their own session's environment and the test server only rejected what it drove. A pane whose zmx name names another server is now left to that server.
    pub(super) fn poll(&mut self, live: &[LiveRecord], server_id: &str) -> Vec<ActivityEvent> {
        self.step(live, &mut |pid| {
            pane_of(pid).filter(|name| name.split('-').next() == Some(server_id))
        })
    }

    /// The Ghostex panes (project id, session id) a live `claude` is shown in right now.
    pub(super) fn panes(&self) -> impl Iterator<Item = &(String, String)> {
        self.panes.keys()
    }

    /// Every pane a live `claude` is shown in, with what it shows.
    pub(super) fn pane_states(&self) -> impl Iterator<Item = (&(String, String), &PaneState)> {
        self.panes.iter()
    }

    /// Turns the live records into hook events by comparing each pane with what it showed on the
    /// last pass. `pane_lookup` finds the zmx name of the Ghostex pane a `claude` process hangs
    /// below; the conversation always comes from the record, never from a hook.
    fn step(
        &mut self,
        live: &[LiveRecord],
        pane_lookup: &mut dyn FnMut(u32) -> Option<String>,
    ) -> Vec<ActivityEvent> {
        let jobs: HashMap<String, &Value> = live
            .iter()
            .map(|live| &live.record)
            .filter(|record| record["kind"] != "interactive")
            .filter_map(|record| Some((text(record, "jobId")?, record)))
            .collect();

        /*
        CDXC:SessionIdentity 2026-11-05 WHY:
        A pane shows one `claude`, but two live records can still map to it (a pid reused by the
        pane's own shell before claude_session_record.rs caught it, a relaunch overlapping its
        predecessor for a pass). Taking each in turn made the pane flip between their
        conversations on every pass, sending a SessionStart each time, and whichever came last
        became the session's agent session id (thread G1y7g on 2026-11-05: ten thousand flips and
        a September conversation stored). Each pane follows one record per pass: the process it
        already showed while that one lives, else the earliest started.
        */
        let mut pane_owner: HashMap<(String, String), (bool, i64, u32)> = HashMap::new();
        for LiveRecord { record, .. } in live {
            if record["kind"] != "interactive" {
                continue;
            }
            let Some(pid) = record["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok())
            else {
                continue;
            };
            let Some(pane) = self
                .owners
                .entry(pid)
                .or_insert_with(|| pane_lookup(pid))
                .as_deref()
                .and_then(pane_from_zmx_name)
            else {
                continue;
            };
            let shown_before = self.panes.get(&pane).is_some_and(|state| state.pid == pid);
            let rank = (
                !shown_before,
                record["startedAt"].as_i64().unwrap_or(i64::MAX),
                pid,
            );
            pane_owner
                .entry(pane)
                .and_modify(|best| *best = (*best).min(rank))
                .or_insert(rank);
        }

        // Turn ends settled since the last pass come before this pass's news, so a turn's Stop is
        // never sent after the UserPromptSubmit of the turn that follows it.
        let mut events = Vec::new();
        let now = Instant::now();
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.turn_ends)
            .into_iter()
            .partition(|end| end.due <= now);
        self.turn_ends = waiting;
        events.extend(due.into_iter().map(settle_turn_end));
        let mut live_panes = HashSet::new();
        let mut live_pids = HashSet::new();
        for LiveRecord { config_dir, record } in live {
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
            let Some(surface) = self
                .owners
                .entry(pid)
                .or_insert_with(|| pane_lookup(pid))
                .clone()
            else {
                continue;
            };
            let Some(pane) = pane_from_zmx_name(&surface) else {
                continue;
            };
            if pane_owner.get(&pane).map(|(_, _, owner)| *owner) != Some(pid) {
                continue;
            }
            // A terminal showing a daemon-hosted conversation: that conversation's record speaks.
            let shown = text(record, "parkedJobId")
                .and_then(|job| jobs.get(&job).copied())
                .unwrap_or(record);
            let Some(status) = text(shown, "status") else {
                continue;
            };
            // CDXC:AgentHooks 2026-11-02 WHY:
            // Claude 2.1.316 writes `shell` while it sits at the prompt with a background shell still running ("1 shell still running"), so a turn ending that way never went `idle` and never reported its Stop. It is idle for activity; the shells stay known as running (`shells_running`, and the transcript's own launches for the roster), and the shell finishing later is no event.
            // CDXC:AgentHooks 2026-11-12 FIX:
            // The day's comparison found the Main Coordinator ringing on every reply while its `cargo xtask rebuild` ran in the background: Claude's Stop hook lists that shell in `background_tasks`, which keeps the session working (CDXC:Notifications 2026-09-15, the user's rule to ring only once background work is done), but the observer's Stop carried none. Its Stop now lists the running shells the same way (`background_tasks`).
            let shells_running = status == "shell";
            let status = if shells_running {
                "idle".to_string()
            } else {
                status
            };
            let cwd = text(shown, "cwd");
            let next = PaneState {
                pid,
                status,
                shells_running,
                waiting_for: text(shown, "waitingFor"),
                conversation: text(shown, "sessionId"),
                surface,
                config_dir: config_dir.clone(),
                cwd: cwd.clone(),
            };
            live_panes.insert(pane.clone());
            // Looked up only when the pane has news: a conversation with no transcript yet is
            // searched for in every project folder.
            let transcript = std::cell::OnceCell::new();
            let transcript = || {
                transcript
                    .get_or_init(|| {
                        next.conversation
                            .as_deref()
                            .and_then(|id| transcript_path(config_dir, cwd.as_deref(), id))
                    })
                    .clone()
            };
            let previous = self.panes.insert(pane.clone(), next.clone());
            let make = |name: &str| {
                let mut event =
                    ActivityEvent::new(&pane.0, &pane.1, "claude", name, next.conversation.clone());
                event.transcript = transcript();
                event
            };
            // Claude's SessionStart names why it fired. A new conversation has no transcript until
            // its first prompt, except that /clear writes the new one's first lines at once
            // (Claude 2.1.312), starting with the /clear command itself; any other conversation
            // already on disk was resumed.
            let start = |same_process: bool| {
                let source = match transcript() {
                    Some(path) if !(same_process && opened_by_clear(&path)) => "resume",
                    _ if same_process => "clear",
                    _ => "startup",
                };
                make("SessionStart").with("source", json!(source))
            };
            // A conversation that is already working when it starts (a launch with a prompt, a
            // prompt sent right after /clear) also reports the turn, as the hooks do.
            let started_turn = || match next.status.as_str() {
                "busy" => Some(make("UserPromptSubmit")),
                "waiting" => waiting_event(next.waiting_for.as_deref())
                    .map(|name| make(name).with("waitingFor", json!(next.waiting_for))),
                _ => None,
            };
            let Some(previous) = previous else {
                let started = record["startedAt"].as_i64().unwrap_or_default();
                let age = chrono::Utc::now().timestamp_millis() - started;
                if (0..FRESH_START.as_millis() as i64).contains(&age) {
                    events.push(start(false));
                    events.extend(started_turn());
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
                let same_process = previous.pid == next.pid;
                if same_process && previous.conversation.is_some() {
                    events.push(ActivityEvent::new(
                        &pane.0,
                        &pane.1,
                        "claude",
                        "SessionEnd",
                        previous.conversation.clone(),
                    ));
                }
                self.reminders.remove(&pane);
                events.push(start(same_process));
                events.extend(started_turn());
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
                // A queued prompt starting its turn was submitted when it was sent (claude_turns.rs).
                "busy"
                    if self
                        .cursors
                        .get(&pane)
                        .is_some_and(TurnCursor::submitted_prompt_next) =>
                {
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
                    let since = shown["statusUpdatedAt"]
                        .as_i64()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                    // The idle between a turn and its quick restart, read late: the transcript
                    // already sent that turn's Stop and the next prompt, so the pane stays busy.
                    let cursor = self.cursors.get_mut(&pane);
                    let already_sent = cursor.as_ref().is_some_and(|cursor| {
                        cursor
                            .restart_sent_ms
                            .is_some_and(|sent| since <= sent + CLOCK_SLACK_MS)
                    });
                    if from_turn && already_sent {
                        if let Some(state) = self.panes.get_mut(&pane) {
                            state.status = previous.status.clone();
                            state.waiting_for = previous.waiting_for.clone();
                        }
                    } else if from_turn {
                        if let Some(cursor) = cursor {
                            cursor.record_idle_ms = Some(since);
                        }
                        self.reminders
                            .insert(pane.clone(), since + IDLE_REMINDER_MS);
                        self.turn_ends.push(TurnEnd {
                            event: make("Stop")
                                .with("background_tasks", background_tasks(next.shells_running)),
                            conversation: next.conversation.clone(),
                            config_dir: config_dir.clone(),
                            cwd: cwd.clone(),
                            due: Instant::now() + TURN_END_SETTLE,
                        });
                    }
                }
                _ => {}
            }
        }
        let mut followed: Vec<_> = live_panes.iter().cloned().collect();
        followed.sort();
        for pane in &followed {
            self.read_turns(pane, &mut events);
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
        self.cursors.retain(|pane, _| live_panes.contains(pane));
        events
    }

    /// Sends what the transcripts of the followed panes gained since the last pass and the
    /// records did not show: a turn that ended and restarted between two passes, and prompts
    /// folded into a running turn (CDXC:AgentHooks 2026-11-11 in claude_turns.rs).
    fn read_turns(&mut self, pane: &(String, String), events: &mut Vec<ActivityEvent>) {
        let Some(state) = self.panes.get(pane) else {
            return;
        };
        let Some(conversation) = state.conversation.clone() else {
            self.cursors.remove(pane);
            return;
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let lookup = |state: &PaneState| {
            transcript_path(&state.config_dir, state.cwd.as_deref(), &conversation)
        };
        let cursor = match self.cursors.get_mut(pane) {
            Some(cursor) if cursor.conversation == conversation => cursor,
            _ => {
                let cursor = TurnCursor::new(&conversation, lookup(state), now_ms);
                self.cursors.insert(pane.clone(), cursor);
                self.cursors.get_mut(pane).expect("inserted")
            }
        };
        if cursor.path.is_none() {
            // Only the expected folder each pass; the search of every project folder now and then.
            let expected = state
                .cwd
                .as_deref()
                .map(|cwd| expected_transcript_path(&state.config_dir, cwd, &conversation))
                .filter(|path| path.is_file());
            let found = match expected {
                Some(path) => Some(path),
                None if cursor.next_lookup_ms <= now_ms => {
                    cursor.next_lookup_ms = now_ms + TRANSCRIPT_LOOKUP_MS;
                    lookup(state)
                }
                None => None,
            };
            match found {
                Some(path) => cursor.found(path),
                None => return,
            }
        }
        let path = cursor.path.clone();
        let make = |name: &str, prompt: Option<&String>| {
            let mut event =
                ActivityEvent::new(&pane.0, &pane.1, "claude", name, Some(conversation.clone()));
            if let Some(prompt) = prompt {
                event = event.with("prompt", json!(prompt));
            }
            event.transcript = path.clone();
            event
        };
        let mut found = Vec::new();
        for mark in cursor.read_new(state.status == "idle") {
            match mark {
                TurnMark::QueuedPrompt { prompt } => {
                    found.push(make("UserPromptSubmit", Some(&prompt)))
                }
                // Claude's hook reports a task notice folded into the turn; nothing to drive.
                TurnMark::FoldedNotice => found.push(make("UserPromptSubmit", None).compare_only()),
                TurnMark::Restart {
                    end,
                    ended_turn_started_ms,
                    ended_turn_prompt,
                    prompt,
                    prompt_ms,
                    prompt_submitted,
                } => {
                    let record_saw_idle = cursor.record_idle_ms.is_some_and(|idle| {
                        idle > ended_turn_started_ms && idle <= prompt_ms + CLOCK_SLACK_MS
                    });
                    if record_saw_idle || state.status == "idle" {
                        continue;
                    }
                    found.push(
                        make(end, ended_turn_prompt.as_ref())
                            .with("background_tasks", background_tasks(false))
                            .with("quickRestart", json!(true)),
                    );
                    // A queued prompt was submitted when it was sent; its turn only shows work.
                    found.push(if prompt_submitted {
                        make("SessionBusy", None).activity_only()
                    } else {
                        make("UserPromptSubmit", prompt.as_ref())
                    });
                    cursor.restart_sent_ms = Some(prompt_ms);
                }
            }
        }
        events.extend(found);
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
        // Claude sends StopFailure instead of Stop for a turn its last API call failed.
        Some(TurnTail::Failed) => event.event_name = "StopFailure".to_string(),
        _ => {}
    }
    event.transcript = Some(path);
    event
}

pub(super) fn transcript_path(
    config_dir: &Path,
    cwd: Option<&str>,
    conversation: &str,
) -> Option<PathBuf> {
    if let Some(path) = cwd.map(|cwd| expected_transcript_path(config_dir, cwd, conversation)) {
        if path.is_file() {
            return Some(path);
        }
    }
    crate::agent_transcripts::find_claude_transcript(conversation)
}

/// Where Claude writes a conversation started in `cwd`, written yet or not.
pub(super) fn expected_transcript_path(
    config_dir: &Path,
    cwd: &str,
    conversation: &str,
) -> PathBuf {
    let folder: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    config_dir
        .join("projects")
        .join(folder)
        .join(format!("{conversation}.jsonl"))
}

/// Whether a transcript's first lines are the `/clear` that started it.
pub(super) fn opened_by_clear(path: &Path) -> bool {
    let mut head = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(64 * 1024).read_to_end(&mut head))
        .is_ok()
        && String::from_utf8_lossy(&head)
            .lines()
            .take(16)
            .any(|line| line.contains("<command-name>/clear</command-name>"))
}

enum TurnTail {
    Interrupted,
    Compacted(String),
    /// The turn's last API call failed (an `isApiErrorMessage` reply).
    Failed,
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
            Some("assistant") if entry["isApiErrorMessage"] == true => {
                return Some(TurnTail::Failed)
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

#[cfg(test)]
mod tests {
    use super::*;

    const PANE: (&str, &str) = ("P1", "G1");

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "gx-hookless-{name}-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        /// Writes `<config>/projects/<cwd folder>/<id>.jsonl`, as Claude does once a conversation
        /// has its first line.
        fn write_transcript(&self, profile: &str, id: &str) -> PathBuf {
            let folder = self
                .cwd()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
            let dir = self.dir.join(profile).join("projects").join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join(format!("{id}.jsonl"));
            std::fs::write(
                &path,
                "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n",
            )
            .unwrap();
            path
        }

        fn cwd(&self) -> String {
            self.dir.join("project").to_string_lossy().into_owned()
        }

        fn record(&self, profile: &str, pid: u32, conversation: &str, status: &str) -> LiveRecord {
            LiveRecord {
                config_dir: self.dir.join(profile),
                record: json!({
                    "pid": pid,
                    "sessionId": conversation,
                    "cwd": self.cwd(),
                    "kind": "interactive",
                    "status": status,
                    "startedAt": chrono::Utc::now().timestamp_millis(),
                }),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn step(observer: &mut ClaudeObserver, live: &[LiveRecord]) -> Vec<ActivityEvent> {
        observer.step(live, &mut |_| Some(format!("S1-{}-{}", PANE.0, PANE.1)))
    }

    fn names(events: &[ActivityEvent]) -> Vec<(String, Option<String>, Option<String>)> {
        events
            .iter()
            .map(|event| {
                (
                    event.event_name.clone(),
                    event.agent_session_id.clone(),
                    event
                        .payload
                        .get("source")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                )
            })
            .collect()
    }

    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn start(conversation: &str, source: &str) -> (String, Option<String>, Option<String>) {
        (
            "SessionStart".to_string(),
            Some(conversation.to_string()),
            Some(source.to_string()),
        )
    }

    fn end(conversation: &str) -> (String, Option<String>, Option<String>) {
        (
            "SessionEnd".to_string(),
            Some(conversation.to_string()),
            None,
        )
    }

    #[test]
    fn clear_moves_the_pane_to_the_new_unwritten_conversation() {
        let fixture = Fixture::new("clear");
        let (first, cleared) = (id(), id());
        fixture.write_transcript(".claude", &first);
        let mut observer = ClaudeObserver::default();
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &first, "idle")],
        );
        assert_eq!(names(&events), vec![start(&first, "resume")]);
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &cleared, "idle")],
        );
        assert_eq!(names(&events), vec![end(&first), start(&cleared, "clear")]);
        assert_eq!(events[1].transcript, None);
        // The first prompt of the cleared conversation, sent at once.
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &cleared, "busy")],
        );
        assert_eq!(
            names(&events),
            vec![("UserPromptSubmit".to_string(), Some(cleared.clone()), None)]
        );
    }

    #[test]
    fn clear_that_writes_its_transcript_at_once_is_still_a_clear() {
        let fixture = Fixture::new("clear-written");
        let (first, cleared) = (id(), id());
        fixture.write_transcript(".claude", &first);
        let path = fixture.write_transcript(".claude", &cleared);
        std::fs::write(
            &path,
            "{\"type\":\"user\",\"message\":{\"content\":\"<command-name>/clear</command-name>\"}}\n",
        )
        .unwrap();
        let mut observer = ClaudeObserver::default();
        step(
            &mut observer,
            &[fixture.record(".claude", 100, &first, "idle")],
        );
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &cleared, "idle")],
        );
        assert_eq!(names(&events), vec![end(&first), start(&cleared, "clear")]);
        assert_eq!(events[1].transcript.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn resume_in_the_same_process_names_the_resumed_transcript() {
        let fixture = Fixture::new("resume");
        let (fresh, resumed) = (id(), id());
        let path = fixture.write_transcript(".claude", &resumed);
        let mut observer = ClaudeObserver::default();
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &fresh, "idle")],
        );
        assert_eq!(names(&events), vec![start(&fresh, "startup")]);
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &resumed, "idle")],
        );
        assert_eq!(names(&events), vec![end(&fresh), start(&resumed, "resume")]);
        assert_eq!(events[1].transcript.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn an_account_switch_restarts_claude_in_another_profile() {
        let fixture = Fixture::new("account");
        let conversation = id();
        fixture.write_transcript(".claude", &conversation);
        let profile_path = fixture.write_transcript("profile-2", &conversation);
        let mut observer = ClaudeObserver::default();
        step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "idle")],
        );
        // The old process is gone and the account's `claude --resume` took the pane over: a new
        // process, so no SessionEnd from the old one, which never left its conversation.
        let events = step(
            &mut observer,
            &[fixture.record("profile-2", 200, &conversation, "idle")],
        );
        assert_eq!(names(&events), vec![start(&conversation, "resume")]);
        assert_eq!(
            events[0].transcript.as_deref(),
            Some(profile_path.as_path())
        );
    }

    /// Two live records on one pane (thread G1y7g on 2026-11-05, see CDXC:SessionIdentity
    /// 2026-11-05 in `step`): the pane keeps the conversation it showed, in either order, instead
    /// of flipping to the other one on every pass.
    #[test]
    fn a_second_record_on_the_same_pane_never_takes_it_over() {
        let fixture = Fixture::new("two-records");
        let (own, other) = (id(), id());
        fixture.write_transcript(".claude", &own);
        fixture.write_transcript("profile-3", &other);
        let mut observer = ClaudeObserver::default();
        let own_record = fixture.record(".claude", 100, &own, "idle");
        let events = step(&mut observer, &[own_record.clone_record()]);
        assert_eq!(names(&events), vec![start(&own, "resume")]);
        let mut other_record = fixture.record("profile-3", 200, &other, "idle");
        other_record.record["startedAt"] = json!(1_790_829_608_184_i64);
        for live in [
            [own_record.clone_record(), other_record.clone_record()],
            [other_record.clone_record(), own_record.clone_record()],
        ] {
            assert_eq!(names(&step(&mut observer, &live)), Vec::new());
        }
        assert_eq!(
            observer.panes.values().next().map(|state| state.pid),
            Some(100)
        );
        // Seen together on the first pass, the earliest started process owns the pane.
        let mut fresh = ClaudeObserver::default();
        let mut later = fixture.record(".claude", 300, &other, "idle");
        later.record["startedAt"] = json!(chrono::Utc::now().timestamp_millis() + 5_000);
        step(&mut fresh, &[later, own_record.clone_record()]);
        assert_eq!(
            fresh
                .panes
                .values()
                .next()
                .and_then(|state| state.conversation.clone()),
            Some(own.clone())
        );
    }

    #[test]
    fn a_launch_with_a_prompt_reports_its_turn() {
        let fixture = Fixture::new("launch");
        let conversation = id();
        let mut observer = ClaudeObserver::default();
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "busy")],
        );
        assert_eq!(
            names(&events),
            vec![
                start(&conversation, "startup"),
                (
                    "UserPromptSubmit".to_string(),
                    Some(conversation.clone()),
                    None
                ),
            ]
        );
    }

    #[test]
    fn a_parked_terminal_speaks_for_the_daemon_conversation() {
        let fixture = Fixture::new("parked");
        let (own, hosted) = (id(), id());
        let mut terminal = fixture.record(".claude", 100, &own, "idle");
        let mut observer = ClaudeObserver::default();
        step(&mut observer, &[terminal.clone_record()]);
        terminal.record["parkedJobId"] = json!("job1");
        let job = LiveRecord {
            config_dir: fixture.dir.join(".claude"),
            record: json!({"pid": 300, "sessionId": hosted, "kind": "bg", "jobId": "job1", "status": "busy", "cwd": fixture.cwd()}),
        };
        let events = step(&mut observer, &[terminal, job]);
        assert_eq!(
            names(&events),
            vec![
                end(&own),
                start(&hosted, "clear"),
                ("UserPromptSubmit".to_string(), Some(hosted.clone()), None),
            ]
        );
    }

    #[test]
    fn a_turn_that_ends_with_a_shell_still_running_reports_its_stop() {
        let fixture = Fixture::new("shell");
        let conversation = id();
        fixture.write_transcript(".claude", &conversation);
        let mut observer = ClaudeObserver::default();
        step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "idle")],
        );
        step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "busy")],
        );
        // Claude 2.1.316: at the prompt, "1 shell still running".
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "shell")],
        );
        assert!(
            events.is_empty(),
            "the turn end waits for its transcript lines"
        );
        std::thread::sleep(TURN_END_SETTLE + Duration::from_millis(50));
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "shell")],
        );
        assert_eq!(
            names(&events),
            vec![("Stop".to_string(), Some(conversation.clone()), None)]
        );
        // Listed as the Stop hook's background work: the session stays working, as with the hooks.
        assert_eq!(
            events[0].payload.get("background_tasks"),
            Some(&background_tasks(true))
        );
        assert_eq!(events[0].activity().as_deref(), Some("working"));
        let pane = (PANE.0.to_string(), PANE.1.to_string());
        assert!(observer.panes[&pane].shells_running);
        assert_eq!(observer.panes[&pane].status, "idle");
        // The shell finishing is no event; a new prompt is a new turn.
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "idle")],
        );
        assert!(events.is_empty());
        assert!(!observer.panes[&pane].shells_running);
        let events = step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "busy")],
        );
        assert_eq!(
            names(&events),
            vec![("UserPromptSubmit".to_string(), Some(conversation), None)]
        );
    }

    fn append(path: &Path, rows: &[Value]) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        for row in rows {
            writeln!(file, "{row}").unwrap();
        }
    }

    fn iso(ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn prompt_row(text: &str, ms: i64) -> Value {
        json!({"type": "user", "timestamp": iso(ms), "message": {"content": text}})
    }

    fn turn_end_row(ms: i64) -> Value {
        json!({"type": "system", "subtype": "turn_duration", "timestamp": iso(ms)})
    }

    fn status_at(mut live: LiveRecord, ms: i64) -> LiveRecord {
        live.record["statusUpdatedAt"] = json!(ms);
        live
    }

    fn prompts(events: &[ActivityEvent]) -> Vec<(String, Option<String>)> {
        events
            .iter()
            .map(|event| {
                (
                    event.event_name.clone(),
                    event
                        .payload
                        .get("prompt")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                )
            })
            .collect()
    }

    /// CDXC:AgentHooks 2026-11-11 (claude_turns.rs): a queued prompt restarts the turn 37 to 74
    /// ms after it ended, between two reads of the record, which stays busy.
    #[test]
    fn a_turn_restarted_between_two_passes_sends_its_stop_once() {
        let fixture = Fixture::new("quick-restart");
        let conversation = id();
        let path = fixture.write_transcript(".claude", &conversation);
        let busy = || fixture.record(".claude", 100, &conversation, "busy");
        let idle = || fixture.record(".claude", 100, &conversation, "idle");
        let mut observer = ClaudeObserver::default();
        step(&mut observer, &[idle()]);
        let t = chrono::Utc::now().timestamp_millis();
        append(&path, &[prompt_row("first", t)]);
        let events = step(&mut observer, &[status_at(busy(), t)]);
        assert_eq!(names(&events)[0].0, "UserPromptSubmit");
        assert_eq!(events.len(), 1, "the record's prompt is not sent twice");
        append(
            &path,
            &[
                json!({"type": "assistant", "timestamp": iso(t + 1_000), "message": {"content": [{"type": "text", "text": "done"}]}}),
                turn_end_row(t + 1_001),
                prompt_row("queued", t + 1_050),
            ],
        );
        let events = step(&mut observer, &[status_at(busy(), t + 1_050)]);
        assert_eq!(
            prompts(&events),
            vec![
                ("Stop".to_string(), Some("first".to_string())),
                ("UserPromptSubmit".to_string(), Some("queued".to_string())),
            ]
        );
        assert_eq!(events[0].transcript.as_deref(), Some(path.as_path()));
        assert_eq!(
            events[0].agent_session_id.as_deref(),
            Some(conversation.as_str())
        );
        // The record's idle between the two turns, read late, is that same turn end.
        let events = step(&mut observer, &[status_at(idle(), t + 1_010)]);
        assert!(events.is_empty(), "{:?}", names(&events));
        let events = step(&mut observer, &[status_at(busy(), t + 1_050)]);
        assert!(events.is_empty(), "{:?}", names(&events));
        // The second turn ends with the record showing it: one Stop from the record, and the
        // transcript's turn end and next prompt add nothing.
        append(&path, &[turn_end_row(t + 5_000)]);
        let events = step(&mut observer, &[status_at(idle(), t + 5_000)]);
        assert!(
            events.is_empty(),
            "the turn end waits for its transcript lines"
        );
        std::thread::sleep(TURN_END_SETTLE + Duration::from_millis(50));
        append(&path, &[prompt_row("third", t + 6_000)]);
        let events = step(&mut observer, &[status_at(busy(), t + 6_000)]);
        assert_eq!(
            names(&events)
                .into_iter()
                .map(|(name, ..)| name)
                .collect::<Vec<_>>(),
            vec!["Stop".to_string(), "UserPromptSubmit".to_string()]
        );
    }

    fn queue_row(operation: &str, content: Option<&str>) -> Value {
        json!({"type": "queue-operation", "operation": operation, "content": content})
    }

    fn event_names(events: &[ActivityEvent]) -> Vec<(String, bool)> {
        events
            .iter()
            .map(|event| (event.event_name.clone(), event.event_signal))
            .collect()
    }

    #[test]
    fn a_prompt_sent_while_claude_works_is_submitted_once() {
        let fixture = Fixture::new("mid-turn");
        let conversation = id();
        let path = fixture.write_transcript(".claude", &conversation);
        let busy = || fixture.record(".claude", 100, &conversation, "busy");
        let mut observer = ClaudeObserver::default();
        step(
            &mut observer,
            &[fixture.record(".claude", 100, &conversation, "idle")],
        );
        let t = chrono::Utc::now().timestamp_millis();
        append(&path, &[prompt_row("work", t)]);
        assert_eq!(step(&mut observer, &[busy()]).len(), 1);
        append(&path, &[queue_row("enqueue", Some("also this"))]);
        let events = step(&mut observer, &[busy()]);
        assert_eq!(
            prompts(&events),
            vec![(
                "UserPromptSubmit".to_string(),
                Some("also this".to_string())
            )]
        );
        // Folded into the running turn later: nothing more.
        append(
            &path,
            &[
                queue_row("remove", Some("also this")),
                json!({"type": "user", "timestamp": iso(t + 900), "message": {"content": [{"type": "tool_result", "tool_use_id": "a"}]}}),
                json!({"type": "attachment", "timestamp": iso(t + 800), "attachment": {"type": "queued_command", "commandMode": "prompt", "prompt": "also this"}}),
            ],
        );
        assert!(step(&mut observer, &[busy()]).is_empty());
    }

    /// The live sequence of 2026-11-11 (claude_turns.rs): a prompt queued during a text-only turn
    /// is submitted when sent and restarts the turn 16 ms after it ends.
    #[test]
    fn a_queued_prompt_is_submitted_once_whether_or_not_the_record_shows_the_gap() {
        for record_sees_the_gap in [false, true] {
            let fixture = Fixture::new("queued");
            let conversation = id();
            let path = fixture.write_transcript(".claude", &conversation);
            let busy = || fixture.record(".claude", 100, &conversation, "busy");
            let idle = || fixture.record(".claude", 100, &conversation, "idle");
            let mut observer = ClaudeObserver::default();
            step(&mut observer, &[idle()]);
            let t = chrono::Utc::now().timestamp_millis();
            append(&path, &[prompt_row("story", t)]);
            let mut sent = step(&mut observer, &[status_at(busy(), t)]);
            append(&path, &[queue_row("enqueue", Some("now QUEUED"))]);
            sent.extend(step(&mut observer, &[status_at(busy(), t)]));
            append(
                &path,
                &[queue_row("dequeue", None), turn_end_row(t + 1_000)],
            );
            if record_sees_the_gap {
                sent.extend(step(&mut observer, &[status_at(idle(), t + 1_000)]));
                std::thread::sleep(TURN_END_SETTLE + Duration::from_millis(50));
            }
            append(&path, &[prompt_row("now QUEUED", t + 1_016)]);
            sent.extend(step(&mut observer, &[status_at(busy(), t + 1_016)]));
            sent.extend(step(&mut observer, &[status_at(busy(), t + 1_016)]));
            assert_eq!(
                event_names(&sent),
                vec![
                    ("UserPromptSubmit".to_string(), true),
                    ("UserPromptSubmit".to_string(), true),
                    ("Stop".to_string(), true),
                    ("SessionBusy".to_string(), false),
                ],
                "record sees the gap: {record_sees_the_gap}"
            );
        }
    }

    /// A pane first seen mid-conversation reads only what its transcript gains from then on.
    #[test]
    fn the_transcript_already_on_disk_is_not_news() {
        let fixture = Fixture::new("baseline-tail");
        let conversation = id();
        let path = fixture.write_transcript(".claude", &conversation);
        let t = chrono::Utc::now().timestamp_millis();
        append(
            &path,
            &[turn_end_row(t - 10_000), prompt_row("old", t - 9_000)],
        );
        let mut observer = ClaudeObserver::default();
        let busy = fixture.record(".claude", 100, &conversation, "busy");
        let events = step(&mut observer, &[busy.clone_record()]);
        // Only the record's start and turn; the old rows send no Stop and name no prompt.
        assert_eq!(
            prompts(&events),
            vec![
                ("SessionStart".to_string(), None),
                ("UserPromptSubmit".to_string(), None)
            ]
        );
        assert!(step(&mut observer, &[busy]).is_empty());
    }

    impl LiveRecord {
        fn clone_record(&self) -> LiveRecord {
            LiveRecord {
                config_dir: self.config_dir.clone(),
                record: self.record.clone(),
            }
        }
    }
}
