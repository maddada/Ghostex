//! What a thread's newest turn has done so far, for its Workers row: the phase its newest tool call
//! says it is in ("building"), and how many tool calls it made and files it changed.
//!
//! CDXC:Coordinators 2026-11-18 DECISION:
//! The Workers popover stopped quoting a worker's latest chat words (the user's picks are recorded
//! in gx-chat-core extras/coordinator_threads.rs). Each running row says instead what the worker is
//! doing and how much it has done this turn ("Building · 46 tool calls · 3 files"), and a finished
//! row the same counts for its last turn. gxserver works them out here from the transcript: the
//! whole turn, not only the 24 rows the supervisor reads for its line (coordinator_runtime.rs
//! `read_turn`), back to the prompt that started it.
//!
//! CDXC:Coordinators 2026-11-18 WHY:
//! A worker's turn can run to thousands of rows and its row is rebuilt every two seconds, so the
//! reading is cached per thread. An unchanged transcript costs one `stat`. A grown one is read back
//! only to the rows already counted (rows keep their byte offset across reads), unless a newer
//! prompt is found first, which starts the count again. Only threads in flight or active in the
//! last two hours are read (panel.rs), and the first read of a turn stops after `MAX_ROWS`.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use serde_json::{json, Value};

use super::records::SessionKey;
use crate::session_chat::{SessionChatBlock, SessionChatMessage, SessionChatRole};
use crate::session_chat_queue_runtime::SessionChatTranscriptGate;
use crate::session_chat_tail::{read_session_chat_tail_page, SessionChatTailPage};

/// Rows read on the first page when an existing turn is already tracked: a live turn appends at
/// most a few rows per tick, so reading a small tail first avoids decoding 200 rows (many MB of
/// JSON) on every append; if the previously counted row is older, the loop pages back with
/// `PAGE_ROWS`.
const INCREMENTAL_PAGE_ROWS: usize = 6;
/// Rows read per page while walking back to the turn's prompt.
const PAGE_ROWS: usize = 200;
/// The most rows one read walks back; a longer turn counts what is within them.
const MAX_ROWS: usize = 4_000;
/// Threads kept once this many are cached: the ones read in the last ten minutes.
const KEEP_ENTRIES: usize = 300;

/// A thread's newest turn as its Workers row shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnWork {
    /// The phase its newest tool call names ([`phase_of`]), `None` before its first call.
    pub phase: Option<&'static str>,
    pub tool_calls: usize,
    pub files: usize,
}

impl TurnWork {
    /// The row's `turnWork` field.
    pub fn to_value(&self) -> Value {
        let mut value = json!({"toolCalls": self.tool_calls, "files": self.files});
        if let Some(phase) = self.phase {
            value["phase"] = json!(phase);
        }
        value
    }
}

#[derive(Default)]
struct Entry {
    gate: SessionChatTranscriptGate,
    path: Option<PathBuf>,
    stamp: Option<(u64, Option<SystemTime>)>,
    /// The byte offset of the prompt that started the counted turn, `None` when none was found.
    start: Option<u64>,
    /// The newest row counted, and the calls it held then (a row can grow as its agent writes on).
    newest: Option<(u64, usize)>,
    tool_calls: usize,
    files: BTreeSet<String>,
    phase: Option<&'static str>,
    used: Option<Instant>,
}

