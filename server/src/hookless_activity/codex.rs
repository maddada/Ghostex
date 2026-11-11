//! Codex's rollout JSONL, tailed for the conversation Ghostex has bound to the session and turned
//! into the hook events Codex's hooks send.
//!
//! CDXC:AgentHooks 2026-11-01 WHY:
//! Codex (0.165) appends each turn's `event_msg` `task_started` and `task_complete`, an interrupt's `turn_aborted`, every tool call (`response_item` `function_call`, `custom_tool_call`, `local_shell_call`, `web_search_call`) and its output, and `compacted`. It writes nothing while an approval waits on the user (seen live on 2026-11-01: the rollout's last line stayed the asking `exec` call until the answer), so attention is invisible here; code mode's `wait` tool fires no hook and is skipped. Ghostex's Codex sessions run the TUI without Codex's shared app-server (the `ghostex-codex-wrapper`), so the daemon's thread status cannot be matched to them.
//!
//! CDXC:AgentHooks 2026-11-11 DECISION:
//! User, on no longer installing hooks for Claude, Codex and OpenCode and tracking them from what each agent exposes itself: picked "Claude, then OpenCode and Codex", knowing Codex may not be possible. Codex's approval wait does have a hook-free source, just not in any file: since 0.163 the TUI reports its state on every draw as OSC 7501 (`state=blocked:app=codex` while an approval, question or sign-in waits, with `:kind=permission|question|auth` and a `done` after a turn since 0.164; 0.163 ends a turn with `idle`, which program_status promotes to finished), written unconditionally to any terminal that is not an old VTE (codex-rs tui/src/terminal_program_status.rs). zmx records it per session and gxserver's Combined program status (program_status/) already turns `blocked` into attention for every session, hook or not, so this observer does not look for approvals. The rollout stays the source of turns and tools: Codex keeps approval and elicitation events out of it on purpose (rollout/src/policy.rs lists `ExecApprovalRequest`, `RequestPermissions`, `ElicitationRequest` as transient), its OTEL `codex.tool_decision` only fires after the answer, and its `notify` program only on a finished turn. Where the hook is missing nothing reports the conversation, so the rollout is the one Codex's process holds open (the zmx process scan, `find_open_rollouts`), which also follows a resume or `/new` inside the TUI; the hook's id and path are the fallback.
//!
//! CDXC:AgentHooks 2026-11-11 WHY:
//! Codex's sub-agents need nothing from this observer or the hook. Codex fires SubagentStart and SubagentStop inside the child, naming the child's own rollout, so Ghostex's hook ingest has always dropped them (`is_codex_subagent_transcript`, reason `subagent-hook`), and they move no activity anyway (roster-only, event_mapping.rs). The subagent list and the fleet that keeps a session working read Codex's own state (`thread_spawn_edges` in state_5.sqlite, each child rollout's newest turn, the children the live process holds open, session_chat_codex_fleet.rs), and the run boundary is the live process's start (session_chat_async_questions.rs `record_process_run_start`), so they already run without the hook. Seen live on 0.167: a hooked and a hook-free session listed their spawned sub-agent at the same moment and dropped it when it finished.
//! SEE-ALSO: server/src/program_status/ingest.rs (`apply_program_status`), server/src/zmx/provider.rs (`codex_process_session_identity_from_paths`), server/src/session_chat_codex_blocking.rs (the screen reader for the same prompts).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

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
    /// The root rollout each session's Codex process holds open, with its conversation id, read
    /// off the process (`find_open_rollouts`); preferred over the ids the hook reported.
    open_rollouts: HashMap<(String, String), (String, PathBuf)>,
    open_rollouts_read_at: Option<Instant>,
    /// Each open file's conversation id if it is a root rollout (`None`: not one, such as a
    /// subagent's): a rollout's first line never changes, so it is read once.
    root_rollouts: HashMap<PathBuf, Option<String>>,
}

