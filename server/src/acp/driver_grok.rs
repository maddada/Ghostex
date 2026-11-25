//! Native `grok agent stdio` ACP + xAI extension driver for `ghostex acp-host --agent grok`.
//!
//! Speaks Grok CLI's stdio ACP JSON-RPC protocol with xAI's extension notifications and reverse
//! requests directly in Rust:
//! - Enforces Ghostex's four live approval levels (`Ask`, `Auto-edits`, `Auto`, `Full permissions`)
//!   matching Grok's `--permission-mode default`, `--permission-mode auto`, and `--always-approve`
//!   launch modes, with clean subprocess recycle when switching into/out of `Auto` or `--always-approve`.
//! - Sends `initialize` with `_meta: { clientType: "extension" }` and authenticates via
//!   `xai.api_key` (when `XAI_API_KEY` is set) or `cached_token`.
//! - Handles `session/load` replay-idle readiness (2s idle gap when Grok leaves `session/load`
//!   unanswered after replay finishes) and falls back to `session/resume`.
//! - Races `session/prompt` RPC responses against `x.ai/session/prompt_complete`,
//!   `_x.ai/session/prompt_complete`, and `turn_completed` session notifications.
//! - Supports `_x.ai/interject` mid-turn steering (with optional images), `session/cancel` with
//!   `_meta: { cancelTrigger: "ctrl_c" }` and process-group restart on stuck cancellation,
//!   `x.ai/ask_user_question` / `_x.ai/ask_user_question` interactive questions,
//!   `x.ai/exit_plan_mode` / `_x.ai/exit_plan_mode` plan gates, protocol v1/v2 model + reasoning
//!   effort selection, image prompt blocks, leading `$skill` -> `/skill` expansion via
//!   `grok inspect --json`, MCP `use_tool` normalization, background subagent / task / monitor
//!   tracking (`agentFleet`), wake continuation turns, context usage, `gxserver` reload survival,
//!   and Switch to Terminal (`grok -r <sessionId>`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
    build_acp_server_argv, is_allowed_connector_env_key, transcript_path_for_session, AcpAgentSpec,
};

const GROK_RPC_TIMEOUT: Duration = Duration::from_secs(30);
const GROK_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
const GROK_LOAD_TIMEOUT: Duration = Duration::from_secs(90);
const GROK_LOAD_REPLAY_IDLE_GAP: Duration = Duration::from_secs(2);
const GROK_SKILLS_PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const GROK_SKILLS_CACHE_TTL: Duration = Duration::from_secs(30);
const GROK_COMPLETED_PROMPT_ID_LIMIT: usize = 128;

const GROK_API_KEY_ENV: &str = "XAI_API_KEY";
const GROK_OAUTH2_REFERRER_ENV: &str = "GROK_OAUTH2_REFERRER";
const GROK_OAUTH2_REFERRER_VALUE: &str = "ghostex";
const GROK_AUTH_METHOD_API_KEY: &str = "xai.api_key";
const GROK_AUTH_METHOD_CACHED_TOKEN: &str = "cached_token";
const GROK_DEFAULT_MODEL_SLUG: &str = "grok-build";
const GROK_ALLOW_EDITS_SESSION_OPTION_ID: &str = "allow-edits-session";
const XAI_RATE_LIMITED_ERROR_CODE: i64 = -32003;
const XAI_TASK_COMPLETED_PROMPT_ID_PREFIX: &str = "task-completed-";
const XAI_SUBAGENT_COMPLETED_PROMPT_ID_PREFIX: &str = "subagent-completed-";
const XAI_INTERJECT_METHOD: &str = "_x.ai/interject";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrokLaunchMode {
    Default,
    Auto,
    FullAccess,
}

impl GrokLaunchMode {
    fn from_approval_level(level: AcpApprovalLevel, read_only: bool) -> Self {
        if read_only {
            return Self::Default;
        }
        match level {
            AcpApprovalLevel::Auto => Self::Auto,
            AcpApprovalLevel::FullAccess => Self::FullAccess,
            AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits => Self::Default,
        }
    }

    fn spawn_args(self) -> Vec<String> {
        match self {
            Self::Auto => vec![
                "--permission-mode".to_string(),
                "auto".to_string(),
                "agent".to_string(),
                "stdio".to_string(),
            ],
            Self::FullAccess => vec![
                "agent".to_string(),
                "--always-approve".to_string(),
                "stdio".to_string(),
            ],
            Self::Default => vec![
                "--permission-mode".to_string(),
                "default".to_string(),
                "agent".to_string(),
                "stdio".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug)]
enum GrokPromptOutcome {
    Success {
        stop_reason: String,
        usage: Option<Value>,
    },
    Error {
        code: i64,
        message: String,
        is_usage_limit: bool,
    },
}

#[derive(Clone, Debug)]
enum PendingPromptKind {
    Permission,
    AskUserQuestion { raw_questions: Vec<Value> },
    ExitPlanMode,
}

#[derive(Clone, Debug)]
struct GrokPendingRequest {
    rpc_id: Value,
    tool_name: String,
    summary: String,
    tool_use_id: Option<String>,
    reason: Option<String>,
    options: Vec<AcpPermissionOption>,
    kind: PendingPromptKind,
    custom_prompt_override: Option<Value>,
}

impl GrokPendingRequest {
    fn is_question(&self) -> bool {
        matches!(self.kind, PendingPromptKind::AskUserQuestion { .. })
    }

    fn to_pending_permission(&self) -> PendingPermission {
        PendingPermission {
            rpc_id: self.rpc_id.clone(),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            tool_use_id: self.tool_use_id.clone(),
            reason: self.reason.clone(),
            is_question: self.is_question(),
            options: self.options.clone(),
            custom_prompt_override: self.custom_prompt_override.clone(),
        }
    }

    fn to_ghostex_prompt(&self) -> Value {
        self.to_pending_permission().to_ghostex_prompt()
    }

    fn resolve_permission_option_id(
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
            "allow" | "allow_once" | "1" | "y" | "yes" | "accept" => {
                (false, &["allow_once", "allow_always"])
            }
            "always" | "allow_always" | "acceptforsession" | "2" => {
                if let Some(opt) = self.options.iter().find(|opt| {
                    opt.kind == "allow_always"
                        && opt.option_id.trim() == GROK_ALLOW_EDITS_SESSION_OPTION_ID
                }) {
                    return Some(opt.option_id.clone());
                }
                (false, &["allow_always", "allow_once"])
            }
            "deny" | "decline" | "reject" | "reject_once" | "escape" | "cancel" | "\x1b" | "n"
            | "no" => (true, &["reject_once", "reject_always"]),
            "reject_always" => (true, &["reject_always", "reject_once"]),
            _ => (false, &["allow_once"]),
        };
        for kind in wanted_kinds {
            if let Some(opt) = self.options.iter().find(|opt| opt.kind == *kind) {
                return Some(opt.option_id.clone());
            }
        }
        if is_deny {
            return None;
        }
        self.options
            .iter()
            .find(|opt| !opt.kind.starts_with("reject"))
            .map(|opt| opt.option_id.clone())
    }
}

#[derive(Clone, Debug)]
struct GrokSubagentEntry {
    child_session_id: String,
    name: String,
    task: Option<String>,
    model: Option<String>,
    started_at_ms: i64,
    working: bool,
    done: bool,
    elapsed_seconds: Option<u64>,
    tokens: Option<String>,
}

#[derive(Clone, Debug)]
struct GrokBackgroundTaskEntry {
    running: bool,
}

#[derive(Clone, Debug)]
struct GrokToolInfo {
    title: String,
    raw_input: Value,
    raw_output: Value,
}

struct PendingPromptWaiter {
    session_id: String,
    prompt_id: String,
    tx: mpsc::Sender<GrokPromptOutcome>,
}

struct GrokState {
    family: String,
    protocol_version: u64,
    native_session_id: Option<String>,
    working: bool,
    wake_prompt_id: Option<String>,
    replaying_load: bool,
    last_replay_activity_ms: Option<i64>,
    last_turn_interrupted: bool,
    turn_generation: u64,
    approval_level: AcpApprovalLevel,
    spawned_launch_mode: GrokLaunchMode,
    read_only: bool,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    available_models: Value,
    config_options: Value,
    pending_requests: VecDeque<GrokPendingRequest>,
    pending_prompt_waiters: Vec<PendingPromptWaiter>,
    completed_prompt_ids: VecDeque<String>,
    active_tools: HashMap<String, GrokToolInfo>,
    subagent_order: Vec<String>,
    subagents: HashMap<String, GrokSubagentEntry>,
    background_tasks: HashMap<String, GrokBackgroundTaskEntry>,
    last_proposed_plan_markdown: Option<String>,
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
    fatal_error: Option<String>,
    last_error: Option<(String, Option<String>)>,
    last_event_at_ms: i64,
    child_exited: bool,
}

impl GrokState {
    fn new(
        family: &str,
        approval_level: AcpApprovalLevel,
        spawned_launch_mode: GrokLaunchMode,
        read_only: bool,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            family: family.to_string(),
            protocol_version: 1,
            native_session_id: None,
            working: false,
            wake_prompt_id: None,
            replaying_load: false,
            last_replay_activity_ms: None,
            last_turn_interrupted: false,
            turn_generation: 0,
            approval_level,
            spawned_launch_mode,
            read_only,
            cwd,
            writable_roots,
            current_model: None,
            current_effort: None,
            available_models: Value::Array(Vec::new()),
            config_options: Value::Array(Vec::new()),
            pending_requests: VecDeque::new(),
            pending_prompt_waiters: Vec::new(),
            completed_prompt_ids: VecDeque::new(),
            active_tools: HashMap::new(),
            subagent_order: Vec::new(),
            subagents: HashMap::new(),
            background_tasks: HashMap::new(),
            last_proposed_plan_markdown: None,
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
            fatal_error: None,
            last_error: None,
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

    fn remember_completed_prompt_id(&mut self, prompt_id: &str) {
        if prompt_id.is_empty()
            || self
                .completed_prompt_ids
                .iter()
                .any(|existing| existing == prompt_id)
        {
            return;
        }
        self.completed_prompt_ids.push_back(prompt_id.to_string());
        while self.completed_prompt_ids.len() > GROK_COMPLETED_PROMPT_ID_LIMIT {
            self.completed_prompt_ids.pop_front();
        }
    }

    fn is_completed_prompt_id(&self, prompt_id: &str) -> bool {
        self.completed_prompt_ids
            .iter()
            .any(|existing| existing == prompt_id)
    }

    fn register_prompt_waiter(
        &mut self,
        session_id: String,
        prompt_id: String,
    ) -> mpsc::Receiver<GrokPromptOutcome> {
        let (tx, rx) = mpsc::channel();
        self.pending_prompt_waiters.push(PendingPromptWaiter {
            session_id,
            prompt_id,
            tx,
        });
        rx
    }

    fn remove_prompt_waiter(&mut self, prompt_id: &str) {
        self.pending_prompt_waiters
            .retain(|entry| entry.prompt_id != prompt_id);
    }

    fn resolve_prompt_waiter(
        &mut self,
        session_id: &str,
        prompt_id: Option<&str>,
        outcome: GrokPromptOutcome,
    ) -> bool {
        if let Some(pid) = prompt_id {
            if self.is_completed_prompt_id(pid) {
                return false;
            }
        }
        let idx = if let Some(pid) = prompt_id {
            self.pending_prompt_waiters
                .iter()
                .position(|w| w.session_id == session_id && w.prompt_id == pid)
        } else {
            let matching: Vec<usize> = self
                .pending_prompt_waiters
                .iter()
                .enumerate()
                .filter_map(|(i, w)| (w.session_id == session_id).then_some(i))
                .collect();
            if matching.len() == 1 {
                matching.first().copied()
            } else {
                None
            }
        };
        let Some(idx) = idx else {
            return false;
        };
        let waiter = self.pending_prompt_waiters.remove(idx);
        self.remember_completed_prompt_id(&waiter.prompt_id);
        let _ = waiter.tx.send(outcome);
        true
    }

    fn abort_all_prompt_waiters(&mut self) {
        let drained: Vec<PendingPromptWaiter> = self.pending_prompt_waiters.drain(..).collect();
        for waiter in drained {
            self.remember_completed_prompt_id(&waiter.prompt_id);
            let _ = waiter.tx.send(GrokPromptOutcome::Success {
                stop_reason: "cancelled".to_string(),
                usage: None,
            });
        }
    }

    fn upsert_subagent(
        &mut self,
        child_session_id: &str,
        name: Option<&str>,
        task: Option<&str>,
        model: Option<&str>,
        working: Option<bool>,
        done: Option<bool>,
        elapsed_seconds: Option<u64>,
        tokens_used: Option<u64>,
    ) {
        let id = child_session_id.trim();
        if id.is_empty() {
            return;
        }
        let is_new = !self.subagents.contains_key(id);
        if is_new {
            self.subagent_order.push(id.to_string());
        }
        let entry = self
            .subagents
            .entry(id.to_string())
            .or_insert_with(|| GrokSubagentEntry {
                child_session_id: id.to_string(),
                name: name
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("subagent")
                    .to_string(),
                task: None,
                model: None,
                started_at_ms: now_ms(),
                working: true,
                done: false,
                elapsed_seconds: None,
                tokens: None,
            });
        if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
            entry.name = n.to_string();
        }
        if let Some(t) = task.map(str::trim).filter(|s| !s.is_empty()) {
            entry.task = Some(t.to_string());
        }
        if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
            entry.model = Some(m.to_string());
        }
        if let Some(w) = working {
            entry.working = w;
        }
        if let Some(d) = done {
            entry.done = d;
            if d {
                entry.working = false;
            }
        }
        if let Some(secs) = elapsed_seconds {
            entry.elapsed_seconds = Some(secs);
        }
        if let Some(toks) = tokens_used.filter(|t| *t > 0) {
            entry.tokens = Some(format_compact_tokens(toks));
        }
    }

