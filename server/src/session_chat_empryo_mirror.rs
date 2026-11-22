/*
CDXC:SessionChat 2026-10-06 WHY:
Empryo (3.9.0-beta) appends `<project>/.empryo/sessions/<id>/session.jsonl`, the path its hooks report, and writes a whole turn as ONE record: the user's prompt, then a cumulative `turn-checkpoint` snapshot of the reply while it works (not on every turn), then the finished `assistant` record, each carrying ordered `segments` (`text`, `reasoning`, `tools`) over a `toolCalls` list whose entries hold their own results. The chat contract decodes one message per line, so this module mirrors the log into a Ghostex-owned jsonl in the gxserver state dir with one row per chat message plus explicit turn rows, and `session_chat_decode_empryo.rs` stays line-local. A turn's rows come from its newest snapshot, so a checkpoint or the final record rewrites the mirror's tail, which the rename-on-rewrite contract the Antigravity and Cursor mirrors share reports to the follower as replaced content. Older logs reuse one `turnId` for every prompt in a tab, so turns are keyed by the user record's own `ui.id`, which is unique in every format seen.
*/

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::resume_lookup::home_dir;
use crate::session_chat::{extract_string, parse_json_object, tool_result_output};
use crate::session_chat_empryo_events::{read_empryo_screen_events, EmpryoScreenEvents};
use crate::session_chat_empryo_notes::{anchor_at, insert_notices, notice_row, EmpryoTabNotes};

const EMPRYO_SESSION_ID_MAX_LENGTH: usize = 128;

pub(crate) fn is_safe_empryo_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= EMPRYO_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn empryo_mirror_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("empryo-chat-mirror")
}

/// The session id a raw log path belongs to: `…/.empryo/sessions/<id>/session.jsonl`.
fn session_id_from_raw_path(path: &Path) -> Option<String> {
    if path.file_name().and_then(|name| name.to_str()) != Some("session.jsonl") {
        return None;
    }
    let id = path.parent()?.file_name()?.to_str()?.to_string();
    is_safe_empryo_session_id(&id).then_some(id)
}

/* ------------------------------------------------------------ tools */

/// Empryo's tools under the names and argument keys the chat's tool rules already render
/// (`Bash(…)` rows, `Read` previews, `Edit` diffs). A tool with no counterpart keeps its own.
fn empryo_canonical_tool_call(name: &str, args: Value) -> (String, Value) {
    let Some(record) = args.as_object() else {
        return (name.to_string(), args);
    };
    let field = |key: &str| record.get(key).filter(|value| !value.is_null()).cloned();
    let object = |entries: Vec<(&str, Option<Value>)>| {
        Value::Object(
            entries
                .into_iter()
                .filter_map(|(key, value)| value.map(|value| (key.to_string(), value)))
                .collect(),
        )
    };
    let canonical = match name {
        "shell" => Some((
            "Bash",
            object(vec![
                ("command", field("command")),
                ("timeout", field("timeout")),
                ("run_in_background", field("background")),
            ]),
        )),
        "edit_file" if record.contains_key("oldString") => Some((
            "Edit",
            object(vec![
                ("file_path", field("path")),
                ("old_string", field("oldString")),
                ("new_string", field("newString")),
            ]),
        )),
        "read" => {
            // One file reads like Claude's Read; several files make one Read row naming them.
            let files = record.get("files").and_then(Value::as_array);
            match files.map(Vec::as_slice) {
                Some([file]) => {
                    let file = file.as_object();
                    let range = file
                        .and_then(|file| file.get("ranges"))
                        .and_then(Value::as_array)
                        .filter(|ranges| ranges.len() == 1)
                        .and_then(|ranges| ranges[0].as_object());
                    let start = range
                        .and_then(|range| range.get("start"))
                        .and_then(Value::as_u64);
                    let end = range
                        .and_then(|range| range.get("end"))
                        .and_then(Value::as_u64);
                    Some((
                        "Read",
                        object(vec![
                            ("file_path", file.and_then(|file| file.get("path")).cloned()),
                            ("offset", start.map(Value::from)),
                            (
                                "limit",
                                start
                                    .zip(end)
                                    .filter(|(start, end)| end >= start)
                                    .map(|(start, end)| Value::from(end - start + 1)),
                            ),
                        ]),
                    ))
                }
                // Several files read as one Read row naming them, as Empryo's own row does.
                Some(files) if !files.is_empty() => {
                    let paths: Vec<String> = files
                        .iter()
                        .filter_map(|file| extract_string(file.get("path")))
                        .collect();
                    Some((
                        "Read",
                        object(vec![
                            ("description", Some(Value::from(paths.join(", ")))),
                            ("files", field("files")),
                        ]),
                    ))
                }
                _ => None,
            }
        }
        // `run_tool` runs one of Empryo's deferred tools by name; its row names that tool.
        "run_tool" => {
            let mut input = record.clone();
            if let Some(tool) = extract_string(record.get("name")) {
                input.insert("description".into(), Value::from(tool));
            }
            Some(("run_tool", Value::Object(input)))
        }
        "grep" => Some((
            "Grep",
            object(vec![("pattern", field("pattern")), ("path", field("path"))]),
        )),
        "web_search" => Some(("WebSearch", object(vec![("query", field("query"))]))),
        "fetch_page" => Some(("WebFetch", object(vec![("url", field("url"))]))),
        _ => None,
    };
    match canonical {
        Some((canonical_name, input)) => (canonical_name.to_string(), input),
        None => (name.to_string(), args),
    }
}

