//! Bidirectional `claude --output-format stream-json --input-format stream-json --permission-prompt-tool=stdio`
//! driver for `ghostex acp-host --agent claude`.
//!
//! Runs Claude Code natively over NDJSON stdio with zero Node/Bun wrapper dependency, keeping
//! `cswap run <N> --share-history -- ...` wrappers intact, enforcing Ghostex's four live approval
//! levels (`Ask`, `Auto-edits`, `Auto`, `Full access`) with one-time `allow` decisions (never
//! sending `updatedPermissions`), handling `AskUserQuestion` and `ExitPlanMode` interactively,
//! streaming text/thinking/tool progress and context usage, surviving `gxserver` reloads inside
//! `zmx`, and supporting in-place Switch to Terminal (`claude --resume <id>`).

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::approval::{
    canonicalize_writable_roots, evaluate_permission, AcpApprovalLevel, AcpPermissionOption,
    PermissionDecision,
};
use super::host::{
    append_bounded, exec_interactive_cli, now_iso_rfc3339, now_ms, GxserverEndpoint, HostArgs,
    PendingPermission, SpoolWriter, TerminationSignalGuard, LIVE_STREAM_MAX_CHARS,
    POLL_RETRY_DELAY,
};
use super::spec::{
    build_acp_server_argv, build_ghostex_claude_settings_json, transcript_path_for_session,
    validated_ghostex_prompt_file, AcpAgentSpec,
};

const CLAUDE_CTRL_TIMEOUT: Duration = Duration::from_secs(30);

struct ClaudeTransport {
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<String, mpsc::Sender<Value>>>,
    spool: Mutex<SpoolWriter>,
}

impl ClaudeTransport {
    fn new(stdin: ChildStdin, session_id: &str) -> Self {
        Self {
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            spool: Mutex::new(SpoolWriter::open(session_id)),
        }
    }

    fn write_json(&self, message: &Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        let Some(stdin) = guard.as_mut() else {
            return Err("Claude stdio stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Claude: {e}"))
    }

    fn close_stdin(&self) {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        *guard = None;
    }

    fn control_request(&self, request: Value, timeout: Duration) -> Result<Value, String> {
        let req_id = format!("gx-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(req_id.clone(), tx);
        let msg = json!({
            "type": "control_request",
            "request_id": req_id,
            "request": request,
        });
        if let Err(err) = self.write_json(&msg) {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&req_id);
            return Err(err);
        }
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                let inner = resp.get("response").unwrap_or(&Value::Null);
                if inner.get("subtype").and_then(Value::as_str) == Some("error") {
                    let message = inner
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("Claude control request failed");
                    Err(message.to_string())
                } else {
                    Ok(inner.get("response").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&req_id);
                Err("Claude control request timed out.".to_string())
            }
        }
    }

    fn control_respond_allow(
        &self,
        request_id: &str,
        updated_input: Value,
        tool_use_id: Option<&str>,
    ) -> Result<(), String> {
        let mut body = Map::new();
        body.insert("behavior".to_string(), json!("allow"));
        body.insert("updatedInput".to_string(), updated_input);
        if let Some(id) = tool_use_id.filter(|s| !s.is_empty()) {
            body.insert("toolUseID".to_string(), json!(id));
        }
        // Never send `updatedPermissions` so auto-approvals stay allow-once and nothing is saved
        // inside Claude's settings.
        let msg = json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request_id,
                "response": Value::Object(body),
            }
        });
        self.write_json(&msg)
    }

    fn control_respond_deny(
        &self,
        request_id: &str,
        message: &str,
        interrupt: bool,
        tool_use_id: Option<&str>,
    ) -> Result<(), String> {
        let mut body = Map::new();
        body.insert("behavior".to_string(), json!("deny"));
        body.insert("message".to_string(), json!(message));
        if interrupt {
            body.insert("interrupt".to_string(), json!(true));
        }
        if let Some(id) = tool_use_id.filter(|s| !s.is_empty()) {
            body.insert("toolUseID".to_string(), json!(id));
        }
        let msg = json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request_id,
                "response": Value::Object(body),
            }
        });
        self.write_json(&msg)
    }

    fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

#[derive(Clone, Debug)]
struct PendingClaudeRequest {
    request_id: String,
    tool_name: String,
    summary: String,
    tool_use_id: Option<String>,
    raw_input: Value,
    reason: Option<String>,
    is_question: bool,
}

