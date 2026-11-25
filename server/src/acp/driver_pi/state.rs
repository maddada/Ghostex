//! Shared state, pending request queue, and live activity reporting for Pi Native.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::transport::HeldPrompt;
use super::{
    now_ms, transcript_path_for_session, AcpApprovalLevel, GxserverEndpoint, PendingPermission,
};

#[derive(Clone, Debug)]
pub(super) struct PiModelEntry {
    pub(super) provider: String,
    pub(super) id: String,
    pub(super) slug: String,
    pub(super) context_window: Option<u64>,
}

#[derive(Clone, Debug)]
pub(super) enum PiRequestKind {
    ToolApproval {
        tool_name: String,
    },
    ExtensionConfirm,
    Select {
        title: String,
        options: Vec<String>,
        rpiv_custom_option: Option<String>,
    },
    TextInput {
        title: String,
        placeholder: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct PendingPiRequest {
    pub(super) request_id: String,
    pub(super) kind: PiRequestKind,
    pub(super) tool_name: String,
    pub(super) summary: String,
    pub(super) tool_use_id: Option<String>,
    pub(super) reason: Option<String>,
}

impl PendingPiRequest {
    pub(super) fn is_question(&self) -> bool {
        matches!(
            self.kind,
            PiRequestKind::Select { .. } | PiRequestKind::TextInput { .. }
        )
    }

    pub(super) fn to_ghostex_prompt(&self) -> Value {
        match &self.kind {
            PiRequestKind::ToolApproval { .. } => {
                let mut prompt = json!({
                    "kind": "approval",
                    "tool": self.tool_name,
                    "summary": self.summary,
                    "toolUseId": self.tool_use_id,
                    "options": [
                        { "optionId": "allow_once", "label": "Allow", "kind": "allow_once" },
                        { "optionId": "allow_always", "label": "Always allow in session", "kind": "allow_always" },
                        { "optionId": "reject_once", "label": "Deny", "kind": "reject_once" }
                    ],
                });
                if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
                    prompt["reason"] = json!(reason);
                }
                prompt
            }
            PiRequestKind::ExtensionConfirm => {
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
            PiRequestKind::Select {
                title,
                options,
                rpiv_custom_option,
            } => {
                let formatted_opts: Vec<Value> =
                    options.iter().map(|opt| json!({ "label": opt })).collect();
                json!({
                    "kind": "question",
                    "toolUseId": self.tool_use_id,
                    "questions": [{
                        "question": if title.trim().is_empty() { "Choose an option." } else { title.as_str() },
                        "header": "Question",
                        "multiSelect": false,
                        "allowCustom": rpiv_custom_option.is_some(),
                        "options": formatted_opts,
                    }],
                })
            }
            PiRequestKind::TextInput { title, placeholder } => {
                let question = match placeholder
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    Some(ph) if !title.trim().is_empty() => format!("{} ({ph})", title.trim()),
                    Some(ph) => ph.to_string(),
                    None if !title.trim().is_empty() => title.trim().to_string(),
                    None => "Enter a response.".to_string(),
                };
                json!({
                    "kind": "question",
                    "toolUseId": self.tool_use_id,
                    "questions": [{
                        "question": question,
                        "header": "Question",
                        "multiSelect": false,
                        "allowCustom": true,
                        "options": [],
                    }],
                })
            }
        }
    }

    pub(super) fn to_pending_permission(&self) -> PendingPermission {
        let prompt = self.to_ghostex_prompt();
        PendingPermission {
            rpc_id: json!(self.request_id),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            tool_use_id: self.tool_use_id.clone(),
            reason: self.reason.clone(),
            is_question: self.is_question(),
            options: Vec::new(),
            custom_prompt_override: Some(prompt),
        }
    }
}

pub(super) struct PiState {
    pub(super) native_session_id: Option<String>,
    pub(super) session_file: Option<String>,
    pub(super) child_pid: u32,
    pub(super) working: bool,
    pub(super) is_compacting: bool,
    pub(super) auto_retrying: bool,
    pub(super) last_turn_interrupted: bool,
    pub(super) approval_level: AcpApprovalLevel,
    pub(super) read_only: bool,
    pub(super) cwd: PathBuf,
    pub(super) writable_roots: Vec<PathBuf>,
    pub(super) session_approvals: HashSet<String>,
    pub(super) current_provider: Option<String>,
    pub(super) current_model_id: Option<String>,
    pub(super) current_model_name: Option<String>,
    pub(super) current_effort: Option<String>,
    pub(super) available_models: Vec<PiModelEntry>,
    pub(super) available_models_json: Value,
    pub(super) pending_requests: VecDeque<PendingPiRequest>,
    pub(super) pending_rpiv_custom_input: Option<(String, Instant)>,
    pub(super) held_prompts: VecDeque<HeldPrompt>,
    pub(super) live_stream_text: String,
    pub(super) live_stream_started_at: Option<String>,
    pub(super) live_thought_text: String,
    pub(super) live_thought_started_at: Option<String>,
    pub(super) live_tool_label: Option<String>,
    pub(super) live_tool_detail: Option<String>,
    pub(super) live_tool_started_at: Option<String>,
    pub(super) compacting_started_at: Option<String>,
    pub(super) usage_used_tokens: Option<u64>,
    pub(super) usage_window_size: Option<u64>,
    pub(super) session_title: Option<String>,
    pub(super) last_error: Option<(String, Option<String>)>,
    pub(super) last_event_at_ms: i64,
    pub(super) child_exited: bool,
}

impl PiState {
    pub(super) fn new(
        approval_level: AcpApprovalLevel,
        read_only: bool,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
        child_pid: u32,
    ) -> Self {
        Self {
            native_session_id: None,
            session_file: None,
            child_pid,
            working: false,
            is_compacting: false,
            auto_retrying: false,
            last_turn_interrupted: false,
            approval_level,
            read_only,
            cwd,
            writable_roots,
            session_approvals: HashSet::new(),
            current_provider: None,
            current_model_id: None,
            current_model_name: None,
            current_effort: None,
            available_models: Vec::new(),
            available_models_json: Value::Array(Vec::new()),
            pending_requests: VecDeque::new(),
            pending_rpiv_custom_input: None,
            held_prompts: VecDeque::new(),
            live_stream_text: String::new(),
            live_stream_started_at: None,
            live_thought_text: String::new(),
            live_thought_started_at: None,
            live_tool_label: None,
            live_tool_detail: None,
            live_tool_started_at: None,
            compacting_started_at: None,
            usage_used_tokens: None,
            usage_window_size: None,
            session_title: None,
            last_error: None,
            last_event_at_ms: now_ms(),
            child_exited: false,
        }
    }