/// `final_response` only marks where Empryo's answer starts streaming; its TUI draws no row.
fn is_hidden_empryo_tool(name: &str) -> bool {
    name == "final_response"
}

/// CDXC:SessionChat 2026-10-07 WHY:
/// A `code_script` call runs Empryo's tools from its script and logs each as its own `tool-start` with a `hidden-<tool>-<uuid>` id; the TUI draws only the script's row ("Synthesized") and the final record holds only the script, so the chat showing the inner calls while the turn ran made rows appear and then vanish.
fn is_hidden_empryo_call(id: &str) -> bool {
    id.starts_with("hidden-")
}

fn empryo_tool_result(result: &Value) -> (String, bool) {
    let Some(record) = result.as_object() else {
        return (
            result.as_str().map(str::to_string).unwrap_or_default(),
            false,
        );
    };
    let failed = record.get("success") == Some(&Value::Bool(false));
    let output = extract_string(record.get("output"));
    let error = extract_string(record.get("error"));
    let text = match (output, error) {
        (Some(output), Some(error)) if failed => format!("{output}\n{error}"),
        (Some(output), _) => output,
        (None, Some(error)) => error,
        (None, None) => String::new(),
    };
    (text, failed)
}

/// A `tool-end` record's `output`: `{"type": "text" | "error-text", "value": …}`.
fn empryo_tool_end(output: Option<&Value>, at: Value) -> EmpryoToolEnd {
    let is_error = output
        .and_then(|output| output.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.starts_with("error"));
    let output = tool_result_output(output.map(|output| output.get("value").unwrap_or(output)));
    EmpryoToolEnd {
        output,
        is_error,
        at,
    }
}

/* ------------------------------------------------------------ turns */

struct EmpryoTurn {
    /// The user record's own `ui.id`, unique per prompt in every log format.
    key: String,
    /// The record's `turnId`, which `ui-truncate` names (shared by a tab's prompts in old logs).
    turn_id: Option<String>,
    user: Map<String, Value>,
    /// The newest snapshot of the reply: the last checkpoint, then the final record.
    reply: Option<Map<String, Value>>,
    /// `complete` or `partial` once the final `assistant` record landed.
    end_status: Option<String>,
    /// The tools `tool-start`/`tool-end` records reported while the turn runs, in start order.
    live_calls: Vec<EmpryoLiveCall>,
    /// When Empryo accepted the prompt, which is before it briefs the model and logs the record.
    started_at: Option<i64>,
}

impl EmpryoTurn {
    /// When the turn began: its acceptance, else its prompt's record.
    fn start(&self) -> Option<i64> {
        self.started_at
            .or_else(|| self.user.get("timestamp").and_then(Value::as_i64))
    }
}

struct EmpryoLiveCall {
    id: String,
    name: String,
    args: Value,
    started_at: Value,
    /// Set once `tool-end` landed.
    ended: Option<EmpryoToolEnd>,
}

struct EmpryoToolEnd {
    output: String,
    is_error: bool,
    at: Value,
}

#[derive(Default)]
struct EmpryoLog {
    turns_by_tab: HashMap<String, Vec<EmpryoTurn>>,
    /// The newest `pendingPrompts` patch per tab, in the order its prompts were typed.
    pending_by_tab: HashMap<String, Vec<(String, String)>>,
    /// CDXC:SessionChat 2026-10-06 DECISION:
    /// Sven: one Ghostex session follows one Empryo tab, the tab of the most recent user turn; records from its other tabs are hidden.
    followed_tab: Option<String>,
    /// CDXC:SessionChat 2026-10-07 WHY:
    /// Empryo 3.9.1-beta logs every window that joins a repository's shared engine in the engine's session, so one `session.jsonl` holds other Ghostex sessions' tabs too (their first tab, session_chat_empryo_tabs.rs). Those are never this session's to follow.
    foreign_tabs: HashSet<String>,
    /// `pendingPrompts[].acceptedAt` by `turnId`, the newest acceptance winning (old logs reuse ids).
    accepted_at: HashMap<String, i64>,
    /// Each tab's model and effort changes and Empryo's own `system` rows.
    notes_by_tab: HashMap<String, EmpryoTabNotes>,
}

const OBSERVED_KINDS: [&str; 9] = [
    "\"k\":\"tab\"",
    "\"k\":\"user\"",
    "\"k\":\"turn-checkpoint\"",
    "\"k\":\"assistant\"",
    "\"k\":\"tool-start\"",
    "\"k\":\"tool-end\"",
    "\"k\":\"ui-truncate\"",
    "\"k\":\"ui-clear\"",
    "\"k\":\"system\"",
];