impl PendingClaudeRequest {
    fn to_ghostex_prompt(&self) -> Value {
        if self.is_question {
            let questions = self
                .raw_input
                .get("questions")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|q| {
                            let options = q
                                .get("options")
                                .and_then(Value::as_array)
                                .map(|opts| {
                                    opts.iter()
                                        .filter_map(|opt| match opt {
                                            Value::String(s) => Some(json!({ "label": s })),
                                            Value::Object(obj) => {
                                                let label = obj
                                                    .get("label")
                                                    .and_then(Value::as_str)
                                                    .unwrap_or_default();
                                                if label.is_empty() {
                                                    return None;
                                                }
                                                let mut item = json!({ "label": label });
                                                if let Some(desc) =
                                                    obj.get("description").and_then(Value::as_str)
                                                {
                                                    item["description"] = json!(desc);
                                                }
                                                Some(item)
                                            }
                                            _ => None,
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            json!({
                                "question": q.get("question").and_then(Value::as_str).unwrap_or("Choose an option."),
                                "header": q.get("header").and_then(Value::as_str).unwrap_or("Question"),
                                "multiSelect": q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
                                "allowCustom": true,
                                "options": options,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            return json!({
                "kind": "question",
                "toolUseId": self.tool_use_id,
                "questions": questions,
            });
        }
        let mut prompt = json!({
            "kind": "approval",
            "tool": self.tool_name,
            "summary": self.summary,
            "toolUseId": self.tool_use_id,
            "options": [
                { "optionId": "allow_once", "label": "Allow", "kind": "allow_once" },
                { "optionId": "reject_once", "label": "Deny", "kind": "reject_once" }
            ],
        });
        if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
            prompt["reason"] = json!(reason);
        }
        prompt
    }

    fn to_activity_permission(&self) -> PendingPermission {
        PendingPermission {
            rpc_id: Value::String(self.request_id.clone()),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            tool_use_id: self.tool_use_id.clone(),
            reason: self.reason.clone(),
            is_question: self.is_question,
            options: vec![
                AcpPermissionOption {
                    option_id: "allow_once".to_string(),
                    name: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                },
                AcpPermissionOption {
                    option_id: "reject_once".to_string(),
                    name: "Deny".to_string(),
                    kind: "reject_once".to_string(),
                },
            ],
            custom_prompt_override: Some(self.to_ghostex_prompt()),
        }
    }
}

struct ClaudeState {
    native_session_id: Option<String>,
    working: bool,
    last_turn_interrupted: bool,
    last_turn_stop_error: Option<String>,
    last_auth_error: Option<String>,
    last_error: Option<(Option<String>, String)>,
    compacting: bool,
    compacting_started_at: Option<String>,
    active_turn_user_msg: Option<Value>,
    active_turn_auth_retried: bool,
    turn_saw_auth_error: Option<String>,
    turn_saw_api_error: Option<(Option<String>, String)>,
    pending_auth_retry: Option<Value>,
    approval_level: AcpApprovalLevel,
    read_only: bool,
    native_mode: String,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    available_models: Value,
    pending_requests: VecDeque<PendingClaudeRequest>,
    live_stream_text: String,
    live_stream_started_at: Option<String>,
    live_thought_text: String,
    live_thought_started_at: Option<String>,
    live_tool_label: Option<String>,
    live_tool_detail: Option<String>,
    live_tool_started_at: Option<String>,
    usage_used_tokens: Option<u64>,
    usage_window_size: Option<u64>,
    last_event_at_ms: i64,
    child_pid: u32,
    child_exited: bool,
}

impl ClaudeState {
    fn new(
        approval_level: AcpApprovalLevel,
        read_only: bool,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
        initial_model: Option<String>,
        initial_effort: Option<String>,
    ) -> Self {
        Self {
            native_session_id: None,
            working: false,
            last_turn_interrupted: false,
            last_turn_stop_error: None,
            last_auth_error: None,
            last_error: None,
            compacting: false,
            compacting_started_at: None,
            active_turn_user_msg: None,
            active_turn_auth_retried: false,
            turn_saw_auth_error: None,
            turn_saw_api_error: None,
            pending_auth_retry: None,
            approval_level,
            read_only,
            native_mode: if read_only { "plan" } else { "default" }.to_string(),
            cwd,
            writable_roots,
            current_model: initial_model,
            current_effort: initial_effort,
            available_models: Value::Array(Vec::new()),
            pending_requests: VecDeque::new(),
            live_stream_text: String::new(),
            live_stream_started_at: None,
            live_thought_text: String::new(),
            live_thought_started_at: None,
            live_tool_label: None,
            live_tool_detail: None,
            live_tool_started_at: None,
            usage_used_tokens: None,
            usage_window_size: Some(200_000),
            last_event_at_ms: now_ms(),
            child_pid: 0,
            child_exited: false,
        }
    }

    fn clear_live_turn_output(&mut self) {
        self.live_stream_text.clear();
        self.live_stream_started_at = None;
        self.live_thought_text.clear();
        self.live_thought_started_at = None;
        self.live_tool_label = None;
        self.live_tool_detail = None;
        self.live_tool_started_at = None;
    }

    fn live_activity_json(&self) -> Value {
        if self.compacting {
            return json!({
                "kind": "compacting",
                "label": "Compacting conversation",
                "detectedAt": self.compacting_started_at.as_deref().unwrap_or_default(),
            });
        }
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

    fn context_usage_json(&self) -> Value {
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

    fn snapshot_json(&self) -> Value {
        let transcript_path = self
            .native_session_id
            .as_deref()
            .and_then(|id| transcript_path_for_session("claude", id))
            .map(|p| p.to_string_lossy().to_string());
        let last_error = self.last_error.as_ref().map(|(code, msg)| {
            json!({
                "code": code,
                "message": msg,
            })
        });
        json!({
            "family": "claude",
            "nativeSessionId": self.native_session_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": self.native_mode,
            "nativeAutoActive": self.native_mode == "auto",
            "nativeAutoAvailable": true,
            "availableModels": self.available_models,
            "availableModes": [
                { "id": "default", "name": "Ask" },
                { "id": "auto", "name": "Auto" }
            ],
            "configOptions": [],
            "pendingPermission": self.pending_requests.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": transcript_path,
            "childPid": self.child_pid,
            "childExited": self.child_exited,
            "authError": self.last_auth_error,
            "lastError": last_error,
        })
    }
}

#[derive(Clone)]
struct ClaudeActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl ClaudeActivityReporter {
    fn start(endpoint: Option<GxserverEndpoint>, state: Arc<Mutex<ClaudeState>>) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                let (act, sid, perm, interrupted, stop_error) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s
                        .pending_requests
                        .front()
                        .map(|p| p.to_activity_permission());
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
                        front,
                        s.last_turn_interrupted,
                        s.last_turn_stop_error.clone(),
                    )
                };
                ep.report_activity_with_error(
                    "claude",
                    act,
                    sid.as_deref(),
                    None,
                    perm.as_ref(),
                    interrupted,
                    stop_error.as_deref(),
                );
            }
        });
        Self { tx: Some(tx) }
    }

    fn sync(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(());
        }
    }
}

struct ClaudeExitGuard {
    state: Arc<Mutex<ClaudeState>>,
    process_gen: Arc<AtomicU64>,
    my_gen: u64,
}
impl Drop for ClaudeExitGuard {
    fn drop(&mut self) {
        if self.process_gen.load(Ordering::Acquire) != self.my_gen {
            return;
        }
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.child_exited = true;
        s.working = false;
    }
}

struct ClaudeProcess {
    child: Child,
    transport: Arc<ClaudeTransport>,
}

fn extract_claude_message_text(msg: &Value) -> String {
    if let Some(text) = msg.get("result").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return text.trim().to_string();
        }
    }
    match msg.pointer("/message/content") {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn is_claude_auth_error(error_code: Option<&str>, text: &str) -> bool {
    if error_code.is_some_and(|c| c.eq_ignore_ascii_case("authentication_failed")) {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    lower.contains("failed to authenticate")
        || lower.contains("oauth session expired")
        || lower.contains("oauth token revoked")
        || lower.contains("please run /login")
        || (lower.contains("401") && lower.contains("authentication_error"))
}

fn claude_tool_kind(tool_name: &str) -> &'static str {
    match tool_name {
        "Read" | "Glob" | "Grep" | "LS" | "TaskList" | "TaskGet" | "TodoRead" => "read",
        "WebSearch" | "WebFetch" => "fetch",
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "edit",
        _ => "execute",
    }
}

fn claude_tool_summary(tool_name: &str, input: &Value) -> String {
    if tool_name == "ExitPlanMode" {
        return "Approve plan and start implementing".to_string();
    }
    [
        "command",
        "file_path",
        "path",
        "notebook_path",
        "pattern",
        "url",
        "query",
        "description",
    ]
    .iter()
    .find_map(|k| input.get(*k).and_then(Value::as_str))
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_string)
    .unwrap_or_else(|| tool_name.to_string())
}

fn synthetic_acp_params_for_claude(tool_name: &str, input: &Value) -> Value {
    let kind = claude_tool_kind(tool_name);
    let mut locations = Vec::new();
    for key in ["file_path", "path", "notebook_path"] {
        if let Some(path) = input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            locations.push(json!({ "path": path }));
        }
    }
    json!({
        "toolCall": {
            "name": tool_name,
            "kind": kind,
            "rawInput": input,
            "locations": locations,
        }
    })
}

fn build_ask_user_answers(raw_input: &Value, selections: &[Value]) -> Option<Value> {
    let questions = raw_input.get("questions").and_then(Value::as_array)?;
    let mut answers = Map::new();
    for (idx, q) in questions.iter().enumerate() {
        let q_text = q
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if q_text.is_empty() {
            continue;
        }
        let Some(sel) = selections.get(idx) else {
            continue;
        };
        let mut chosen = Vec::new();
        let options = q.get("options").and_then(Value::as_array);
        if let Some(indices) = sel.get("indices").and_then(Value::as_array) {
            for i in indices.iter().filter_map(Value::as_u64) {
                if let Some(label) = options.and_then(|arr| arr.get(i as usize)).and_then(|opt| {
                    opt.as_str()
                        .or_else(|| opt.get("label").and_then(Value::as_str))
                }) {
                    chosen.push(label.to_string());
                }
            }
        }
        if let Some(other) = sel
            .get("other")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            chosen.push(other.to_string());
        }
        if !chosen.is_empty() {
            answers.insert(q_text.to_string(), json!(chosen.join(", ")));
        }
    }
    if answers.is_empty() {
        return None;
    }
    let mut updated = raw_input.as_object().cloned().unwrap_or_default();
    updated.insert("answers".to_string(), Value::Object(answers));
    Some(Value::Object(updated))
}

fn build_claude_user_content(
    paths: &crate::paths::GxserverPaths,
    text: &str,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut content = Vec::new();
    for raw in image_paths {
        let (bytes, mime) = super::read_validated_acp_image(paths, raw)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": mime,
                "data": encoded,
            }
        }));
    }
    if !text.trim().is_empty() || content.is_empty() {
        content.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    Ok(content)
}