    fn model_context_window(&self, model_id: Option<&str>) -> Option<u64> {
        let target = model_id
            .or(self.current_model.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        self.available_models
            .as_array()?
            .iter()
            .find(|entry| {
                entry
                    .get("modelId")
                    .or_else(|| entry.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    == Some(target)
            })
            .and_then(|entry| entry.pointer("/_meta/totalContextTokens"))
            .and_then(Value::as_u64)
            .filter(|n| *n > 0)
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
        let used = self.usage_used_tokens.filter(|u| *u > 0);
        let size = self
            .usage_window_size
            .filter(|s| *s > 0)
            .or_else(|| self.model_context_window(None));
        let Some(used) = used else {
            return Value::Null;
        };
        let mut obj = Map::new();
        obj.insert("usedTokens".to_string(), json!(used));
        if let Some(size) = size {
            obj.insert("windowSize".to_string(), json!(size));
            let pct = ((used as f64 / size as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u32;
            obj.insert("usedPercentage".to_string(), json!(pct));
        }
        Value::Object(obj)
    }

    fn agent_fleet_json(&self) -> Value {
        if self.subagent_order.is_empty() {
            return Value::Null;
        }
        let now = now_ms();
        let agents: Vec<Value> = self
            .subagent_order
            .iter()
            .filter_map(|id| self.subagents.get(id))
            .map(|entry| {
                let elapsed = entry.elapsed_seconds.or_else(|| {
                    (now >= entry.started_at_ms)
                        .then_some(((now - entry.started_at_ms) / 1000) as u64)
                });
                let mut obj = Map::new();
                obj.insert("id".to_string(), json!(entry.child_session_id));
                obj.insert("startedAt".to_string(), json!(entry.started_at_ms));
                obj.insert(
                    "status".to_string(),
                    json!(if entry.working { "working" } else { "idle" }),
                );
                if entry.done && !entry.working {
                    obj.insert("done".to_string(), json!(true));
                }
                obj.insert("name".to_string(), json!(entry.name));
                if let Some(model) = entry.model.as_deref() {
                    obj.insert("model".to_string(), json!(model));
                }
                if let Some(task) = entry.task.as_deref() {
                    obj.insert("task".to_string(), json!(task));
                }
                if let Some(secs) = elapsed {
                    obj.insert("elapsedSeconds".to_string(), json!(secs));
                }
                if let Some(tokens) = entry.tokens.as_deref() {
                    obj.insert("tokens".to_string(), json!(tokens));
                }
                Value::Object(obj)
            })
            .collect();
        if agents.is_empty() {
            return Value::Null;
        }
        json!({
            "agents": agents,
            "detectedAt": now_iso_rfc3339(),
            "stale": false,
        })
    }

    fn snapshot_json(&self) -> Value {
        let transcript_path = self
            .native_session_id
            .as_deref()
            .and_then(|id| transcript_path_for_session(&self.family, id))
            .map(|p| p.to_string_lossy().to_string());
        let last_error = self.last_error.as_ref().map(|(msg, code)| {
            let mut obj = Map::new();
            obj.insert("message".to_string(), json!(msg));
            if let Some(c) = code.as_deref() {
                obj.insert("code".to_string(), json!(c));
            }
            Value::Object(obj)
        });
        json!({
            "family": self.family,
            "nativeSessionId": self.native_session_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": match self.spawned_launch_mode {
                GrokLaunchMode::Auto => "auto",
                GrokLaunchMode::FullAccess => "always-approve",
                GrokLaunchMode::Default => "default",
            },
            "nativeAutoActive": self.spawned_launch_mode == GrokLaunchMode::Auto,
            "nativeAutoAvailable": true,
            "availableModels": self.available_models,
            "availableModes": [
                { "id": "default", "name": "Ask" },
                { "id": "auto", "name": "Auto" },
                { "id": "always-approve", "name": "Full permissions" },
            ],
            "configOptions": self.config_options,
            "pendingPermission": self.pending_requests.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "agentFleet": self.agent_fleet_json(),
            "sessionTitle": self.session_title,
            "fatalError": self.fatal_error,
            "lastError": last_error,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": transcript_path,
            "childExited": self.child_exited,
        })
    }
}

struct GrokTransport {
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    spool: Mutex<SpoolWriter>,
}

impl GrokTransport {
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
            return Err("Grok Native host stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Grok Native host: {e}"))
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

    fn start_call(
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

    fn finish_call(
        &self,
        id: u64,
        method: &str,
        rx: mpsc::Receiver<Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if let Some(error) = resp.get("error") {
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Grok Native request failed");
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
                Err(format!("Grok Native request `{method}` timed out."))
            }
        }
    }

    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let (id, rx) = self.start_call(method, params)?;
        self.finish_call(id, method, rx, timeout)
    }

    fn respond_to_agent(&self, rpc_id: Value, result: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result,
        }))
    }

    fn respond_error_to_agent(&self, rpc_id: Value, code: i64, message: &str) {
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

struct GrokProcess {
    child: Child,
    child_lifetime: super::child_lifetime::ChildLifetime,
    transport: Arc<GrokTransport>,
}

struct GrokReaderExitGuard {
    state: Arc<Mutex<GrokState>>,
    intentional_stop: Arc<AtomicBool>,
}

impl Drop for GrokReaderExitGuard {
    fn drop(&mut self) {
        if !self.intentional_stop.load(Ordering::Acquire) {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            s.child_exited = true;
            s.working = false;
            s.abort_all_prompt_waiters();
        }
    }
}

#[derive(Clone)]
struct GrokActivityReporter {
    tx: Option<mpsc::Sender<Option<String>>>,
}

impl GrokActivityReporter {
    fn start(
        endpoint: Option<GxserverEndpoint>,
        state: Arc<Mutex<GrokState>>,
        family: String,
    ) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<Option<String>>();
        thread::spawn(move || {
            let mut pending_next: Option<Option<String>> = None;
            loop {
                let mut stop_error = match pending_next.take() {
                    Some(v) => v,
                    None => match rx.recv() {
                        Ok(v) => v,
                        Err(_) => return,
                    },
                };
                while let Ok(next_err) = rx.try_recv() {
                    if next_err.is_some() {
                        stop_error = next_err;
                    }
                }
                let (act, sid, perm, interrupted) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s
                        .pending_requests
                        .front()
                        .map(|r| r.to_pending_permission());
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
                    )
                };
                let mut ok = ep.report_activity_with_error(
                    &family,
                    act,
                    sid.as_deref(),
                    None,
                    perm.as_ref(),
                    interrupted,
                    stop_error.as_deref(),
                );
                for delay_ms in [500u64, 1_000, 2_000] {
                    if ok {
                        break;
                    }
                    match rx.recv_timeout(Duration::from_millis(delay_ms)) {
                        Ok(next_err) => {
                            pending_next = Some(next_err.or(stop_error));
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let (act, sid, perm, interrupted) = {
                                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                                let front = s
                                    .pending_requests
                                    .front()
                                    .map(|r| r.to_pending_permission());
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
                                )
                            };
                            ok = ep.report_activity_with_error(
                                &family,
                                act,
                                sid.as_deref(),
                                None,
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
            let _ = tx.send(None);
        }
    }

    fn sync_with_error(&self, code: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Some(code));
        }
    }
}

struct CachedGrokSkills {
    fetched_at: Instant,
    enabled_names: HashSet<String>,
}

fn format_compact_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tokens")
    }
}

fn non_empty_str(val: Option<&Value>) -> Option<&str> {
    val.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn is_valid_grok_reasoning_effort_token(value: &str) -> bool {
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

fn normalize_grok_reasoning_effort(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    is_valid_grok_reasoning_effort_token(trimmed).then(|| trimmed.to_ascii_lowercase())
}

fn resolve_grok_auth_method_id() -> &'static str {
    if env::var(GROK_API_KEY_ENV)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        GROK_AUTH_METHOD_API_KEY
    } else {
        GROK_AUTH_METHOD_CACHED_TOKEN
    }
}