pub(crate) fn pending_prompts(patch: &Value) -> Option<Vec<(String, String)>> {
    let pending = patch.get("pendingPrompts")?.as_object()?;
    Some(
        pending
            .iter()
            .filter_map(|(key, entry)| {
                let turn_id = extract_string(entry.get("turnId")).unwrap_or_else(|| key.clone());
                let text = extract_string(entry.get("text"))?;
                Some((turn_id, text))
            })
            .collect(),
    )
}

impl EmpryoLog {
    /// CDXC:SessionChat 2026-10-06 DECISION:
    /// Sven: Empryo's chat reads the `ui` copy of the `user`, `assistant` and `turn-checkpoint` records and ignores the `core` copy, which carries the repository map Empryo appends to every prompt.
    fn observe(&mut self, line: &str) {
        // Every record opens with its kind; skip the large ones chat never reads (edit
        // baselines, `core` rewrites) before parsing them.
        let head = line.get(..32).unwrap_or(line);
        if !OBSERVED_KINDS.iter().any(|kind| head.contains(kind)) {
            return;
        }
        let Some(mut record) = parse_json_object(line) else {
            return;
        };
        let Some(tab) = extract_string(record.get("tabId")) else {
            return;
        };
        let kind = extract_string(record.get("k")).unwrap_or_default();
        match kind.as_str() {
            "tab" => {
                let Some(patch) = record.get("patch") else {
                    return;
                };
                let at = record.get("ts").and_then(Value::as_i64).unwrap_or_default();
                self.notes_by_tab
                    .entry(tab.clone())
                    .or_default()
                    .observe_patch(&tab, patch, at);
                for (key, entry) in patch
                    .get("pendingPrompts")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    if let Some(accepted) = entry.get("acceptedAt").and_then(Value::as_i64) {
                        let turn_id = extract_string(entry.get("turnId")).unwrap_or(key.clone());
                        self.accepted_at.insert(turn_id, accepted);
                    }
                }
                let Some(pending) = pending_prompts(patch) else {
                    return;
                };
                if !pending.is_empty() && !self.foreign_tabs.contains(&tab) {
                    self.followed_tab = Some(tab.clone());
                }
                self.pending_by_tab.insert(tab, pending);
            }
            "user" => {
                let Some(Value::Object(ui)) = record.remove("ui") else {
                    return;
                };
                let Some(key) = extract_string(ui.get("id")) else {
                    return;
                };
                if !self.foreign_tabs.contains(&tab) {
                    self.followed_tab = Some(tab.clone());
                }
                let turn_id = extract_string(record.get("turnId"));
                let recorded_at = ui.get("timestamp").and_then(Value::as_i64);
                let started_at = turn_id
                    .as_ref()
                    .and_then(|turn_id| self.accepted_at.get(turn_id))
                    .copied()
                    .filter(|accepted| recorded_at.is_none_or(|at| *accepted <= at));
                self.turns_by_tab.entry(tab).or_default().push(EmpryoTurn {
                    key,
                    turn_id,
                    user: ui,
                    reply: None,
                    end_status: None,
                    live_calls: Vec::new(),
                    started_at,
                });
            }
            // CDXC:SessionChat 2026-10-07 WHY:
            // Empryo 3.9.1-beta writes no `turn-checkpoint` while a turn runs; it logs each tool as a `tool-start` record and its output as a `tool-end` record, then the whole reply only when the turn ends. Those records are what lets the chat show a long turn's tools as they run instead of nothing until its answer.
            "tool-start" | "tool-end" => {
                let (Some(turn_id), Some(call_id)) = (
                    extract_string(record.get("turnId")),
                    extract_string(record.get("toolCallId")),
                ) else {
                    return;
                };
                // Older logs reuse one `turnId` for a tab's prompts, so the newest such turn owns it.
                let Some(turn) = self.turns_by_tab.get_mut(&tab).and_then(|turns| {
                    turns
                        .iter_mut()
                        .rev()
                        .find(|turn| turn.turn_id.as_deref() == Some(turn_id.as_str()))
                }) else {
                    return;
                };
                if turn.end_status.is_some() {
                    return;
                }
                let timestamp = record.get("ts").cloned().unwrap_or(Value::Null);
                if kind == "tool-start" {
                    if turn.live_calls.iter().all(|call| call.id != call_id) {
                        turn.live_calls.push(EmpryoLiveCall {
                            id: call_id,
                            name: extract_string(record.get("name"))
                                .unwrap_or_else(|| "tool".into()),
                            args: record.remove("args").unwrap_or(Value::Null),
                            started_at: timestamp,
                            ended: None,
                        });
                    }
                } else if let Some(call) =
                    turn.live_calls.iter_mut().find(|call| call.id == call_id)
                {
                    call.ended = Some(empryo_tool_end(record.get("output"), timestamp));
                }
            }
            "turn-checkpoint" | "assistant" => {
                let Some(Value::Object(ui)) = record.remove("ui") else {
                    return;
                };
                let Some(turn) = self
                    .turns_by_tab
                    .get_mut(&tab)
                    .and_then(|turns| turns.last_mut())
                else {
                    return;
                };
                if turn.end_status.is_some() {
                    return;
                }
                turn.reply = Some(ui);
                if kind == "assistant" {
                    // The final record holds every call with its result.
                    turn.live_calls.clear();
                    turn.end_status = Some(
                        extract_string(record.get("status")).unwrap_or_else(|| "complete".into()),
                    );
                }
            }
            // CDXC:SessionChat 2026-10-06 WHY:
            // `/checkpoint undo` (and Empryo's other rewinds) cut the tab's history with a `ui-truncate` from a turn or a message, and `/clear` with a `ui-clear`. Empryo's own replay drops everything from that point, so the chat drops the same turns its terminal stops showing.
            "ui-truncate" => {
                let Some(turns) = self.turns_by_tab.get_mut(&tab) else {
                    return;
                };
                let mut cut = None;
                if let Some(message_id) = extract_string(record.get("fromMessageId")) {
                    if let Some(index) = turns.iter().position(|turn| turn.key == message_id) {
                        cut = turns[index].start();
                        turns.truncate(index);
                    } else if let Some(index) = turns.iter().position(|turn| {
                        turn.reply
                            .as_ref()
                            .and_then(|reply| extract_string(reply.get("id")))
                            .as_deref()
                            == Some(message_id.as_str())
                    }) {
                        turns.truncate(index + 1);
                        // The reply is gone, so the turn waits for the one Empryo writes next.
                        turns[index].reply = None;
                        turns[index].end_status = None;
                        turns[index].live_calls.clear();
                    }
                } else if let Some(turn_id) = extract_string(record.get("fromTurnId")) {
                    if let Some(index) = turns
                        .iter()
                        .position(|turn| turn.turn_id.as_deref() == Some(turn_id.as_str()))
                    {
                        cut = turns[index].start();
                        turns.truncate(index);
                    }
                }
                if let (Some(cut), Some(notes)) = (cut, self.notes_by_tab.get_mut(&tab)) {
                    notes.cut(cut);
                }
            }
            "ui-clear" => {
                self.turns_by_tab.remove(&tab);
                self.notes_by_tab.remove(&tab);
            }
            "system" => {
                let Some(Value::Object(ui)) = record.remove("ui") else {
                    return;
                };
                let at = ui
                    .get("timestamp")
                    .or_else(|| record.get("ts"))
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                self.notes_by_tab
                    .entry(tab.clone())
                    .or_default()
                    .observe_system(&tab, &ui, at);
            }
            _ => {}
        }
    }
}