fn entries() -> &'static Mutex<HashMap<SessionKey, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<SessionKey, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// BLOCKING (a transcript read when the file changed since the last one). The newest turn of the
/// thread `key`, whose session row is `session`; `None` without a transcript.
pub fn turn_work(key: &SessionKey, session: &Value) -> Option<TurnWork> {
    let mut entries = entries().lock().ok()?;
    if entries.len() > KEEP_ENTRIES {
        entries.retain(|_, entry| {
            entry
                .used
                .is_some_and(|used| used.elapsed().as_secs() < 600)
        });
    }
    let entry = entries.entry(key.clone()).or_default();
    entry.used = Some(Instant::now());
    let (agent, path) = entry.gate.transcript(session)?;
    let metadata = std::fs::metadata(&path).ok()?;
    let stamp = (metadata.len(), metadata.modified().ok());
    if entry.path.as_ref() != Some(&path) || entry.stamp.is_some_and(|(length, _)| length > stamp.0)
    {
        // Another transcript, or this one rewritten shorter: count from scratch.
        *entry = Entry {
            gate: std::mem::take(&mut entry.gate),
            used: entry.used,
            ..Entry::default()
        };
        entry.path = Some(path.clone());
    }
    if entry.stamp != Some(stamp) {
        read_new_rows(entry, agent, &path);
        entry.stamp = Some(stamp);
    }
    Some(TurnWork {
        phase: entry.phase,
        tool_calls: entry.tool_calls,
        files: entry.files.len(),
    })
}

/// Walks back from the newest row to the rows already counted or to the turn's prompt, and counts
/// what it passed.
fn read_new_rows(
    entry: &mut Entry,
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: &std::path::Path,
) {
    // Newest first.
    let mut fresh: Vec<SessionChatMessage> = Vec::new();
    // The row counted last, as it reads now.
    let mut boundary: Option<SessionChatMessage> = None;
    let mut prompt: Option<u64> = None;
    let mut before = None;
    let mut read = 0;
    'pages: while read < MAX_ROWS {
        let page_rows = if before.is_none() && entry.newest.is_some() {
            INCREMENTAL_PAGE_ROWS
        } else {
            PAGE_ROWS
        };
        let Ok(SessionChatTailPage::Page {
            messages,
            has_more,
            before_offset,
            ..
        }) = read_session_chat_tail_page(agent, path, page_rows, before)
        else {
            break;
        };
        read += messages.len();
        for message in messages.into_iter().rev() {
            if is_prompt(&message) {
                prompt = Some(message.byte_offset.unwrap_or(0));
                break 'pages;
            }
            if let (Some((counted, _)), Some(offset)) = (entry.newest, message.byte_offset) {
                if offset <= counted {
                    if offset == counted {
                        boundary = Some(message);
                    }
                    break 'pages;
                }
            }
            fresh.push(message);
        }
        if !has_more || read == 0 {
            break;
        }
        before = Some(before_offset);
    }
    let continues = prompt.is_none() && entry.newest.is_some() && entry.start.is_some();
    if !continues {
        entry.tool_calls = 0;
        entry.files.clear();
        entry.phase = None;
        entry.newest = None;
        entry.start = prompt;
    }
    let collect = |calls: &[(&str, &Value)], files: &mut BTreeSet<String>| {
        for (name, input) in calls {
            crate::session_chat_history::collect_file_paths(name, input, files);
        }
    };
    // `fresh` is newest first: the first of its rows with a call names the phase, and the newest
    // row is where the next read stops.
    let previous = entry.newest;
    let mut phase = None;
    for (index, row) in fresh.iter().enumerate() {
        let calls = calls_of(row);
        entry.tool_calls += calls.len();
        collect(&calls, &mut entry.files);
        if phase.is_none() {
            phase = last_phase(&calls);
        }
        if index == 0 {
            entry.newest = row.byte_offset.map(|offset| (offset, calls.len()));
        }
    }
    // The row counted last may have grown: its new calls count, and name the phase when no newer
    // row made one.
    if let Some(row) = boundary.as_ref().filter(|_| continues) {
        let calls = calls_of(row);
        let counted = previous
            .filter(|(offset, _)| Some(*offset) == row.byte_offset)
            .map_or(0, |(_, calls)| calls);
        if calls.len() > counted {
            entry.tool_calls += calls.len() - counted;
            collect(&calls[counted..], &mut entry.files);
            if phase.is_none() {
                phase = last_phase(&calls[counted..]);
            }
        }
        if fresh.is_empty() {
            entry.newest = row.byte_offset.map(|offset| (offset, calls.len()));
        }
    }
    if phase.is_some() {
        entry.phase = phase;
    }
    if entry.start.is_none() && !continues {
        // No prompt within reach: the count still starts somewhere, so later reads add to it.
        entry.start = Some(0);
    }
}

