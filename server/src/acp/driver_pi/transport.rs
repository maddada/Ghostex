//! Pi RPC JSONL transport, process lifecycle, and shared protocol helpers.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::state::{PendingPiRequest, PiState};
use super::SpoolWriter;

#[derive(Clone, Debug)]
pub(super) struct HeldPrompt {
    pub(super) text: String,
    pub(super) images: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(super) struct InFlightPrompt {
    pub(super) text: String,
    pub(super) images: Vec<Value>,
}

pub(super) struct PiRpcTransport {
    pub(super) stdin: Mutex<Option<ChildStdin>>,
    pub(super) next_id: AtomicU64,
    pub(super) pending: Mutex<HashMap<String, mpsc::Sender<Value>>>,
    pub(super) in_flight_prompts: Mutex<HashMap<String, InFlightPrompt>>,
    pub(super) spool: Mutex<SpoolWriter>,
}

impl PiRpcTransport {
    pub(super) fn new(stdin: ChildStdin, session_id: &str) -> Self {
        Self {
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            in_flight_prompts: Mutex::new(HashMap::new()),
            spool: Mutex::new(SpoolWriter::open(session_id)),
        }
    }

    pub(super) fn next_request_id(&self) -> String {
        format!("gx-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    pub(super) fn write_json(&self, message: &Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        let Some(stdin) = guard.as_mut() else {
            return Err("Pi RPC stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Pi RPC: {e}"))
    }

    pub(super) fn close_stdin(&self) {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        *guard = None;
    }

    pub(super) fn call(
        &self,
        command: &str,
        extra: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_request_id();
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id.clone(), tx);
        let mut msg = Map::new();
        msg.insert("id".to_string(), json!(id));
        msg.insert("type".to_string(), json!(command));
        if let Value::Object(map) = extra {
            for (k, v) in map {
                msg.insert(k, v);
            }
        }
        if let Err(err) = self.write_json(&Value::Object(msg)) {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(err);
        }
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if resp.get("success").and_then(Value::as_bool) == Some(false) {
                    let message = resp
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("Pi RPC command failed");
                    Err(message.to_string())
                } else {
                    Ok(resp.get("data").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err(format!("Pi RPC `{command}` timed out."))
            }
        }
    }

    pub(super) fn send_prompt_async(
        &self,
        text: &str,
        images: Vec<Value>,
        streaming_behavior: Option<&str>,
    ) -> Result<String, String> {
        let id = self.next_request_id();
        let mut msg = Map::new();
        msg.insert("id".to_string(), json!(id));
        msg.insert("type".to_string(), json!("prompt"));
        msg.insert("message".to_string(), json!(text));
        if let Some(behavior) = streaming_behavior {
            msg.insert("streamingBehavior".to_string(), json!(behavior));
        }
        if !images.is_empty() {
            msg.insert("images".to_string(), Value::Array(images.clone()));
        }
        self.in_flight_prompts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                id.clone(),
                InFlightPrompt {
                    text: text.to_string(),
                    images,
                },
            );
        if let Err(err) = self.write_json(&Value::Object(msg)) {
            self.in_flight_prompts
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(err);
        }
        Ok(id)
    }

    pub(super) fn send_compact_async(
        &self,
        custom_instructions: Option<&str>,
    ) -> Result<String, String> {
        let id = self.next_request_id();
        let mut msg = Map::new();
        msg.insert("id".to_string(), json!(id));
        msg.insert("type".to_string(), json!("compact"));
        if let Some(instr) = custom_instructions.map(str::trim).filter(|s| !s.is_empty()) {
            msg.insert("customInstructions".to_string(), json!(instr));
        }
        self.write_json(&Value::Object(msg))?;
        Ok(id)
    }

    pub(super) fn respond_extension_ui(
        &self,
        request_id: &str,
        fields: Value,
    ) -> Result<(), String> {
        let mut msg = Map::new();
        msg.insert("type".to_string(), json!("extension_ui_response"));
        msg.insert("id".to_string(), json!(request_id));
        if let Value::Object(map) = fields {
            for (k, v) in map {
                msg.insert(k, v);
            }
        }
        self.write_json(&Value::Object(msg))
    }

    pub(super) fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

pub(super) fn pi_tool_acp_kind(tool_name: &str) -> &'static str {
    match tool_name.trim().to_ascii_lowercase().as_str() {
        "read" | "grep" | "find" | "ls" | "tool_search" => "read",
        "edit" | "write" => "edit",
        "bash" | "shell" | "exec" | "command" => "execute",
        "web_fetch" | "fetch" | "web_search" => "fetch",
        _ => "other",
    }
}

pub(super) fn summarize_pi_tool_input(tool_name: &str, input: &Value) -> String {
    for key in [
        "command",
        "CommandLine",
        "path",
        "file_path",
        "filePath",
        "pattern",
        "query",
        "url",
    ] {
        if let Some(val) = input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return val.to_string();
        }
    }
    if input.is_object() && !input.as_object().is_some_and(Map::is_empty) {
        if let Ok(encoded) = serde_json::to_string(input) {
            return encoded.chars().take(240).collect();
        }
    }
    tool_name.to_string()
}