/* ------------------------------------------------------------ rows */

/// One call in an `assistant` row's `toolCalls`, or `None` for a tool Empryo draws no row for.
fn tool_call_entry(id: &str, name: &str, args: Value) -> Option<Value> {
    if is_hidden_empryo_tool(name) {
        return None;
    }
    let (name, input) = empryo_canonical_tool_call(name, args);
    Some(json!({ "callId": id, "name": name, "input": input }))
}

/// Named after its first call, so a call keeps its row id from `tool-start` to the final record.
fn tool_calls_row(key: &str, first_id: &str, timestamp: &Value, calls: Vec<Value>) -> Value {
    json!({
        "row": "assistant", "id": format!("empryo:{key}:calls:{first_id}"), "turn": key,
        "ts": timestamp, "toolCalls": calls,
    })
}

fn tool_result_row(
    key: &str,
    id: &str,
    timestamp: &Value,
    output: String,
    is_error: bool,
) -> Value {
    json!({
        "row": "tool", "id": format!("empryo:{key}:result:{id}"), "turn": key,
        "ts": timestamp, "callId": id, "output": output, "isError": is_error,
    })
}

/// The tools a running turn has started that its newest snapshot does not hold yet, one row per
/// call with its result once it ended.
fn push_live_call_rows(rows: &mut Vec<Value>, turn: &EmpryoTurn) {
    let snapshot_calls: HashSet<&str> = turn
        .reply
        .as_ref()
        .and_then(|reply| reply.get("toolCalls"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|call| call.get("id")?.as_str())
        .collect();
    let key = &turn.key;
    for call in &turn.live_calls {
        if snapshot_calls.contains(call.id.as_str()) || is_hidden_empryo_call(&call.id) {
            continue;
        }
        let Some(entry) = tool_call_entry(&call.id, &call.name, call.args.clone()) else {
            continue;
        };
        rows.push(tool_calls_row(key, &call.id, &call.started_at, vec![entry]));
        if let Some(end) = &call.ended {
            rows.push(tool_result_row(
                key,
                &call.id,
                &end.at,
                end.output.clone(),
                end.is_error,
            ));
        }
    }
}

fn push_reply_rows(rows: &mut Vec<Value>, turn: &EmpryoTurn, reply: &Map<String, Value>) {
    let key = &turn.key;
    let timestamp = reply
        .get("timestamp")
        .or_else(|| turn.user.get("timestamp"))
        .cloned()
        .unwrap_or(Value::Null);
    let tool_calls: HashMap<&str, &Map<String, Value>> = reply
        .get("toolCalls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|call| Some((call.get("id")?.as_str()?, call)))
        .collect();
    let mut emitted_calls: HashSet<&str> = HashSet::new();
    let mut push_tools = |rows: &mut Vec<Value>, ids: Vec<&str>| {
        let mut first_id = None;
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for id in ids {
            let Some((&id, call)) = tool_calls.get_key_value(id) else {
                continue;
            };
            if !emitted_calls.insert(id) {
                continue;
            }
            let name = extract_string(call.get("name")).unwrap_or_else(|| "tool".into());
            let args = call.get("args").cloned().unwrap_or(Value::Null);
            let Some(entry) = tool_call_entry(id, &name, args) else {
                continue;
            };
            first_id.get_or_insert(id);
            calls.push(entry);
            if let Some(result) = call.get("result").filter(|result| !result.is_null()) {
                let (output, is_error) = empryo_tool_result(result);
                results.push(tool_result_row(key, id, &timestamp, output, is_error));
            }
        }
        if let Some(first_id) = first_id {
            rows.push(tool_calls_row(key, first_id, &timestamp, calls));
        }
        rows.extend(results);
    };
    let segments = reply
        .get("segments")
        .and_then(Value::as_array)
        .filter(|segments| !segments.is_empty());
    let Some(segments) = segments else {
        // Records without segments carry their answer as `content` after any tool calls.
        let ids: Vec<&str> = tool_calls.keys().copied().collect();
        push_tools(rows, ids);
        if let Some(text) = extract_string(reply.get("content")) {
            rows.push(json!({
                "row": "assistant", "id": format!("empryo:{key}:text"), "turn": key,
                "ts": timestamp, "text": text,
            }));
        }
        return;
    };
    for (index, segment) in segments.iter().enumerate() {
        let Some(segment) = segment.as_object() else {
            continue;
        };
        match segment.get("type").and_then(Value::as_str) {
            Some(kind @ ("text" | "reasoning")) => {
                // Reasoning from providers that encrypt it is recorded with empty content.
                let Some(text) = extract_string(segment.get("content")) else {
                    continue;
                };
                let row = if kind == "text" {
                    "assistant"
                } else {
                    "reasoning"
                };
                rows.push(json!({
                    "row": row, "id": format!("empryo:{key}:{kind}:{index}"), "turn": key,
                    "ts": segment.get("at").cloned().unwrap_or_else(|| timestamp.clone()),
                    "text": text,
                }));
            }
            Some("tools") => {
                let ids: Vec<&str> = segment
                    .get("toolCallIds")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                push_tools(rows, ids);
            }
            _ => {}
        }
    }
}

/// CDXC:SessionChat 2026-10-06 DECISION:
/// Sven: a `pendingPrompts` tab patch or a `user` record starts an Empryo turn, an `assistant` record with status `complete` or `partial` ends it, and a `turn-checkpoint` updates the turn in flight.
fn turn_row(key: &str, state: &str, timestamp: Value) -> Value {
    json!({ "row": "turn", "turn": key, "state": state, "ts": timestamp })
}

/// The turn the followed tab is running, which the screen's events are read for
/// (session_chat_empryo_events.rs).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EmpryoInFlight {
    pub(crate) key: String,
    /// The prompt as typed, which the screen's newest prompt must match.
    pub(crate) prompt: String,
    /// The calls that have started, in start order, without the ones Empryo draws no row for.
    pub(crate) calls: Vec<String>,
}

/// One turn's rows, before its notes are placed.
struct TurnBlock {
    key: String,
    prompt: String,
    start: Option<i64>,
    /// When the final record landed; `None` while the turn runs.
    end: Option<i64>,
    rows: Vec<Value>,
    /// Where the turn's own rows start, after its prompt and working rows.
    body_start: usize,
    /// The calls Empryo draws a row for, (id, start), in start order.
    calls: Vec<(String, i64)>,
    /// The completed or interrupted row that closes it.
    closing: Option<Value>,
    /// Notes made while it ran: (time, anchor call, row).
    notes: Vec<(i64, Option<String>, Value)>,
    /// Notes made after it ended and before the next turn began.
    seam: Vec<Value>,
}

fn turn_calls(turn: &EmpryoTurn) -> Vec<(String, i64)> {
    let mut calls: Vec<(String, i64)> = Vec::new();
    let recorded = turn
        .reply
        .as_ref()
        .and_then(|reply| reply.get("toolCalls"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|call| {
            let name = call.get("name").and_then(Value::as_str).unwrap_or_default();
            Some((
                call.get("id")?.as_str()?,
                name,
                call.get("startedAt").and_then(Value::as_i64)?,
            ))
        });
    let live = turn.live_calls.iter().filter_map(|call| {
        Some((
            call.id.as_str(),
            call.name.as_str(),
            call.started_at.as_i64()?,
        ))
    });
    for (id, name, started) in recorded.chain(live) {
        if !is_hidden_empryo_tool(name)
            && !is_hidden_empryo_call(id)
            && calls.iter().all(|(known, _)| known != id)
        {
            calls.push((id.to_string(), started));
        }
    }
    calls.sort_by_key(|(_, started)| *started);
    calls
}

/// The mirror's rows for a raw log, and the turn the followed tab is running. Only complete lines
/// are read: a torn tail would otherwise be mirrored as a parse failure and never revisited.
fn mirror_rows(
    raw: &[u8],
    foreign_tabs: HashSet<String>,
    screen_events: &EmpryoScreenEvents,
) -> (Vec<Value>, Option<EmpryoInFlight>) {
    let complete = match raw.iter().rposition(|byte| *byte == b'\n') {
        Some(end) => &raw[..=end],
        None => &[][..],
    };
    let text = String::from_utf8_lossy(complete);
    let mut log = EmpryoLog {
        foreign_tabs,
        ..EmpryoLog::default()
    };
    for line in text.lines() {
        log.observe(line);
    }
    let Some(tab) = log.followed_tab.clone() else {
        return (Vec::new(), None);
    };
    let turns = log.turns_by_tab.remove(&tab).unwrap_or_default();
    let mut blocks: Vec<TurnBlock> = Vec::new();
    for turn in &turns {
        let mut rows: Vec<Value> = Vec::new();
        let user = &turn.user;
        let timestamp = user.get("timestamp").cloned().unwrap_or(Value::Null);
        let prompt = extract_string(user.get("content"));
        // Background-agent reports Empryo injects as hidden prompts still start a turn.
        let hidden = user.get("hidden") == Some(&Value::Bool(true));
        if let Some(text) = prompt.clone().filter(|_| !hidden) {
            let images: Vec<Value> = user
                .get("images")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|image| {
                    Value::from(
                        extract_string(image.get("label")).unwrap_or_else(|| "image".into()),
                    )
                })
                .collect();
            rows.push(json!({
                "row": "user", "id": format!("empryo:{}", turn.key), "turn": turn.key,
                "ts": timestamp, "text": text, "images": images,
            }));
        }
        rows.push(turn_row(&turn.key, "working", timestamp.clone()));
        let body_start = rows.len();
        if let Some(reply) = turn.reply.as_ref() {
            push_reply_rows(&mut rows, turn, reply);
        }
        let mut closing = None;
        let mut end = None;
        if let Some(status) = turn.end_status.as_deref() {
            let state = if status == "partial" {
                "interrupted"
            } else {
                "completed"
            };
            let ended_at = turn
                .reply
                .as_ref()
                .and_then(|reply| reply.get("timestamp").cloned())
                .unwrap_or_else(|| timestamp.clone());
            end = Some(ended_at.as_i64().unwrap_or(i64::MAX));
            closing = Some(turn_row(&turn.key, state, ended_at));
        } else {
            push_live_call_rows(&mut rows, turn);
        }
        blocks.push(TurnBlock {
            key: turn.key.clone(),
            prompt: prompt.unwrap_or_default(),
            start: turn.start(),
            end,
            rows,
            body_start,
            calls: turn_calls(turn),
            closing,
            notes: Vec::new(),
            seam: Vec::new(),
        });
    }
    // A prompt Empryo accepted but has not recorded yet (it briefs the model first, which can
    // take most of a minute) shows at once; its user record later takes the same id.
    for (turn_id, text) in log.pending_by_tab.remove(&tab).unwrap_or_default() {
        let key = format!("{turn_id}-user");
        if turns.iter().any(|turn| turn.key == key) {
            continue;
        }
        let rows = vec![
            json!({
                "row": "user", "id": format!("empryo:{key}"), "turn": key, "ts": Value::Null,
                "text": text, "images": [],
            }),
            turn_row(&key, "working", Value::Null),
        ];
        blocks.push(TurnBlock {
            key,
            prompt: text,
            start: log.accepted_at.get(&turn_id).copied(),
            end: None,
            rows,
            body_start: 2,
            calls: Vec::new(),
            closing: None,
            notes: Vec::new(),
            seam: Vec::new(),
        });
    }
    // A note made while a turn ran sits inside it, after the call that had started last; one made
    // between turns follows the turn before it, and one made before the first leads the chat.
    let mut leading: Vec<Value> = Vec::new();
    let notes = log
        .notes_by_tab
        .remove(&tab)
        .map(|notes| notes.notes)
        .unwrap_or_default();
    for note in notes {
        let owner = blocks
            .iter()
            .rposition(|block| block.start.is_some_and(|start| start <= note.at));
        let Some(owner) = owner else {
            leading.push(notice_row(&note.id, None, note.at, &note.text));
            continue;
        };
        let block = &mut blocks[owner];
        let row = notice_row(&note.id, Some(&block.key), note.at, &note.text);
        if block.end.is_some_and(|end| note.at > end) {
            block.seam.push(row);
        } else {
            let anchor = anchor_at(&block.calls, note.at);
            block.notes.push((note.at, anchor, row));
        }
    }
    for block in &mut blocks {
        for (index, event) in screen_events
            .get(&block.key)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let id = format!("empryo:{}:event:{index}", block.key);
            block.notes.push((
                event.seen_at,
                event.after.clone(),
                notice_row(&id, Some(&block.key), event.seen_at, &event.notice_text()),
            ));
        }
    }
    // The running turn is the newest recorded one while it is open; otherwise the first accepted
    // prompt, which Empryo is briefing. Prompts queued behind it are not running yet.
    let recorded = turns.len();
    let in_flight = blocks[..recorded]
        .last()
        .filter(|block| block.closing.is_none())
        .or_else(|| blocks[recorded..].first())
        .map(|block| EmpryoInFlight {
            key: block.key.clone(),
            prompt: block.prompt.clone(),
            calls: block.calls.iter().map(|(id, _)| id.clone()).collect(),
        });
    let mut rows = leading;
    for mut block in blocks {
        block.notes.sort_by_key(|(at, _, _)| *at);
        insert_notices(
            &mut block.rows,
            block.body_start,
            block
                .notes
                .into_iter()
                .map(|(_, call, row)| (call, row))
                .collect(),
        );
        rows.extend(block.rows);
        rows.extend(block.closing);
        rows.extend(block.seam);
    }
    (rows, in_flight)
}