    pub(super) fn current_model_slug(&self) -> Option<String> {
        let id = self
            .current_model_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        if let Some(provider) = self
            .current_provider
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(format!("{provider}/{id}"))
        } else {
            Some(id.to_string())
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

    pub(super) fn live_activity_json(&self) -> Value {
        if self.is_compacting {
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

    pub(super) fn apply_get_state(&mut self, data: &Value) {
        if let Some(sid) = data
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.native_session_id = Some(sid.to_string());
        }
        if let Some(file) = data
            .get("sessionFile")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.session_file = Some(file.to_string());
        }
        if let Some(name) = data
            .get("sessionName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.session_title = Some(name.to_string());
        }
        if let Some(model) = data.get("model").and_then(Value::as_object) {
            self.apply_model_object(model);
        }
        if let Some(level) = data
            .get("thinkingLevel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.current_effort = Some(level.to_ascii_lowercase());
        }
    }

    pub(super) fn apply_model_object(&mut self, model: &Map<String, Value>) {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let provider = model
            .get("provider")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        // Pi reports `{"id":"unknown","provider":"unknown","name":"unknown"}` when no provider
        // is authenticated yet; ignore placeholder values so Ghostex never pins `unknown/unknown`
        // as the default Pi model (which breaks `pi --model unknown/unknown`) or renders `Unknown`.
        let is_placeholder = |val: &str| {
            val.eq_ignore_ascii_case("unknown")
                || ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(val)
        };
        if id.is_some_and(is_placeholder) {
            self.current_provider = None;
            self.current_model_id = None;
            self.current_model_name = None;
            return;
        }
        if let Some(provider) = provider.filter(|p| !is_placeholder(p)) {
            self.current_provider = Some(provider.to_string());
        }
        if let Some(id) = id {
            self.current_model_id = Some(id.to_string());
        }
        if let Some(name) = model
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && !is_placeholder(s))
        {
            self.current_model_name = Some(name.to_string());
        }
        if let Some(ctx) = model
            .get("contextWindow")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
        {
            self.usage_window_size = Some(ctx);
        }
    }

    pub(super) fn apply_available_models(&mut self, data: &Value) {
        let Some(models) = data.get("models").and_then(Value::as_array) else {
            return;
        };
        let mut parsed = Vec::with_capacity(models.len());
        let mut json_rows = Vec::with_capacity(models.len());
        for entry in models {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            let Some(id) = obj
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let provider = obj
                .get("provider")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            let slug = if provider.is_empty() {
                id.to_string()
            } else {
                format!("{provider}/{id}")
            };
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .to_string();
            let context_window = obj
                .get("contextWindow")
                .and_then(Value::as_u64)
                .filter(|v| *v > 0);
            let reasoning = obj
                .get("reasoning")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            json_rows.push(json!({
                "modelId": slug,
                "id": slug,
                "provider": provider,
                "rawId": id,
                "name": name,
                "contextWindow": context_window,
                "reasoning": reasoning,
            }));
            parsed.push(PiModelEntry {
                provider: provider.to_string(),
                id: id.to_string(),
                slug,
                context_window,
            });
        }
        // Populate contextWindow for the current model if we didn't get it from get_state.
        if self.usage_window_size.is_none() {
            if let Some(slug) = self.current_model_slug() {
                if let Some(found) = parsed.iter().find(|m| m.slug == slug) {
                    self.usage_window_size = found.context_window;
                }
            }
        }
        self.available_models = parsed;
        self.available_models_json = Value::Array(json_rows);
    }