pub(super) fn parse_pi_compact_command(text: &str) -> Option<Option<String>> {
    let trimmed = text.trim();
    if trimmed == "/compact" {
        return Some(None);
    }
    let rest = trimmed.strip_prefix("/compact")?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    let instructions = rest.trim();
    if instructions.is_empty() {
        Some(None)
    } else {
        Some(Some(instructions.to_string()))
    }
}

pub(super) fn normalize_pi_thinking_level(raw: &str) -> Option<String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "" => None,
        "none" | "off" => Some("off".to_string()),
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max" => Some(lower),
        _ => None,
    }
}

pub(super) fn read_pi_streaming_behavior() -> &'static str {
    let paths = crate::paths::get_gxserver_paths(None);
    let setting = crate::shared_settings_file::read_shared_settings(&paths).and_then(|value| {
        value
            .get("piMessagesWhileWorking")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_string)
    });
    if setting.as_deref() == Some("followUp") {
        "followUp"
    } else {
        "steer"
    }
}

pub(super) fn extract_usage_tokens(usage: &Value) -> Option<u64> {
    if let Some(total) = usage
        .get("totalTokens")
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
    {
        return Some(total);
    }
    let input = usage.get("input").and_then(Value::as_u64).unwrap_or(0);
    let output = usage.get("output").and_then(Value::as_u64).unwrap_or(0);
    let cache_read = usage.get("cacheRead").and_then(Value::as_u64).unwrap_or(0);
    let cache_write = usage.get("cacheWrite").and_then(Value::as_u64).unwrap_or(0);
    let sum = input
        .saturating_add(output)
        .saturating_add(cache_read)
        .saturating_add(cache_write);
    (sum > 0).then_some(sum)
}

pub(super) fn cancel_all_pending_requests(transport: &PiRpcTransport, state: &Mutex<PiState>) {
    let drained: Vec<PendingPiRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_rpiv_custom_input = None;
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        let _ = transport.respond_extension_ui(&req.request_id, json!({ "cancelled": true }));
    }
}

pub(super) fn stop_pi_child_gracefully(
    transport: &PiRpcTransport,
    child: &mut Child,
    lifetime: &mut super::super::child_lifetime::ChildLifetime,
    child_pid: &AtomicU32,
) {
    transport.close_stdin();
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                child_pid.store(0, Ordering::Release);
                lifetime.cleanup();
                return;
            }
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    super::super::child_lifetime::force_kill_child(child);
                    child_pid.store(0, Ordering::Release);
                    lifetime.cleanup();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                child_pid.store(0, Ordering::Release);
                lifetime.cleanup();
                return;
            }
        }
    }
}

pub(super) fn encode_pi_images(
    paths: &crate::paths::GxserverPaths,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut images = Vec::with_capacity(image_paths.len());
    for raw in image_paths {
        let (bytes, mime) = super::super::read_validated_acp_image(paths, raw)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        images.push(json!({
            "type": "image",
            "data": encoded,
            "mimeType": mime,
        }));
    }
    Ok(images)
}

pub(super) fn resolve_pi_launch_session_args(resume_id: Option<&str>) -> (Vec<String>, bool) {
    let Some(raw) = resume_id.map(str::trim).filter(|s| !s.is_empty()) else {
        return (Vec::new(), false);
    };
    let as_path = Path::new(raw);
    if as_path.is_file() {
        return (vec!["--session".to_string(), raw.to_string()], true);
    }
    if let Some(existing) = crate::session_chat_paths::resolve_session_chat_transcript_path(
        crate::session_chat::SessionChatTranscriptAgent::Pi,
        Some(raw),
        None,
    ) {
        if existing.is_file() {
            return (
                vec![
                    "--session".to_string(),
                    existing.to_string_lossy().to_string(),
                ],
                true,
            );
        }
    }
    if crate::agents::pi_session_id::is_pi_session_id(raw) {
        return (vec!["--session-id".to_string(), raw.to_string()], false);
    }
    (vec!["--session".to_string(), raw.to_string()], true)
}

pub(super) fn spawn_pi_process(
    prog: &str,
    base_argv: &[String],
    acp_envs: &[(String, String)],
    extension_path: &Path,
    session_args: &[String],
    cwd: &str,
    record_target: Option<&super::super::child_lifetime::ChildRecordTarget>,
) -> Result<(Child, super::super::child_lifetime::ChildLifetime), String> {
    let mut argv = base_argv.to_vec();
    argv.push("--extension".to_string());
    argv.push(extension_path.to_string_lossy().to_string());
    argv.extend(session_args.iter().cloned());
    let child = super::super::child_lifetime::end_with_host(&mut Command::new(prog))
        .env_remove(super::super::channel::ACP_CHANNEL_SECRET_ENV)
        // Disable Terminal-based Ghostex Pi hooks and chat channel inside the Native RPC child
        // so only `ghostex acp-host` owns `/api/agentChatChannel` and hook reporting.
        .env("GHOSTEX_PI_HOOKS_DISABLED", "1")
        .env("GHOSTEX_CHAT_CHANNEL_DISABLED", "1")
        .envs(acp_envs.iter().cloned())
        .args(&argv)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to start `{prog} {}`: {err}", argv.join(" ")))?;
    let lifetime = super::super::child_lifetime::ChildLifetime::attach(&child, record_target);
    Ok((child, lifetime))
}