fn apply_claude_approval_level(
    transport: &ClaudeTransport,
    state: &Mutex<ClaudeState>,
    mode_sync_lock: &Mutex<()>,
    target_level: AcpApprovalLevel,
    leave_read_only: bool,
) -> Result<(), String> {
    let _mode_guard = mode_sync_lock.lock().unwrap_or_else(|p| p.into_inner());
    // Keep Claude in `"plan"` while a `--read-only` thread stays at `Ask`, in `"default"` for
    // Ask / Auto-edits / Full access so `can_use_tool` always routes through Ghostex's allow-once
    // policy (`acp/approval.rs`) and AskUserQuestion / ExitPlanMode always prompt the user, and
    // in Claude's native `"auto"` mode only for `Auto`.
    let (current_native, effective_read_only) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let ro = if leave_read_only || target_level != AcpApprovalLevel::Ask {
            false
        } else {
            s.read_only
        };
        (s.native_mode.clone(), ro)
    };
    let desired_native = match target_level {
        AcpApprovalLevel::Auto => "auto",
        AcpApprovalLevel::Ask if effective_read_only => "plan",
        AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits | AcpApprovalLevel::FullAccess => {
            "default"
        }
    };
    if current_native == desired_native {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.approval_level = target_level;
        s.read_only = effective_read_only;
        return Ok(());
    }
    match transport.control_request(
        json!({
            "subtype": "set_permission_mode",
            "mode": desired_native,
        }),
        CLAUDE_CTRL_TIMEOUT,
    ) {
        Ok(_) => {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.approval_level = target_level;
            s.read_only = effective_read_only;
            s.native_mode = desired_native.to_string();
            Ok(())
        }
        Err(err) => {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if current_native == "auto" && desired_native != "auto" {
                return Err(format!("Could not leave Claude's automatic mode: {err}"));
            }
            // Entering native Auto failed (e.g. account/model doesn't support Auto): stay safely
            // on `"default"` (Ask-equivalent) while recording the requested level.
            s.approval_level = target_level;
            s.read_only = effective_read_only;
            Ok(())
        }
    }
}

fn cancel_all_pending_claude(transport: &ClaudeTransport, state: &Mutex<ClaudeState>) {
    let drained: Vec<PendingClaudeRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        let _ = transport.control_respond_deny(
            &req.request_id,
            "Cancelled by user.",
            false,
            req.tool_use_id.as_deref(),
        );
    }
}

