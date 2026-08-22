use crate::model::Agent;
use crate::model::Session;
use crate::model::TranscriptBlock;
use crate::model::TranscriptKind;
use chrono::DateTime;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/*
CDXC:GhostexHistoryScan 2026-06-25-19:49:
ghostex-history should reuse Zehn's knowledge of local agent storage locations while rendering full sessions instead of individual prompt search hits.
Scan Codex, Claude, Pi, Cursor, and Grok JSONL histories directly from the user home folder; keep opencode out of v1 because its SQLite source needs a separate transcript adapter.

CDXC:GhostexHistoryScan 2026-06-25-20:01:
The list view should match Codex's active-session picker instead of an archival dump.
Skip archived Codex rollouts by default, dedupe repeated session ids, and read only a small prefix for list metadata so full transcript parsing happens when a row is opened.
For Cursor, load metadata only for session ids that have transcript files because local acp-sessions folders can be much larger than the visible history list.

CDXC:GhostexHistoryScan 2026-06-25-19:49:
The v1 transcript intentionally omits tool outputs and tool-result messages.
Persist only a concise tool-call block with the tool name so later work can decide how to display bulky command output without making the first viewer unusable.

CDXC:GhostexHistoryScan 2026-06-25-20:33:
Only user-authored prompts belong in the v1 transcript.
Drop injected environment/setup records, unwrap Grok user_query tags, and keep Grok assistant reasoning plus tool_calls so each source maps to the same user/thinking/tool/response block model.
*/

const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const SUMMARY_PREFIX_BYTES: u64 = 128 * 1024;

#[derive(Clone, Copy)]
enum ParseMode {
    Summary,
    Full,
}

pub struct ScanOptions {
    pub home: PathBuf,
    pub agent_filter: Option<Agent>,
}

pub fn scan_sessions(options: &ScanOptions) -> Vec<Session> {
    let mut sessions = Vec::new();
    if should_scan(options, Agent::Codex) {
        sessions.extend(scan_codex(&options.home));
    }
    if should_scan(options, Agent::Claude) {
        sessions.extend(scan_claude(&options.home));
    }
    if should_scan(options, Agent::Pi) {
        sessions.extend(scan_pi(&options.home));
    }
    if should_scan(options, Agent::Cursor) {
        sessions.extend(scan_cursor(&options.home));
    }
    if should_scan(options, Agent::Grok) {
        sessions.extend(scan_grok(&options.home));
    }
    sessions = dedup_sessions(sessions);
    sessions.retain(|session| !session.blocks.is_empty());
    sessions.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.agent.label().cmp(b.agent.label()))
            .then_with(|| a.display_title().cmp(b.display_title()))
    });
    sessions
}

fn dedup_sessions(sessions: Vec<Session>) -> Vec<Session> {
    let mut by_session = HashMap::<(Agent, String), Session>::new();
    for session in sessions {
        let key = (session.agent, session.id.clone());
        match by_session.get(&key) {
            Some(existing) if existing.updated_at >= session.updated_at => {}
            _ => {
                by_session.insert(key, session);
            }
        }
    }
    by_session.into_values().collect()
}

pub fn load_transcript(session: &Session) -> io::Result<Vec<TranscriptBlock>> {
    let parsed = match session.agent {
        Agent::Claude => parse_claude_file(&session.path, ParseMode::Full),
        Agent::Codex => parse_codex_file(&session.path, &HashMap::new(), ParseMode::Full),
        Agent::Cursor => parse_cursor_file(&session.path, &HashMap::new(), ParseMode::Full),
        Agent::Grok => parse_grok_file(&session.path, ParseMode::Full),
        Agent::Pi => parse_pi_file(&session.path, ParseMode::Full),
    }
    .ok_or_else(|| io::Error::other("failed to parse transcript"))?;
    Ok(parsed.blocks)
}

fn should_scan(options: &ScanOptions, agent: Agent) -> bool {
    options.agent_filter.is_none_or(|filter| filter == agent)
}