/// The whole mirror for a raw log, one row per line, and the turn the followed tab is running.
fn build_mirror(
    raw: &[u8],
    foreign_tabs: HashSet<String>,
    screen_events: &EmpryoScreenEvents,
) -> (Vec<u8>, Option<EmpryoInFlight>) {
    let (rows, in_flight) = mirror_rows(raw, foreign_tabs, screen_events);
    let mut out = Vec::new();
    for row in rows {
        if let Ok(serialized) = serde_json::to_vec(&row) {
            out.extend_from_slice(&serialized);
            out.push(b'\n');
        }
    }
    (out, in_flight)
}

/// The visible prompts of the followed tab, oldest first: Generate Name's history source
/// (`agent_transcripts.rs`), read through the same rows the chat shows: from the log that holds
/// the session's own tab, without the tabs other Ghostex sessions own (a shared engine logs them
/// all in one session, so the newest prompt there can be another session's).
pub(crate) fn empryo_user_prompts(session_log: &Path) -> Vec<String> {
    let (log, foreign_tabs) = crate::session_chat_empryo_tabs::empryo_tab_scan(session_log)
        .unwrap_or_else(|| (session_log.to_path_buf(), HashSet::new()));
    let Ok(raw) = fs::read(&log) else {
        return Vec::new();
    };
    mirror_rows(&raw, foreign_tabs, &EmpryoScreenEvents::new())
        .0
        .iter()
        .filter(|row| row.get("row").and_then(Value::as_str) == Some("user"))
        .filter_map(|row| extract_string(row.get("text")))
        .collect()
}

