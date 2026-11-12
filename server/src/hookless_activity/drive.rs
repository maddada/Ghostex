//! Claude's hook-free observer driving Claude sessions' state, by default (Use agent hooks off,
//! agent_hooks/hook_free.rs), and with Use agent hooks on wherever Claude's hook is not installed.
//!
//! CDXC:AgentHooks 2026-11-01 DECISION:
//! User: "make Claude find its conversation from the process alone, so its hooks can be dropped once the day's results look clean", with the comparison left on across a day of real use. While the observer drives, every event it reads off Claude's session records goes through the same `/api/ingestAgentHookEvent` path a hook event takes (status, conversation id, transcript path, first prompt, question and approval cards, notifications, account error recovery), and a hook event for a pane the observer follows is still recorded for the comparison but drives nothing.
//! CDXC:AgentHooks 2026-11-07 FIX:
//! Issue #475: on a headless Linux computer Ghostex's hooks were never installed (#431), so its Claude threads stayed "working" after their turns ended and their coordinator heard nothing for hours. Where Claude's hook is not installed (never installed, turned off with disableAllHooks, or not current) nothing else drives Claude's state, so the observer drives it even with Use agent hooks on. The hook status is read at most every 30 seconds.
//! CDXC:AgentHooks 2026-11-12 DECISION:
//! "Stop installing the hooks and remove Ghostex's own" (the user's decision in agent_hooks/hook_free.rs): the Debugging switch "Track Claude without its hooks" is gone and its behaviour is the default for Claude, Codex and OpenCode alike. The gate below covers all three: a hook event for a session its agent's watcher follows drives nothing, unless the session fell back to the hook (hook_free_fallback.rs), whose watcher then stops driving it.
//! SEE-ALSO: server/src/hookless_activity/claude.rs (the observer), server/src/server/agent_http.rs (where a superseded hook stops), server/src/agent_hooks/hook_free.rs (the setting).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::comparison::ActivityEvent;
use crate::agent_hooks::hook_free;
use crate::domain::DomainRepository;
use crate::logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel};
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Marks an event the observer sent, so the hook gate and the comparison's hook side skip it.
pub(crate) const OBSERVER_MARK: &str = "hookFreeObserver";
/// How long one read of an agent's hook status is trusted.
const HOOK_STATUS_TTL: Duration = Duration::from_secs(30);
const TRANSCRIPT_TAIL: u64 = 512 * 1024;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

static HOOK_INSTALLED: LazyLock<Mutex<Option<(Instant, Option<bool>)>>> =
    LazyLock::new(Default::default);
/// The sessions (project id, session id) each agent's watcher follows, as it last saw.
static TRACKED: LazyLock<Mutex<HashMap<&'static str, HashSet<(String, String)>>>> =
    LazyLock::new(Default::default);

/// Why the observer drives Claude's sessions, if it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DriveReason {
    /// Use agent hooks is off: Ghostex follows Claude without its hook.
    HookFree,
    /// Use agent hooks is on, but Claude's hook is not installed on this computer.
    HooksMissing,
}

impl DriveReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DriveReason::HookFree => "hookFree",
            DriveReason::HooksMissing => "hooksMissing",
        }
    }
}

/// Hook-free first; else a hook status that says the hook is not installed. A status that could
/// not be read (`None`) keeps the hooks.
pub(super) fn drive_reason_for(
    hook_free: bool,
    hook_installed: Option<bool>,
) -> Option<DriveReason> {
    if hook_free {
        Some(DriveReason::HookFree)
    } else if hook_installed == Some(false) {
        Some(DriveReason::HooksMissing)
    } else {
        None
    }
}

pub(crate) fn claude_drive_enabled(paths: &GxserverPaths) -> bool {
    claude_drive_reason(paths).is_some()
}

pub(crate) fn claude_drive_reason(paths: &GxserverPaths) -> Option<DriveReason> {
    agent_drive_reason(paths, "claude", &HOOK_INSTALLED)
}