fn scan_codex(home: &Path) -> Vec<Session> {
    let mut titles = HashMap::new();
    load_codex_titles(&home.join(".codex"), &mut titles);
    for profile in read_dirs(&home.join(".codex-profiles")) {
        load_codex_titles(&profile, &mut titles);
    }

    let mut files = Vec::new();
    collect_jsonl_files(&home.join(".codex/sessions"), &mut files);
    for profile in read_dirs(&home.join(".codex-profiles")) {
        collect_jsonl_files(&profile.join("sessions"), &mut files);
    }

    files
        .into_iter()
        .filter_map(|path| parse_codex_file(&path, &titles, ParseMode::Summary))
        .collect()
}

fn load_codex_titles(root: &Path, titles: &mut HashMap<String, String>) {
    let path = root.join("session_index.jsonl");
    let Ok(data) = read_text_limited(&path) else {
        return;
    };
    for value in json_lines(&data) {
        let Some(id) = string_field(&value, "id") else {
            continue;
        };
        let Some(title) = title_from_value(&value) else {
            continue;
        };
        titles.insert(id.to_string(), title);
    }
}

fn parse_codex_file(
    path: &Path,
    titles: &HashMap<String, String>,
    mode: ParseMode,
) -> Option<Session> {
    let data = read_history_text(path, mode).ok()?;
    let mut blocks = Vec::new();
    let mut id = session_id_from_path(path);
    let mut title = String::new();
    let mut project = String::new();
    let mut created_at = None;
    let mut updated_at = file_modified_seconds(path).unwrap_or_default();

    for value in json_lines(&data) {
        let ts = timestamp_from_value(&value);
        if let Some(ts) = ts {
            created_at.get_or_insert(ts);
            updated_at = updated_at.max(ts);
        }

        match string_field(&value, "type") {
            Some("session_meta") => {
                if let Some(payload) = value.get("payload") {
                    if let Some(meta_id) = string_field(payload, "id") {
                        id = meta_id.to_string();
                    }
                    if let Some(cwd) = string_field(payload, "cwd") {
                        project = cwd.to_string();
                    }
                    if title.is_empty() {
                        title = title_from_value(payload).unwrap_or_default();
                    }
                }
            }
            Some("message") => {
                parse_role_message(
                    Agent::Codex,
                    string_field(&value, "role"),
                    value.get("content"),
                    ts,
                    &mut blocks,
                );
            }
            Some("response_item") => {
                if let Some(payload) = value.get("payload") {
                    parse_codex_response_item(payload, ts, &mut blocks);
                }
            }
            _ => {}
        }
    }

    if let Some(index_title) = titles.get(&id) {
        title = index_title.clone();
    }
    let preview = first_user_preview(&blocks).unwrap_or_else(|| "No user prompt".to_string());
    if title.is_empty() {
        title = preview.clone();
    }
    Some(Session {
        agent: Agent::Codex,
        id,
        title: clean_inline(&title),
        project,
        path: path.to_path_buf(),
        created_at,
        updated_at,
        preview,
        blocks,
        transcript_loaded: matches!(mode, ParseMode::Full),
    })
}

fn parse_codex_response_item(value: &Value, ts: Option<i64>, blocks: &mut Vec<TranscriptBlock>) {
    match string_field(value, "type").unwrap_or_default() {
        "message" => parse_role_message(
            Agent::Codex,
            string_field(value, "role"),
            value.get("content"),
            ts,
            blocks,
        ),
        "reasoning" => {
            if let Some(text) = reasoning_text(value) {
                push_block(blocks, TranscriptKind::Thinking, text, ts);
            }
        }
        "function_call" | "custom_tool_call" | "local_shell_call" | "mcp_tool_call"
        | "tool_call" => {
            push_tool_block(blocks, tool_name(value).unwrap_or("tool"), ts);
        }
        "function_call_output"
        | "custom_tool_call_output"
        | "mcp_tool_call_output"
        | "tool_call_output" => {}
        _ => {
            if looks_like_tool_call(value) {
                push_tool_block(blocks, tool_name(value).unwrap_or("tool"), ts);
            }
        }
    }
}