/// How often the open rollouts are read again: the process scan is cached for about as long.
const OPEN_ROLLOUTS_REFRESH: Duration = Duration::from_secs(2);

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
    /// Reads the rollout each Codex session's process holds open (the zmx process scan, cached),
    /// so a session whose hook never reported its conversation is still followed, and an in-TUI
    /// resume or `/new` moves the tail to the new rollout.
    pub(super) fn find_open_rollouts(&mut self, home_dir: &std::path::Path, sessions: &[Value]) {
        if self
            .open_rollouts_read_at
            .is_some_and(|at| at.elapsed() < OPEN_ROLLOUTS_REFRESH)
        {
            return;
        }
        self.open_rollouts_read_at = Some(Instant::now());
        let mut names = Vec::new();
        let mut keys = HashMap::new();
        for session in sessions {
            if read_session_text(session, "agentId").as_deref() != Some("codex") {
                continue;
            }
            let (Some(project_id), Some(session_id), Some(name)) = (
                read_session_text(session, "projectId"),
                read_session_text(session, "sessionId"),
                read_session_text(session, "zmxName"),
            ) else {
                continue;
            };
            keys.insert(name.clone(), (project_id, session_id));
            names.push(name);
        }
        let Ok(identities) =
            crate::zmx::read_cached_zmx_session_process_identities(&names, home_dir)
        else {
            // A failed scan keeps what the last one found.
            return;
        };
        self.open_rollouts = open_rollouts_from(&keys, &identities, &mut self.root_rollouts);
        if self.root_rollouts.len() > 1024 {
            self.root_rollouts.clear();
        }
    }

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
            let key = (project_id.clone(), session_id.clone());
            let open = self
                .open_rollouts
                .get(&key)
                .filter(|(_, path)| path.is_file())
                .cloned();
            let (thread, path) = match open {
                Some((thread, path)) => (Some(thread), Some(path)),
                None => {
                    let thread = read_runtime_text(session, "agentSessionId");
                    let path = read_runtime_text(session, "agentSessionPath")
                        .map(PathBuf::from)
                        .filter(|path| path.is_file())
                        .or_else(|| {
                            let thread = thread.as_deref()?;
                            self.lookups
                                .entry(thread.to_string())
                                .or_insert_with(|| {
                                    crate::agent_transcripts::find_codex_transcript(thread)
                                })
                                .clone()
                        });
                    (thread, path)
                }
            };
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

/// The open root rollout of each session the process scan found Codex in, by session.
///
/// CDXC:AgentHooks 2026-11-11 WHY:
/// After `/new` (or a resume) inside the TUI, Codex 0.167 keeps the previous conversation's rollout open beside the new one (seen live with lsof), so the process scan, which needs exactly one open root rollout, names none, and without the hook the row's id and path went on naming the old conversation; the observer tailed a file nothing wrote to and missed every later turn. Of the root rollouts the process holds open, the one written last is the conversation the TUI is on: a new or resumed one is written when it opens, the one left behind no more.
fn open_rollouts_from(
    keys: &HashMap<String, (String, String)>,
    identities: &HashMap<String, crate::zmx::ZmxProcessIdentity>,
    root_rollouts: &mut HashMap<PathBuf, Option<String>>,
) -> HashMap<(String, String), (String, PathBuf)> {
    let mut found = HashMap::new();
    for (name, identity) in identities {
        if identity.agent_id.as_deref() != Some("codex") {
            continue;
        }
        let Some(key) = keys.get(name) else {
            continue;
        };
        let newest = identity
            .open_file_paths
            .iter()
            .flat_map(|paths| paths.iter())
            .filter_map(|path| {
                let id = match root_rollouts.get(path) {
                    Some(known) => known.clone()?,
                    None => {
                        // Not cached while its first line is still unwritten.
                        let known = root_rollout_id(path)?;
                        root_rollouts.insert(path.clone(), known.clone());
                        known?
                    }
                };
                let written = std::fs::metadata(path)
                    .and_then(|meta| meta.modified())
                    .ok()?;
                Some((written, id, path.clone()))
            })
            .max_by_key(|(written, _, _)| *written)
            .map(|(_, id, path)| (id, path));
        // No open file list (Windows' helper failed, or too many files): the scan's own answer.
        let chosen = newest.or_else(|| {
            Some((
                identity.agent_session_id.clone()?,
                PathBuf::from(identity.agent_session_path.as_deref()?),
            ))
        });
        if let Some(chosen) = chosen {
            found.insert(key.clone(), chosen);
        }
    }
    found
}

/// Whether an open file is a Codex root rollout, with its conversation id: under `sessions/`,
/// its first line a `session_meta` naming the id in its file name, and not a subagent's.
/// `None` while that first line cannot be read yet.
fn root_rollout_id(path: &std::path::Path) -> Option<Option<String>> {
    let Some(id) = crate::zmx::codex_session_id_from_transcript_path(path) else {
        return Some(None);
    };
    let meta = crate::session_chat_successor::read_codex_session_meta(path)?;
    Some((!meta.is_subagent && meta.session_id == id).then_some(id))
}

