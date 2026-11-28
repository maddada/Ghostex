//! JSON-RPC stdio transport, session runtime state, model/effort config-option resolution,
//! activity reporter, and helper utilities for Native Cursor sessions.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::config_and_store::{read_cursor_acp_context_usage, CursorTransportFailure};
use super::permissions::PendingCursorRequest;
use super::transcript::CursorTranscriptWriter;
use super::CURSOR_RPC_TIMEOUT;
use crate::acp::approval::AcpApprovalLevel;
use crate::acp::host::{now_ms, GxserverEndpoint, SpoolWriter};

/* ----------------------------------------------------- transport & state */

pub(super) struct CursorTransport {
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pub(super) pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    spool: Mutex<SpoolWriter>,
}

impl CursorTransport {
    pub(super) fn new(stdin: ChildStdin, session_id: &str) -> Self {
        Self {
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            spool: Mutex::new(SpoolWriter::open(session_id)),
        }
    }

    pub(super) fn write_json(&self, message: &Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        let Some(stdin) = guard.as_mut() else {
            return Err("Cursor Native host stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Cursor Native host: {e}"))
    }

    pub(super) fn close_stdin(&self) {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        *guard = None;
    }

    pub(super) fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    pub(super) fn start_call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, mpsc::Receiver<Value>), String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(err) = self.write_json(&msg) {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(err);
        }
        Ok((id, rx))
    }

    pub(super) fn finish_call(
        &self,
        id: u64,
        method: &str,
        rx: mpsc::Receiver<Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if let Some(error) = resp.get("error") {
                    Err(super::super::host::extract_jsonrpc_error_message(
                        error,
                        "Cursor Native request failed",
                    ))
                } else {
                    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err(format!("Cursor Native request `{method}` timed out."))
            }
        }
    }

    pub(super) fn call(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let (id, rx) = self.start_call(method, params)?;
        self.finish_call(id, method, rx, timeout)
    }

    pub(super) fn respond_to_agent(&self, rpc_id: Value, result: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result,
        }))
    }

    pub(super) fn respond_error_to_agent(&self, rpc_id: Value, code: i64, message: &str) {
        let _ = self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": code, "message": message },
        }));
    }

    pub(super) fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

pub(super) struct CursorState {
    pub(super) native_session_id: Option<String>,
    pub(super) working: bool,
    pub(super) last_turn_interrupted: bool,
    pub(super) last_turn_stop_error: Option<String>,
    pub(super) turn_generation: u64,
    pub(super) read_only: bool,
    pub(super) approval_level: AcpApprovalLevel,
    pub(super) cwd: PathBuf,
    pub(super) writable_roots: Vec<PathBuf>,
    pub(super) current_model: Option<String>,
    pub(super) current_effort: Option<String>,
    pub(super) current_mode: Option<String>,
    pub(super) available_models: Value,
    pub(super) available_modes: Value,
    pub(super) config_options: Value,
    pub(super) pending_requests: VecDeque<PendingCursorRequest>,
    pub(super) live_stream_text: String,
    pub(super) live_stream_started_at: Option<String>,
    pub(super) live_thought_text: String,
    pub(super) live_thought_started_at: Option<String>,
    pub(super) live_tool_label: Option<String>,
    pub(super) live_tool_detail: Option<String>,
    pub(super) live_tool_started_at: Option<String>,
    pub(super) usage_used_tokens: Option<u64>,
    pub(super) usage_window_size: Option<u64>,
    pub(super) session_title: Option<String>,
    pub(super) last_event_at_ms: i64,
    pub(super) child_pid: u32,
    pub(super) child_exited: bool,
    pub(super) last_auth_error: Option<String>,
    pub(super) last_error: Option<(Option<String>, String)>,
    pub(super) transport_failure: CursorTransportFailure,
    pub(super) transcript: CursorTranscriptWriter,
    pub(super) subagents: super::subagents::CursorSubagents,
}

impl CursorState {
    pub(super) fn new(
        approval_level: AcpApprovalLevel,
        read_only: bool,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
        child_pid: u32,
    ) -> Self {
        Self {
            native_session_id: None,
            working: false,
            last_turn_interrupted: false,
            last_turn_stop_error: None,
            turn_generation: 0,
            read_only,
            approval_level,
            cwd,
            writable_roots,
            current_model: None,
            current_effort: None,
            current_mode: None,
            available_models: Value::Array(Vec::new()),
            available_modes: Value::Array(Vec::new()),
            config_options: Value::Array(Vec::new()),
            pending_requests: VecDeque::new(),
            live_stream_text: String::new(),
            live_stream_started_at: None,
            live_thought_text: String::new(),
            live_thought_started_at: None,
            live_tool_label: None,
            live_tool_detail: None,
            live_tool_started_at: None,
            usage_used_tokens: None,
            usage_window_size: None,
            session_title: None,
            last_event_at_ms: now_ms(),
            child_pid,
            child_exited: false,
            last_auth_error: None,
            last_error: None,
            transport_failure: CursorTransportFailure::default(),
            transcript: CursorTranscriptWriter::new(),
            subagents: Default::default(),
        }
    }