/// Why an agent's watcher drives its sessions here, if it does (never in a test).
pub(super) fn agent_drive_reason(
    paths: &GxserverPaths,
    agent_id: &str,
    hook_cache: &Mutex<Option<(Instant, Option<bool>)>>,
) -> Option<DriveReason> {
    if cfg!(test) {
        return None;
    }
    if hook_free::hook_free(paths, agent_id) {
        return Some(DriveReason::HookFree);
    }
    drive_reason_for(false, agent_hook_installed(paths, agent_id, hook_cache))
}

/// Whether an agent's Ghostex hook is installed here and will run (`hookInstalled` in its hook
/// status, read without repairing anything), cached for `HOOK_STATUS_TTL`; `None` when the status
/// could not be read.
pub(super) fn agent_hook_installed(
    paths: &GxserverPaths,
    agent_id: &str,
    cache: &Mutex<Option<(Instant, Option<bool>)>>,
) -> Option<bool> {
    let mut cached = lock(cache);
    if let Some((at, installed)) = *cached {
        if at.elapsed() < HOOK_STATUS_TTL {
            return installed;
        }
    }
    let mut params = Map::new();
    params.insert("agentIds".to_string(), json!([agent_id]));
    params.insert("autoUpgradeInstalled".to_string(), json!(false));
    let installed = crate::agent_hooks::read_agent_hook_status(paths, &params)
        .ok()
        .and_then(|status| {
            status["agents"]
                .as_array()?
                .iter()
                .find(|row| row["agentId"] == agent_id)?
                .get("hookInstalled")?
                .as_bool()
        });
    *cached = Some((Instant::now(), installed));
    installed
}

/// The sessions `agent`'s watcher follows right now (none while it does not drive).
pub(super) fn set_tracked<'a>(
    agent: &'static str,
    panes: impl Iterator<Item = &'a (String, String)>,
) {
    lock(&TRACKED).insert(agent, panes.cloned().collect());
}

pub(super) fn is_tracked(agent: &str, pane: &(String, String)) -> bool {
    lock(&TRACKED)
        .get(agent)
        .is_some_and(|panes| panes.contains(pane))
}

/// The agents whose watcher can stand in for their hook.
fn watched_agent(agent: Option<&str>) -> Option<&'static str> {
    match agent? {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

/// Whether a hook event no longer drives state: its agent's watcher drives and follows the session
/// it names, and the session did not fall back to the hook. A session the watcher cannot see (a
/// Claude older than 2.1.172, which writes no session record, a Codex whose rollout is not found
/// yet) keeps its hook.
pub(crate) fn hook_superseded(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> bool {
    if endpoint_path != "/api/ingestAgentHookEvent" || params.contains_key(OBSERVER_MARK) {
        return false;
    }
    let normalized =
        crate::agents::normalize_agent_id(params.get("agentName").and_then(Value::as_str));
    let Some(agent) = watched_agent(normalized.as_deref()) else {
        return false;
    };
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::trim);
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return false;
    };
    let pane = (project_id.to_string(), session_id.to_string());
    is_tracked(agent, &pane)
        && super::agent_drive::drive_enabled(&state.paths, agent)
        && !hook_free::session_fell_back(&state.paths, project_id, session_id)
}

#[derive(Clone, Debug, PartialEq)]
struct ToolCall {
    name: String,
    input: Value,
    id: Option<String>,
}

/// Turns observer events into hook events and sends them through the hook path.
#[derive(Default)]
pub(super) struct ClaudeDriver {
    /// The tool call each pane's approval or question waits on, so the answer retires its card.
    waiting: HashMap<(String, String), ToolCall>,
    /// Waits whose tool call was not in the transcript yet, read again until this deadline.
    unread_waits: HashMap<(String, String), (ActivityEvent, Instant)>,
    /// The questions read off each pane's screen for the wait it shows, by the dialog's tab row,
    /// so a dialog of several questions has its tabs walked once (claude_question_screen.rs);
    /// `None` for one whose walk failed, which is not walked again.
    screen_calls: HashMap<(String, String), (String, Option<ToolCall>)>,
}

