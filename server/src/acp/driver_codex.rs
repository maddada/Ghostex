//! Native `codex app-server --stdio` JSON-RPC driver for `ghostex acp-host --agent codex`.
//!
//! Speaks Codex's structured app-server protocol directly in Rust (zero Node/Bun wrapper),
//! enforcing Ghostex's four live approval levels (`Ask`, `Auto-edits`, `Auto`, `Full access`) with
//! one-time `"accept"` decisions (never `"acceptForSession"`), supporting `requestUserInput`
//! questions, local image attachments, models & reasoning efforts, live streaming/thinking/tool
//! progress, token usage, `gxserver` reload survival inside `zmx`, and Switch to Terminal
//! (`codex resume <threadId>`).

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

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
    build_acp_server_argv, is_allowed_connector_env_key, transcript_path_for_session, AcpAgentSpec,
};

const CODEX_RPC_TIMEOUT: Duration = Duration::from_secs(30);

#[path = "driver_codex_rewind.rs"]
mod rewind;

#[path = "driver_codex_clear.rs"]
mod clear;

struct CodexTransport {
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    spool: Mutex<SpoolWriter>,
}

impl CodexTransport {
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
            return Err("Codex app-server stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Codex app-server: {e}"))
    }

    fn close_stdin(&self) {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        *guard = None;
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
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
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if let Some(error) = resp.get("error") {
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex RPC failed");
                    Err(message.to_string())
                } else {
                    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err(format!("Codex RPC `{method}` timed out."))
            }
        }
    }

    fn respond(&self, rpc_id: Value, result: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result,
        }))
    }

    fn respond_error(&self, rpc_id: Value, code: i64, message: &str) {
        let _ = self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": code, "message": message },
        }));
    }

    fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

#[derive(Clone, Debug)]
struct McpElicitationField {
    key: String,
    field_type: String,
    multi_select: bool,
    /// Wire values corresponding 1:1 with the question's display `options` (so titled `oneOf`
    /// `[{ const, title }]` and legacy `enum` + `enumNames` send the wire value, never the display
    /// title).
    option_values: Vec<String>,
}

#[derive(Clone, Debug)]
enum CodexRequestKind {
    CommandApproval,
    FileChangeApproval,
    PermissionsApproval {
        permissions: Value,
    },
    UserInput {
        questions: Vec<Value>,
    },
    McpElicitation {
        fields: Vec<McpElicitationField>,
        questions: Vec<Value>,
    },
}

#[derive(Clone, Debug)]
struct PendingCodexRequest {
    rpc_id: Value,
    kind: CodexRequestKind,
    tool_name: String,
    summary: String,
    tool_use_id: Option<String>,
    reason: Option<String>,
}

impl PendingCodexRequest {
    fn is_question(&self) -> bool {
        match &self.kind {
            CodexRequestKind::UserInput { .. } => true,
            CodexRequestKind::McpElicitation { questions, .. } => !questions.is_empty(),
            _ => false,
        }
    }

    fn to_ghostex_prompt(&self) -> Value {
        if let CodexRequestKind::UserInput { questions }
        | CodexRequestKind::McpElicitation { questions, .. } = &self.kind
        {
            if !questions.is_empty() {
                let formatted: Vec<Value> = questions
                    .iter()
                    .map(|q| {
                        let options = q
                            .get("options")
                            .and_then(Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|opt| {
                                        let label = opt.get("label").and_then(Value::as_str)?;
                                        let mut item = json!({ "label": label });
                                        if let Some(desc) =
                                            opt.get("description").and_then(Value::as_str)
                                        {
                                            item["description"] = json!(desc);
                                        }
                                        Some(item)
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        json!({
                            "question": q.get("question").and_then(Value::as_str).unwrap_or("Choose an option."),
                            "header": q.get("header").and_then(Value::as_str).unwrap_or("Question"),
                            "multiSelect": q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
                            "allowCustom": q.get("isOther").and_then(Value::as_bool).unwrap_or(true),
                            "options": options,
                        })
                    })
                    .collect();
                return json!({
                    "kind": "question",
                    "toolUseId": self.tool_use_id,
                    "questions": formatted,
                });
            }
        }
        let mut prompt = json!({
            "kind": "approval",
            "tool": self.tool_name,
            "summary": self.summary,
            "toolUseId": self.tool_use_id,
            "options": [
                { "optionId": "accept", "label": "Allow", "kind": "allow_once" },
                { "optionId": "decline", "label": "Deny", "kind": "reject_once" }
            ],
        });
        if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
            prompt["reason"] = json!(reason);
        }
        prompt
    }

    fn to_activity_permission(&self) -> PendingPermission {
        PendingPermission {
            rpc_id: self.rpc_id.clone(),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            tool_use_id: self.tool_use_id.clone(),
            reason: self.reason.clone(),
            is_question: self.is_question(),
            options: vec![
                AcpPermissionOption {
                    option_id: "accept".to_string(),
                    name: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                },
                AcpPermissionOption {
                    option_id: "decline".to_string(),
                    name: "Deny".to_string(),
                    kind: "reject_once".to_string(),
                },
            ],
            custom_prompt_override: Some(self.to_ghostex_prompt()),
        }
    }
}

const TURN_ID_HISTORY_CAP: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodexDenyMode {
    None,
    HomeOrGbot,
    GbotApi,
}

fn codex_error_info_code(info: Option<&Value>) -> Option<String> {
    match info? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
}

fn codex_model_is_available(available_models: &Value, requested: &str) -> bool {
    let Some(arr) = available_models.as_array() else {
        return true;
    };
    if arr.is_empty() {
        return true;
    }
    arr.iter().any(|entry| {
        ["modelId", "id", "model", "slug"]
            .iter()
            .filter_map(|key| entry.get(*key).and_then(Value::as_str))
            .any(|candidate| candidate.eq_ignore_ascii_case(requested))
    })
}

fn push_capped_turn_id(ring: &mut VecDeque<String>, tid: String) {
    if ring.iter().any(|existing| existing == &tid) {
        return;
    }
    while ring.len() >= TURN_ID_HISTORY_CAP {
        ring.pop_front();
    }
    ring.push_back(tid);
}

fn remove_turn_id(ring: &mut VecDeque<String>, tid: &str) -> bool {
    if let Some(pos) = ring.iter().position(|existing| existing == tid) {
        ring.remove(pos);
        true
    } else {
        false
    }
}

/// Compute the Ghostex server-generated Codex permission profile name for `(deny_mode, level,
/// read_only)`: always the MOST restrictive of the level's sandbox, `--read-only`, and the deny
/// profile (a deny profile adds denies on top of the level and NEVER widens `readOnly` or
/// `workspaceWrite` to `"/"="write"` or network-enabled).
fn desired_codex_profile(
    deny_mode: CodexDenyMode,
    level: AcpApprovalLevel,
    read_only: bool,
) -> Option<&'static str> {
    match deny_mode {
        CodexDenyMode::None => None,
        CodexDenyMode::GbotApi => {
            if read_only {
                Some("ghostex-api-ro")
            } else {
                Some("ghostex-api-ws")
            }
        }
        CodexDenyMode::HomeOrGbot => {
            if read_only {
                Some("ghostex-ro")
            } else if level == AcpApprovalLevel::FullAccess {
                Some("ghostex-full")
            } else {
                Some("ghostex-ws")
            }
        }
    }
}

struct CodexState {
    thread_id: Option<String>,
    thread_path: Option<String>,
    active_turn_id: Option<String>,
    awaiting_turn_start: bool,
    ignored_turn_ids: VecDeque<String>,
    completed_turn_ids: VecDeque<String>,
    working: bool,
    last_turn_interrupted: bool,
    last_turn_stop_error: Option<String>,
    last_error: Option<(Option<String>, String)>,
    compacting: bool,
    compacting_started_at: Option<String>,
    approval_level: AcpApprovalLevel,
    use_cli_default: bool,
    read_only: bool,
    deny_mode: CodexDenyMode,
    active_profile: Option<&'static str>,
    /// True when a mid-turn upgrade needs a different Ghostex deny profile; applied deterministically
    /// at the next turn boundary (by restarting `app-server` and resuming the thread).
    pending_profile_reload: bool,
    /// Non-empty when restarting/resuming `app-server` under the new approval profile failed
    /// (fail-closed: refuses new turns and surfaces a visible error notice instead of running looser).
    fatal_error: Option<String>,
    developer_instructions: Option<String>,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    available_models: Value,
    pending_requests: VecDeque<PendingCodexRequest>,
    file_change_items: HashMap<String, Value>,
    live_stream_text: String,
    live_stream_started_at: Option<String>,
    live_thought_text: String,
    live_thought_started_at: Option<String>,
    live_tool_label: Option<String>,
    live_tool_detail: Option<String>,
    live_tool_started_at: Option<String>,
    usage_used_tokens: Option<u64>,
    usage_window_size: Option<u64>,
    session_title: Option<String>,
    last_event_at_ms: i64,
    child_pid: u32,
    child_exited: bool,
}

impl CodexState {
    #[allow(clippy::too_many_arguments)]
    fn new(
        approval_level: AcpApprovalLevel,
        use_cli_default: bool,
        read_only: bool,
        deny_mode: CodexDenyMode,
        active_profile: Option<&'static str>,
        developer_instructions: Option<String>,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
        initial_model: Option<String>,
        initial_effort: Option<String>,
    ) -> Self {
        Self {
            thread_id: None,
            thread_path: None,
            active_turn_id: None,
            awaiting_turn_start: false,
            ignored_turn_ids: VecDeque::new(),
            completed_turn_ids: VecDeque::new(),
            working: false,
            last_turn_interrupted: false,
            last_turn_stop_error: None,
            last_error: None,
            compacting: false,
            compacting_started_at: None,
            approval_level,
            use_cli_default,
            read_only,
            deny_mode,
            active_profile,
            pending_profile_reload: false,
            fatal_error: None,
            developer_instructions,
            cwd,
            writable_roots,
            current_model: initial_model,
            current_effort: initial_effort,
            available_models: Value::Array(Vec::new()),
            pending_requests: VecDeque::new(),
            file_change_items: HashMap::new(),
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
        let transcript_path = self.thread_path.clone().or_else(|| {
            self.thread_id
                .as_deref()
                .and_then(|id| transcript_path_for_session("codex", id))
                .map(|p| p.to_string_lossy().to_string())
        });
        let last_error = self.last_error.as_ref().map(|(code, msg)| {
            json!({
                "code": code,
                "message": msg,
            })
        });
        let auth_error = self.last_error.as_ref().and_then(|(code, msg)| {
            code.as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("unauthorized"))
                .then_some(msg.as_str())
        });
        json!({
            "family": "codex",
            "nativeSessionId": self.thread_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": self.approval_level.as_str(),
            "nativeAutoActive": self.approval_level == AcpApprovalLevel::Auto,
            "nativeAutoAvailable": true,
            "availableModels": self.available_models,
            "availableModes": [
                { "id": "ask", "name": "Ask" },
                { "id": "auto-edits", "name": "Auto-edits" },
                { "id": "auto", "name": "Auto" },
                { "id": "full-access", "name": "Full permissions" }
            ],
            "configOptions": [],
            "pendingPermission": self.pending_requests.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "sessionTitle": self.session_title,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": transcript_path,
            "childPid": self.child_pid,
            "childExited": self.child_exited,
            "fatalError": self.fatal_error,
            "lastError": last_error,
            "authError": auth_error,
        })
    }
}

