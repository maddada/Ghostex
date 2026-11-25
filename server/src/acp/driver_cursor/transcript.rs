//! Live Cursor-format JSONL transcript writer and ACP tool/diff canonicalization for Native
//! Cursor sessions.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use super::config_and_store::{
    cursor_native_transcript_meta_path, cursor_native_transcript_path, write_private_file,
};
use crate::acp::host::now_ms;

/* ------------------------------------------- live transcript persistence */

pub(super) struct ActiveToolBuffer {
    call_id: String,
    title: String,
    kind: String,
    raw_input: Value,
    locations: Vec<Value>,
    content: Vec<Value>,
    raw_output: Value,
    emitted_call: bool,
}

pub(super) struct CursorTranscriptWriter {
    path: Option<PathBuf>,
    file: Option<File>,
    seq: u64,
    pub(super) suppress_writes: bool,
    pending_thought: String,
    pending_text: String,
    tools: HashMap<String, ActiveToolBuffer>,
}

impl CursorTranscriptWriter {
    pub(super) fn new() -> Self {
        Self {
            path: None,
            file: None,
            seq: 0,
            suppress_writes: false,
            pending_thought: String::new(),
            pending_text: String::new(),
            tools: HashMap::new(),
        }
    }

    /// Bind this writer to `<state_dir>/cursor-native-transcripts/<session_id>.jsonl`.
    /// Returns `true` if the transcript already contains prior turns (seeded or resumed),
    /// telling the caller to suppress duplicate `session/load` replay writes.
    pub(super) fn bind(&mut self, session_id: &str) -> bool {
        let Some(path) = cursor_native_transcript_path(session_id) else {
            return false;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }
        let mut already_populated =
            path.is_file() && fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false);