    pub(super) fn resolve_model_target(&self, raw: &str) -> Option<(String, String)> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        // 1. Exact `provider/id` match against reported models (handles providers or ids with `/`).
        if let Some(found) = self
            .available_models
            .iter()
            .find(|m| m.slug.eq_ignore_ascii_case(trimmed))
        {
            return Some((found.provider.clone(), found.id.clone()));
        }
        // 2. Exact `id` match against reported models (prefer current provider if ambiguous).
        if let Some(cur_provider) = self.current_provider.as_deref() {
            if let Some(found) = self
                .available_models
                .iter()
                .find(|m| m.provider == cur_provider && m.id.eq_ignore_ascii_case(trimmed))
            {
                return Some((found.provider.clone(), found.id.clone()));
            }
        }
        if let Some(found) = self
            .available_models
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(trimmed))
        {
            return Some((found.provider.clone(), found.id.clone()));
        }
        // 3. Fallback: split on first `/`.
        if let Some((provider, model_id)) = trimmed.split_once('/') {
            let p = provider.trim();
            let id = model_id.trim();
            if !p.is_empty() && !id.is_empty() {
                return Some((p.to_string(), id.to_string()));
            }
        }
        // 4. Fallback: use current provider with bare id.
        if let Some(cur_provider) = self
            .current_provider
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some((cur_provider.to_string(), trimmed.to_string()));
        }
        None
    }

    pub(super) fn effective_transcript_path(&self) -> Option<String> {
        self.session_file.clone().or_else(|| {
            self.native_session_id
                .as_deref()
                .and_then(|id| transcript_path_for_session("pi", id))
                .map(|p| p.to_string_lossy().to_string())
        })
    }

    pub(super) fn snapshot_json(&self) -> Value {
        let last_error = self.last_error.as_ref().map(|(message, code)| {
            json!({
                "message": message,
                "code": code,
            })
        });
        json!({
            "family": "pi",
            "nativeSessionId": self.native_session_id,
            "working": self.working || self.is_compacting,
            "model": self.current_model_slug(),
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": Value::Null,
            "nativeAutoActive": false,
            "nativeAutoAvailable": false,
            "availableModels": self.available_models_json,
            "availableModes": [],
            "configOptions": [],
            "pendingPermission": self.pending_requests.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "sessionTitle": self.session_title,
            "lastError": last_error,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": self.effective_transcript_path(),
            "childExited": self.child_exited,
        })
    }
}

#[derive(Clone)]
pub(super) struct PiActivityReporter {
    pub(super) tx: Option<mpsc::Sender<()>>,
}

impl PiActivityReporter {
    pub(super) fn start(endpoint: Option<GxserverEndpoint>, state: Arc<Mutex<PiState>>) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                let read_state = || {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s
                        .pending_requests
                        .front()
                        .map(|p| p.to_pending_permission());
                    let act = if front.is_some() {
                        "attention"
                    } else if s.working || s.is_compacting {
                        "working"
                    } else {
                        "idle"
                    };
                    let stop_err = (!s.working && !s.is_compacting && front.is_none())
                        .then(|| s.last_error.as_ref().map(|(msg, _)| msg.clone()))
                        .flatten();
                    (
                        act,
                        s.native_session_id.clone(),
                        s.effective_transcript_path(),
                        front,
                        s.last_turn_interrupted,
                        stop_err,
                    )
                };
                let (act, sid, path, perm, interrupted, stop_err) = read_state();
                let mut ok = ep.report_activity_with_error(
                    "pi",
                    act,
                    sid.as_deref(),
                    path.as_deref(),
                    perm.as_ref(),
                    interrupted,
                    stop_err.as_deref(),
                );
                for delay_ms in [500u64, 1_000, 2_000] {
                    if ok {
                        break;
                    }
                    match rx.recv_timeout(Duration::from_millis(delay_ms)) {
                        Ok(()) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let (act, sid, path, perm, interrupted, stop_err) = read_state();
                            ok = ep.report_activity_with_error(
                                "pi",
                                act,
                                sid.as_deref(),
                                path.as_deref(),
                                perm.as_ref(),
                                interrupted,
                                stop_err.as_deref(),
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

pub(super) struct PiReaderExitGuard {
    pub(super) state: Arc<Mutex<PiState>>,
    pub(super) activity: PiActivityReporter,
}

impl Drop for PiReaderExitGuard {
    fn drop(&mut self) {
        {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            s.child_exited = true;
            s.working = false;
            s.is_compacting = false;
        }
        self.activity.sync();
    }
}