#[derive(Clone)]
struct CodexActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl CodexActivityReporter {
    fn start(endpoint: Option<GxserverEndpoint>, state: Arc<Mutex<CodexState>>) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                let (act, sid, path, perm, interrupted, stop_error) = {
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
                        s.thread_id.clone(),
                        s.thread_path.clone(),
                        front,
                        s.last_turn_interrupted,
                        s.last_turn_stop_error.clone(),
                    )
                };
                let mut ok = ep.report_activity_with_error(
                    "codex",
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
                            let (act, sid, path, perm, interrupted, stop_error) = {
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
                                    s.thread_id.clone(),
                                    s.thread_path.clone(),
                                    front,
                                    s.last_turn_interrupted,
                                    s.last_turn_stop_error.clone(),
                                )
                            };
                            ok = ep.report_activity_with_error(
                                "codex",
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

    fn sync(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(());
        }
    }
}

struct CodexExitGuard {
    state: Arc<Mutex<CodexState>>,
    process_gen: Arc<AtomicU64>,
    my_gen: u64,
}
impl Drop for CodexExitGuard {
    fn drop(&mut self) {
        if self.process_gen.load(Ordering::Acquire) != self.my_gen {
            return;
        }
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.child_exited = true;
        s.working = false;
    }
}

struct CodexProcess {
    child: Child,
    lifetime: super::child_lifetime::ChildLifetime,
    transport: Arc<CodexTransport>,
}

struct CodexSpawnConfig {
    prog: String,
    argv: Vec<String>,
    acp_envs: Vec<(String, String)>,
    cwd: String,
    spool_session_id: String,
    record_target: Option<super::child_lifetime::ChildRecordTarget>,
}

/// Build `(approvalPolicy, approvalsReviewer, sandboxPolicy)` for Codex `turn/start` matching
/// Ghostex's four approval levels (plus `--read-only` threads when at `Ask`).
fn codex_turn_policies(
    level: AcpApprovalLevel,
    read_only: bool,
    writable_roots: &[PathBuf],
) -> (Value, &'static str, Value) {
    let roots: Vec<String> = writable_roots
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    match level {
        AcpApprovalLevel::Ask if read_only => {
            (json!("untrusted"), "user", json!({ "type": "readOnly" }))
        }
        // In Ask, keep `workspaceWrite` + `approvalPolicy: "untrusted"` so Codex proposes file
        // edits via `apply_patch` (sending `item/fileChange/requestApproval`) and shell commands
        // via `item/commandExecution/requestApproval` for Ghostex's user approval gate, rather
        // than rejecting writes in `readOnly` mode (which stays reserved for `--read-only` threads).
        AcpApprovalLevel::Ask => (
            json!("untrusted"),
            "user",
            json!({ "type": "workspaceWrite", "writableRoots": roots }),
        ),
        // In Auto-edits, keep approvalPolicy = "untrusted" so Codex asks `driver_codex` for both
        // file changes and shell commands; `driver_codex` evaluates them via `acp/approval.rs`,
        // auto-approving workspace-contained edits with one-time `"accept"` and asking the user
        // for commands and outside-workspace edits.
        AcpApprovalLevel::AutoEdits => (
            json!("untrusted"),
            "user",
            json!({ "type": "workspaceWrite", "writableRoots": roots }),
        ),
        AcpApprovalLevel::Auto => (
            json!("on-request"),
            "auto_review",
            json!({ "type": "workspaceWrite", "writableRoots": roots }),
        ),
        // Granular policy with `mcp_elicitations: true` (never `"never"`, which silently rejects
        // elicitation prompts before the user sees them).
        AcpApprovalLevel::FullAccess => (
            json!({
                "granular": {
                    "mcp_elicitations": true,
                    "request_permissions": false,
                    "rules": false,
                    "sandbox_approval": false,
                    "skill_approval": false
                }
            }),
            "user",
            json!({ "type": "dangerFullAccess" }),
        ),
    }
}

/// Map the effective `(approvalPolicy, approvalsReviewer, sandbox)` that Codex `thread/start` or
/// `thread/resume` reported back from `~/.codex/config.toml` to Ghostex's approval level.
fn codex_thread_policy_to_approval_level(
    approval_policy: Option<&Value>,
    reviewer: Option<&str>,
    sandbox: Option<&Value>,
) -> AcpApprovalLevel {
    let sandbox_type = sandbox.and_then(|s| s.get("type")).and_then(Value::as_str);
    let is_never = match approval_policy {
        Some(Value::String(s)) => s.trim().eq_ignore_ascii_case("never"),
        Some(Value::Object(map)) => {
            map.get("granular")
                .and_then(Value::as_object)
                .is_some_and(|g| {
                    g.get("sandbox_approval").and_then(Value::as_bool) == Some(false)
                        && g.get("rules").and_then(Value::as_bool) == Some(false)
                        && g.get("request_permissions").and_then(Value::as_bool) == Some(false)
                })
        }
        _ => false,
    };
    let is_auto_reviewer = matches!(
        reviewer.map(str::trim),
        Some("auto_review" | "guardian_subagent")
    );
    if is_never && sandbox_type == Some("dangerFullAccess") {
        AcpApprovalLevel::FullAccess
    } else if is_auto_reviewer || is_never {
        AcpApprovalLevel::Auto
    } else {
        AcpApprovalLevel::Ask
    }
}

fn approval_level_rank(level: AcpApprovalLevel) -> u8 {
    match level {
        AcpApprovalLevel::Ask => 0,
        AcpApprovalLevel::AutoEdits => 1,
        AcpApprovalLevel::Auto => 2,
        AcpApprovalLevel::FullAccess => 3,
    }
}