/// The phase of the newest call that does more than wait on an earlier one.
fn last_phase(calls: &[(&str, &Value)]) -> Option<&'static str> {
    calls
        .iter()
        .rev()
        .find(|(name, _)| !continues_a_call(name))
        .map(|(name, input)| phase_of(name, input))
}

/// The tool calls a row made, in order.
fn calls_of(message: &SessionChatMessage) -> Vec<(&str, &Value)> {
    if message.role != SessionChatRole::Assistant {
        return Vec::new();
    }
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::ToolCall { name, input, .. } => Some((name.as_str(), input)),
            _ => None,
        })
        .collect()
}

/// A prompt that starts a turn: the user's (or a coordinator's, or a background job's) words, not
/// the tool results the agent's calls bring back, and not a prompt still waiting in its queue.
fn is_prompt(message: &SessionChatMessage) -> bool {
    message.role == SessionChatRole::User
        && !message.queued
        && !message
            .blocks
            .iter()
            .any(|block| matches!(block, SessionChatBlock::ToolResult { .. }))
        && message.blocks.iter().any(|block| match block {
            SessionChatBlock::Text { text } => !text.trim().is_empty(),
            _ => false,
        })
}

/// The phase a tool call puts its turn in, as the key the chat core labels ("Building") and the
/// renderers draw an icon for: `reading`, `web`, `editing`, `building`, `testing`, `command`,
/// `agent`, `planning` or `tool`.
pub fn phase_of(name: &str, input: &Value) -> &'static str {
    let name = name.rsplit("__").next().unwrap_or(name);
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase();
    let parsed = input
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let input = parsed.as_ref().unwrap_or(input);
    match name.as_str() {
        "read" | "read_file" | "read_many_files" | "grep" | "grep_search" | "glob" | "ls"
        | "list" | "list_dir" | "list_directory" | "view" | "view_image" | "search"
        | "search_files" | "file_search" | "codebase_search" | "find" | "notebookread" => "reading",
        "webfetch" | "web_fetch" | "websearch" | "web_search" | "fetch" => "web",
        "edit" | "multiedit" | "write" | "write_file" | "edit_file" | "create_file"
        | "str_replace" | "str_replace_editor" | "replace" | "apply_patch" | "notebookedit" => {
            "editing"
        }
        "bash" | "shell" | "exec_command" | "local_shell" | "run_shell_command"
        | "run_terminal_cmd" | "terminal" | "exec" => match command_text(input) {
            Some(command) if name == "exec" && command.contains("apply_patch") => "editing",
            Some(command) => command_phase(&command),
            None => "command",
        },
        "task" | "agent" | "spawn_agent" | "subagent" => "agent",
        "todowrite" | "update_plan" | "todo_write" => "planning",
        _ => "tool",
    }
}

/// The command a shell call ran: Claude's `command`, Codex's `cmd` or its `["bash", "-lc", …]`.
fn command_text(input: &Value) -> Option<String> {
    if let Some(text) = input.as_str() {
        // Codex's `exec` runs JavaScript that calls `tools.exec_command({cmd: "…"})`.
        let commands = exec_commands(text);
        return Some(if commands.is_empty() {
            text.to_string()
        } else {
            commands.join(" ; ")
        });
    }
    for key in ["command", "cmd"] {
        match input.get(key) {
            Some(Value::String(text)) => return Some(text.clone()),
            Some(Value::Array(parts)) => {
                let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
                return Some(match parts.as_slice() {
                    [_, flag, script, ..] if matches!(*flag, "-c" | "-lc") => script.to_string(),
                    _ => parts.join(" "),
                });
            }
            _ => {}
        }
    }
    None
}

