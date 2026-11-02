//! What a Claude session's transcript says it loaded, and the background commands it ran.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! Claude Code 2.1.x writes its context inputs into the transcript as `attachment` records: `deferred_tools_delta` (every tool, MCP ones named `mcp__<server>__<tool>`, plus `pendingMcpServers` / `failedMcpServers`), `mcp_instructions_delta` (servers by their real names), `instructions` and `nested_memory` (the CLAUDE.md / AGENTS.md files), `skill_listing` and `invoked_skills`. That is the live process's own record, so it is what the panel lists; nothing has to be started to read it. The deltas run on across a resume (a resumed process announces only what changed), and a compaction re-announces everything, so the sets reset at `compact_boundary` and nowhere else. Lines are tested for their markers before being parsed, so a long transcript's tail costs a substring search per line.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::session_chat::{read_transcript_file_version, TranscriptFileVersion};
use crate::session_chat_fleet_transcript::timestamp;

/// Transcripts kept scanned in memory; the oldest goes first.
const CACHED_TRANSCRIPTS: usize = 24;
/// Finished background commands kept for the panel's fold.
const FINISHED_KEPT: usize = 20;
/// A record longer than this is skipped unparsed (Claude's own snapshots of the prompt can be big,
/// and none of them is a record this reader wants).
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
/// Longest background command kept for the hover.
const MAX_COMMAND_CHARS: usize = 2_000;
const MAX_TITLE_CHARS: usize = 160;

/// One memory file Claude loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MemoryFile {
    pub path: String,
    /// `User`, `Project`, `Local`, … as Claude names the file's scope.
    pub kind: String,
}

/// One background command the session started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BackgroundRun {
    pub id: String,
    /// `shell` or `monitor`.
    pub kind: &'static str,
    pub title: String,
    pub command: Option<String>,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    /// `completed`, `failed`, `killed`, `stopped`, … from Claude's notification.
    pub status: Option<String>,
    /// Claude's one-line summary of how it ended.
    pub summary: Option<String>,
}

#[derive(Clone, Debug)]
struct Launch {
    kind: &'static str,
    title: String,
    command: Option<String>,
    at: i64,
}

/// Everything the panel reads from the transcript.
#[derive(Clone, Debug, Default)]
pub(super) struct TranscriptFacts {
    /// The MCP tools Claude announced and did not withdraw, by the server key in their names.
    pub mcp_tools: BTreeMap<String, BTreeSet<String>>,
    /// Servers' real names (`claude.ai Claude Docs`), by their key (`claude_ai_Claude_Docs`).
    pub mcp_names: BTreeMap<String, String>,
    /// Servers still connecting, and servers that failed, by key.
    pub mcp_pending: BTreeSet<String>,
    pub mcp_failed: BTreeSet<String>,
    /// The last `instructions` set, then the nested files loaded after it.
    pub memory_files: Vec<MemoryFile>,
    /// Every skill Claude listed to the model, with its description.
    pub skills: BTreeMap<String, String>,
    /// Skills the session used, in the order it first used them.
    pub invoked_skills: Vec<String>,
    /// The conversation has a message, so Claude can resume it.
    ///
    /// CDXC:SessionDetails 2026-10-25 WHY:
    /// Claude Code writes a conversation only with its first message (until then the transcript is missing or holds just start-up records such as hook results), and `claude --resume <id>` before that ends with "No conversation found with session ID: <id>". The panel showed that as "Couldn't count: Claude Code stopped before it counted the context" on every session opened before its first message, and kept it for the failed-retry wait after the message arrived (seen on a new session in the home folder, 2026-10-25). The breakdown is not asked for until this is true.
    pub conversation_started: bool,
    /// Background commands by Claude's task id, and the order they started in.
    background: HashMap<String, BackgroundRun>,
    background_order: Vec<String>,
    pending_launches: HashMap<String, Launch>,
    last_at: i64,
    /// How far the process starts Ghostex recorded for this conversation are applied.
    starts: crate::hookless_activity::ProcessStartCursor,
}