/// Claude can mark itself waiting a moment before it writes the asking call to its transcript
/// (seen live on 2026-11-01: an AskUserQuestion row 50ms after the record), or before it draws the
/// question it never writes there (2.1.322, claude_question_screen.rs), so a wait whose call is
/// missing is read again for this long and sent once more with the card when it appears.
const UNREAD_WAIT_RETRY: Duration = Duration::from_secs(8);

/// How long one tab step waits for the screen to change.
const TAB_STEP_WAIT: Duration = Duration::from_millis(1500);

/// Writes `key` to the pane's terminal and returns its screen once it has changed from `before`.
fn screen_after_key(zmx_name: &str, before: &str, key: &str) -> Option<String> {
    crate::zmx::session_chat_zmx_write(zmx_name, key).ok()?;
    let started = Instant::now();
    while started.elapsed() < TAB_STEP_WAIT {
        std::thread::sleep(Duration::from_millis(80));
        let capture = crate::zmx::read_zmx_session_screen_capture(zmx_name).ok()?;
        if !capture.truncated && capture.text != before {
            return Some(capture.text);
        }
    }
    None
}

/// The row of tabs of a question dialog on screen, which names its questions.
fn question_tab_row(screen: &str) -> Option<String> {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let row = crate::session_chat_question_liveness::claude_question_tab_row(&lines)?;
    Some(lines[row].trim().to_string())
}

impl ClaudeDriver {
    /// The tool call a wait waits on: the transcript's newest call without a result, else, for a
    /// question (`input needed`), the questions Claude draws on screen, a dialog of several read
    /// by walking its tabs once.
    fn waiting_call(
        &mut self,
        state: &AppState,
        pane: &(String, String),
        event: &ActivityEvent,
    ) -> Option<ToolCall> {
        if let Some(call) = event.transcript.as_deref().and_then(pending_tool_call) {
            return Some(call);
        }
        if event.payload.get("waitingFor").and_then(Value::as_str) != Some("input needed") {
            return None;
        }
        let zmx_name = {
            let db = open_gxserver_database(&state.paths).ok()?;
            let repository = DomainRepository::new(&db, &state.metadata.server_id);
            let session = repository.get_session(&pane.0, &pane.1).ok()??;
            crate::zmx::provider_zmx_session_name(&session).ok()?
        };
        let capture = crate::zmx::read_zmx_session_screen_capture(&zmx_name).ok()?;
        if capture.truncated {
            return None;
        }
        let tab_row = question_tab_row(&capture.text)?;
        if let Some((known, call)) = self.screen_calls.get(pane) {
            if *known == tab_row {
                return call.clone();
            }
        }
        let walks = super::claude_question_screen::several_questions_on_screen(&capture.text);
        let mut screen = capture.text;
        let call = super::claude_question_screen::questions_on_screen(&screen.clone(), |key| {
            let next = screen_after_key(&zmx_name, &screen, key)?;
            screen = next.clone();
            Some(next)
        })
        .map(|input| ToolCall {
            name: "AskUserQuestion".to_string(),
            input,
            id: None,
        });
        if call.is_some() || walks {
            self.screen_calls
                .insert(pane.clone(), (tab_row, call.clone()));
        }
        call
    }

    /// Sends the waits whose tool call has reached the transcript since they were first sent.
    pub(super) fn retry_unread_waits(&mut self, state: &AppState) {
        let now = Instant::now();
        let panes: Vec<_> = self.unread_waits.keys().cloned().collect();
        for pane in panes {
            let Some((event, deadline)) = self.unread_waits.get(&pane) else {
                continue;
            };
            if *deadline <= now {
                self.unread_waits.remove(&pane);
                continue;
            }
            let event = event.clone();
            if self.waiting_call(state, &pane, &event).is_some() {
                if let Some((event, _)) = self.unread_waits.remove(&pane) {
                    self.drive(state, &event);
                }
            }
        }
    }