    pub(super) fn clear_live_turn_output(&mut self) {
        self.live_stream_text.clear();
        self.live_stream_started_at = None;
        self.live_thought_text.clear();
        self.live_thought_started_at = None;
        self.live_tool_label = None;
        self.live_tool_detail = None;
        self.live_tool_started_at = None;
        self.transport_failure = CursorTransportFailure::default();
    }

    pub(super) fn live_activity_json(&self) -> Value {
        if !self.working {
            return Value::Null;
        }
        if !self.live_stream_text.trim().is_empty() {
            return json!({
                "kind": "stream",
                "text": self.live_stream_text,
                "detectedAt": self.live_stream_started_at.as_deref().unwrap_or_default(),
            });
        }
        if let Some(label) = self
            .live_tool_label
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            return json!({
                "kind": "tool",
                "label": label,
                "detail": self.live_tool_detail,
                "detectedAt": self.live_tool_started_at.as_deref().unwrap_or_default(),
            });
        }
        if !self.live_thought_text.trim().is_empty() {
            return json!({
                "kind": "thinking",
                "text": self.live_thought_text,
                "detectedAt": self.live_thought_started_at.as_deref().unwrap_or_default(),
            });
        }
        Value::Null
    }

    pub(super) fn context_usage_json(&self) -> Value {
        let Some(used) = self.usage_used_tokens.filter(|u| *u > 0) else {
            return Value::Null;
        };
        let mut obj = Map::new();
        obj.insert("usedTokens".to_string(), json!(used));
        if let Some(size) = self.usage_window_size.filter(|s| *s > 0) {
            obj.insert("windowSize".to_string(), json!(size));
            let pct = ((used as f64 / size as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u32;
            obj.insert("usedPercentage".to_string(), json!(pct));
        }
        Value::Object(obj)
    }

    pub(super) fn refresh_context_usage_from_store(&mut self) {
        let Some(sid) = self.native_session_id.as_deref() else {
            return;
        };
        if let Some((used, max)) = read_cursor_acp_context_usage(sid) {
            self.usage_used_tokens = Some(used);
            if let Some(m) = max {
                self.usage_window_size = Some(m);
            }
        }
    }

    pub(super) fn sync_from_config_options(&mut self, options: &Value) {
        self.config_options = options.clone();
        let Some(arr) = options.as_array() else {
            return;
        };
        let mut effort_val: Option<String> = None;
        for opt in arr {
            let id = opt.get("id").and_then(Value::as_str).unwrap_or_default();
            let cat = opt
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let cur = opt
                .get("currentValue")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if id == "mode" || cat == "mode" {
                if let Some(v) = cur {
                    self.current_mode = Some(v.to_string());
                }
            } else if id == "model" || cat == "model" {
                if let Some(v) = cur {
                    let base = strip_cursor_model_brackets(v);
                    if !base.is_empty() {
                        self.current_model = Some(base.to_string());
                    }
                }
                if let Some(opts) = opt.get("options").and_then(Value::as_array) {
                    let mapped: Vec<Value> = opts
                        .iter()
                        .filter_map(|item| {
                            let value = item.get("value").and_then(Value::as_str)?;
                            let name = item.get("name").and_then(Value::as_str).unwrap_or(value);
                            Some(json!({ "modelId": value, "name": name }))
                        })
                        .collect();
                    if !mapped.is_empty() {
                        self.available_models = Value::Array(mapped);
                    }
                }
            } else if is_effort_config_option(id, cat) {
                if let Some(v) = cur {
                    if let Some(norm) = normalize_cursor_effort_from_acp(v) {
                        effort_val = Some(norm);
                    }
                }
            }
        }
        self.current_effort = effort_val;
    }

    pub(super) fn snapshot_json(&self) -> Value {
        let transcript_path = self.transcript.path_string();
        let last_error = self.last_error.as_ref().map(|(code, msg)| {
            json!({
                "code": code,
                "message": msg,
            })
        });
        json!({
            "family": "cursor",
            "nativeSessionId": self.native_session_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": self.current_mode,
            "nativeAutoActive": false,
            "nativeAutoAvailable": false,
            "availableModels": self.available_models,
            "availableModes": self.available_modes,
            "configOptions": self.config_options,
            "pendingPermission": self.pending_requests.front().map(|p| p.perm.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "sessionTitle": self.session_title,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": transcript_path,
            "childPid": self.child_pid,
            "childExited": self.child_exited,
            "authError": self.last_auth_error,
            "lastError": last_error,
            "agentFleet": self.subagents.fleet_json(),
        })
    }
}

/* ------------------------------------------- model & effort helpers */

pub(crate) fn strip_cursor_model_brackets(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed
        .split_once('[')
        .map_or(trimmed, |(base, _)| base.trim())
}

pub(super) fn is_effort_config_option(id: &str, category: &str) -> bool {
    let id_lower = id.trim().to_ascii_lowercase();
    let cat_lower = category.trim().to_ascii_lowercase();
    cat_lower == "thought_level"
        || matches!(
            id_lower.as_str(),
            "effort" | "reasoning" | "reasoning_effort" | "thought_level"
        )
}

pub(super) fn normalize_cursor_effort_from_acp(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Some("none".to_string()),
        "minimal" => Some("minimal".to_string()),
        "low" => Some("low".to_string()),
        "medium" | "med" => Some("medium".to_string()),
        "high" => Some("high".to_string()),
        "xhigh" | "extra_high" | "extra-high" | "extra high" => Some("xhigh".to_string()),
        "max" => Some("max".to_string()),
        _ => None,
    }
}

pub(super) fn find_cursor_model_option_value(
    config_options: &Value,
    requested: &str,
) -> Option<String> {
    let req = requested.trim();
    if req.is_empty() {
        return None;
    }
    let mapped = if req.eq_ignore_ascii_case("auto") {
        "default"
    } else {
        req
    };
    let options = config_options
        .as_array()?
        .iter()
        .find(|o| o.get("id").and_then(Value::as_str) == Some("model"))?
        .get("options")?
        .as_array()?;

    // 1. Exact value match (including parameterized base or bracketed id).
    for opt in options {
        if let Some(val) = opt.get("value").and_then(Value::as_str) {
            if val.eq_ignore_ascii_case(mapped)
                || strip_cursor_model_brackets(val).eq_ignore_ascii_case(mapped)
            {
                return Some(val.to_string());
            }
        }
    }
    // 2. Strip leading `cursor-` prefix (e.g., catalog `cursor-grok-4.5` -> ACP `grok-4.5`).
    if let Some(stripped) = mapped.strip_prefix("cursor-") {
        for opt in options {
            if let Some(val) = opt.get("value").and_then(Value::as_str) {
                if val.eq_ignore_ascii_case(stripped)
                    || strip_cursor_model_brackets(val).eq_ignore_ascii_case(stripped)
                {
                    return Some(val.to_string());
                }
            }
        }
    }
    // 3. Match display name.
    for opt in options {
        let name = opt.get("name").and_then(Value::as_str).unwrap_or_default();
        if name.eq_ignore_ascii_case(req) {
            if let Some(val) = opt.get("value").and_then(Value::as_str) {
                return Some(val.to_string());
            }
        }
    }
    None
}

pub(super) fn find_cursor_effort_config_target(
    config_options: &Value,
    requested_effort: &str,
) -> Option<(String, String)> {
    let wanted = normalize_cursor_effort_from_acp(requested_effort)?;
    for opt in config_options.as_array()? {
        let id = opt.get("id").and_then(Value::as_str)?;
        let cat = opt.get("category").and_then(Value::as_str).unwrap_or("");
        if !is_effort_config_option(id, cat) {
            continue;
        }
        let values = opt.get("options")?.as_array()?;
        for item in values {
            let val = item.get("value").and_then(Value::as_str)?;
            if normalize_cursor_effort_from_acp(val).as_deref() == Some(wanted.as_str()) {
                return Some((id.to_string(), val.to_string()));
            }
        }
    }
    None
}

pub(super) fn apply_cursor_model_and_effort(
    transport: &CursorTransport,
    state: &Mutex<CursorState>,
    requested_model: Option<&str>,
    requested_effort: Option<&str>,
) -> Result<(), String> {
    let (sid, config_options) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (s.native_session_id.clone(), s.config_options.clone())
    };
    let Some(sid) = sid else {
        return Err("Cursor Native session not initialized yet.".to_string());
    };

    let mut latest_configs = config_options;
    if let Some(raw_model) = requested_model.map(str::trim).filter(|m| !m.is_empty()) {
        let target_val =
            find_cursor_model_option_value(&latest_configs, raw_model).unwrap_or_else(|| {
                if raw_model.eq_ignore_ascii_case("auto") {
                    "default".to_string()
                } else {
                    raw_model.to_string()
                }
            });
        let res = transport.call(
            "session/set_config_option",
            json!({
                "sessionId": sid,
                "configId": "model",
                "value": target_val,
            }),
            CURSOR_RPC_TIMEOUT,
        )?;
        if let Some(configs) = res.get("configOptions") {
            latest_configs = configs.clone();
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.sync_from_config_options(&latest_configs);
        } else {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.current_model = Some(strip_cursor_model_brackets(&target_val).to_string());
        }
    }

    if let Some(raw_effort) = requested_effort.map(str::trim).filter(|e| !e.is_empty()) {
        if let Some((cfg_id, val)) = find_cursor_effort_config_target(&latest_configs, raw_effort) {
            if let Ok(res) = transport.call(
                "session/set_config_option",
                json!({
                    "sessionId": sid,
                    "configId": cfg_id,
                    "value": val,
                }),
                CURSOR_RPC_TIMEOUT,
            ) {
                if let Some(configs) = res.get("configOptions") {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.sync_from_config_options(configs);
                }
            }
        }
    }

    Ok(())
}

