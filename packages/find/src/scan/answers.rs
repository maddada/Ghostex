//! Turn-ending agent answer extraction across local agent transcripts.
//!
//! CDXC:PromptSearch 2026-11-22 WHY:
//! Find previously indexed only user prompts from per-agent history files.
//! Indexing each turn's final closing assistant message alongside user prompts
//! lets users search root causes, decisions, file paths and fixes without
//! drowning the index in tool logs, diffs or thinking walls.
//! One rule applies across every agent: the last non-empty assistant text
//! before the next real user message (or end of transcript) is the turn-ending
//! answer; an interrupted turn is still indexed with `interrupted = true`.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent::Agent;

use super::{
    Meta, Record, RecordKind, clean_title, field, mtime_seconds, parse_iso8601_seconds, parse_line,
    read_dir_sorted, string_field, tagged_text, timestamp_from_object, visible_user_prompt,
};

const MAX_ANSWER_BYTES: usize = 64 * 1024;
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct TranscriptSource {
    pub agent: Agent,
    pub path: PathBuf,
    pub session_id: String,
    pub project: String,
    pub title: String,
    pub size: u64,
    pub mtime_ns: i64,
    pub mtime_sec: i64,
}

fn file_stamp(path: &Path) -> Option<(u64, i64, i64)> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    let mtime_sec = mtime_seconds(&meta);
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(mtime_sec.saturating_mul(1_000_000_000));
    Some((size, mtime_ns, mtime_sec))
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim_end().to_string()
}

fn clean_assistant_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("<system-reminder>")
        || trimmed.starts_with("<ghostex-system>")
        || trimmed.starts_with("[Request interrupted")
    {
        return None;
    }
    Some(truncate_utf8(trimmed, MAX_ANSWER_BYTES))
}

fn push_source(
    out: &mut Vec<TranscriptSource>,
    agent: Agent,
    path: PathBuf,
    session_id: String,
    project: String,
    title: String,
) {
    let Some((size, mtime_ns, mtime_sec)) = file_stamp(&path) else {
        return;
    };
    if size == 0 {
        return;
    }
    out.push(TranscriptSource {
        agent,
        path,
        session_id,
        project,
        title,
        size,
        mtime_ns,
        mtime_sec,
    });
}

/// Discover transcript files across all supported agents, ordered newest first
/// so incremental catch-up and background backfill index recent work first.
pub fn discover_transcript_sources(home: &Path) -> Vec<TranscriptSource> {
    let mut sources = Vec::new();
    discover_claude(home, &mut sources);
    discover_codex(home, &mut sources);
    discover_antigravity(home, &mut sources);
    discover_cursor(home, &mut sources);
    discover_pi_like(home, Agent::Pi, ".pi/agent/sessions", &mut sources);
    discover_pi_like(home, Agent::Omp, ".omp/agent/sessions", &mut sources);
    discover_grok(home, &mut sources);
    discover_copilot(home, &mut sources);
    sources.sort_by(|a, b| b.mtime_ns.cmp(&a.mtime_ns));
    sources
}

