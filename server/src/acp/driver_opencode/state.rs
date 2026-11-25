//! Shared host state, activity reporting, model/attachment/skill parsing, and status snapshots.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::super::approval::AcpApprovalLevel;
use super::super::host::{now_ms, GxserverEndpoint};
use super::approvals::PendingOpenCodeRequest;
use super::mirror::{
    build_fleet, extract_tasks_from_messages, mirror_path, serialize_mirror_jsonl,
    write_atomic_mirror_bytes, LiveParts,
};

fn extract_image_references_from_text(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[Image #") {
        rest = &rest[start + 8..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        let label = rest[digits..].strip_prefix('·').unwrap_or(&rest[digits..]);
        let Some(destination) = label.strip_prefix("](") else {
            continue;
        };
        let mut depth = 1usize;
        let mut end = None;
        for (index, ch) in destination.char_indices() {
            match ch {
                '\n' | '\r' => break,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { continue };
        let path = destination[..end].trim();
        let path = path
            .strip_prefix('<')
            .and_then(|p| p.strip_suffix('>'))
            .unwrap_or(path);
        if !path.is_empty() && !paths.iter().any(|existing| existing == path) {
            paths.push(path.to_string());
        }
        rest = &destination[end + 1..];
    }
    paths
}

pub(super) fn build_opencode_file_attachments(
    gx_paths: &crate::paths::GxserverPaths,
    text: &str,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut combined = image_paths.to_vec();
    for referenced in extract_image_references_from_text(text) {
        if !combined.iter().any(|p| p == &referenced) {
            combined.push(referenced);
        }
    }
    let mut files = Vec::new();
    for raw in &combined {
        let (bytes, raw_mime) = super::super::read_validated_acp_image(gx_paths, raw)?;
        let (bytes, encoding) = if raw_mime == "image/bmp" || raw_mime == "image/avif" {
            let img = image::load_from_memory(&bytes)
                .map_err(|_| "The attached image could not be decoded for OpenCode.".to_string())?;
            let mut png = Vec::new();
            img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|_| {
                    "The attached image could not be converted for OpenCode.".to_string()
                })?;
            crate::session_chat_opencode::opencode_image_bytes(png).map_err(|e| e.message)?
        } else {
            crate::session_chat_opencode::opencode_image_bytes(bytes).map_err(|e| e.message)?
        };
        let mime = match encoding {
            "png" => "image/png",
            "jpg" => "image/jpeg",
            _ => raw_mime,
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        let name = Path::new(raw)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image");
        files.push(json!({
            "uri": format!("data:{mime};base64,{b64}"),
            "name": name,
        }));
    }
    Ok(files)
}

pub(super) fn extract_skill_references(text: &str, known_skills: &HashSet<String>) -> Vec<Value> {
    if known_skills.is_empty() {
        return Vec::new();
    }
    let mut matched = Vec::new();
    let mut seen = HashSet::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let boundary = i == 0 || {
                let prev = bytes[i - 1];
                !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.')
            };
            if boundary {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() {
                    let b = bytes[end];
                    if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':' | b'/') {
                        end += 1;
                    } else {
                        break;
                    }
                }
                let candidate = &text[start..end];
                let candidate = candidate.trim_end_matches(['.', ',', ':', ';']);
                if !candidate.is_empty()
                    && known_skills.contains(candidate)
                    && seen.insert(candidate.to_string())
                {
                    matched.push(json!({ "id": candidate }));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    matched
}

pub(super) fn parse_model_reference(model_value: &str) -> Option<(String, String)> {
    let (provider, id) = model_value.trim().split_once('/')?;
    let provider = provider.trim();
    let id = id.trim();
    if provider.is_empty() || id.is_empty() {
        return None;
    }
    Some((provider.to_string(), id.to_string()))
}

pub(super) fn extract_model_from_info_or_messages(
    info: &Value,
    messages: &[Value],
) -> (Option<String>, Option<String>) {
    let parse_obj = |m: &Value| -> Option<(String, Option<String>)> {
        let provider = m.get("providerID").and_then(Value::as_str)?.trim();
        let id = m
            .get("id")
            .or_else(|| m.get("modelID"))
            .and_then(Value::as_str)?
            .trim();
        if provider.is_empty() || id.is_empty() {
            return None;
        }
        let variant = m
            .get("variant")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("default"))
            .map(str::to_string);
        Some((format!("{provider}/{id}"), variant))
    };
    if let Some(found) = parse_obj(&info["model"]) {
        return (Some(found.0), found.1);
    }
    for msg in messages.iter().rev() {
        if msg["type"] == "assistant" {
            if let Some(found) = parse_obj(&msg["model"]) {
                return (Some(found.0), found.1);
            }
        }
    }
    (None, None)
}

pub(super) struct OpenCodeState {
    pub(super) session_id: Option<String>,
    pub(super) session_info: Value,
    pub(super) messages: Vec<Value>,
    pub(super) live_parts: LiveParts,
    pub(super) last_mirror_write: Instant,
    pub(super) working: bool,
    pub(super) compacting: bool,
    pub(super) compacting_started_at: Option<String>,
    pub(super) last_turn_interrupted: bool,
    pub(super) last_turn_stop_error: Option<String>,
    pub(super) last_auth_error: Option<String>,
    pub(super) last_error: Option<(Option<String>, String)>,
    pub(super) fatal_error: Option<String>,
    pub(super) external_form_url: Option<String>,
    pub(super) approval_level: AcpApprovalLevel,
    /// Keep default with no Mode-menu pick yet: OpenCode's own `permission` config decides what
    /// asks, so Ghostex adds no rules of its own (CDXC:NativeApprovals 2026-11-25 in
    /// `acp/cli_default.rs`).
    pub(super) cli_default: bool,
    pub(super) read_only: bool,
    pub(super) current_agent: String,
    pub(super) plan_agent_paths: Vec<String>,
    pub(super) session_grants: Vec<(String, String)>,
    pub(super) cwd: PathBuf,
    pub(super) writable_roots: Vec<PathBuf>,
    pub(super) current_model: Option<String>,
    pub(super) current_effort: Option<String>,
    pub(super) available_models: Value,
    pub(super) model_catalog: Option<Value>,
    pub(super) model_variants: HashMap<String, Vec<String>>,
    pub(super) model_context_windows: HashMap<String, u64>,
    pub(super) known_commands: HashSet<String>,
    pub(super) known_skills: HashSet<String>,
    pub(super) pending_requests: VecDeque<PendingOpenCodeRequest>,
    pub(super) child_sessions: HashMap<String, Value>,
    pub(super) active_sessions: HashSet<String>,
    pub(super) running_shells: HashSet<String>,
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
    pub(super) developer_instructions: Option<String>,
}

impl OpenCodeState {
    /// The level Ghostex's session rules enforce, `None` while OpenCode's own config decides.
    pub(super) fn enforced_level(&self) -> Option<AcpApprovalLevel> {
        (!self.cli_default).then_some(self.approval_level)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        approval_level: AcpApprovalLevel,
        read_only: bool,
        developer_instructions: Option<String>,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
        initial_model: Option<String>,
        initial_effort: Option<String>,
    ) -> Self {
        Self {
            session_id: None,
            session_info: Value::Null,
            messages: Vec::new(),
            live_parts: LiveParts::default(),
            last_mirror_write: Instant::now() - Duration::from_secs(10),
            working: false,
            compacting: false,
            compacting_started_at: None,
            last_turn_interrupted: false,
            last_turn_stop_error: None,
            last_auth_error: None,
            last_error: None,
            fatal_error: None,
            external_form_url: None,
            approval_level,
            cli_default: false,
            read_only,
            current_agent: if read_only { "plan" } else { "build" }.to_string(),
            plan_agent_paths: Vec::new(),
            session_grants: Vec::new(),
            cwd,
            writable_roots,
            current_model: initial_model,
            current_effort: initial_effort
                .filter(|e| !ghostex_gx_chat_core::menus::catalog::is_placeholder_effort_value(e)),
            available_models: Value::Array(Vec::new()),
            model_catalog: None,
            model_variants: HashMap::new(),
            model_context_windows: HashMap::new(),
            known_commands: HashSet::new(),
            known_skills: HashSet::new(),
            pending_requests: VecDeque::new(),
            child_sessions: HashMap::new(),
            active_sessions: HashSet::new(),
            running_shells: HashSet::new(),
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
            developer_instructions,
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

    pub(super) fn transcript_path_string(&self) -> Option<String> {
        self.session_id
            .as_deref()
            .filter(|id| crate::session_chat_opencode::safe_id(id))
            .map(|id| mirror_path(id).to_string_lossy().to_string())
    }

    pub(super) fn write_mirror_now(&mut self) {
        let Some(sid) = self
            .session_id
            .as_deref()
            .filter(|id| crate::session_chat_opencode::safe_id(id))
        else {
            return;
        };
        let bytes = serialize_mirror_jsonl(
            &self.messages,
            &self.session_info,
            self.working,
            self.last_turn_interrupted,
            &self.live_parts,
        );
        let path = mirror_path(sid);
        let _ = write_atomic_mirror_bytes(&path, &bytes);
        self.last_mirror_write = Instant::now();
    }

    pub(super) fn validated_variant(&self, model: &str, requested: Option<&str>) -> Option<String> {
        let variant = requested
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("default"))?;
        match self.model_variants.get(model) {
            Some(allowed) if !allowed.is_empty() => allowed
                .iter()
                .find(|v| v.eq_ignore_ascii_case(variant))
                .cloned(),
            Some(_) => None,
            // Catalog not loaded yet: pass through as requested.
            None => Some(variant.to_string()),
        }
    }

    pub(super) fn update_window_for_current_model(&mut self) {
        if let Some(model) = self.current_model.as_deref() {
            if let Some(&win) = self.model_context_windows.get(model) {
                if win > 0 {
                    self.usage_window_size = Some(win);
                }
            }
        }
    }

    pub(super) fn live_activity_json(&self) -> Value {
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

    pub(super) fn snapshot_json(&self) -> Value {
        let last_error = self.last_error.as_ref().map(|(code, msg)| {
            json!({
                "code": code,
                "message": msg,
            })
        });
        let tasks = extract_tasks_from_messages(&self.messages)
            .map(|t| t.to_value())
            .unwrap_or(Value::Null);
        let fleet = build_fleet(&self.child_sessions, &self.active_sessions, &self.messages)
            .map(|f| f.to_value())
            .unwrap_or(Value::Null);
        json!({
            "family": "opencode",
            "nativeSessionId": self.session_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": self.current_agent,
            "nativeAutoActive": false,
            "nativeAutoAvailable": false,
            "availableModels": self.available_models,
            "modelCatalog": self.model_catalog,
            "availableModes": [],
            "configOptions": [],
            "pendingPermission": self.pending_requests.front().map(|p| p.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_requests.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "sessionTitle": self.session_title,
            "tasks": tasks,
            "fleet": fleet,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": self.transcript_path_string(),
            "childPid": self.child_pid,
            "childExited": self.child_exited,
            "authError": self.last_auth_error,
            "fatalError": self.fatal_error,
            "lastError": last_error,
            "externalFormUrl": self.external_form_url,
        })
    }
}

#[derive(Clone)]
pub(super) struct OpenCodeActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl OpenCodeActivityReporter {
    pub(super) fn start(
        endpoint: Option<GxserverEndpoint>,
        state: Arc<Mutex<OpenCodeState>>,
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
                        s.session_id.clone(),
                        s.transcript_path_string(),
                        front,
                        s.last_turn_interrupted,
                        s.last_turn_stop_error.clone(),
                    )
                };
                let (act, sid, tpath, perm, interrupted, stop_error) = snapshot();
                let mut ok = ep.report_activity_with_error(
                    "opencode",
                    act,
                    sid.as_deref(),
                    tpath.as_deref(),
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
                            let (act, sid, tpath, perm, interrupted, stop_error) = snapshot();
                            ok = ep.report_activity_with_error(
                                "opencode",
                                act,
                                sid.as_deref(),
                                tpath.as_deref(),
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