fn is_xai_uuid(s: &str) -> bool {
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

fn find_uuid_after_prefix(text: &str, prefix_lower: &str) -> Option<String> {
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

fn extract_xai_child_session_id(output: &str) -> Option<String> {
    find_uuid_after_prefix(output, "agent id:")
        .or_else(|| find_uuid_after_prefix(output, "subagent_id:"))
        .or_else(|| find_uuid_after_prefix(output, "=== task "))
}

fn is_xai_async_spawn_ack(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    if lower.contains("subagent started in background") {
        return true;
    }
    find_uuid_after_prefix(output, "subagent_id:").is_some()
        && lower.contains("get_command_or_subagent_output")
}

fn is_xai_monitor_start_ack(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("monitor started") && lower.contains("task ")
}

fn decode_byte_text(value: Option<&Value>) -> Option<String> {
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

fn xai_tool_output_text(raw_output: &Value, content: Option<&Value>) -> Option<String> {
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

fn is_generic_tool_title(title: Option<&str>) -> bool {
    let key = title.unwrap_or_default().trim().to_ascii_lowercase();
    key.is_empty() || matches!(key.as_str(), "tool" | "tool call" | "terminal")
}

fn resolve_xai_tool_title(
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

fn is_xai_spawn_or_task_tool(title: &str, raw_input: &Value) -> bool {
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

fn is_xai_get_output_tool(title: &str, raw_input: &Value, raw_output: &Value) -> bool {
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

fn is_xai_monitor_tool(
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

fn xai_background_shell_task_id(raw_output: &Value) -> Option<String> {
    (non_empty_str(raw_output.get("type")) == Some("BackgroundTaskStarted"))
        .then(|| non_empty_str(raw_output.get("task_id")).map(str::to_string))
        .flatten()
}

fn extract_xai_monitor_task_id(
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

fn is_grok_plan_markdown_path(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty()
        || !normalized.ends_with("/plan.md")
        || normalized.split('/').any(|seg| seg == "..")
    {
        return false;
    }
    normalized.contains("/.grok/sessions/") || normalized.contains("/sessions/")
}

fn extract_grok_plan_markdown_from_update(update: &Value) -> Option<String> {
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

fn extract_leading_skill_mention(text: &str) -> Option<(String, &str)> {
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

fn discover_grok_enabled_skills(
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

fn rewrite_leading_grok_skill_mention(
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

fn build_grok_prompt_blocks(
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

fn build_grok_interject_image_blocks(
    paths: &crate::paths::GxserverPaths,
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
    Ok(blocks)
}

fn apply_grok_model_state(state: &mut GrokState, models: &Value) {
    use ghostex_gx_chat_core::menus::catalog::{
        is_placeholder_model_label, is_placeholder_model_value,
    };
    if let Some(cur) = models
        .get("currentModelId")
        .or_else(|| models.get("currentModel"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !is_placeholder_model_value(m) && *m != GROK_DEFAULT_MODEL_SLUG)
    {
        state.current_model = Some(cur.to_string());
    }
    if let Some(avail) = models.get("availableModels").and_then(Value::as_array) {
        let filtered: Vec<Value> = avail
            .iter()
            .filter(|entry| {
                let id = entry
                    .get("modelId")
                    .or_else(|| entry.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let name = entry
                    .get("name")
                    .or_else(|| entry.get("displayName"))
                    .and_then(Value::as_str)
                    .unwrap_or(id);
                !is_placeholder_model_value(id) && !is_placeholder_model_label(name)
            })
            .cloned()
            .collect();
        state.available_models = Value::Array(filtered);
    }
    if let Some(cur_model) = state.current_model.as_deref() {
        if let Some(entry) = state.available_models.as_array().and_then(|arr| {
            arr.iter().find(|e| {
                e.get("modelId")
                    .or_else(|| e.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    == Some(cur_model)
            })
        }) {
            if state.current_effort.is_none() {
                if let Some(eff) = normalize_grok_reasoning_effort(
                    entry
                        .pointer("/_meta/reasoningEffort")
                        .and_then(Value::as_str),
                ) {
                    state.current_effort = Some(eff);
                }
            }
            if state.usage_window_size.is_none() {
                if let Some(window) = entry
                    .pointer("/_meta/totalContextTokens")
                    .and_then(Value::as_u64)
                    .filter(|n| *n > 0)
                {
                    state.usage_window_size = Some(window);
                }
            }
        }
    }
}

fn apply_grok_session_metadata(state: &mut GrokState, result: &Value) {
    if let Some(sid) = non_empty_str(result.get("sessionId")) {
        state.native_session_id = Some(sid.to_string());
    }
    if let Some(models) = result.get("models") {
        apply_grok_model_state(state, models);
    } else if let Some(models) = result.pointer("/_meta/modelState") {
        apply_grok_model_state(state, models);
    }
    if let Some(configs) = result.get("configOptions") {
        state.config_options = configs.clone();
        if let Some(arr) = configs.as_array() {
            for opt in arr {
                if opt.get("category").and_then(Value::as_str) == Some("thought_level") {
                    if let Some(val) = normalize_grok_reasoning_effort(
                        opt.get("currentValue").and_then(Value::as_str),
                    ) {
                        state.current_effort = Some(val);
                    }
                }
            }
        }
    }
}

fn grok_model_offered_efforts(available_models: &Value, model_id: &str) -> Vec<String> {
    let Some(entry) = available_models.as_array().and_then(|arr| {
        arr.iter().find(|e| {
            e.get("modelId")
                .or_else(|| e.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                == Some(model_id)
        })
    }) else {
        return Vec::new();
    };
    if entry
        .pointer("/_meta/supportsReasoningEffort")
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Vec::new();
    }
    let Some(efforts) = entry
        .pointer("/_meta/reasoningEfforts")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    efforts
        .iter()
        .filter_map(|item| {
            let raw = non_empty_str(item.get("value")).or_else(|| non_empty_str(item.get("id")))?;
            normalize_grok_reasoning_effort(Some(raw))
        })
        .collect()
}

fn find_thought_level_config_option_id(config_options: &Value) -> Option<String> {
    config_options.as_array()?.iter().find_map(|opt| {
        let is_select = opt
            .get("type")
            .and_then(Value::as_str)
            .is_none_or(|t| t == "select");
        let cat = opt.get("category").and_then(Value::as_str).unwrap_or("");
        let id = opt.get("id").and_then(Value::as_str)?;
        (is_select && (cat == "thought_level" || id.contains("effort") || id.contains("thinking")))
            .then(|| id.to_string())
    })
}

fn apply_grok_model_and_effort(
    transport: &GrokTransport,
    state: &Mutex<GrokState>,
    requested_model: Option<&str>,
    requested_effort: Option<&str>,
) -> Result<(), String> {
    let (sid, proto_ver, current_model, avail, config_opts) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (
            s.native_session_id.clone(),
            s.protocol_version,
            s.current_model.clone(),
            s.available_models.clone(),
            s.config_options.clone(),
        )
    };
    let Some(sid) = sid else {
        return Err("Grok Native session is not initialized yet.".to_string());
    };
    let clean_model = requested_model
        .map(str::trim)
        .filter(|m| {
            !m.is_empty()
                && *m != GROK_DEFAULT_MODEL_SLUG
                && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
        })
        .map(str::to_string);
    let clean_effort = normalize_grok_reasoning_effort(requested_effort);
    let target_model = clean_model.clone().or(current_model.clone());

    if proto_ver <= 1 {
        if let Some(target) = target_model {
            let offered = grok_model_offered_efforts(&avail, &target);
            let valid_effort = clean_effort
                .as_ref()
                .filter(|eff| offered.is_empty() || offered.iter().any(|o| o == *eff))
                .cloned();
            let mut params = json!({
                "sessionId": sid,
                "modelId": target,
            });
            if let Some(ref eff) = valid_effort {
                params["_meta"] = json!({ "reasoningEffort": eff });
            }
            let res = transport.call("session/set_model", params, GROK_RPC_TIMEOUT);
            if let Err(err) = res {
                if valid_effort.is_some() {
                    // Retry without reasoningEffort metadata if Grok rejected the effort.
                    transport.call(
                        "session/set_model",
                        json!({ "sessionId": sid, "modelId": target }),
                        GROK_RPC_TIMEOUT,
                    )?;
                } else {
                    return Err(err);
                }
            }
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.current_model = Some(target.clone());
            if let Some(window) = s.model_context_window(Some(&target)) {
                s.usage_window_size = Some(window);
            }
            if let Some(eff) = valid_effort.or(clean_effort.clone()) {
                s.current_effort = Some(eff);
            }
        }
        // Also apply `thought_level` config option when exposed by Grok 1.0.93+.
        if let Some(ref eff) = clean_effort {
            if let Some(cfg_id) = find_thought_level_config_option_id(&config_opts) {
                let _ = transport.call(
                    "session/set_config_option",
                    json!({
                        "sessionId": sid,
                        "configId": cfg_id,
                        "value": eff,
                    }),
                    GROK_RPC_TIMEOUT,
                );
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.current_effort = Some(eff.clone());
            }
        }
        return Ok(());
    }

    // Protocol v2+: model switch via session/set_model, effort via thought_level config option.
    if let Some(target) = clean_model {
        transport.call(
            "session/set_model",
            json!({ "sessionId": sid, "modelId": target }),
            GROK_RPC_TIMEOUT,
        )?;
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.current_model = Some(target.clone());
        if let Some(window) = s.model_context_window(Some(&target)) {
            s.usage_window_size = Some(window);
        }
    }
    if let Some(eff) = clean_effort {
        let cfg_id = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            find_thought_level_config_option_id(&s.config_options)
        };
        if let Some(cfg_id) = cfg_id {
            let _ = transport.call(
                "session/set_config_option",
                json!({
                    "sessionId": sid,
                    "configId": cfg_id,
                    "value": eff,
                }),
                GROK_RPC_TIMEOUT,
            );
        }
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.current_effort = Some(eff);
    }
    Ok(())
}

fn parse_grok_permission_request(rpc_id: Value, params: &Value) -> GrokPendingRequest {
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_use_id = non_empty_str(tool_call.get("toolCallId")).map(str::to_string);
    let tool_name = resolve_xai_tool_title(
        tool_call
            .get("name")
            .or_else(|| tool_call.get("title"))
            .or_else(|| tool_call.get("kind"))
            .and_then(Value::as_str),
        tool_call.get("_meta"),
        tool_call.get("rawInput").unwrap_or(&Value::Null),
        tool_call.get("rawOutput").unwrap_or(&Value::Null),
    )
    .unwrap_or_else(|| "Tool".to_string());
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
                "url",
                "query",
                "description",
            ]
            .iter()
            .find_map(|key| non_empty_str(v.get(*key)))
            .map(str::to_string)
            .or_else(|| non_empty_str(tool_call.pointer("/locations/0/path")).map(str::to_string))
            .or_else(|| serde_json::to_string(v).ok())
        })
        .or_else(|| non_empty_str(tool_call.pointer("/locations/0/path")).map(str::to_string))
        .unwrap_or_else(|| tool_name.clone());

    // Filter options to match Grok's safe approval policy:
    // keep allow_once, reject_once, reject_always, and only session-scoped `allow-edits-session`
    // for allow_always (other allow_always choices persist project grants).
    let raw_options: Vec<AcpPermissionOption> = params
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

    let filtered_options: Vec<AcpPermissionOption> = raw_options
        .iter()
        .filter(|opt| {
            opt.kind != "allow_always" || opt.option_id.trim() == GROK_ALLOW_EDITS_SESSION_OPTION_ID
        })
        .cloned()
        .collect();
    let options = if filtered_options.is_empty() {
        raw_options
    } else {
        filtered_options
    };

    GrokPendingRequest {
        rpc_id,
        tool_name,
        summary,
        tool_use_id,
        reason: None,
        options,
        kind: PendingPromptKind::Permission,
        custom_prompt_override: None,
    }
}

fn unwrap_xai_ext_params<'a>(params: &'a Value) -> &'a Value {
    if params.get("method").is_some() && params.get("params").is_some_and(Value::is_object) {
        params.get("params").unwrap_or(params)
    } else {
        params
    }
}

fn parse_xai_ask_user_question_request(rpc_id: Value, params: &Value) -> GrokPendingRequest {
    let unwrapped = unwrap_xai_ext_params(params);
    let tool_call_id = non_empty_str(unwrapped.get("toolCallId")).map(str::to_string);
    let raw_questions: Vec<Value> = unwrapped
        .get("questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let ghostex_questions: Vec<Value> = raw_questions
        .iter()
        .map(|q| {
            let question_text = q
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("Choose an option.");
            let multi_select = q
                .get("multiSelect")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let options: Vec<Value> = q
                .get("options")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| {
                            let label = opt.get("label").and_then(Value::as_str)?;
                            let desc = opt
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or(label);
                            Some(json!({
                                "label": label,
                                "description": desc,
                            }))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let options = if options.is_empty() {
                vec![json!({ "label": "OK", "description": "Continue" })]
            } else {
                options
            };
            json!({
                "question": question_text,
                "header": "Question",
                "multiSelect": multi_select,
                "allowCustom": true,
                "options": options,
            })
        })
        .collect();

    let first_question = raw_questions
        .first()
        .and_then(|q| q.get("question"))
        .and_then(Value::as_str)
        .unwrap_or("Question")
        .to_string();

    let prompt_override = json!({
        "kind": "question",
        "toolUseId": tool_call_id,
        "questions": ghostex_questions,
    });

    GrokPendingRequest {
        rpc_id,
        tool_name: "AskUserQuestion".to_string(),
        summary: first_question,
        tool_use_id: tool_call_id,
        reason: None,
        options: Vec::new(),
        kind: PendingPromptKind::AskUserQuestion { raw_questions },
        custom_prompt_override: Some(prompt_override),
    }
}

fn build_xai_ask_user_question_response(raw_questions: &[Value], selections: &[Value]) -> Value {
    let mut answers_map = Map::new();
    let mut annotations_map = Map::new();

    for (idx, q) in raw_questions.iter().enumerate() {
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
        let multi_select = q
            .get("multiSelect")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let options = q.get("options").and_then(Value::as_array);
        let mut selected_labels: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let mut preview: Option<String> = None;

        if let Some(indices) = sel.get("indices").and_then(Value::as_array) {
            for i in indices.iter().filter_map(Value::as_u64) {
                if let Some(opt) = options.and_then(|arr| arr.get(i as usize)) {
                    if let Some(label) = opt.get("label").and_then(Value::as_str) {
                        selected_labels.push(label.to_string());
                        if !multi_select && preview.is_none() {
                            preview = non_empty_str(opt.get("preview")).map(str::to_string);
                        }
                    }
                }
            }
        }
        if let Some(other) = non_empty_str(sel.get("other")) {
            // Check if "other" matches a preset option label.
            let matched = options.and_then(|arr| {
                arr.iter().find(|opt| {
                    opt.get("label")
                        .and_then(Value::as_str)
                        .is_some_and(|l| l == other)
                })
            });
            if let Some(opt) = matched {
                if let Some(label) = opt.get("label").and_then(Value::as_str) {
                    selected_labels.push(label.to_string());
                    if !multi_select && preview.is_none() {
                        preview = non_empty_str(opt.get("preview")).map(str::to_string);
                    }
                }
            } else {
                notes.push(other.to_string());
            }
        }

        if selected_labels.is_empty() && notes.is_empty() {
            continue;
        }
        let final_labels = if selected_labels.is_empty() {
            vec!["Other".to_string()]
        } else {
            selected_labels
        };
        answers_map.insert(q_text.to_string(), json!(final_labels));

        if preview.is_some() || !notes.is_empty() {
            let mut ann = Map::new();
            if let Some(p) = preview {
                ann.insert("preview".to_string(), json!(p));
            }
            if !notes.is_empty() {
                ann.insert("notes".to_string(), json!(notes.join("\n")));
            }
            annotations_map.insert(q_text.to_string(), Value::Object(ann));
        }
    }

    if answers_map.is_empty() {
        return json!({ "outcome": "cancelled" });
    }
    let mut resp = Map::new();
    resp.insert("outcome".to_string(), json!("accepted"));
    resp.insert("answers".to_string(), Value::Object(answers_map));
    if !annotations_map.is_empty() {
        resp.insert("annotations".to_string(), Value::Object(annotations_map));
    }
    Value::Object(resp)
}

fn parse_xai_exit_plan_mode_request(
    rpc_id: Value,
    params: &Value,
    fallback_plan: Option<String>,
) -> GrokPendingRequest {
    let unwrapped = unwrap_xai_ext_params(params);
    let tool_call_id = non_empty_str(unwrapped.get("toolCallId")).map(str::to_string);
    let _plan_content = non_empty_str(unwrapped.get("planContent"))
        .map(str::to_string)
        .or(fallback_plan);
    let options = vec![
        AcpPermissionOption {
            option_id: "approved".to_string(),
            name: "Approve plan".to_string(),
            kind: "allow_once".to_string(),
        },
        AcpPermissionOption {
            option_id: "abandoned".to_string(),
            name: "Decline".to_string(),
            kind: "reject_once".to_string(),
        },
    ];
    GrokPendingRequest {
        rpc_id,
        tool_name: "ExitPlanMode".to_string(),
        summary: "Approve plan and start implementing".to_string(),
        tool_use_id: tool_call_id,
        reason: None,
        options,
        kind: PendingPromptKind::ExitPlanMode,
        custom_prompt_override: None,
    }
}

fn respond_pending_request_cancelled(transport: &GrokTransport, req: GrokPendingRequest) {
    let payload = match req.kind {
        PendingPromptKind::Permission => json!({ "outcome": { "outcome": "cancelled" } }),
        PendingPromptKind::AskUserQuestion { .. } => json!({ "outcome": "cancelled" }),
        PendingPromptKind::ExitPlanMode => json!({
            "outcome": "abandoned",
            "feedback": "Cancelled.",
        }),
    };
    let _ = transport.respond_to_agent(req.rpc_id, payload);
}

fn cancel_all_pending_requests(transport: &GrokTransport, state: &Mutex<GrokState>) {
    let drained: Vec<GrokPendingRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        respond_pending_request_cancelled(transport, req);
    }
}

fn parse_turn_completed_usage(update: &Value) -> Option<u64> {
    let usage = update.get("usage")?;
    usage
        .get("inputTokens")
        .or_else(|| usage.get("totalTokens"))
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
}

fn extract_subagent_end_notice_from_text(text: &str) -> Option<(String, bool)> {
    let lower = text.to_ascii_lowercase();
    let pos = lower.find("background subagent ")?;
    let rest = &text[pos + "background subagent ".len()..];
    let trimmed = rest.trim_start_matches(['"', '\'', ' ']);
    let uuid: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    if !is_xai_uuid(&uuid) {
        return None;
    }
    let after_uuid = trimmed[uuid.len()..].trim_start_matches(['"', '\'', ' ']);
    // Skip balanced descriptor parentheses so nested titles cannot supply false outcome verbs.
    let tail = if after_uuid.starts_with('(') {
        let mut depth = 0usize;
        let mut cut = 0usize;
        for (i, ch) in after_uuid.char_indices() {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    cut = i + 1;
                    break;
                }
            }
        }
        after_uuid[cut..].trim_start().to_ascii_lowercase()
    } else {
        after_uuid.to_ascii_lowercase()
    };
    if tail.contains("completed successfully")
        || tail.contains("completed")
        || tail.contains("finished")
        || tail.contains("ended")
    {
        return Some((uuid, true));
    }
    if tail.contains("failed")
        || tail.contains("error")
        || tail.contains("crashed")
        || tail.contains("cancelled")
        || tail.contains("canceled")
        || tail.contains("interrupted")
        || tail.contains("timed out")
    {
        return Some((uuid, false));
    }
    None
}

fn is_wake_prompt_id(prompt_id: &str) -> bool {
    prompt_id.starts_with(XAI_TASK_COMPLETED_PROMPT_ID_PREFIX)
        || prompt_id.starts_with(XAI_SUBAGENT_COMPLETED_PROMPT_ID_PREFIX)
}

fn handle_grok_stdout_message(
    msg: &Value,
    transport: &Arc<GrokTransport>,
    state: &Arc<Mutex<GrokState>>,
    activity: &GrokActivityReporter,
) {
    transport.spool_event(msg);
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.last_event_at_ms = now_ms();
        if s.replaying_load {
            s.last_replay_activity_ms = Some(s.last_event_at_ms);
        }
    }

    let method = msg.get("method").and_then(Value::as_str);
    let has_id = msg.get("id").is_some();

    // 1. JSON-RPC response to our outgoing request.
    if method.is_none() && has_id {
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            if let Some(tx) = transport
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id)
            {
                let _ = tx.send(msg.clone());
            }
        }
        return;
    }

    // 2. Incoming reverse JSON-RPC request from Grok agent.
    if let (Some(method), true) = (method, has_id) {
        let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "session/request_permission" => {
                let mut req = parse_grok_permission_request(rpc_id, &params);
                let (level, spawned_mode, roots, session_cwd) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    (
                        s.approval_level,
                        s.spawned_launch_mode,
                        s.writable_roots.clone(),
                        s.cwd.clone(),
                    )
                };
                // Auto prompts from `--permission-mode auto` represent classifier refusals
                // and must always reach the user.
                if spawned_mode != GrokLaunchMode::Auto && level != AcpApprovalLevel::Auto {
                    match evaluate_permission(level, &params, &req.options, &roots, &session_cwd) {
                        PermissionDecision::AutoApprove { option_id } => {
                            println!(
                                "\r\n[Native] Auto-approved ({}): {} ({})",
                                level.label(),
                                req.tool_name,
                                req.summary
                            );
                            let _ = std::io::stdout().flush();
                            let _ = transport.respond_to_agent(
                                req.rpc_id,
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
                            req.reason = reason;
                        }
                    }
                }
                println!(
                    "\r\n[Native] Waiting for approval: {} ({})",
                    req.tool_name, req.summary
                );
                let _ = std::io::stdout().flush();
                let was_empty = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let empty = s.pending_requests.is_empty();
                    s.pending_requests.push_back(req);
                    empty
                };
                if was_empty {
                    activity.sync();
                }
            }
            "x.ai/ask_user_question" | "_x.ai/ask_user_question" => {
                let req = parse_xai_ask_user_question_request(rpc_id, &params);
                println!("\r\n[Native] Waiting for question answer: {}", req.summary);
                let _ = std::io::stdout().flush();
                let was_empty = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let empty = s.pending_requests.is_empty();
                    s.pending_requests.push_back(req);
                    empty
                };
                if was_empty {
                    activity.sync();
                }
            }
            "x.ai/exit_plan_mode" | "_x.ai/exit_plan_mode" => {
                let fallback = state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .last_proposed_plan_markdown
                    .clone();
                let req = parse_xai_exit_plan_mode_request(rpc_id, &params, fallback);
                println!("\r\n[Native] Waiting for plan approval...");
                let _ = std::io::stdout().flush();
                let was_empty = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let empty = s.pending_requests.is_empty();
                    s.pending_requests.push_back(req);
                    empty
                };
                if was_empty {
                    activity.sync();
                }
            }
            _ => {
                transport.respond_error_to_agent(
                    rpc_id,
                    -32601,
                    &format!("Unsupported client method: {method}"),
                );
            }
        }
        return;
    }

    // 3. Incoming JSON-RPC notifications from Grok agent.
    let Some(method) = method else {
        return;
    };
    let params = msg.get("params").unwrap_or(&Value::Null);

    match method {
        "x.ai/session/prompt_complete" | "_x.ai/session/prompt_complete" => {
            let sid = non_empty_str(params.get("sessionId")).unwrap_or_default();
            let prompt_id = non_empty_str(params.get("promptId"));
            let stop_reason = non_empty_str(params.get("stopReason")).unwrap_or("end_turn");
            let agent_result = non_empty_str(params.get("agentResult"));
            let outcome = match stop_reason {
                "rate_limit" => GrokPromptOutcome::Error {
                    code: XAI_RATE_LIMITED_ERROR_CODE,
                    message: "Grok usage limit reached. Try again later.".to_string(),
                    is_usage_limit: true,
                },
                "error" => GrokPromptOutcome::Error {
                    code: -32603,
                    message: agent_result
                        .unwrap_or("Grok ended the turn with an error.")
                        .to_string(),
                    is_usage_limit: false,
                },
                other => GrokPromptOutcome::Success {
                    stop_reason: other.to_string(),
                    usage: None,
                },
            };
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.resolve_prompt_waiter(sid, prompt_id, outcome);
            return;
        }
        "x.ai/task_backgrounded"
        | "_x.ai/task_backgrounded"
        | "x.ai/task_completed"
        | "_x.ai/task_completed" => {
            let is_completed = method.ends_with("task_completed");
            if let Some(update) = params.get("update") {
                let snapshot = update.get("task_snapshot");
                let task_id = non_empty_str(snapshot.and_then(|s| s.get("task_id")))
                    .or_else(|| non_empty_str(update.get("task_id")))
                    .or_else(|| non_empty_str(update.get("tool_call_id")));
                if let Some(tid) = task_id {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if is_completed {
                        if let Some(entry) = s.background_tasks.get_mut(tid) {
                            entry.running = false;
                        }
                    } else {
                        s.background_tasks
                            .insert(tid.to_string(), GrokBackgroundTaskEntry { running: true });
                    }
                }
            }
            return;
        }
        "x.ai/session_notification" | "_x.ai/session_notification" | "_x.ai/session/update" => {
            let sid = non_empty_str(params.get("sessionId")).unwrap_or_default();
            let Some(update) = params.get("update") else {
                return;
            };
            let kind = update
                .get("sessionUpdate")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match kind {
                "model_changed" => {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if s.native_session_id
                        .as_deref()
                        .is_none_or(|root| root == sid)
                    {
                        if let Some(m) = non_empty_str(update.get("model_id"))
                            .or_else(|| non_empty_str(update.get("modelId")))
                        {
                            s.current_model = Some(m.to_string());
                            if let Some(window) = s.model_context_window(Some(m)) {
                                s.usage_window_size = Some(window);
                            }
                        }
                        if let Some(eff) = normalize_grok_reasoning_effort(
                            update
                                .get("reasoning_effort")
                                .or_else(|| update.get("reasoningEffort"))
                                .and_then(Value::as_str),
                        ) {
                            s.current_effort = Some(eff);
                        }
                    }
                }
                "subagent_spawned" => {
                    let child_id = non_empty_str(update.get("child_session_id"))
                        .or_else(|| non_empty_str(update.get("subagent_id")));
                    if let Some(child_id) = child_id {
                        let name = non_empty_str(update.get("subagent_type"));
                        let task = non_empty_str(update.get("description"));
                        let model = non_empty_str(update.get("model"));
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.upsert_subagent(
                            child_id,
                            name,
                            task,
                            model,
                            Some(true),
                            Some(false),
                            None,
                            None,
                        );
                    }
                }
                "subagent_progress" => {
                    let child_id = non_empty_str(update.get("child_session_id"))
                        .or_else(|| non_empty_str(update.get("subagent_id")));
                    if let Some(child_id) = child_id {
                        let elapsed = update
                            .get("duration_ms")
                            .and_then(Value::as_u64)
                            .map(|ms| ms / 1000);
                        let tokens = update.get("tokens_used").and_then(Value::as_u64);
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.upsert_subagent(
                            child_id,
                            None,
                            None,
                            None,
                            Some(true),
                            Some(false),
                            elapsed,
                            tokens,
                        );
                    }
                }
                "subagent_finished" => {
                    let child_id = non_empty_str(update.get("child_session_id"))
                        .or_else(|| non_empty_str(update.get("subagent_id")));
                    if let Some(child_id) = child_id {
                        let elapsed = update
                            .get("duration_ms")
                            .and_then(Value::as_u64)
                            .map(|ms| ms / 1000);
                        let tokens = update.get("tokens_used").and_then(Value::as_u64);
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.upsert_subagent(
                            child_id,
                            None,
                            None,
                            None,
                            Some(false),
                            Some(true),
                            elapsed,
                            tokens,
                        );
                    }
                    activity.sync();
                }
                "turn_completed" => {
                    let prompt_id = non_empty_str(update.get("prompt_id"))
                        .or_else(|| non_empty_str(update.get("promptId")));
                    let stop_reason = non_empty_str(update.get("stop_reason"))
                        .or_else(|| non_empty_str(update.get("stopReason")))
                        .unwrap_or("end_turn");
                    let agent_result = non_empty_str(update.get("agent_result"));
                    let used_tokens = parse_turn_completed_usage(update);

                    let mut sync_after_wake = false;
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let is_root = s
                            .native_session_id
                            .as_deref()
                            .is_none_or(|root| root == sid);
                        if is_root {
                            if s.usage_used_tokens.is_none() {
                                if let Some(u) = used_tokens {
                                    s.usage_used_tokens = Some(u);
                                }
                            }
                            if let Some(pid) = prompt_id {
                                if is_wake_prompt_id(pid) {
                                    if s.wake_prompt_id.as_deref() == Some(pid) {
                                        s.wake_prompt_id = None;
                                        if s.pending_prompt_waiters.is_empty() {
                                            s.working = false;
                                            s.clear_live_turn_output();
                                            sync_after_wake = true;
                                        }
                                    }
                                } else {
                                    let outcome = match stop_reason {
                                        "rate_limit" => GrokPromptOutcome::Error {
                                            code: XAI_RATE_LIMITED_ERROR_CODE,
                                            message: "Grok usage limit reached. Try again later."
                                                .to_string(),
                                            is_usage_limit: true,
                                        },
                                        "error" => GrokPromptOutcome::Error {
                                            code: -32603,
                                            message: agent_result
                                                .unwrap_or("Grok ended the turn with an error.")
                                                .to_string(),
                                            is_usage_limit: false,
                                        },
                                        other => GrokPromptOutcome::Success {
                                            stop_reason: other.to_string(),
                                            usage: update.get("usage").cloned(),
                                        },
                                    };
                                    s.resolve_prompt_waiter(sid, Some(pid), outcome);
                                }
                            }
                        }
                    }
                    if sync_after_wake {
                        activity.sync();
                    }
                }
                _ => {}
            }
            return;
        }
        "session/update" => {}
        _ => return,
    }

    // Standard ACP `session/update` notification.
    let sid = non_empty_str(params.get("sessionId")).unwrap_or_default();
    let Some(update) = params.get("update") else {
        return;
    };
    let meta_prompt_id = non_empty_str(params.pointer("/_meta/promptId"));
    let meta_total_tokens = params
        .pointer("/_meta/totalTokens")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0);
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let (is_root_session, replaying) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let is_root = s
            .native_session_id
            .as_deref()
            .is_none_or(|root| root == sid);
        if is_root {
            if let Some(tokens) = meta_total_tokens {
                s.usage_used_tokens = Some(tokens);
            }
        }
        (is_root, s.replaying_load)
    };

    if !is_root_session {
        // Child subagent update: never leak child chunks into the root live stream!
        if !sid.is_empty() {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = s.subagents.get_mut(sid) {
                if !entry.done {
                    entry.working = true;
                }
            }
        }
        return;
    }

    // If a background wake continuation starts streaming on the root session while idle, mark working.
    let mut started_wake = false;
    if !replaying {
        if let Some(pid) = meta_prompt_id.filter(|p| is_wake_prompt_id(p)) {
            if matches!(
                kind,
                "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update"
            ) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if !s.working && s.pending_prompt_waiters.is_empty() {
                    s.working = true;
                    s.wake_prompt_id = Some(pid.to_string());
                    s.last_turn_interrupted = false;
                    s.clear_live_turn_output();
                    started_wake = true;
                }
            }
        }
    }
    if started_wake {
        activity.sync();
    }

    match kind {
        "config_option_update" => {
            if let Some(opts) = update.get("configOptions") {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.config_options = opts.clone();
                if let Some(arr) = opts.as_array() {
                    for opt in arr {
                        if opt.get("category").and_then(Value::as_str) == Some("thought_level") {
                            if let Some(val) = normalize_grok_reasoning_effort(
                                opt.get("currentValue").and_then(Value::as_str),
                            ) {
                                s.current_effort = Some(val);
                            }
                        }
                    }
                }
            }
        }
        "user_message_chunk" => {
            if let Some(text) = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
            {
                if let Some((child_id, _ok)) = extract_subagent_end_notice_from_text(text) {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if s.subagents.contains_key(&child_id) {
                        s.upsert_subagent(
                            &child_id,
                            None,
                            None,
                            None,
                            Some(false),
                            Some(true),
                            None,
                            None,
                        );
                    }
                }
            }
        }
        "agent_message_chunk" if !replaying => {
            if let Some(text) = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_stream_started_at.is_none() {
                    s.live_stream_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_stream_text, text, LIVE_STREAM_MAX_CHARS);
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
            }
        }
        "agent_thought_chunk" if !replaying => {
            if let Some(text) = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_thought_started_at.is_none() {
                    s.live_thought_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_thought_text, text, LIVE_STREAM_MAX_CHARS);
            }
        }
        "tool_call" | "tool_call_update" => {
            let call_id = non_empty_str(update.get("toolCallId"))
                .unwrap_or_default()
                .to_string();
            let raw_status = update
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();

            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(plan_md) = extract_grok_plan_markdown_from_update(update) {
                s.last_proposed_plan_markdown = Some(plan_md);
            }

            let existing = if !call_id.is_empty() {
                s.active_tools.get(&call_id).cloned()
            } else {
                None
            };
            let raw_input = update
                .get("rawInput")
                .filter(|v| !v.is_null())
                .cloned()
                .or_else(|| existing.as_ref().map(|e| e.raw_input.clone()))
                .unwrap_or(Value::Null);
            let raw_output = update
                .get("rawOutput")
                .filter(|v| !v.is_null())
                .cloned()
                .or_else(|| existing.as_ref().map(|e| e.raw_output.clone()))
                .unwrap_or(Value::Null);
            let raw_title = update
                .get("title")
                .or_else(|| update.get("name"))
                .or_else(|| update.get("kind"))
                .and_then(Value::as_str)
                .or_else(|| existing.as_ref().map(|e| e.title.as_str()));
            let resolved_title =
                resolve_xai_tool_title(raw_title, update.get("_meta"), &raw_input, &raw_output)
                    .unwrap_or_else(|| "Tool".to_string());
            let output_text = xai_tool_output_text(&raw_output, update.get("content"));

            let bg_task_id = extract_xai_monitor_task_id(
                &resolved_title,
                &raw_input,
                &raw_output,
                output_text.as_deref(),
            );

            // Track background shell / monitor starts.
            if let Some(ref tid) = bg_task_id {
                s.background_tasks
                    .entry(tid.clone())
                    .or_insert_with(|| GrokBackgroundTaskEntry { running: true });
            }

            // Track subagent spawn / get_output / kill from tool calls.
            if is_xai_spawn_or_task_tool(&resolved_title, &raw_input) {
                if let Some(child_id) = output_text
                    .as_deref()
                    .and_then(extract_xai_child_session_id)
                {
                    let async_ack = output_text.as_deref().is_some_and(is_xai_async_spawn_ack);
                    let working = if raw_status == "failed" {
                        false
                    } else if async_ack {
                        true
                    } else {
                        raw_status != "completed"
                    };
                    let done = !working;
                    s.upsert_subagent(
                        &child_id,
                        non_empty_str(raw_input.get("subagent_type")),
                        non_empty_str(raw_input.get("description")),
                        non_empty_str(raw_input.get("model")),
                        Some(working),
                        Some(done),
                        None,
                        None,
                    );
                }
            } else if is_xai_get_output_tool(&resolved_title, &raw_input, &raw_output) {
                if let Some(child_id) = output_text
                    .as_deref()
                    .and_then(extract_xai_child_session_id)
                {
                    let result_obj = raw_output
                        .get("Result")
                        .or_else(|| raw_output.get("result"));
                    let structured_status = non_empty_str(result_obj.and_then(|r| r.get("status")))
                        .map(|v| v.to_ascii_lowercase());
                    let is_terminal = matches!(
                        structured_status.as_deref(),
                        Some("completed" | "success" | "failed" | "error")
                    ) || result_obj
                        .and_then(|r| r.get("exit_code"))
                        .and_then(Value::as_i64)
                        .is_some();
                    if is_terminal && s.subagents.contains_key(&child_id) {
                        s.upsert_subagent(
                            &child_id,
                            None,
                            None,
                            None,
                            Some(false),
                            Some(true),
                            None,
                            None,
                        );
                    }
                }
            } else if raw_status == "completed" {
                let lower_title = resolved_title.to_ascii_lowercase();
                let variant =
                    non_empty_str(raw_input.get("variant")).map(|v| v.to_ascii_lowercase());
                let is_kill = lower_title.contains("kill_command_or_subagent")
                    || variant.as_deref() == Some("kill")
                    || variant.as_deref() == Some("killtask")
                    || non_empty_str(raw_output.get("type")) == Some("KillTask");
                if is_kill {
                    let mut ids: Vec<String> = Vec::new();
                    if let Some(arr) = raw_input.get("task_ids").and_then(Value::as_array) {
                        for entry in arr {
                            if let Some(id) = non_empty_str(Some(entry)) {
                                ids.push(id.to_string());
                            }
                        }
                    }
                    if let Some(id) = non_empty_str(raw_input.get("task_id"))
                        .or_else(|| non_empty_str(raw_input.get("taskId")))
                    {
                        ids.push(id.to_string());
                    }
                    for tid in ids {
                        if let Some(bg) = s.background_tasks.get_mut(&tid) {
                            bg.running = false;
                        }
                        if s.subagents.contains_key(&tid) {
                            s.upsert_subagent(
                                &tid,
                                None,
                                None,
                                None,
                                Some(false),
                                Some(true),
                                None,
                                None,
                            );
                        }
                    }
                }
            }

            if !call_id.is_empty() {
                s.active_tools.insert(
                    call_id,
                    GrokToolInfo {
                        title: resolved_title.clone(),
                        raw_input: raw_input.clone(),
                        raw_output: raw_output.clone(),
                    },
                );
            }

            if !replaying {
                if matches!(raw_status, "completed" | "failed" | "cancelled") {
                    s.live_tool_label = None;
                    s.live_tool_detail = None;
                    s.live_tool_started_at = None;
                } else {
                    s.live_tool_label = Some(resolved_title);
                    if s.live_tool_started_at.is_none() {
                        s.live_tool_started_at = Some(now_iso_rfc3339());
                    }
                    let detail = non_empty_str(update.pointer("/locations/0/path"))
                        .or_else(|| non_empty_str(raw_input.get("command")))
                        .or_else(|| non_empty_str(raw_input.get("file_path")))
                        .or_else(|| non_empty_str(raw_input.get("path")))
                        .or_else(|| non_empty_str(raw_input.get("url")))
                        .or_else(|| non_empty_str(raw_input.get("query")))
                        .map(str::to_string);
                    if detail.is_some() {
                        s.live_tool_detail = detail;
                    }
                }
            }
        }
        "usage_update" => {
            let used = update.get("used").and_then(Value::as_u64);
            let size = update.get("size").and_then(Value::as_u64);
            if used.is_some() || size.is_some() {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(u) = used.filter(|n| *n > 0) {
                    s.usage_used_tokens = Some(u);
                }
                if let Some(sz) = size.filter(|n| *n > 0) {
                    s.usage_window_size = Some(sz);
                }
            }
        }
        "session_info_update" => {
            if let Some(title) = non_empty_str(update.get("title")) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.session_title = Some(title.to_string());
            }
        }
        _ => {}
    }
}