/// The commands a Codex `exec` source passes to `tools.exec_command`: each `cmd` string, read as
/// the JSON string it is written as, without evaluating anything.
fn exec_commands(source: &str) -> Vec<String> {
    let mut commands = Vec::new();
    for call in source.split("exec_command(").skip(1) {
        let Some(at) = call.find("cmd") else {
            continue;
        };
        let rest = call[at + 3..].trim_start_matches(['"', '\'', ' ']);
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let mut values =
            serde_json::Deserializer::from_str(rest.trim_start()).into_iter::<String>();
        if let Some(Ok(command)) = values.next() {
            commands.push(command);
        }
    }
    commands
}

/// A call that only waits on or feeds an earlier one (Codex's `wait` and `write_stdin`): it keeps
/// the phase of the call it continues.
pub fn continues_a_call(name: &str) -> bool {
    let name = name.rsplit("__").next().unwrap_or(name);
    matches!(
        name.rsplit('.')
            .next()
            .unwrap_or(name)
            .to_ascii_lowercase()
            .as_str(),
        "wait" | "write_stdin" | "bashoutput" | "read_output"
    )
}

/// Programs that only read: a command made of nothing else is reading code.
const READING_PROGRAMS: &[&str] = &[
    "rg", "grep", "cat", "head", "tail", "ls", "find", "fd", "wc", "less", "tree", "jq", "stat",
    "file", "pwd", "echo", "printf", "sort", "uniq", "cut", "awk", "nl", "diff", "which",
];

/// Wrappers that run the program after them.
const WRAPPERS: &[&str] = &["timeout", "time", "env", "nice", "sudo", "nohup", "command"];

/// A shell command's phase: testing when any part of it runs tests, else building when any part
/// builds, else reading when every part only reads, else running a command.
pub fn command_phase(command: &str) -> &'static str {
    let mut parts_seen = 0;
    let mut all_reading = true;
    let mut building = false;
    for part in command.split(['\n', ';', '|', '&']) {
        let words: Vec<&str> = part
            .split_whitespace()
            .map(|word| word.trim_matches(|c| c == '(' || c == ')' || c == '"' || c == '\''))
            .filter(|word| !word.is_empty())
            .collect();
        // Skip variable assignments and wrappers (with their numeric arguments).
        let mut rest = words.as_slice();
        while let Some((first, tail)) = rest.split_first() {
            let program = first.rsplit('/').next().unwrap_or(first);
            if first.contains('=') && !first.starts_with('-')
                || WRAPPERS.contains(&program)
                || first.starts_with('-') && rest.len() < words.len()
                || first
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == 's' || c == 'm')
                    && rest.len() < words.len()
            {
                rest = tail;
            } else {
                break;
            }
        }
        let Some((first, args)) = rest.split_first() else {
            continue;
        };
        let program = first
            .rsplit('/')
            .next()
            .unwrap_or(first)
            .to_ascii_lowercase();
        // `2>&1` splits into `2>` and `1`; neither is a program.
        if program == "cd"
            || program.starts_with("2>")
            || program.starts_with('>')
            || program.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        parts_seen += 1;
        let arg = |word: &str| args.iter().any(|arg| *arg == word);
        let arg_prefix = |prefix: &str| args.iter().any(|arg| arg.starts_with(prefix));
        let tests = match program.as_str() {
            "pytest" | "vitest" | "jest" | "playwright" | "nextest" => true,
            "cargo" => matches!(args.first(), Some(&"test") | Some(&"nextest")),
            "npm" | "pnpm" | "yarn" | "bun" | "npx" | "deno" => {
                arg("test")
                    || arg_prefix("test:")
                    || arg("vitest")
                    || arg("jest")
                    || arg("playwright")
            }
            "go" | "swift" | "mvn" | "gradle" | "./gradlew" | "gradlew" | "make" | "xcodebuild" => {
                arg("test")
            }
            "python" | "python3" => arg("pytest") || arg("unittest"),
            _ => false,
        };
        if tests {
            return "testing";
        }
        let builds = match program.as_str() {
            "tsc" | "xcodebuild" | "ninja" | "cmake" | "make" | "gradle" | "gradlew"
            | "./gradlew" | "mvn" | "rustc" | "webpack" | "vite" | "esbuild" => true,
            "cargo" => matches!(
                args.first(),
                Some(&"build") | Some(&"check") | Some(&"clippy") | Some(&"fmt")
            ),
            "npm" | "pnpm" | "yarn" | "bun" | "npx" => {
                arg("build")
                    || arg_prefix("build:")
                    || arg("tsc")
                    || arg("typecheck")
                    || arg("lint")
            }
            "go" | "swift" => arg("build") || arg("vet"),
            _ => false,
        };
        building |= builds;
        let reading = READING_PROGRAMS.contains(&program.as_str())
            || program == "sed" && !arg_prefix("-i")
            || program == "git"
                && matches!(
                    args.first(),
                    Some(&"diff")
                        | Some(&"log")
                        | Some(&"show")
                        | Some(&"status")
                        | Some(&"blame")
                        | Some(&"grep")
                        | Some(&"ls-files")
                        | Some(&"rev-parse")
                        | Some(&"branch")
                );
        all_reading &= reading;
    }
    if building {
        "building"
    } else if parts_seen > 0 && all_reading {
        "reading"
    } else {
        "command"
    }
}