/* ----------------------------------------------------- activity reporter */

#[derive(Clone)]
pub(super) struct CursorActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl CursorActivityReporter {
    pub(super) fn start(
        endpoint: Option<GxserverEndpoint>,
        state: Arc<Mutex<CursorState>>,
    ) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                let snapshot = || {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s.pending_requests.front().map(|r| r.perm.clone());
                    let act = if front.is_some() {
                        "attention"
                    } else if s.working {
                        "working"
                    } else {
                        "idle"
                    };
                    (
                        act,
                        s.native_session_id.clone(),
                        s.transcript.path_string(),
                        front,
                        s.last_turn_interrupted,
                        s.last_turn_stop_error.clone(),
                    )
                };
                let (act, sid, path, perm, interrupted, stop_error) = snapshot();
                let mut ok = ep.report_activity_with_error(
                    "cursor",
                    act,
                    sid.as_deref(),
                    path.as_deref(),
                    perm.as_ref(),
                    interrupted,
                    stop_error.as_deref(),
                );
                for delay_ms in [500u64, 1_000, 2_000] {
                    if ok {
                        break;
                    }
                    match rx.recv_timeout(Duration::from_millis(delay_ms)) {
                        Ok(()) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let (act, sid, path, perm, interrupted, stop_error) = snapshot();
                            ok = ep.report_activity_with_error(
                                "cursor",
                                act,
                                sid.as_deref(),
                                path.as_deref(),
                                perm.as_ref(),
                                interrupted,
                                stop_error.as_deref(),
                            );
                        }
                    }
                }
            }
        });
        Self { tx: Some(tx) }
    }

    pub(super) fn sync(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(());
        }
    }
}