/// Spawn a fresh `codex app-server --stdio` process, run `initialize`/`initialized`, populate
/// models if needed, and open/resume/fork the thread under the requested profile. Never falls back
/// to a deny-dropping `sandboxPolicy`: if `thread/resume` fails (other than `"no rollout found"` on
/// a thread with no turns yet), stops the child and returns `Err`.
#[allow(clippy::too_many_arguments)]
fn spawn_and_init_codex(
    cfg: &CodexSpawnConfig,
    resume_thread_id: Option<&str>,
    fork_session: bool,
    approval_level: AcpApprovalLevel,
    use_cli_default: bool,
    read_only: bool,
    profile: Option<&'static str>,
    state: &Arc<Mutex<CodexState>>,
    activity: &CodexActivityReporter,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<CodexTransport>>>>,
    process_gen: &Arc<AtomicU64>,
    my_gen: u64,
) -> Result<CodexProcess, String> {
    let mut child = super::child_lifetime::end_with_host(&mut Command::new(&cfg.prog))
        .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .envs(cfg.acp_envs.iter().cloned())
        .args(&cfg.argv)
        .current_dir(&cfg.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            format!(
                "Failed to start `{} {}`: {err}",
                cfg.prog,
                cfg.argv.join(" ")
            )
        })?;
    let mut lifetime =
        super::child_lifetime::ChildLifetime::attach(&child, cfg.record_target.as_ref());
    child_pid.store(child.id(), Ordering::Release);
    state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = child.id();
    let Some(stdin) = child.stdin.take() else {
        super::child_lifetime::force_kill_child(&mut child);
        lifetime.cleanup();
        child_pid.store(0, Ordering::Release);
        state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
        return Err("Codex app-server stdin unavailable.".to_string());
    };
    let Some(stdout) = child.stdout.take() else {
        super::child_lifetime::force_kill_child(&mut child);
        lifetime.cleanup();
        child_pid.store(0, Ordering::Release);
        state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
        return Err("Codex app-server stdout unavailable.".to_string());
    };
    let stderr = child.stderr.take();
    let transport = Arc::new(CodexTransport::new(stdin, &cfg.spool_session_id));
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

    spawn_codex_stdout_reader(
        stdout,
        transport.clone(),
        state.clone(),
        activity.clone(),
        process_gen.clone(),
        my_gen,
    );

    if let Err(err) = transport.call(
        "initialize",
        json!({
            "clientInfo": {
                "name": "ghostex",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": { "experimentalApi": true },
        }),
        CODEX_RPC_TIMEOUT,
    ) {
        // Bump generation before stopping so the reader's `CodexExitGuard` does not mark
        // `child_exited = true` and kill `acp-host` (keeping `fatalError` visible and
        // Switch to Terminal working).
        process_gen.fetch_add(1, Ordering::AcqRel);
        *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
        state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
        stop_codex_gracefully(&transport, &mut child, &mut lifetime, child_pid);
        return Err(format!("Codex app-server initialize failed: {err}"));
    }
    let _ = transport.notify("initialized", json!({}));

    let need_models = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .available_models
        .as_array()
        .is_none_or(Vec::is_empty);
    if need_models {
        if let Ok(list) = transport.call("model/list", json!({}), CODEX_RPC_TIMEOUT) {
            if let Some(data) = list.get("data").and_then(Value::as_array) {
                use ghostex_gx_chat_core::menus::catalog::{
                    is_placeholder_model_label, is_placeholder_model_value,
                };
                let mapped: Vec<Value> = data
                    .iter()
                    .filter_map(|m| {
                        let id = m
                            .get("model")
                            .or_else(|| m.get("id"))
                            .and_then(Value::as_str)
                            .filter(|v| !is_placeholder_model_value(v))?;
                        let raw_name = m
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or(id);
                        let name = if is_placeholder_model_label(raw_name) {
                            id
                        } else {
                            raw_name
                        };
                        Some(json!({
                            "modelId": id,
                            "name": name,
                            "supportedReasoningEfforts": m.get("supportedReasoningEfforts").cloned().unwrap_or(json!([])),
                            "defaultReasoningEffort": m.get("defaultReasoningEffort").cloned().unwrap_or(Value::Null),
                        }))
                    })
                    .collect();
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .available_models = Value::Array(mapped);
            }
        }
    }

    let (roots, model, dev_instr) = {
        use ghostex_gx_chat_core::menus::catalog::{
            family_and_version, is_placeholder_model_value,
        };
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.current_model
            .as_deref()
            .is_none_or(is_placeholder_model_value)
        {
            let avail = s.available_models.as_array();
            let newest_sol = avail.and_then(|models| {
                models
                    .iter()
                    .filter_map(|m| {
                        let id = m.get("modelId").and_then(Value::as_str)?;
                        let name = m.get("name").and_then(Value::as_str).unwrap_or(id);
                        let (fam, ver) =
                            family_and_version(name).or_else(|| family_and_version(id))?;
                        let words: Vec<&str> = fam.split_whitespace().collect();
                        words.contains(&"sol").then_some((id.to_string(), ver))
                    })
                    .max_by(|a, b| a.1.cmp(&b.1))
                    .map(|(id, _)| id)
            });
            let resolved = newest_sol
                .or_else(|| {
                    avail
                        .and_then(|models| models.first())
                        .and_then(|m| m.get("modelId"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .or_else(|| {
                    crate::agent_model_catalog::preferred_default_model("codex").map(|(v, _, _)| v)
                });
            if let Some(chosen) = resolved {
                s.current_model = Some(chosen);
            }
        }
        (
            s.writable_roots.clone(),
            s.current_model.clone(),
            s.developer_instructions.clone(),
        )
    };
    let keep_cli_default = use_cli_default && !read_only;
    let effective_profile = if keep_cli_default && profile == Some("ghostex-ws") {
        // When home-folder secret denies are active under Keep default, mirror the user's own
        // configured `sandbox_mode` (`danger-full-access` -> `ghostex-full`, else `ghostex-ws`).
        let cfg_sandbox = transport
            .call("config/read", json!({}), CODEX_RPC_TIMEOUT)
            .ok()
            .and_then(|res| {
                res.pointer("/config/sandbox_mode")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        if cfg_sandbox.as_deref() == Some("danger-full-access") {
            Some("ghostex-full")
        } else {
            profile
        }
    } else {
        profile
    };
    let (init_approval, init_reviewer, _) = codex_turn_policies(approval_level, read_only, &roots);
    let init_sandbox_mode = match approval_level {
        AcpApprovalLevel::FullAccess => "danger-full-access",
        AcpApprovalLevel::Ask if read_only => "read-only",
        AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits | AcpApprovalLevel::Auto => {
            "workspace-write"
        }
    };

    let open_fresh_thread = || -> Result<Value, String> {
        let mut start_params = Map::new();
        start_params.insert("cwd".to_string(), json!(cfg.cwd));
        if !keep_cli_default {
            start_params.insert("approvalPolicy".to_string(), init_approval.clone());
            start_params.insert("approvalsReviewer".to_string(), json!(init_reviewer));
        }
        if let Some(prof) = effective_profile {
            start_params.insert("config".to_string(), json!({ "default_permissions": prof }));
        } else if !keep_cli_default {
            start_params.insert("sandbox".to_string(), json!(init_sandbox_mode));
        }
        if let Some(m) = model.as_deref().filter(|s| !s.is_empty()) {
            start_params.insert("model".to_string(), json!(m));
        }
        if let Some(instr) = dev_instr.as_deref().filter(|s| !s.is_empty()) {
            start_params.insert("developerInstructions".to_string(), json!(instr));
        }
        transport.call(
            "thread/start",
            Value::Object(start_params),
            CODEX_RPC_TIMEOUT,
        )
    };

    let thread_res = if let Some(resume_id) =
        resume_thread_id.map(str::trim).filter(|s| !s.is_empty())
    {
        let method = if fork_session {
            "thread/fork"
        } else {
            "thread/resume"
        };
        let mut req_params = Map::new();
        req_params.insert("threadId".to_string(), json!(resume_id));
        req_params.insert("cwd".to_string(), json!(cfg.cwd));
        if !keep_cli_default {
            req_params.insert("approvalPolicy".to_string(), init_approval.clone());
            req_params.insert("approvalsReviewer".to_string(), json!(init_reviewer));
        }
        if let Some(prof) = effective_profile {
            req_params.insert("config".to_string(), json!({ "default_permissions": prof }));
        } else if !keep_cli_default {
            req_params.insert("sandbox".to_string(), json!(init_sandbox_mode));
        }
        if let Some(instr) = dev_instr.as_deref().filter(|s| !s.is_empty()) {
            req_params.insert("developerInstructions".to_string(), json!(instr));
        }
        match transport.call(method, Value::Object(req_params), CODEX_RPC_TIMEOUT) {
            Ok(res) => Ok(res),
            Err(err) if !fork_session && err.to_ascii_lowercase().contains("no rollout found") => {
                // Thread has never persisted a turn yet: start a fresh thread under the new profile.
                open_fresh_thread()
            }
            Err(err) => Err(err),
        }
    } else {
        open_fresh_thread()
    };

    let thread_res = match thread_res {
        Ok(res) => res,
        Err(err) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
            state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
            stop_codex_gracefully(&transport, &mut child, &mut lifetime, child_pid);
            return Err(format!("Codex thread open/resume failed: {err}"));
        }
    };

    let thread_id = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let tid = thread_res
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                if fork_session {
                    None
                } else {
                    resume_thread_id.map(str::to_string)
                }
            });
        s.thread_id = tid.clone();
        s.thread_path = thread_res
            .pointer("/thread/path")
            .and_then(Value::as_str)
            .map(str::to_string);
        if s.current_model
            .as_deref()
            .is_none_or(ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value)
        {
            s.current_model = thread_res
                .get("model")
                .and_then(Value::as_str)
                .filter(|m| !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m))
                .map(str::to_string);
        }
        if s.current_effort.is_none() {
            s.current_effort = thread_res
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .filter(|e| !e.trim().is_empty())
                .map(str::to_string)
                .or_else(|| {
                    let cur = s.current_model.as_deref()?;
                    s.available_models.as_array()?.iter().find_map(|m| {
                        (m.get("modelId").and_then(Value::as_str) == Some(cur))
                            .then(|| m.get("defaultReasoningEffort").and_then(Value::as_str))
                            .flatten()
                            .filter(|e| !e.trim().is_empty())
                            .map(str::to_string)
                    })
                })
                .or_else(|| {
                    crate::agent_model_catalog::resolve_acp_model(
                        "codex",
                        s.current_model.as_deref(),
                        None,
                        s.available_models.as_array().map(Vec::as_slice),
                    )
                    .and_then(|(_, _, effort)| effort)
                });
        }
        if keep_cli_default {
            s.approval_level = codex_thread_policy_to_approval_level(
                thread_res.get("approvalPolicy"),
                thread_res.get("approvalsReviewer").and_then(Value::as_str),
                thread_res.get("sandbox"),
            );
        }
        s.active_profile = effective_profile;
        s.pending_profile_reload = false;
        s.fatal_error = None;
        s.child_exited = false;
        tid
    };

    if fork_session && thread_id.is_none() {
        process_gen.fetch_add(1, Ordering::AcqRel);
        *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
        state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = 0;
        stop_codex_gracefully(&transport, &mut child, &mut lifetime, child_pid);
        return Err("Codex thread/fork did not return a forked thread id.".to_string());
    }

    Ok(CodexProcess {
        child,
        lifetime,
        transport,
    })
}

/// Wait briefly if `turn/start` is still in flight so `active_turn_id` can arrive, then cancel
/// any pending approvals, send `turn/interrupt`, and wait up to 5s for `turn/completed`.
fn interrupt_active_codex_turn(proc_opt: &Option<CodexProcess>, state: &Arc<Mutex<CodexState>>) {
    let id_wait_until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < id_wait_until {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        if !s.working || s.active_turn_id.is_some() || !s.awaiting_turn_start {
            break;
        }
        drop(s);
        thread::sleep(Duration::from_millis(50));
    }
    if let Some(proc) = proc_opt.as_ref() {
        cancel_all_pending_codex(&proc.transport, state);
        let (tid_opt, turn_id_opt, still_working) = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if s.working {
                s.last_turn_interrupted = true;
            }
            (s.thread_id.clone(), s.active_turn_id.clone(), s.working)
        };
        if still_working {
            let interrupted_ok = if let (Some(tid_int), Some(turn_id)) = (tid_opt, turn_id_opt) {
                if let Err(err) = proc.transport.call(
                    "turn/interrupt",
                    json!({ "threadId": tid_int, "turnId": turn_id }),
                    Duration::from_secs(5),
                ) {
                    eprintln!("[Codex Native] Warning: turn/interrupt failed: {err}");
                    false
                } else {
                    true
                }
            } else {
                false
            };
            if interrupted_ok {
                let wait_until = Instant::now() + Duration::from_secs(5);
                while Instant::now() < wait_until {
                    if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

/// Stop the current `codex app-server` (if running) under a new generation and restart + resume
/// the thread with the current server-generated approval level and deny profile. Fails closed: if
/// restarting/resuming fails, `*proc_opt` stays `None` and `s.fatal_error` records the error so
/// the session errors visibly rather than running under a looser profile.
#[allow(clippy::too_many_arguments)]
fn restart_codex_process(
    proc_opt: &mut Option<CodexProcess>,
    cfg: &CodexSpawnConfig,
    state: &Arc<Mutex<CodexState>>,
    activity: &CodexActivityReporter,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<CodexTransport>>>>,
    process_gen: &Arc<AtomicU64>,
) -> Result<(), String> {
    let new_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
    *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
    if let Some(mut old) = proc_opt.take() {
        cancel_all_pending_codex(&old.transport, state);
        stop_codex_gracefully(&old.transport, &mut old.child, &mut old.lifetime, child_pid);
    }
    let (resume_id, level, use_cli_default, read_only, next_profile) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.working = false;
        s.compacting = false;
        s.compacting_started_at = None;
        s.child_pid = 0;
        s.child_exited = false;
        s.fatal_error = None;
        s.awaiting_turn_start = false;
        s.active_turn_id = None;
        s.file_change_items.clear();
        s.pending_requests.clear();
        s.clear_live_turn_output();
        let prof = desired_codex_profile(s.deny_mode, s.approval_level, s.read_only);
        (
            s.thread_id.clone(),
            s.approval_level,
            s.use_cli_default,
            s.read_only,
            prof,
        )
    };
    match spawn_and_init_codex(
        cfg,
        resume_id.as_deref(),
        false,
        level,
        use_cli_default,
        read_only,
        next_profile,
        state,
        activity,
        child_pid,
        active_transport,
        process_gen,
        new_gen,
    ) {
        Ok(next_proc) => {
            *proc_opt = Some(next_proc);
            activity.sync();
            Ok(())
        }
        Err(err) => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.fatal_error = Some(err.clone());
                s.working = false;
            }
            activity.sync();
            Err(err)
        }
    }
}

/// Apply a new approval level on Codex deterministically:
/// 1. Mid-turn upgrade: record the new level and defer any profile change (`pending_profile_reload`)
///    to the next turn boundary without disrupting the running turn.
/// 2. Mid-turn downgrade: wait briefly if `awaiting_turn_start`, cancel pending approvals, interrupt
///    the running turn (`turn/interrupt`), and wait up to 5s for `turn/completed`. Fail closed: if
///    the turn does not end in time (or `turn/interrupt` fails / no turn ID arrived), immediately
///    stop the `app-server` process so the stronger policy cannot keep running.
/// 3. Idle (or after stopping `app-server`): if the server-generated deny profile changed or the
///    process was stopped, restart `app-server` and resume the thread under the new profile. Never
///    falls back to a deny-dropping `sandboxPolicy`; returns `Err` visibly on failure.
#[allow(clippy::too_many_arguments)]
fn apply_codex_approval_level(
    proc_opt: &mut Option<CodexProcess>,
    cfg: &CodexSpawnConfig,
    state: &Arc<Mutex<CodexState>>,
    activity: &CodexActivityReporter,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<CodexTransport>>>>,
    process_gen: &Arc<AtomicU64>,
    desired: AcpApprovalLevel,
) -> Result<(), String> {
    let (
        prev_level,
        prev_use_cli_default,
        effective_read_only,
        deny_mode,
        prev_profile,
        is_working,
    ) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let next_ro = if desired != AcpApprovalLevel::Ask {
            false
        } else {
            s.read_only
        };
        (
            s.approval_level,
            s.use_cli_default,
            next_ro,
            s.deny_mode,
            s.active_profile,
            s.working,
        )
    };
    let is_downgrade = approval_level_rank(desired) < approval_level_rank(prev_level)
        || (prev_use_cli_default && desired == AcpApprovalLevel::Ask);
    let next_profile = desired_codex_profile(deny_mode, desired, effective_read_only);

    if is_working && !is_downgrade {
        // Mid-turn upgrade: keep the running turn undisturbed and reload the profile cleanly at
        // the next turn boundary.
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.approval_level = desired;
        s.use_cli_default = false;
        s.read_only = effective_read_only;
        if next_profile != prev_profile {
            s.pending_profile_reload = true;
        }
        return Ok(());
    }

    let mut must_restart = false;
    if is_working && is_downgrade {
        interrupt_active_codex_turn(proc_opt, state);

        // Fail closed: if the stronger turn is still running after interrupt/wait, immediately
        // stop the app-server process so it cannot execute anything further under the stronger level.
        if state.lock().unwrap_or_else(|p| p.into_inner()).working {
            process_gen.fetch_add(1, Ordering::AcqRel);
            *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
            if let Some(mut old) = proc_opt.take() {
                cancel_all_pending_codex(&old.transport, state);
                stop_codex_gracefully(&old.transport, &mut old.child, &mut old.lifetime, child_pid);
            }
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.working = false;
            s.child_pid = 0;
            s.active_turn_id = None;
            s.awaiting_turn_start = false;
            s.file_change_items.clear();
            s.clear_live_turn_output();
            drop(s);
            must_restart = true;
        }
        activity.sync();
    }

    let need_reload = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.approval_level = desired;
        s.use_cli_default = false;
        s.read_only = effective_read_only;
        must_restart
            || proc_opt.is_none()
            || next_profile != s.active_profile
            || s.fatal_error.is_some()
    };

    if need_reload {
        restart_codex_process(
            proc_opt,
            cfg,
            state,
            activity,
            child_pid,
            active_transport,
            process_gen,
        )?;
    }
    Ok(())
}