fn spawn_and_initialize_grok(
    spec: &AcpAgentSpec,
    parsed: &HostArgs,
    launch_mode: GrokLaunchMode,
    resume_session_id: Option<&str>,
    cwd: &str,
    spool_session_id: &str,
    record_target: Option<&super::child_lifetime::ChildRecordTarget>,
    child_pid: &Arc<AtomicU32>,
    state: &Arc<Mutex<GrokState>>,
    activity: &GrokActivityReporter,
    intentional_stop: &Arc<AtomicBool>,
) -> Result<GrokProcess, String> {
    let (mut acp_envs, prog, _) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
            .ok_or_else(|| format!("Invalid Native host command for {}", spec.family))?;
    acp_envs.push((
        GROK_OAUTH2_REFERRER_ENV.to_string(),
        GROK_OAUTH2_REFERRER_VALUE.to_string(),
    ));
    for (k, v) in &parsed.profile.connector_envs {
        if is_allowed_connector_env_key(k) {
            acp_envs.push((k.clone(), v.clone()));
        }
    }
    if launch_mode == GrokLaunchMode::Default {
        // Prevent Grok CLI from seeding Auto permission mode from ~/.claude/settings.json
        // defaultMode when Ghostex explicitly launched Ask or Auto-edits.
        acp_envs.push(("GROK_AUTO_PERMISSION_MODE".to_string(), "0".to_string()));
        acp_envs.push((
            "GROK_DEFAULT_PERMISSION_MODE".to_string(),
            "ask".to_string(),
        ));
    }
    let argv = launch_mode.spawn_args();

    let mut child = super::child_lifetime::end_with_host(&mut Command::new(&prog))
        .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .envs(acp_envs)
        .args(&argv)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to start `{prog} {}`: {err}", argv.join(" ")))?;

    let mut child_lifetime = super::child_lifetime::ChildLifetime::attach(&child, record_target);
    child_pid.store(child.id(), Ordering::Release);

    let Some(stdin) = child.stdin.take() else {
        super::child_lifetime::force_kill_child(&mut child);
        child_pid.store(0, Ordering::Release);
        child_lifetime.cleanup();
        return Err("Failed to open stdin pipe to Grok Native host.".to_string());
    };
    let Some(stdout) = child.stdout.take() else {
        super::child_lifetime::force_kill_child(&mut child);
        child_pid.store(0, Ordering::Release);
        child_lifetime.cleanup();
        return Err("Failed to open stdout pipe from Grok Native host.".to_string());
    };
    let stderr = child.stderr.take();

    let transport = Arc::new(GrokTransport::new(stdin, spool_session_id));

    // Drain stderr into spool.
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
        let stop_out = intentional_stop.clone();
        thread::spawn(move || {
            let _guard = GrokReaderExitGuard {
                state: st_out.clone(),
                intentional_stop: stop_out,
            };
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
                    handle_grok_stdout_message(&msg, &t_out, &st_out, &act_out);
                }));
                if res.is_err() {
                    eprintln!("[Grok Native] Warning: recovered from panic handling stdout line.");
                }
            }
        });
    }

    let cleanup_on_err = |transport: &GrokTransport,
                          child: &mut Child,
                          lifetime: &mut super::child_lifetime::ChildLifetime,
                          child_pid: &AtomicU32| {
        transport.close_stdin();
        super::child_lifetime::force_kill_child(child);
        child_pid.store(0, Ordering::Release);
        lifetime.cleanup();
    };

    // 1. Initialize ACP with xAI `clientType: "extension"`.
    let init_params = json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
            "session": { "notices": {}, "compaction": {} },
        },
        "clientInfo": {
            "name": "ghostex",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "_meta": {
            "clientType": "extension",
        },
    });
    let init_res = match transport.call("initialize", init_params, GROK_INITIALIZE_TIMEOUT) {
        Ok(res) => res,
        Err(err) => {
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(format!("Grok Native initialize failed: {err}"));
        }
    };
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.protocol_version = init_res
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if let Some(model_state) = init_res.pointer("/_meta/modelState") {
            apply_grok_model_state(&mut s, model_state);
        }
    }

    // 2. Authenticate when advertised.
    let auth_methods = init_res
        .get("authMethods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !auth_methods.is_empty() {
        let preferred = resolve_grok_auth_method_id();
        let method_id = if auth_methods
            .iter()
            .any(|m| m.get("id").and_then(Value::as_str) == Some(preferred))
        {
            preferred
        } else {
            auth_methods
                .first()
                .and_then(|m| m.get("id"))
                .and_then(Value::as_str)
                .unwrap_or(preferred)
        };
        if let Err(err) = transport.call(
            "authenticate",
            json!({ "methodId": method_id }),
            GROK_RPC_TIMEOUT,
        ) {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.fatal_error = Some(err.clone());
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(format!("Grok Native authenticate failed: {err}"));
        }
    }

    // 3. Open or resume session (with replay-idle readiness for `session/load`).
    let session_result = if let Some(resume_id) = resume_session_id.filter(|s| !s.trim().is_empty())
    {
        let load_params = json!({
            "sessionId": resume_id,
            "cwd": cwd,
            "mcpServers": [],
        });
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.replaying_load = true;
            s.last_replay_activity_ms = None;
        }
        let load_res = match transport.start_call("session/load", load_params.clone()) {
            Ok((rpc_id, rx)) => {
                let deadline = Instant::now() + GROK_LOAD_TIMEOUT;
                let outcome = loop {
                    match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(resp) => {
                            if let Some(error) = resp.get("error") {
                                let msg = error
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("session/load failed");
                                break Err(msg.to_string());
                            }
                            let mut res = resp.get("result").cloned().unwrap_or_else(|| json!({}));
                            if res.get("sessionId").is_none() {
                                if let Some(obj) = res.as_object_mut() {
                                    obj.insert("sessionId".to_string(), json!(resume_id));
                                }
                            }
                            break Ok(res);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            break Err("Grok Native host exited during session/load.".to_string());
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Grok's session/load can stay unanswered after replay finishes.
                            let last_replay = state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .last_replay_activity_ms;
                            if let Some(last_ms) = last_replay {
                                if now_ms().saturating_sub(last_ms)
                                    >= GROK_LOAD_REPLAY_IDLE_GAP.as_millis() as i64
                                {
                                    transport
                                        .pending
                                        .lock()
                                        .unwrap_or_else(|p| p.into_inner())
                                        .remove(&rpc_id);
                                    break Ok(json!({ "sessionId": resume_id }));
                                }
                            }
                            if Instant::now() >= deadline {
                                transport
                                    .pending
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .remove(&rpc_id);
                                break Err("Grok Native session/load timed out.".to_string());
                            }
                        }
                    }
                };
                outcome
            }
            Err(err) => Err(err),
        };
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.replaying_load = false;
            s.last_replay_activity_ms = None;
        }
        match load_res {
            Ok(res) => Ok(res),
            Err(_) => transport
                .call("session/resume", load_params, GROK_RPC_TIMEOUT)
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
            GROK_RPC_TIMEOUT,
        )
    };

    let session_result = match session_result {
        Ok(res) => res,
        Err(err) => {
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(format!("Grok Native session open failed: {err}"));
        }
    };

    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.spawned_launch_mode = launch_mode;
        s.child_exited = false;
        s.fatal_error = None;
        apply_grok_session_metadata(&mut s, &session_result);
    }

    Ok(GrokProcess {
        child,
        child_lifetime,
        transport,
    })
}