fn scan_claude(home: &Path) -> Vec<Session> {
    let mut files = Vec::new();
    collect_jsonl_files(&home.join(".claude/transcripts"), &mut files);
    collect_jsonl_files(&home.join(".claude/projects"), &mut files);
    collect_jsonl_files(&home.join(".claude/projects2"), &mut files);
    for profile in read_dirs(&home.join(".claude-profiles")) {
        collect_jsonl_files(&profile.join("projects"), &mut files);
        collect_jsonl_files(&profile.join("projects2"), &mut files);
    }
    files
        .into_iter()
        .filter_map(|path| parse_claude_file(&path, ParseMode::Summary))
        .collect()
}

fn parse_claude_file(path: &Path, mode: ParseMode) -> Option<Session> {
    let data = read_history_text(path, mode).ok()?;
    let mut blocks = Vec::new();
    let mut id = session_id_from_path(path);
    let mut project = String::new();
    let mut title = String::new();
    let mut created_at = None;
    let mut updated_at = file_modified_seconds(path).unwrap_or_default();

    for value in json_lines(&data) {
        let ts = timestamp_from_value(&value);
        if let Some(ts) = ts {
            created_at.get_or_insert(ts);
            updated_at = updated_at.max(ts);
        }
        if let Some(session_id) = string_field(&value, "sessionId") {
            id = session_id.to_string();
        }
        if let Some(cwd) = string_field(&value, "cwd") {
            project = cwd.to_string();
        }
        if title.is_empty() {
            title = title_from_value(&value).unwrap_or_default();
        }

        match string_field(&value, "type") {
            Some("tool_use") => {
                push_tool_block(
                    &mut blocks,
                    string_field(&value, "tool_name")
                        .or_else(|| string_field(&value, "name"))
                        .unwrap_or("tool"),
                    ts,
                );
            }
            Some("tool_result") => {}
            Some("user") | Some("assistant") => {
                parse_claude_role_record(&value, ts, &mut blocks);
            }
            _ => parse_claude_role_record(&value, ts, &mut blocks),
        }
    }

    let preview = first_user_preview(&blocks).unwrap_or_else(|| "No user prompt".to_string());
    if title.is_empty() {
        title = preview.clone();
    }
    Some(Session {
        agent: Agent::Claude,
        id,
        title: clean_inline(&title),
        project,
        path: path.to_path_buf(),
        created_at,
        updated_at,
        preview,
        blocks,
        transcript_loaded: matches!(mode, ParseMode::Full),
    })
}

fn parse_claude_role_record(value: &Value, ts: Option<i64>, blocks: &mut Vec<TranscriptBlock>) {
    let message = value.get("message").unwrap_or(value);
    let role = string_field(message, "role").or_else(|| string_field(value, "role"));
    let content = message.get("content").or_else(|| value.get("content"));
    parse_role_message(Agent::Claude, role, content, ts, blocks);
}

fn scan_pi(home: &Path) -> Vec<Session> {
    let mut files = Vec::new();
    collect_jsonl_files(&home.join(".pi/agent/sessions"), &mut files);
    files
        .into_iter()
        .filter_map(|path| parse_pi_file(&path, ParseMode::Summary))
        .collect()
}

