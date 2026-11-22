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
use super::spec::{build_acp_server_argv, transcript_path_for_session, AcpAgentSpec};

const CODEX_RPC_TIMEOUT: Duration = Duration::from_secs(30);

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
enum CodexRequestKind {
    CommandApproval,
    FileChangeApproval,
    PermissionsApproval { permissions: Value },
    UserInput { questions: Vec<Value> },
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
        matches!(self.kind, CodexRequestKind::UserInput { .. })
    }

    fn to_ghostex_prompt(&self) -> Value {
        if let CodexRequestKind::UserInput { questions } = &self.kind {
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
                        "multiSelect": false,
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

struct CodexState {
    thread_id: Option<String>,
    thread_path: Option<String>,
    active_turn_id: Option<String>,
    working: bool,
    approval_level: AcpApprovalLevel,
    read_only: bool,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    available_models: Value,
    pending_requests: VecDeque<PendingCodexRequest>,
    file_change_locations_by_item: HashMap<String, Vec<Value>>,
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
    child_exited: bool,
}

impl CodexState {
    fn new(
        approval_level: AcpApprovalLevel,
        read_only: bool,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
        initial_model: Option<String>,
        initial_effort: Option<String>,
    ) -> Self {
        Self {
            thread_id: None,
            thread_path: None,
            active_turn_id: None,
            working: false,
            approval_level,
            read_only,
            cwd,
            writable_roots,
            current_model: initial_model,
            current_effort: initial_effort,
            available_models: Value::Array(Vec::new()),
            pending_requests: VecDeque::new(),
            file_change_locations_by_item: HashMap::new(),
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
                { "id": "full-access", "name": "Full access" }
            ],
            "configOptions": [],
            "pendingPermission": self.pending_requests.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "sessionTitle": self.session_title,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": transcript_path,
            "childExited": self.child_exited,
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
                let (act, sid, path, perm) = {
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
                    (act, s.thread_id.clone(), s.thread_path.clone(), front)
                };
                ep.report_activity_with_path(
                    "codex",
                    act,
                    sid.as_deref(),
                    path.as_deref(),
                    perm.as_ref(),
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

struct CodexExitGuard(Arc<Mutex<CodexState>>);
impl Drop for CodexExitGuard {
    fn drop(&mut self) {
        let mut s = self.0.lock().unwrap_or_else(|p| p.into_inner());
        s.child_exited = true;
        s.working = false;
    }
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
        AcpApprovalLevel::Ask if read_only => (
            json!("untrusted"),
            "user",
            json!({ "type": "readOnly" }),
        ),
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

fn extract_codex_file_locations(params: &Value) -> Vec<Value> {
    let mut locations = Vec::new();
    if let Some(changes) = params.get("changes").and_then(Value::as_array) {
        for c in changes {
            if let Some(path) = c
                .get("path")
                .or_else(|| c.get("filePath"))
                .and_then(Value::as_str)
            {
                locations.push(json!({ "path": path }));
            }
        }
    }
    if let Some(changes_obj) = params.get("changes").and_then(Value::as_object) {
        for key in changes_obj.keys() {
            locations.push(json!({ "path": key }));
        }
    }
    if let Some(files) = params.get("files").and_then(Value::as_array) {
        for f in files {
            if let Some(path) = f
                .as_str()
                .or_else(|| f.get("path").and_then(Value::as_str))
            {
                locations.push(json!({ "path": path }));
            }
        }
    }
    if locations.is_empty() {
        if let Some(root) = params.get("grantRoot").and_then(Value::as_str) {
            locations.push(json!({ "path": root }));
        }
    }
    locations
}

fn build_codex_user_input_answers(questions: &[Value], selections: &[Value]) -> Value {
    let mut answers = Map::new();
    for (idx, q) in questions.iter().enumerate() {
        let q_id = q
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
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
        }
    }
}

fn stop_codex_gracefully(transport: &CodexTransport, child: &mut Child, child_pid: &AtomicU32) {
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

pub(super) fn run(spec: &AcpAgentSpec, parsed: HostArgs) -> i32 {
    let cwd_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd = cwd_path.to_string_lossy().to_string();
    let writable_roots = canonicalize_writable_roots(&cwd_path, &parsed.writable_roots);
    let endpoint = GxserverEndpoint::from_env();
    let spool_session_id = endpoint
        .as_ref()
        .map(|ep| ep.session_id.clone())
        .unwrap_or_else(|| format!("local-{}", std::process::id()));

    println!("Ghostex Non-CLI Host · {} · Starting...", spec.display_name);
    let _ = std::io::stdout().flush();

    let gx_paths = crate::paths::get_gxserver_paths(None);
    let signal_guard = TerminationSignalGuard::install();

    let Some((acp_envs, prog, argv)) = build_acp_server_argv(spec, parsed.cli_command.as_deref())
    else {
        eprintln!("Invalid Codex Non-CLI command.");
        return 1;
    };

    let mut child = match Command::new(&prog)
        .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .envs(acp_envs)
        .args(&argv)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            eprintln!("Failed to start `{prog} {}`: {err}", argv.join(" "));
            return 1;
        }
    };
    let child_pid = Arc::new(AtomicU32::new(child.id()));
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        return 1;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return 1;
    };
    let stderr = child.stderr.take();

    let transport = Arc::new(CodexTransport::new(stdin, &spool_session_id));
    let state = Arc::new(Mutex::new(CodexState::new(
        parsed.approval_level,
        parsed.read_only,
        cwd_path.clone(),
        writable_roots,
        parsed.model.clone(),
        parsed.effort.clone(),
    )));
    let activity = CodexActivityReporter::start(endpoint.clone(), state.clone());
    let shutting_down = Arc::new(AtomicBool::new(false));

    #[cfg(unix)]
    {
        let t_sig = transport.clone();
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if super::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
                    t_sig.close_stdin();
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

    // Stdout JSON-RPC reader loop.
    {
        let t_out = transport.clone();
        let st_out = state.clone();
        let act_out = activity.clone();
        thread::spawn(move || {
            let _exit_guard = CodexExitGuard(st_out.clone());
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                        return;
                    };
                    t_out.spool_event(&msg);
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
                                            "\r\n[Codex Non-CLI] Auto-approved ({}): Bash ({})",
                                            level.label(),
                                            command
                                        );
                                        let _ = std::io::stdout().flush();
                                        let _ =
                                            t_out.respond(rpc_id, json!({ "decision": "accept" }));
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
                                let mut locations = extract_codex_file_locations(&params);
                                let (level, roots, cwd) = {
                                    let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    if locations.is_empty() {
                                        if let Some(id) = tool_use_id.as_deref() {
                                            if let Some(cached) =
                                                s.file_change_locations_by_item.get(id)
                                            {
                                                locations = cached.clone();
                                            }
                                        }
                                    }
                                    (s.approval_level, s.writable_roots.clone(), s.cwd.clone())
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
                                        "rawInput": {},
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
                                            "\r\n[Codex Non-CLI] Auto-approved ({}): Edit ({})",
                                            level.label(),
                                            summary
                                        );
                                        let _ = std::io::stdout().flush();
                                        let _ =
                                            t_out.respond(rpc_id, json!({ "decision": "accept" }));
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
                    let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
                    let params = msg.get("params").unwrap_or(&Value::Null);
                    match method {
                        "turn/started" => {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            if let Some(tid) = params
                                .pointer("/turn/id")
                                .or_else(|| params.get("turnId"))
                                .and_then(Value::as_str)
                            {
                                s.active_turn_id = Some(tid.to_string());
                            }
                            drop(s);
                            act_out.sync();
                        }
                        "turn/completed" => {
                            let orphaned: Vec<PendingCodexRequest> = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.working = false;
                                s.active_turn_id = None;
                                s.clear_live_turn_output();
                                s.pending_requests.drain(..).collect()
                            };
                            for req in orphaned {
                                match req.kind {
                                    CodexRequestKind::CommandApproval
                                    | CodexRequestKind::FileChangeApproval => {
                                        let _ = t_out
                                            .respond(req.rpc_id, json!({ "decision": "cancel" }));
                                    }
                                    CodexRequestKind::PermissionsApproval { .. } => {
                                        let _ =
                                            t_out.respond(req.rpc_id, json!({ "permissions": {} }));
                                    }
                                    CodexRequestKind::UserInput { .. } => {
                                        let _ = t_out.respond(req.rpc_id, json!({ "answers": {} }));
                                    }
                                }
                            }
                            println!("[Codex Non-CLI] Turn finished.");
                            let _ = std::io::stdout().flush();
                            act_out.sync();
                        }
                        "item/agentMessage/delta" => {
                            if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if s.live_stream_started_at.is_none() {
                                    s.live_stream_started_at = Some(now_iso_rfc3339());
                                }
                                append_bounded(
                                    &mut s.live_stream_text,
                                    delta,
                                    LIVE_STREAM_MAX_CHARS,
                                );
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
                                append_bounded(
                                    &mut s.live_thought_text,
                                    delta,
                                    LIVE_STREAM_MAX_CHARS,
                                );
                            }
                        }
                        "item/started" => {
                            if let Some(item) = params.get("item") {
                                let itype =
                                    item.get("type").and_then(Value::as_str).unwrap_or_default();
                                if matches!(
                                    itype,
                                    "commandExecution" | "fileChange" | "mcpToolCall"
                                ) {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    if itype == "fileChange" {
                                        if let Some(item_id) =
                                            item.get("id").and_then(Value::as_str)
                                        {
                                            let locs = extract_codex_file_locations(item);
                                            if !locs.is_empty() {
                                                s.file_change_locations_by_item
                                                    .insert(item_id.to_string(), locs);
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
                                let window =
                                    tu.get("modelContextWindow").and_then(Value::as_u64);
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if let Some(u) = used.filter(|v| *v > 0) {
                                    s.usage_used_tokens = Some(u);
                                }
                                if let Some(w) = window.filter(|v| *v > 0) {
                                    s.usage_window_size = Some(w);
                                }
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
                    eprintln!("[Codex Non-CLI] Warning: recovered from panic in stdout reader.");
                }
            }
        });
    }

    // 1. Initialize Codex app-server.
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
        eprintln!("Codex app-server initialize failed: {err}");
        stop_codex_gracefully(&transport, &mut child, &child_pid);
        return 1;
    }
    let _ = transport.notify("initialized", json!({}));

    // 2. Fetch available models.
    if let Ok(list) = transport.call("model/list", json!({}), CODEX_RPC_TIMEOUT) {
        if let Some(data) = list.get("data").and_then(Value::as_array) {
            let mapped: Vec<Value> = data
                .iter()
                .filter_map(|m| {
                    let id = m
                        .get("model")
                        .or_else(|| m.get("id"))
                        .and_then(Value::as_str)?;
                    let name = m
                        .get("displayName")
                        .and_then(Value::as_str)
                        .unwrap_or(id);
                    Some(json!({
                        "modelId": id,
                        "name": name,
                        "supportedReasoningEfforts": m.get("supportedReasoningEfforts").cloned().unwrap_or(json!([])),
                    }))
                })
                .collect();
            state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .available_models = Value::Array(mapped);
        }
    }

    // 3. Start, fork, or resume thread with explicit approvalPolicy/sandbox so any user-global
    // `approval_policy = "never"` in `~/.codex/config.toml` cannot override Ghostex's level.
    let (init_approval, init_reviewer, _init_sandbox_policy) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        codex_turn_policies(s.approval_level, s.read_only, &s.writable_roots)
    };
    let init_sandbox_mode = match parsed.approval_level {
        AcpApprovalLevel::FullAccess => "danger-full-access",
        AcpApprovalLevel::Ask if parsed.read_only => "read-only",
        AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits | AcpApprovalLevel::Auto => {
            "workspace-write"
        }
    };
    let thread_res = if let Some(resume_id) = parsed
        .resume_session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let method = if parsed.fork_session {
            "thread/fork"
        } else {
            "thread/resume"
        };
        transport.call(
            method,
            json!({
                "threadId": resume_id,
                "cwd": cwd,
                "approvalPolicy": init_approval,
                "approvalsReviewer": init_reviewer,
                "sandbox": init_sandbox_mode,
            }),
            CODEX_RPC_TIMEOUT,
        )
    } else {
        let mut start_params = Map::new();
        start_params.insert("cwd".to_string(), json!(cwd));
        start_params.insert("approvalPolicy".to_string(), init_approval);
        start_params.insert("approvalsReviewer".to_string(), json!(init_reviewer));
        start_params.insert("sandbox".to_string(), json!(init_sandbox_mode));
        if let Some(m) = parsed.model.as_deref().filter(|s| !s.is_empty()) {
            start_params.insert("model".to_string(), json!(m));
        }
        transport.call(
            "thread/start",
            Value::Object(start_params),
            CODEX_RPC_TIMEOUT,
        )
    };
    let thread_res = match thread_res {
        Ok(res) => res,
        Err(err) => {
            eprintln!("Codex thread open failed: {err}");
            stop_codex_gracefully(&transport, &mut child, &child_pid);
            return 1;
        }
    };
    let thread_id = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let tid = thread_res
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| parsed.resume_session_id.clone());
        s.thread_id = tid.clone();
        s.thread_path = thread_res
            .pointer("/thread/path")
            .and_then(Value::as_str)
            .map(str::to_string);
        if s.current_model.is_none() {
            s.current_model = thread_res
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if s.current_effort.is_none() {
            s.current_effort = thread_res
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        tid
    };

    println!(
        "Ghostex Non-CLI Host · {} · Ready (thread: {})",
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

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            println!("\r\n[Codex Non-CLI] Agent process exited.");
            break;
        }
        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let mut poll_params = json!({
            "action": "poll",
            "sessionRef": ep.session_ref,
            "agent": channel_agent,
            "pid": std::process::id(),
            "instance": ep.instance,
            "version": 1,
            "capabilities": capabilities,
        });
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
                    state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .approval_level = desired;
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
                    let (tid, turn_id) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.thread_id.clone(), s.active_turn_id.clone())
                    };
                    if let (Some(tid), Some(turn_id)) = (tid, turn_id) {
                        let _ = transport.call(
                            "turn/interrupt",
                            json!({ "threadId": tid, "turnId": turn_id }),
                            Duration::from_secs(8),
                        );
                    }
                    cancel_all_pending_codex(&transport, &state);
                    activity.sync();
                    ep.ack(id, true, json!({ "aborted": true }), None);
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
                                .position(|p| p.tool_use_id.as_deref() == Some(wanted))
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests.pop_front()
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
                            transport.respond(req.rpc_id, json!({ "decision": val }))
                        }
                        CodexRequestKind::PermissionsApproval { permissions } => {
                            if is_deny {
                                transport.respond(req.rpc_id, json!({ "permissions": {} }))
                            } else {
                                transport.respond(
                                    req.rpc_id,
                                    json!({ "permissions": permissions, "scope": "turn" }),
                                )
                            }
                        }
                        CodexRequestKind::UserInput { .. } => {
                            transport.respond(req.rpc_id, json!({ "answers": {} }))
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
                                .position(|p| p.tool_use_id.as_deref() == Some(wanted))
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
                        _ => json!({ "answers": {} }),
                    };
                    match transport.respond(req.rpc_id, body) {
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
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if !model.is_empty() {
                            s.current_model = Some(model);
                        }
                        if !effort.is_empty() {
                            s.current_effort = Some(effort);
                        }
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
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.approval_level = level;
                        if level != AcpApprovalLevel::Ask {
                            s.read_only = false;
                        }
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
                    let (is_working, tid, turn_id, level, read_only, roots, model, effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            s.thread_id.clone(),
                            s.active_turn_id.clone(),
                            s.approval_level,
                            s.read_only,
                            s.writable_roots.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    let Some(tid) = tid else {
                        ep.ack(id, false, Value::Null, Some("Codex thread not initialized."));
                        continue;
                    };
                    if is_working && !interrupt {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
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
                    if is_working && interrupt {
                        // Prefer Codex's native `turn/steer` when delivering mid-turn input (for
                        // example an async question answer while Codex is sleeping for input) so
                        // the running turn wakes immediately without aborting; fall back to
                        // `turn/interrupt` + `turn/start` if steering is refused.
                        if let Some(ref active_tid) = turn_id {
                            if transport
                                .call(
                                    "turn/steer",
                                    json!({
                                        "threadId": tid,
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
                        cancel_all_pending_codex(&transport, &state);
                        if let Some(turn_id) = turn_id {
                            let _ = transport.call(
                                "turn/interrupt",
                                json!({ "threadId": tid, "turnId": turn_id }),
                                Duration::from_secs(5),
                            );
                        }
                        let wait_until = SystemTime::now() + Duration::from_secs(4);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    cancel_all_pending_codex(&transport, &state);
                    let (approval_policy, reviewer, sandbox_policy) =
                        codex_turn_policies(level, read_only, &roots);
                    let mut turn_params = Map::new();
                    turn_params.insert("threadId".to_string(), json!(tid));
                    turn_params.insert("input".to_string(), Value::Array(input));
                    turn_params.insert("cwd".to_string(), json!(cwd));
                    turn_params.insert("approvalPolicy".to_string(), approval_policy);
                    turn_params.insert("approvalsReviewer".to_string(), json!(reviewer));
                    turn_params.insert("sandboxPolicy".to_string(), sandbox_policy);
                    if let Some(m) = model.as_deref().filter(|v| !v.is_empty()) {
                        turn_params.insert("model".to_string(), json!(m));
                    }
                    if let Some(e) = effort.as_deref().filter(|v| !v.is_empty()) {
                        turn_params.insert("effort".to_string(), json!(e));
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.clear_live_turn_output();
                    }
                    println!("\r\n[Codex Non-CLI] Turn started...");
                    let _ = std::io::stdout().flush();
                    match transport.call("turn/start", Value::Object(turn_params), CODEX_RPC_TIMEOUT)
                    {
                        Ok(res) => {
                            if let Some(new_turn_id) =
                                res.pointer("/turn/id").and_then(Value::as_str)
                            {
                                state
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .active_turn_id = Some(new_turn_id.to_string());
                            }
                            activity.sync();
                            ep.ack(
                                id,
                                true,
                                json!({ "accepted": true, "sessionId": tid }),
                                None,
                            );
                        }
                        Err(err) => {
                            state.lock().unwrap_or_else(|p| p.into_inner()).working = false;
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
                    let (is_working, has_pending, tid, turn_id) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.thread_id.clone(),
                            s.active_turn_id.clone(),
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
                    cancel_all_pending_codex(&transport, &state);
                    if is_working {
                        if let (Some(tid_ref), Some(turn_id_ref)) = (tid.as_deref(), turn_id.as_deref())
                        {
                            let _ = transport.call(
                                "turn/interrupt",
                                json!({ "threadId": tid_ref, "turnId": turn_id_ref }),
                                Duration::from_secs(3),
                            );
                        }
                    }
                    shutting_down.store(true, Ordering::Release);
                    stop_codex_gracefully(&transport, &mut child, &child_pid);
                    ep.ack(
                        id,
                        true,
                        json!({ "switched": true, "nativeSessionId": tid }),
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
                        Some(&format!("Unsupported Codex Non-CLI command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (tid, accept_all) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.thread_id.clone(),
                s.approval_level.implies_cli_accept_all(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            tid.as_deref(),
            accept_all,
        );
    }

    shutting_down.store(true, Ordering::Release);
    stop_codex_gracefully(&transport, &mut child, &child_pid);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
