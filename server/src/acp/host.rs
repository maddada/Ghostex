//! `ghostex acp-host`: runs inside a session's `zmx` PTY, owns the agent's stdio
//! ACP subprocess (`agy acp --stdio`, etc.), spools every ACP event to an
//! append-only per-session file, bridges commands/approvals/state over
//! `/api/agentChatChannel`, survives `gxserver` reloads without dropping the
//! agent, and can switch in-place into the interactive CLI terminal on request.

use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{Value, json};

use crate::constants::{
    GXSERVER_LOCAL_API_HOST, GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION,
};

use super::spec::{
    AcpAgentSpec, build_cli_switch_argv, spec_for_family, transcript_path_for_session,
};

const ACP_LOAD_TIMEOUT: Duration = Duration::from_secs(90);
const ACP_RPC_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_RETRY_DELAY: Duration = Duration::from_millis(300);

struct HostArgs {
    family: String,
    cli_command: Option<String>,
    resume_session_id: Option<String>,
    accept_all: bool,
    model: Option<String>,
    effort: Option<String>,
}

fn parse_args(args: &[String]) -> Result<HostArgs, String> {
    let mut family = String::new();
    let mut cli_command = None;
    let mut resume_session_id = None;
    let mut accept_all = false;
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
            "--accept-all" => accept_all = true,
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
        accept_all,
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
        pending_prompt: Option<Value>,
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
        if let Some(prompt) = pending_prompt {
            params["sessionChatPrompt"] = prompt;
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
    file: Option<File>,
    seq: u64,
}

impl SpoolWriter {
    fn open(session_id: &str) -> Self {
        let dir = crate::paths::get_gxserver_paths(None)
            .root_dir
            .join("acp-spool");
        let _ = fs::create_dir_all(&dir);
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
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Self { file, seq: 0 }
    }

    fn append(&mut self, event: &Value) {
        self.seq += 1;
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let line = json!({
            "seq": self.seq,
            "tsMs": now_ms(),
            "event": event,
        });
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

#[derive(Clone, Debug)]
struct PendingPermission {
    rpc_id: Value,
    tool_name: String,
    summary: String,
    tool_use_id: Option<String>,
    options: Vec<PermissionOption>,
}

#[derive(Clone, Debug)]
struct PermissionOption {
    option_id: String,
    name: String,
    kind: String,
}

impl PendingPermission {
    fn to_ghostex_prompt(&self) -> Value {
        json!({
            "kind": "approval",
            "tool": self.tool_name,
            "summary": self.summary,
            "toolUseId": self.tool_use_id,
            "options": self.options.iter().map(|opt| json!({
                "optionId": opt.option_id,
                "label": opt.name,
                "kind": opt.kind,
            })).collect::<Vec<_>>(),
        })
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
        let wanted_kinds: &[&str] = match decision.trim().to_ascii_lowercase().as_str() {
            "allow" | "allow_once" | "1" | "y" | "yes" => &["allow_once", "allow_always"],
            "always" | "allow_always" | "2" => &["allow_always", "allow_once"],
            "deny" | "reject" | "reject_once" | "escape" | "n" | "no" => {
                &["reject_once", "reject_always"]
            }
            "reject_always" => &["reject_always", "reject_once"],
            _ => &["allow_once"],
        };
        for kind in wanted_kinds {
            if let Some(opt) = self.options.iter().find(|opt| opt.kind == *kind) {
                return Some(opt.option_id.clone());
            }
        }
        self.options.first().map(|opt| opt.option_id.clone())
    }
}

struct HostState {
    family: String,
    native_session_id: Option<String>,
    working: bool,
    current_model: Option<String>,
    current_effort: Option<String>,
    current_mode: Option<String>,
    available_models: Value,
    available_modes: Value,
    config_options: Value,
    pending_permission: Option<PendingPermission>,
    child_exited: bool,
}

impl HostState {
    fn new(family: &str) -> Self {
        Self {
            family: family.to_string(),
            native_session_id: None,
            working: false,
            current_model: None,
            current_effort: None,
            current_mode: None,
            available_models: Value::Array(Vec::new()),
            available_modes: Value::Array(Vec::new()),
            config_options: Value::Array(Vec::new()),
            pending_permission: None,
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
            "mode": self.current_mode,
            "availableModels": self.available_models,
            "availableModes": self.available_modes,
            "configOptions": self.config_options,
            "pendingPermission": self.pending_permission.as_ref().map(|p| p.to_ghostex_prompt()),
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
        .get("title")
        .or_else(|| tool_call.get("kind"))
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
            v.get("command")
                .or_else(|| v.get("CommandLine"))
                .or_else(|| v.get("path"))
                .or_else(|| v.get("Path"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| serde_json::to_string(v).ok())
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
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("allow_once")
                        .to_string();
                    Some(PermissionOption {
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
        options,
    }
}

fn image_mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/png",
    }
}

fn build_prompt_blocks(text: &str, image_paths: &[String]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for raw in image_paths {
        let path = PathBuf::from(raw);
        if let Ok(bytes) = fs::read(&path) {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            blocks.push(json!({
                "type": "image",
                "data": encoded,
                "mimeType": image_mime_for_path(&path),
            }));
        }
    }
    if !text.trim().is_empty() || blocks.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    blocks
}

fn stop_child_gracefully(transport: &AcpTransport, child: &mut Child) {
    transport.close_stdin();
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return,
        }
    }
}

fn exec_interactive_cli(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
    native_session_id: Option<&str>,
    accept_all: bool,
) -> i32 {
    let (program, args) = build_cli_switch_argv(spec, cli_command, native_session_id, accept_all);
    println!(
        "\r\nSwitching to interactive {} terminal...",
        spec.display_name
    );
    let _ = std::io::stdout().flush();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(&program).args(&args).exec();
        eprintln!("Failed to launch {program}: {err}");
        1
    }

    #[cfg(not(unix))]
    {
        match Command::new(&program).args(&args).status() {
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
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let endpoint = GxserverEndpoint::from_env();
    let spool_session_id = endpoint
        .as_ref()
        .map(|ep| ep.session_id.clone())
        .unwrap_or_else(|| format!("local-{}", std::process::id()));

    println!(
        "Ghostex ACP Host · {} · Starting ACP server...",
        spec.display_name
    );
    let _ = std::io::stdout().flush();

    let Some((prog, argv)) = spec.acp_argv.split_first() else {
        eprintln!("Invalid ACP server command for {}", spec.family);
        return 1;
    };
    let mut child = match Command::new(prog)
        .args(argv)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            eprintln!("Failed to start `{}`: {err}", spec.acp_argv.join(" "));
            return 1;
        }
    };
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
    let state = Arc::new(Mutex::new(HostState::new(spec.family)));
    let shutting_down = Arc::new(AtomicBool::new(false));

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

    // Stdout JSON-RPC reader loop.
    {
        let t_out = transport.clone();
        let st_out = state.clone();
        let ep_out = endpoint.clone();
        let fam = spec.family.to_string();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                    continue;
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
                    continue;
                }
                if has_method && has_id {
                    let method = msg
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
                    let params = msg.get("params").cloned().unwrap_or(Value::Null);
                    if method == "session/request_permission" {
                        let perm = parse_permission_request(rpc_id, &params);
                        println!(
                            "\r\n[ACP] Waiting for approval: {} ({})",
                            perm.tool_name, perm.summary
                        );
                        let _ = std::io::stdout().flush();
                        let prompt_json = perm.to_ghostex_prompt();
                        let native_id = {
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            s.pending_permission = Some(perm);
                            s.native_session_id.clone()
                        };
                        if let Some(ep) = ep_out.as_ref() {
                            ep.report_activity(
                                &fam,
                                "waiting",
                                native_id.as_deref(),
                                Some(prompt_json),
                            );
                        }
                    } else {
                        t_out.respond_error_to_agent(
                            rpc_id,
                            -32601,
                            &format!("Unsupported client method: {method}"),
                        );
                    }
                    continue;
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
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.current_mode = Some(mode_id.to_string());
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
            }
            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
            s.child_exited = true;
            s.working = false;
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
        stop_child_gracefully(&transport, &mut child);
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
            stop_child_gracefully(&transport, &mut child);
            return 1;
        }
    };

    let native_session_id = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        apply_session_metadata(&mut s, &session_result);
        s.native_session_id.clone()
    };

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
        "Ghostex ACP Host · {} · Ready (conversation: {})",
        spec.display_name,
        native_session_id.as_deref().unwrap_or("pending")
    );
    println!(
        "Use Chat View to talk with this agent, or switch to Terminal to open the interactive CLI."
    );
    let _ = std::io::stdout().flush();

    if let Some(ep) = endpoint.as_ref() {
        ep.report_activity(spec.family, "idle", native_session_id.as_deref(), None);
    }

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
                if reconnecting {
                    reconnecting = false;
                    // Re-sync live state after a gxserver reload.
                    let (act, sid, prompt) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let act = if s.pending_permission.is_some() {
                            "waiting"
                        } else if s.working {
                            "working"
                        } else {
                            "idle"
                        };
                        (
                            act,
                            s.native_session_id.clone(),
                            s.pending_permission.as_ref().map(|p| p.to_ghostex_prompt()),
                        )
                    };
                    ep.report_activity(spec.family, act, sid.as_deref(), prompt);
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
                    let pending = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .pending_permission
                        .take();
                    if let Some(perm) = pending {
                        let _ = transport.respond_to_agent(
                            perm.rpc_id,
                            json!({ "outcome": { "outcome": "cancelled" } }),
                        );
                    }
                    ep.ack(id, true, json!({ "aborted": true }), None);
                }
                "approve" => {
                    let decision = params
                        .get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("allow");
                    let explicit = params.get("optionId").and_then(Value::as_str);
                    let perm = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .pending_permission
                        .take();
                    let Some(perm) = perm else {
                        ep.ack(id, false, Value::Null, Some("No pending ACP approval."));
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
                            let sid = state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .native_session_id
                                .clone();
                            ep.report_activity(spec.family, "working", sid.as_deref(), None);
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
                    match transport.call(
                        "session/set_mode",
                        json!({ "sessionId": sid, "modeId": mode }),
                        ACP_RPC_TIMEOUT,
                    ) {
                        Ok(_) => {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.current_mode = Some(mode);
                            let snap = s.snapshot_json();
                            ep.ack(id, true, snap, None);
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
                    if is_working && interrupt {
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                        let wait_until = SystemTime::now() + Duration::from_secs(4);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    let blocks = build_prompt_blocks(&text, &images);
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.pending_permission = None;
                    }
                    ep.report_activity(spec.family, "working", Some(&sid), None);
                    ep.ack(
                        id,
                        true,
                        json!({ "accepted": true, "sessionId": sid }),
                        None,
                    );

                    let t_turn = transport.clone();
                    let st_turn = state.clone();
                    let ep_turn = ep.clone();
                    let fam = spec.family.to_string();
                    thread::spawn(move || {
                        println!("\r\n[ACP] Turn started...");
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
                        println!("[ACP] Turn finished ({stop_reason}).");
                        let _ = std::io::stdout().flush();
                        {
                            let mut s = st_turn.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = false;
                            s.pending_permission = None;
                        }
                        ep_turn.report_activity(&fam, "idle", Some(&sid), None);
                    });
                }
                "switch_to_cli" => {
                    let (is_working, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.native_session_id.clone())
                    };
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
                    stop_child_gracefully(&transport, &mut child);
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
        let sid = state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .native_session_id
            .clone();
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            parsed.accept_all,
        );
    }

    stop_child_gracefully(&transport, &mut child);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    0
}