    pub(super) fn drive(&mut self, state: &AppState, event: &ActivityEvent) {
        if event.compare_only {
            return;
        }
        let pane = (event.project_id.clone(), event.session_id.clone());
        self.unread_waits.remove(&pane);
        // A new question is read off the screen again.
        if event.event_name != "PermissionRequest" {
            self.screen_calls.remove(&pane);
        }
        // Its hook drives a session that fell back to it (hook_free_fallback.rs).
        if hook_free::session_fell_back(&state.paths, &pane.0, &pane.1) {
            return;
        }
        let Some(params) = self.params(state, &pane, event) else {
            return;
        };
        let event_name = params.get("eventName").cloned();
        let adopted = params.contains_key("agentSessionId");
        let response = crate::server::dispatch_agent_http_blocking(
            state,
            "/api/ingestAgentHookEvent".to_string(),
            format!("hook-free-{}", uuid::Uuid::new_v4()),
            params,
        );
        let _ = state.logger.log_routine(
            DiagnosticLogScenario::HooklessActivityComparison,
            GxserverLogInput {
                level: LogLevel::Debug,
                event: "hooklessActivity.drive".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: Some(json!({
                    "projectId": pane.0,
                    "sessionId": pane.1,
                    "event": event_name,
                    "observerEvent": event.event_name,
                    "baseline": event.baseline,
                    "conversationSent": adopted,
                    "status": response.response.status().as_u16(),
                })),
            },
        );
    }

    fn params(
        &mut self,
        state: &AppState,
        pane: &(String, String),
        event: &ActivityEvent,
    ) -> Option<Map<String, Value>> {
        let transcript = event.transcript.as_deref();
        let mut payload = event.payload.clone();
        // A baseline is where a pane already stood when the observer first looked (gxserver
        // started, the switch turned on): it only fills in what the hooks may have missed, so it
        // never unparks a session or ends a finished turn's attention.
        let (event_name, with_status) = match (event.baseline, event.event_name.as_str()) {
            (true, "UserPromptSubmit") => ("SessionBusy".to_string(), true),
            (false, "SessionBusy") => match self.waiting.remove(pane) {
                // The answer reached Claude: the asking call's completion retires its card.
                Some(call) => {
                    insert_tool(&mut payload, &call);
                    ("PostToolUse".to_string(), true)
                }
                None => ("SessionBusy".to_string(), true),
            },
            (_, "PermissionRequest") => {
                match self.waiting_call(state, pane, event) {
                    Some(call) => {
                        insert_tool(&mut payload, &call);
                        self.waiting.insert(pane.clone(), call);
                    }
                    None => {
                        self.waiting.remove(pane);
                        self.unread_waits.insert(
                            pane.clone(),
                            (event.clone(), Instant::now() + UNREAD_WAIT_RETRY),
                        );
                    }
                }
                ("PermissionRequest".to_string(), true)
            }
            (true, _) => ("SessionStart".to_string(), false),
            (false, name) => {
                if matches!(
                    name,
                    "Stop"
                        | "StopFailure"
                        | "Interrupt"
                        | "PostCompact"
                        | "SessionEnd"
                        | "SessionStart"
                ) {
                    self.waiting.remove(pane);
                }
                (name.to_string(), true)
            }
        };
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(pane.0));
        params.insert("sessionId".to_string(), json!(pane.1));
        params.insert("agentName".to_string(), json!("claude"));
        params.insert("eventName".to_string(), json!(event_name));
        params.insert("rawEventName".to_string(), json!(event_name));
        params.insert(OBSERVER_MARK.to_string(), json!(true));
        if with_status {
            // The status the notify hook posts, which decides the events gxserver's own table
            // leaves to it (SessionStart, PostCompact, Interrupt).
            if let Some(status) = crate::agent_hooks::event_mapping::activity_for_hook_event(
                "claude",
                &event_name,
                &Value::Object(payload.clone()),
            ) {
                params.insert("status".to_string(), json!(status));
            }
            if let Some(source) = payload.get("source").and_then(Value::as_str) {
                params.insert("hookSource".to_string(), json!(source));
            }
        }
        // `background_tasks` decides a Stop's activity again on the hook path, as the notify hook
        // forwards it (CDXC:Notifications 2026-09-15 in agent_hooks/event_mapping.rs).
        for key in [
            "background_tasks",
            "source",
            "trigger",
            "notificationKind",
            "toolName",
            "toolInput",
            "toolUseId",
        ] {
            if let Some(value) = payload.get(key) {
                params.insert(key.to_string(), value.clone());
            }
        }
        if let Some(conversation) = event.agent_session_id.as_deref() {
            if adoptable(state, pane, conversation, transcript) {
                params.insert("agentSessionId".to_string(), json!(conversation));
                if let Some(path) = transcript {
                    params.insert(
                        "agentSessionPath".to_string(),
                        json!(path.to_string_lossy()),
                    );
                }
            }
        }
        // What the notify hook sends as `firstUserMessage` (the prompt of a UserPromptSubmit),
        // read back from the transcript; a first turn whose prompt is not written yet when it
        // starts is named when it ends. A prompt folded into a running turn, or a turn read from
        // the transcript after the next one started, names its own prompt (claude_turns.rs).
        if !event.baseline && matches!(event_name.as_str(), "UserPromptSubmit" | "Stop") {
            let named = payload
                .get("prompt")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|prompt| is_actual_user_prompt(prompt))
                .map(str::to_string);
            let prompt = if payload.contains_key("prompt") {
                named
            } else {
                transcript.and_then(newest_user_prompt)
            };
            if let Some(prompt) = prompt {
                params.insert("firstUserMessage".to_string(), json!(prompt));
            }
        }
        Some(params)
    }
}