/* ------------------------------------------------------------ sync */

#[derive(Default)]
struct EmpryoMirrorState {
    raw_path: PathBuf,
    /// The session's own log, which the tab scan starts from (session_chat_empryo_tabs.rs).
    own_path: PathBuf,
    /// The tabs other Ghostex sessions own, which the mirror leaves out.
    foreign_tabs: HashSet<String>,
    /// [`crate::session_chat_empryo_tabs::empryo_tab_scan_key`] at the last scan.
    scanned_at: Option<(Option<std::time::SystemTime>, Option<std::time::SystemTime>)>,
    raw_len: u64,
    raw_modified: Option<std::time::SystemTime>,
    /// The size and time of the events kept from the screen (session_chat_empryo_events.rs).
    events_stamp: Option<(u64, Option<std::time::SystemTime>)>,
    output_len: usize,
    output_hash: u64,
    in_flight: Option<EmpryoInFlight>,
}

/// Keyed by mirror path, which the follower holds once resolution handed it out.
static MIRROR_STATES: Mutex<Option<HashMap<PathBuf, EmpryoMirrorState>>> = Mutex::new(None);

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// Append when the new output extends the old byte-for-byte; otherwise write a temp file and
/// rename it over the mirror so the follower's inode check reports `content_replaced` instead
/// of reading a half-written file.
fn write_mirror(
    mirror_path: &Path,
    output: &[u8],
    state: &EmpryoMirrorState,
    mirror_exists: bool,
) -> Option<()> {
    fs::create_dir_all(mirror_path.parent()?).ok()?;
    let pure_append = mirror_exists
        && state.output_len > 0
        && output.len() >= state.output_len
        && content_hash(&output[..state.output_len]) == state.output_hash;
    if pure_append {
        if output.len() > state.output_len {
            let mut file = fs::OpenOptions::new().append(true).open(mirror_path).ok()?;
            file.write_all(&output[state.output_len..]).ok()?;
        }
        return Some(());
    }
    let temp_path = mirror_path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&temp_path).ok()?;
    file.write_all(output).ok()?;
    file.flush().ok()?;
    drop(file);
    fs::rename(&temp_path, mirror_path).ok()
}