fn stop_claude_gracefully(transport: &ClaudeTransport, child: &mut Child, child_pid: &AtomicU32) {
    transport.close_stdin();
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                child_pid.store(0, Ordering::Release);
                return;
            }
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    child_pid.store(0, Ordering::Release);
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                child_pid.store(0, Ordering::Release);
                return;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_and_init_claude(
    spec: &AcpAgentSpec,
    parsed: &HostArgs,
    cwd: &str,
    spool_session_id: &str,
    resume_session_id: Option<&str>,
    fork_session: bool,
    model: Option<&str>,
    effort: Option<&str>,
    read_only: bool,
    approval_level: AcpApprovalLevel,
    state: &Arc<Mutex<ClaudeState>>,
    activity: &ClaudeActivityReporter,
    mode_sync_lock: &Arc<Mutex<()>>,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<ClaudeTransport>>>>,
    process_gen: &Arc<AtomicU64>,
    my_gen: u64,
) -> Result<ClaudeProcess, String> {
    let Some((acp_envs, prog, mut argv)) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
    else {
        return Err("Invalid Claude Native command.".to_string());
    };
    if read_only {
        // `--read-only` threads start Claude in its native `plan` mode, matching CLI `--read-only`.
        for idx in 0..argv.len() {
            if argv[idx] == "--permission-mode" && idx + 1 < argv.len() {
                argv[idx + 1] = "plan".to_string();
            }
        }
    }
    // Synthesize Ghostex's own `--settings` JSON strictly from server-generated fields (never
    // from untrusted command strings, never containing hooks, command helpers, or allow rules).
    if let Some(settings_json) =
        build_ghostex_claude_settings_json(&parsed.profile, Some(cwd), effort, false)
    {
        argv.push("--settings".to_string());
        argv.push(settings_json);
    }
    if let Some(prompt_file) =
        validated_ghostex_prompt_file(parsed.profile.append_system_prompt_file.as_deref())
    {
        argv.push("--append-system-prompt-file".to_string());
        argv.push(prompt_file);
    }
    if let Some(resume_id) = resume_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        argv.push("--resume".to_string());
        argv.push(resume_id.to_string());
        if fork_session {
            argv.push("--fork-session".to_string());
        }
    }
    if let Some(model) = model
        .map(str::trim)
        .filter(|s| !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(s))
    {
        argv.push("--model".to_string());
        argv.push(model.to_string());
    }
    if let Some(effort) = effort.map(str::trim).filter(|s| !s.is_empty()) {
        argv.push("--effort".to_string());
        argv.push(effort.to_string());
    }

    let mut child = Command::new(&prog)
        .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .env_remove("CLAUDECODE")
        .envs(acp_envs)
        .args(&argv)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to start `{prog} {}`: {err}", argv.join(" ")))?;
    child_pid.store(child.id(), Ordering::Release);
    state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = child.id();
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        child_pid.store(0, Ordering::Release);
        state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
        return Err("Claude child stdin unavailable.".to_string());
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        child_pid.store(0, Ordering::Release);
        state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
        return Err("Claude child stdout unavailable.".to_string());
    };
    let stderr = child.stderr.take();
    let transport = Arc::new(ClaudeTransport::new(stdin, spool_session_id));
    *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = Some(transport.clone());

    if let Some(stderr) = stderr {
        let t_err = transport.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    t_err.spool_event(&json!({ "stderr": trimmed }));
                }
            }
        });
    }

    // Stdout NDJSON reader loop scoped to `my_gen`.
    {
        let t_out = transport.clone();
        let st_out = state.clone();
        let act_out = activity.clone();
        let mode_lock_out = mode_sync_lock.clone();
        let gen_out = process_gen.clone();
        thread::spawn(move || {
            let _exit_guard = ClaudeExitGuard {
                state: st_out.clone(),
                process_gen: gen_out.clone(),
                my_gen,
            };
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if gen_out.load(Ordering::Acquire) != my_gen {
                    break;
                }
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                        return;
                    };
                    t_out.spool_event(&msg);
                    if gen_out.load(Ordering::Acquire) != my_gen {
                        return;
                    }
                    {
                        let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                        s.last_event_at_ms = now_ms();
                        if let Some(sid) = msg
                            .get("session_id")
                            .and_then(Value::as_str)
                            .filter(|v| !v.is_empty())
                        {
                            s.native_session_id = Some(sid.to_string());
                        }
                    }

                    let msg_type = msg.get("type").and_then(Value::as_str).unwrap_or_default();
                    match msg_type {
                        "control_response" => {
                            if let Some(req_id) =
                                msg.pointer("/response/request_id").and_then(Value::as_str)
                            {
                                if let Some(tx) = t_out
                                    .pending
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .remove(req_id)
                                {
                                    let _ = tx.send(msg);
                                }
                            }
                        }
                        "control_request" => {
                            let req_id = msg
                                .get("request_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let req = msg.get("request").unwrap_or(&Value::Null);
                            let subtype = req
                                .get("subtype")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if subtype == "can_use_tool" {
                                let tool_name = req
                                    .get("tool_name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Tool")
                                    .to_string();
                                let input = req.get("input").cloned().unwrap_or(json!({}));
                                let tool_use_id = req
                                    .get("tool_use_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                let is_question = tool_name == "AskUserQuestion";
                                let is_plan_exit = tool_name == "ExitPlanMode";
                                let summary = claude_tool_summary(&tool_name, &input);

                                let mut reason = None;
                                if !is_question && !is_plan_exit {
                                    let (level, roots, cwd) = {
                                        let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                        (s.approval_level, s.writable_roots.clone(), s.cwd.clone())
                                    };
                                    let synthetic =
                                        synthetic_acp_params_for_claude(&tool_name, &input);
                                    let opts = vec![
                                        AcpPermissionOption {
                                            option_id: "allow_once".to_string(),
                                            name: "Allow".to_string(),
                                            kind: "allow_once".to_string(),
                                        },
                                        AcpPermissionOption {
                                            option_id: "reject_once".to_string(),
                                            name: "Deny".to_string(),
                                            kind: "reject_once".to_string(),
                                        },
                                    ];
                                    match evaluate_permission(
                                        level, &synthetic, &opts, &roots, &cwd,
                                    ) {
                                        PermissionDecision::AutoApprove { .. } => {
                                            println!(
                                                "\r\n[Claude Native] Auto-approved ({}): {} ({})",
                                                level.label(),
                                                tool_name,
                                                summary
                                            );
                                            let _ = std::io::stdout().flush();
                                            let _ = t_out.control_respond_allow(
                                                &req_id,
                                                input,
                                                tool_use_id.as_deref(),
                                            );
                                            return;
                                        }
                                        PermissionDecision::AskUser { reason: r } => {
                                            reason = r;
                                        }
                                    }
                                }
                                println!(
                                    "\r\n[Claude Native] Waiting for {}: {} ({})",
                                    if is_question {
                                        "question answer"
                                    } else {
                                        "approval"
                                    },
                                    tool_name,
                                    summary
                                );
                                let _ = std::io::stdout().flush();
                                let pending = PendingClaudeRequest {
                                    request_id: req_id,
                                    tool_name,
                                    summary,
                                    tool_use_id,
                                    raw_input: input,
                                    reason,
                                    is_question,
                                };
                                let was_empty = {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    let empty = s.pending_requests.is_empty();
                                    s.pending_requests.push_back(pending);
                                    empty
                                };
                                if was_empty {
                                    act_out.sync();
                                }
                            } else {
                                let _ = t_out.write_json(&json!({
                                    "type": "control_response",
                                    "response": {
                                        "subtype": "error",
                                        "request_id": req_id,
                                        "error": format!("Unsupported control request: {subtype}"),
                                    }
                                }));
                            }
                        }
                        "system" => {
                            let subtype = msg.get("subtype").and_then(Value::as_str);
                            if subtype == Some("init") {
                                let _mode_guard =
                                    mode_lock_out.lock().unwrap_or_else(|p| p.into_inner());
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if let Some(model) = msg
                                    .get("model")
                                    .and_then(Value::as_str)
                                    .filter(|m| {
                                        !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
                                    })
                                {
                                    let preserved =
                                        crate::agent_model_catalog::preserve_claude_1m_context(
                                            s.current_model.as_deref(),
                                            model,
                                            s.usage_window_size,
                                        );
                                    s.current_model = Some(preserved);
                                }
                                if let Some(pm) = msg
                                    .get("permissionMode")
                                    .and_then(Value::as_str)
                                    .filter(|m| !m.is_empty())
                                {
                                    s.native_mode = pm.to_string();
                                }
                            } else if subtype == Some("status") {
                                let status = msg.get("status").and_then(Value::as_str);
                                if status == Some("requesting") {
                                    // When a mid-turn `user` message arrives just as a reply finishes
                                    // without another tool call, Claude emits `result` for turn 1 and
                                    // immediately begins a follow-up turn with `system/status: requesting`.
                                    // Track that follow-up turn so its `result` is not ignored and live
                                    // activity stays `working` until it finishes.
                                    let started = {
                                        let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                        s.compacting = false;
                                        s.compacting_started_at = None;
                                        if !s.working {
                                            s.working = true;
                                            s.last_turn_interrupted = false;
                                            s.clear_live_turn_output();
                                            true
                                        } else {
                                            false
                                        }
                                    };
                                    if started {
                                        act_out.sync();
                                    }
                                } else if status == Some("compacting") {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    s.compacting = true;
                                    if s.compacting_started_at.is_none() {
                                        s.compacting_started_at = Some(now_iso_rfc3339());
                                    }
                                } else {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                }
                            } else if subtype == Some("compact_boundary") {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.compacting = false;
                                s.compacting_started_at = None;
                            }
                        }
                        "stream_event" => {
                            let ev = msg.get("event").unwrap_or(&Value::Null);
                            let ev_type =
                                ev.get("type").and_then(Value::as_str).unwrap_or_default();
                            match ev_type {
                                "content_block_delta" => {
                                    let delta = ev.get("delta").unwrap_or(&Value::Null);
                                    let dtype = delta
                                        .get("type")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default();
                                    if dtype == "text_delta" {
                                        if let Some(text) =
                                            delta.get("text").and_then(Value::as_str)
                                        {
                                            let mut s =
                                                st_out.lock().unwrap_or_else(|p| p.into_inner());
                                            if s.live_stream_started_at.is_none() {
                                                s.live_stream_started_at = Some(now_iso_rfc3339());
                                            }
                                            append_bounded(
                                                &mut s.live_stream_text,
                                                text,
                                                LIVE_STREAM_MAX_CHARS,
                                            );
                                            s.live_tool_label = None;
                                            s.live_tool_detail = None;
                                            s.live_tool_started_at = None;
                                        }
                                    } else if dtype == "thinking_delta" {
                                        if let Some(thinking) =
                                            delta.get("thinking").and_then(Value::as_str)
                                        {
                                            let mut s =
                                                st_out.lock().unwrap_or_else(|p| p.into_inner());
                                            if s.live_thought_started_at.is_none() {
                                                s.live_thought_started_at = Some(now_iso_rfc3339());
                                            }
                                            append_bounded(
                                                &mut s.live_thought_text,
                                                thinking,
                                                LIVE_STREAM_MAX_CHARS,
                                            );
                                        }
                                    }
                                }
                                "content_block_start" => {
                                    let cb = ev.get("content_block").unwrap_or(&Value::Null);
                                    if cb.get("type").and_then(Value::as_str) == Some("tool_use") {
                                        if let Some(name) = cb.get("name").and_then(Value::as_str) {
                                            let mut s =
                                                st_out.lock().unwrap_or_else(|p| p.into_inner());
                                            s.live_tool_label = Some(name.to_string());
                                            s.live_tool_started_at = Some(now_iso_rfc3339());
                                        }
                                    }
                                }
                                "message_start" => {
                                    let started = {
                                        let mut s =
                                            st_out.lock().unwrap_or_else(|p| p.into_inner());
                                        let flipped = if !s.working && !s.last_turn_interrupted {
                                            s.working = true;
                                            true
                                        } else {
                                            false
                                        };
                                        if let Some(usage) = ev.pointer("/message/usage") {
                                            let used = usage
                                                .get("input_tokens")
                                                .and_then(Value::as_u64)
                                                .unwrap_or(0)
                                                + usage
                                                    .get("cache_read_input_tokens")
                                                    .and_then(Value::as_u64)
                                                    .unwrap_or(0)
                                                + usage
                                                    .get("cache_creation_input_tokens")
                                                    .and_then(Value::as_u64)
                                                    .unwrap_or(0);
                                            if used > 0 {
                                                s.usage_used_tokens = Some(used);
                                            }
                                        }
                                        flipped
                                    };
                                    if started {
                                        act_out.sync();
                                    }
                                }
                                _ => {}
                            }
                        }
                        "assistant" => {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            if let Some(model) = msg
                                .pointer("/message/model")
                                .and_then(Value::as_str)
                                .filter(|m| {
                                    *m != "<synthetic>"
                                        && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
                                })
                            {
                                let preserved =
                                    crate::agent_model_catalog::preserve_claude_1m_context(
                                        s.current_model.as_deref(),
                                        model,
                                        s.usage_window_size,
                                    );
                                s.current_model = Some(preserved);
                            }
                            if let Some(usage) = msg.pointer("/message/usage") {
                                let used = usage
                                    .get("input_tokens")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0)
                                    + usage
                                        .get("cache_read_input_tokens")
                                        .and_then(Value::as_u64)
                                        .unwrap_or(0)
                                    + usage
                                        .get("cache_creation_input_tokens")
                                        .and_then(Value::as_u64)
                                        .unwrap_or(0);
                                if used > 0 {
                                    s.usage_used_tokens = Some(used);
                                }
                            }
                            if msg.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
                                let err_code = msg
                                    .get("error")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|c| !c.is_empty())
                                    .map(str::to_string);
                                let err_text = extract_claude_message_text(&msg);
                                if is_claude_auth_error(err_code.as_deref(), &err_text) {
                                    s.turn_saw_auth_error = Some(if err_text.is_empty() {
                                        "Failed to authenticate: OAuth session expired.".to_string()
                                    } else {
                                        err_text
                                    });
                                } else if !err_text.is_empty() || err_code.is_some() {
                                    s.turn_saw_api_error = Some((
                                        err_code,
                                        if err_text.is_empty() {
                                            "Claude API call failed.".to_string()
                                        } else {
                                            err_text
                                        },
                                    ));
                                }
                            }
                        }
                        "result" => {
                            // Deterministic 1:1 turn correlation: `abort` and `send(interrupt)`
                            // never write a new `user` turn until the previous turn's `result`
                            // has arrived or the old process has been replaced by a new `my_gen`.
                            let (ignored, orphaned, retrying_auth): (
                                bool,
                                Vec<PendingClaudeRequest>,
                                bool,
                            ) = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if let Some(model_usage) =
                                    msg.get("modelUsage").and_then(Value::as_object)
                                {
                                    if let Some((_, first)) = model_usage.iter().next() {
                                        if let Some(cw) =
                                            first.get("contextWindow").and_then(Value::as_u64)
                                        {
                                            if cw > 0 {
                                                s.usage_window_size = Some(cw);
                                            }
                                        }
                                    }
                                }
                                let result_interrupted = msg
                                    .get("subtype")
                                    .and_then(Value::as_str)
                                    .is_some_and(|sub| {
                                        let lower = sub.to_ascii_lowercase();
                                        lower.contains("interrupt")
                                            || lower.contains("cancel")
                                            || lower.contains("abort")
                                    });
                                let result_is_error =
                                    msg.get("is_error").and_then(Value::as_bool) == Some(true);
                                let result_text = extract_claude_message_text(&msg);
                                let result_error_code = msg
                                    .get("error")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|c| !c.is_empty())
                                    .map(str::to_string);
                                let auth_error_text = s.turn_saw_auth_error.take().or_else(|| {
                                    (result_is_error
                                        && is_claude_auth_error(
                                            result_error_code.as_deref(),
                                            &result_text,
                                        ))
                                    .then(|| {
                                        if result_text.is_empty() {
                                            "Failed to authenticate: OAuth session expired."
                                                .to_string()
                                        } else {
                                            result_text.clone()
                                        }
                                    })
                                });
                                let api_error = s.turn_saw_api_error.take().or_else(|| {
                                    (result_is_error && auth_error_text.is_none()).then(|| {
                                        (
                                            result_error_code,
                                            if result_text.is_empty() {
                                                "Claude API call failed.".to_string()
                                            } else {
                                                result_text
                                            },
                                        )
                                    })
                                });
                                if !s.working {
                                    (true, Vec::new(), false)
                                } else if let Some(auth_err) = auth_error_text {
                                    if !s.active_turn_auth_retried
                                        && s.active_turn_user_msg.is_some()
                                    {
                                        // Long-running Claude hosts can hold a stale in-memory
                                        // OAuth token even while cswap/disk holds a fresh one.
                                        // Restart the Claude subprocess once and replay this turn
                                        // silently before surfacing a sign-in error.
                                        s.active_turn_auth_retried = true;
                                        s.pending_auth_retry = s.active_turn_user_msg.take();
                                        s.compacting = false;
                                        s.compacting_started_at = None;
                                        s.clear_live_turn_output();
                                        (false, s.pending_requests.drain(..).collect(), true)
                                    } else {
                                        s.working = false;
                                        s.compacting = false;
                                        s.compacting_started_at = None;
                                        s.active_turn_user_msg = None;
                                        s.last_auth_error = Some(auth_err);
                                        s.last_error = None;
                                        s.last_turn_stop_error =
                                            Some("authentication_failed".to_string());
                                        s.clear_live_turn_output();
                                        (false, s.pending_requests.drain(..).collect(), false)
                                    }
                                } else if let Some((code, err_text)) = api_error {
                                    s.working = false;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    s.active_turn_user_msg = None;
                                    s.last_auth_error = None;
                                    s.last_turn_stop_error =
                                        Some(code.clone().unwrap_or_else(|| "unknown".to_string()));
                                    s.last_error = Some((code, err_text));
                                    s.clear_live_turn_output();
                                    (false, s.pending_requests.drain(..).collect(), false)
                                } else {
                                    s.working = false;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    s.active_turn_user_msg = None;
                                    s.active_turn_auth_retried = false;
                                    s.last_auth_error = None;
                                    s.last_error = None;
                                    s.last_turn_stop_error = None;
                                    if result_interrupted {
                                        s.last_turn_interrupted = true;
                                    }
                                    s.clear_live_turn_output();
                                    (false, s.pending_requests.drain(..).collect(), false)
                                }
                            };
                            if ignored {
                                return;
                            }
                            for req in orphaned {
                                let _ = t_out.control_respond_deny(
                                    &req.request_id,
                                    "Turn ended.",
                                    false,
                                    req.tool_use_id.as_deref(),
                                );
                            }
                            if retrying_auth {
                                println!(
                                    "[Claude Native] OAuth error on turn; restarting Claude once to refresh credentials..."
                                );
                                let _ = std::io::stdout().flush();
                                return;
                            }
                            println!("[Claude Native] Turn finished.");
                            let _ = std::io::stdout().flush();
                            act_out.sync();
                        }
                        _ => {}
                    }
                }));
                if res.is_err() {
                    eprintln!("[Claude Native] Warning: recovered from panic in stdout reader.");
                }
            }
        });
    }

    // 1. Initialize Claude control protocol.
    let mut resolved_default_to_apply: Option<String> = None;
    match transport.control_request(json!({ "subtype": "initialize" }), CLAUDE_CTRL_TIMEOUT) {
        Ok(init_resp) => {
            use ghostex_gx_chat_core::menus::catalog::{
                family_and_version, is_placeholder_model_label, is_placeholder_model_value,
            };
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(models) = init_resp.get("models").and_then(Value::as_array) {
                // Capture what Claude's "default" alias resolves to, but never emit the
                // "Default (recommended)" alias row itself into `availableModels`.
                let mut default_resolved: Option<String> = None;
                let mut mapped: Vec<Value> = Vec::new();
                for m in models {
                    let raw_val = m.get("value").and_then(Value::as_str).unwrap_or_default();
                    let raw_name = m
                        .get("displayName")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let resolved_id = m
                        .get("resolvedModel")
                        .and_then(Value::as_str)
                        .filter(|v| !is_placeholder_model_value(v));
                    if is_placeholder_model_value(raw_val) || is_placeholder_model_label(raw_name) {
                        if default_resolved.is_none() {
                            default_resolved = resolved_id.map(str::to_string);
                        }
                        continue;
                    }
                    let Some(id) = resolved_id
                        .or_else(|| (!is_placeholder_model_value(raw_val)).then_some(raw_val))
                    else {
                        continue;
                    };
                    // Prefer the versioned model name from description ("Opus 5.5 · ...") or
                    // transcript normalization over bare "Opus", and never a placeholder.
                    let desc_name = m
                        .get("description")
                        .and_then(Value::as_str)
                        .and_then(|d| d.split(" · ").next())
                        .map(str::trim)
                        .filter(|d| {
                            !d.is_empty() && !is_placeholder_model_label(d) && d.len() <= 40
                        });
                    let parsed_name =
                        crate::session_chat_options::claude_transcript_model_choice(id)
                            .map(|c| c.label)
                            .filter(|l| !is_placeholder_model_label(l));
                    let name = parsed_name
                        .or_else(|| desc_name.map(str::to_string))
                        .or_else(|| {
                            (!is_placeholder_model_label(raw_name)).then(|| raw_name.to_string())
                        })
                        .unwrap_or_else(|| id.to_string());
                    if mapped
                        .iter()
                        .any(|existing| existing.get("modelId").and_then(Value::as_str) == Some(id))
                    {
                        continue;
                    }
                    mapped.push(json!({
                        "modelId": id,
                        "name": name,
                        "supportedEffortLevels": m.get("supportedEffortLevels").cloned().unwrap_or(json!([])),
                    }));
                }
                let needs_model = s
                    .current_model
                    .as_deref()
                    .is_none_or(is_placeholder_model_value);
                if needs_model {
                    // Pick the newest Opus from the reported list; else what "default" resolved
                    // to; else the first concrete model; else the catalog default.
                    let newest_opus = mapped
                        .iter()
                        .filter_map(|entry| {
                            let id = entry.get("modelId").and_then(Value::as_str)?;
                            let name = entry.get("name").and_then(Value::as_str).unwrap_or(id);
                            let (fam, ver) =
                                family_and_version(name).or_else(|| family_and_version(id))?;
                            let words: Vec<&str> = fam.split_whitespace().collect();
                            words.contains(&"opus").then_some((id.to_string(), ver))
                        })
                        .max_by(|a, b| a.1.cmp(&b.1))
                        .map(|(id, _)| id);
                    let chosen = newest_opus
                        .or(default_resolved)
                        .or_else(|| {
                            mapped
                                .first()
                                .and_then(|m| m.get("modelId"))
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .or_else(|| {
                            crate::agent_model_catalog::preferred_default_model("claude")
                                .map(|(v, _, _)| v)
                        });
                    if let Some(chosen) = chosen {
                        s.current_model = Some(chosen.clone());
                        resolved_default_to_apply = Some(chosen);
                    }
                }
                s.available_models = Value::Array(mapped);
            }
        }
        Err(err) => {
            // Bump generation before stopping so the reader's `ClaudeExitGuard` does not mark
            // `child_exited = true` and tear down `acp-host` when a retry (e.g. turn-1 fresh
            // restart) can still recover.
            process_gen.fetch_add(1, Ordering::AcqRel);
            *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
            state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
            stop_claude_gracefully(&transport, &mut child, child_pid);
            return Err(format!("Claude Native initialize failed: {err}"));
        }
    }
    if let Some(chosen_model) = resolved_default_to_apply {
        let _ = transport.control_request(
            json!({ "subtype": "set_model", "model": chosen_model }),
            CLAUDE_CTRL_TIMEOUT,
        );
    }

    // 2. The freshly spawned Claude process always starts in `"plan"` (for `--read-only`) or
    // `"default"`. Reset `native_mode` before syncing so a restart at `Auto` always sends
    // `set_permission_mode: "auto"` to the new child process.
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.native_mode = if read_only { "plan" } else { "default" }.to_string();
    }
    if let Err(err) =
        apply_claude_approval_level(&transport, state, mode_sync_lock, approval_level, false)
    {
        eprintln!("[Claude Native] Warning: could not sync approval level: {err}");
    }

    Ok(ClaudeProcess { child, transport })
}