fn parse_pi_file(path: &Path, mode: ParseMode) -> Option<Session> {
    let data = read_history_text(path, mode).ok()?;
    let mut blocks = Vec::new();
    let mut id = session_id_from_path(path);
    let mut project = String::new();
    let mut title = String::new();
    let mut created_at = None;
    let mut updated_at = file_modified_seconds(path).unwrap_or_default();

    for value in json_lines(&data) {
        let ts = timestamp_from_value(&value);
        if let Some(ts) = ts {
            created_at.get_or_insert(ts);
            updated_at = updated_at.max(ts);
        }
        match string_field(&value, "type") {
            Some("session") => {
                if let Some(session_id) = string_field(&value, "id") {
                    id = session_id.to_string();
                }
                if let Some(cwd) = string_field(&value, "cwd") {
                    project = cwd.to_string();
                }
            }
            Some("session_info") => {
                if let Some(name) = string_field(&value, "name") {
                    title = name.to_string();
                }
            }
            Some("message") => {
                if let Some(message) = value.get("message") {
                    parse_role_message(
                        Agent::Pi,
                        string_field(message, "role"),
                        message.get("content"),
                        ts,
                        &mut blocks,
                    );
                }
            }
            _ => {}
        }
    }

    let preview = first_user_preview(&blocks).unwrap_or_else(|| "No user prompt".to_string());
    if title.is_empty() {
        title = preview.clone();
    }
    Some(Session {
        agent: Agent::Pi,
        id,
        title: clean_inline(&title),
        project,
        path: path.to_path_buf(),
        created_at,
        updated_at,
        preview,
        blocks,
        transcript_loaded: matches!(mode, ParseMode::Full),
    })
}

fn scan_cursor(home: &Path) -> Vec<Session> {
    let mut files = Vec::new();
    let mut session_ids = HashSet::new();
    for project_dir in read_dirs(&home.join(".cursor/projects")) {
        let transcripts_dir = project_dir.join("agent-transcripts");
        for session_dir in read_dirs(&transcripts_dir) {
            let Some(session_id) = session_dir.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let path = session_dir.join(format!("{session_id}.jsonl"));
            if path.is_file() {
                session_ids.insert(session_id.to_string());
                files.push(path);
            }
        }
    }
    let cursor_meta = load_cursor_meta(home, &session_ids);
    files
        .into_iter()
        .filter_map(|path| parse_cursor_file(&path, &cursor_meta, ParseMode::Summary))
        .collect()
}

fn load_cursor_meta(
    home: &Path,
    session_ids: &HashSet<String>,
) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    let root = home.join(".cursor/acp-sessions");
    for id in session_ids {
        let Ok(data) = read_text_limited(&root.join(id).join("meta.json")) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let cwd = string_field(&value, "cwd").unwrap_or_default().to_string();
        let title = title_from_value(&value).unwrap_or_default();
        out.insert(id.clone(), (cwd, title));
    }
    out
}

fn parse_cursor_file(
    path: &Path,
    meta: &HashMap<String, (String, String)>,
    mode: ParseMode,
) -> Option<Session> {
    let data = read_history_text(path, mode).ok()?;
    let id = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| session_id_from_path(path));
    let (project, mut title) = meta.get(&id).cloned().unwrap_or_default();
    let mut blocks = Vec::new();
    let mut created_at = None;
    let updated_at = file_modified_seconds(path).unwrap_or_default();

    for value in json_lines(&data) {
        let ts = timestamp_from_value(&value);
        if let Some(ts) = ts {
            created_at.get_or_insert(ts);
        }
        let role = string_field(&value, "role");
        let content = value
            .get("message")
            .and_then(|message| message.get("content"));
        parse_role_message(Agent::Cursor, role, content, ts, &mut blocks);
    }

    let preview = first_user_preview(&blocks).unwrap_or_else(|| "No user prompt".to_string());
    if title.is_empty() {
        title = preview.clone();
    }
    Some(Session {
        agent: Agent::Cursor,
        id,
        title: clean_inline(&title),
        project,
        path: path.to_path_buf(),
        created_at,
        updated_at,
        preview,
        blocks,
        transcript_loaded: matches!(mode, ParseMode::Full),
    })
}

fn scan_grok(home: &Path) -> Vec<Session> {
    let mut files = Vec::new();
    collect_named_files(
        &home.join(".grok/sessions"),
        "chat_history.jsonl",
        &mut files,
    );
    files
        .into_iter()
        .filter_map(|path| parse_grok_file(&path, ParseMode::Summary))
        .collect()
}