/// One sync pass. Rebuilds only when the raw log's size or mtime moved, or the mirror is gone
/// (a wiped state dir or a fresh daemon always rebuilds).
///
/// CDXC:SessionChat 2026-10-07 WHY:
/// A rebuild parses the whole raw log, because tab membership, the in-flight turn and where screen events attach all depend on earlier records; parsing only the appended tail would need all of that carried between passes. It is blocking file work, which is why the queue scheduler runs its readiness pass in `spawn_blocking`.
fn sync_mirror(mirror_path: &Path, state: &mut EmpryoMirrorState) -> Option<()> {
    // A tab whose window joined another session's engine is logged there, which shows once the
    // window has named its tab in `tabs.json`; a session started later adds its own tab to the ones
    // left out. Either changes the scan key, and only then is the folder scanned again.
    let scan_key = crate::session_chat_empryo_tabs::empryo_tab_scan_key(&state.own_path);
    let mut rescanned = false;
    if state.scanned_at != Some(scan_key) {
        state.scanned_at = Some(scan_key);
        if let Some((host, foreign_tabs)) =
            crate::session_chat_empryo_tabs::empryo_tab_scan(&state.own_path)
        {
            rescanned = host != state.raw_path || foreign_tabs != state.foreign_tabs;
            state.raw_path = host;
            state.foreign_tabs = foreign_tabs;
        }
    }
    let raw_meta = fs::metadata(&state.raw_path).ok()?;
    let raw_len = raw_meta.len();
    let raw_modified = raw_meta.modified().ok();
    let events_stamp =
        fs::metadata(crate::session_chat_empryo_events::empryo_screen_events_path(mirror_path))
            .ok()
            .map(|meta| (meta.len(), meta.modified().ok()));
    let mirror_exists = mirror_path.is_file();
    if !mirror_exists {
        state.output_len = 0;
        state.output_hash = 0;
    }
    let up_to_date = !rescanned
        && mirror_exists
        && state.output_len > 0
        && state.raw_len == raw_len
        && state.raw_modified == raw_modified
        && state.events_stamp == events_stamp;
    if up_to_date {
        return Some(());
    }
    let mut raw = fs::read(&state.raw_path).ok()?;
    // CDXC:SessionChat 2026-10-07 WHY: a tab followed in another session's engine log has only its turns since it joined there; what came before stays in the session's own log, where a fork's copied conversation sits under the fork's tab (seen live 2026-10-07: a fork beside its open source session showed an empty chat, then only its new turns). The own log is read first, so those turns lead the chat.
    if state.raw_path != state.own_path {
        if let Ok(mut own) = fs::read(&state.own_path) {
            if !own.is_empty() && !own.ends_with(b"\n") {
                own.push(b'\n');
            }
            own.extend_from_slice(&raw);
            raw = own;
        }
    }
    let (output, in_flight) = build_mirror(
        &raw,
        state.foreign_tabs.clone(),
        &read_empryo_screen_events(mirror_path),
    );
    write_mirror(mirror_path, &output, state, mirror_exists)?;
    state.raw_len = raw_len;
    state.raw_modified = raw_modified;
    state.events_stamp = events_stamp;
    state.output_len = output.len();
    state.output_hash = content_hash(&output);
    state.in_flight = in_flight;
    Some(())
}

