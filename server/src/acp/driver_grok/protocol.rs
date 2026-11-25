//! Grok ACP and xAI extension helpers: IDs, tool normalization, plan detection, skills, and images.

use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{json, Value};

use super::{
    GROK_API_KEY_ENV, GROK_AUTH_METHOD_API_KEY, GROK_AUTH_METHOD_CACHED_TOKEN,
    GROK_SKILLS_CACHE_TTL, GROK_SKILLS_PROBE_TIMEOUT,
};

pub(super) struct CachedGrokSkills {
    pub(super) fetched_at: Instant,
    pub(super) enabled_names: HashSet<String>,
}

pub(super) fn format_compact_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tokens")
    }
}

pub(super) fn non_empty_str(val: Option<&Value>) -> Option<&str> {
    val.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub(super) fn is_valid_grok_reasoning_effort_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 32
        && trimmed
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub(super) fn normalize_grok_reasoning_effort(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    is_valid_grok_reasoning_effort_token(trimmed).then(|| trimmed.to_ascii_lowercase())
}

pub(super) fn resolve_grok_auth_method_id() -> &'static str {
    if env::var(GROK_API_KEY_ENV)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        GROK_AUTH_METHOD_API_KEY
    } else {
        GROK_AUTH_METHOD_CACHED_TOKEN
    }
}

pub(super) fn is_xai_uuid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

pub(super) fn find_uuid_after_prefix(text: &str, prefix_lower: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(pos) = lower[search..].find(prefix_lower) {
        let start = search + pos + prefix_lower.len();
        let rest = text[start..].trim_start();
        let candidate: String = rest
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
            .collect();
        if is_xai_uuid(&candidate) {
            return Some(candidate);
        }
        search = start;
    }
    None
}

pub(super) fn extract_xai_child_session_id(output: &str) -> Option<String> {
    find_uuid_after_prefix(output, "agent id:")
        .or_else(|| find_uuid_after_prefix(output, "subagent_id:"))
        .or_else(|| find_uuid_after_prefix(output, "=== task "))
}

pub(super) fn is_xai_async_spawn_ack(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    if lower.contains("subagent started in background") {
        return true;
    }
    find_uuid_after_prefix(output, "subagent_id:").is_some()
        && lower.contains("get_command_or_subagent_output")
}

pub(super) fn is_xai_monitor_start_ack(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("monitor started") && lower.contains("task ")
}

pub(super) fn decode_byte_text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(s) = value.as_str() {
        let t = s.trim();
        return (!t.is_empty()).then(|| t.to_string());
    }
    let arr = value.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(arr.len());
    for entry in arr {
        let n = entry.as_u64()?;
        if n > 255 {
            return None;
        }
        bytes.push(n as u8);
    }
    let decoded = String::from_utf8_lossy(&bytes).trim().to_string();
    (!decoded.is_empty()).then_some(decoded)
}