fn insert_tool(payload: &mut Map<String, Value>, call: &ToolCall) {
    payload.insert("toolName".to_string(), json!(call.name));
    if !call.input.is_null() {
        payload.insert("toolInput".to_string(), call.input.clone());
    }
    if let Some(id) = &call.id {
        payload.insert("toolUseId".to_string(), json!(id));
    }
}

/// Whether the session may take `conversation` now. Claude names a new conversation (/clear, a
/// fresh start) before writing a line of it, and gxserver's identity guard refuses such an id while
/// the session's current conversation is on disk, together with the whole event
/// (`unwritten_conversation_conflict` in agents/identity/conflicts.rs). The observer leaves the id
/// out until its transcript exists, so the activity still lands; a later event carries it.
fn adoptable(
    state: &AppState,
    pane: &(String, String),
    conversation: &str,
    transcript: Option<&Path>,
) -> bool {
    if transcript.is_some() {
        return true;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(session)) = repository.get_session(&pane.0, &pane.1) else {
        return false;
    };
    let stored = |key: &str| {
        session
            .pointer(&format!("/runtimeSettings/{key}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    match stored("agentSessionId") {
        None => true,
        Some(current) if current == conversation => true,
        Some(_) => !stored("agentSessionPath")
            .map(|path| crate::resume_lookup::expand_home(&path))
            .is_some_and(|path| path.is_file()),
    }
}

/// The newest lines of a transcript, oldest first, without a partial first line.
fn tail_entries(path: &Path) -> Vec<Value> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or_default();
    let start = length.saturating_sub(TRANSCRIPT_TAIL);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if start > 0 {
        lines.next();
    }
    lines
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

/// The main thread's newest tool call with no result yet: what an approval or question waits on.
fn pending_tool_call(path: &Path) -> Option<ToolCall> {
    pending_tool_call_in(&tail_entries(path))
}

fn pending_tool_call_in(entries: &[Value]) -> Option<ToolCall> {
    let mut open: Vec<ToolCall> = Vec::new();
    for entry in entries {
        if entry["isSidechain"] == true {
            continue;
        }
        let Some(blocks) = entry["message"]["content"].as_array() else {
            continue;
        };
        for block in blocks {
            match block["type"].as_str() {
                Some("tool_use") if entry["type"] == "assistant" => {
                    if let Some(name) = block["name"].as_str() {
                        open.push(ToolCall {
                            name: name.to_string(),
                            input: block["input"].clone(),
                            id: block["id"].as_str().map(str::to_string),
                        });
                    }
                }
                Some("tool_result") => {
                    let id = block["tool_use_id"].as_str();
                    open.retain(|call| call.id.as_deref() != id);
                }
                _ => {}
            }
        }
    }
    open.pop()
}

/// The newest prompt the user typed, as the notify hook would send it.
fn newest_user_prompt(path: &Path) -> Option<String> {
    newest_user_prompt_in(&tail_entries(path))
}

fn newest_user_prompt_in(entries: &[Value]) -> Option<String> {
    entries.iter().rev().find_map(|entry| {
        if entry["type"] != "user"
            || entry["isMeta"] == true
            || entry["isCompactSummary"] == true
            || entry["isSidechain"] == true
        {
            return None;
        }
        let text = match &entry["message"]["content"] {
            Value::String(text) => text.clone(),
            Value::Array(parts) => parts
                .iter()
                .filter(|part| part["type"] == "text")
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => return None,
        };
        let text = text.trim();
        if text.is_empty()
            || text.starts_with("<command-name>")
            || text.starts_with("<local-command")
            || text.starts_with("[Request interrupted by user")
        {
            return None;
        }
        is_actual_user_prompt(text).then(|| text.to_string())
    })
}

/// `is_actual_user_message_prompt` in agent_hooks/notify_runtime.rs: slash and meta submissions
/// are commands, not the user's message.
fn is_actual_user_prompt(prompt: &str) -> bool {
    let Some(normalized) =
        crate::agents::activity::normalize_first_prompt_title_claim_prompt(Some(prompt))
    else {
        return false;
    };
    !crate::agents::activity::is_first_prompt_claim_meta_prompt(&normalized)
        && !crate::agents::activity::is_first_prompt_claim_slash_command(Some(prompt), &normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_observer_drives_hook_free_or_where_the_hook_is_missing() {
        assert_eq!(
            drive_reason_for(true, Some(true)),
            Some(DriveReason::HookFree)
        );
        assert_eq!(
            drive_reason_for(false, Some(false)),
            Some(DriveReason::HooksMissing)
        );
        assert_eq!(drive_reason_for(false, Some(true)), None);
        // A status that could not be read keeps the hooks.
        assert_eq!(drive_reason_for(false, None), None);
    }

    #[test]
    fn the_waiting_call_is_the_newest_without_a_result() {
        let entries = vec![
            json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "a", "name": "Bash", "input": {"command": "ls"}}]}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "a"}]}}),
            json!({"type": "assistant", "isSidechain": true, "message": {"content": [{"type": "tool_use", "id": "s", "name": "Read", "input": {}}]}}),
            json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "q", "name": "AskUserQuestion", "input": {"questions": []}}]}}),
        ];
        let call = pending_tool_call_in(&entries).unwrap();
        assert_eq!(call.name, "AskUserQuestion");
        assert_eq!(call.id.as_deref(), Some("q"));
        assert_eq!(pending_tool_call_in(&entries[..2]), None);
    }

    #[test]
    fn the_prompt_skips_commands_tool_results_and_interrupts() {
        let entries = vec![
            json!({"type": "user", "message": {"content": "name the planets"}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "a", "content": "x"}]}}),
            json!({"type": "user", "message": {"content": "<command-name>/model</command-name>"}}),
            json!({"type": "user", "isMeta": true, "message": {"content": "Caveat"}}),
            json!({"type": "user", "message": {"content": [{"type": "text", "text": "[Request interrupted by user]"}]}}),
        ];
        assert_eq!(
            newest_user_prompt_in(&entries).as_deref(),
            Some("name the planets")
        );
    }
}