impl TranscriptFacts {
    /// Finished background commands, newest first.
    pub(super) fn finished_background(&self) -> Vec<BackgroundRun> {
        let mut finished: Vec<BackgroundRun> = self
            .background
            .values()
            .filter(|run| run.ended_at_ms.is_some())
            .cloned()
            .collect();
        finished.sort_by(|a, b| {
            b.ended_at_ms
                .cmp(&a.ended_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        finished.truncate(FINISHED_KEPT);
        finished
    }
}

/// The key Claude puts in an MCP tool's name for server `name`: every character other than an
/// ASCII letter, digit, `_` or `-` becomes `_`.
pub(super) fn mcp_server_key(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn mcp_tool_parts(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    (!server.is_empty() && !tool.is_empty()).then_some((server, tool))
}

fn strings(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_str)
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\u{2026}", text[..end].trim_end()),
        None => text.to_string(),
    }
}

fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split_once(&format!("<{name}>"))?
        .1
        .split_once(&format!("</{name}>"))
        .map(|(value, _)| value.trim())
}

fn terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "killed" | "stopped" | "cancelled" | "canceled"
    )
}

/// The skill names of one `skill_listing`: its `names`, else its `- name: description` lines.
fn listed_skills(attachment: &Value) -> Vec<(String, String)> {
    let content = attachment
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut described: HashMap<&str, &str> = HashMap::new();
    for line in content.lines() {
        if let Some((name, description)) = line
            .strip_prefix("- ")
            .and_then(|line| line.split_once(": "))
        {
            described.insert(name.trim(), description.trim());
        }
    }
    let names: Vec<&str> = strings(attachment.get("names")).collect();
    if names.is_empty() {
        return described
            .into_iter()
            .map(|(name, description)| (name.to_string(), description.to_string()))
            .collect();
    }
    names
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                described.get(name).copied().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

impl TranscriptFacts {
    fn reset_context(&mut self) {
        self.mcp_tools.clear();
        self.mcp_names.clear();
        self.mcp_pending.clear();
        self.mcp_failed.clear();
        self.memory_files.clear();
        self.skills.clear();
    }

    fn note_server_names(&mut self, names: impl Iterator<Item = String>) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        for name in names {
            let key = mcp_server_key(&name);
            self.mcp_names.insert(key.clone(), name);
            keys.insert(key);
        }
        keys
    }

    fn observe_attachment(&mut self, attachment: &Value) {
        match attachment.get("type").and_then(Value::as_str) {
            Some("deferred_tools_delta") => {
                for name in strings(attachment.get("addedNames"))
                    .chain(strings(attachment.get("readdedNames")))
                {
                    if let Some((server, tool)) = mcp_tool_parts(name) {
                        self.mcp_tools
                            .entry(server.to_string())
                            .or_default()
                            .insert(tool.to_string());
                    }
                }
                for name in strings(attachment.get("removedNames")) {
                    if let Some((server, tool)) = mcp_tool_parts(name) {
                        if let Some(tools) = self.mcp_tools.get_mut(server) {
                            tools.remove(tool);
                            if tools.is_empty() {
                                self.mcp_tools.remove(server);
                            }
                        }
                    }
                }
                // Each delta restates who is still connecting and who failed.
                if attachment.get("pendingMcpServers").is_some() {
                    self.mcp_pending = self.note_server_names(
                        strings(attachment.get("pendingMcpServers")).map(str::to_string),
                    );
                }
                if attachment.get("failedMcpServers").is_some() {
                    self.mcp_failed = self.note_server_names(
                        strings(attachment.get("failedMcpServers")).map(str::to_string),
                    );
                }
                for key in self.mcp_tools.keys() {
                    self.mcp_pending.remove(key);
                }
            }
            Some("mcp_instructions_delta") => {
                self.note_server_names(strings(attachment.get("addedNames")).map(str::to_string));
            }
            Some("instructions") => {
                self.memory_files = attachment
                    .get("files")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|file| {
                        Some(MemoryFile {
                            path: file.get("path")?.as_str()?.to_string(),
                            kind: file
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("Project")
                                .to_string(),
                        })
                    })
                    .collect();
            }
            Some("nested_memory") => {
                let Some(path) = attachment.get("path").and_then(Value::as_str) else {
                    return;
                };
                if self.memory_files.iter().any(|file| file.path == path) {
                    return;
                }
                self.memory_files.push(MemoryFile {
                    path: path.to_string(),
                    kind: attachment
                        .pointer("/content/type")
                        .and_then(Value::as_str)
                        .unwrap_or("Project")
                        .to_string(),
                });
            }
            Some("skill_listing") => {
                if attachment.get("isInitial") == Some(&Value::Bool(true)) {
                    self.skills.clear();
                }
                for (name, description) in listed_skills(attachment) {
                    self.skills.insert(name, description);
                }
            }
            Some("invoked_skills") => {
                for skill in attachment
                    .get("skills")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(name) = skill.get("name").and_then(Value::as_str) {
                        if !self.invoked_skills.iter().any(|known| known == name) {
                            self.invoked_skills.push(name.to_string());
                        }
                    }
                }
            }
            Some("queued_command") => {
                if attachment.pointer("/origin/kind").and_then(Value::as_str)
                    == Some("task-notification")
                {
                    if let Some(prompt) = attachment.get("prompt").and_then(Value::as_str) {
                        self.notifications(prompt);
                    }
                }
            }
            _ => {}
        }
    }

    fn notifications(&mut self, content: &str) {
        if !content.trim_start().starts_with("<task-notification>") {
            return;
        }
        for part in content.split("<task-notification>").skip(1) {
            let Some((body, _)) = part.split_once("</task-notification>") else {
                continue;
            };
            let (Some(id), Some(status)) = (tag(body, "task-id"), tag(body, "status")) else {
                continue;
            };
            if !terminal_status(status) {
                continue;
            }
            if let Some(run) = self.background.get_mut(id) {
                if run.ended_at_ms.is_none() {
                    run.ended_at_ms = Some(self.last_at);
                }
                run.status = Some(status.to_string());
                run.summary = tag(body, "summary").map(str::to_string);
            }
        }
    }

    fn observe_launches(&mut self, record: &Value) {
        let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) else {
            return;
        };
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let kind = match block.get("name").and_then(Value::as_str) {
                        Some("Bash" | "PowerShell") => "shell",
                        Some("Monitor") => "monitor",
                        _ => continue,
                    };
                    let Some(id) = block.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let input = block.get("input");
                    let command = input
                        .and_then(|input| input.get("command"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|command| !command.is_empty());
                    let title = input
                        .and_then(|input| input.get("description"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|description| !description.is_empty())
                        .map(|description| cut(description, MAX_TITLE_CHARS))
                        .or_else(|| {
                            command.and_then(|command| {
                                command
                                    .lines()
                                    .next()
                                    .map(|line| cut(line.trim(), MAX_TITLE_CHARS))
                            })
                        });
                    let Some(title) = title else {
                        continue;
                    };
                    self.pending_launches.insert(
                        id.to_string(),
                        Launch {
                            kind,
                            title,
                            command: command.map(|command| cut(command, MAX_COMMAND_CHARS)),
                            at: self.last_at,
                        },
                    );
                }
                Some("tool_result") => {
                    let Some(launch) = block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .and_then(|id| self.pending_launches.remove(id))
                    else {
                        continue;
                    };
                    let task_id = record
                        .pointer("/toolUseResult/backgroundTaskId")
                        .or_else(|| record.pointer("/toolUseResult/taskId"))
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty());
                    if let Some(task_id) = task_id {
                        if !self.background.contains_key(task_id) {
                            self.background_order.push(task_id.to_string());
                        }
                        self.background.insert(
                            task_id.to_string(),
                            BackgroundRun {
                                id: task_id.to_string(),
                                kind: launch.kind,
                                title: launch.title,
                                command: launch.command,
                                started_at_ms: launch.at,
                                ended_at_ms: None,
                                status: None,
                                summary: None,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// A new or resumed Claude process starts with no shells: the old process's died with it.
    fn forget_unfinished_background(&mut self) {
        self.pending_launches.clear();
        let unfinished: Vec<String> = self
            .background
            .iter()
            .filter(|(_, run)| run.ended_at_ms.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        for id in unfinished {
            self.background.remove(&id);
            self.background_order.retain(|known| *known != id);
        }
    }

    fn trim_background(&mut self) {
        // Ended commands beyond what the fold shows are dropped, oldest first.
        let mut ended: Vec<(i64, String)> = self
            .background
            .values()
            .filter_map(|run| run.ended_at_ms.map(|at| (at, run.id.clone())))
            .collect();
        if ended.len() <= FINISHED_KEPT * 2 {
            return;
        }
        ended.sort();
        for (_, id) in ended.iter().take(ended.len() - FINISHED_KEPT) {
            self.background.remove(id);
            self.background_order.retain(|known| known != id);
        }
    }

    /// Applies the recorded process starts due by `upto` (hookless_activity/process_starts.rs).
    fn apply_process_starts(
        &mut self,
        starts: &[crate::hookless_activity::ClaudeProcessStart],
        upto: i64,
    ) {
        for start in self.starts.due(starts, upto) {
            if start.source.new_process() {
                self.forget_unfinished_background();
            }
        }
    }

    fn observe(&mut self, line: &[u8], starts: &[crate::hookless_activity::ClaudeProcessStart]) {
        // Cheap marker tests first: most records are messages this reader skips.
        let text = String::from_utf8_lossy(line);
        if !self.conversation_started
            && (text.contains("\"type\":\"user\"") || text.contains("\"type\":\"assistant\""))
        {
            self.conversation_started = serde_json::from_slice::<Value>(line).is_ok_and(|record| {
                matches!(
                    record.get("type").and_then(Value::as_str),
                    Some("user" | "assistant")
                )
            });
        }
        let wanted = [
            "\"attachment\"",
            "compact_boundary",
            "run_in_background",
            "\"Monitor\"",
            "backgroundTaskId",
            "\"taskId\"",
            "<task-notification>",
        ];
        if !wanted.iter().any(|marker| text.contains(marker)) {
            return;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if let Some(at) = timestamp(&record) {
            self.apply_process_starts(starts, at);
            self.last_at = at;
        }
        match record.get("type").and_then(Value::as_str) {
            Some("attachment") => {
                let Some(attachment) = record.get("attachment") else {
                    return;
                };
                // An old transcript's hook line for the same start.
                if crate::hookless_activity::claude_hook_line_start(&record)
                    .is_some_and(|source| source.new_process())
                {
                    self.forget_unfinished_background();
                }
                self.observe_attachment(attachment);
            }
            Some("system")
                if record.get("subtype").and_then(Value::as_str) == Some("compact_boundary") =>
            {
                self.reset_context();
            }
            Some("assistant") | Some("user") => {
                self.observe_launches(&record);
                if record.get("type").and_then(Value::as_str) == Some("user")
                    && record.pointer("/origin/kind").and_then(Value::as_str)
                        == Some("task-notification")
                {
                    if let Some(content) =
                        record.pointer("/message/content").and_then(Value::as_str)
                    {
                        self.notifications(content);
                    }
                    for block in record
                        .pointer("/message/content")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            self.notifications(text);
                        }
                    }
                }
            }
            Some("queue-operation")
                if record.get("operation").and_then(Value::as_str) == Some("enqueue") =>
            {
                if let Some(content) = record.get("content").and_then(Value::as_str) {
                    self.notifications(content);
                }
            }
            _ => {}
        }
        self.trim_background();
    }
}

#[derive(Clone)]
struct CacheEntry {
    version: TranscriptFileVersion,
    consumed_to: u64,
    facts: TranscriptFacts,
    used: u64,
}

/// The facts of the Claude transcript at `path`, reading only what was appended since the last call.
/// Blocking.
pub(super) fn scan_claude_transcript(path: &Path) -> std::io::Result<TranscriptFacts> {
    static CACHE: OnceLock<Mutex<(u64, HashMap<PathBuf, CacheEntry>)>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let version = read_transcript_file_version(path)?;
    let previous = cache
        .lock()
        .ok()
        .and_then(|cache| cache.1.get(path).cloned());
    let mut entry = previous
        .filter(|entry| {
            entry.version.identity == version.identity && entry.version.size <= version.size
        })
        .unwrap_or_else(|| CacheEntry {
            version: version.clone(),
            consumed_to: 0,
            facts: TranscriptFacts::default(),
            used: 0,
        });
    let starts = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(crate::hookless_activity::read_claude_process_starts)
        .unwrap_or_default();
    if entry.consumed_to < version.size {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(entry.consumed_to))?;
        let mut reader = BufReader::new(file.take(version.size - entry.consumed_to));
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 || line.last() != Some(&b'\n') {
                // An unterminated last line is still being written; it is read next time.
                break;
            }
            if read <= MAX_RECORD_BYTES {
                entry.facts.observe(&line, &starts);
            }
            entry.consumed_to += read as u64;
        }
    }
    // Starts after the newest record: the process has written nothing since.
    entry.facts.apply_process_starts(&starts, i64::MAX);
    entry.version = version;
    let facts = entry.facts.clone();
    if let Ok(mut cache) = cache.lock() {
        cache.0 += 1;
        entry.used = cache.0;
        if cache.1.len() >= CACHED_TRANSCRIPTS && !cache.1.contains_key(path) {
            if let Some(oldest) = cache
                .1
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone())
            {
                cache.1.remove(&oldest);
            }
        }
        cache.1.insert(path.to_path_buf(), entry);
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hookless_activity::{ClaudeProcessStart, ClaudeStartSource};

    fn launch(second: u32, id: &str, task: &str) -> [String; 2] {
        [
            format!(
                r#"{{"type":"assistant","timestamp":"2026-11-02T10:00:{second:02}.000Z","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"Bash","input":{{"command":"sleep 600","description":"{task}","run_in_background":true}}}}]}}}}"#
            ),
            format!(
                r#"{{"type":"user","timestamp":"2026-11-02T10:00:{:02}.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}"}}]}},"toolUseResult":{{"backgroundTaskId":"{task}"}}}}"#,
                second + 1
            ),
        ]
    }

    fn unfinished(lines: &[String], starts: &[ClaudeProcessStart]) -> Vec<String> {
        let mut facts = TranscriptFacts::default();
        for line in lines {
            facts.observe(line.as_bytes(), starts);
        }
        facts.apply_process_starts(starts, i64::MAX);
        let mut ids: Vec<String> = facts
            .background
            .values()
            .filter(|run| run.ended_at_ms.is_none())
            .map(|run| run.id.clone())
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_recorded_process_start_forgets_the_old_processes_shells() {
        let lines: Vec<String> = launch(1, "toolu_a", "old")
            .into_iter()
            .chain(launch(20, "toolu_b", "new"))
            .collect();
        assert_eq!(unfinished(&lines, &[]), vec!["new", "old"]);
        let resumed = ClaudeProcessStart {
            at: crate::session_chat_fleet_transcript::parse_time("2026-11-02T10:00:10.000Z")
                .unwrap(),
            source: ClaudeStartSource::Resume,
        };
        assert_eq!(unfinished(&lines, &[resumed]), vec!["new"]);
        let cleared = ClaudeProcessStart {
            source: ClaudeStartSource::Clear,
            ..resumed
        };
        assert_eq!(unfinished(&lines, &[cleared]), vec!["new", "old"]);
    }
}