        // On Terminal -> Native switch (including Native -> Terminal -> Native round-trips),
        // seed or refresh the Native transcript from the Cursor mirror whenever the raw CLI
        // transcript has grown past `seededFromRawLen`.
        if let Some(raw_cli_path) =
            crate::session_chat_paths::find_cursor_chat_transcript(session_id)
        {
            let raw_len = fs::metadata(&raw_cli_path).map(|m| m.len()).unwrap_or(0);
            let prev_seeded_len = cursor_native_transcript_meta_path(session_id)
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .and_then(|v| v.get("seededFromRawLen").and_then(Value::as_u64))
                .unwrap_or(0);
            let needs_seed = !already_populated || raw_len > prev_seeded_len;
            if needs_seed {
                if let Some(mirror) =
                    crate::session_chat_cursor_mirror::resolve_cursor_chat_transcript_path(
                        Some(session_id),
                        Some(&raw_cli_path),
                    )
                {
                    if mirror != path && mirror.is_file() {
                        if let Ok(bytes) = fs::read(&mirror) {
                            if !bytes.is_empty() && write_private_file(&path, &bytes).is_ok() {
                                already_populated = true;
                                if let Some(meta_path) =
                                    cursor_native_transcript_meta_path(session_id)
                                {
                                    let _ = write_private_file(
                                        &meta_path,
                                        json!({ "seededFromRawLen": raw_len })
                                            .to_string()
                                            .as_bytes(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Count existing lines so `ghostexId` sequence numbers remain strictly monotonic.
        if already_populated {
            if let Ok(existing) = fs::read_to_string(&path) {
                self.seq = existing.lines().count() as u64 + 10;
            }
        }

        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        self.file = opts.open(&path).ok();
        self.path = Some(path);
        already_populated
    }

    pub(super) fn path_string(&self) -> Option<String> {
        self.path.as_ref().map(|p| p.to_string_lossy().into_owned())
    }

    pub(super) fn next_id(&mut self, prefix: &str) -> String {
        self.seq += 1;
        format!("native:{prefix}:{}", self.seq)
    }

    pub(super) fn append_record(&mut self, mut record: Value) {
        if self.suppress_writes {
            return;
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if let Some(obj) = record.as_object_mut() {
            obj.entry("ghostexTimestamp".to_string())
                .or_insert_with(|| json!(now_ms()));
            obj.entry("origin".to_string())
                .or_insert_with(|| json!("cursor-native"));
        }
        if let Ok(encoded) = serde_json::to_string(&record) {
            let _ = file.write_all(encoded.as_bytes());
            let _ = file.write_all(b"\n");
            let _ = file.flush();
        }
    }

    pub(super) fn write_user_message(&mut self, text: &str) {
        self.flush_thought();
        self.flush_text();
        let id = self.next_id("u");
        self.append_record(json!({
            "role": "user",
            "message": {
                "content": [{ "type": "text", "text": text }]
            },
            "ghostexId": id,
        }));
    }

    pub(super) fn push_thought(&mut self, chunk: &str) {
        if self.suppress_writes || chunk.is_empty() {
            return;
        }
        self.flush_text();
        self.pending_thought.push_str(chunk);
    }

    pub(super) fn push_text(&mut self, chunk: &str) {
        if self.suppress_writes || chunk.is_empty() {
            return;
        }
        self.flush_thought();
        self.pending_text.push_str(chunk);
    }

    pub(super) fn flush_thought(&mut self) {
        if self.pending_thought.trim().is_empty() {
            self.pending_thought.clear();
            return;
        }
        let text = std::mem::take(&mut self.pending_thought);
        let id = self.next_id("th");
        self.append_record(json!({
            "role": "assistant",
            "message": {
                "content": [{ "type": "thinking", "text": text }]
            },
            "ghostexId": id,
        }));
    }

    pub(super) fn flush_text(&mut self) {
        if self.pending_text.trim().is_empty() {
            self.pending_text.clear();
            return;
        }
        let text = std::mem::take(&mut self.pending_text);
        let id = self.next_id("t");
        self.append_record(json!({
            "role": "assistant",
            "message": {
                "content": [{ "type": "text", "text": text }]
            },
            "ghostexId": id,
        }));
    }

    pub(super) fn write_plan_todos(&mut self, entries: &[Value]) {
        if self.suppress_writes || entries.is_empty() {
            return;
        }
        self.flush_thought();
        self.flush_text();
        let todos: Vec<Value> = entries
            .iter()
            .filter_map(|entry| {
                let content = entry
                    .get("content")
                    .or_else(|| entry.get("text"))
                    .and_then(Value::as_str)?
                    .trim();
                if content.is_empty() {
                    return None;
                }
                let raw_status = entry
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending");
                let status = match raw_status {
                    "in_progress" | "inProgress" | "running" => "in_progress",
                    "completed" | "done" => "completed",
                    _ => "pending",
                };
                Some(json!({
                    "content": content,
                    "status": status,
                }))
            })
            .collect();
        if todos.is_empty() {
            return;
        }
        let call_id = self.next_id("todo");
        let id = self.next_id("tc");
        self.append_record(json!({
            "role": "assistant",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "id": call_id,
                    "name": "TodoWrite",
                    "input": { "todos": todos },
                }]
            },
            "ghostexId": id,
        }));
    }

    pub(super) fn write_plan_markdown(&mut self, name: Option<&str>, plan_markdown: &str) {
        if self.suppress_writes || plan_markdown.trim().is_empty() {
            return;
        }
        self.flush_thought();
        self.flush_text();
        let call_id = self.next_id("plan");
        let id = self.next_id("tc");
        let mut input = Map::new();
        if let Some(n) = name.filter(|s| !s.trim().is_empty()) {
            input.insert("name".to_string(), json!(n));
        }
        input.insert("plan".to_string(), json!(plan_markdown));
        self.append_record(json!({
            "role": "assistant",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "id": call_id,
                    "name": "CreatePlan",
                    "input": Value::Object(input),
                }]
            },
            "ghostexId": id,
        }));
    }

    pub(super) fn handle_tool_update(&mut self, update: &Value) {
        if self.suppress_writes {
            return;
        }
        let Some(call_id) = update
            .get("toolCallId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return;
        };
        self.flush_thought();
        self.flush_text();

        let status = update
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let entry = self
            .tools
            .entry(call_id.to_string())
            .or_insert_with(|| ActiveToolBuffer {
                call_id: call_id.to_string(),
                title: String::new(),
                kind: String::new(),
                raw_input: Value::Null,
                locations: Vec::new(),
                content: Vec::new(),
                raw_output: Value::Null,
                emitted_call: false,
            });

        if let Some(title) = update.get("title").and_then(Value::as_str) {
            if !title.trim().is_empty() {
                entry.title = title.to_string();
            }
        }
        if let Some(kind) = update.get("kind").and_then(Value::as_str) {
            if !kind.trim().is_empty() {
                entry.kind = kind.to_string();
            }
        }
        if let Some(raw_input) = update.get("rawInput") {
            if !raw_input.is_null() && raw_input.as_object().map(|m| !m.is_empty()).unwrap_or(true)
            {
                entry.raw_input = raw_input.clone();
            }
        }
        if let Some(locs) = update.get("locations").and_then(Value::as_array) {
            if !locs.is_empty() {
                entry.locations = locs.clone();
            }
        }
        if let Some(content) = update.get("content").and_then(Value::as_array) {
            if !content.is_empty() {
                entry.content = content.clone();
            }
        }
        if let Some(raw_output) = update.get("rawOutput") {
            if !raw_output.is_null() {
                entry.raw_output = raw_output.clone();
            }
        }

        let is_terminal = matches!(
            status,
            "completed" | "failed" | "cancelled" | "canceled" | "rejected"
        );
        let ready_to_emit_call = !entry.emitted_call
            && (is_terminal
                || entry.raw_input.as_object().is_some_and(|m| !m.is_empty())
                || !entry.locations.is_empty());

        let mut call_record: Option<(String, String, Value)> = None;
        let mut result_record: Option<(String, String, bool, Option<String>)> = None;

        if ready_to_emit_call {
            entry.emitted_call = true;
            let (name, input) = canonicalize_acp_tool(entry);
            call_record = Some((entry.call_id.clone(), name, input));
        }

        if is_terminal {
            let finished = self.tools.remove(call_id).unwrap();
            let (output, is_error, diff) = extract_acp_tool_result(&finished, status);
            result_record = Some((finished.call_id, output, is_error, diff));
        }

        if let Some((cid, name, input)) = call_record {
            let id = self.next_id("tc");
            self.append_record(json!({
                "role": "assistant",
                "message": {
                    "content": [{
                        "type": "tool_use",
                        "id": cid,
                        "name": name,
                        "input": input,
                    }]
                },
                "ghostexId": id,
            }));
        }

        if let Some((cid, output, is_error, diff)) = result_record {
            let id = self.next_id("tr");
            let mut block = json!({
                "type": "tool_result",
                "tool_use_id": cid,
                "content": output,
            });
            if is_error {
                block["is_error"] = json!(true);
            }
            if let Some(diff_str) = diff.filter(|d| !d.trim().is_empty()) {
                block["diff"] = json!(diff_str);
            }
            self.append_record(json!({
                "role": "tool",
                "message": { "content": [block] },
                "ghostexId": id,
            }));
        }
    }

    pub(super) fn finish_turn(&mut self, status: &str, error_message: Option<&str>) {
        if self.suppress_writes {
            return;
        }
        self.flush_thought();
        self.flush_text();
        // Flush any orphaned in-flight tool calls cleanly.
        let orphans: Vec<ActiveToolBuffer> = self.tools.drain().map(|(_, v)| v).collect();
        for tool in orphans {
            if !tool.emitted_call {
                let (name, input) = canonicalize_acp_tool(&tool);
                let id = self.next_id("tc");
                self.append_record(json!({
                    "role": "assistant",
                    "message": {
                        "content": [{
                            "type": "tool_use",
                            "id": tool.call_id,
                            "name": name,
                            "input": input,
                        }]
                    },
                    "ghostexId": id,
                }));
            }
        }
        let id = self.next_id("end");
        let mut rec = json!({
            "type": "turn_ended",
            "status": status,
            "ghostexId": id,
        });
        if let Some(err) = error_message.filter(|s| !s.trim().is_empty()) {
            rec["error"] = json!(err);
        }
        self.append_record(rec);
    }
}

pub(super) fn canonicalize_acp_tool(tool: &ActiveToolBuffer) -> (String, Value) {
    let mut input = tool.raw_input.as_object().cloned().unwrap_or_default();
    let loc_path = tool
        .locations
        .first()
        .and_then(|l| l.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let diff_path = tool.content.iter().find_map(|item| {
        (item.get("type").and_then(Value::as_str) == Some("diff"))
            .then(|| item.get("path").and_then(Value::as_str))
            .flatten()
            .map(str::to_string)
    });
    let lower_title = tool.title.to_ascii_lowercase();

    match tool.kind.as_str() {
        "read" => {
            if !input.contains_key("file_path") {
                if let Some(p) = input.remove("path").or_else(|| loc_path.map(Value::String)) {
                    input.insert("file_path".to_string(), p);
                }
            }
            ("Read".to_string(), Value::Object(input))
        }
        "edit" | "delete" | "move" => {
            if lower_title.starts_with("delete ") || tool.kind == "delete" {
                if !input.contains_key("path") {
                    if let Some(p) = loc_path.or(diff_path) {
                        input.insert("path".to_string(), Value::String(p));
                    }
                }
                return ("Delete".to_string(), Value::Object(input));
            }
            if !input.contains_key("file_path") {
                if let Some(p) = input
                    .remove("path")
                    .or_else(|| diff_path.or(loc_path).map(Value::String))
                {
                    input.insert("file_path".to_string(), p);
                }
            }
            if lower_title.starts_with("write ") || input.contains_key("fileText") {
                if let Some(text) = input
                    .remove("fileText")
                    .or_else(|| input.remove("contents"))
                {
                    input.insert("content".to_string(), text);
                }
                ("Write".to_string(), Value::Object(input))
            } else {
                ("Edit".to_string(), Value::Object(input))
            }
        }
        "execute" => {
            if !input.contains_key("command") {
                let cmd = tool.title.trim().trim_matches('`');
                if !cmd.is_empty() {
                    input.insert("command".to_string(), Value::String(cmd.to_string()));
                }
            }
            ("Bash".to_string(), Value::Object(input))
        }
        "search" => {
            if input.contains_key("globPattern") || input.contains_key("glob_pattern") {
                if let Some(p) = input
                    .remove("globPattern")
                    .or_else(|| input.remove("glob_pattern"))
                {
                    input.insert("pattern".to_string(), p);
                }
                if let Some(d) = input
                    .remove("targetDirectory")
                    .or_else(|| input.remove("target_directory"))
                {
                    input.insert("path".to_string(), d);
                }
                ("Glob".to_string(), Value::Object(input))
            } else if input.contains_key("pattern") {
                ("Grep".to_string(), Value::Object(input))
            } else if lower_title.starts_with("list ") || lower_title.starts_with("ls ") {
                ("LS".to_string(), Value::Object(input))
            } else if input.contains_key("query") {
                if lower_title.contains("web") {
                    ("WebSearch".to_string(), Value::Object(input))
                } else {
                    ("SemSearch".to_string(), Value::Object(input))
                }
            } else {
                ("Grep".to_string(), Value::Object(input))
            }
        }
        "fetch" => {
            if lower_title.contains("search") {
                ("WebSearch".to_string(), Value::Object(input))
            } else {
                ("WebFetch".to_string(), Value::Object(input))
            }
        }
        _ => {
            if lower_title.starts_with("create plan") || input.contains_key("plan") {
                ("CreatePlan".to_string(), Value::Object(input))
            } else if input.contains_key("todos") {
                ("TodoWrite".to_string(), Value::Object(input))
            } else if input.contains_key("prompt") && input.contains_key("description") {
                ("Task".to_string(), Value::Object(input))
            } else if let (Some(provider), Some(tool_name)) = (
                input.get("providerIdentifier").and_then(Value::as_str),
                input.get("toolName").and_then(Value::as_str),
            ) {
                let name = format!("mcp__{provider}__{tool_name}");
                let inner = input.get("args").cloned().unwrap_or(Value::Object(input));
                (name, inner)
            } else {
                let name = if !tool.title.trim().is_empty() {
                    tool.title.clone()
                } else if !tool.kind.trim().is_empty() {
                    tool.kind.clone()
                } else {
                    "Tool".to_string()
                };
                (name, Value::Object(input))
            }
        }
    }
}

pub(super) fn build_unified_diff(path: &str, old_text: Option<&str>, new_text: &str) -> String {
    let old = old_text.unwrap_or_default();
    let old_lines: Vec<&str> = if old.is_empty() {
        Vec::new()
    } else {
        old.lines().collect()
    };
    let new_lines: Vec<&str> = if new_text.is_empty() {
        Vec::new()
    } else {
        new_text.lines().collect()
    };

    // Trim common prefix and suffix lines to keep diff hunks compact and readable.
    let mut prefix = 0usize;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix + prefix < old_lines.len()
        && suffix + prefix < new_lines.len()
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let ctx = 3usize;
    let hunk_old_start = prefix.saturating_sub(ctx);
    let hunk_new_start = prefix.saturating_sub(ctx);
    let old_change_end = old_lines.len() - suffix;
    let new_change_end = new_lines.len() - suffix;
    let hunk_old_end = (old_change_end + ctx).min(old_lines.len());
    let hunk_new_end = (new_change_end + ctx).min(new_lines.len());

    let old_count = hunk_old_end - hunk_old_start;
    let new_count = hunk_new_end - hunk_new_start;
    let old_hdr_start = if old_count == 0 {
        hunk_old_start
    } else {
        hunk_old_start + 1
    };
    let new_hdr_start = if new_count == 0 {
        hunk_new_start
    } else {
        hunk_new_start + 1
    };

    let mut out = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -{old_hdr_start},{old_count} +{new_hdr_start},{new_count} @@\n"
    );
    for line in &old_lines[hunk_old_start..prefix] {
        out.push(' ');
        out.push_str(line);
        out.push('\n');
    }
    for line in &old_lines[prefix..old_change_end] {
        out.push('-');
        out.push_str(line);
        out.push('\n');
    }
    for line in &new_lines[prefix..new_change_end] {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    for line in &old_lines[old_change_end..hunk_old_end] {
        out.push(' ');
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub(super) fn extract_acp_tool_result(
    tool: &ActiveToolBuffer,
    status: &str,
) -> (String, bool, Option<String>) {
    let mut is_error = matches!(status, "failed" | "cancelled" | "canceled" | "rejected");
    let mut diff: Option<String> = None;

    for item in &tool.content {
        if item.get("type").and_then(Value::as_str) == Some("diff") {
            let path = item.get("path").and_then(Value::as_str).unwrap_or("file");
            let old_text = item.get("oldText").and_then(Value::as_str);
            let new_text = item.get("newText").and_then(Value::as_str).unwrap_or("");
            diff = Some(build_unified_diff(path, old_text, new_text));
            break;
        }
    }

    if diff.is_none() {
        if let Some(d) = tool
            .raw_output
            .get("diffString")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            diff = Some(d.to_string());
        }
    }

    if tool.kind == "execute" {
        if let Some(code) = tool.raw_output.get("exitCode").and_then(Value::as_i64) {
            if code != 0 {
                is_error = true;
            }
        }
        let stdout = tool
            .raw_output
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or("");
        let stderr = tool
            .raw_output
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or("");
        let combined = [stdout, stderr]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !combined.is_empty() {
            return (combined.trim_end().to_string(), is_error, diff);
        }
    }

    if tool.raw_output.get("isError").and_then(Value::as_bool) == Some(true)
        || tool.raw_output.get("rejected").and_then(Value::as_bool) == Some(true)
        || tool
            .raw_output
            .get("error")
            .is_some_and(|e| !e.is_null() && e.as_str() != Some(""))
    {
        is_error = true;
    }

    if let Some(text) = tool
        .raw_output
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| tool.raw_output.get("error").and_then(Value::as_str))
        .or_else(|| tool.raw_output.as_str())
    {
        return (text.trim_end().to_string(), is_error, diff);
    }

    // Collect text content items if rawOutput was absent.
    let content_texts: Vec<&str> = tool
        .content
        .iter()
        .filter_map(|item| {
            item.pointer("/content/text")
                .or_else(|| item.get("text"))
                .and_then(Value::as_str)
        })
        .collect();
    if !content_texts.is_empty() {
        return (content_texts.join("\n"), is_error, diff);
    }

    if !tool.raw_output.is_null() {
        if let Ok(encoded) = serde_json::to_string_pretty(&tool.raw_output) {
            return (encoded, is_error, diff);
        }
    }

    (String::new(), is_error, diff)
}
