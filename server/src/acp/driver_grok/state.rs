//! Shared state, approval launch mode, pending requests, subagent/task tracking, and snapshots.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc;

use serde_json::{json, Map, Value};

use crate::acp::approval::{AcpApprovalLevel, AcpPermissionOption};
use crate::acp::host::{now_iso_rfc3339, now_ms, PendingPermission};
use crate::acp::spec::transcript_path_for_session;

use super::protocol::format_compact_tokens;
use super::{GROK_ALLOW_EDITS_SESSION_OPTION_ID, GROK_COMPLETED_PROMPT_ID_LIMIT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GrokLaunchMode {
    Default,
    Auto,
    FullAccess,
}

impl GrokLaunchMode {
    pub(super) fn from_approval_level(level: AcpApprovalLevel, read_only: bool) -> Self {
        if read_only {
            return Self::Default;
        }
        match level {
            AcpApprovalLevel::Auto => Self::Auto,
            AcpApprovalLevel::FullAccess => Self::FullAccess,
            AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits => Self::Default,
        }
    }

    pub(super) fn spawn_args(self) -> Vec<String> {
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
pub(super) enum GrokPromptOutcome {
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
pub(super) enum PendingPromptKind {
    Permission,
    AskUserQuestion { raw_questions: Vec<Value> },
    ExitPlanMode,
}

#[derive(Clone, Debug)]
pub(super) struct GrokPendingRequest {
    pub(super) rpc_id: Value,
    pub(super) tool_name: String,
    pub(super) summary: String,
    pub(super) tool_use_id: Option<String>,
    pub(super) reason: Option<String>,
    pub(super) options: Vec<AcpPermissionOption>,
    pub(super) kind: PendingPromptKind,
    pub(super) custom_prompt_override: Option<Value>,
}

impl GrokPendingRequest {
    pub(super) fn is_question(&self) -> bool {
        matches!(self.kind, PendingPromptKind::AskUserQuestion { .. })
    }

    pub(super) fn to_pending_permission(&self) -> PendingPermission {
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

    pub(super) fn to_ghostex_prompt(&self) -> Value {
        self.to_pending_permission().to_ghostex_prompt()
    }

    pub(super) fn resolve_permission_option_id(
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
pub(super) struct GrokSubagentEntry {
    pub(super) child_session_id: String,
    pub(super) name: String,
    pub(super) task: Option<String>,
    pub(super) model: Option<String>,
    pub(super) started_at_ms: i64,
    pub(super) working: bool,
    pub(super) done: bool,
    pub(super) elapsed_seconds: Option<u64>,
    pub(super) tokens: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct GrokBackgroundTaskEntry {
    pub(super) running: bool,
}

#[derive(Clone, Debug)]
pub(super) struct GrokToolInfo {
    pub(super) title: String,
    pub(super) raw_input: Value,
    pub(super) raw_output: Value,
}

pub(super) struct PendingPromptWaiter {
    pub(super) session_id: String,
    pub(super) prompt_id: String,
    pub(super) tx: mpsc::Sender<GrokPromptOutcome>,
}

pub(super) struct GrokState {
    pub(super) family: String,
    pub(super) protocol_version: u64,
    pub(super) native_session_id: Option<String>,
    pub(super) working: bool,
    pub(super) wake_prompt_id: Option<String>,
    pub(super) replaying_load: bool,
    pub(super) last_replay_activity_ms: Option<i64>,
    pub(super) last_turn_interrupted: bool,
    pub(super) turn_generation: u64,
    pub(super) approval_level: AcpApprovalLevel,
    pub(super) spawned_launch_mode: GrokLaunchMode,
    pub(super) read_only: bool,
    pub(super) cwd: PathBuf,
    pub(super) writable_roots: Vec<PathBuf>,
    pub(super) current_model: Option<String>,
    pub(super) current_effort: Option<String>,
    pub(super) available_models: Value,
    pub(super) config_options: Value,
    pub(super) pending_requests: VecDeque<GrokPendingRequest>,
    pub(super) pending_prompt_waiters: Vec<PendingPromptWaiter>,
    pub(super) completed_prompt_ids: VecDeque<String>,
    pub(super) active_tools: HashMap<String, GrokToolInfo>,
    pub(super) subagent_order: Vec<String>,
    pub(super) subagents: HashMap<String, GrokSubagentEntry>,
    pub(super) background_tasks: HashMap<String, GrokBackgroundTaskEntry>,
    pub(super) last_proposed_plan_markdown: Option<String>,
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
    pub(super) fatal_error: Option<String>,
    pub(super) last_error: Option<(String, Option<String>)>,
    pub(super) last_event_at_ms: i64,
    pub(super) child_exited: bool,
}

impl GrokState {
    pub(super) fn new(
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

    pub(super) fn clear_live_turn_output(&mut self) {
        self.live_stream_text.clear();
        self.live_stream_started_at = None;
        self.live_thought_text.clear();
        self.live_thought_started_at = None;
        self.live_tool_label = None;
        self.live_tool_detail = None;
        self.live_tool_started_at = None;
    }

    pub(super) fn remember_completed_prompt_id(&mut self, prompt_id: &str) {
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

    pub(super) fn is_completed_prompt_id(&self, prompt_id: &str) -> bool {
        self.completed_prompt_ids
            .iter()
            .any(|existing| existing == prompt_id)
    }

    pub(super) fn register_prompt_waiter(
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

    pub(super) fn remove_prompt_waiter(&mut self, prompt_id: &str) {
        self.pending_prompt_waiters
            .retain(|entry| entry.prompt_id != prompt_id);
    }

    pub(super) fn resolve_prompt_waiter(
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

    pub(super) fn abort_all_prompt_waiters(&mut self) {
        let drained: Vec<PendingPromptWaiter> = self.pending_prompt_waiters.drain(..).collect();
        for waiter in drained {
            self.remember_completed_prompt_id(&waiter.prompt_id);
            let _ = waiter.tx.send(GrokPromptOutcome::Success {
                stop_reason: "cancelled".to_string(),
                usage: None,
            });
        }
    }

    pub(super) fn upsert_subagent(
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

    pub(super) fn model_context_window(&self, model_id: Option<&str>) -> Option<u64> {
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

    pub(super) fn agent_fleet_json(&self) -> Value {
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

    pub(super) fn snapshot_json(&self) -> Value {
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