fn parse_grok_file(path: &Path, mode: ParseMode) -> Option<Session> {
    let data = read_history_text(path, mode).ok()?;
    let summary = path
        .parent()
        .and_then(|dir| read_text_limited(&dir.join("summary.json")).ok());
    let summary_value = summary
        .as_deref()
        .and_then(|data| serde_json::from_str::<Value>(data).ok());
    let info = summary_value
        .as_ref()
        .and_then(|value| value.get("info"))
        .unwrap_or(&Value::Null);
    let id = string_field(info, "id")
        .or_else(|| {
            summary_value
                .as_ref()
                .and_then(|value| string_field(value, "id"))
        })
        .map(str::to_string)
        .unwrap_or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("grok")
                .to_string()
        });
    let project = string_field(info, "cwd")
        .or_else(|| {
            summary_value
                .as_ref()
                .and_then(|value| string_field(value, "git_root_dir"))
        })
        .unwrap_or_default()
        .to_string();
    let mut title = summary_value
        .as_ref()
        .and_then(title_from_value)
        .or_else(|| title_from_value(info))
        .unwrap_or_default();
    let mut blocks = Vec::new();
    let mut created_at = None;
    let mut updated_at = file_modified_seconds(path).unwrap_or_default();
    if let Some(summary_value) = summary_value.as_ref() {
        if let Some(ts) = summary_value.get("created_at").and_then(timestamp_value) {
            created_at.get_or_insert(ts);
        }
        if let Some(ts) = timestamp_from_value(summary_value) {
            updated_at = updated_at.max(ts);
        }
    }

    for value in json_lines(&data) {
        let ts = timestamp_from_value(&value);
        if let Some(ts) = ts {
            created_at.get_or_insert(ts);
            updated_at = updated_at.max(ts);
        }
        match string_field(&value, "type") {
            Some("user") => {
                if let Some(text) = content_text(value.get("content")) {
                    push_user_block(&mut blocks, text, ts);
                }
            }
            Some("assistant") => {
                parse_grok_assistant_record(&value, ts, &mut blocks);
            }
            Some("tool_use") | Some("toolCall") | Some("tool_call") => {
                push_tool_block(&mut blocks, tool_name(&value).unwrap_or("tool"), ts);
            }
            _ => {}
        }
    }

    let preview = first_user_preview(&blocks).unwrap_or_else(|| "No user prompt".to_string());
    if title.is_empty() {
        title = preview.clone();
    }
    Some(Session {
        agent: Agent::Grok,
        id,
        title: clean_inline(&title),
        project,
        path: path.to_path_buf(),
        created_at,
        updated_at,
        preview,
        blocks,
        transcript_loaded: matches!(mode, ParseMode::Full),
    })
}

fn parse_role_message(
    agent: Agent,
    role: Option<&str>,
    content: Option<&Value>,
    ts: Option<i64>,
    blocks: &mut Vec<TranscriptBlock>,
) {
    match role.unwrap_or_default() {
        "user" => {
            if content_is_tool_result(content) {
                return;
            }
            if let Some(text) = content_text(content) {
                push_user_block(blocks, text, ts);
            }
        }
        "assistant" => parse_assistant_content(agent, content, ts, blocks),
        "toolResult" | "tool_result" | "developer" | "system" => {}
        _ => {}
    }
}

fn parse_grok_assistant_record(value: &Value, ts: Option<i64>, blocks: &mut Vec<TranscriptBlock>) {
    if let Some(text) = content_text(value.get("reasoning")) {
        push_block(blocks, TranscriptKind::Thinking, text, ts);
    }
    if let Some(text) = content_text(value.get("content")) {
        push_block(blocks, TranscriptKind::Agent, text, ts);
    }
    if let Some(tool_calls) = value.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            push_tool_block(blocks, tool_name(tool_call).unwrap_or("tool"), ts);
        }
    }
}