fn discover_claude(home: &Path, out: &mut Vec<TranscriptSource>) {
    let claude_root = ghostex_paths::agent_homes::claude_default_home(home);
    let mut roots = vec![claude_root.join("projects"), claude_root.join("projects2")];
    let profiles = home.join(".claude-profiles");
    if let Ok(entries) = fs::read_dir(&profiles) {
        for entry in entries.flatten() {
            let p = entry.path().join("projects");
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    for base in roots {
        for proj_dir in read_dir_sorted(&base) {
            let Ok(entries) = fs::read_dir(&proj_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                if session_id.starts_with("agent-") {
                    continue;
                }
                push_source(
                    out,
                    Agent::Claude,
                    path,
                    session_id,
                    String::new(),
                    String::new(),
                );
            }
        }
    }
}

fn discover_codex(home: &Path, out: &mut Vec<TranscriptSource>) {
    let base = ghostex_paths::agent_homes::codex_default_home(home).join("sessions");
    for year in read_dir_sorted(&base) {
        for month in read_dir_sorted(&year) {
            for day in read_dir_sorted(&month) {
                let Ok(entries) = fs::read_dir(&day) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                    {
                        push_source(
                            out,
                            Agent::Codex,
                            path,
                            String::new(),
                            String::new(),
                            String::new(),
                        );
                    }
                }
            }
        }
    }
}

fn read_antigravity_meta(root: &Path) -> HashMap<String, (String, i64)> {
    let mut out = HashMap::new();
    let db_path = root.join("conversation_summaries.db");
    if !db_path.is_file() {
        return out;
    }
    let Ok(conn) = crate::agent_store::open_agent_store(&db_path) else {
        return out;
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT conversation_id, title, CAST(last_modified_time AS TEXT) FROM conversation_summaries",
    ) else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    }) else {
        return out;
    };
    for (id, title, modified) in rows.flatten() {
        let Some(id) = id.filter(|s| !s.is_empty()) else {
            continue;
        };
        let ts = modified.as_deref().map(parse_iso8601_seconds).unwrap_or(0);
        out.insert(id, (title.unwrap_or_default(), ts));
    }
    out
}

fn discover_antigravity(home: &Path, out: &mut Vec<TranscriptSource>) {
    let root = home.join(".gemini/antigravity-cli");
    let brain = root.join("brain");
    if !brain.is_dir() {
        return;
    }
    let summaries = read_antigravity_meta(&root);
    for dir in read_dir_sorted(&brain) {
        let Some(session_id) = dir.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        let logs = dir.join(".system_generated/logs");
        let full = logs.join("transcript_full.jsonl");
        let path = if full.is_file() {
            full
        } else {
            let fallback = logs.join("transcript.jsonl");
            if fallback.is_file() {
                fallback
            } else {
                continue;
            }
        };
        let title = summaries
            .get(&session_id)
            .map(|(t, _)| t.clone())
            .unwrap_or_default();
        push_source(
            out,
            Agent::Antigravity,
            path,
            session_id,
            String::new(),
            title,
        );
    }
}

fn discover_cursor(home: &Path, out: &mut Vec<TranscriptSource>) {
    let base = home.join(".cursor/projects");
    for proj in read_dir_sorted(&base) {
        let transcripts = proj.join("agent-transcripts");
        for sess in read_dir_sorted(&transcripts) {
            let session_id = sess
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let Ok(entries) = fs::read_dir(&sess) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    push_source(
                        out,
                        Agent::Cursor,
                        path,
                        session_id.clone(),
                        String::new(),
                        String::new(),
                    );
                }
            }
        }
    }
}

fn discover_pi_like(home: &Path, agent: Agent, rel: &str, out: &mut Vec<TranscriptSource>) {
    let base = home.join(rel);
    for dir in read_dir_sorted(&base) {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                push_source(
                    out,
                    agent,
                    path,
                    String::new(),
                    String::new(),
                    String::new(),
                );
            }
        }
    }
}

fn discover_grok(home: &Path, out: &mut Vec<TranscriptSource>) {
    let base = home.join(".grok/sessions");
    for proj in read_dir_sorted(&base) {
        for sess in read_dir_sorted(&proj) {
            let path = sess.join("chat_history.jsonl");
            if path.is_file() {
                let session_id = sess
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                push_source(
                    out,
                    Agent::Grok,
                    path,
                    session_id,
                    String::new(),
                    String::new(),
                );
            }
        }
    }
}

fn discover_copilot(home: &Path, out: &mut Vec<TranscriptSource>) {
    let base = home.join(".copilot/session-state");
    for sess in read_dir_sorted(&base) {
        let path = sess.join("events.jsonl");
        if path.is_file() {
            let session_id = sess
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            push_source(
                out,
                Agent::Copilot,
                path,
                session_id,
                String::new(),
                String::new(),
            );
        }
    }
}

struct TurnAccumulator {
    agent: Agent,
    session_id: String,
    project: String,
    title: String,
    fallback_ts: i64,
    pending_text: Option<String>,
    pending_ts: i64,
    pending_interrupted: bool,
    out: Vec<Record>,
}

