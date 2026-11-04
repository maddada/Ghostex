//! Claude's own finished `EnterPlanMode` calls, read off the transcripts of busy sessions for the
//! plan-mode guard (session_chat_plan_mode_guard.rs `observe_finished_call`).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::claude::{transcript_path, ClaudeObserver};
use crate::server::AppState;

const TOOL: &str = "EnterPlanMode";
/// The most read from one transcript in one pass; more than that is skipped, not parsed.
const MAX_READ: u64 = 8 * 1024 * 1024;

struct Tail {
    conversation: Option<String>,
    path: Option<PathBuf>,
    /// Where the next read starts; only what Claude writes after the pane was first seen counts.
    offset: u64,
    /// `EnterPlanMode` calls whose result has not been read yet.
    calls: HashSet<String>,
    /// The pane was busy on the last pass, so it gets one more read once it is idle.
    busy: bool,
}

#[derive(Default)]
pub(super) struct PlanModeWatch {
    tails: HashMap<(String, String), Tail>,
}

impl PlanModeWatch {
    pub(super) fn poll(&mut self, state: &AppState, observer: &ClaudeObserver) {
        let mut live = HashSet::new();
        for (pane, pane_state) in observer.pane_states() {
            live.insert(pane.clone());
            let resolve = || {
                pane_state.conversation.as_deref().and_then(|id| {
                    transcript_path(&pane_state.config_dir, pane_state.cwd.as_deref(), id)
                })
            };
            let tail = match self.tails.get_mut(pane) {
                Some(tail) if tail.conversation == pane_state.conversation => tail,
                _ => {
                    // A conversation already on disk is read from its end; one not written yet
                    // from its start, when it appears.
                    let path = resolve();
                    let offset = path
                        .as_deref()
                        .and_then(|path| std::fs::metadata(path).ok())
                        .map_or(0, |meta| meta.len());
                    self.tails.insert(
                        pane.clone(),
                        Tail {
                            conversation: pane_state.conversation.clone(),
                            path,
                            offset,
                            calls: HashSet::new(),
                            busy: false,
                        },
                    );
                    self.tails.get_mut(pane).expect("just inserted")
                }
            };
            let busy = pane_state.status != "idle";
            if !busy && !tail.busy {
                continue;
            }
            tail.busy = busy;
            if tail.path.is_none() {
                tail.path = resolve();
            }
            let Some(path) = tail.path.clone() else {
                continue;
            };
            for tool_use_id in read_finished_calls(&path, &mut tail.offset, &mut tail.calls) {
                crate::session_chat_plan_mode_guard::observe_finished_call(
                    state,
                    pane.0.clone(),
                    pane.1.clone(),
                    &tool_use_id,
                    Some(path.to_string_lossy().into_owned()),
                );
            }
        }
        self.tails.retain(|pane, _| live.contains(pane));
    }
}

/// The `EnterPlanMode` calls whose successful result Claude wrote since `offset`, moving it to the
/// end of the last complete line.
fn read_finished_calls(path: &Path, offset: &mut u64, calls: &mut HashSet<String>) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or_default();
    if length < *offset || length - *offset > MAX_READ {
        *offset = length;
        return Vec::new();
    }
    if length == *offset || file.seek(SeekFrom::Start(*offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.take(length - *offset).read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return Vec::new();
    };
    *offset += end as u64 + 1;
    let mut finished = Vec::new();
    for line in String::from_utf8_lossy(&bytes[..end]).lines() {
        read_line(line, calls, &mut finished);
    }
    finished
}

fn read_line(line: &str, calls: &mut HashSet<String>, finished: &mut Vec<String>) {
    let names_tool = line.contains(TOOL);
    let answers_call = !calls.is_empty() && line.contains("tool_result");
    if !names_tool && !answers_call {
        return;
    }
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if record["isSidechain"] == true {
        return;
    }
    let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for block in blocks {
        match block["type"].as_str() {
            Some("tool_use") if record["type"] == "assistant" && block["name"] == TOOL => {
                if let Some(id) = block["id"].as_str() {
                    calls.insert(id.to_string());
                }
            }
            Some("tool_result") if record["type"] == "user" => {
                let Some(id) = block["tool_use_id"].as_str() else {
                    continue;
                };
                // Only a call that succeeded switched the mode, as with the PostToolUse hook.
                if calls.remove(id) && block["is_error"] != true {
                    finished.push(id.to_string());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_finished_enter_plan_mode_call_is_read_once_from_new_lines_only() {
        let dir = std::env::temp_dir().join(format!("gx-plan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let call = |id: &str| {
            format!("{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"EnterPlanMode\",\"input\":{{}}}}]}}}}\n")
        };
        let result = |id: &str, error: bool| {
            format!("{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"{id}\",\"is_error\":{error}}}]}}}}\n")
        };
        // An old call, written before the pane was first seen.
        std::fs::write(&path, format!("{}{}", call("old"), result("old", false))).unwrap();
        let mut offset = std::fs::metadata(&path).unwrap().len();
        let mut calls = HashSet::new();
        let append = |text: &str| {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        };
        append(&call("new"));
        assert!(read_finished_calls(&path, &mut offset, &mut calls).is_empty());
        // The result, with half of the next line still being written.
        append(&format!("{}{{\"type\":", result("new", false)));
        assert_eq!(
            read_finished_calls(&path, &mut offset, &mut calls),
            vec!["new".to_string()]
        );
        append("\"user\"}\n");
        assert!(read_finished_calls(&path, &mut offset, &mut calls).is_empty());
        // A refused call changed nothing.
        append(&format!("{}{}", call("refused"), result("refused", true)));
        assert!(read_finished_calls(&path, &mut offset, &mut calls).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