fn parse_assistant_content(
    _agent: Agent,
    content: Option<&Value>,
    ts: Option<i64>,
    blocks: &mut Vec<TranscriptBlock>,
) {
    let Some(content) = content else {
        return;
    };
    match content {
        Value::String(text) => push_block(blocks, TranscriptKind::Agent, text, ts),
        Value::Array(items) => {
            let mut response = Vec::new();
            let mut thinking = Vec::new();
            for item in items {
                match string_field(item, "type").unwrap_or_default() {
                    "text" | "output_text" => {
                        if let Some(text) = string_field(item, "text") {
                            response.push(text.to_string());
                        }
                    }
                    "thinking" | "reasoning" => {
                        if let Some(text) =
                            string_field(item, "thinking").or_else(|| string_field(item, "text"))
                        {
                            thinking.push(text.to_string());
                        }
                    }
                    "tool_use" | "toolCall" | "tool_call" => {
                        flush_assistant_chunks(blocks, &mut thinking, &mut response, ts);
                        push_tool_block(blocks, tool_name(item).unwrap_or("tool"), ts);
                    }
                    _ => {
                        if looks_like_tool_call(item) {
                            flush_assistant_chunks(blocks, &mut thinking, &mut response, ts);
                            push_tool_block(blocks, tool_name(item).unwrap_or("tool"), ts);
                        }
                    }
                }
            }
            flush_assistant_chunks(blocks, &mut thinking, &mut response, ts);
        }
        _ => {}
    }
}

fn flush_assistant_chunks(
    blocks: &mut Vec<TranscriptBlock>,
    thinking: &mut Vec<String>,
    response: &mut Vec<String>,
    ts: Option<i64>,
) {
    if !thinking.is_empty() {
        push_block(blocks, TranscriptKind::Thinking, thinking.join("\n\n"), ts);
        thinking.clear();
    }
    if !response.is_empty() {
        push_block(blocks, TranscriptKind::Agent, response.join("\n\n"), ts);
        response.clear();
    }
}

fn reasoning_text(value: &Value) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(text) = content_text(value.get("summary")) {
        parts.push(text);
    }
    if let Some(text) = content_text(value.get("content")) {
        parts.push(text);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn content_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => nonempty(text.to_string()),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                match item {
                    Value::String(text) => parts.push(text.to_string()),
                    Value::Object(_) => {
                        if matches!(
                            string_field(item, "type"),
                            Some("tool_result") | Some("toolResult")
                        ) {
                            continue;
                        }
                        if let Some(text) = string_field(item, "text")
                            .or_else(|| string_field(item, "content"))
                            .or_else(|| string_field(item, "thinking"))
                        {
                            parts.push(text.to_string());
                        } else if let Some(summary) = item.get("summary") {
                            if let Some(text) = content_text(Some(summary)) {
                                parts.push(text);
                            }
                        }
                    }
                    _ => {}
                }
            }
            nonempty(parts.join("\n\n"))
        }
        Value::Object(_) => string_field(value?, "text")
            .or_else(|| string_field(value?, "content"))
            .map(str::to_string)
            .and_then(nonempty),
        _ => None,
    }
}

fn content_is_tool_result(content: Option<&Value>) -> bool {
    let Some(Value::Array(items)) = content else {
        return false;
    };
    !items.is_empty()
        && items.iter().all(|item| {
            matches!(
                string_field(item, "type"),
                Some("tool_result") | Some("toolResult")
            ) || item.get("tool_use_id").is_some()
        })
}

fn tool_name(value: &Value) -> Option<&str> {
    string_field(value, "name")
        .or_else(|| string_field(value, "tool_name"))
        .or_else(|| string_field(value, "toolName"))
        .or_else(|| string_field(value, "tool"))
}

fn looks_like_tool_call(value: &Value) -> bool {
    value.get("toolCallId").is_some()
        || value.get("tool_call_id").is_some()
        || value.get("call_id").is_some()
        || value.get("arguments").is_some()
}