/// Deterministically finish an interrupted Claude turn:
/// - If already idle, cancels any stale pending permissions and returns immediately.
/// - Otherwise sends `interrupt` and waits for the aborted turn's `result` (`working == false`).
/// - If `result` does not arrive within the timeout (or `interrupt` fails), replaces the Claude
///   process with a fresh resumed process under a new generation so stale events/results can never
///   correlate with the next turn.
#[allow(clippy::too_many_arguments)]
fn drain_or_restart_claude_turn(
    proc: &mut ClaudeProcess,
    spec: &AcpAgentSpec,
    parsed: &HostArgs,
    cwd: &str,
    spool_session_id: &str,
    state: &Arc<Mutex<ClaudeState>>,
    activity: &ClaudeActivityReporter,
    mode_sync_lock: &Arc<Mutex<()>>,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<ClaudeTransport>>>>,
    process_gen: &Arc<AtomicU64>,
    wait_timeout: Duration,
) -> Result<(), String> {
    let was_working = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working {
            s.last_turn_interrupted = true;
        }
        s.working
    };
    cancel_all_pending_claude(&proc.transport, state);
    if !was_working {
        return Ok(());
    }
    let interrupt_res = proc
        .transport
        .control_request(json!({ "subtype": "interrupt" }), Duration::from_secs(5));
    if let Err(err) = &interrupt_res {
        eprintln!("[Claude Native] Warning: interrupt control_request failed: {err}");
    }
    let wait_until = Instant::now() + wait_timeout;
    while Instant::now() < wait_until {
        if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }

    // The aborted turn's `result` did not arrive in time: deterministically restart and resume
    // Claude under a fresh generation so every future `result` maps 1:1 to a new turn.
    let new_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
    *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
    stop_claude_gracefully(&proc.transport, &mut proc.child, child_pid);
    let (resume_id, model, effort, read_only, level) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.working = false;
        s.child_pid = 0;
        s.pending_requests.clear();
        s.clear_live_turn_output();
        (
            s.native_session_id.clone(),
            s.current_model.clone(),
            s.current_effort.clone(),
            s.read_only,
            s.approval_level,
        )
    };
    let next_proc = match spawn_and_init_claude(
        spec,
        parsed,
        cwd,
        spool_session_id,
        resume_id.as_deref(),
        false,
        model.as_deref(),
        effort.as_deref(),
        read_only,
        level,
        state,
        activity,
        mode_sync_lock,
        child_pid,
        active_transport,
        process_gen,
        new_gen,
    ) {
        Ok(p) => p,
        Err(_err)
            if resume_id.as_deref().is_some_and(|id| {
                crate::agent_transcripts::find_claude_transcript(id).is_none()
            }) =>
        {
            // Only when NO transcript exists on disk for `resume_id` (turn-1 abort before Claude
            // ever persisted a conversation) retry once with a fresh session. Never wipe
            // `native_session_id` or open a blank chat when a persisted transcript exists.
            let retry_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.native_session_id = None;
                s.child_exited = false;
            }
            spawn_and_init_claude(
                spec,
                parsed,
                cwd,
                spool_session_id,
                None,
                false,
                model.as_deref(),
                effort.as_deref(),
                read_only,
                level,
                state,
                activity,
                mode_sync_lock,
                child_pid,
                active_transport,
                process_gen,
                retry_gen,
            )?
        }
        Err(err) => return Err(err),
    };
    *proc = next_proc;
    Ok(())
}