fn read_from(path: &PathBuf, offset: u64, length: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let start = offset.max(length.saturating_sub(MAX_READ));
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(length - start).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "01a1cace-53ec-7371-9636-22aead416642";
    const NEW: &str = "01a1cacf-0d60-7e40-9490-0655946213eb";
    const CHILD: &str = "01a1cad0-1111-7222-8333-444455556666";

    fn rollout(dir: &std::path::Path, id: &str, meta: Value, lines: &str) -> PathBuf {
        let path = dir.join(format!("rollout-2026-11-11T17-31-21-{id}.jsonl"));
        let head = json!({ "type": "session_meta", "payload": meta });
        std::fs::write(&path, format!("{head}\n{lines}")).expect("rollout");
        path
    }

    fn codex(paths: Vec<PathBuf>) -> crate::zmx::ZmxProcessIdentity {
        crate::zmx::ZmxProcessIdentity {
            agent_id: Some("codex".to_string()),
            open_file_paths: Some(paths.into()),
            ..Default::default()
        }
    }

    fn keys() -> HashMap<String, (String, String)> {
        HashMap::from([("S1-P1-G1".to_string(), ("P1".to_string(), "G1".to_string()))])
    }

    /// After `/new` Codex holds the old and the new rollout open (and a subagent's, if one ran):
    /// the newest root rollout is the conversation, and it wins over the stale bound path.
    #[test]
    fn the_newest_root_rollout_codex_holds_open_is_followed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("sessions").join("2026").join("11").join("11");
        std::fs::create_dir_all(&dir).expect("dir");
        let old = rollout(&dir, OLD, json!({ "id": OLD }), "");
        let started = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n";
        std::thread::sleep(Duration::from_millis(20));
        let new = rollout(&dir, NEW, json!({ "id": NEW }), started);
        std::thread::sleep(Duration::from_millis(20));
        let child = rollout(
            &dir,
            CHILD,
            json!({ "id": CHILD, "source": { "subagent": { "thread_spawn": { "parent_thread_id": NEW } } } }),
            started,
        );
        let identities = HashMap::from([(
            "S1-P1-G1".to_string(),
            codex(vec![old.clone(), child, temp.path().join("other.log"), new.clone()]),
        )]);
        let mut cache = HashMap::new();
        let mut observer = CodexObserver {
            open_rollouts: open_rollouts_from(&keys(), &identities, &mut cache),
            ..Default::default()
        };
        assert_eq!(
            observer.open_rollouts.get(&("P1".to_string(), "G1".to_string())),
            Some(&(NEW.to_string(), new))
        );
        let session = json!({
            "agentId": "codex",
            "projectId": "P1",
            "sessionId": "G1",
            "runtimeSettings": { "agentSessionId": OLD, "agentSessionPath": old },
        });
        let events = observer.poll(&[session]);
        let names: Vec<_> = events.iter().map(|event| event.event_name.as_str()).collect();
        assert_eq!(names, ["SessionStart", "UserPromptSubmit"]);
        assert!(events
            .iter()
            .all(|event| event.agent_session_id.as_deref() == Some(NEW)));
    }

    /// A rollout whose first line is not written yet is looked at again on the next scan.
    #[test]
    fn a_rollout_without_its_first_line_is_read_again() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("sessions");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(format!("rollout-2026-11-11T17-31-21-{NEW}.jsonl"));
        std::fs::write(&path, "").expect("empty");
        let identities = HashMap::from([("S1-P1-G1".to_string(), codex(vec![path.clone()]))]);
        let mut cache = HashMap::new();
        assert!(open_rollouts_from(&keys(), &identities, &mut cache).is_empty());
        std::fs::write(&path, format!("{}\n", json!({ "type": "session_meta", "payload": { "id": NEW } })))
            .expect("meta");
        assert_eq!(
            open_rollouts_from(&keys(), &identities, &mut cache).len(),
            1
        );
    }

    /// Another agent's process, or a Codex with no open rollout and no answer of its own,
    /// names nothing.
    #[test]
    fn only_a_codex_process_with_a_root_rollout_counts() {
        let mut keys = keys();
        keys.insert("S1-P1-G2".to_string(), ("P1".to_string(), "G2".to_string()));
        let identities = HashMap::from([
            ("S1-P1-G1".to_string(), codex(Vec::new())),
            (
                "S1-P1-G2".to_string(),
                crate::zmx::ZmxProcessIdentity {
                    agent_id: Some("claude".to_string()),
                    agent_session_id: Some("c".to_string()),
                    agent_session_path: Some("/x.jsonl".to_string()),
                    ..Default::default()
                },
            ),
        ]);
        assert!(open_rollouts_from(&keys, &identities, &mut HashMap::new()).is_empty());
    }
}