fn push_tool_block(blocks: &mut Vec<TranscriptBlock>, name: &str, ts: Option<i64>) {
    push_block(
        blocks,
        TranscriptKind::Tool,
        format!("tool call: {}", clean_inline(name)),
        ts,
    );
}

fn push_user_block(blocks: &mut Vec<TranscriptBlock>, text: impl Into<String>, ts: Option<i64>) {
    let text = text.into();
    if let Some(text) = visible_user_prompt(&text) {
        push_block(blocks, TranscriptKind::User, text, ts);
    }
}

fn visible_user_prompt(text: &str) -> Option<String> {
    if let Some(query) = tagged_text(text, "user_query") {
        return nonempty(query);
    }
    if is_setup_context(text) {
        return None;
    }
    nonempty(text.to_string())
}

fn tagged_text(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

fn push_block(
    blocks: &mut Vec<TranscriptBlock>,
    kind: TranscriptKind,
    text: impl Into<String>,
    ts: Option<i64>,
) {
    if let Some(block) = TranscriptBlock::new(kind, text, ts) {
        blocks.push(block);
    }
}

fn first_user_preview(blocks: &[TranscriptBlock]) -> Option<String> {
    blocks
        .iter()
        .filter(|block| block.kind == TranscriptKind::User)
        .find(|block| !is_setup_context(&block.text))
        .map(|block| truncate_clean(&block.text, 160))
}

fn is_setup_context(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("# AGENTS.md instructions")
        || trimmed.starts_with("<user_info>")
        || trimmed.starts_with("<user_instructions>")
        || trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<codex_internal_context")
        || trimmed.starts_with("<git_status>")
        || trimmed.starts_with("<rules>")
        || trimmed.starts_with("<agent_skills>")
        || trimmed.starts_with("<mcp_file_system>")
        || trimmed.starts_with("<system_reminder>")
}

fn json_lines(data: &str) -> impl Iterator<Item = Value> + '_ {
    data.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            return None;
        }
        serde_json::from_str::<Value>(line).ok()
    })
}

fn string_field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}

fn title_from_value(value: &Value) -> Option<String> {
    for key in [
        "thread_name",
        "threadName",
        "title",
        "session_title",
        "sessionTitle",
        "session_summary",
        "generated_title",
        "name",
    ] {
        let Some(title) = string_field(value, key) else {
            continue;
        };
        let title = clean_inline(title);
        if !title.is_empty() {
            return Some(title);
        }
    }
    None
}

fn timestamp_from_value(value: &Value) -> Option<i64> {
    for key in [
        "updated_at",
        "updatedAt",
        "timestamp",
        "created_at",
        "createdAt",
        "ts",
        "time",
    ] {
        let Some(raw) = value.get(key) else {
            continue;
        };
        if let Some(ts) = timestamp_value(raw) {
            return Some(ts);
        }
    }
    None
}

fn timestamp_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().map(normalize_timestamp),
        Value::String(text) => parse_timestamp(text),
        Value::Object(_) => timestamp_from_value(value),
        _ => None,
    }
}

fn parse_timestamp(text: &str) -> Option<i64> {
    if let Ok(number) = text.parse::<i64>() {
        return Some(normalize_timestamp(number));
    }
    DateTime::parse_from_rfc3339(text)
        .map(|dt| dt.timestamp())
        .ok()
}

fn normalize_timestamp(number: i64) -> i64 {
    if number > 10_000_000_000 {
        number / 1000
    } else {
        number
    }
}

fn collect_jsonl_files(root: &Path, out: &mut Vec<PathBuf>) {
    collect_files(
        root,
        |path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"),
        out,
    );
}

fn collect_named_files(root: &Path, name: &str, out: &mut Vec<PathBuf>) {
    collect_files(
        root,
        |path| path.file_name().and_then(|n| n.to_str()) == Some(name),
        out,
    );
}

fn collect_files(root: &Path, matches: impl Fn(&Path) -> bool + Copy, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if should_skip_path(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_files(&path, matches, out);
        } else if file_type.is_file() && matches(&path) {
            out.push(path);
        }
    }
}