#[cfg(test)]
mod tests {
    use super::{command_phase, phase_of};
    use serde_json::json;

    /// The phase a tool call names, for each agent's tool names and shell commands.
    #[test]
    fn tool_calls_name_their_phase() {
        assert_eq!(phase_of("Read", &json!({"file_path": "a.rs"})), "reading");
        assert_eq!(phase_of("Grep", &json!({})), "reading");
        assert_eq!(phase_of("Edit", &json!({})), "editing");
        assert_eq!(
            phase_of("apply_patch", &json!("*** Begin Patch")),
            "editing"
        );
        assert_eq!(phase_of("WebSearch", &json!({})), "web");
        assert_eq!(phase_of("Task", &json!({})), "agent");
        assert_eq!(phase_of("TodoWrite", &json!({})), "planning");
        assert_eq!(phase_of("mcp__spaceo__spaceo_click", &json!({})), "tool");
        let bash = |command: &str| phase_of("Bash", &json!({"command": command}));
        assert_eq!(bash("cargo build -p ghostex 2>&1 | tail -20"), "building");
        assert_eq!(bash("cd server && cargo test turn_work"), "testing");
        assert_eq!(bash("timeout 300 cargo check"), "building");
        assert_eq!(bash("rg -n foo src | head"), "reading");
        assert_eq!(bash("git diff --stat"), "reading");
        assert_eq!(bash("sed -n 1,40p a.rs"), "reading");
        assert_eq!(bash("sed -i '' s/a/b/ a.rs"), "command");
        assert_eq!(bash("git commit -m x"), "command");
        assert_eq!(bash("CI=1 pnpm run test:unit"), "testing");
        assert_eq!(bash("npm run build"), "building");
        assert_eq!(
            phase_of("shell", &json!({"command": ["bash", "-lc", "cargo test"]})),
            "testing"
        );
        assert_eq!(
            phase_of("exec_command", &json!({"cmd": "ls -la"})),
            "reading"
        );
        assert_eq!(command_phase("ghostex visual check page.html"), "command");
        assert_eq!(
            phase_of(
                "exec",
                &json!(
                    r#"const r = await tools.exec_command({cmd:"rustc main.rs -o /tmp/m && /tmp/m","workdir":"/x"});"#
                )
            ),
            "building"
        );
        assert_eq!(
            phase_of(
                "exec",
                &json!(r#"await tools.exec_command({"cmd": "rg -n foo ."})"#)
            ),
            "reading"
        );
        assert!(super::continues_a_call("wait"));
    }
}