impl TurnAccumulator {
    fn new(source: &TranscriptSource) -> Self {
        Self {
            agent: source.agent,
            session_id: source.session_id.clone(),
            project: source.project.clone(),
            title: source.title.clone(),
            fallback_ts: source.mtime_sec,
            pending_text: None,
            pending_ts: 0,
            pending_interrupted: false,
            out: Vec::new(),
        }
    }

    fn flush_turn(&mut self) {
        let Some(text) = self.pending_text.take() else {
            self.pending_interrupted = false;
            return;
        };
        let ts = if self.pending_ts > 0 {
            self.pending_ts
        } else {
            self.fallback_ts
        };
        let interrupted = std::mem::take(&mut self.pending_interrupted);
        self.out.push(Record {
            agent: self.agent,
            kind: RecordKind::AgentAnswer,
            interrupted,
            title: self.title.clone(),
            text,
            project: self.project.clone(),
            session: self.session_id.clone(),
            ts,
            meta: Meta::default(),
        });
    }

    fn on_user_turn(&mut self) {
        self.flush_turn();
    }

    fn on_assistant_text(&mut self, text: &str, ts: i64) {
        if let Some(clean) = clean_assistant_text(text) {
            self.pending_text = Some(clean);
            if ts > 0 {
                self.pending_ts = ts;
            }
            self.pending_interrupted = false;
        }
    }

    fn on_interrupted(&mut self) {
        if self.pending_text.is_some() {
            self.pending_interrupted = true;
        }
    }

    fn finish(mut self) -> Vec<Record> {
        self.flush_turn();
        // Backfill project/title/session learned later in the file onto earlier answers.
        for rec in &mut self.out {
            if rec.project.is_empty() && !self.project.is_empty() {
                rec.project = self.project.clone();
            }
            if rec.title.is_empty() && !self.title.is_empty() {
                rec.title = self.title.clone();
            }
            if rec.session.is_empty() && !self.session_id.is_empty() {
                rec.session = self.session_id.clone();
            }
        }
        self.out
    }
}

fn extract_assistant_blocks_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => clean_assistant_text(s),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                let kind = string_field(item, "type").unwrap_or_default();
                if matches!(kind, "text" | "output_text") {
                    if let Some(t) = string_field(item, "text") {
                        let trimmed = t.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed);
                        }
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                clean_assistant_text(&parts.join("\n\n"))
            }
        }
        _ => None,
    }
}

fn read_lines_stream(path: &Path, mut on_line: impl FnMut(&Value)) {
    let Ok(file) = File::open(path) else {
        return;
    };
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut buf = Vec::with_capacity(8 * 1024);
    loop {
        buf.clear();
        let Ok(n) = reader.read_until(b'\n', &mut buf) else {
            break;
        };
        if n == 0 {
            break;
        }
        if buf.len() > MAX_LINE_BYTES {
            continue;
        }
        if let Some(v) = parse_line(&buf) {
            on_line(&v);
        }
    }
}

/// Extract clean turn-ending agent answers from one transcript file.
pub fn extract_answers_from_file(source: &TranscriptSource) -> Vec<Record> {
    match source.agent {
        Agent::Claude => extract_claude_answers(source),
        Agent::Codex => extract_codex_answers(source),
        Agent::Antigravity => extract_antigravity_answers(source),
        Agent::Cursor => extract_cursor_answers(source),
        Agent::Pi | Agent::Omp => extract_pi_answers(source),
        Agent::Grok => extract_grok_answers(source),
        Agent::Copilot => extract_copilot_answers(source),
        _ => Vec::new(),
    }
}