fn should_skip_path(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("node_modules" | "target" | ".git" | "dist" | "build" | "out" | ".cache")
    )
}

fn read_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map(|_| entry.path())
        })
        .collect()
}

fn read_history_text(path: &Path, mode: ParseMode) -> io::Result<String> {
    match mode {
        ParseMode::Summary => read_text_prefix(path, SUMMARY_PREFIX_BYTES),
        ParseMode::Full => read_text_limited(path),
    }
}

fn read_text_prefix(path: &Path, limit: u64) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref().take(limit).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_text_limited(path: &Path) -> io::Result<String> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "history file is too large for v1 scanner",
        ));
    }
    fs::read_to_string(path)
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| {
            stem.rsplit_once('_')
                .map(|(_, id)| id)
                .or_else(|| stem.rsplit_once('-').map(|(_, id)| id))
                .unwrap_or(stem)
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn file_modified_seconds(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_seconds)
}

fn system_time_seconds(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs() as i64)
}

fn clean_inline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_clean(text: &str, max_chars: usize) -> String {
    let clean = clean_inline(text);
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut out = clean
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push_str("...");
    out
}

fn nonempty(text: String) -> Option<String> {
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_response_items_skip_tool_outputs() {
        let mut blocks = Vec::new();
        let user: Value = serde_json::json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "hello"}]
        });
        parse_codex_response_item(&user, Some(10), &mut blocks);
        let tool: Value = serde_json::json!({
            "type": "function_call",
            "name": "shell",
            "arguments": {"cmd": "secret"}
        });
        parse_codex_response_item(&tool, Some(11), &mut blocks);
        let output: Value = serde_json::json!({
            "type": "function_call_output",
            "output": "should not render"
        });
        parse_codex_response_item(&output, Some(12), &mut blocks);

        assert_eq!(
            blocks
                .iter()
                .map(|block| (block.kind, block.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (TranscriptKind::User, "hello"),
                (TranscriptKind::Tool, "tool call: shell")
            ]
        );
    }

    #[test]
    fn pi_assistant_content_keeps_thinking_tool_and_answer() {
        let content: Value = serde_json::json!([
            {"type": "thinking", "thinking": "plan it"},
            {"type": "toolCall", "name": "bash", "arguments": {"command": "ignored"}},
            {"type": "text", "text": "done"}
        ]);
        let mut blocks = Vec::new();
        parse_role_message(
            Agent::Pi,
            Some("assistant"),
            Some(&content),
            None,
            &mut blocks,
        );

        assert_eq!(
            blocks
                .iter()
                .map(|block| (block.kind, block.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (TranscriptKind::Thinking, "plan it"),
                (TranscriptKind::Tool, "tool call: bash"),
                (TranscriptKind::Agent, "done")
            ]
        );
    }

    #[test]
    fn user_prompt_filter_skips_setup_and_unwraps_grok_query() {
        assert_eq!(
            visible_user_prompt("<user_query>\nship the transcript viewer\n</user_query>")
                .as_deref(),
            Some("ship the transcript viewer")
        );
        assert!(visible_user_prompt("<user_info>\nOS Version: darwin\n</user_info>").is_none());
    }

    #[test]
    fn grok_assistant_record_keeps_reasoning_tool_and_answer() {
        let value: Value = serde_json::json!({
            "type": "assistant",
            "content": "checking",
            "reasoning": {"text": "think it through", "encrypted": "ignored"},
            "tool_calls": [{"name": "Read", "arguments": "{\"path\":\"ignored\"}"}]
        });
        let mut blocks = Vec::new();
        parse_grok_assistant_record(&value, Some(20), &mut blocks);

        assert_eq!(
            blocks
                .iter()
                .map(|block| (block.kind, block.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (TranscriptKind::Thinking, "think it through"),
                (TranscriptKind::Agent, "checking"),
                (TranscriptKind::Tool, "tool call: Read")
            ]
        );
    }
}