pub(super) struct ReaderExitGuard(pub(super) Arc<Mutex<CursorState>>);

impl Drop for ReaderExitGuard {
    fn drop(&mut self) {
        let mut s = self.0.lock().unwrap_or_else(|p| p.into_inner());
        s.child_exited = true;
        s.working = false;
        s.subagents.end_all();
    }
}

pub(super) fn build_prompt_blocks(
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

pub(super) fn stop_child_gracefully(
    transport: &CursorTransport,
    child: &mut Child,
    lifetime: &mut crate::acp::child_lifetime::ChildLifetime,
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
                    crate::acp::child_lifetime::force_kill_child(child);
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

/// Detect `/plan`, `/ask`, or `/agent` slash-mode commands (with optional trailing prompt text).
pub(super) fn parse_cursor_mode_slash(text: &str) -> Option<(&'static str, &str)> {
    let trimmed = text.trim();
    for (prefix, mode_id) in [("/plan", "plan"), ("/ask", "ask"), ("/agent", "agent")] {
        if trimmed.eq_ignore_ascii_case(prefix) {
            return Some((mode_id, ""));
        }
        if let Some(rest) = trimmed
            .strip_prefix(prefix)
            .filter(|r| r.starts_with(char::is_whitespace))
        {
            return Some((mode_id, rest.trim()));
        }
    }
    None
}