/// Extract every mutated path (including `kind.move_path` / `kind.movePath`) from a Codex
/// `fileChange` `changes` payload. Never falls back to `grantRoot`, so `grantRoot` can never mask
/// the actual changed file paths during `Auto-edits` evaluation.
fn extract_codex_change_locations(changes: &Value) -> Vec<Value> {
    let mut locations = Vec::new();
    let push_path = |locs: &mut Vec<Value>, raw: &str| {
        let trimmed = raw.trim();
        if !trimmed.is_empty()
            && !locs
                .iter()
                .any(|l| l.get("path").and_then(Value::as_str) == Some(trimmed))
        {
            locs.push(json!({ "path": trimmed }));
        }
    };
    if let Some(arr) = changes.as_array() {
        for c in arr {
            if let Some(path) = c
                .get("path")
                .or_else(|| c.get("filePath"))
                .and_then(Value::as_str)
            {
                push_path(&mut locations, path);
            }
            if let Some(kind_obj) = c.get("kind").and_then(Value::as_object) {
                for move_key in ["move_path", "movePath"] {
                    if let Some(mp) = kind_obj.get(move_key).and_then(Value::as_str) {
                        push_path(&mut locations, mp);
                    }
                }
            }
            for move_key in ["move_path", "movePath"] {
                if let Some(mp) = c.get(move_key).and_then(Value::as_str) {
                    push_path(&mut locations, mp);
                }
            }
        }
    } else if let Some(obj) = changes.as_object() {
        for (key, val) in obj {
            push_path(&mut locations, key);
            if let Some(kind_obj) = val.get("kind").and_then(Value::as_object) {
                for move_key in ["move_path", "movePath"] {
                    if let Some(mp) = kind_obj.get(move_key).and_then(Value::as_str) {
                        push_path(&mut locations, mp);
                    }
                }
            }
        }
    }
    locations
}

/// Build Ghostex question items for a Codex `mcpServer/elicitation/request` (`mode`: `"form"`,
/// `"openai/form"`, or `"openaiForm"`), returning `(fields, questions)` while preserving exact
/// wire values (`const` / `enum`) separate from display labels (`title` / `enumNames`) and
/// handling `type: "array"` multi-select enums.
fn build_mcp_elicitation_questions(params: &Value) -> (Vec<McpElicitationField>, Vec<Value>) {
    let mut fields = Vec::new();
    let mut questions = Vec::new();
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("MCP server requests input");
    let mode = params.get("mode").and_then(Value::as_str).unwrap_or("");
    if !matches!(mode, "form" | "openai/form" | "openaiForm") {
        return (fields, questions);
    }
    let Some(props) = params
        .pointer("/requestedSchema/properties")
        .and_then(Value::as_object)
    else {
        return (fields, questions);
    };
    for (key, schema) in props {
        let ftype = schema
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("string")
            .to_string();
        let title = schema
            .get("title")
            .or_else(|| schema.get("description"))
            .and_then(Value::as_str)
            .unwrap_or(key.as_str());
        let mut options = Vec::new();
        let mut option_values = Vec::new();
        let mut multi_select = false;

        if ftype == "boolean" {
            options.push(json!({ "label": "Yes" }));
            option_values.push("true".to_string());
            options.push(json!({ "label": "No" }));
            option_values.push("false".to_string());
        } else if ftype == "array" {
            multi_select = true;
            if let Some(items) = schema.get("items").and_then(Value::as_object) {
                if let Some(enum_vals) = items.get("enum").and_then(Value::as_array) {
                    let names = items.get("enumNames").and_then(Value::as_array);
                    for (idx, v) in enum_vals.iter().enumerate() {
                        if let Some(wire) = v.as_str() {
                            let label = names
                                .and_then(|n| n.get(idx))
                                .and_then(Value::as_str)
                                .unwrap_or(wire);
                            options.push(json!({ "label": label }));
                            option_values.push(wire.to_string());
                        }
                    }
                } else if let Some(choices) = items
                    .get("anyOf")
                    .or_else(|| items.get("oneOf"))
                    .and_then(Value::as_array)
                {
                    for item in choices {
                        if let Some(wire) = item
                            .get("const")
                            .or_else(|| item.get("title"))
                            .and_then(Value::as_str)
                        {
                            let label = item.get("title").and_then(Value::as_str).unwrap_or(wire);
                            options.push(json!({ "label": label }));
                            option_values.push(wire.to_string());
                        }
                    }
                }
            }
        } else if let Some(enum_vals) = schema.get("enum").and_then(Value::as_array) {
            let names = schema.get("enumNames").and_then(Value::as_array);
            for (idx, v) in enum_vals.iter().enumerate() {
                if let Some(wire) = v.as_str() {
                    let label = names
                        .and_then(|n| n.get(idx))
                        .and_then(Value::as_str)
                        .unwrap_or(wire);
                    options.push(json!({ "label": label }));
                    option_values.push(wire.to_string());
                }
            }
        } else if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
            for item in one_of {
                if let Some(wire) = item
                    .get("const")
                    .or_else(|| item.get("title"))
                    .and_then(Value::as_str)
                {
                    let label = item.get("title").and_then(Value::as_str).unwrap_or(wire);
                    options.push(json!({ "label": label }));
                    option_values.push(wire.to_string());
                }
            }
        }
        fields.push(McpElicitationField {
            key: key.clone(),
            field_type: ftype,
            multi_select,
            option_values,
        });
        questions.push(json!({
            "id": key,
            "header": title,
            "question": format!("{message}: {title}"),
            "multiSelect": multi_select,
            "isOther": options.is_empty(),
            "options": options,
        }));
    }
    (fields, questions)
}

fn build_mcp_elicitation_response(
    fields: &[McpElicitationField],
    questions: &[Value],
    selections: &[Value],
) -> Value {
    let mut content = Map::new();
    for (idx, field) in fields.iter().enumerate() {
        let Some(sel) = selections.get(idx) else {
            continue;
        };
        let options = questions
            .get(idx)
            .and_then(|q| q.get("options"))
            .and_then(Value::as_array);
        let wire_for_index = |i: usize| -> Option<String> {
            field.option_values.get(i).cloned().or_else(|| {
                options
                    .and_then(|o| o.get(i))
                    .and_then(|o| o.get("label").and_then(Value::as_str))
                    .map(str::to_string)
            })
        };
        if field.field_type == "array" || field.multi_select {
            let mut vals = Vec::new();
            if let Some(indices) = sel.get("indices").and_then(Value::as_array) {
                for i in indices.iter().filter_map(Value::as_u64) {
                    if let Some(wire) = wire_for_index(i as usize) {
                        vals.push(Value::String(wire));
                    }
                }
            }
            if let Some(other) = sel
                .get("other")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                vals.push(Value::String(other.to_string()));
            }
            if !vals.is_empty() {
                content.insert(field.key.clone(), Value::Array(vals));
            }
            continue;
        }
        let raw_val = sel
            .get("other")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                sel.get("indices")
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(Value::as_u64)
                    .and_then(|i| wire_for_index(i as usize))
            });
        let Some(val) = raw_val else {
            continue;
        };
        let json_val = match field.field_type.as_str() {
            "boolean" => Value::Bool(matches!(
                val.to_ascii_lowercase().as_str(),
                "yes" | "true" | "1"
            )),
            "integer" => val
                .parse::<i64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(val)),
            "number" => val
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or_else(|| Value::String(val)),
            _ => Value::String(val),
        };
        content.insert(field.key.clone(), json_val);
    }
    if content.is_empty() {
        json!({ "action": "decline" })
    } else {
        json!({ "action": "accept", "content": Value::Object(content) })
    }
}

fn build_codex_user_input_answers(questions: &[Value], selections: &[Value]) -> Value {
    let mut answers = Map::new();
    for (idx, q) in questions.iter().enumerate() {
        let q_id = q.get("id").and_then(Value::as_str).unwrap_or_default();
        if q_id.is_empty() {
            continue;
        }
        let Some(sel) = selections.get(idx) else {
            continue;
        };
        let mut vals = Vec::new();
        let options = q.get("options").and_then(Value::as_array);
        if let Some(indices) = sel.get("indices").and_then(Value::as_array) {
            for i in indices.iter().filter_map(Value::as_u64) {
                if let Some(label) = options
                    .and_then(|arr| arr.get(i as usize))
                    .and_then(|opt| opt.get("label").and_then(Value::as_str))
                {
                    vals.push(json!(label));
                }
            }
        }
        if let Some(other) = sel
            .get("other")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            vals.push(json!(other));
        }
        if !vals.is_empty() {
            answers.insert(q_id.to_string(), json!({ "answers": vals }));
        }
    }
    json!({ "answers": Value::Object(answers) })
}

fn cancel_all_pending_codex(transport: &CodexTransport, state: &Mutex<CodexState>) {
    let drained: Vec<PendingCodexRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        match req.kind {
            CodexRequestKind::CommandApproval | CodexRequestKind::FileChangeApproval => {
                let _ = transport.respond(req.rpc_id, json!({ "decision": "cancel" }));
            }
            CodexRequestKind::PermissionsApproval { .. } => {
                let _ = transport.respond(req.rpc_id, json!({ "permissions": {} }));
            }
            CodexRequestKind::UserInput { .. } => {
                let _ = transport.respond(req.rpc_id, json!({ "answers": {} }));
            }
            CodexRequestKind::McpElicitation { .. } => {
                let _ = transport.respond(req.rpc_id, json!({ "action": "cancel" }));
            }
        }
    }
}