pub(super) fn xai_tool_output_text(raw_output: &Value, content: Option<&Value>) -> Option<String> {
    if let Some(obj) = raw_output.as_object() {
        if let Some(direct) = non_empty_str(obj.get("text"))
            .or_else(|| non_empty_str(obj.get("output_for_prompt")))
            .map(str::to_string)
            .or_else(|| decode_byte_text(obj.get("output")))
        {
            return Some(direct);
        }
        if let Some(result) = obj
            .get("Result")
            .or_else(|| obj.get("result"))
            .and_then(Value::as_object)
        {
            if let Some(nested) = non_empty_str(result.get("output"))
                .or_else(|| non_empty_str(result.get("text")))
                .map(str::to_string)
                .or_else(|| decode_byte_text(result.get("output")))
            {
                return Some(nested);
            }
        }
    } else if let Some(s) = raw_output.as_str() {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Some(arr) = content.and_then(Value::as_array) {
        let joined: Vec<&str> = arr
            .iter()
            .filter_map(|entry| {
                entry
                    .pointer("/content/text")
                    .or_else(|| entry.get("text"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })
            .collect();
        if !joined.is_empty() {
            return Some(joined.join("\n"));
        }
    }
    None
}

pub(super) fn is_generic_tool_title(title: Option<&str>) -> bool {
    let key = title.unwrap_or_default().trim().to_ascii_lowercase();
    key.is_empty() || matches!(key.as_str(), "tool" | "tool call" | "terminal")
}

pub(super) fn resolve_xai_tool_title(
    title: Option<&str>,
    meta: Option<&Value>,
    raw_input: &Value,
    raw_output: &Value,
) -> Option<String> {
    // Normalize MCP `use_tool` (`<server>__<tool>`).
    if meta
        .and_then(|m| m.pointer("/x.ai~1tool/name"))
        .and_then(Value::as_str)
        == Some("use_tool")
    {
        if let Some(tool_name) = non_empty_str(raw_input.get("tool_name")) {
            if let Some((server, tool)) = tool_name.split_once("__") {
                if !server.is_empty() && !tool.is_empty() {
                    return Some(format!("{server}:{tool}"));
                }
            }
        }
    }
    if !is_generic_tool_title(title) {
        return title.map(str::trim).map(str::to_string);
    }
    let description = non_empty_str(raw_input.get("description"));
    let variant = non_empty_str(raw_input.get("variant")).map(|v| v.to_ascii_lowercase());
    if let Some(desc) = description {
        if variant.as_deref() == Some("monitor")
            && !desc.to_ascii_lowercase().starts_with("monitor")
        {
            return Some(format!("Monitor: {desc}"));
        }
        return Some(desc.to_string());
    }
    match variant.as_deref() {
        Some("monitor") => return Some("Monitor".to_string()),
        Some("task" | "cursortask" | "spawn_subagent") => return Some("Task".to_string()),
        Some("taskoutput" | "task_output") => return Some("Task output".to_string()),
        Some(other) => return Some(other.to_string()),
        None => {}
    }
    if let Some(cmd) =
        non_empty_str(raw_input.get("command")).or_else(|| non_empty_str(raw_output.get("command")))
    {
        if cmd.chars().count() <= 80 {
            return Some(cmd.to_string());
        }
        let truncated: String = cmd.chars().take(77).collect();
        return Some(format!("{truncated}..."));
    }
    title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(super) fn is_xai_spawn_or_task_tool(title: &str, raw_input: &Value) -> bool {
    let key = title.trim().to_ascii_lowercase();
    if key == "task" || key == "spawn_subagent" || key.contains("spawn subagent") {
        return true;
    }
    if let Some(variant) = non_empty_str(raw_input.get("variant")) {
        if matches!(
            variant.to_ascii_lowercase().as_str(),
            "cursortask" | "task" | "spawn_subagent"
        ) {
            return true;
        }
    }
    non_empty_str(raw_input.get("subagent_type")).is_some()
        || non_empty_str(raw_input.get("subagentType")).is_some()
}

pub(super) fn is_xai_get_output_tool(title: &str, raw_input: &Value, raw_output: &Value) -> bool {
    let key = title.trim().to_ascii_lowercase();
    if key == "get_command_or_subagent_output"
        || key.contains("get_command_or_subagent_output")
        || key == "wait_commands_or_subagents"
    {
        return true;
    }
    if let Some(variant) = non_empty_str(raw_input.get("variant")) {
        if matches!(
            variant.to_ascii_lowercase().as_str(),
            "taskoutput" | "task_output"
        ) {
            return true;
        }
    }
    if let Some(ty) = non_empty_str(raw_output.get("type")) {
        if matches!(
            ty.to_ascii_lowercase().as_str(),
            "taskoutput" | "task_output"
        ) {
            return true;
        }
    }
    key.starts_with("[subagent:") && raw_input.get("task_ids").is_some_and(Value::is_array)
}

pub(super) fn is_xai_monitor_tool(
    title: &str,
    raw_input: &Value,
    raw_output: &Value,
    output: Option<&str>,
) -> bool {
    let key = title.trim().to_ascii_lowercase();
    if key == "monitor" || key.starts_with("monitor ") {
        return true;
    }
    if non_empty_str(raw_input.get("variant")).is_some_and(|v| v.eq_ignore_ascii_case("monitor")) {
        return true;
    }
    if non_empty_str(raw_output.get("type")).is_some_and(|t| t.eq_ignore_ascii_case("monitor")) {
        return true;
    }
    output.is_some_and(is_xai_monitor_start_ack)
}

pub(super) fn xai_background_shell_task_id(raw_output: &Value) -> Option<String> {
    (non_empty_str(raw_output.get("type")) == Some("BackgroundTaskStarted"))
        .then(|| non_empty_str(raw_output.get("task_id")).map(str::to_string))
        .flatten()
}

pub(super) fn extract_xai_monitor_task_id(
    title: &str,
    raw_input: &Value,
    raw_output: &Value,
    output: Option<&str>,
) -> Option<String> {
    if let Some(bg_id) = xai_background_shell_task_id(raw_output) {
        return Some(bg_id);
    }
    if !is_xai_monitor_tool(title, raw_input, raw_output, output) {
        return None;
    }
    if non_empty_str(raw_output.get("type")).is_some_and(|t| t.eq_ignore_ascii_case("monitor")) {
        if let Some(id) = non_empty_str(raw_output.get("taskId"))
            .or_else(|| non_empty_str(raw_output.get("task_id")))
        {
            return Some(id.to_string());
        }
    }
    if let Some(out) = output {
        if let Some(id) = find_uuid_after_prefix(out, "monitor started (task ") {
            return Some(id);
        }
    }
    non_empty_str(raw_input.get("task_id"))
        .or_else(|| non_empty_str(raw_input.get("taskId")))
        .or_else(|| {
            raw_input
                .get("task_ids")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .map(str::to_string)
}

pub(super) fn is_grok_plan_markdown_path(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty()
        || !normalized.ends_with("/plan.md")
        || normalized.split('/').any(|seg| seg == "..")
    {
        return false;
    }
    normalized.contains("/.grok/sessions/") || normalized.contains("/sessions/")
}

pub(super) fn extract_grok_plan_markdown_from_update(update: &Value) -> Option<String> {
    if let Some(raw_input) = update.get("rawInput").and_then(Value::as_object) {
        let file_path = non_empty_str(raw_input.get("file_path"))
            .or_else(|| non_empty_str(raw_input.get("path")));
        if let (Some(path), Some(content)) = (file_path, non_empty_str(raw_input.get("content"))) {
            if is_grok_plan_markdown_path(path) {
                return Some(content.to_string());
            }
        }
    }
    if let Some(content) = update.get("content").and_then(Value::as_array) {
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("diff") {
                continue;
            }
            let path = non_empty_str(block.get("path"));
            let new_text = non_empty_str(block.get("newText"));
            if let (Some(path), Some(text)) = (path, new_text) {
                if is_grok_plan_markdown_path(path) {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}

pub(super) fn extract_leading_skill_mention(text: &str) -> Option<(String, &str)> {
    let trimmed_start = text.trim_start();
    let rest = trimmed_start.strip_prefix('$')?;
    let mut end = 0usize;
    for (idx, ch) in rest.char_indices() {
        if idx == 0 {
            if !ch.is_ascii_alphabetic() {
                return None;
            }
            end = ch.len_utf8();
        } else if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.') {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let after = &rest[end..];
    if !after.is_empty() && !after.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    Some((rest[..end].to_string(), after))
}

pub(super) fn discover_grok_enabled_skills(
    grok_bin: &str,
    cwd: &Path,
    cache: &Mutex<Option<CachedGrokSkills>>,
) -> HashSet<String> {
    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.fetched_at.elapsed() < GROK_SKILLS_CACHE_TTL {
                return cached.enabled_names.clone();
            }
        }
    }
    let mut child = match Command::new(grok_bin)
        .args(["inspect", "--json"])
        .current_dir(cwd)
        .env("GROK_DISABLE_AUTOUPDATER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return HashSet::new(),
    };
    let deadline = Instant::now() + GROK_SKILLS_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return HashSet::new();
                }
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return HashSet::new();
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return HashSet::new(),
        }
    }
    let mut stdout_str = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read as _;
        let _ = out.read_to_string(&mut stdout_str);
    }
    let mut enabled = HashSet::new();
    if let Ok(parsed) = serde_json::from_str::<Value>(&stdout_str) {
        if let Some(skills) = parsed.get("skills").and_then(Value::as_array) {
            for entry in skills {
                if entry.get("userInvocable").and_then(Value::as_bool) == Some(false) {
                    continue;
                }
                if let Some(name) = non_empty_str(entry.get("name")) {
                    enabled.insert(name.to_string());
                }
            }
        }
    }
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(CachedGrokSkills {
            fetched_at: Instant::now(),
            enabled_names: enabled.clone(),
        });
    }
    enabled
}

pub(super) fn rewrite_leading_grok_skill_mention(
    text: &str,
    grok_bin: &str,
    cwd: &Path,
    cache: &Mutex<Option<CachedGrokSkills>>,
) -> String {
    let Some((name, rest)) = extract_leading_skill_mention(text) else {
        return text.to_string();
    };
    let enabled = discover_grok_enabled_skills(grok_bin, cwd, cache);
    if enabled.contains(&name) {
        format!("/{name}{rest}")
    } else {
        text.to_string()
    }
}

pub(super) fn build_grok_prompt_blocks(
    paths: &crate::paths::GxserverPaths,
    text: &str,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut blocks = Vec::new();
    for raw in image_paths {
        let (bytes, mime) = crate::acp::read_validated_acp_image(paths, raw)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        blocks.push(json!({
            "type": "image",
            "data": encoded,
            "mimeType": mime,
        }));
    }
    if !text.trim().is_empty() || blocks.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    Ok(blocks)
}

pub(super) fn build_grok_interject_image_blocks(
    paths: &crate::paths::GxserverPaths,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut blocks = Vec::new();
    for raw in image_paths {
        let (bytes, mime) = crate::acp::read_validated_acp_image(paths, raw)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        blocks.push(json!({
            "type": "image",
            "data": encoded,
            "mimeType": mime,
        }));
    }
    Ok(blocks)
}