pub(super) fn run(spec: &AcpAgentSpec, parsed: HostArgs) -> i32 {
    let cwd_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd = cwd_path.to_string_lossy().to_string();
    let writable_roots = canonicalize_writable_roots(&cwd_path, &parsed.writable_roots);
    let endpoint = GxserverEndpoint::from_env_or_secret_file(parsed.channel_secret_file.as_deref());
    let spool_session_id = endpoint
        .as_ref()
        .map(|ep| ep.session_id.clone())
        .unwrap_or_else(|| format!("local-{}", std::process::id()));

    println!("Ghostex Native Host · {} · Starting...", spec.display_name);
    let _ = std::io::stdout().flush();

    let gx_paths = crate::paths::get_gxserver_paths(None);
    let signal_guard = TerminationSignalGuard::install();

    let state = Arc::new(Mutex::new(ClaudeState::new(
        parsed.approval_level,
        parsed.read_only,
        cwd_path.clone(),
        writable_roots,
        parsed.model.clone(),
        parsed.effort.clone(),
    )));
    let mode_sync_lock = Arc::new(Mutex::new(()));
    if !parsed.fork_session {
        if let Some(resume_id) = parsed
            .resume_session_id
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .native_session_id = Some(resume_id.to_string());
        }
    }
    let activity = ClaudeActivityReporter::start(endpoint.clone(), state.clone());
    let shutting_down = Arc::new(AtomicBool::new(false));
    let child_pid = Arc::new(AtomicU32::new(0));
    let active_transport: Arc<Mutex<Option<Arc<ClaudeTransport>>>> = Arc::new(Mutex::new(None));
    let process_gen = Arc::new(AtomicU64::new(1));

    let mut proc = match spawn_and_init_claude(
        spec,
        &parsed,
        &cwd,
        &spool_session_id,
        parsed.resume_session_id.as_deref(),
        parsed.fork_session,
        parsed.model.as_deref(),
        parsed.effort.as_deref(),
        parsed.read_only,
        parsed.approval_level,
        &state,
        &activity,
        &mode_sync_lock,
        &child_pid,
        &active_transport,
        &process_gen,
        1,
    ) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };

    #[cfg(unix)]
    {
        let pid_sig = child_pid.clone();
        let transport_sig = active_transport.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if super::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
                    if let Some(t) = transport_sig
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .take()
                    {
                        t.close_stdin();
                    }
                    let pid = pid_sig.swap(0, Ordering::AcqRel);
                    if pid != 0 {
                        let cpid = pid as libc::pid_t;
                        unsafe {
                            libc::kill(cpid, libc::SIGTERM);
                        }
                        let deadline = Instant::now() + Duration::from_secs(1);
                        let mut reaped = false;
                        while Instant::now() < deadline {
                            let waited =
                                unsafe { libc::waitpid(cpid, std::ptr::null_mut(), libc::WNOHANG) };
                            if waited > 0
                                || (waited < 0
                                    && std::io::Error::last_os_error().raw_os_error()
                                        != Some(libc::EINTR))
                            {
                                reaped = true;
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                        if !reaped {
                            unsafe {
                                libc::kill(cpid, libc::SIGKILL);
                                let _ = libc::waitpid(cpid, std::ptr::null_mut(), libc::WNOHANG);
                            }
                        }
                    }
                    if let Some(ep) = ep_sig.as_ref() {
                        ep.close_channel();
                    }
                    std::process::exit(143);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    let native_sid = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .native_session_id
        .clone();
    println!(
        "Ghostex Native Host · {} · Ready (conversation: {})",
        spec.display_name,
        native_sid.as_deref().unwrap_or("pending")
    );
    let _ = std::io::stdout().flush();
    activity.sync();

    // 3. Main `/api/agentChatChannel` long-poll loop.
    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
        "mid_turn_send",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "switch_to_cli"
    ]);
    let channel_agent = format!("acp:{}", spec.family);
    let mut reconnecting = false;
    let mut crash_restarts: VecDeque<Instant> = VecDeque::new();

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        let retry_msg = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.pending_auth_retry.take()
        };
        if let Some(mut retry_user_msg) = retry_msg {
            let new_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
            *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
            stop_claude_gracefully(&proc.transport, &mut proc.child, &child_pid);
            let (resume_id, model, effort, read_only, level) = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.child_pid = 0;
                s.child_exited = false;
                s.pending_requests.clear();
                s.clear_live_turn_output();
                (
                    s.native_session_id.clone(),
                    s.current_model.clone(),
                    s.current_effort.clone(),
                    s.read_only,
                    s.approval_level,
                )
            };
            match spawn_and_init_claude(
                spec,
                &parsed,
                &cwd,
                &spool_session_id,
                resume_id.as_deref(),
                false,
                model.as_deref(),
                effort.as_deref(),
                read_only,
                level,
                &state,
                &activity,
                &mode_sync_lock,
                &child_pid,
                &active_transport,
                &process_gen,
                new_gen,
            ) {
                Ok(next_proc) => {
                    proc = next_proc;
                    let sid = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .native_session_id
                        .clone();
                    if let Some(sid_str) = sid.as_deref() {
                        retry_user_msg["session_id"] = json!(sid_str);
                    }
                    if let Err(err) = proc.transport.write_json(&retry_user_msg) {
                        eprintln!(
                            "[Claude Native] Failed to replay turn after OAuth restart: {err}"
                        );
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = false;
                        s.last_auth_error = Some(format!(
                            "Failed to authenticate: OAuth session expired ({err})."
                        ));
                        s.last_turn_stop_error = Some("authentication_failed".to_string());
                        drop(s);
                        activity.sync();
                    } else {
                        println!("[Claude Native] Replayed turn after credential refresh restart.");
                        let _ = std::io::stdout().flush();
                    }
                }
                Err(err) => {
                    eprintln!("[Claude Native] Could not restart Claude after OAuth error: {err}");
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.working = false;
                    s.last_auth_error = Some(format!(
                        "Failed to authenticate: OAuth session expired ({err})."
                    ));
                    s.last_turn_stop_error = Some("authentication_failed".to_string());
                    drop(s);
                    activity.sync();
                }
            }
            continue;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            let now = Instant::now();
            while crash_restarts
                .front()
                .is_some_and(|at| now.duration_since(*at) > Duration::from_secs(60))
            {
                crash_restarts.pop_front();
            }
            if crash_restarts.len() >= 3 {
                println!("\r\n[Claude Native] Agent process exited repeatedly; stopping host.");
                break;
            }
            crash_restarts.push_back(now);
            println!("\r\n[Claude Native] Agent process exited unexpectedly; restarting...");
            let _ = std::io::stdout().flush();
            thread::sleep(Duration::from_millis(500));
            let new_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
            *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
            stop_claude_gracefully(&proc.transport, &mut proc.child, &child_pid);
            let (resume_id, model, effort, read_only, level) = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.working = false;
                s.compacting = false;
                s.compacting_started_at = None;
                s.child_pid = 0;
                s.child_exited = false;
                s.pending_requests.clear();
                s.clear_live_turn_output();
                (
                    s.native_session_id.clone(),
                    s.current_model.clone(),
                    s.current_effort.clone(),
                    s.read_only,
                    s.approval_level,
                )
            };
            match spawn_and_init_claude(
                spec,
                &parsed,
                &cwd,
                &spool_session_id,
                resume_id.as_deref(),
                false,
                model.as_deref(),
                effort.as_deref(),
                read_only,
                level,
                &state,
                &activity,
                &mode_sync_lock,
                &child_pid,
                &active_transport,
                &process_gen,
                new_gen,
            ) {
                Ok(next_proc) => {
                    proc = next_proc;
                    println!(
                        "\r\n[Claude Native] Agent process restarted and conversation resumed."
                    );
                    let _ = std::io::stdout().flush();
                    activity.sync();
                    continue;
                }
                Err(err) => {
                    eprintln!("[Claude Native] Failed to restart crashed agent process: {err}");
                    break;
                }
            }
        }
        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let is_working_now = state.lock().unwrap_or_else(|p| p.into_inner()).working;
        let mut poll_params = json!({
            "action": "poll",
            "sessionRef": ep.session_ref,
            "agent": channel_agent,
            "pid": std::process::id(),
            "instance": ep.instance,
            "version": 1,
            "capabilities": capabilities,
        });
        if is_working_now {
            poll_params["holdMs"] = json!(1500);
        }
        if let Some(secret) = ep.channel_secret.as_deref() {
            poll_params["channelSecret"] = json!(secret);
        }
        let poll_body = json!({
            "requestId": format!("acp-poll-{}", now_ms()),
            "params": poll_params,
        });
        let resp = match ep.post_json("/api/agentChatChannel", poll_body, Duration::from_secs(28)) {
            Ok(resp) => {
                if let Some(desired) = resp
                    .pointer("/result/desiredApprovalLevel")
                    .or_else(|| resp.get("desiredApprovalLevel"))
                    .and_then(Value::as_str)
                    .and_then(AcpApprovalLevel::parse)
                {
                    let current = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .approval_level;
                    if desired != current {
                        let _ = apply_claude_approval_level(
                            &proc.transport,
                            &state,
                            &mode_sync_lock,
                            desired,
                            false,
                        );
                    }
                }
                if reconnecting {
                    reconnecting = false;
                    activity.sync();
                }
                resp
            }
            Err(_) => {
                reconnecting = true;
                thread::sleep(POLL_RETRY_DELAY);
                continue;
            }
        };
        let commands = resp
            .pointer("/result/commands")
            .or_else(|| resp.get("commands"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for cmd in commands {
            let id = cmd.get("id").and_then(Value::as_str).unwrap_or_default();
            let kind = cmd.get("kind").and_then(Value::as_str).unwrap_or_default();
            let params = cmd.get("params").cloned().unwrap_or(Value::Null);
            match kind {
                "state" => {
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "abort" => {
                    match drain_or_restart_claude_turn(
                        &mut proc,
                        spec,
                        &parsed,
                        &cwd,
                        &spool_session_id,
                        &state,
                        &activity,
                        &mode_sync_lock,
                        &child_pid,
                        &active_transport,
                        &process_gen,
                        Duration::from_secs(5),
                    ) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "aborted": true }), None);
                        }
                        Err(err) => {
                            state.lock().unwrap_or_else(|p| p.into_inner()).child_exited = true;
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                        }
                    }
                }
                "approve" => {
                    let decision = params
                        .get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("allow");
                    let target_tool_use_id = params
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let req = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|p| {
                                    !p.is_question && p.tool_use_id.as_deref() == Some(wanted)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|p| !p.is_question)
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        ep.ack(id, false, Value::Null, Some("No pending Claude approval."));
                        continue;
                    };
                    let norm = decision.trim().to_ascii_lowercase();
                    let is_deny = matches!(
                        norm.as_str(),
                        "deny"
                            | "reject"
                            | "reject_once"
                            | "reject_always"
                            | "escape"
                            | "cancel"
                            | "\x1b"
                            | "n"
                            | "no"
                    );
                    let leaving_plan = !is_deny && req.tool_name == "ExitPlanMode";
                    let res = if is_deny {
                        proc.transport.control_respond_deny(
                            &req.request_id,
                            "User denied permission.",
                            false,
                            req.tool_use_id.as_deref(),
                        )
                    } else {
                        proc.transport.control_respond_allow(
                            &req.request_id,
                            req.raw_input,
                            req.tool_use_id.as_deref(),
                        )
                    };
                    match res {
                        Ok(()) => {
                            if leaving_plan {
                                let level = {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.read_only = false;
                                    s.approval_level
                                };
                                let _ = apply_claude_approval_level(
                                    &proc.transport,
                                    &state,
                                    &mode_sync_lock,
                                    level,
                                    true,
                                );
                            }
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "answer_question" => {
                    let selections = params
                        .get("selections")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let target_tool_use_id = params
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let req = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|p| {
                                    p.is_question && p.tool_use_id.as_deref() == Some(wanted)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|p| p.is_question)
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                        continue;
                    };
                    let res = if let Some(updated_input) =
                        build_ask_user_answers(&req.raw_input, &selections)
                    {
                        proc.transport.control_respond_allow(
                            &req.request_id,
                            updated_input,
                            req.tool_use_id.as_deref(),
                        )
                    } else {
                        proc.transport.control_respond_deny(
                            &req.request_id,
                            "User dismissed the question.",
                            false,
                            req.tool_use_id.as_deref(),
                        )
                    };
                    match res {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let model = params
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let effort = params
                        .get("effort")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if !model.is_empty() {
                        if let Err(err) = proc.transport.control_request(
                            json!({ "subtype": "set_model", "model": model }),
                            CLAUDE_CTRL_TIMEOUT,
                        ) {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .current_model = Some(model);
                    }
                    if !effort.is_empty() {
                        if let Err(err) = proc.transport.control_request(
                            json!({
                                "subtype": "apply_flag_settings",
                                "settings": { "effortLevel": effort },
                            }),
                            CLAUDE_CTRL_TIMEOUT,
                        ) {
                            ep.ack(
                                id,
                                false,
                                Value::Null,
                                Some(&format!("Could not update Claude effort level: {err}")),
                            );
                            continue;
                        }
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .current_effort = Some(effort);
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "mode" => {
                    let mode = params
                        .get("mode")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let Some(level) = AcpApprovalLevel::parse(mode) else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some(&format!("Unsupported approval level: {mode}")),
                        );
                        continue;
                    };
                    if let Err(err) = apply_claude_approval_level(
                        &proc.transport,
                        &state,
                        &mode_sync_lock,
                        level,
                        level != AcpApprovalLevel::Ask,
                    ) {
                        ep.ack(id, false, Value::Null, Some(&err));
                        continue;
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "send" => {
                    let text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let images: Vec<String> = params
                        .get("images")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let (is_working, has_pending) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, !s.pending_requests.is_empty())
                    };
                    if !interrupt && has_pending {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }
                    let content = match build_claude_user_content(&gx_paths, &text, &images) {
                        Ok(c) => c,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    if is_working && !interrupt {
                        let sid = state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .native_session_id
                            .clone();
                        let mut user_msg = json!({
                            "type": "user",
                            "message": {
                                "role": "user",
                                "content": content,
                            }
                        });
                        if let Some(sid_str) = sid.as_deref() {
                            user_msg["session_id"] = json!(sid_str);
                        }
                        println!("\r\n[Claude Native] Queued mid-turn message...");
                        let _ = std::io::stdout().flush();
                        match proc.transport.write_json(&user_msg) {
                            Ok(()) => {
                                ep.ack(
                                    id,
                                    true,
                                    json!({
                                        "accepted": true,
                                        "midTurn": true,
                                        "sessionId": sid,
                                    }),
                                    None,
                                );
                            }
                            Err(err) => {
                                ep.ack(id, false, Value::Null, Some(&err));
                            }
                        }
                        continue;
                    }
                    if is_working && interrupt {
                        if let Err(err) = drain_or_restart_claude_turn(
                            &mut proc,
                            spec,
                            &parsed,
                            &cwd,
                            &spool_session_id,
                            &state,
                            &activity,
                            &mode_sync_lock,
                            &child_pid,
                            &active_transport,
                            &process_gen,
                            Duration::from_secs(5),
                        ) {
                            state.lock().unwrap_or_else(|p| p.into_inner()).child_exited = true;
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    }
                    cancel_all_pending_claude(&proc.transport, &state);
                    let sid = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.compacting = false;
                        s.compacting_started_at = None;
                        s.last_turn_interrupted = false;
                        s.last_turn_stop_error = None;
                        s.last_auth_error = None;
                        s.last_error = None;
                        s.active_turn_auth_retried = false;
                        s.turn_saw_auth_error = None;
                        s.turn_saw_api_error = None;
                        s.pending_auth_retry = None;
                        s.clear_live_turn_output();
                        s.native_session_id.clone()
                    };
                    let mut user_msg = json!({
                        "type": "user",
                        "message": {
                            "role": "user",
                            "content": content,
                        }
                    });
                    if let Some(sid_str) = sid.as_deref() {
                        user_msg["session_id"] = json!(sid_str);
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.active_turn_user_msg = Some(user_msg.clone());
                    }
                    activity.sync();
                    println!("\r\n[Claude Native] Turn started...");
                    let _ = std::io::stdout().flush();
                    match proc.transport.write_json(&user_msg) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(
                                id,
                                true,
                                json!({ "accepted": true, "sessionId": sid }),
                                None,
                            );
                        }
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.working = false;
                                s.last_turn_interrupted = true;
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                        }
                    }
                }
                "switch_to_cli" => {
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let (is_working, has_pending, sid, carry_model, carry_effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let model = s.current_model.as_deref().map(|m| {
                            crate::agent_model_catalog::preserve_claude_1m_context(
                                Some(m),
                                m,
                                s.usage_window_size,
                            )
                        });
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            model,
                            s.current_effort.clone(),
                        )
                    };
                    if (is_working || has_pending) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Claude is working on a turn. Wait or interrupt before switching to Terminal."),
                        );
                        continue;
                    }
                    cancel_all_pending_claude(&proc.transport, &state);
                    if is_working {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        let _ = proc.transport.control_request(
                            json!({ "subtype": "interrupt" }),
                            Duration::from_secs(3),
                        );
                    }
                    shutting_down.store(true, Ordering::Release);
                    process_gen.fetch_add(1, Ordering::AcqRel);
                    stop_claude_gracefully(&proc.transport, &mut proc.child, &child_pid);
                    ep.ack(
                        id,
                        true,
                        json!({
                            "switched": true,
                            "nativeSessionId": sid,
                            "model": carry_model,
                            "effort": carry_effort,
                        }),
                        None,
                    );
                    ep.close_channel();
                    switch_to_cli_requested = true;
                    break;
                }
                other => {
                    ep.ack(
                        id,
                        false,
                        Value::Null,
                        Some(&format!("Unsupported Claude Native command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, accept_all, read_only, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            let model = s.current_model.as_deref().map(|m| {
                crate::agent_model_catalog::preserve_claude_1m_context(
                    Some(m),
                    m,
                    s.usage_window_size,
                )
            });
            (
                s.native_session_id.clone(),
                s.approval_level.implies_cli_accept_all(),
                s.read_only,
                model,
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            accept_all,
            read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    process_gen.fetch_add(1, Ordering::AcqRel);
    stop_claude_gracefully(&proc.transport, &mut proc.child, &child_pid);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