fn stop_grok_process(
    proc: &mut GrokProcess,
    child_pid: &AtomicU32,
    intentional_stop: &AtomicBool,
    force_immediately: bool,
) {
    intentional_stop.store(true, Ordering::Release);
    proc.transport.close_stdin();
    if force_immediately {
        super::child_lifetime::force_kill_child(&mut proc.child);
        child_pid.store(0, Ordering::Release);
        proc.child_lifetime.cleanup();
        return;
    }
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                child_pid.store(0, Ordering::Release);
                proc.child_lifetime.cleanup();
                return;
            }
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    super::child_lifetime::force_kill_child(&mut proc.child);
                    child_pid.store(0, Ordering::Release);
                    proc.child_lifetime.cleanup();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                child_pid.store(0, Ordering::Release);
                proc.child_lifetime.cleanup();
                return;
            }
        }
    }
}

/// Entry point for `ghostex acp-host --agent grok`.
pub(super) fn run(spec: &AcpAgentSpec, parsed: HostArgs) -> i32 {
    let cwd_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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

    let initial_level = parsed
        .approval_level
        .or_else(|| parsed.read_only.then_some(AcpApprovalLevel::Ask))
        .unwrap_or(AcpApprovalLevel::Ask);
    let initial_launch_mode = GrokLaunchMode::from_approval_level(initial_level, parsed.read_only);

    let state = Arc::new(Mutex::new(GrokState::new(
        spec.family,
        initial_level,
        initial_launch_mode,
        parsed.read_only,
        cwd_path.clone(),
        writable_roots,
    )));
    let activity =
        GrokActivityReporter::start(endpoint.clone(), state.clone(), spec.family.to_string());
    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);
    let child_pid = Arc::new(AtomicU32::new(0));
    let shutting_down = Arc::new(AtomicBool::new(false));
    let intentional_stop = Arc::new(AtomicBool::new(false));
    let skills_cache = Mutex::new(None::<CachedGrokSkills>);
    let grok_bin = build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
        .map(|(_, prog, _)| prog)
        .unwrap_or_else(|| "grok".to_string());

    let mut proc = match spawn_and_initialize_grok(
        spec,
        &parsed,
        initial_launch_mode,
        parsed.resume_session_id.as_deref(),
        &cwd,
        &spool_session_id,
        record_target.as_ref(),
        &child_pid,
        &state,
        &activity,
        &intentional_stop,
    ) {
        Ok(proc) => proc,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };

    // Apply initial model / effort selection if requested.
    if parsed.model.is_some() || parsed.effort.is_some() {
        if let Err(err) = apply_grok_model_and_effort(
            &proc.transport,
            &state,
            parsed.model.as_deref(),
            parsed.effort.as_deref(),
        ) {
            eprintln!("[Grok Native] Warning: initial model selection failed: {err}");
        }
    }

    // Watch for SIGTERM/SIGHUP/SIGINT so the Grok subprocess is stopped promptly.
    #[cfg(unix)]
    {
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if super::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
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

    let native_session_id = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .native_session_id
        .clone();
    println!(
        "Ghostex Native Host · {} · Ready (conversation: {})",
        spec.display_name,
        native_session_id.as_deref().unwrap_or("pending")
    );
    println!("Use Chat View to talk with this agent, or switch to Terminal to open Terminal mode.");
    let _ = std::io::stdout().flush();

    activity.sync();

    let next_prompt_seq = AtomicU64::new(1);
    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
        "steer",
        "mid_turn_send",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "switch_to_cli",
    ]);
    let channel_agent = format!("acp:{}", spec.family);
    let mut reconnecting = false;

    let recycle_grok_subprocess = |proc: &mut GrokProcess,
                                   target_launch_mode: GrokLaunchMode,
                                   force_kill: bool|
     -> Result<(), String> {
        let (sid, cur_model, cur_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.native_session_id.clone(),
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        cancel_all_pending_requests(&proc.transport, &state);
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.abort_all_prompt_waiters();
            s.working = false;
            s.wake_prompt_id = None;
            s.clear_live_turn_output();
        }
        stop_grok_process(proc, &child_pid, &intentional_stop, force_kill);
        intentional_stop.store(false, Ordering::Release);
        let new_proc = spawn_and_initialize_grok(
            spec,
            &parsed,
            target_launch_mode,
            sid.as_deref(),
            &cwd,
            &spool_session_id,
            record_target.as_ref(),
            &child_pid,
            &state,
            &activity,
            &intentional_stop,
        )?;
        *proc = new_proc;
        if cur_model.is_some() || cur_effort.is_some() {
            let _ = apply_grok_model_and_effort(
                &proc.transport,
                &state,
                cur_model.as_deref(),
                cur_effort.as_deref(),
            );
        }
        Ok(())
    };

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            println!("\r\n[Native] Agent process exited.");
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
                    let (current, is_working, spawned_mode, read_only) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.approval_level,
                            s.working,
                            s.spawned_launch_mode,
                            s.read_only,
                        )
                    };
                    if desired != current {
                        let target_launch = GrokLaunchMode::from_approval_level(desired, read_only);
                        if target_launch == spawned_mode {
                            state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .approval_level = desired;
                        } else if !is_working {
                            if let Err(err) =
                                recycle_grok_subprocess(&mut proc, target_launch, false)
                            {
                                eprintln!(
                                    "[Grok Native] Warning: failed to recycle for approval level {}: {err}",
                                    desired.label()
                                );
                            } else {
                                state
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .approval_level = desired;
                            }
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
                    let (was_working, sid, spawned_mode) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let w = s.working;
                        if w {
                            s.last_turn_interrupted = true;
                        }
                        (w, s.native_session_id.clone(), s.spawned_launch_mode)
                    };
                    if let Some(sid_ref) = sid.as_deref() {
                        // Grok Ctrl+C blocks stale background wakes until the next genuine user turn.
                        let _ = proc.transport.notify(
                            "session/cancel",
                            json!({
                                "sessionId": sid_ref,
                                "_meta": { "cancelTrigger": "ctrl_c" },
                            }),
                        );
                    }
                    cancel_all_pending_requests(&proc.transport, &state);
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.abort_all_prompt_waiters();
                    }
                    if was_working {
                        let wait_until = SystemTime::now() + Duration::from_secs(2);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                        // User Stop while active restarts the Grok process group to terminate any
                        // stuck foreground tools cleanly and resumes the same conversation.
                        let still_working = state.lock().unwrap_or_else(|p| p.into_inner()).working;
                        if still_working {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.turn_generation = s.turn_generation.wrapping_add(1);
                            s.working = false;
                            s.wake_prompt_id = None;
                            s.clear_live_turn_output();
                        }
                        if let Err(err) = recycle_grok_subprocess(&mut proc, spawned_mode, true) {
                            eprintln!("[Grok Native] Warning: restart after stop failed: {err}");
                        }
                    }
                    activity.sync();
                    ep.ack(id, true, json!({ "aborted": true }), None);
                }
                "approve" => {
                    let decision = params
                        .get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("allow");
                    let explicit = params.get("optionId").and_then(Value::as_str);
                    let target_tool_use_id = non_empty_str(params.get("toolUseId"));
                    let req = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|p| {
                                    !p.is_question() && p.tool_use_id.as_deref() == Some(wanted_id)
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
                        let msg = if target_tool_use_id.is_some() {
                            "That Native approval request is no longer pending."
                        } else {
                            "No pending Native approval."
                        };
                        ep.ack(id, false, Value::Null, Some(msg));
                        continue;
                    };
                    let response_payload = match &req.kind {
                        PendingPromptKind::ExitPlanMode => {
                            let normalized = decision.trim().to_ascii_lowercase();
                            let approved = matches!(
                                normalized.as_str(),
                                "allow"
                                    | "allow_once"
                                    | "always"
                                    | "allow_always"
                                    | "approved"
                                    | "accept"
                                    | "1"
                                    | "y"
                                    | "yes"
                            ) || explicit == Some("approved");
                            if approved {
                                json!({ "outcome": "approved" })
                            } else {
                                json!({
                                    "outcome": "abandoned",
                                    "feedback": "The user did not approve the plan yet. Wait for their feedback before implementing.",
                                })
                            }
                        }
                        PendingPromptKind::Permission => {
                            let outcome = if decision.eq_ignore_ascii_case("escape")
                                || decision.eq_ignore_ascii_case("cancel")
                            {
                                json!({ "outcome": "cancelled" })
                            } else if let Some(opt_id) =
                                req.resolve_permission_option_id(explicit, decision)
                            {
                                json!({ "outcome": "selected", "optionId": opt_id })
                            } else {
                                json!({ "outcome": "cancelled" })
                            };
                            json!({ "outcome": outcome })
                        }
                        PendingPromptKind::AskUserQuestion { .. } => {
                            json!({ "outcome": "cancelled" })
                        }
                    };
                    match proc
                        .transport
                        .respond_to_agent(req.rpc_id, response_payload)
                    {
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
                    let target_tool_use_id = non_empty_str(params.get("toolUseId"));
                    let req = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|p| {
                                    p.is_question() && p.tool_use_id.as_deref() == Some(wanted_id)
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
                    let payload = match &req.kind {
                        PendingPromptKind::AskUserQuestion { raw_questions } => {
                            build_xai_ask_user_question_response(raw_questions, &selections)
                        }
                        _ => json!({ "outcome": "cancelled" }),
                    };
                    match proc.transport.respond_to_agent(req.rpc_id, payload) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let model = non_empty_str(params.get("model")).map(str::to_string);
                    let effort = non_empty_str(params.get("effort")).map(str::to_string);
                    if let Err(err) = apply_grok_model_and_effort(
                        &proc.transport,
                        &state,
                        model.as_deref(),
                        effort.as_deref(),
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
                    let (is_working, spawned_mode, read_only) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.spawned_launch_mode, s.read_only)
                    };
                    let target_launch = GrokLaunchMode::from_approval_level(level, read_only);
                    if target_launch != spawned_mode {
                        if is_working {
                            ep.ack(
                                id,
                                false,
                                Value::Null,
                                Some(
                                    "Wait for the current turn to finish before changing Grok's permission mode.",
                                ),
                            );
                            continue;
                        }
                        if let Err(err) = recycle_grok_subprocess(&mut proc, target_launch, false) {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.approval_level = level;
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "steer" => {
                    let raw_text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let text = rewrite_leading_grok_skill_mention(
                        raw_text,
                        &grok_bin,
                        &cwd_path,
                        &skills_cache,
                    );
                    let (is_working, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.native_session_id.clone())
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Grok Native session is not initialized yet."),
                        );
                        continue;
                    };
                    if is_working {
                        match proc.transport.call(
                            XAI_INTERJECT_METHOD,
                            json!({
                                "sessionId": sid,
                                "text": text,
                            }),
                            GROK_RPC_TIMEOUT,
                        ) {
                            Ok(res)
                                if res.get("status").and_then(Value::as_str) == Some("queued")
                                    || res.pointer("/result/status").and_then(Value::as_str)
                                        == Some("queued") =>
                            {
                                println!(
                                    "\r\n[Native] Delivered mid-turn steer via _x.ai/interject..."
                                );
                                let _ = std::io::stdout().flush();
                                ep.ack(id, true, json!({ "steered": true }), None);
                                continue;
                            }
                            Ok(_) => {
                                ep.ack(
                                    id,
                                    false,
                                    Value::Null,
                                    Some("Grok did not queue the interjection."),
                                );
                                continue;
                            }
                            Err(err) => {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                        }
                    }
                    // If the turn already settled, start a normal turn with the message.
                    let blocks = match build_grok_prompt_blocks(&gx_paths, &text, &[]) {
                        Ok(b) => b,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    let prompt_id = format!(
                        "gx-xai-prompt-{}",
                        next_prompt_seq.fetch_add(1, Ordering::Relaxed)
                    );
                    let (turn_gen, fallback_rx) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.wake_prompt_id = None;
                        s.last_turn_interrupted = false;
                        s.last_error = None;
                        s.clear_live_turn_output();
                        let rx = s.register_prompt_waiter(sid.clone(), prompt_id.clone());
                        (s.turn_generation, rx)
                    };
                    match proc.transport.start_call(
                        "session/prompt",
                        json!({
                            "sessionId": sid,
                            "prompt": blocks,
                            "_meta": {
                                "promptId": prompt_id,
                                "requestId": prompt_id,
                            },
                        }),
                    ) {
                        Ok((rpc_id, rpc_rx)) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "steered": true, "sessionId": sid }), None);
                            spawn_grok_prompt_watcher(
                                proc.transport.clone(),
                                state.clone(),
                                activity.clone(),
                                rpc_id,
                                rpc_rx,
                                fallback_rx,
                                prompt_id,
                                turn_gen,
                            );
                        }
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.remove_prompt_waiter(&prompt_id);
                                if s.turn_generation == turn_gen {
                                    s.working = false;
                                }
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                        }
                    }
                }
                "send" => {
                    let raw_text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let text = rewrite_leading_grok_skill_mention(
                        raw_text,
                        &grok_bin,
                        &cwd_path,
                        &skills_cache,
                    );
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

                    let (is_working, has_pending_perm, sid, spawned_mode) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            s.spawned_launch_mode,
                        )
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Grok Native session is not initialized yet."),
                        );
                        continue;
                    };
                    if !interrupt && has_pending_perm {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }

                    // Mid-turn follow-up without interrupt: steer via `_x.ai/interject`.
                    if is_working && !interrupt {
                        let image_blocks =
                            match build_grok_interject_image_blocks(&gx_paths, &images) {
                                Ok(blocks) => blocks,
                                Err(err) => {
                                    ep.ack(id, false, Value::Null, Some(&err));
                                    continue;
                                }
                            };
                        let mut interject_params = json!({
                            "sessionId": sid,
                            "text": text,
                        });
                        if !image_blocks.is_empty() {
                            interject_params["content"] = Value::Array(image_blocks);
                        }
                        match proc.transport.call(
                            XAI_INTERJECT_METHOD,
                            interject_params,
                            GROK_RPC_TIMEOUT,
                        ) {
                            Ok(res)
                                if res.get("status").and_then(Value::as_str) == Some("queued")
                                    || res.pointer("/result/status").and_then(Value::as_str)
                                        == Some("queued") =>
                            {
                                println!("\r\n[Native] Delivered mid-turn message...");
                                let _ = std::io::stdout().flush();
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
                                continue;
                            }
                            _ => {
                                // If the turn finished while we called interject, fall through to
                                // a normal `session/prompt`; otherwise report busy so Ghostex queues.
                                if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                    ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                                    continue;
                                }
                            }
                        }
                    }

                    let blocks = match build_grok_prompt_blocks(&gx_paths, &text, &images) {
                        Ok(blocks) => blocks,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    if is_working && interrupt {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.last_turn_interrupted = true;
                            s.abort_all_prompt_waiters();
                        }
                        cancel_all_pending_requests(&proc.transport, &state);
                        let _ = proc.transport.notify(
                            "session/cancel",
                            json!({
                                "sessionId": sid,
                                "_meta": { "cancelTrigger": "ctrl_c" },
                            }),
                        );
                        let wait_until = SystemTime::now() + Duration::from_secs(3);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                        if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            if let Err(err) = recycle_grok_subprocess(&mut proc, spawned_mode, true)
                            {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                        }
                    } else {
                        cancel_all_pending_requests(&proc.transport, &state);
                    }

                    let prompt_id = format!(
                        "gx-xai-prompt-{}",
                        next_prompt_seq.fetch_add(1, Ordering::Relaxed)
                    );
                    let (prev_working, prev_gen, turn_gen, fallback_rx) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let prev_w = s.working;
                        let prev_g = s.turn_generation;
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.wake_prompt_id = None;
                        s.last_turn_interrupted = false;
                        s.last_error = None;
                        s.clear_live_turn_output();
                        let rx = s.register_prompt_waiter(sid.clone(), prompt_id.clone());
                        (prev_w, prev_g, s.turn_generation, rx)
                    };

                    let (rpc_id, rpc_rx) = match proc.transport.start_call(
                        "session/prompt",
                        json!({
                            "sessionId": sid,
                            "prompt": blocks,
                            "_meta": {
                                "promptId": prompt_id,
                                "requestId": prompt_id,
                            },
                        }),
                    ) {
                        Ok(pair) => pair,
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.remove_prompt_waiter(&prompt_id);
                                if s.turn_generation == turn_gen {
                                    s.turn_generation = prev_gen;
                                    s.working = prev_working;
                                }
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    println!("\r\n[Native] Turn started...");
                    let _ = std::io::stdout().flush();
                    activity.sync();
                    ep.ack(
                        id,
                        true,
                        json!({
                            "accepted": true,
                            "midTurn": false,
                            "sessionId": sid,
                        }),
                        None,
                    );

                    spawn_grok_prompt_watcher(
                        proc.transport.clone(),
                        state.clone(),
                        activity.clone(),
                        rpc_id,
                        rpc_rx,
                        fallback_rx,
                        prompt_id,
                        turn_gen,
                    );
                }
                "switch_to_cli" => {
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let (is_working, has_pending_perm, sid, carry_model, carry_effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    if (is_working || has_pending_perm) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("The agent is currently working on a turn. Interrupt it or wait for it to finish before switching to Terminal."),
                        );
                        continue;
                    }
                    cancel_all_pending_requests(&proc.transport, &state);
                    if is_working {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.last_turn_interrupted = true;
                            s.abort_all_prompt_waiters();
                        }
                        if let Some(sid_ref) = sid.as_deref() {
                            let _ = proc.transport.notify(
                                "session/cancel",
                                json!({
                                    "sessionId": sid_ref,
                                    "_meta": { "cancelTrigger": "ctrl_c" },
                                }),
                            );
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
                    stop_grok_process(&mut proc, &child_pid, &intentional_stop, false);
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
                        Some(&format!("Unsupported Grok Native host command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, approval_level, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.native_session_id.clone(),
                s.approval_level,
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            approval_level,
            parsed.read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    stop_grok_process(&mut proc, &child_pid, &intentional_stop, false);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}

#[allow(clippy::too_many_arguments)]
fn spawn_grok_prompt_watcher(
    transport: Arc<GrokTransport>,
    state: Arc<Mutex<GrokState>>,
    activity: GrokActivityReporter,
    rpc_id: u64,
    rpc_rx: mpsc::Receiver<Value>,
    fallback_rx: mpsc::Receiver<GrokPromptOutcome>,
    prompt_id: String,
    turn_gen: u64,
) {
    thread::spawn(move || {
        // Race the JSON-RPC `session/prompt` response against xAI's completion notifications
        // (`x.ai/session/prompt_complete` or `turn_completed`), matching the xAI ACP extension.
        let deadline = Instant::now() + Duration::from_secs(60 * 60);
        let outcome = loop {
            if let Ok(fallback) = fallback_rx.try_recv() {
                transport
                    .pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&rpc_id);
                break fallback;
            }
            match rpc_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(resp) => {
                    if let Some(err) = resp.get("error") {
                        let code = err.get("code").and_then(Value::as_i64).unwrap_or(-32603);
                        let message = err
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Grok request failed")
                            .to_string();
                        break GrokPromptOutcome::Error {
                            code,
                            message,
                            is_usage_limit: code == XAI_RATE_LIMITED_ERROR_CODE,
                        };
                    }
                    let result = resp.get("result").cloned().unwrap_or(Value::Null);
                    let stop_reason = result
                        .get("stopReason")
                        .and_then(Value::as_str)
                        .unwrap_or("end_turn")
                        .to_string();
                    break GrokPromptOutcome::Success {
                        stop_reason,
                        usage: result.get("usage").cloned(),
                    };
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // Check if fallback already arrived right before disconnect.
                    if let Ok(fallback) = fallback_rx.try_recv() {
                        break fallback;
                    }
                    break GrokPromptOutcome::Error {
                        code: -32603,
                        message: "Grok Native host disconnected during turn.".to_string(),
                        is_usage_limit: false,
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        transport
                            .pending
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&rpc_id);
                        break GrokPromptOutcome::Error {
                            code: -32603,
                            message: "Grok Native turn timed out.".to_string(),
                            is_usage_limit: false,
                        };
                    }
                }
            }
        };

        let (stop_reason_label, was_cancelled, stop_error_code, last_err_pair) = match &outcome {
            GrokPromptOutcome::Success { stop_reason, .. } => {
                let cancelled = matches!(
                    stop_reason.to_ascii_lowercase().as_str(),
                    "cancelled" | "canceled" | "interrupted" | "aborted"
                );
                (stop_reason.clone(), cancelled, None, None)
            }
            GrokPromptOutcome::Error {
                code,
                message,
                is_usage_limit,
            } => {
                let err_code = if *is_usage_limit {
                    "rate_limit".to_string()
                } else {
                    format!("acp_{code}")
                };
                (
                    format!("error: {message}"),
                    false,
                    Some(err_code.clone()),
                    Some((message.clone(), Some(err_code))),
                )
            }
        };

        let (is_current_turn, orphaned_reqs) = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.remove_prompt_waiter(&prompt_id);
            s.remember_completed_prompt_id(&prompt_id);
            if s.usage_used_tokens.is_none() {
                if let GrokPromptOutcome::Success {
                    usage: Some(ref u), ..
                } = outcome
                {
                    if let Some(total) = u
                        .get("inputTokens")
                        .or_else(|| u.get("totalTokens"))
                        .and_then(Value::as_u64)
                        .filter(|n| *n > 0)
                    {
                        s.usage_used_tokens = Some(total);
                    }
                }
            }
            if s.turn_generation == turn_gen {
                s.working = false;
                if was_cancelled {
                    s.last_turn_interrupted = true;
                }
                if let Some(pair) = last_err_pair {
                    s.last_error = Some(pair);
                }
                s.clear_live_turn_output();
                let drained: Vec<GrokPendingRequest> = s.pending_requests.drain(..).collect();
                (true, drained)
            } else {
                (false, Vec::new())
            }
        };

        for req in orphaned_reqs {
            respond_pending_request_cancelled(&transport, req);
        }

        if is_current_turn {
            println!("[Native] Turn finished ({stop_reason_label}).");
            let _ = std::io::stdout().flush();
            if let Some(code) = stop_error_code {
                activity.sync_with_error(code);
            } else {
                activity.sync();
            }
        }
    });
}
