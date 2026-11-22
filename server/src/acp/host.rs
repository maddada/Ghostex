//! `ghostex acp-host`: runs inside a session's `zmx` PTY, owns the agent's stdio
//! ACP subprocess (`agy acp --stdio`, etc.), spools every ACP event to an
//! append-only per-session file, bridges commands/approvals/state over
//! `/api/agentChatChannel`, survives `gxserver` reloads without dropping the
//! agent, and can switch in-place into the interactive CLI terminal on request.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{Value, json};

use crate::constants::{
    GXSERVER_LOCAL_API_HOST, GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION,
};

use super::approval::{
    AcpApprovalLevel, AcpPermissionOption, PermissionDecision, canonicalize_writable_roots,
    evaluate_permission, find_advertised_auto_mode_id, find_advertised_default_mode_id,
    is_auto_or_bypass_mode_id,
};
use super::spec::{
    AcpAgentSpec, build_acp_server_argv, build_cli_switch_argv, spec_for_family,
    transcript_path_for_session,
};

const ACP_LOAD_TIMEOUT: Duration = Duration::from_secs(90);
const ACP_RPC_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_RETRY_DELAY: Duration = Duration::from_millis(300);
const SPOOL_MAX_BYTES: u64 = 16 * 1024 * 1024;
const SPOOL_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[cfg(unix)]
static SIGNAL_TERMINATE: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn handle_termination_signal(_signal: libc::c_int) {
    SIGNAL_TERMINATE.store(true, Ordering::SeqCst);
}

struct TerminationSignalGuard;

impl TerminationSignalGuard {
    fn install() -> Self {
        #[cfg(unix)]
        unsafe {
            SIGNAL_TERMINATE.store(false, Ordering::SeqCst);
            let handler = handle_termination_signal as extern "C" fn(libc::c_int);
            libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
            libc::signal(libc::SIGHUP, handler as libc::sighandler_t);
            libc::signal(libc::SIGINT, handler as libc::sighandler_t);
        }
        Self
    }
}

impl Drop for TerminationSignalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            SIGNAL_TERMINATE.store(false, Ordering::SeqCst);
        }
    }
}

struct HostArgs {
    family: String,
    cli_command: Option<String>,
    resume_session_id: Option<String>,
    approval_level: AcpApprovalLevel,
    writable_roots: Vec<PathBuf>,
    model: Option<String>,
    effort: Option<String>,
}

fn parse_args(args: &[String]) -> Result<HostArgs, String> {
    let mut family = String::new();
    let mut cli_command = None;
    let mut resume_session_id = None;
    let mut approval_level = AcpApprovalLevel::Ask;
    let mut writable_roots = Vec::new();
    let mut model = None;
    let mut effort = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent" => {
                i += 1;
                family = args.get(i).cloned().unwrap_or_default();
            }
            "--cli-command" => {
                i += 1;
                cli_command = args.get(i).cloned();
            }
            "--resume" => {
                i += 1;
                resume_session_id = args.get(i).cloned();
            }
            "--accept-all" => approval_level = AcpApprovalLevel::FullAccess,
            "--approval-level" => {
                i += 1;
                let raw = args.get(i).map(String::as_str).unwrap_or_default();
                approval_level = AcpApprovalLevel::parse(raw)
                    .ok_or_else(|| format!("Unknown --approval-level value: {raw}"))?;
            }
            "--writable-root" => {
                i += 1;
                if let Some(root) = args.get(i).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    writable_roots.push(PathBuf::from(root));
                }
            }
            "--model" => {
                i += 1;
                model = args.get(i).cloned();
            }
            "--effort" => {
                i += 1;
                effort = args.get(i).cloned();
            }
            other => return Err(format!("Unknown acp-host option: {other}")),
        }
        i += 1;
    }
    if family.is_empty() {
        return Err("acp-host requires --agent <family>.".to_string());
    }
    Ok(HostArgs {
        family,
        cli_command,
        resume_session_id,
        approval_level,
        writable_roots,
        model,
        effort,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Clone)]
struct GxserverEndpoint {
    base_url: String,
    token: String,
    session_ref: String,
    project_id: String,
    session_id: String,
    instance: String,
}

impl GxserverEndpoint {
    fn from_env() -> Option<Self> {
        let session_ref = env::var("GHOSTEX_GLOBAL_SESSION_REF")
            .ok()?
            .trim()
            .to_string();
        if session_ref.is_empty() {
            return None;
        }
        let (project_id, session_id) =
            match crate::agent_hooks::probing::parse_global_session_ref(&session_ref) {
                (Some(p), Some(s)) => (p, s),
                _ => return None,
            };
        let paths = crate::paths::get_gxserver_paths(None);
        let base_url = env::var("GHOSTEX_GXSERVER_BASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| {
                let port = crate::config::read_selected_local_api_port().unwrap_or(58744);
                format!("http://{GXSERVER_LOCAL_API_HOST}:{port}")
            })
            .trim_end_matches('/')
            .to_string();
        let token = env::var("GHOSTEX_SESSION_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                fs::read_to_string(&paths.auth_token_file)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_default();
        let instance = format!("acp-{}-{}", std::process::id(), now_ms());
        Some(Self {
            base_url,
            token,
            session_ref,
            project_id,
            session_id,
            instance,
        })
    }