fn stop_codex_gracefully(
    transport: &CodexTransport,
    child: &mut Child,
    lifetime: &mut super::child_lifetime::ChildLifetime,
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
                    super::child_lifetime::force_kill_child(child);
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

fn spawn_codex_stdout_reader(
    stdout: std::process::ChildStdout,
    t_out: Arc<CodexTransport>,
    st_out: Arc<Mutex<CodexState>>,
    act_out: CodexActivityReporter,
    gen_out: Arc<AtomicU64>,
    my_gen: u64,
) {
    thread::spawn(move || {
        let _exit_guard = CodexExitGuard {
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
                }
                let has_method = msg.get("method").and_then(Value::as_str).is_some();
                let has_id = msg.get("id").is_some();

                // RPC response to our call.
                if !has_method && has_id {
                    if let Some(id) = msg.get("id").and_then(Value::as_u64) {
                        if let Some(tx) = t_out
                            .pending
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&id)
                        {
                            let _ = tx.send(msg);
                        }
                    }
                    return;
                }

                // Server-initiated request requiring a response (approvals / questions).
                if has_method && has_id {
                    let method = msg
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
                    let params = msg.get("params").cloned().unwrap_or(Value::Null);
                    let allow_opts = vec![
                        AcpPermissionOption {
                            option_id: "accept".to_string(),
                            name: "Allow".to_string(),
                            kind: "allow_once".to_string(),
                        },
                        AcpPermissionOption {
                            option_id: "decline".to_string(),
                            name: "Deny".to_string(),
                            kind: "reject_once".to_string(),
                        },
                    ];

                    match method {
                        "item/commandExecution/requestApproval" => {
                            let command = params
                                .get("command")
                                .and_then(Value::as_str)
                                .unwrap_or("shell command")
                                .to_string();
                            let tool_use_id = params
                                .get("itemId")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let (level, roots, cwd) = {
                                let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                (s.approval_level, s.writable_roots.clone(), s.cwd.clone())
                            };
                            let synthetic = json!({
                                "toolCall": {
                                    "name": "Bash",
                                    "kind": "execute",
                                    "rawInput": { "command": command },
                                }
                            });
                            let reason = match evaluate_permission(
                                level,
                                &synthetic,
                                &allow_opts,
                                &roots,
                                &cwd,
                            ) {
                                PermissionDecision::AutoApprove { .. } => {
                                    println!(
                                        "\r\n[Codex Native] Auto-approved ({}): Bash ({})",
                                        level.label(),
                                        command
                                    );
                                    let _ = std::io::stdout().flush();
                                    let _ = t_out.respond(rpc_id, json!({ "decision": "accept" }));
                                    return;
                                }
                                PermissionDecision::AskUser { reason } => reason,
                            };
                            let pending = PendingCodexRequest {
                                rpc_id,
                                kind: CodexRequestKind::CommandApproval,
                                tool_name: "Bash".to_string(),
                                summary: command,
                                tool_use_id,
                                reason,
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
                        }
                        "item/fileChange/requestApproval" => {
                            let tool_use_id = params
                                .get("itemId")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            // Always consult the full cached `item/started` `changes` payload
                            // first (never letting `grantRoot` hide actual changed paths or
                            // `kind.move_path`), falling back to `params.changes` / `params.files`
                            // only when no cached item exists. If neither is present, fail
                            // closed (`locations: []`, `rawInput: {}`) so `evaluate_permission`
                            // asks the user.
                            let (cached_changes, level, roots, cwd) = {
                                let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                let cached = tool_use_id
                                    .as_deref()
                                    .and_then(|id| s.file_change_items.get(id))
                                    .cloned();
                                (
                                    cached,
                                    s.approval_level,
                                    s.writable_roots.clone(),
                                    s.cwd.clone(),
                                )
                            };
                            let changes_val = cached_changes
                                .or_else(|| params.get("changes").cloned())
                                .or_else(|| params.get("files").cloned());
                            let locations = changes_val
                                .as_ref()
                                .map(extract_codex_change_locations)
                                .unwrap_or_default();
                            let raw_input = match changes_val {
                                Some(c) if !c.is_null() => json!({ "changes": c }),
                                _ => json!({}),
                            };
                            let summary = locations
                                .first()
                                .and_then(|l| l.get("path"))
                                .and_then(Value::as_str)
                                .or_else(|| params.get("reason").and_then(Value::as_str))
                                .unwrap_or("file changes")
                                .to_string();
                            let synthetic = json!({
                                "toolCall": {
                                    "name": "Edit",
                                    "kind": "edit",
                                    "rawInput": raw_input,
                                    "locations": locations,
                                }
                            });
                            let reason = match evaluate_permission(
                                level,
                                &synthetic,
                                &allow_opts,
                                &roots,
                                &cwd,
                            ) {
                                PermissionDecision::AutoApprove { .. } => {
                                    println!(
                                        "\r\n[Codex Native] Auto-approved ({}): Edit ({})",
                                        level.label(),
                                        summary
                                    );
                                    let _ = std::io::stdout().flush();
                                    let _ = t_out.respond(rpc_id, json!({ "decision": "accept" }));
                                    return;
                                }
                                PermissionDecision::AskUser { reason } => reason,
                            };
                            let pending = PendingCodexRequest {
                                rpc_id,
                                kind: CodexRequestKind::FileChangeApproval,
                                tool_name: "Edit".to_string(),
                                summary,
                                tool_use_id,
                                reason,
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
                        }
                        "item/permissions/requestApproval" => {
                            let level = st_out
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .approval_level;
                            let permissions =
                                params.get("permissions").cloned().unwrap_or(json!({}));
                            if level == AcpApprovalLevel::FullAccess {
                                // Always scope `"turn"` (never `"session"`).
                                let _ = t_out.respond(
                                    rpc_id,
                                    json!({ "permissions": permissions, "scope": "turn" }),
                                );
                                return;
                            }
                            let summary = params
                                .get("reason")
                                .and_then(Value::as_str)
                                .unwrap_or("Additional permissions requested")
                                .to_string();
                            let tool_use_id = params
                                .get("itemId")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let pending = PendingCodexRequest {
                                rpc_id,
                                kind: CodexRequestKind::PermissionsApproval { permissions },
                                tool_name: "Permissions".to_string(),
                                summary,
                                tool_use_id,
                                reason: None,
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
                        }
                        "item/tool/requestUserInput" => {
                            let questions = params
                                .get("questions")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default();
                            let tool_use_id = params
                                .get("itemId")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let summary = questions
                                .first()
                                .and_then(|q| q.get("question"))
                                .and_then(Value::as_str)
                                .unwrap_or("Question")
                                .to_string();
                            let pending = PendingCodexRequest {
                                rpc_id,
                                kind: CodexRequestKind::UserInput { questions },
                                tool_name: "requestUserInput".to_string(),
                                summary,
                                tool_use_id,
                                reason: None,
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
                        }
                        "mcpServer/elicitation/request" => {
                            let (fields, questions) = build_mcp_elicitation_questions(&params);
                            let server_name = params
                                .get("serverName")
                                .and_then(Value::as_str)
                                .unwrap_or("MCP server");
                            let message = params
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Input requested");
                            let url = params
                                .get("url")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty());
                            let summary = match url {
                                Some(u) => format!("{server_name}: {message} ({u})"),
                                None => format!("{server_name}: {message}"),
                            };
                            let tool_use_id = params
                                .get("elicitationId")
                                .or_else(|| params.get("itemId"))
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let pending = PendingCodexRequest {
                                rpc_id,
                                kind: CodexRequestKind::McpElicitation { fields, questions },
                                tool_name: format!("MCP ({server_name})"),
                                summary,
                                tool_use_id,
                                reason: None,
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
                        }
                        other => {
                            t_out.respond_error(
                                rpc_id,
                                -32601,
                                &format!("Unsupported Codex client method: {other}"),
                            );
                        }
                    }
                    return;
                }

                // Notifications from Codex app-server.
                let method = msg
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let params = msg.get("params").unwrap_or(&Value::Null);
                match method {
                    "turn/started" => {
                        let tid = params
                            .pointer("/turn/id")
                            .or_else(|| params.get("turnId"))
                            .and_then(Value::as_str);
                        let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(tid_str) = tid {
                            if s.ignored_turn_ids.iter().any(|id| id == tid_str) {
                                return;
                            }
                            s.active_turn_id = Some(tid_str.to_string());
                        }
                        s.awaiting_turn_start = false;
                        s.working = true;
                        drop(s);
                        act_out.sync();
                    }
                    "turn/completed" => {
                        let completed_tid = params
                            .pointer("/turn/id")
                            .or_else(|| params.get("turnId"))
                            .and_then(Value::as_str);
                        let turn_status = params
                            .pointer("/turn/status")
                            .or_else(|| params.get("status"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                        let turn_interrupted = matches!(
                            turn_status.as_str(),
                            "interrupted" | "cancelled" | "canceled" | "aborted"
                        );
                        let turn_error = params
                            .pointer("/turn/error")
                            .or_else(|| params.get("error"))
                            .filter(|v| !v.is_null())
                            .and_then(|err| {
                                let msg = err
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|m| !m.is_empty())
                                    .map(str::to_string)?;
                                let code = codex_error_info_code(err.get("codexErrorInfo"));
                                Some((code, msg))
                            });
                        let turn_failed = turn_status == "failed" || turn_error.is_some();
                        let (stale_turn, orphaned): (bool, Vec<PendingCodexRequest>) = {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            if let Some(done_id) = completed_tid {
                                push_capped_turn_id(&mut s.completed_turn_ids, done_id.to_string());
                                if remove_turn_id(&mut s.ignored_turn_ids, done_id) {
                                    (true, Vec::new())
                                } else if s
                                    .active_turn_id
                                    .as_deref()
                                    .is_some_and(|active| active != done_id)
                                {
                                    (true, Vec::new())
                                } else if s.active_turn_id.is_none() && s.awaiting_turn_start {
                                    // An untracked old turn completed while a new `turn/start`
                                    // RPC is in flight (before `turn/started` for the new turn
                                    // arrived): do not clobber the new turn.
                                    (true, Vec::new())
                                } else {
                                    s.working = false;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    if turn_interrupted {
                                        s.last_turn_interrupted = true;
                                    }
                                    if let Some((code, msg)) = turn_error {
                                        s.last_turn_stop_error = Some(
                                            code.clone().unwrap_or_else(|| "error".to_string()),
                                        );
                                        s.last_error = Some((code, msg));
                                    } else if turn_failed && s.last_turn_stop_error.is_none() {
                                        s.last_turn_stop_error = Some("error".to_string());
                                    } else if !turn_failed && s.last_turn_stop_error.is_none() {
                                        s.last_error = None;
                                    }
                                    s.awaiting_turn_start = false;
                                    s.active_turn_id = None;
                                    s.file_change_items.clear();
                                    s.clear_live_turn_output();
                                    (false, s.pending_requests.drain(..).collect())
                                }
                            } else if s.awaiting_turn_start {
                                (true, Vec::new())
                            } else {
                                s.working = false;
                                s.compacting = false;
                                s.compacting_started_at = None;
                                if turn_interrupted {
                                    s.last_turn_interrupted = true;
                                }
                                if let Some((code, msg)) = turn_error {
                                    s.last_turn_stop_error =
                                        Some(code.clone().unwrap_or_else(|| "error".to_string()));
                                    s.last_error = Some((code, msg));
                                } else if turn_failed && s.last_turn_stop_error.is_none() {
                                    s.last_turn_stop_error = Some("error".to_string());
                                } else if !turn_failed && s.last_turn_stop_error.is_none() {
                                    s.last_error = None;
                                }
                                s.awaiting_turn_start = false;
                                s.active_turn_id = None;
                                s.file_change_items.clear();
                                s.clear_live_turn_output();
                                (false, s.pending_requests.drain(..).collect())
                            }
                        };
                        if stale_turn {
                            return;
                        }
                        for req in orphaned {
                            match req.kind {
                                CodexRequestKind::CommandApproval
                                | CodexRequestKind::FileChangeApproval => {
                                    let _ =
                                        t_out.respond(req.rpc_id, json!({ "decision": "cancel" }));
                                }
                                CodexRequestKind::PermissionsApproval { .. } => {
                                    let _ = t_out.respond(req.rpc_id, json!({ "permissions": {} }));
                                }
                                CodexRequestKind::UserInput { .. } => {
                                    let _ = t_out.respond(req.rpc_id, json!({ "answers": {} }));
                                }
                                CodexRequestKind::McpElicitation { .. } => {
                                    let _ =
                                        t_out.respond(req.rpc_id, json!({ "action": "cancel" }));
                                }
                            }
                        }
                        println!("[Codex Native] Turn finished.");
                        let _ = std::io::stdout().flush();
                        act_out.sync();
                    }
                    "item/agentMessage/delta" => {
                        if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            if s.live_stream_started_at.is_none() {
                                s.live_stream_started_at = Some(now_iso_rfc3339());
                            }
                            append_bounded(&mut s.live_stream_text, delta, LIVE_STREAM_MAX_CHARS);
                            s.live_tool_label = None;
                            s.live_tool_detail = None;
                            s.live_tool_started_at = None;
                        }
                    }
                    "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                        if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            if s.live_thought_started_at.is_none() {
                                s.live_thought_started_at = Some(now_iso_rfc3339());
                            }
                            append_bounded(&mut s.live_thought_text, delta, LIVE_STREAM_MAX_CHARS);
                        }
                    }
                    "item/started" => {
                        if let Some(item) = params.get("item") {
                            let itype =
                                item.get("type").and_then(Value::as_str).unwrap_or_default();
                            if matches!(itype, "commandExecution" | "fileChange" | "mcpToolCall") {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if itype == "fileChange" {
                                    if let Some(item_id) = item.get("id").and_then(Value::as_str) {
                                        if let Some(changes) = item.get("changes") {
                                            s.file_change_items
                                                .insert(item_id.to_string(), changes.clone());
                                        }
                                    }
                                }
                                s.live_tool_label = Some(itype.to_string());
                                s.live_tool_detail = item
                                    .get("command")
                                    .or_else(|| item.get("tool"))
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                s.live_tool_started_at = Some(now_iso_rfc3339());
                            }
                        }
                    }
                    "item/completed" => {
                        let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(item_id) = params
                            .pointer("/item/id")
                            .or_else(|| params.get("itemId"))
                            .and_then(Value::as_str)
                        {
                            s.file_change_items.remove(item_id);
                        }
                        s.live_tool_label = None;
                        s.live_tool_detail = None;
                        s.live_tool_started_at = None;
                    }
                    "thread/tokenUsage/updated" => {
                        if let Some(tu) = params.get("tokenUsage") {
                            let used = tu
                                .pointer("/last/totalTokens")
                                .or_else(|| tu.pointer("/total/totalTokens"))
                                .and_then(Value::as_u64);
                            let window = tu.get("modelContextWindow").and_then(Value::as_u64);
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            if let Some(u) = used.filter(|v| *v > 0) {
                                s.usage_used_tokens = Some(u);
                            }
                            if let Some(w) = window.filter(|v| *v > 0) {
                                s.usage_window_size = Some(w);
                            }
                        }
                    }
                    "error" => {
                        let will_retry = params
                            .get("willRetry")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        if !will_retry {
                            let err_obj = params.get("error").unwrap_or(params);
                            let msg = err_obj
                                .get("message")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|m| !m.is_empty())
                                .map(str::to_string);
                            let code = codex_error_info_code(err_obj.get("codexErrorInfo"));
                            if let Some(msg) = msg {
                                eprintln!("[Codex Native] API error: {msg}");
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.last_turn_stop_error =
                                    Some(code.clone().unwrap_or_else(|| "error".to_string()));
                                s.last_error = Some((code, msg));
                            }
                        }
                    }
                    "thread/compacted" => {
                        let should_sync = {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            s.compacting = false;
                            s.compacting_started_at = None;
                            if s.working && s.active_turn_id.is_none() {
                                s.working = false;
                                true
                            } else {
                                false
                            }
                        };
                        if should_sync {
                            act_out.sync();
                        }
                    }
                    "thread/name/updated" => {
                        if let Some(name) = params
                            .get("threadName")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                        {
                            st_out
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .session_title = Some(name.to_string());
                        }
                    }
                    _ => {}
                }
            }));
            if res.is_err() {
                eprintln!("[Codex Native] Warning: recovered from panic in stdout reader.");
            }
        }
    });
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

    let Some((acp_envs, prog, mut argv)) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
    else {
        let msg = "Invalid Codex Native command.";
        eprintln!("{msg}");
        super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, msg, false);
        return 1;
    };
    // Synthesize Ghostex's own server-side Codex flags strictly from `parsed.profile` and Ghostex
    // settings (never from untrusted command strings).
    for (k, v) in &parsed.profile.connector_envs {
        if is_allowed_connector_env_key(k) {
            argv.push("-c".to_string());
            argv.push(format!(
                "shell_environment_policy.set.{k}={}",
                serde_json::to_string(v).unwrap_or_default()
            ));
        }
    }
    if parsed.profile.gbot_computer_use_off {
        let codex_home = acp_envs
            .iter()
            .find(|(k, _)| k == "CODEX_HOME")
            .map(|(_, v)| PathBuf::from(v));
        for srv in crate::agents::codex_disabled_screen_servers(codex_home.as_deref()) {
            argv.push("-c".to_string());
            argv.push(format!("mcp_servers.{srv}.enabled=false"));
        }
    }
    if parsed.profile.lean_worker {
        for feat in crate::agents::lean_codex_disabled_features_if_enabled() {
            argv.push("--disable".to_string());
            argv.push(feat);
        }
    }
    argv.extend(crate::connectors::native_connector_tool_args("codex"));

    let use_cli_default = parsed.approval_level.is_none() && !parsed.read_only;
    let initial_approval_level = if parsed.read_only {
        AcpApprovalLevel::Ask
    } else {
        parsed.approval_level.unwrap_or(AcpApprovalLevel::Ask)
    };
    let (deny_mode, init_profile) = if let Some((is_api, deny_paths)) =
        crate::agents::codex_deny_paths_for_session(
            Some(&cwd),
            parsed.profile.gbot_chat,
            parsed.profile.gbot_api_chat,
        ) {
        let denies: Vec<String> = deny_paths
            .iter()
            .map(|p| format!("{}=\"deny\"", crate::agents::codex_toml_string(p)))
            .collect();
        let mode = if is_api {
            let mut ro_entries = vec!["\"/\"=\"read\"".to_string()];
            ro_entries.extend(denies.iter().cloned());
            let mut ws_entries = vec!["\"/\"=\"read\"".to_string()];
            if !cwd.trim().is_empty() {
                ws_entries.push(format!(
                    "{}=\"write\"",
                    crate::agents::codex_toml_string(&cwd)
                ));
            }
            ws_entries.extend(denies.iter().cloned());
            argv.push("-c".to_string());
            argv.push(format!(
                "permissions.ghostex-api-ro={{filesystem={{{}}}, network={{enabled=false}}}}",
                ro_entries.join(", ")
            ));
            argv.push("-c".to_string());
            argv.push(format!(
                "permissions.ghostex-api-ws={{filesystem={{{}}}, network={{enabled=true}}}}",
                ws_entries.join(", ")
            ));
            CodexDenyMode::GbotApi
        } else {
            let mut ro_entries = vec!["\"/\"=\"read\"".to_string()];
            ro_entries.extend(denies.iter().cloned());
            let full_fs = crate::agents::codex_bypass_filesystem(
                Some(&cwd),
                &deny_paths,
                cfg!(target_os = "linux"),
            );
            argv.push("-c".to_string());
            argv.push(format!(
                "permissions.ghostex-ro={{filesystem={{{}}}, network={{enabled=false}}}}",
                ro_entries.join(", ")
            ));
            argv.push("-c".to_string());
            argv.push(format!(
                "permissions.ghostex-ws={{extends=\":workspace\", filesystem={{{}}}, network={{enabled=false}}}}",
                denies.join(", ")
            ));
            argv.push("-c".to_string());
            argv.push(format!(
                "permissions.ghostex-full={{filesystem={{{full_fs}}}, network={{enabled=true}}}}"
            ));
            CodexDenyMode::HomeOrGbot
        };
        let prof = desired_codex_profile(mode, initial_approval_level, parsed.read_only);
        (mode, prof)
    } else {
        (CodexDenyMode::None, None)
    };

    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);
    let spawn_cfg = CodexSpawnConfig {
        prog,
        argv,
        acp_envs,
        cwd: cwd.clone(),
        spool_session_id,
        record_target: record_target.clone(),
    };

    let file_instructions = parsed
        .profile
        .append_system_prompt_file
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.trim().to_string())
        .filter(|s| !s.is_empty());
    let combined_developer_instructions = match (
        parsed
            .profile
            .developer_instructions
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        file_instructions.as_deref(),
    ) {
        (Some(inline), Some(file_text)) => Some(format!("{inline}\n\n{file_text}")),
        (Some(inline), None) => Some(inline.to_string()),
        (None, Some(file_text)) => Some(file_text.to_string()),
        (None, None) => None,
    };

    let state = Arc::new(Mutex::new(CodexState::new(
        initial_approval_level,
        use_cli_default,
        parsed.read_only,
        deny_mode,
        init_profile,
        combined_developer_instructions,
        cwd_path.clone(),
        writable_roots,
        parsed.model.clone(),
        parsed.effort.clone(),
    )));
    let activity = CodexActivityReporter::start(endpoint.clone(), state.clone());
    let shutting_down = Arc::new(AtomicBool::new(false));
    let child_pid = Arc::new(AtomicU32::new(0));
    let active_transport: Arc<Mutex<Option<Arc<CodexTransport>>>> = Arc::new(Mutex::new(None));
    let process_gen = Arc::new(AtomicU64::new(1));

    let mut proc_opt = match spawn_and_init_codex(
        &spawn_cfg,
        parsed.resume_session_id.as_deref(),
        parsed.fork_session,
        initial_approval_level,
        use_cli_default,
        parsed.read_only,
        init_profile,
        &state,
        &activity,
        &child_pid,
        &active_transport,
        &process_gen,
        1,
    ) {
        Ok(proc) => Some(proc),
        Err(err) => {
            eprintln!("{err}");
            super::host::report_driver_startup_error(
                endpoint.as_ref(),
                spec.family,
                &err,
                super::startup_error::looks_like_auth_error(&err),
            );
            return 1;
        }
    };

    #[cfg(unix)]
    {
        let pid_sig = child_pid.clone();
        let transport_sig = active_transport.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
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
                    super::child_lifetime::terminate_child_on_host_signal(pid, target_sig.as_ref());
                    if let Some(ep) = ep_sig.as_ref() {
                        ep.close_channel();
                    }
                    std::process::exit(143);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    let thread_id = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .thread_id
        .clone();
    println!(
        "Ghostex Native Host · {} · Ready (thread: {})",
        spec.display_name,
        thread_id.as_deref().unwrap_or("pending")
    );
    let _ = std::io::stdout().flush();
    activity.sync();

    // 4. Main `/api/agentChatChannel` long-poll loop.
    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
        "mid_turn_send",
        "steer",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "switch_to_cli",
        "rewind"
    ]);
    let channel_agent = format!("acp:{}", spec.family);
    let mut reconnecting = false;
    let mut crash_restarts: VecDeque<Instant> = VecDeque::new();

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
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
                println!("\r\n[Codex Native] Agent process exited repeatedly; stopping host.");
                break;
            }
            crash_restarts.push_back(now);
            println!("\r\n[Codex Native] Agent process exited unexpectedly; restarting...");
            let _ = std::io::stdout().flush();
            thread::sleep(Duration::from_millis(500));
            match restart_codex_process(
                &mut proc_opt,
                &spawn_cfg,
                &state,
                &activity,
                &child_pid,
                &active_transport,
                &process_gen,
            ) {
                Ok(()) => {
                    println!(
                        "\r\n[Codex Native] Agent process restarted and conversation resumed."
                    );
                    let _ = std::io::stdout().flush();
                    activity.sync();
                    continue;
                }
                Err(err) => {
                    eprintln!("[Codex Native] Failed to restart crashed agent process: {err}");
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
                let newly_registered = resp
                    .pointer("/result/newlyRegistered")
                    .or_else(|| resp.get("newlyRegistered"))
                    .and_then(Value::as_bool)
                    == Some(true);
                let mut had_desired_approval = false;
                if let Some(desired) = resp
                    .pointer("/result/desiredApprovalLevel")
                    .or_else(|| resp.get("desiredApprovalLevel"))
                    .and_then(Value::as_str)
                    .and_then(AcpApprovalLevel::parse)
                {
                    had_desired_approval = true;
                    let (current, is_cli_default) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.approval_level, s.use_cli_default)
                    };
                    if desired != current || is_cli_default {
                        if let Err(err) = apply_codex_approval_level(
                            &mut proc_opt,
                            &spawn_cfg,
                            &state,
                            &activity,
                            &child_pid,
                            &active_transport,
                            &process_gen,
                            desired,
                        ) {
                            eprintln!(
                                "[Codex Native] Warning: could not sync approval level: {err}"
                            );
                        }
                    }
                }
                if reconnecting || newly_registered || had_desired_approval {
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
                    interrupt_active_codex_turn(&proc_opt, &state);
                    if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                        if let Err(err) = restart_codex_process(
                            &mut proc_opt,
                            &spawn_cfg,
                            &state,
                            &activity,
                            &child_pid,
                            &active_transport,
                            &process_gen,
                        ) {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    }
                    activity.sync();
                    ep.ack(id, true, json!({ "aborted": true }), None);
                }
                "approve" => {
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex process is not running."),
                        );
                        continue;
                    };
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
                                    !p.is_question() && p.tool_use_id.as_deref() == Some(wanted)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|p| !p.is_question())
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        ep.ack(id, false, Value::Null, Some("No pending Codex approval."));
                        continue;
                    };
                    let norm = decision.trim().to_ascii_lowercase();
                    let is_deny = matches!(
                        norm.as_str(),
                        "deny"
                            | "reject"
                            | "reject_once"
                            | "reject_always"
                            | "decline"
                            | "escape"
                            | "cancel"
                            | "\x1b"
                            | "n"
                            | "no"
                    );
                    let res = match req.kind {
                        CodexRequestKind::CommandApproval
                        | CodexRequestKind::FileChangeApproval => {
                            let val = if is_deny { "decline" } else { "accept" };
                            proc.transport
                                .respond(req.rpc_id, json!({ "decision": val }))
                        }
                        CodexRequestKind::PermissionsApproval { permissions } => {
                            if is_deny {
                                proc.transport
                                    .respond(req.rpc_id, json!({ "permissions": {} }))
                            } else {
                                proc.transport.respond(
                                    req.rpc_id,
                                    json!({ "permissions": permissions, "scope": "turn" }),
                                )
                            }
                        }
                        CodexRequestKind::McpElicitation { .. } => {
                            let action = if is_deny { "decline" } else { "accept" };
                            proc.transport
                                .respond(req.rpc_id, json!({ "action": action }))
                        }
                        CodexRequestKind::UserInput { .. } => {
                            proc.transport.respond(req.rpc_id, json!({ "answers": {} }))
                        }
                    };
                    match res {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "answer_question" => {
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex process is not running."),
                        );
                        continue;
                    };
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
                                    p.is_question() && p.tool_use_id.as_deref() == Some(wanted)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|p| p.is_question())
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                        continue;
                    };
                    let body = match req.kind {
                        CodexRequestKind::UserInput { questions } => {
                            build_codex_user_input_answers(&questions, &selections)
                        }
                        CodexRequestKind::McpElicitation { fields, questions } => {
                            build_mcp_elicitation_response(&fields, &questions, &selections)
                        }
                        _ => json!({ "answers": {} }),
                    };
                    match proc.transport.respond(req.rpc_id, body) {
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
                        .trim()
                        .to_string();
                    let effort = params
                        .get("effort")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let invalid_model = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if !model.is_empty()
                            && !codex_model_is_available(&s.available_models, &model)
                        {
                            true
                        } else {
                            if !model.is_empty() {
                                s.current_model = Some(model.clone());
                            }
                            if !effort.is_empty() {
                                s.current_effort = Some(effort);
                            }
                            false
                        }
                    };
                    if invalid_model {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some(&format!("Unsupported Codex model: {model}")),
                        );
                        continue;
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
                    if let Err(err) = apply_codex_approval_level(
                        &mut proc_opt,
                        &spawn_cfg,
                        &state,
                        &activity,
                        &child_pid,
                        &active_transport,
                        &process_gen,
                        level,
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
                "steer" => {
                    let text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let (is_working, tid_initial, turn_id) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.thread_id.clone(), s.active_turn_id.clone())
                    };
                    let input = vec![json!({ "type": "text", "text": text })];
                    if is_working {
                        if let (Some(proc), Some(tid_ref), Some(active_tid)) = (
                            proc_opt.as_ref(),
                            tid_initial.as_deref(),
                            turn_id.as_deref(),
                        ) {
                            if proc
                                .transport
                                .call(
                                    "turn/steer",
                                    json!({
                                        "threadId": tid_ref,
                                        "expectedTurnId": active_tid,
                                        "input": input,
                                    }),
                                    Duration::from_secs(10),
                                )
                                .is_ok()
                            {
                                ep.ack(id, true, json!({ "steered": true }), None);
                                continue;
                            }
                        }
                        interrupt_active_codex_turn(&proc_opt, &state);
                        if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            if let Err(err) = restart_codex_process(
                                &mut proc_opt,
                                &spawn_cfg,
                                &state,
                                &activity,
                                &child_pid,
                                &active_transport,
                                &process_gen,
                            ) {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                        }
                    }
                    // Apply any pending profile reload (from a mid-turn upgrade) or recover at
                    // the turn boundary before starting the next turn.
                    let need_reload = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        proc_opt.is_none() || s.pending_profile_reload || s.fatal_error.is_some()
                    };
                    if need_reload {
                        if let Err(err) = restart_codex_process(
                            &mut proc_opt,
                            &spawn_cfg,
                            &state,
                            &activity,
                            &child_pid,
                            &active_transport,
                            &process_gen,
                        ) {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    }
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex process is not running."),
                        );
                        continue;
                    };
                    let (
                        tid,
                        level,
                        use_cli_default,
                        read_only,
                        use_profile_sandbox,
                        roots,
                        model,
                        effort,
                    ) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.thread_id.clone(),
                            s.approval_level,
                            s.use_cli_default,
                            s.read_only,
                            s.deny_mode != CodexDenyMode::None,
                            s.writable_roots.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    let Some(tid) = tid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex thread not initialized."),
                        );
                        continue;
                    };
                    if text.trim().eq_ignore_ascii_case("/compact") {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            s.compacting = true;
                            s.compacting_started_at = Some(now_iso_rfc3339());
                            s.last_turn_interrupted = false;
                            s.last_turn_stop_error = None;
                            s.last_error = None;
                            s.awaiting_turn_start = false;
                            s.active_turn_id = None;
                            s.clear_live_turn_output();
                        }
                        activity.sync();
                        println!("\r\n[Codex Native] Compacting thread...");
                        let _ = std::io::stdout().flush();
                        match proc.transport.call(
                            "thread/compact/start",
                            json!({ "threadId": tid }),
                            CODEX_RPC_TIMEOUT,
                        ) {
                            Ok(_) => {
                                ep.ack(
                                    id,
                                    true,
                                    json!({ "accepted": true, "compacting": true, "sessionId": tid }),
                                    None,
                                );
                            }
                            Err(err) => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.working = false;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    s.last_turn_stop_error = Some("error".to_string());
                                    s.last_error = Some((None, err.clone()));
                                }
                                activity.sync();
                                ep.ack(id, false, Value::Null, Some(&err));
                            }
                        }
                        continue;
                    }
                    let mut turn_params = Map::new();
                    turn_params.insert("threadId".to_string(), json!(tid));
                    turn_params.insert("input".to_string(), Value::Array(input));
                    turn_params.insert("cwd".to_string(), json!(cwd));
                    if !(use_cli_default && !read_only) {
                        let (approval_policy, reviewer, sandbox_policy) =
                            codex_turn_policies(level, read_only, &roots);
                        turn_params.insert("approvalPolicy".to_string(), approval_policy);
                        turn_params.insert("approvalsReviewer".to_string(), json!(reviewer));
                        if !use_profile_sandbox {
                            turn_params.insert("sandboxPolicy".to_string(), sandbox_policy);
                        }
                    }
                    if let Some(m) = model.as_deref().filter(|v| !v.is_empty()) {
                        turn_params.insert("model".to_string(), json!(m));
                    }
                    if let Some(e) = effort.as_deref().filter(|v| !v.is_empty()) {
                        turn_params.insert("effort".to_string(), json!(e));
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.compacting = false;
                        s.compacting_started_at = None;
                        s.last_turn_interrupted = false;
                        s.last_turn_stop_error = None;
                        s.last_error = None;
                        s.awaiting_turn_start = true;
                        s.active_turn_id = None;
                        s.clear_live_turn_output();
                    }
                    activity.sync();
                    match proc.transport.call(
                        "turn/start",
                        Value::Object(turn_params),
                        CODEX_RPC_TIMEOUT,
                    ) {
                        Ok(res) => {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.awaiting_turn_start = false;
                            if let Some(new_turn_id) =
                                res.pointer("/turn/id").and_then(Value::as_str)
                            {
                                if !s.completed_turn_ids.iter().any(|c| c == new_turn_id) {
                                    s.active_turn_id = Some(new_turn_id.to_string());
                                }
                            }
                            drop(s);
                            activity.sync();
                            ep.ack(
                                id,
                                true,
                                json!({ "accepted": true, "sessionId": tid }),
                                None,
                            );
                        }
                        Err(err) => {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.awaiting_turn_start = false;
                            s.working = false;
                            s.last_turn_interrupted = true;
                            drop(s);
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                        }
                    }
                }
                "rewind" => {
                    let message_id = params
                        .get("messageId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    let Some(proc) = proc_opt.as_ref().filter(|_| !message_id.is_empty()) else {
                        ep.ack(id, false, Value::Null, Some("REWIND_NOT_FOUND"));
                        continue;
                    };
                    match rewind::rewind_codex_thread(&proc.transport, &state, &message_id) {
                        Ok(result) => {
                            activity.sync();
                            ep.ack(id, true, result, None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
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
                    if clear::is_clear_command(&text, &images) {
                        match clear::start_new_thread(
                            &mut proc_opt,
                            &spawn_cfg,
                            &state,
                            &activity,
                            &child_pid,
                            &active_transport,
                            &process_gen,
                        ) {
                            Ok(result) => ep.ack(id, true, result, None),
                            Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                        }
                        continue;
                    }
                    let mut input = Vec::new();
                    let mut img_err = None;
                    for raw in &images {
                        match super::validate_acp_image_path(&gx_paths, raw) {
                            Ok((canonical, _)) => {
                                input.push(json!({
                                    "type": "localImage",
                                    "path": canonical.to_string_lossy().to_string(),
                                }));
                            }
                            Err(err) => {
                                img_err = Some(err);
                                break;
                            }
                        }
                    }
                    if let Some(err) = img_err {
                        ep.ack(id, false, Value::Null, Some(&err));
                        continue;
                    }
                    if !text.trim().is_empty() || input.is_empty() {
                        input.push(json!({
                            "type": "text",
                            "text": text,
                        }));
                    }
                    if !interrupt {
                        // Wait briefly if `turn/start` is still returning `active_turn_id`.
                        let wait_until = Instant::now() + Duration::from_secs(2);
                        while Instant::now() < wait_until {
                            let awaiting = {
                                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.working && s.awaiting_turn_start && s.active_turn_id.is_none()
                            };
                            if !awaiting {
                                break;
                            }
                            thread::sleep(Duration::from_millis(25));
                        }
                        let (is_working, has_pending, tid_mid, active_turn_id) = {
                            let s = state.lock().unwrap_or_else(|p| p.into_inner());
                            (
                                s.working,
                                !s.pending_requests.is_empty(),
                                s.thread_id.clone(),
                                s.active_turn_id.clone(),
                            )
                        };
                        if has_pending {
                            ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                            continue;
                        }
                        if is_working {
                            let steer_result = match (
                                proc_opt.as_ref(),
                                tid_mid.as_deref(),
                                active_turn_id.as_deref(),
                            ) {
                                (Some(proc), Some(tid_ref), Some(active_tid)) => {
                                    Some(proc.transport.call(
                                        "turn/steer",
                                        json!({
                                            "threadId": tid_ref,
                                            "expectedTurnId": active_tid,
                                            "input": input.clone(),
                                        }),
                                        Duration::from_secs(10),
                                    ))
                                }
                                _ => None,
                            };
                            match steer_result {
                                Some(Ok(_)) => {
                                    println!("\r\n[Codex Native] Steered active turn...");
                                    let _ = std::io::stdout().flush();
                                    ep.ack(
                                        id,
                                        true,
                                        json!({
                                            "accepted": true,
                                            "midTurn": true,
                                            "steered": true,
                                            "sessionId": tid_mid,
                                        }),
                                        None,
                                    );
                                    continue;
                                }
                                Some(Err(err)) => {
                                    // If the turn just completed as we called `turn/steer`, wait
                                    // briefly for `turn/completed` to settle and fall through to
                                    // `turn/start`; if the turn is still running, never cancel it.
                                    let settle_until = Instant::now() + Duration::from_millis(300);
                                    while Instant::now() < settle_until {
                                        if !state.lock().unwrap_or_else(|p| p.into_inner()).working
                                        {
                                            break;
                                        }
                                        thread::sleep(Duration::from_millis(25));
                                    }
                                    let (still_working, now_pending) = {
                                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                                        (s.working, !s.pending_requests.is_empty())
                                    };
                                    if now_pending {
                                        ep.ack(
                                            id,
                                            false,
                                            Value::Null,
                                            Some("ACP_AGENT_WAITING_INPUT"),
                                        );
                                        continue;
                                    }
                                    if still_working {
                                        eprintln!(
                                            "[Codex Native] Warning: turn/steer failed while turn active: {err}"
                                        );
                                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                                        continue;
                                    }
                                }
                                None => {
                                    if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    let is_working = state.lock().unwrap_or_else(|p| p.into_inner()).working;
                    if is_working && interrupt {
                        // Interrupt-and-send ALWAYS calls `turn/interrupt` and waits for
                        // `turn/completed` (restarting app-server if it does not stop in time) so
                        // the new turn runs strictly under the current approval level and profile.
                        interrupt_active_codex_turn(&proc_opt, &state);
                        if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            if let Err(err) = restart_codex_process(
                                &mut proc_opt,
                                &spawn_cfg,
                                &state,
                                &activity,
                                &child_pid,
                                &active_transport,
                                &process_gen,
                            ) {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                        }
                    }
                    // Apply any pending profile reload (from a mid-turn upgrade) or fail-closed
                    // recovery deterministically at the turn boundary before `turn/start`.
                    let need_reload = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        proc_opt.is_none() || s.pending_profile_reload || s.fatal_error.is_some()
                    };
                    if need_reload {
                        if let Err(err) = restart_codex_process(
                            &mut proc_opt,
                            &spawn_cfg,
                            &state,
                            &activity,
                            &child_pid,
                            &active_transport,
                            &process_gen,
                        ) {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    }
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex process is not running."),
                        );
                        continue;
                    };
                    cancel_all_pending_codex(&proc.transport, &state);
                    let (
                        tid,
                        level,
                        use_cli_default,
                        read_only,
                        use_profile_sandbox,
                        roots,
                        model,
                        effort,
                    ) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.thread_id.clone(),
                            s.approval_level,
                            s.use_cli_default,
                            s.read_only,
                            s.deny_mode != CodexDenyMode::None,
                            s.writable_roots.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    let Some(tid) = tid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex thread not initialized."),
                        );
                        continue;
                    };
                    if images.is_empty() && text.trim().eq_ignore_ascii_case("/compact") {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            s.compacting = true;
                            s.compacting_started_at = Some(now_iso_rfc3339());
                            s.last_turn_interrupted = false;
                            s.last_turn_stop_error = None;
                            s.last_error = None;
                            s.awaiting_turn_start = false;
                            s.active_turn_id = None;
                            s.clear_live_turn_output();
                        }
                        activity.sync();
                        println!("\r\n[Codex Native] Compacting thread...");
                        let _ = std::io::stdout().flush();
                        match proc.transport.call(
                            "thread/compact/start",
                            json!({ "threadId": tid }),
                            CODEX_RPC_TIMEOUT,
                        ) {
                            Ok(_) => {
                                ep.ack(
                                    id,
                                    true,
                                    json!({ "accepted": true, "compacting": true, "sessionId": tid }),
                                    None,
                                );
                            }
                            Err(err) => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.working = false;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    s.last_turn_stop_error = Some("error".to_string());
                                    s.last_error = Some((None, err.clone()));
                                }
                                activity.sync();
                                ep.ack(id, false, Value::Null, Some(&err));
                            }
                        }
                        continue;
                    }
                    let mut turn_params = Map::new();
                    turn_params.insert("threadId".to_string(), json!(tid));
                    turn_params.insert("input".to_string(), Value::Array(input));
                    turn_params.insert("cwd".to_string(), json!(cwd));
                    if !(use_cli_default && !read_only) {
                        let (approval_policy, reviewer, sandbox_policy) =
                            codex_turn_policies(level, read_only, &roots);
                        turn_params.insert("approvalPolicy".to_string(), approval_policy);
                        turn_params.insert("approvalsReviewer".to_string(), json!(reviewer));
                        if !use_profile_sandbox {
                            turn_params.insert("sandboxPolicy".to_string(), sandbox_policy);
                        }
                    }
                    if let Some(m) = model.as_deref().filter(|v| !v.is_empty()) {
                        turn_params.insert("model".to_string(), json!(m));
                    }
                    if let Some(e) = effort.as_deref().filter(|v| !v.is_empty()) {
                        turn_params.insert("effort".to_string(), json!(e));
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.compacting = false;
                        s.compacting_started_at = None;
                        s.last_turn_interrupted = false;
                        s.last_turn_stop_error = None;
                        s.last_error = None;
                        s.awaiting_turn_start = true;
                        s.active_turn_id = None;
                        s.clear_live_turn_output();
                    }
                    activity.sync();
                    println!("\r\n[Codex Native] Turn started...");
                    let _ = std::io::stdout().flush();
                    match proc.transport.call(
                        "turn/start",
                        Value::Object(turn_params),
                        CODEX_RPC_TIMEOUT,
                    ) {
                        Ok(res) => {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.awaiting_turn_start = false;
                            if let Some(new_turn_id) =
                                res.pointer("/turn/id").and_then(Value::as_str)
                            {
                                if !s.completed_turn_ids.iter().any(|c| c == new_turn_id) {
                                    s.active_turn_id = Some(new_turn_id.to_string());
                                }
                            }
                            drop(s);
                            activity.sync();
                            ep.ack(
                                id,
                                true,
                                json!({ "accepted": true, "sessionId": tid }),
                                None,
                            );
                        }
                        Err(err) => {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.awaiting_turn_start = false;
                            s.working = false;
                            s.last_turn_interrupted = true;
                            drop(s);
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
                    let (is_working, has_pending, tid, turn_id, carry_model, carry_effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.thread_id.clone(),
                            s.active_turn_id.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    if (is_working || has_pending) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Codex is working on a turn. Wait or interrupt before switching to Terminal."),
                        );
                        continue;
                    }
                    if let Some(proc) = proc_opt.as_ref() {
                        cancel_all_pending_codex(&proc.transport, &state);
                        if is_working {
                            state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .last_turn_interrupted = true;
                            if let (Some(tid_ref), Some(turn_id_ref)) =
                                (tid.as_deref(), turn_id.as_deref())
                            {
                                let _ = proc.transport.call(
                                    "turn/interrupt",
                                    json!({ "threadId": tid_ref, "turnId": turn_id_ref }),
                                    Duration::from_secs(3),
                                );
                            }
                        }
                    }
                    shutting_down.store(true, Ordering::Release);
                    process_gen.fetch_add(1, Ordering::AcqRel);
                    if let Some(mut old) = proc_opt.take() {
                        stop_codex_gracefully(
                            &old.transport,
                            &mut old.child,
                            &mut old.lifetime,
                            &child_pid,
                        );
                    }
                    ep.ack(
                        id,
                        true,
                        json!({
                            "switched": true,
                            "nativeSessionId": tid,
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
                        Some(&format!("Unsupported Codex Native command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (tid, approval_level, read_only, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.thread_id.clone(),
                s.approval_level,
                s.read_only,
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            tid.as_deref(),
            approval_level,
            read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    process_gen.fetch_add(1, Ordering::AcqRel);
    if let Some(mut old) = proc_opt.take() {
        stop_codex_gracefully(
            &old.transport,
            &mut old.child,
            &mut old.lifetime,
            &child_pid,
        );
    }
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