fn extract_claude_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        if field(v, "isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        if acc.project.is_empty() {
            if let Some(cwd) = string_field(v, "cwd").filter(|s| !s.is_empty()) {
                acc.project = cwd.to_string();
            }
        }
        if acc.session_id.is_empty() {
            if let Some(sid) = string_field(v, "sessionId").filter(|s| !s.is_empty()) {
                acc.session_id = sid.to_string();
            }
        }
        if acc.title.is_empty() {
            if let Some(t) = string_field(v, "customTitle")
                .or_else(|| string_field(v, "summary"))
                .and_then(clean_title)
            {
                acc.title = t;
            }
        }
        let kind = string_field(v, "type").unwrap_or_default();
        let ts = timestamp_from_object(v);
        match kind {
            "user" => {
                if field(v, "isMeta").and_then(Value::as_bool) == Some(true) {
                    return;
                }
                let content = field(v, "message").and_then(|m| field(m, "content"));
                let is_tool_result_only = content.is_some_and(|c| {
                    c.as_array().is_some_and(|arr| {
                        !arr.is_empty()
                            && arr
                                .iter()
                                .all(|b| string_field(b, "type") == Some("tool_result"))
                    })
                });
                if is_tool_result_only {
                    return;
                }
                if let Some(text) = content.and_then(extract_assistant_blocks_text) {
                    if text.starts_with("[Request interrupted") {
                        acc.on_interrupted();
                        return;
                    }
                    if visible_user_prompt(&text).is_some() {
                        acc.on_user_turn();
                    }
                }
            }
            "assistant" => {
                if let Some(content) = field(v, "message").and_then(|m| field(m, "content")) {
                    if let Some(text) = extract_assistant_blocks_text(content) {
                        acc.on_assistant_text(&text, ts);
                    }
                }
            }
            _ => {}
        }
    });
    acc.finish()
}

fn extract_codex_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        let kind = string_field(v, "type").unwrap_or_default();
        let ts = timestamp_from_object(v);
        match kind {
            "session_meta" => {
                if let Some(payload) = field(v, "payload") {
                    if acc.session_id.is_empty() {
                        if let Some(id) = string_field(payload, "id") {
                            acc.session_id = id.to_string();
                        }
                    }
                    if acc.project.is_empty() {
                        if let Some(cwd) = string_field(payload, "cwd") {
                            acc.project = cwd.to_string();
                        }
                    }
                }
            }
            "event_msg" => {
                if let Some(payload) = field(v, "payload") {
                    let ev_type = string_field(payload, "type").unwrap_or_default();
                    match ev_type {
                        "user_message" => {
                            if let Some(msg) = string_field(payload, "message") {
                                if visible_user_prompt(msg).is_some() {
                                    acc.on_user_turn();
                                }
                            }
                        }
                        "agent_message" => {
                            if let Some(msg) = string_field(payload, "message") {
                                acc.on_assistant_text(msg, ts);
                            }
                        }
                        "task_complete" => {
                            if let Some(msg) = string_field(payload, "last_agent_message") {
                                acc.on_assistant_text(msg, ts);
                            }
                            acc.flush_turn();
                        }
                        "turn_aborted" => {
                            acc.on_interrupted();
                        }
                        _ => {}
                    }
                }
            }
            "response_item" => {
                if let Some(payload) = field(v, "payload") {
                    if string_field(payload, "type") == Some("message") {
                        let role = string_field(payload, "role").unwrap_or_default();
                        if role == "user" {
                            if let Some(content) = field(payload, "content") {
                                if let Some(text) = super::content_text(content) {
                                    if visible_user_prompt(&text).is_some() {
                                        acc.on_user_turn();
                                    }
                                }
                            }
                        } else if role == "assistant" {
                            if let Some(content) = field(payload, "content") {
                                if let Some(text) = extract_assistant_blocks_text(content) {
                                    acc.on_assistant_text(&text, ts);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    });
    acc.finish()
}

fn extract_antigravity_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        let step_type = string_field(v, "type").unwrap_or_default();
        let ts = timestamp_from_object(v);
        match step_type {
            "USER_INPUT" => {
                if let Some(content) = string_field(v, "content") {
                    if acc.project.is_empty() {
                        if let Some(ws) = tagged_text(content, "WORKSPACE_PATH")
                            .or_else(|| tagged_text(content, "CWD"))
                        {
                            if !ws.is_empty() {
                                acc.project = ws.to_string();
                            }
                        }
                    }
                    let text = tagged_text(content, "USER_REQUEST").unwrap_or(content);
                    if visible_user_prompt(text).is_some() {
                        acc.on_user_turn();
                    }
                }
            }
            "PLANNER_RESPONSE" => {
                let status = string_field(v, "status").unwrap_or_default();
                let has_tool_calls = field(v, "tool_calls")
                    .and_then(Value::as_array)
                    .is_some_and(|arr| !arr.is_empty());
                if let Some(content) = string_field(v, "content") {
                    // Skip mid-turn commentary attached to tool calls unless no final answer follows.
                    if !has_tool_calls || acc.pending_text.is_none() {
                        acc.on_assistant_text(content, ts);
                    }
                }
                if matches!(status, "CANCELLED" | "INTERRUPTED" | "ABORTED") {
                    acc.on_interrupted();
                }
            }
            _ => {}
        }
    });
    acc.finish()
}

fn extract_cursor_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        let role = string_field(v, "role").unwrap_or_default();
        let ts = timestamp_from_object(v);
        let content = field(v, "message")
            .and_then(|m| field(m, "content"))
            .or_else(|| field(v, "content"));
        match role {
            "user" => {
                if let Some(text) = content.and_then(super::content_text) {
                    if visible_user_prompt(&text).is_some() {
                        acc.on_user_turn();
                    }
                }
            }
            "assistant" => {
                if let Some(text) = content.and_then(extract_assistant_blocks_text) {
                    acc.on_assistant_text(&text, ts);
                }
            }
            _ => {}
        }
    });
    acc.finish()
}