    fn post_json(&self, path: &str, body: Value, timeout: Duration) -> Result<Value, String> {
        let url = format!("{}{path}", self.base_url);
        let agent: ureq::Agent = crate::http_agent::config()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .build()
            .into();
        let mut req = agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header(
                GXSERVER_PROTOCOL_HEADER,
                &GXSERVER_PROTOCOL_VERSION.to_string(),
            )
            .header("X-Ghostex-Session-Ref", &self.session_ref);
        if !self.token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {}", self.token));
        }
        let response = req.send_json(&body).map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let parsed: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}: {parsed}"));
        }
        Ok(parsed)
    }

    fn report_activity(
        &self,
        family: &str,
        activity: &str,
        native_session_id: Option<&str>,
        pending_permission: Option<&PendingPermission>,
    ) {
        let event_name = match activity {
            "working" => "UserPromptSubmit",
            "attention" => "PermissionRequest",
            _ => "Stop",
        };
        let mut params = json!({
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "agentName": family,
            "eventName": event_name,
            "status": activity,
            "timestamp": now_ms(),
        });
        if let Some(sid) = native_session_id.filter(|s| !s.is_empty()) {
            params["agentSessionId"] = json!(sid);
            if let Some(path) = transcript_path_for_session(family, sid) {
                params["agentSessionPath"] = json!(path.to_string_lossy().to_string());
            }
        }
        if let Some(perm) = pending_permission {
            params["toolName"] = json!(perm.tool_name);
            params["toolInput"] = json!({ "command": perm.summary });
            if let Some(tool_use_id) = perm.tool_use_id.as_deref().filter(|s| !s.is_empty()) {
                params["toolUseId"] = json!(tool_use_id);
            }
            if let Some(reason) = perm.reason.as_deref().filter(|s| !s.trim().is_empty()) {
                params["approvalReason"] = json!(reason);
            }
            params["sessionChatPrompt"] = perm.to_ghostex_prompt();
        }
        let body = json!({
            "requestId": format!("acp-act-{}", now_ms()),
            "params": params,
        });
        let _ = self.post_json("/api/ingestAgentHookEvent", body, Duration::from_secs(5));
    }

    fn ack(&self, id: &str, ok: bool, result: Value, error: Option<&str>) {
        let mut params = json!({
            "action": "ack",
            "sessionRef": self.session_ref,
            "id": id,
            "ok": ok,
            "result": result,
        });
        if let Some(err) = error {
            params["error"] = json!(err);
        }
        let body = json!({
            "requestId": format!("acp-ack-{}", now_ms()),
            "params": params,
        });
        let _ = self.post_json("/api/agentChatChannel", body, Duration::from_secs(5));
    }

    fn close_channel(&self) {
        let body = json!({
            "requestId": format!("acp-close-{}", now_ms()),
            "params": {
                "action": "close",
                "sessionRef": self.session_ref,
                "instance": self.instance,
            },
        });
        let _ = self.post_json("/api/agentChatChannel", body, Duration::from_secs(3));
    }
}

struct SpoolWriter {
    path: PathBuf,
    file: Option<File>,
    bytes_written: u64,
    seq: u64,
}

