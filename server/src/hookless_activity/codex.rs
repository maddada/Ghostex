//! Codex's rollout JSONL, tailed for the conversation Ghostex has bound to the session and turned
//! into the hook events Codex's hooks send.
//!
//! CDXC:AgentHooks 2026-11-01 WHY:
//! Codex (0.165) appends each turn's `event_msg` `task_started` and `task_complete`, an interrupt's `turn_aborted`, every tool call (`response_item` `function_call`, `custom_tool_call`, `local_shell_call`, `web_search_call`) and its output, and `compacted`. It writes nothing while an approval waits on the user (seen live on 2026-11-01: the rollout's last line stayed the asking `exec` call until the answer), so attention is invisible here; code mode's `wait` tool fires no hook and is skipped. Ghostex's Codex sessions run the TUI without Codex's shared app-server (the `ghostex-codex-wrapper`), so the daemon's thread status cannot be matched to them.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use super::comparison::ActivityEvent;
use crate::server::{read_runtime_text, read_session_text};

pub(crate) const OBSERVABLE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "Interrupt",
];

/// A rollout created this recently is read from its start (its first prompt is news); an older
/// one only from its current end.
const FRESH_ROLLOUT: Duration = Duration::from_secs(20);
const MAX_READ: u64 = 4 * 1024 * 1024;
const BASELINE_READ: u64 = 256 * 1024;

struct Tail {
    path: PathBuf,
    offset: u64,
    partial: String,
    quiet_calls: HashSet<String>,
}

#[derive(Default)]
pub(super) struct CodexObserver {
    tails: HashMap<(String, String), Tail>,
    lookups: HashMap<String, Option<PathBuf>>,
}

/// The hook event a rollout line stands for. `quiet_calls` holds the call ids of tools Codex
/// fires no hook for, so their outputs are skipped too.
fn line_event(
    line: &Value,
    quiet_calls: &mut HashSet<String>,
) -> Option<(&'static str, Option<String>)> {
    let payload = &line["payload"];
    let kind = payload["type"].as_str();
    match line["type"].as_str()? {
        "session_meta" => Some(("SessionStart", None)),
        "compacted" => Some(("Compacted", None)),
        "event_msg" => match kind? {
            "task_started" => Some(("UserPromptSubmit", None)),
            "task_complete" => Some(("Stop", None)),
            "turn_aborted" => Some(("Interrupt", payload["reason"].as_str().map(str::to_string))),
            _ => None,
        },
        "response_item" => match kind? {
            "function_call" | "custom_tool_call" | "local_shell_call" | "web_search_call"
            | "tool_search_call" => {
                let name = payload["name"].as_str();
                // Code mode's `wait` polls a running `exec` cell; Codex fires no hook for it.
                if name == Some("wait") {
                    if let Some(call) = payload["call_id"].as_str() {
                        quiet_calls.insert(call.to_string());
                    }
                    return None;
                }
                Some(("PreToolUse", name.map(str::to_string)))
            }
            "function_call_output" | "custom_tool_call_output" | "tool_search_output" => {
                if payload["call_id"]
                    .as_str()
                    .is_some_and(|call| quiet_calls.remove(call))
                {
                    return None;
                }
                Some(("PostToolUse", None))
            }
            _ => None,
        },
        _ => None,
    }
}

impl CodexObserver {
    pub(super) fn poll(&mut self, sessions: &[Value]) -> Vec<ActivityEvent> {
        let mut events = Vec::new();
        let mut active = Vec::new();
        for session in sessions {
            if read_session_text(session, "agentId").as_deref() != Some("codex") {
                continue;
            }
            let (Some(project_id), Some(session_id)) = (
                read_session_text(session, "projectId"),
                read_session_text(session, "sessionId"),
            ) else {
                continue;
            };
            let thread = read_runtime_text(session, "agentSessionId");
            let path = read_runtime_text(session, "agentSessionPath")
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .or_else(|| {
                    let thread = thread.as_deref()?;
                    self.lookups
                        .entry(thread.to_string())
                        .or_insert_with(|| crate::agent_transcripts::find_codex_transcript(thread))
                        .clone()
                });
            let key = (project_id.clone(), session_id.clone());
            active.push(key.clone());
            let Some(path) = path else {
                continue;
            };
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let length = meta.len();
            let switched = self.tails.get(&key).is_some_and(|tail| tail.path != path);
            if !self.tails.contains_key(&key) || switched {
                let fresh = meta
                    .created()
                    .ok()
                    .and_then(|created| SystemTime::now().duration_since(created).ok())
                    .is_some_and(|age| age < FRESH_ROLLOUT);
                if switched && !fresh {
                    // A resumed conversation: its history is not news, the switch is.
                    events.push(ActivityEvent::new(
                        &project_id,
                        &session_id,
                        "codex",
                        "SessionStart",
                        thread.clone(),
                    ));
                }
                let mut quiet_calls = HashSet::new();
                if !fresh {
                    // Where the conversation stands now: its newest lines, as a baseline.
                    let start = length.saturating_sub(BASELINE_READ);
                    for line in read_from(&path, start, length).unwrap_or_default().lines() {
                        let Some((name, _)) = serde_json::from_str::<Value>(line)
                            .ok()
                            .and_then(|value| line_event(&value, &mut quiet_calls))
                        else {
                            continue;
                        };
                        events.push(
                            ActivityEvent::new(
                                &project_id,
                                &session_id,
                                "codex",
                                name,
                                thread.clone(),
                            )
                            .baseline(),
                        );
                    }
                }
                self.tails.insert(
                    key.clone(),
                    Tail {
                        path: path.clone(),
                        offset: if fresh { 0 } else { length },
                        partial: String::new(),
                        quiet_calls,
                    },
                );
            }
            let Some(tail) = self.tails.get_mut(&key) else {
                continue;
            };
            if length < tail.offset {
                tail.offset = 0;
                tail.partial.clear();
            }
            if length == tail.offset {
                continue;
            }
            let Some(chunk) = read_from(&path, tail.offset, length) else {
                continue;
            };
            tail.offset = length;
            tail.partial.push_str(&chunk);
            let complete = match tail.partial.rfind('\n') {
                Some(end) => tail.partial.drain(..=end).collect::<String>(),
                None => continue,
            };
            for line in complete.lines() {
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                let Some((name, detail)) = line_event(&value, &mut tail.quiet_calls) else {
                    continue;
                };
                let mut event =
                    ActivityEvent::new(&project_id, &session_id, "codex", name, thread.clone());
                if let Some(detail) = detail {
                    event = event.with(
                        if name == "PreToolUse" {
                            "toolName"
                        } else {
                            "reason"
                        },
                        json!(detail),
                    );
                }
                events.push(event);
            }
        }
        self.tails.retain(|key, _| active.contains(key));
        events
    }
}

fn read_from(path: &PathBuf, offset: u64, length: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let start = offset.max(length.saturating_sub(MAX_READ));
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(length - start).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}