/// Path-resolution entry: sync, then hand back the mirror as "the transcript". The hook names
/// the raw log (`CDXC:AgentHooks`); a session started from `$HOME` keeps it under
/// `~/.empryo/sessions/`. Like Claude's, the log appears only once the first prompt is sent.
pub fn resolve_empryo_chat_transcript_path(
    session_id: Option<&str>,
    raw_hint: Option<&Path>,
) -> Option<PathBuf> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| raw_hint.and_then(session_id_from_raw_path))?;
    if !is_safe_empryo_session_id(&session_id) {
        return None;
    }
    let raw_path = raw_hint
        .filter(|path| path.is_file())
        .map(Path::to_path_buf)
        .or_else(|| {
            Some(
                home_dir()
                    .join(".empryo")
                    .join("sessions")
                    .join(&session_id)
                    .join("session.jsonl"),
            )
            .filter(|path| path.is_file())
        })?;
    // Two projects can hold the same session id, so the raw log's path names the folder.
    let mirror_path = empryo_mirror_dir()
        .join(format!(
            "{:016x}",
            content_hash(raw_path.as_os_str().as_encoded_bytes())
        ))
        .join(format!("{session_id}.jsonl"));
    let mut states_guard = MIRROR_STATES.lock().ok()?;
    let state = states_guard
        .get_or_insert_with(HashMap::new)
        .entry(mirror_path.clone())
        .or_default();
    if state.own_path != raw_path {
        state.own_path = raw_path.clone();
        state.raw_path = raw_path;
        state.foreign_tabs = HashSet::new();
        state.scanned_at = None;
    }
    sync_mirror(&mirror_path, state)?;
    Some(mirror_path)
}

/// Steady-state entry for the follower's drain tick, which holds only the resolved path.
pub(crate) fn sync_empryo_transcript_mirror_for_path(mirror_path: &Path) {
    let Ok(mut states_guard) = MIRROR_STATES.lock() else {
        return;
    };
    if let Some(state) = states_guard
        .as_mut()
        .and_then(|states| states.get_mut(mirror_path))
    {
        sync_mirror(mirror_path, state);
    }
}

/// The turn the mirror's followed tab is running, as of its last sync.
pub(crate) fn empryo_turn_in_flight(mirror_path: &Path) -> Option<EmpryoInFlight> {
    MIRROR_STATES
        .lock()
        .ok()?
        .as_ref()?
        .get(mirror_path)?
        .in_flight
        .clone()
}