impl SpoolWriter {
    fn open_private_file(path: &PathBuf, truncate: bool) -> Option<File> {
        let mut options = OpenOptions::new();
        options.create(true).write(true);
        if truncate {
            options.truncate(true);
        } else {
            options.append(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        }
        Some(file)
    }

    fn sweep_stale_spools(dir: &PathBuf) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !(name.ends_with(".jsonl") || name.ends_with(".jsonl.1")) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if now.duration_since(modified).unwrap_or_default() > SPOOL_RETENTION {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    fn open(session_id: &str) -> Self {
        let dir = crate::paths::get_gxserver_paths(None)
            .root_dir
            .join("acp-spool");
        let _ = fs::create_dir_all(&dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
        Self::sweep_stale_spools(&dir);
        let safe_id: String = session_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .collect();
        let name = if safe_id.is_empty() {
            format!("pid-{}.jsonl", std::process::id())
        } else {
            format!("{safe_id}.jsonl")
        };
        let path = dir.join(name);
        let file = Self::open_private_file(&path, false);
        let bytes_written = file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map(|m| m.len())
            .unwrap_or(0);
        Self {
            path,
            file,
            bytes_written,
            seq: 0,
        }
    }

    fn rotate(&mut self) {
        self.file = None;
        let rotated = PathBuf::from(format!("{}.1", self.path.display()));
        let _ = fs::remove_file(&rotated);
        let _ = fs::rename(&self.path, &rotated);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&rotated, fs::Permissions::from_mode(0o600));
        }
        self.file = Self::open_private_file(&self.path, true);
        self.bytes_written = 0;
    }

    fn append(&mut self, event: &Value) {
        self.seq += 1;
        let line = json!({
            "seq": self.seq,
            "tsMs": now_ms(),
            "event": event,
        });
        let Ok(encoded) = serde_json::to_string(&line) else {
            return;
        };
        let write_len = (encoded.len() + 1) as u64;
        if self.bytes_written.saturating_add(write_len) > SPOOL_MAX_BYTES {
            self.rotate();
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if file.write_all(encoded.as_bytes()).is_ok()
            && file.write_all(b"\n").is_ok()
            && file.flush().is_ok()
        {
            self.bytes_written = self.bytes_written.saturating_add(write_len);
        }
    }
}

#[derive(Clone, Debug)]
struct PendingPermission {
    rpc_id: Value,
    tool_name: String,
    summary: String,
    tool_use_id: Option<String>,
    reason: Option<String>,
    options: Vec<AcpPermissionOption>,
}

impl PendingPermission {
    fn to_ghostex_prompt(&self) -> Value {
        let mut prompt = json!({
            "kind": "approval",
            "tool": self.tool_name,
            "summary": self.summary,
            "toolUseId": self.tool_use_id,
            "options": self.options.iter().map(|opt| json!({
                "optionId": opt.option_id,
                "label": opt.name,
                "kind": opt.kind,
            })).collect::<Vec<_>>(),
        });
        if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
            prompt["reason"] = json!(reason);
        }
        prompt
    }

    fn resolve_option_id(
        &self,
        explicit_option_id: Option<&str>,
        decision: &str,
    ) -> Option<String> {
        if let Some(id) = explicit_option_id.filter(|s| !s.is_empty()) {
            if self.options.iter().any(|opt| opt.option_id == id) {
                return Some(id.to_string());
            }
        }
        let normalized = decision.trim().to_ascii_lowercase();
        let (is_deny, wanted_kinds): (bool, &[&str]) = match normalized.as_str() {
            "allow" | "allow_once" | "1" | "y" | "yes" => (false, &["allow_once", "allow_always"]),
            "always" | "allow_always" | "2" => (false, &["allow_always", "allow_once"]),
            "deny" | "reject" | "reject_once" | "escape" | "cancel" | "\x1b" | "n" | "no" => {
                (true, &["reject_once", "reject_always"])
            }
            "reject_always" => (true, &["reject_always", "reject_once"]),
            _ => (false, &["allow_once"]),
        };
        for kind in wanted_kinds {
            if let Some(opt) = self.options.iter().find(|opt| opt.kind == *kind) {
                return Some(opt.option_id.clone());
            }
        }
        if is_deny {
            // Never fall back to an allow option on deny/reject; caller responds with `cancelled`.
            return None;
        }
        self.options
            .iter()
            .find(|opt| !opt.kind.starts_with("reject"))
            .map(|opt| opt.option_id.clone())
    }
}

struct HostState {
    family: String,
    native_session_id: Option<String>,
    working: bool,
    turn_generation: u64,
    approval_level: AcpApprovalLevel,
    in_flight_target_level: Option<AcpApprovalLevel>,
    default_native_mode: Option<String>,
    native_auto_active: bool,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    current_mode: Option<String>,
    available_models: Value,
    available_modes: Value,
    config_options: Value,
    pending_permissions: VecDeque<PendingPermission>,
    child_exited: bool,
}

impl HostState {
    fn new(
        family: &str,
        approval_level: AcpApprovalLevel,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            family: family.to_string(),
            native_session_id: None,
            working: false,
            turn_generation: 0,
            approval_level,
            in_flight_target_level: None,
            default_native_mode: None,
            native_auto_active: false,
            cwd,
            writable_roots,
            current_model: None,
            current_effort: None,
            current_mode: None,
            available_models: Value::Array(Vec::new()),
            available_modes: Value::Array(Vec::new()),
            config_options: Value::Array(Vec::new()),
            pending_permissions: VecDeque::new(),
            child_exited: false,
        }
    }

    fn snapshot_json(&self) -> Value {
        let transcript_path = self
            .native_session_id
            .as_deref()
            .and_then(|id| transcript_path_for_session(&self.family, id))
            .map(|p| p.to_string_lossy().to_string());
        json!({
            "family": self.family,
            "nativeSessionId": self.native_session_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": self.current_mode,
            "nativeAutoActive": self.native_auto_active,
            "nativeAutoAvailable": find_advertised_auto_mode_id(&self.available_modes).is_some(),
            "availableModels": self.available_models,
            "availableModes": self.available_modes,
            "configOptions": self.config_options,
            "pendingPermission": self.pending_permissions.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_permissions.len(),
            "transcriptPath": transcript_path,
            "childExited": self.child_exited,
        })
    }
}

struct AcpTransport {
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    spool: Mutex<SpoolWriter>,
}

impl AcpTransport {
    fn new(stdin: ChildStdin, session_id: &str) -> Self {
        Self {
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            spool: Mutex::new(SpoolWriter::open(session_id)),
        }
    }

    fn write_json(&self, message: &Value) -> Result<(), String> {
        let mut guard = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(stdin) = guard.as_mut() else {
            return Err("ACP server stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to ACP server: {e}"))
    }

    fn close_stdin(&self) {
        let mut guard = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_json(&msg)
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
                        .unwrap_or("ACP request failed");
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
                Err(format!("ACP request `{method}` timed out."))
            }
        }
    }

    fn respond_to_agent(&self, rpc_id: Value, result: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result,
        });
        self.write_json(&msg)
    }

    fn respond_error_to_agent(&self, rpc_id: Value, code: i64, message: &str) {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": code, "message": message },
        });
        let _ = self.write_json(&msg);
    }

    fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