fn extract_pi_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        let kind = string_field(v, "type").unwrap_or_default();
        let ts = timestamp_from_object(v);
        if kind == "session" {
            if acc.session_id.is_empty() {
                if let Some(id) = string_field(v, "id") {
                    acc.session_id = id.to_string();
                }
            }
            if acc.project.is_empty() {
                if let Some(cwd) = string_field(v, "cwd") {
                    acc.project = cwd.to_string();
                }
            }
            return;
        }
        if kind != "message" {
            return;
        }
        let Some(msg) = field(v, "message") else {
            return;
        };
        let role = string_field(msg, "role").unwrap_or_default();
        let content = field(msg, "content");
        match role {
            "user" => {
                if let Some(text) = content.and_then(super::content_text) {
                    if visible_user_prompt(&text).is_some() {
                        acc.on_user_turn();
                    }
                }
            }
            "assistant" => {
                if let Some(text) = content.and_then(extract_assistant_blocks_text) {
                    acc.on_assistant_text(&text, ts);
                }
            }
            _ => {}
        }
    });
    acc.finish()
}

fn extract_grok_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        let role = string_field(v, "role").unwrap_or_default();
        let ts = timestamp_from_object(v);
        let content = field(v, "content");
        match role {
            "user" => {
                if let Some(text) = content.and_then(super::content_text) {
                    if visible_user_prompt(&text).is_some() {
                        acc.on_user_turn();
                    }
                }
            }
            "assistant" => {
                if let Some(text) = content.and_then(extract_assistant_blocks_text) {
                    acc.on_assistant_text(&text, ts);
                }
            }
            _ => {}
        }
    });
    acc.finish()
}

fn extract_copilot_answers(source: &TranscriptSource) -> Vec<Record> {
    let mut acc = TurnAccumulator::new(source);
    read_lines_stream(&source.path, |v| {
        let kind = string_field(v, "type").unwrap_or_default();
        let ts = timestamp_from_object(v);
        let data = field(v, "data");
        match kind {
            "user.message" => {
                if let Some(msg) = data.and_then(|d| string_field(d, "content")) {
                    if visible_user_prompt(msg).is_some() {
                        acc.on_user_turn();
                    }
                }
            }
            "assistant.message" => {
                if let Some(msg) = data.and_then(|d| string_field(d, "content")) {
                    acc.on_assistant_text(msg, ts);
                }
            }
            "assistant.turn_end" => {
                acc.flush_turn();
            }
            _ => {}
        }
    });
    acc.finish()
}