fn apply_session_metadata(state: &mut HostState, result: &Value) {
    if let Some(sid) = result
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        state.native_session_id = Some(sid.to_string());
    }
    if let Some(models) = result.get("models") {
        if let Some(cur) = models
            .get("currentModelId")
            .or_else(|| models.get("currentModel"))
            .and_then(Value::as_str)
        {
            state.current_model = Some(cur.to_string());
        }
        if let Some(avail) = models.get("availableModels") {
            state.available_models = avail.clone();
        }
    }
    if let Some(modes) = result.get("modes") {
        if let Some(cur) = modes
            .get("currentModeId")
            .or_else(|| modes.get("currentMode"))
            .and_then(Value::as_str)
        {
            state.current_mode = Some(cur.to_string());
        }
        if let Some(avail) = modes.get("availableModes") {
            state.available_modes = avail.clone();
        }
    }
    if let Some(configs) = result.get("configOptions") {
        state.config_options = configs.clone();
    }
}

fn parse_permission_request(rpc_id: Value, params: &Value) -> PendingPermission {
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_name = tool_call
        .get("name")
        .or_else(|| tool_call.get("kind"))
        .or_else(|| tool_call.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Tool")
        .to_string();
    let tool_use_id = tool_call
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let summary = tool_call
        .get("rawInput")
        .and_then(|v| {
            [
                "command",
                "CommandLine",
                "file_path",
                "filePath",
                "path",
                "Path",
                "target_path",
                "targetPath",
                "TargetFile",
                "AbsolutePath",
            ]
            .iter()
            .find_map(|key| v.get(*key).and_then(Value::as_str))
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                tool_call
                    .pointer("/locations/0/path")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
            })
            .or_else(|| serde_json::to_string(v).ok())
        })
        .or_else(|| {
            tool_call
                .pointer("/locations/0/path")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| tool_name.clone());
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let option_id = item.get("optionId").and_then(Value::as_str)?.to_string();
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&option_id)
                        .to_string();
                    // Never assume an omitted `kind` is `allow_once`: auto-approval only picks
                    // options explicitly marked `allow_once`.
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    Some(AcpPermissionOption {
                        option_id,
                        name,
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    PendingPermission {
        rpc_id,
        tool_name,
        summary,
        tool_use_id,
        reason: None,
        options,
    }
}

fn build_prompt_blocks(
    paths: &crate::paths::GxserverPaths,
    text: &str,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut blocks = Vec::new();
    for raw in image_paths {
        let (bytes, mime) = super::read_validated_acp_image(paths, raw)?;
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

/// Single ordered background worker for `/api/ingestAgentHookEvent` activity updates.
/// Every state transition sends a non-blocking wake signal; the worker reads the current
/// authoritative `pending_permissions.front()` / `working` / `idle` state before posting so a
/// stale `"working"` report can never overwrite a newly arrived `"attention"` card and the stdout
/// JSON-RPC reader never blocks on HTTP.
#[derive(Clone)]
struct ActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl ActivityReporter {
    fn start(
        endpoint: Option<GxserverEndpoint>,
        state: Arc<Mutex<HostState>>,
        family: String,
    ) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                // Coalesce burst updates so we always publish the latest state.
                while rx.try_recv().is_ok() {}
                let (act, sid, perm) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s.pending_permissions.front().cloned();
                    let act = if front.is_some() {
                        "attention"
                    } else if s.working {
                        "working"
                    } else {
                        "idle"
                    };
                    (act, s.native_session_id.clone(), front)
                };
                ep.report_activity(&family, act, sid.as_deref(), perm.as_ref());
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

/// Ensures `child_exited` is always set if the stdout JSON-RPC reader thread exits unexpectedly.
struct ReaderExitGuard(Arc<Mutex<HostState>>);

impl Drop for ReaderExitGuard {
    fn drop(&mut self) {
        let mut s = self.0.lock().unwrap_or_else(|p| p.into_inner());
        s.child_exited = true;
        s.working = false;
    }
}

/// Keep the underlying ACP server in its advertised `auto` mode only while `target_level` is
/// `Auto` (and the agent advertises both an auto mode and a safe non-auto mode to return to), and
/// in its default/ask mode for `Ask`, `Auto-edits`, and `Full access` so `acp-host` evaluates
/// every `session/request_permission` live with `allow_once`.
///
/// Serialized by `mode_sync_lock` and records `in_flight_target_level` so an agent's own
/// `current_mode_update` notification during an intentional transition into `Auto` is never
/// misread as drift. Fails visibly (`Err`) whenever leaving an active native auto/bypass mode
/// cannot be confirmed on the subprocess.
fn apply_approval_level_and_sync_native_mode(
    transport: &AcpTransport,
    state: &Mutex<HostState>,
    mode_sync_lock: &Mutex<()>,
    target_level: AcpApprovalLevel,
) -> Result<(), String> {
    let _mode_guard = mode_sync_lock.lock().unwrap_or_else(|p| p.into_inner());
    let (sid, current_mode, was_auto_or_bypass, target_mode, is_auto_target) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let auto_id = find_advertised_auto_mode_id(&s.available_modes);
        let default_id = s.default_native_mode.clone().or_else(|| {
            find_advertised_default_mode_id(&s.available_modes, s.current_mode.as_deref())
        });
        let was_auto = s.native_auto_active
            || s.current_mode
                .as_deref()
                .is_some_and(is_auto_or_bypass_mode_id);
        let (target, is_auto) = match target_level {
            AcpApprovalLevel::Auto => match (auto_id, default_id.clone()) {
                // Only enter native auto if we also know a safe non-auto mode to switch back to.
                (Some(id), Some(_)) => (Some(id), true),
                (_, default) => (default, false),
            },
            AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits | AcpApprovalLevel::FullAccess => {
                (default_id, false)
            }
        };
        s.in_flight_target_level = Some(target_level);
        (
            s.native_session_id.clone(),
            s.current_mode.clone(),
            was_auto,
            target,
            is_auto,
        )
    };

    let Some(sid) = sid else {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight_target_level = None;
        s.approval_level = target_level;
        return Ok(());
    };

    let Some(target_mode) = target_mode else {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight_target_level = None;
        if was_auto_or_bypass && !is_auto_target {
            return Err(
                "Could not leave the agent's automatic mode because the agent did not advertise a safe non-automatic mode.".to_string(),
            );
        }
        s.approval_level = target_level;
        s.native_auto_active = false;
        return Ok(());
    };

    if current_mode.as_deref() == Some(target_mode.as_str()) {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight_target_level = None;
        s.approval_level = target_level;
        s.native_auto_active = target_level == AcpApprovalLevel::Auto && is_auto_target;
        return Ok(());
    }

    match transport.call(
        "session/set_mode",
        json!({ "sessionId": sid, "modeId": target_mode }),
        ACP_RPC_TIMEOUT,
    ) {
        Ok(_) => {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.in_flight_target_level = None;
            s.approval_level = target_level;
            s.current_mode = Some(target_mode);
            s.native_auto_active = target_level == AcpApprovalLevel::Auto && is_auto_target;
            Ok(())
        }
        Err(err) => {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.in_flight_target_level = None;
            if was_auto_or_bypass && !is_auto_target {
                s.native_auto_active = true;
                return Err(format!(
                    "Could not leave the agent's automatic mode: {err}"
                ));
            }
            // Entering native Auto failed: stay safely on the non-auto native mode (behaving like
            // Ask) while recording the requested level.
            s.approval_level = target_level;
            s.native_auto_active = false;
            Ok(())
        }
    }
}

fn cancel_all_pending_permissions(transport: &AcpTransport, state: &Mutex<HostState>) {
    let drained: Vec<PendingPermission> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_permissions.drain(..).collect()
    };
    for perm in drained {
        let _ = transport.respond_to_agent(
            perm.rpc_id,
            json!({ "outcome": { "outcome": "cancelled" } }),
        );
    }
}

fn stop_child_gracefully(transport: &AcpTransport, child: &mut Child, child_pid: &AtomicU32) {
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

fn exec_interactive_cli(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
    native_session_id: Option<&str>,
    accept_all: bool,
) -> i32 {
    let (envs, program, args) =
        build_cli_switch_argv(spec, cli_command, native_session_id, accept_all);
    println!(
        "\r\nSwitching to interactive {} terminal...",
        spec.display_name
    );
    let _ = std::io::stdout().flush();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(&program).envs(envs).args(&args).exec();
        eprintln!("Failed to launch {program}: {err}");
        1
    }

    #[cfg(not(unix))]
    {
        match Command::new(&program).envs(envs).args(&args).status() {
            Ok(status) => status.code().unwrap_or(0),
            Err(err) => {
                eprintln!("Failed to launch {program}: {err}");
                1
            }
        }
    }
}

/// Entry point for `ghostex acp-host`.
pub fn run(args: &[String]) -> i32 {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    let Some(spec) = spec_for_family(&parsed.family) else {
        eprintln!("Agent `{}` does not support ACP mode yet.", parsed.family);
        return 2;
    };
    let cwd_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
        eprintln!("Invalid ACP server command for {}", spec.family);
        return 1;
    };
    let mut child = match Command::new(&prog)
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
        eprintln!("Failed to open stdin pipe to ACP server.");
        let _ = child.kill();
        return 1;
    };
    let Some(stdout) = child.stdout.take() else {
        eprintln!("Failed to open stdout pipe from ACP server.");
        let _ = child.kill();
        return 1;
    };
    let stderr = child.stderr.take();

    let transport = Arc::new(AcpTransport::new(stdin, &spool_session_id));
    let state = Arc::new(Mutex::new(HostState::new(
        spec.family,
        parsed.approval_level,
        cwd_path.clone(),
        writable_roots,
    )));
    let mode_sync_lock = Arc::new(Mutex::new(()));
    let activity = ActivityReporter::start(endpoint.clone(), state.clone(), spec.family.to_string());
    let shutting_down = Arc::new(AtomicBool::new(false));

    // Watch for SIGTERM/SIGHUP/SIGINT so the ACP subprocess is stopped promptly even while the
    // main thread is blocked in `/api/agentChatChannel` long-poll.
    #[cfg(unix)]
    {
        let t_sig = transport.clone();
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if SIGNAL_TERMINATE.load(Ordering::Acquire) {
                    down_sig.store(true, Ordering::Release);
                    t_sig.close_stdin();
                    let pid = pid_sig.swap(0, Ordering::AcqRel);
                    if pid != 0 {
                        let cpid = pid as libc::pid_t;
                        unsafe {
                            libc::kill(cpid, libc::SIGTERM);
                        }
                        let mut reaped = false;
                        let deadline = Instant::now() + Duration::from_secs(1);
                        while Instant::now() < deadline {
                            let waited = unsafe {
                                libc::waitpid(cpid, std::ptr::null_mut(), libc::WNOHANG)
                            };
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

    // Drain stderr into spool so pipe never blocks.
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

    // Stdout JSON-RPC reader loop (panic-safe so an unexpected message can never wedge `acp-host`).
    {
        let t_out = transport.clone();
        let st_out = state.clone();
        let mode_lock_out = mode_sync_lock.clone();
        let act_out = activity.clone();
        thread::spawn(move || {
            let _exit_guard = ReaderExitGuard(st_out.clone());
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let handle_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                        return;
                    };
                    t_out.spool_event(&msg);
                    let has_method = msg.get("method").and_then(Value::as_str).is_some();
                    let has_id = msg.get("id").is_some();
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
                    if has_method && has_id {
                        let method = msg
                            .get("method")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
                        let params = msg.get("params").cloned().unwrap_or(Value::Null);
                        if method == "session/request_permission" {
                            let mut perm = parse_permission_request(rpc_id, &params);
                            let (level, roots, session_cwd) = {
                                let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                (s.approval_level, s.writable_roots.clone(), s.cwd.clone())
                            };
                            match evaluate_permission(
                                level,
                                &params,
                                &perm.options,
                                &roots,
                                &session_cwd,
                            ) {
                                PermissionDecision::AutoApprove { option_id } => {
                                    println!(
                                        "\r\n[ACP] Auto-approved ({}): {} ({})",
                                        level.label(),
                                        perm.tool_name,
                                        perm.summary
                                    );
                                    let _ = std::io::stdout().flush();
                                    let _ = t_out.respond_to_agent(
                                        perm.rpc_id,
                                        json!({
                                            "outcome": {
                                                "outcome": "selected",
                                                "optionId": option_id,
                                            }
                                        }),
                                    );
                                    return;
                                }
                                PermissionDecision::AskUser { reason } => {
                                    perm.reason = reason;
                                }
                            }
                            println!(
                                "\r\n[ACP] Waiting for approval: {} ({})",
                                perm.tool_name, perm.summary
                            );
                            let _ = std::io::stdout().flush();
                            let was_empty = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                let empty = s.pending_permissions.is_empty();
                                s.pending_permissions.push_back(perm);
                                empty
                            };
                            if was_empty {
                                act_out.sync();
                            }
                        } else {
                            t_out.respond_error_to_agent(
                                rpc_id,
                                -32601,
                                &format!("Unsupported client method: {method}"),
                            );
                        }
                        return;
                    }
                    // JSON-RPC notification from agent.
                    if let Some(update) = msg.pointer("/params/update") {
                        let kind = update
                            .get("sessionUpdate")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        match kind {
                            "current_mode_update" => {
                                if let Some(mode_id) = update
                                    .get("currentModeId")
                                    .or_else(|| update.get("modeId"))
                                    .and_then(Value::as_str)
                                {
                                    let needs_drift_resync = {
                                        let mut s =
                                            st_out.lock().unwrap_or_else(|p| p.into_inner());
                                        s.current_mode = Some(mode_id.to_string());
                                        let expected = s
                                            .in_flight_target_level
                                            .unwrap_or(s.approval_level);
                                        if is_auto_or_bypass_mode_id(mode_id) {
                                            s.native_auto_active = true;
                                            expected != AcpApprovalLevel::Auto
                                        } else {
                                            s.native_auto_active = false;
                                            false
                                        }
                                    };
                                    if needs_drift_resync {
                                        // Re-sync off the stdout reader thread, re-reading the
                                        // latest authoritative `approval_level` under `mode_sync_lock`.
                                        let t_sync = t_out.clone();
                                        let st_sync = st_out.clone();
                                        let lock_sync = mode_lock_out.clone();
                                        let drifted = mode_id.to_string();
                                        thread::spawn(move || {
                                            let latest = {
                                                let s = st_sync
                                                    .lock()
                                                    .unwrap_or_else(|p| p.into_inner());
                                                let expected = s
                                                    .in_flight_target_level
                                                    .unwrap_or(s.approval_level);
                                                (expected != AcpApprovalLevel::Auto)
                                                    .then_some(s.approval_level)
                                            };
                                            if let Some(target_level) = latest {
                                                if let Err(err) =
                                                    apply_approval_level_and_sync_native_mode(
                                                        &t_sync,
                                                        &st_sync,
                                                        &lock_sync,
                                                        target_level,
                                                    )
                                                {
                                                    eprintln!(
                                                        "[ACP] Warning: agent switched to native mode `{drifted}` while level is {}: {err}",
                                                        target_level.label()
                                                    );
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                            "config_option_update" => {
                                if let Some(opts) = update.get("configOptions") {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    s.config_options = opts.clone();
                                }
                            }
                            _ => {}
                        }
                    }
                }));
                if handle_res.is_err() {
                    eprintln!("[ACP] Warning: recovered from panic while handling ACP stdout message.");
                }
            }
        });
    }

    // 1. Initialize ACP protocol.
    let init_params = json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
        },
        "clientInfo": {
            "name": "ghostex",
            "version": env!("CARGO_PKG_VERSION"),
        },
    });
    if let Err(err) = transport.call("initialize", init_params, ACP_RPC_TIMEOUT) {
        eprintln!("ACP initialize failed: {err}");
        stop_child_gracefully(&transport, &mut child, &child_pid);
        return 1;
    }

    // 2. Open or resume conversation.
    let session_result = if let Some(resume_id) = parsed.resume_session_id.as_deref() {
        let load_params = json!({
            "sessionId": resume_id,
            "cwd": cwd,
            "mcpServers": [],
        });
        match transport.call("session/load", load_params.clone(), ACP_LOAD_TIMEOUT) {
            Ok(mut res) => {
                if res.get("sessionId").is_none() {
                    if let Some(obj) = res.as_object_mut() {
                        obj.insert("sessionId".to_string(), json!(resume_id));
                    }
                }
                Ok(res)
            }
            Err(_) => transport
                .call("session/resume", load_params, ACP_RPC_TIMEOUT)
                .map(|mut res| {
                    if res.get("sessionId").is_none() {
                        if let Some(obj) = res.as_object_mut() {
                            obj.insert("sessionId".to_string(), json!(resume_id));
                        }
                    }
                    res
                }),
        }
    } else {
        transport.call(
            "session/new",
            json!({ "cwd": cwd, "mcpServers": [] }),
            ACP_RPC_TIMEOUT,
        )
    };

    let session_result = match session_result {
        Ok(res) => res,
        Err(err) => {
            eprintln!("ACP session open failed: {err}");
            stop_child_gracefully(&transport, &mut child, &child_pid);
            return 1;
        }
    };

    let native_session_id = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        apply_session_metadata(&mut s, &session_result);
        s.default_native_mode =
            find_advertised_default_mode_id(&s.available_modes, s.current_mode.as_deref());
        s.native_session_id.clone()
    };

    if let Err(err) = apply_approval_level_and_sync_native_mode(
        &transport,
        &state,
        &mode_sync_lock,
        parsed.approval_level,
    ) {
        eprintln!("[ACP] Warning: could not sync initial native mode: {err}");
    }

    if let Some(model) = parsed.model.as_deref().filter(|m| !m.is_empty()) {
        if let Some(sid) = native_session_id.as_deref() {
            if transport
                .call(
                    "session/set_model",
                    json!({ "sessionId": sid, "modelId": model }),
                    ACP_RPC_TIMEOUT,
                )
                .is_ok()
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.current_model = Some(model.to_string());
            }
        }
    }
    if let Some(effort) = parsed.effort.as_deref().filter(|e| !e.is_empty()) {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.current_effort = Some(effort.to_string());
    }

    println!(
        "Ghostex Non-CLI Host · {} · Ready (conversation: {})",
        spec.display_name,
        native_session_id.as_deref().unwrap_or("pending")
    );
    println!(
        "Use Chat View to talk with this agent, or switch to Terminal to open Agent CLI mode."
    );
    let _ = std::io::stdout().flush();

    activity.sync();

    // 3. Main command poll loop against `/api/agentChatChannel`.
    // Survives every `gxserver` reload cleanly without restarting the ACP subprocess.
    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
        "abort",
        "approve",
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
            println!("\r\n[ACP] Agent process exited.");
            break;
        }
        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let poll_body = json!({
            "requestId": format!("acp-poll-{}", now_ms()),
            "params": {
                "action": "poll",
                "sessionRef": ep.session_ref,
                "agent": channel_agent,
                "pid": std::process::id(),
                "instance": ep.instance,
                "version": 1,
                "capabilities": capabilities,
            },
        });
        let resp = match ep.post_json("/api/agentChatChannel", poll_body, Duration::from_secs(28)) {
            Ok(resp) => {
                // Only pushed once by gxserver on channel open/reconnect (`newly_registered`).
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
                        if let Err(err) = apply_approval_level_and_sync_native_mode(
                            &transport,
                            &state,
                            &mode_sync_lock,
                            desired,
                        ) {
                            eprintln!(
                                "[ACP] Warning: failed to sync persisted approval level {}: {err}",
                                desired.label()
                            );
                        }
                    }
                }
                if reconnecting {
                    reconnecting = false;
                    // Re-sync live state after a gxserver reload.
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
                    let sid = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .native_session_id
                        .clone();
                    if let Some(sid) = sid {
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                    }
                    cancel_all_pending_permissions(&transport, &state);
                    ep.ack(id, true, json!({ "aborted": true }), None);
                }
                "approve" => {
                    let decision = params
                        .get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("allow");
                    let explicit = params.get("optionId").and_then(Value::as_str);
                    let target_tool_use_id = params
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let perm = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_permissions
                                .iter()
                                .position(|p| p.tool_use_id.as_deref() == Some(wanted_id))
                                .and_then(|idx| s.pending_permissions.remove(idx))
                        } else {
                            s.pending_permissions.pop_front()
                        }
                    };
                    let Some(perm) = perm else {
                        let msg = if target_tool_use_id.is_some() {
                            "That ACP approval request is no longer pending."
                        } else {
                            "No pending ACP approval."
                        };
                        ep.ack(id, false, Value::Null, Some(msg));
                        continue;
                    };
                    let outcome = if decision.eq_ignore_ascii_case("escape")
                        || decision.eq_ignore_ascii_case("cancel")
                    {
                        json!({ "outcome": "cancelled" })
                    } else if let Some(opt_id) = perm.resolve_option_id(explicit, decision) {
                        json!({ "outcome": "selected", "optionId": opt_id })
                    } else {
                        json!({ "outcome": "cancelled" })
                    };
                    match transport.respond_to_agent(perm.rpc_id, json!({ "outcome": outcome })) {
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
                    let sid = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .native_session_id
                        .clone();
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("ACP session not initialized yet."),
                        );
                        continue;
                    };
                    if !model.is_empty() {
                        if let Err(err) = transport.call(
                            "session/set_model",
                            json!({ "sessionId": sid, "modelId": model }),
                            ACP_RPC_TIMEOUT,
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
                        // Best-effort effort config option update if exposed by the agent.
                        let effort_cfg_id = state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .config_options
                            .as_array()
                            .and_then(|arr| {
                                arr.iter().find_map(|item| {
                                    let cid = item.get("id").and_then(Value::as_str)?;
                                    let cat =
                                        item.get("category").and_then(Value::as_str).unwrap_or("");
                                    (cat.contains("thought")
                                        || cat.contains("effort")
                                        || cid.contains("effort")
                                        || cid.contains("thinking"))
                                    .then(|| cid.to_string())
                                })
                            });
                        if let Some(cfg_id) = effort_cfg_id {
                            let _ = transport.call(
                                "session/set_config_option",
                                json!({
                                    "sessionId": sid,
                                    "configId": cfg_id,
                                    "value": effort,
                                }),
                                ACP_RPC_TIMEOUT,
                            );
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
                    if let Err(err) = apply_approval_level_and_sync_native_mode(
                        &transport,
                        &state,
                        &mode_sync_lock,
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
                    let (is_working, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.native_session_id.clone())
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("ACP session not initialized yet."),
                        );
                        continue;
                    };
                    if is_working && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("The ACP agent is currently working on a turn."),
                        );
                        continue;
                    }
                    let blocks = match build_prompt_blocks(&gx_paths, &text, &images) {
                        Ok(blocks) => blocks,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    if is_working && interrupt {
                        cancel_all_pending_permissions(&transport, &state);
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                        let wait_until = SystemTime::now() + Duration::from_secs(4);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    cancel_all_pending_permissions(&transport, &state);
                    let turn_gen = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.turn_generation
                    };
                    activity.sync();
                    ep.ack(
                        id,
                        true,
                        json!({ "accepted": true, "sessionId": sid }),
                        None,
                    );

                    let t_turn = transport.clone();
                    let st_turn = state.clone();
                    let act_turn = activity.clone();
                    thread::spawn(move || {
                        println!("\r\n[Non-CLI] Turn started...");
                        let _ = std::io::stdout().flush();
                        let res = t_turn.call(
                            "session/prompt",
                            json!({
                                "sessionId": sid,
                                "prompt": blocks,
                            }),
                            Duration::from_secs(60 * 60),
                        );
                        let stop_reason = match &res {
                            Ok(val) => val
                                .get("stopReason")
                                .and_then(Value::as_str)
                                .unwrap_or("end_turn")
                                .to_string(),
                            Err(err) => format!("error: {err}"),
                        };
                        println!("[Non-CLI] Turn finished ({stop_reason}).");
                        let _ = std::io::stdout().flush();
                        let (is_current_turn, orphaned_perms) = {
                            let mut s = st_turn.lock().unwrap_or_else(|p| p.into_inner());
                            if s.turn_generation == turn_gen {
                                s.working = false;
                                let drained: Vec<PendingPermission> =
                                    s.pending_permissions.drain(..).collect();
                                (true, drained)
                            } else {
                                (false, Vec::new())
                            }
                        };
                        for perm in orphaned_perms {
                            let _ = t_turn.respond_to_agent(
                                perm.rpc_id,
                                json!({ "outcome": { "outcome": "cancelled" } }),
                            );
                        }
                        if is_current_turn {
                            act_turn.sync();
                        }
                    });
                }
                "switch_to_cli" => {
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let (is_working, has_pending_perm, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_permissions.is_empty(),
                            s.native_session_id.clone(),
                        )
                    };
                    if (is_working || has_pending_perm) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("The ACP agent is currently working on a turn. Interrupt it or wait for it to finish before switching to Terminal."),
                        );
                        continue;
                    }
                    cancel_all_pending_permissions(&transport, &state);
                    if is_working {
                        if let Some(sid_ref) = sid.as_deref() {
                            let _ =
                                transport.notify("session/cancel", json!({ "sessionId": sid_ref }));
                        }
                        let wait_until = SystemTime::now() + Duration::from_millis(1500);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    shutting_down.store(true, Ordering::Release);
                    stop_child_gracefully(&transport, &mut child, &child_pid);
                    ep.ack(
                        id,
                        true,
                        json!({
                            "switched": true,
                            "nativeSessionId": sid,
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
                        Some(&format!("Unsupported ACP host command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, accept_all) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.native_session_id.clone(),
                s.approval_level.implies_cli_accept_all(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            accept_all,
        );
    }

    shutting_down.store(true, Ordering::Release);
    stop_child_gracefully(&transport, &mut child, &child_pid);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
