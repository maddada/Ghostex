//! Native OpenCode 2 server driver for `ghostex acp-host --agent opencode`.
//!
//! Spawns a local `opencode serve --hostname=127.0.0.1 --port=<free>` process with a
//! per-host random `OPENCODE_PASSWORD` (OpenCode 2 only), subscribes to its `/api/event`
//! SSE stream, maps Ghostex's four live approval levels (`Ask`, `Auto-edits`, `Auto`,
//! `Full access`) and `--read-only` to OpenCode 2 session permission rules (keeping
//! session-scoped grants in memory and never writing persistent `"always"` approvals),
//! handles permissions, forms/questions, model + variant (effort) catalog and selection,
//! mid-turn steering, slash commands, skills, `!shell` commands, image attachments,
//! subagent fleet + child transcript mirrors, `todowrite` tasks, conversation rewind,
//! `gxserver` reload survival inside `zmx`, and in-place Switch to Terminal
//! (`opencode --session <id>`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
use super::spec::{is_allowed_connector_env_key, validated_ghostex_prompt_file, AcpAgentSpec};
use crate::session_chat::SessionChatRole;
use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_agent_tasks::{SessionChatAgentTask, SessionChatAgentTasks};

const SERVER_START_TIMEOUT: Duration = Duration::from_secs(20);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const HTTP_LONG_TIMEOUT: Duration = Duration::from_secs(60);
const SHELL_TIMEOUT: Duration = Duration::from_secs(3600);
const SSE_SILENT_TIMEOUT: Duration = Duration::from_secs(45);
const MIRROR_DELTA_DEBOUNCE: Duration = Duration::from_millis(120);

const DECLINED_PERMISSION_MESSAGE: &str = "The user declined this request.";
const DECLINED_SYNTHETIC_NOTE: &str =
    "The user declined this request. Do not retry it unless asked.";
const DISMISSED_FORM_MESSAGE: &str = "The user dismissed this prompt.";

/// Path to the Native OpenCode transcript mirror for `session_id`.
pub(crate) fn mirror_path(session_id: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("acp-opencode-mirror")
        .join(format!("{session_id}.jsonl"))
}

/// Path to a mirrored child subagent transcript and its metadata file.
pub(crate) fn subagent_mirror_path(child_id: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("acp-opencode-mirror")
        .join("subagents")
        .join(format!("{child_id}.jsonl"))
}

fn subagent_meta_path(child_id: &str) -> PathBuf {
    subagent_mirror_path(child_id).with_extension("meta.json")
}

/// True when `path` is written by this Native driver (`acp-opencode-mirror/`), so
/// `session_chat_opencode::refresh_for_path` does not try to contact the shared
/// `opencode service` for it.
pub(crate) fn is_mirror_path(path: &Path) -> bool {
    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());
    if parent_name == Some("acp-opencode-mirror") {
        return true;
    }
    if parent_name == Some("subagents") {
        return path
            .parent()
            .and_then(Path::parent)
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("acp-opencode-mirror");
    }
    false
}

/// Resolve a Native OpenCode child subagent mirror owned by `root_id`.
pub(crate) fn resolve_subagent_mirror(
    root_id: &str,
    selector: &str,
) -> Option<(PathBuf, String, Option<String>)> {
    if !crate::session_chat_opencode::safe_id(selector) {
        return None;
    }
    let path = subagent_mirror_path(selector);
    let meta_path = subagent_meta_path(selector);
    let meta: Value = fs::read_to_string(&meta_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())?;
    if meta.get("rootId").and_then(Value::as_str) != Some(root_id) {
        return None;
    }
    if !path.is_file() {
        return None;
    }
    let name = meta
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(selector)
        .to_string();
    let agent_type = meta
        .get("agent")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    Some((path, name, agent_type))
}

fn reserve_ephemeral_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("Could not bind local port for OpenCode server: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("Could not read bound port for OpenCode server: {err}"))?
        .port();
    drop(listener);
    Ok(port)
}

fn generate_server_password() -> String {
    let a = uuid::Uuid::new_v4().simple().to_string();
    let b = uuid::Uuid::new_v4().simple().to_string();
    format!("{a}{b}")
}

fn resolve_opencode_binary() -> String {
    let home = crate::resume_lookup::home_dir();
    if let Some(program) = crate::agent_cli::opencode::opencode2_program(&home) {
        return program.to_string_lossy().to_string();
    }
    "opencode".to_string()
}

fn encode_query_component(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[derive(Clone)]
struct OpenCodeHttp {
    base_url: String,
    authorization: String,
}

impl OpenCodeHttp {
    fn new(base_url: String, password: &str) -> Self {
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("opencode:{password}"));
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            authorization: format!("Basic {auth}"),
        }
    }

    fn build_agent(timeout: Duration) -> ureq::Agent {
        crate::http_agent::config()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .max_redirects(0)
            .build()
            .into()
    }

    fn request_raw(
        &self,
        method: &str,
        path: &str,
        directory: Option<&str>,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<(u16, Value), String> {
        let agent = Self::build_agent(timeout);
        let url = format!("{}{path}", self.base_url);
        let mut builder = ureq::http::Request::builder()
            .method(method)
            .uri(&url)
            .header("Authorization", &self.authorization);
        if let Some(dir) = directory.filter(|s| !s.trim().is_empty()) {
            builder = builder.header("x-opencode-directory", dir);
        }
        let response = match body {
            Some(payload) => {
                let req = builder
                    .header("Content-Type", "application/json")
                    .body(payload.to_string())
                    .map_err(|err| format!("Invalid OpenCode HTTP request: {err}"))?;
                agent.run(req)
            }
            None => {
                let req = builder
                    .body(())
                    .map_err(|err| format!("Invalid OpenCode HTTP request: {err}"))?;
                agent.run(req)
            }
        }
        .map_err(|err| format!("OpenCode request {method} {path} failed: {err}"))?;

        let status = response.status().as_u16();
        if status == 204 {
            return Ok((status, Value::Null));
        }
        let text = response.into_body().read_to_string().unwrap_or_default();
        if text.trim().is_empty() {
            return Ok((status, Value::Null));
        }
        let parsed = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| Value::String(text));
        Ok((status, parsed))
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        let (status, val) = self.request_raw(method, path, None, body, timeout)?;
        if !(200..300).contains(&status) {
            let (_, msg) = readable_provider_error_message(&val);
            return Err(if msg.is_empty() {
                format!("OpenCode {method} {path} failed (HTTP {status}).")
            } else {
                msg
            });
        }
        Ok(val)
    }

    fn request_in(
        &self,
        method: &str,
        path: &str,
        directory: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        let (status, val) = self.request_raw(method, path, Some(directory), body, timeout)?;
        if !(200..300).contains(&status) {
            let (_, msg) = readable_provider_error_message(&val);
            return Err(if msg.is_empty() {
                format!("OpenCode {method} {path} failed (HTTP {status}).")
            } else {
                msg
            });
        }
        Ok(val)
    }

    fn session(
        &self,
        session_id: &str,
        suffix: &str,
        method: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        if !crate::session_chat_opencode::safe_id(session_id) {
            return Err("Invalid OpenCode session ID.".to_string());
        }
        self.request(
            method,
            &format!("/api/session/{session_id}{suffix}"),
            body,
            timeout,
        )
    }

    fn fetch_messages(&self, session_id: &str) -> Result<Vec<Value>, String> {
        let mut messages = Vec::new();
        let mut query = "?limit=200&order=asc".to_string();
        let mut cursors = HashSet::new();
        loop {
            let page = self.session(
                session_id,
                &format!("/message{query}"),
                "GET",
                None,
                HTTP_TIMEOUT,
            )?;
            let rows = page
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| "OpenCode returned an invalid message page.".to_string())?;
            messages.extend(rows.iter().cloned());
            let Some(next) = page.pointer("/cursor/next").and_then(Value::as_str) else {
                break;
            };
            if !cursors.insert(next.to_string()) {
                return Err("OpenCode repeated a message pagination cursor.".to_string());
            }
            query = format!("?limit=200&cursor={}", encode_query_component(next));
        }
        Ok(messages)
    }

    fn open_event_stream(&self) -> Result<ureq::http::Response<ureq::Body>, String> {
        let agent: ureq::Agent = crate::http_agent::config()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_recv_body(Some(SSE_SILENT_TIMEOUT))
            .max_redirects(0)
            .build()
            .into();
        agent
            .get(&format!("{}/api/event", self.base_url))
            .header("Authorization", &self.authorization)
            .header("Accept", "text/event-stream")
            .call()
            .map_err(|err| format!("Could not connect to OpenCode event stream: {err}"))
    }
}

/// Extract a clean `(code, message)` pair from OpenCode error payloads (including JSON-encoded
/// upstream provider error strings).
fn readable_provider_error_message(error: &Value) -> (Option<String>, String) {
    if error.is_null() {
        return (None, String::new());
    }
    if let Some(raw) = error.as_str() {
        let trimmed = raw.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                return readable_provider_error_message(&parsed);
            }
        }
        return (None, collapse_whitespace(trimmed));
    }
    let code = error
        .pointer("/error/type")
        .or_else(|| error.pointer("/error/code"))
        .or_else(|| error.get("type"))
        .or_else(|| error.get("code"))
        .or_else(|| error.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let raw_msg = error
        .pointer("/error/data/message")
        .or_else(|| error.pointer("/error/message"))
        .or_else(|| error.pointer("/data/message"))
        .or_else(|| error.get("message"))
        .or_else(|| error.get("error"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(msg) = raw_msg {
        if msg.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<Value>(msg) {
                let (inner_code, inner_msg) = readable_provider_error_message(&parsed);
                if !inner_msg.is_empty() {
                    return (code.or(inner_code), inner_msg);
                }
            }
        }
        return (code, collapse_whitespace(msg));
    }

    if let Some(err_obj) = error.get("error").filter(|v| v.is_object()) {
        let (inner_code, inner_msg) = readable_provider_error_message(err_obj);
        if !inner_msg.is_empty() {
            return (code.or(inner_code), inner_msg);
        }
    }

    (code, String::new())
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(512));
    let mut prev_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
        if out.len() >= 600 {
            out.push_str("...");
            break;
        }
    }
    out.trim().to_string()
}

fn is_auth_error(code: Option<&str>, message: &str) -> bool {
    if let Some(c) = code {
        let lower = c.to_ascii_lowercase();
        if lower.contains("auth") || lower.contains("unauthorized") {
            return true;
        }
    }
    let lower = message.to_ascii_lowercase();
    lower.contains("api key")
        || lower.contains("invalid_api_key")
        || lower.contains("authentication failed")
        || lower.contains("unauthorized")
        || (lower.contains("401") && lower.contains("token"))
}

/// Live streamed text/reasoning parts that OpenCode's `/message` endpoint still returns empty
/// until the part ends.
#[derive(Default, Clone)]
struct LiveParts {
    parts: HashMap<(String, bool, u64), String>,
}

impl LiveParts {
    fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    fn append(&mut self, message_id: &str, reasoning: bool, ordinal: u64, delta: &str) {
        self.parts
            .entry((message_id.to_string(), reasoning, ordinal))
            .or_default()
            .push_str(delta);
    }

    fn prune(&mut self, messages: &[Value]) {
        self.parts.retain(|(msg_id, _, _), _| {
            !messages.iter().any(|row| {
                row["id"].as_str() == Some(msg_id.as_str()) && !row["time"]["completed"].is_null()
            })
        });
    }

    fn clear(&mut self) {
        self.parts.clear();
    }

    fn apply(&self, messages: &[Value]) -> Vec<Value> {
        if self.parts.is_empty() {
            return messages.to_vec();
        }
        messages
            .iter()
            .map(|row| {
                let Some(id) = row["id"].as_str().filter(|_| row["type"] == "assistant") else {
                    return row.clone();
                };
                if !self.parts.keys().any(|(m, _, _)| m == id) {
                    return row.clone();
                }
                let mut row = row.clone();
                let mut ordinals = [0u64; 2];
                for part in row["content"].as_array_mut().into_iter().flatten() {
                    let reasoning = match part["type"].as_str() {
                        Some("text") => false,
                        Some("reasoning") => true,
                        _ => continue,
                    };
                    let ordinal = ordinals[reasoning as usize];
                    ordinals[reasoning as usize] += 1;
                    if part["text"].as_str().is_some_and(str::is_empty) {
                        if let Some(text) = self.parts.get(&(id.to_string(), reasoning, ordinal)) {
                            part["text"] = Value::String(text.clone());
                        }
                    }
                }
                row
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
enum PendingOpenCodeKind {
    Permission {
        action: String,
        resources: Vec<String>,
    },
    Form {
        form: Value,
    },
}

#[derive(Clone, Debug)]
struct PendingOpenCodeRequest {
    request_id: String,
    session_id: String,
    is_subagent: bool,
    tool_name: String,
    summary: String,
    reason: Option<String>,
    kind: PendingOpenCodeKind,
}

impl PendingOpenCodeRequest {
    fn is_question(&self) -> bool {
        matches!(self.kind, PendingOpenCodeKind::Form { .. })
    }

    fn to_ghostex_prompt(&self) -> Value {
        match &self.kind {
            PendingOpenCodeKind::Permission { .. } => {
                let mut prompt = json!({
                    "kind": "approval",
                    "tool": self.tool_name,
                    "summary": self.summary,
                    "toolUseId": self.request_id,
                    "options": [
                        { "optionId": "once", "label": "Allow", "kind": "allow_once" },
                        { "optionId": "reject", "label": "Deny", "kind": "reject_once" }
                    ],
                });
                if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
                    prompt["reason"] = json!(reason);
                }
                prompt
            }
            PendingOpenCodeKind::Form { form } => {
                if let Some(prompt) = crate::session_chat_opencode::form_prompt(form) {
                    if let Ok(val) = serde_json::to_value(prompt) {
                        return val;
                    }
                }
                json!({
                    "kind": "question",
                    "toolUseId": self.request_id,
                    "questions": [],
                })
            }
        }
    }

    fn to_activity_permission(&self) -> PendingPermission {
        let prompt = self.to_ghostex_prompt();
        PendingPermission {
            rpc_id: Value::String(self.request_id.clone()),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            tool_use_id: Some(self.request_id.clone()),
            reason: self.reason.clone(),
            is_question: self.is_question(),
            options: vec![
                AcpPermissionOption {
                    option_id: "once".to_string(),
                    name: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                },
                AcpPermissionOption {
                    option_id: "reject".to_string(),
                    name: "Deny".to_string(),
                    kind: "reject_once".to_string(),
                },
            ],
            custom_prompt_override: Some(prompt),
        }
    }
}

/// Extract the allowed plan-file edit paths from OpenCode's `GET /api/agent` response for `"plan"`.
fn extract_plan_agent_paths(agents_resp: &Value) -> Vec<String> {
    let Some(agents) = agents_resp.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for agent in agents {
        let id = agent
            .get("id")
            .or_else(|| agent.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !id.eq_ignore_ascii_case("plan") {
            continue;
        }
        for rule in agent
            .get("permissions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if rule.get("action").and_then(Value::as_str) == Some("edit")
                && rule.get("effect").and_then(Value::as_str) == Some("allow")
            {
                if let Some(res) = rule.get("resource").and_then(Value::as_str) {
                    if !res.is_empty() && res != "*" && !paths.iter().any(|p| p == res) {
                        paths.push(res.to_string());
                    }
                }
            }
        }
    }
    paths
}

/// Build OpenCode 2 session permission rules (last match wins) for Ghostex's approval level,
/// optional `--read-only` mode, and in-memory session-scoped grants.
fn build_session_rules(
    level: AcpApprovalLevel,
    read_only: bool,
    plan_paths: &[String],
    grants: &[(String, String)],
) -> Vec<Value> {
    let mut rules = Vec::new();
    if read_only {
        rules.push(json!({ "action": "shell", "resource": "*", "effect": "ask" }));
        rules.push(json!({ "action": "external_directory", "resource": "*", "effect": "ask" }));
        for (action, resource) in grants {
            if action != "edit" {
                rules.push(json!({ "action": action, "resource": resource, "effect": "allow" }));
            }
        }
        rules.push(json!({ "action": "edit", "resource": "*", "effect": "deny" }));
        for path in plan_paths {
            rules.push(json!({ "action": "edit", "resource": path, "effect": "allow" }));
        }
        return rules;
    }
    match level {
        AcpApprovalLevel::FullAccess => {
            rules.push(json!({ "action": "*", "resource": "*", "effect": "allow" }));
        }
        AcpApprovalLevel::AutoEdits => {
            rules.push(json!({ "action": "shell", "resource": "*", "effect": "ask" }));
            rules.push(json!({ "action": "edit", "resource": "*", "effect": "allow" }));
            rules.push(json!({ "action": "external_directory", "resource": "*", "effect": "ask" }));
            for (action, resource) in grants {
                rules.push(json!({ "action": action, "resource": resource, "effect": "allow" }));
            }
        }
        AcpApprovalLevel::Ask | AcpApprovalLevel::Auto => {
            rules.push(json!({ "action": "shell", "resource": "*", "effect": "ask" }));
            rules.push(json!({ "action": "edit", "resource": "*", "effect": "ask" }));
            rules.push(json!({ "action": "external_directory", "resource": "*", "effect": "ask" }));
            for (action, resource) in grants {
                rules.push(json!({ "action": action, "resource": resource, "effect": "allow" }));
            }
        }
    }
    rules
}

fn opencode_permission_tool_name(action: &str) -> String {
    match action.trim().to_ascii_lowercase().as_str() {
        "shell" | "bash" => "Bash".to_string(),
        "edit" | "write" | "apply_patch" => "Edit".to_string(),
        "read" => "Read".to_string(),
        "external_directory" => "External Directory".to_string(),
        "webfetch" | "websearch" => "Web".to_string(),
        other if !other.is_empty() => action.to_string(),
        _ => "OpenCode".to_string(),
    }
}

fn synthetic_acp_permission_params(action: &str, resources: &[String]) -> Value {
    let lower = action.trim().to_ascii_lowercase();
    let (kind, raw_input, locations) = match lower.as_str() {
        "edit" | "write" | "apply_patch" => {
            let locs: Vec<Value> = resources
                .iter()
                .map(|r| r.trim())
                .filter(|r| !r.is_empty() && *r != "*")
                .map(|r| json!({ "path": r }))
                .collect();
            ("edit", json!({ "resources": resources }), locs)
        }
        "read" => {
            let locs: Vec<Value> = resources
                .iter()
                .map(|r| r.trim())
                .filter(|r| !r.is_empty() && *r != "*")
                .map(|r| json!({ "path": r }))
                .collect();
            // Do not auto-approve `*.env` secret reads in Ask/AutoEdits.
            let is_secret = resources
                .iter()
                .any(|r| r.to_ascii_lowercase().contains(".env"));
            (
                if is_secret { "other" } else { "read" },
                json!({ "resources": resources }),
                locs,
            )
        }
        "shell" | "bash" => (
            "execute",
            json!({ "command": resources.join(" ") }),
            Vec::new(),
        ),
        "external_directory" => ("other", json!({ "resources": resources }), Vec::new()),
        "webfetch" | "websearch" => ("fetch", json!({ "resources": resources }), Vec::new()),
        _ => ("other", json!({ "resources": resources }), Vec::new()),
    };
    json!({
        "toolCall": {
            "name": opencode_permission_tool_name(action),
            "kind": kind,
            "rawInput": raw_input,
            "locations": locations,
        }
    })
}

fn asked_form_fields(form: &Value) -> Option<Vec<&Value>> {
    let fields = form.get("fields")?.as_array()?;
    if fields.iter().any(|field| field["type"] == "external") {
        return None;
    }
    Some(
        fields
            .iter()
            .filter(|field| field["hidden"] != true)
            .collect(),
    )
}

fn external_form_url(form: &Value) -> Option<String> {
    form.get("fields")?
        .as_array()?
        .iter()
        .find(|field| field["type"] == "external")
        .and_then(|field| field.get("url").and_then(Value::as_str))
        .map(str::to_string)
}

fn form_condition_holds(field: &Value, answers: &Map<String, Value>) -> bool {
    let Some(when) = field.get("when").filter(|w| w.is_object()) else {
        return true;
    };
    let Some(answer) = when["key"].as_str().and_then(|key| answers.get(key)) else {
        return false;
    };
    let equal = match answer {
        Value::Array(values) => values.contains(&when["value"]),
        other => other == &when["value"],
    };
    if when["op"] == "neq" {
        !equal
    } else {
        equal
    }
}

fn build_form_reply_answers(form: &Value, selections: &[Value]) -> Result<Value, String> {
    let fields = asked_form_fields(form)
        .ok_or_else(|| "This OpenCode form includes an unsupported external step.".to_string())?;
    if fields.len() != selections.len() {
        return Err("Answer every OpenCode question.".to_string());
    }
    let mut answers = Map::new();
    for (field, sel) in fields.iter().copied().zip(selections) {
        let key = field["key"]
            .as_str()
            .ok_or_else(|| "Invalid OpenCode question key.".to_string())?;
        if !form_condition_holds(field, &answers) {
            continue;
        }
        let indices: Vec<usize> = sel
            .get("indices")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_u64)
                    .map(|n| n as usize)
                    .collect()
            })
            .unwrap_or_default();
        let other = sel
            .get("other")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let mut values = Vec::new();
        if field["type"] != "boolean" {
            for idx in &indices {
                let opt = field["options"]
                    .get(*idx)
                    .and_then(|o| {
                        o.get("value")
                            .or_else(|| o.get("label"))
                            .and_then(Value::as_str)
                    })
                    .ok_or_else(|| "A selected option no longer exists.".to_string())?;
                values.push(opt.to_string());
            }
        }
        if let Some(custom) = other {
            values.push(custom.to_string());
        }

        let value = if field["type"] == "boolean" {
            match indices.as_slice() {
                [0] => json!(true),
                [1] => json!(false),
                _ => return Err("Choose Yes or No.".to_string()),
            }
        } else if field["type"] == "multiselect" {
            json!(values)
        } else {
            if values.len() != 1 {
                return Err("Provide one answer for each question.".to_string());
            }
            if matches!(field["type"].as_str(), Some("number" | "integer")) {
                serde_json::from_str::<Value>(&values[0])
                    .ok()
                    .filter(|v| v.is_number())
                    .ok_or_else(|| "This question needs a number.".to_string())?
            } else {
                json!(values[0])
            }
        };
        answers.insert(key.to_string(), value);
    }
    Ok(json!({ "answer": Value::Object(answers) }))
}

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

fn build_opencode_file_attachments(
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
        let (bytes, raw_mime) = super::read_validated_acp_image(gx_paths, raw)?;
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

fn extract_skill_references(text: &str, known_skills: &HashSet<String>) -> Vec<Value> {
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

fn parse_model_reference(model_value: &str) -> Option<(String, String)> {
    let (provider, id) = model_value.trim().split_once('/')?;
    let provider = provider.trim();
    let id = id.trim();
    if provider.is_empty() || id.is_empty() {
        return None;
    }
    Some((provider.to_string(), id.to_string()))
}

fn extract_model_from_info_or_messages(
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

fn extract_usage_from_messages(messages: &[Value]) -> Option<u64> {
    let msg = messages
        .iter()
        .rev()
        .take_while(|m| !(m["type"] == "compaction" && m["status"] == "completed"))
        .find(|m| m["type"] == "assistant" && m["tokens"].is_object())?;
    tokens_total(&msg["tokens"])
}

fn tokens_total(t: &Value) -> Option<u64> {
    let used = [
        &t["input"],
        &t["output"],
        &t["reasoning"],
        &t["cache"]["read"],
        &t["cache"]["write"],
    ]
    .into_iter()
    .filter_map(Value::as_u64)
    .fold(0u64, u64::saturating_add);
    (used > 0).then_some(used)
}

fn extract_tasks_from_messages(messages: &[Value]) -> Option<SessionChatAgentTasks> {
    let todos = messages
        .iter()
        .rev()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten().rev())
        .find(|p| {
            p["type"] == "tool" && p["name"] == "todowrite" && p["state"]["status"] == "completed"
        })?["state"]["input"]["todos"]
        .as_array()?;
    let tasks: Vec<SessionChatAgentTask> = todos
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            Some(SessionChatAgentTask {
                id: t["id"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| i.to_string()),
                subject: t["content"].as_str()?.into(),
                status: t["status"].as_str().unwrap_or("pending").into(),
                active_form: None,
                blocked_by: vec![],
            })
        })
        .collect();
    (!tasks.is_empty()).then_some(SessionChatAgentTasks { tasks })
}

fn finished_task_children(messages: &[Value]) -> HashSet<String> {
    let mut latest = HashMap::new();
    for part in messages
        .iter()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .filter(|p| p["type"] == "tool" && matches!(p["name"].as_str(), Some("task" | "subagent")))
    {
        if let Some(child) = part["state"]["metadata"]["sessionId"]
            .as_str()
            .or_else(|| part["state"]["metadata"]["sessionID"].as_str())
        {
            latest.insert(child.to_string(), part["state"]["status"] == "completed");
        }
    }
    latest
        .into_iter()
        .filter_map(|(child, completed)| completed.then_some(child))
        .collect()
}

fn build_fleet(
    children: &HashMap<String, Value>,
    active_sessions: &HashSet<String>,
    messages: &[Value],
) -> Option<SessionChatAgentFleet> {
    if children.is_empty() {
        return None;
    }
    let now = now_ms();
    let finished = finished_task_children(messages);
    let mut list: Vec<&Value> = children.values().collect();
    list.sort_by_key(|c| c["time"]["created"].as_i64().unwrap_or(0));
    let agents: Vec<SessionChatSubAgent> = list
        .into_iter()
        .filter_map(|child| {
            let id = child["id"].as_str()?;
            let started = child["time"]["created"].as_i64().unwrap_or(now);
            let working = active_sessions.contains(id);
            Some(SessionChatSubAgent {
                id: Some(id.to_string()),
                started_at: started,
                working,
                done: !working && finished.contains(id),
                name: child["agent"].as_str().unwrap_or("OpenCode agent").into(),
                task: child["title"].as_str().map(str::to_string),
                model: child["model"]["id"].as_str().map(str::to_string),
                effort: child["model"]["variant"].as_str().map(str::to_string),
                elapsed_seconds: Some(
                    (if working {
                        now
                    } else {
                        child["time"]["idle"].as_i64().unwrap_or(started)
                    })
                    .saturating_sub(started)
                    .max(0) as u64
                        / 1000,
                ),
                tokens: None,
                nested: None,
            })
        })
        .collect();
    (!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents))
}

fn write_atomic_mirror_bytes(path: &Path, output: &[u8]) -> std::io::Result<()> {
    let previous = fs::read(path).unwrap_or_default();
    if previous == output && path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(output)?;
        temp.persist(path).map_err(|e| e.error)?;
    }
    Ok(())
}

fn serialize_mirror_jsonl(
    messages: &[Value],
    info: &Value,
    working: bool,
    interrupted: bool,
    live: &LiveParts,
) -> Vec<u8> {
    let mut output = Vec::new();
    let effective = if live.is_empty() {
        crate::session_chat_opencode::decode_messages(messages)
    } else {
        crate::session_chat_opencode::decode_messages(&live.apply(messages))
    };
    for row in &effective {
        let _ = serde_json::to_writer(&mut output, row);
        output.push(b'\n');
    }
    if let Some(turn) = effective
        .iter()
        .rev()
        .find(|row| row.role == SessionChatRole::User)
    {
        let idle = messages.iter().rev().find(|m| m["type"] == "idle");
        let state_str = if working {
            "working"
        } else if interrupted || idle.is_some_and(|m| m["outcome"] == "interrupted") {
            "interrupted"
        } else {
            "completed"
        };
        let _ = serde_json::to_writer(
            &mut output,
            &json!({
                "opencodeLifecycle": {
                    "state": state_str,
                    "turnId": turn.id,
                    "timestamp": info["time"]["idle"].as_i64().or(turn.timestamp),
                }
            }),
        );
        output.push(b'\n');
    }
    output
}

struct OpenCodeState {
    session_id: Option<String>,
    session_info: Value,
    messages: Vec<Value>,
    live_parts: LiveParts,
    last_mirror_write: Instant,
    working: bool,
    compacting: bool,
    compacting_started_at: Option<String>,
    last_turn_interrupted: bool,
    last_turn_stop_error: Option<String>,
    last_auth_error: Option<String>,
    last_error: Option<(Option<String>, String)>,
    fatal_error: Option<String>,
    external_form_url: Option<String>,
    approval_level: AcpApprovalLevel,
    read_only: bool,
    current_agent: String,
    plan_agent_paths: Vec<String>,
    session_grants: Vec<(String, String)>,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    available_models: Value,
    model_catalog: Option<Value>,
    model_variants: HashMap<String, Vec<String>>,
    model_context_windows: HashMap<String, u64>,
    known_commands: HashSet<String>,
    known_skills: HashSet<String>,
    pending_requests: VecDeque<PendingOpenCodeRequest>,
    child_sessions: HashMap<String, Value>,
    active_sessions: HashSet<String>,
    running_shells: HashSet<String>,
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
    developer_instructions: Option<String>,
}

impl OpenCodeState {
    #[allow(clippy::too_many_arguments)]
    fn new(
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
            read_only,
            current_agent: if read_only { "plan" } else { "build" }.to_string(),
            plan_agent_paths: Vec::new(),
            session_grants: Vec::new(),
            cwd,
            writable_roots,
            current_model: initial_model,
            current_effort: initial_effort,
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

    fn clear_live_turn_output(&mut self) {
        self.live_stream_text.clear();
        self.live_stream_started_at = None;
        self.live_thought_text.clear();
        self.live_thought_started_at = None;
        self.live_tool_label = None;
        self.live_tool_detail = None;
        self.live_tool_started_at = None;
    }

    fn transcript_path_string(&self) -> Option<String> {
        self.session_id
            .as_deref()
            .filter(|id| crate::session_chat_opencode::safe_id(id))
            .map(|id| mirror_path(id).to_string_lossy().to_string())
    }

    fn write_mirror_now(&mut self) {
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

    fn validated_variant(&self, model: &str, requested: Option<&str>) -> Option<String> {
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

    fn update_window_for_current_model(&mut self) {
        if let Some(model) = self.current_model.as_deref() {
            if let Some(&win) = self.model_context_windows.get(model) {
                if win > 0 {
                    self.usage_window_size = Some(win);
                }
            }
        }
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
struct OpenCodeActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl OpenCodeActivityReporter {
    fn start(endpoint: Option<GxserverEndpoint>, state: Arc<Mutex<OpenCodeState>>) -> Self {
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

    fn sync(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(());
        }
    }
}

struct OpenCodeProcess {
    child: Child,
    lifetime: super::child_lifetime::ChildLifetime,
    http: Arc<OpenCodeHttp>,
}

struct OpenCodeSpawnConfig {
    prog: String,
    connector_envs: Vec<(String, String)>,
    cwd: String,
    spool_session_id: String,
    record_target: Option<super::child_lifetime::ChildRecordTarget>,
}

fn stop_opencode_gracefully(
    child: &mut Child,
    lifetime: &mut super::child_lifetime::ChildLifetime,
    child_pid: &AtomicU32,
) {
    let pid = child.id();
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
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

fn populate_catalog_and_metadata(
    http: &OpenCodeHttp,
    cwd: &str,
    state: &Arc<Mutex<OpenCodeState>>,
) {
    let encoded_dir = encode_query_component(cwd);
    // Cold OpenCode servers may briefly return an empty model list; retry up to 3 times.
    let mut models_resp = Value::Null;
    for attempt in 0..3 {
        if let Ok(resp) = http.request(
            "GET",
            &format!("/api/model?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        ) {
            let count = resp
                .get("data")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            models_resp = resp;
            if count > 0 || attempt == 2 {
                break;
            }
        }
        thread::sleep(Duration::from_millis(350));
    }

    let lineup = crate::session_chat_opencode::lineup_entry(&models_resp, || {
        http.request(
            "GET",
            &format!("/api/provider?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
    });

    let mut available_models = Vec::new();
    let mut model_variants = HashMap::new();
    let mut model_context_windows = HashMap::new();
    let mut model_catalog = None;

    if let Some(entry) = lineup {
        if let Some(rows) = entry.get("models").and_then(Value::as_array) {
            for row in rows {
                let Some(val) = row.get("value").and_then(Value::as_str) else {
                    continue;
                };
                let label = row.get("label").and_then(Value::as_str).unwrap_or(val);
                let efforts: Vec<String> = row
                    .get("efforts")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let ctx = row.get("contextWindow").and_then(Value::as_u64);
                if let Some(w) = ctx.filter(|w| *w > 0) {
                    model_context_windows.insert(val.to_string(), w);
                }
                model_variants.insert(val.to_string(), efforts.clone());
                available_models.push(json!({
                    "modelId": val,
                    "name": label,
                    "supportedReasoningEfforts": efforts,
                    "contextWindow": ctx,
                }));
            }
        }
        model_catalog = Some(json!({
            "schemaVersion": 1,
            "updatedAt": "2026-09-25",
            "effortLabels": {},
            "agents": { "opencode": entry },
        }));
    }

    let plan_paths = http
        .request(
            "GET",
            &format!("/api/agent?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .map(|resp| extract_plan_agent_paths(&resp))
        .unwrap_or_default();

    let known_commands: HashSet<String> = http
        .request(
            "GET",
            &format!("/api/command?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .and_then(|resp| resp.get("data").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter_map(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .map(|s| s.trim().trim_start_matches('/').to_string())
                .filter(|s| !s.is_empty())
        })
        .collect();

    let known_skills: HashSet<String> = http
        .request(
            "GET",
            &format!("/api/skill?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .and_then(|resp| resp.get("data").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter_map(|s| {
            s.get("id")
                .or_else(|| s.get("name"))
                .and_then(Value::as_str)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .collect();

    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
    s.available_models = Value::Array(available_models);
    s.model_catalog = model_catalog;
    s.model_variants = model_variants;
    s.model_context_windows = model_context_windows;
    s.plan_agent_paths = plan_paths;
    s.known_commands = known_commands;
    s.known_skills = known_skills;
    s.update_window_for_current_model();
}

fn sync_child_mirror(http: &OpenCodeHttp, root_id: &str, child_info: &Value, active: bool) {
    let Some(child_id) = child_info
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::session_chat_opencode::safe_id(id))
    else {
        return;
    };
    let Ok(messages) = http.fetch_messages(child_id) else {
        return;
    };
    let bytes = serialize_mirror_jsonl(&messages, child_info, active, false, &LiveParts::default());
    let path = subagent_mirror_path(child_id);
    let _ = write_atomic_mirror_bytes(&path, &bytes);
    let meta = json!({
        "id": child_id,
        "rootId": root_id,
        "title": child_info.get("title").and_then(Value::as_str),
        "agent": child_info.get("agent").and_then(Value::as_str),
    });
    if let Ok(meta_bytes) = serde_json::to_vec(&meta) {
        let _ = write_atomic_mirror_bytes(&subagent_meta_path(child_id), &meta_bytes);
    }
}

fn handle_incoming_permission(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    perm: &Value,
    target_session_id: &str,
    is_subagent: bool,
) {
    let Some(req_id) = perm
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::session_chat_opencode::safe_id(id))
    else {
        return;
    };
    let action = perm
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("permission")
        .to_string();
    let resources: Vec<String> = perm
        .get("resources")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let summary = if resources.is_empty() {
        action.clone()
    } else {
        resources.join("\n")
    };
    let tool_name = opencode_permission_tool_name(&action);
    let (level, roots, cwd, already_pending) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let dup = s
            .pending_requests
            .iter()
            .any(|p| p.request_id == req_id && !p.is_question());
        (
            s.approval_level,
            s.writable_roots.clone(),
            s.cwd.clone(),
            dup,
        )
    };
    if already_pending {
        return;
    }

    let allow_opts = vec![
        AcpPermissionOption {
            option_id: "once".to_string(),
            name: "Allow".to_string(),
            kind: "allow_once".to_string(),
        },
        AcpPermissionOption {
            option_id: "reject".to_string(),
            name: "Deny".to_string(),
            kind: "reject_once".to_string(),
        },
    ];
    let synthetic = synthetic_acp_permission_params(&action, &resources);
    let reason = match evaluate_permission(level, &synthetic, &allow_opts, &roots, &cwd) {
        PermissionDecision::AutoApprove { .. } => {
            println!(
                "\r\n[OpenCode Native] Auto-approved ({}): {} ({})",
                level.label(),
                tool_name,
                summary
            );
            let _ = std::io::stdout().flush();
            let _ = http.session(
                target_session_id,
                &format!("/permission/{req_id}/reply"),
                "POST",
                Some(&json!({ "decision": "once" })),
                HTTP_TIMEOUT,
            );
            return;
        }
        PermissionDecision::AskUser { reason } => reason,
    };

    let pending = PendingOpenCodeRequest {
        request_id: req_id.to_string(),
        session_id: target_session_id.to_string(),
        is_subagent,
        tool_name,
        summary,
        reason,
        kind: PendingOpenCodeKind::Permission { action, resources },
    };
    let was_empty = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let empty = s.pending_requests.is_empty();
        s.pending_requests.push_back(pending);
        empty
    };
    if was_empty {
        activity.sync();
    }
}

fn handle_incoming_form(
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    form: &Value,
    target_session_id: &str,
    is_subagent: bool,
) {
    let Some(form_id) = form
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::session_chat_opencode::safe_id(id))
    else {
        return;
    };
    if asked_form_fields(form).is_none() {
        let url = external_form_url(form);
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.external_form_url = url.or_else(|| Some(String::new()));
        return;
    }
    let summary = form
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Question")
        .to_string();
    let pending = PendingOpenCodeRequest {
        request_id: form_id.to_string(),
        session_id: target_session_id.to_string(),
        is_subagent,
        tool_name: "AskUserQuestion".to_string(),
        summary,
        reason: None,
        kind: PendingOpenCodeKind::Form { form: form.clone() },
    };
    let was_empty = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.pending_requests.iter().any(|p| p.request_id == form_id) {
            return;
        }
        let empty = s.pending_requests.is_empty();
        s.pending_requests.push_back(pending);
        empty
    };
    if was_empty {
        activity.sync();
    }
}

/// Reconcile full session state from the HTTP API (used on startup, after turn events, and on
/// SSE reconnect).
fn reconcile_session(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    sync_children: bool,
) {
    let Some(sid) = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .session_id
        .clone()
    else {
        return;
    };

    let info = http
        .session(&sid, "", "GET", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").cloned());
    let messages = http.fetch_messages(&sid).ok();
    let active_map = http
        .request("GET", "/api/session/active", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let perms = http
        .session(&sid, "/permission", "GET", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let forms = http
        .session(&sid, "/form", "GET", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let children_arr = http
        .request(
            "GET",
            &format!("/api/session?parentID={sid}&limit=200"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default();

    let mut active_set: HashSet<String> = active_map.keys().cloned().collect();
    let mut child_ids = Vec::new();
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(ref info_val) = info {
            s.session_info = info_val.clone();
            if let Some(title) = info_val
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                s.session_title = Some(title.to_string());
            }
            if let Some(agent_name) = info_val
                .get("agent")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|a| !a.is_empty())
            {
                s.current_agent = agent_name.to_string();
            }
        }
        if let Some(ref msgs) = messages {
            s.live_parts.prune(msgs);
            s.messages = msgs.clone();
            if let Some(used) = extract_usage_from_messages(msgs) {
                s.usage_used_tokens = Some(used);
            }
            let running_shells: HashSet<String> = msgs
                .iter()
                .filter(|m| m["type"] == "shell" && m["status"] == "running")
                .filter_map(|m| m["shellID"].as_str().map(str::to_string))
                .collect();
            s.running_shells = running_shells;
        }
        let (detected_model, detected_variant) =
            extract_model_from_info_or_messages(&s.session_info, &s.messages);
        if let Some(m) = detected_model {
            s.current_model = Some(m);
            s.current_effort = detected_variant;
            s.update_window_for_current_model();
        }
        for child in &children_arr {
            if let Some(cid) = child.get("id").and_then(Value::as_str) {
                s.child_sessions.insert(cid.to_string(), child.clone());
                child_ids.push((cid.to_string(), child.clone()));
            }
        }
        // Keep active set aligned with OpenCode server.
        let lead_active = active_set.contains(&sid) || !s.running_shells.is_empty();
        if !lead_active && s.working && !s.compacting {
            s.working = false;
            s.clear_live_turn_output();
            s.live_parts.clear();
        } else if lead_active {
            s.working = true;
        }
        s.active_sessions = std::mem::take(&mut active_set);
        // Prune any pending main-session permissions/forms that OpenCode no longer lists.
        let open_perm_ids: HashSet<&str> = perms
            .iter()
            .filter_map(|p| p.get("id").and_then(Value::as_str))
            .collect();
        let open_form_ids: HashSet<&str> = forms
            .iter()
            .filter_map(|f| f.get("id").and_then(Value::as_str))
            .collect();
        s.pending_requests.retain(|req| {
            if req.session_id != sid {
                return true;
            }
            if req.is_question() {
                open_form_ids.contains(req.request_id.as_str())
            } else {
                open_perm_ids.contains(req.request_id.as_str())
            }
        });
        if forms.iter().all(|f| asked_form_fields(f).is_some()) {
            s.external_form_url = None;
        }
        s.write_mirror_now();
    }

    for p in &perms {
        handle_incoming_permission(http, state, activity, p, &sid, false);
    }
    for f in &forms {
        handle_incoming_form(state, activity, f, &sid, false);
    }

    if sync_children {
        let active_snapshot = state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .active_sessions
            .clone();
        for (cid, cinfo) in &child_ids {
            sync_child_mirror(http, &sid, cinfo, active_snapshot.contains(cid));
            if let Ok(cperms) = http.session(cid, "/permission", "GET", None, HTTP_TIMEOUT) {
                for p in cperms
                    .get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    handle_incoming_permission(http, state, activity, p, cid, true);
                }
            }
            if let Ok(cforms) = http.session(cid, "/form", "GET", None, HTTP_TIMEOUT) {
                for f in cforms
                    .get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    handle_incoming_form(state, activity, f, cid, true);
                }
            }
        }
    }
    activity.sync();
}

fn spawn_sse_loop(
    http: Arc<OpenCodeHttp>,
    state: Arc<Mutex<OpenCodeState>>,
    activity: OpenCodeActivityReporter,
    spool: Arc<Mutex<SpoolWriter>>,
    process_gen: Arc<AtomicU64>,
    my_gen: u64,
) {
    thread::spawn(move || {
        let mut backoff = Duration::from_millis(250);
        let mut ever_connected = false;
        while process_gen.load(Ordering::Acquire) == my_gen {
            match http.open_event_stream() {
                Ok(resp) => {
                    let was_reconnect = ever_connected;
                    ever_connected = true;
                    backoff = Duration::from_millis(250);
                    if was_reconnect {
                        reconcile_session(&http, &state, &activity, true);
                    }
                    let mut reader = BufReader::new(resp.into_body().into_reader());
                    let mut line = String::new();
                    loop {
                        if process_gen.load(Ordering::Acquire) != my_gen {
                            return;
                        }
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) => break,
                            Ok(_) => {
                                let trimmed = line.trim();
                                if let Some(data_str) = trimmed.strip_prefix("data:") {
                                    let payload = data_str.trim();
                                    if let Ok(event) = serde_json::from_str::<Value>(payload) {
                                        if let Ok(mut sp) = spool.lock() {
                                            sp.append(&event);
                                        }
                                        let res = std::panic::catch_unwind(
                                            std::panic::AssertUnwindSafe(|| {
                                                handle_sse_event(&http, &state, &activity, &event);
                                            }),
                                        );
                                        if res.is_err() {
                                            eprintln!(
                                                "[OpenCode Native] Warning: recovered from panic in SSE handler."
                                            );
                                        }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(_) => {}
            }
            if process_gen.load(Ordering::Acquire) != my_gen {
                return;
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    });
}

fn handle_sse_event(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    event: &Value,
) {
    let Some(kind) = event.get("type").and_then(Value::as_str) else {
        return;
    };
    let data = event.get("data").unwrap_or(&Value::Null);
    let event_sid = data
        .get("sessionID")
        .or_else(|| data.pointer("/session/id"))
        .or_else(|| data.pointer("/form/sessionID"))
        .or_else(|| data.pointer("/shell/metadata/sessionID"))
        .and_then(Value::as_str);

    let (main_sid, is_child) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.last_event_at_ms = now_ms();
        let main = s.session_id.clone();
        // Check if this is a newly created descendant session.
        if kind == "session.created" {
            let created = data.get("session").unwrap_or(data);
            if let (Some(cid), Some(parent)) = (
                created.get("id").and_then(Value::as_str),
                created.get("parentID").and_then(Value::as_str),
            ) {
                if main.as_deref() == Some(parent) || s.child_sessions.contains_key(parent) {
                    s.child_sessions.insert(cid.to_string(), created.clone());
                }
            }
        }
        let child = event_sid.is_some_and(|esid| s.child_sessions.contains_key(esid));
        (main, child)
    };

    let Some(main_sid) = main_sid else {
        return;
    };
    let is_main = event_sid == Some(main_sid.as_str());
    if !is_main && !is_child {
        return;
    }

    if is_child {
        let cid = event_sid.unwrap_or_default();
        match kind {
            "session.execution.started" => {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .active_sessions
                    .insert(cid.to_string());
            }
            "session.execution.succeeded"
            | "session.execution.failed"
            | "session.execution.interrupted" => {
                let cinfo = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.active_sessions.remove(cid);
                    s.pending_requests.retain(|r| r.session_id != cid);
                    s.child_sessions.get(cid).cloned()
                };
                if let Some(info) = cinfo {
                    sync_child_mirror(http, &main_sid, &info, false);
                }
                activity.sync();
            }
            "permission.asked" => {
                handle_incoming_permission(http, state, activity, data, cid, true);
            }
            "permission.replied" => {
                if let Some(req_id) = data
                    .get("requestID")
                    .or_else(|| data.get("id"))
                    .and_then(Value::as_str)
                {
                    let changed = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let before = s.pending_requests.len();
                        s.pending_requests.retain(|r| r.request_id != req_id);
                        s.pending_requests.len() != before
                    };
                    if changed {
                        activity.sync();
                    }
                }
            }
            "form.created" => {
                let form = data.get("form").unwrap_or(data);
                handle_incoming_form(state, activity, form, cid, true);
            }
            "form.replied" | "form.cancelled" => {
                if let Some(req_id) = data
                    .get("formID")
                    .or_else(|| data.get("id"))
                    .and_then(Value::as_str)
                {
                    let changed = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let before = s.pending_requests.len();
                        s.pending_requests.retain(|r| r.request_id != req_id);
                        s.pending_requests.len() != before
                    };
                    if changed {
                        activity.sync();
                    }
                }
            }
            "session.text.ended" | "session.tool.success" | "session.tool.failed" => {
                let (cinfo, active) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    (
                        s.child_sessions.get(cid).cloned(),
                        s.active_sessions.contains(cid),
                    )
                };
                if let Some(info) = cinfo {
                    sync_child_mirror(http, &main_sid, &info, active);
                }
            }
            _ => {}
        }
        return;
    }

    // Main session events
    match kind {
        "session.execution.started" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.working = true;
                s.active_sessions.insert(main_sid.clone());
                s.last_turn_interrupted = false;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
            }
            activity.sync();
        }
        "session.execution.succeeded" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.active_sessions.remove(&main_sid);
                s.working = !s.running_shells.is_empty();
                s.compacting = false;
                s.compacting_started_at = None;
                s.last_turn_interrupted = false;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
                s.clear_live_turn_output();
                s.live_parts.clear();
                s.pending_requests.retain(|r| r.session_id != main_sid);
            }
            println!("[OpenCode Native] Turn finished.");
            let _ = std::io::stdout().flush();
            reconcile_session(http, state, activity, true);
        }
        "session.execution.interrupted" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.active_sessions.remove(&main_sid);
                s.working = false;
                s.compacting = false;
                s.compacting_started_at = None;
                s.last_turn_interrupted = true;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
                s.clear_live_turn_output();
                s.live_parts.clear();
                s.pending_requests.retain(|r| r.session_id != main_sid);
            }
            reconcile_session(http, state, activity, true);
        }
        "session.execution.failed" => {
            let err_val = data.get("error").unwrap_or(data);
            let (code, msg) = readable_provider_error_message(err_val);
            let msg = if msg.is_empty() {
                "OpenCode failed to complete the turn.".to_string()
            } else {
                msg
            };
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.active_sessions.remove(&main_sid);
                s.working = false;
                s.compacting = false;
                s.compacting_started_at = None;
                s.clear_live_turn_output();
                s.live_parts.clear();
                s.pending_requests.retain(|r| r.session_id != main_sid);
                if is_auth_error(code.as_deref(), &msg) {
                    s.last_auth_error = Some(msg.clone());
                }
                s.last_turn_stop_error = Some(code.clone().unwrap_or_else(|| "error".to_string()));
                s.last_error = Some((code, msg));
            }
            reconcile_session(http, state, activity, true);
        }
        "session.text.delta" => {
            if let Some(delta) = data.get("delta").and_then(Value::as_str) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_stream_started_at.is_none() {
                    s.live_stream_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_stream_text, delta, LIVE_STREAM_MAX_CHARS);
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
                if let (Some(mid), Some(ord)) = (
                    data.get("assistantMessageID").and_then(Value::as_str),
                    data.get("ordinal").and_then(Value::as_u64),
                ) {
                    s.live_parts.append(mid, false, ord, delta);
                    if s.last_mirror_write.elapsed() >= MIRROR_DELTA_DEBOUNCE {
                        s.write_mirror_now();
                    }
                }
            }
        }
        "session.reasoning.delta" => {
            if let Some(delta) = data.get("delta").and_then(Value::as_str) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_thought_started_at.is_none() {
                    s.live_thought_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_thought_text, delta, LIVE_STREAM_MAX_CHARS);
                if let (Some(mid), Some(ord)) = (
                    data.get("assistantMessageID").and_then(Value::as_str),
                    data.get("ordinal").and_then(Value::as_u64),
                ) {
                    s.live_parts.append(mid, true, ord, delta);
                    if s.last_mirror_write.elapsed() >= MIRROR_DELTA_DEBOUNCE {
                        s.write_mirror_now();
                    }
                }
            }
        }
        "session.text.started" | "session.reasoning.started" => {
            // Fetch message skeleton so live deltas attach to the new assistant message row.
            if let Ok(msgs) = http.fetch_messages(&main_sid) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.messages = msgs;
                s.write_mirror_now();
            }
        }
        "session.text.ended" | "session.reasoning.ended" => {
            if let Ok(msgs) = http.fetch_messages(&main_sid) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.live_parts.prune(&msgs);
                s.messages = msgs;
                s.write_mirror_now();
            }
        }
        "session.tool.input.started" | "session.tool.called" | "session.tool.progress" => {
            let name = data
                .get("name")
                .or_else(|| data.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let detail = data
                .pointer("/input/command")
                .or_else(|| data.pointer("/input/filePath"))
                .or_else(|| data.pointer("/input/path"))
                .or_else(|| data.pointer("/input/pattern"))
                .or_else(|| data.pointer("/input/url"))
                .or_else(|| data.pointer("/input/query"))
                .or_else(|| data.pointer("/input/description"))
                .or_else(|| data.get("title"))
                .and_then(Value::as_str)
                .map(str::to_string);
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.live_tool_label = Some(name.to_string());
                s.live_tool_detail = detail;
                s.live_tool_started_at = Some(now_iso_rfc3339());
            }
            if kind == "session.tool.called" {
                if let Ok(msgs) = http.fetch_messages(&main_sid) {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.messages = msgs;
                    s.write_mirror_now();
                }
            }
        }
        "session.tool.success" | "session.tool.failed" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
            }
            if let Ok(msgs) = http.fetch_messages(&main_sid) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.messages = msgs;
                s.write_mirror_now();
            }
        }
        "session.compaction.started" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.working = true;
                s.compacting = true;
                s.compacting_started_at = Some(now_iso_rfc3339());
            }
            activity.sync();
        }
        "session.compaction.ended" | "session.compaction.failed" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.compacting = false;
                s.compacting_started_at = None;
                if kind == "session.compaction.ended" {
                    s.usage_used_tokens = None;
                }
                if !s.active_sessions.contains(&main_sid) && s.running_shells.is_empty() {
                    s.working = false;
                }
            }
            reconcile_session(http, state, activity, false);
        }
        "session.step.ended" => {
            if let Some(tokens) = data.get("tokens") {
                if let Some(used) = tokens_total(tokens) {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.usage_used_tokens = Some(used);
                    s.update_window_for_current_model();
                }
            }
        }
        "session.step.failed" => {
            if let Some(err) = data.get("error").filter(|v| !v.is_null()) {
                let (code, msg) = readable_provider_error_message(err);
                let is_abort = code
                    .as_deref()
                    .is_some_and(|c| c.to_ascii_lowercase().contains("abort"))
                    || msg.to_ascii_lowercase().contains("aborted");
                if !msg.is_empty() && !is_abort {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if is_auth_error(code.as_deref(), &msg) {
                        s.last_auth_error = Some(msg.clone());
                    }
                    s.last_error = Some((code, msg));
                }
            }
        }
        "session.retry.scheduled" => {
            let attempt = data.get("attempt").and_then(Value::as_u64);
            let msg = data
                .get("message")
                .or_else(|| data.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("Retrying request");
            let label = match attempt {
                Some(n) => format!("Retrying (attempt {n})"),
                None => "Retrying".to_string(),
            };
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.live_tool_label = Some(label);
            s.live_tool_detail = Some(msg.to_string());
            s.live_tool_started_at = Some(now_iso_rfc3339());
        }
        "session.model.selected" => {
            let model_obj = data.get("model").unwrap_or(data);
            if let (Some(provider), Some(id)) = (
                model_obj.get("providerID").and_then(Value::as_str),
                model_obj
                    .get("id")
                    .or_else(|| model_obj.get("modelID"))
                    .and_then(Value::as_str),
            ) {
                let full = format!("{provider}/{id}");
                let variant = model_obj
                    .get("variant")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("default"))
                    .map(str::to_string);
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.current_model = Some(full);
                s.current_effort = variant;
                s.update_window_for_current_model();
            }
        }
        "session.updated" => {
            let session_obj = data.get("session").unwrap_or(data);
            if let Some(title) = session_obj
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .session_title = Some(title.to_string());
            }
        }
        "permission.asked" => {
            handle_incoming_permission(http, state, activity, data, &main_sid, false);
        }
        "permission.replied" => {
            if let Some(req_id) = data
                .get("requestID")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let changed = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let before = s.pending_requests.len();
                    s.pending_requests.retain(|r| r.request_id != req_id);
                    s.pending_requests.len() != before
                };
                if changed {
                    activity.sync();
                }
            }
        }
        "form.created" => {
            let form = data.get("form").unwrap_or(data);
            handle_incoming_form(state, activity, form, &main_sid, false);
        }
        "form.replied" | "form.cancelled" => {
            if let Some(req_id) = data
                .get("formID")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let changed = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let before = s.pending_requests.len();
                    s.pending_requests.retain(|r| r.request_id != req_id);
                    s.pending_requests.len() != before
                };
                if changed {
                    activity.sync();
                }
            }
        }
        "shell.created" => {
            if let Some(shell_id) = data
                .pointer("/shell/id")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.running_shells.insert(shell_id.to_string());
                s.working = true;
            }
            activity.sync();
        }
        "shell.exited" | "shell.deleted" => {
            if let Some(shell_id) = data
                .pointer("/shell/id")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.running_shells.remove(shell_id);
                if !s.active_sessions.contains(&main_sid) && s.running_shells.is_empty() {
                    s.working = false;
                }
            }
            reconcile_session(http, state, activity, false);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_and_init_opencode(
    cfg: &OpenCodeSpawnConfig,
    resume_session_id: Option<&str>,
    fork_session: bool,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    child_pid: &Arc<AtomicU32>,
    process_gen: &Arc<AtomicU64>,
    my_gen: u64,
) -> Result<OpenCodeProcess, String> {
    let port = reserve_ephemeral_port()?;
    let password = generate_server_password();

    let mut cmd = Command::new(&cfg.prog);
    super::child_lifetime::end_with_host(&mut cmd);
    cmd.env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .env_remove("OPENCODE_SERVER_PASSWORD")
        .env_remove("GHOSTEX_SESSION_ID")
        .env("OPENCODE_PASSWORD", &password);
    for (k, v) in &cfg.connector_envs {
        if is_allowed_connector_env_key(k) {
            cmd.env(k, v);
        }
    }
    cmd.args(["serve", "--hostname=127.0.0.1", &format!("--port={port}")])
        .current_dir(&cfg.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|err| {
        format!(
            "Failed to start `{} serve --hostname=127.0.0.1 --port={port}`: {err}",
            cfg.prog
        )
    })?;
    let mut lifetime =
        super::child_lifetime::ChildLifetime::attach(&child, cfg.record_target.as_ref());
    let pid = child.id();
    child_pid.store(pid, Ordering::Release);
    state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = pid;

    let Some(stdout) = child.stdout.take() else {
        super::child_lifetime::force_kill_child(&mut child);
        lifetime.cleanup();
        child_pid.store(0, Ordering::Release);
        return Err("OpenCode server stdout unavailable.".to_string());
    };
    let stderr = child.stderr.take();
    let spool = Arc::new(Mutex::new(SpoolWriter::open(&cfg.spool_session_id)));
    let stderr_tail = Arc::new(Mutex::new(VecDeque::<String>::new()));

    if let Some(stderr) = stderr {
        let sp = spool.clone();
        let tail = stderr_tail.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(mut w) = sp.lock() {
                        w.append(&json!({ "stderr": trimmed }));
                    }
                    let mut t = tail.lock().unwrap_or_else(|p| p.into_inner());
                    if t.len() >= 16 {
                        t.pop_front();
                    }
                    t.push_back(trimmed.to_string());
                }
            }
        });
    }

    let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
    {
        let sp = spool.clone();
        let st_exit = state.clone();
        let gen_exit = process_gen.clone();
        let tail = stderr_tail.clone();
        thread::spawn(move || {
            let mut sent_ready = false;
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(mut w) = sp.lock() {
                        w.append(&json!({ "stdout": trimmed }));
                    }
                    if !sent_ready {
                        if let Some(idx) = trimmed.find("listening on http://") {
                            let url = trimmed[idx + "listening on ".len()..].trim().to_string();
                            sent_ready = true;
                            let _ = ready_tx.send(Ok(url));
                        }
                    }
                }
            }
            if !sent_ready {
                let tail_text = tail
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | ");
                let msg = if tail_text.is_empty() {
                    "OpenCode server exited before reporting its listen URL.".to_string()
                } else {
                    format!("OpenCode server exited during startup: {tail_text}")
                };
                let _ = ready_tx.send(Err(msg));
            }
            if gen_exit.load(Ordering::Acquire) == my_gen {
                let mut s = st_exit.lock().unwrap_or_else(|p| p.into_inner());
                s.child_exited = true;
                s.working = false;
            }
        });
    }

    let base_url = match ready_rx.recv_timeout(SERVER_START_TIMEOUT) {
        Ok(Ok(url)) => url,
        Ok(Err(err)) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            return Err(err);
        }
        Err(_) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            return Err("Timed out waiting for OpenCode server to start.".to_string());
        }
    };

    let http = Arc::new(OpenCodeHttp::new(base_url, &password));
    // Verify `/api/info` responds before subscribing.
    if let Err(err) = http.request("GET", "/api/info", None, HTTP_TIMEOUT) {
        process_gen.fetch_add(1, Ordering::AcqRel);
        stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
        return Err(format!("OpenCode server health check failed: {err}"));
    }

    // Load models, providers, agents (plan paths), commands, and skills.
    populate_catalog_and_metadata(&http, &cfg.cwd, state);

    // Subscribe to `/api/event` BEFORE creating/resuming/prompting so no events are missed.
    spawn_sse_loop(
        http.clone(),
        state.clone(),
        activity.clone(),
        spool.clone(),
        process_gen.clone(),
        my_gen,
    );

    // Create, resume, or fork the OpenCode session.
    let (approval_level, read_only, plan_paths, grants, req_model, req_effort, dev_instr) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (
            s.approval_level,
            s.read_only,
            s.plan_agent_paths.clone(),
            s.session_grants.clone(),
            s.current_model.clone(),
            s.current_effort.clone(),
            s.developer_instructions.clone(),
        )
    };
    let rules = build_session_rules(approval_level, read_only, &plan_paths, &grants);

    let validated_model_obj = req_model.as_deref().and_then(|m| {
        let (provider, id) = parse_model_reference(m)?;
        let variant = state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .validated_variant(m, req_effort.as_deref());
        let mut obj = json!({ "providerID": provider, "id": id });
        if let Some(v) = variant {
            obj["variant"] = json!(v);
        }
        Some(obj)
    });

    let open_or_create = || -> Result<Value, String> {
        if let Some(resume_id) = resume_session_id
            .map(str::trim)
            .filter(|s| !s.is_empty() && crate::session_chat_opencode::safe_id(s))
        {
            if fork_session {
                let forked =
                    http.session(resume_id, "/fork", "POST", Some(&json!({})), HTTP_TIMEOUT)?;
                return Ok(forked.get("data").cloned().unwrap_or(Value::Null));
            }
            let (status, existing) = http.request_raw(
                "GET",
                &format!("/api/session/{resume_id}"),
                None,
                None,
                HTTP_TIMEOUT,
            )?;
            if status == 200 {
                return Ok(existing.get("data").cloned().unwrap_or(Value::Null));
            }
            if status != 404 {
                let (_, msg) = readable_provider_error_message(&existing);
                return Err(if msg.is_empty() {
                    format!("Failed to load OpenCode session {resume_id} (HTTP {status}).")
                } else {
                    msg
                });
            }
            // Pre-minted id that has not been created on the server yet: create it with that id.
            let mut body = json!({
                "id": resume_id,
                "location": { "directory": cfg.cwd },
                "permissions": rules,
            });
            if let Some(ref model_obj) = validated_model_obj {
                body["model"] = model_obj.clone();
            }
            let created = http.request("POST", "/api/session", Some(&body), HTTP_TIMEOUT)?;
            return Ok(created.get("data").cloned().unwrap_or(Value::Null));
        }

        let mut body = json!({
            "location": { "directory": cfg.cwd },
            "permissions": rules,
        });
        if let Some(ref model_obj) = validated_model_obj {
            body["model"] = model_obj.clone();
        }
        let created = http.request("POST", "/api/session", Some(&body), HTTP_TIMEOUT)?;
        Ok(created.get("data").cloned().unwrap_or(Value::Null))
    };

    let session_data = match open_or_create() {
        Ok(data) => data,
        Err(err) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            return Err(err);
        }
    };

    let session_id = session_data
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| crate::session_chat_opencode::safe_id(s))
        .map(str::to_string)
        .ok_or_else(|| {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            "OpenCode did not return a valid session id.".to_string()
        })?;

    // Relocate session directory if cwd changed since last run.
    let existing_dir = session_data
        .pointer("/location/directory")
        .and_then(Value::as_str);
    if existing_dir.is_some_and(|d| d != cfg.cwd) {
        let _ = http.session(
            &session_id,
            "/move",
            "POST",
            Some(&json!({ "directory": cfg.cwd })),
            HTTP_TIMEOUT,
        );
    }

    // Apply Ghostex approval rules to resumed/forked sessions too.
    let _ = http.session(
        &session_id,
        "",
        "PATCH",
        Some(&json!({ "permissions": rules })),
        HTTP_TIMEOUT,
    );

    // Apply agent mode (`plan` for `--read-only`, otherwise keep existing or `build`).
    if read_only {
        let _ = http.session(
            &session_id,
            "/agent",
            "POST",
            Some(&json!({ "agent": "plan" })),
            HTTP_TIMEOUT,
        );
    }

    // Apply explicitly requested model/variant if present.
    if let Some(ref model_obj) = validated_model_obj {
        let _ = http.session(
            &session_id,
            "/model",
            "POST",
            Some(&json!({ "model": model_obj })),
            HTTP_TIMEOUT,
        );
    }

    // Apply Ghostex developer/coordinator/workspace instructions if provided.
    if let Some(instr) = dev_instr.as_deref().filter(|s| !s.trim().is_empty()) {
        let _ = http.session(
            &session_id,
            "/instructions/ghostex",
            "PUT",
            Some(&json!({ "text": instr })),
            HTTP_TIMEOUT,
        );
    }

    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.session_id = Some(session_id);
        s.session_info = session_data;
        s.child_exited = false;
        s.fatal_error = None;
    }

    reconcile_session(&http, state, activity, true);

    Ok(OpenCodeProcess {
        child,
        lifetime,
        http,
    })
}

fn stop_session_shells(http: &OpenCodeHttp, session_id: &str, directory: &str) {
    let mut shell_ids = Vec::new();
    if let Ok(messages) = http.fetch_messages(session_id) {
        for m in messages {
            if m["type"] == "shell" && m["status"] == "running" {
                if let Some(sh) = m["shellID"]
                    .as_str()
                    .filter(|id| crate::session_chat_opencode::safe_id(id))
                {
                    if !shell_ids.iter().any(|e| e == sh) {
                        shell_ids.push(sh.to_string());
                    }
                }
            }
        }
    }
    if let Ok(shells) = http.request_in("GET", "/api/shell", directory, None, HTTP_TIMEOUT) {
        for sh in shells
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if sh.pointer("/metadata/sessionID").and_then(Value::as_str) == Some(session_id)
                && sh["status"] == "running"
            {
                if let Some(id) = sh["id"]
                    .as_str()
                    .filter(|id| crate::session_chat_opencode::safe_id(id))
                {
                    if !shell_ids.iter().any(|e| e == id) {
                        shell_ids.push(id.to_string());
                    }
                }
            }
        }
    }
    for sh in shell_ids {
        let _ = http.request_in(
            "DELETE",
            &format!("/api/shell/{sh}"),
            directory,
            None,
            HTTP_TIMEOUT,
        );
    }
}

fn cancel_all_pending(http: &OpenCodeHttp, state: &Arc<Mutex<OpenCodeState>>) {
    let drained: Vec<PendingOpenCodeRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        match req.kind {
            PendingOpenCodeKind::Permission { .. } => {
                let _ = http.session(
                    &req.session_id,
                    &format!("/permission/{}/reply", req.request_id),
                    "POST",
                    Some(&json!({ "decision": "reject" })),
                    Duration::from_secs(5),
                );
            }
            PendingOpenCodeKind::Form { .. } => {
                let body = req
                    .is_subagent
                    .then(|| json!({ "message": DISMISSED_FORM_MESSAGE }));
                let _ = http.session(
                    &req.session_id,
                    &format!("/form/{}", req.request_id),
                    "DELETE",
                    body.as_ref(),
                    Duration::from_secs(5),
                );
            }
        }
    }
}

fn interrupt_active_turn(http: &OpenCodeHttp, state: &Arc<Mutex<OpenCodeState>>, cwd: &str) {
    cancel_all_pending(http, state);
    let (sid_opt, child_ids) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working {
            s.last_turn_interrupted = true;
        }
        let active_children: Vec<String> = s
            .active_sessions
            .iter()
            .filter(|id| Some(id.as_str()) != s.session_id.as_deref())
            .cloned()
            .collect();
        (s.session_id.clone(), active_children)
    };
    for cid in child_ids {
        let _ = http.session(&cid, "/interrupt", "POST", None, Duration::from_secs(5));
    }
    if let Some(sid) = sid_opt.as_deref() {
        let _ = http.session(sid, "/interrupt", "POST", None, Duration::from_secs(5));
        stop_session_shells(http, sid, cwd);
    }
    let wait_until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < wait_until {
        if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.working = false;
        s.compacting = false;
        s.compacting_started_at = None;
        s.running_shells.clear();
        s.clear_live_turn_output();
    }
}

fn apply_approval_level(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    desired: AcpApprovalLevel,
) -> Result<(), String> {
    let (sid, rules, leave_plan) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let was_read_only = s.read_only;
        s.approval_level = desired;
        if desired != AcpApprovalLevel::Ask {
            s.read_only = false;
        }
        let leave_plan = was_read_only && !s.read_only && s.current_agent == "plan";
        let rules = build_session_rules(
            s.approval_level,
            s.read_only,
            &s.plan_agent_paths,
            &s.session_grants,
        );
        (s.session_id.clone(), rules, leave_plan)
    };
    let Some(sid) = sid else {
        return Err("OpenCode session is not initialized.".to_string());
    };
    http.session(
        &sid,
        "",
        "PATCH",
        Some(&json!({ "permissions": rules })),
        HTTP_TIMEOUT,
    )?;
    if leave_plan {
        let _ = http.session(
            &sid,
            "/agent",
            "POST",
            Some(&json!({ "agent": "build" })),
            HTTP_TIMEOUT,
        );
        state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .current_agent = "build".to_string();
    }
    // If raising to FullAccess while permissions are already waiting, auto-approve them now.
    if desired == AcpApprovalLevel::FullAccess {
        let to_auto: Vec<PendingOpenCodeRequest> = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            let mut rest = VecDeque::new();
            let mut auto = Vec::new();
            while let Some(req) = s.pending_requests.pop_front() {
                if !req.is_question() {
                    auto.push(req);
                } else {
                    rest.push_back(req);
                }
            }
            s.pending_requests = rest;
            auto
        };
        for req in to_auto {
            let _ = http.session(
                &req.session_id,
                &format!("/permission/{}/reply", req.request_id),
                "POST",
                Some(&json!({ "decision": "once" })),
                HTTP_TIMEOUT,
            );
        }
    }
    activity.sync();
    Ok(())
}

fn handle_rewind(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    message_id: &str,
) -> Result<Value, String> {
    let (sid, is_working) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (s.session_id.clone(), s.working)
    };
    if is_working {
        return Err("ACP_AGENT_BUSY".to_string());
    }
    let Some(sid) = sid else {
        return Err("REWIND_NOT_FOUND".to_string());
    };
    let messages = http.fetch_messages(&sid)?;
    if !messages
        .iter()
        .any(|m| m["id"].as_str() == Some(message_id) && m["type"] == "user")
    {
        return Err("REWIND_NOT_FOUND".to_string());
    }
    if let Err(err) = http.session(
        &sid,
        "/revert/stage",
        "POST",
        Some(&json!({ "messageID": message_id, "files": false })),
        HTTP_LONG_TIMEOUT,
    ) {
        let _ = http.session(&sid, "/revert/clear", "POST", None, HTTP_TIMEOUT);
        return Err(err);
    }
    if let Err(err) = http.session(&sid, "/revert/commit", "POST", None, HTTP_LONG_TIMEOUT) {
        let _ = http.session(&sid, "/revert/clear", "POST", None, HTTP_TIMEOUT);
        return Err(err);
    }
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.usage_used_tokens = None;
        s.last_error = None;
        s.last_turn_stop_error = None;
        s.last_turn_interrupted = false;
    }
    reconcile_session(http, state, activity, false);
    Ok(json!({
        "ok": true,
        "targetMessageId": message_id,
        "warning": null,
    }))
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

    let prog = resolve_opencode_binary();
    let initial_approval_level = if parsed.read_only {
        AcpApprovalLevel::Ask
    } else {
        parsed.approval_level.unwrap_or(AcpApprovalLevel::Ask)
    };

    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);
    let spawn_cfg = OpenCodeSpawnConfig {
        prog,
        connector_envs: parsed.profile.connector_envs.clone(),
        cwd: cwd.clone(),
        spool_session_id,
        record_target: record_target.clone(),
    };

    let file_instructions =
        validated_ghostex_prompt_file(parsed.profile.append_system_prompt_file.as_deref())
            .and_then(|path| fs::read_to_string(path).ok())
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

    let state = Arc::new(Mutex::new(OpenCodeState::new(
        initial_approval_level,
        parsed.read_only,
        combined_developer_instructions,
        cwd_path.clone(),
        writable_roots,
        parsed.model.clone(),
        parsed.effort.clone(),
    )));
    let activity = OpenCodeActivityReporter::start(endpoint.clone(), state.clone());
    let shutting_down = Arc::new(AtomicBool::new(false));
    let child_pid = Arc::new(AtomicU32::new(0));
    let process_gen = Arc::new(AtomicU64::new(1));

    let mut proc_opt = match spawn_and_init_opencode(
        &spawn_cfg,
        parsed.resume_session_id.as_deref(),
        parsed.fork_session,
        &state,
        &activity,
        &child_pid,
        &process_gen,
        1,
    ) {
        Ok(proc) => Some(proc),
        Err(err) => {
            eprintln!("[OpenCode Native] Startup failed: {err}");
            return 1;
        }
    };

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

    let initial_sid = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .session_id
        .clone();
    println!(
        "Ghostex Native Host · {} · Ready (session: {})",
        spec.display_name,
        initial_sid.as_deref().unwrap_or("pending")
    );
    let _ = std::io::stdout().flush();
    activity.sync();

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
                println!("\r\n[OpenCode Native] Server exited repeatedly; stopping host.");
                break;
            }
            crash_restarts.push_back(now);
            println!("\r\n[OpenCode Native] Server exited unexpectedly; restarting...");
            let _ = std::io::stdout().flush();
            if let Some(mut old) = proc_opt.take() {
                stop_opencode_gracefully(&mut old.child, &mut old.lifetime, &child_pid);
            }
            thread::sleep(Duration::from_millis(500));
            let next_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
            let resume_id = state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .session_id
                .clone();
            match spawn_and_init_opencode(
                &spawn_cfg,
                resume_id.as_deref(),
                false,
                &state,
                &activity,
                &child_pid,
                &process_gen,
                next_gen,
            ) {
                Ok(proc) => {
                    proc_opt = Some(proc);
                    println!("\r\n[OpenCode Native] Server restarted and session resumed.");
                    let _ = std::io::stdout().flush();
                    activity.sync();
                    continue;
                }
                Err(err) => {
                    eprintln!("[OpenCode Native] Failed to restart server: {err}");
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
                    let current = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .approval_level;
                    if desired != current {
                        if let Some(proc) = proc_opt.as_ref() {
                            if let Err(err) =
                                apply_approval_level(&proc.http, &state, &activity, desired)
                            {
                                eprintln!(
                                    "[OpenCode Native] Warning: could not sync approval level: {err}"
                                );
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
                    if let Some(proc) = proc_opt.as_ref() {
                        interrupt_active_turn(&proc.http, &state, &cwd);
                        reconcile_session(&proc.http, &state, &activity, true);
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
                            Some("OpenCode server is not running."),
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
                                .position(|p| !p.is_question() && p.request_id == wanted)
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|p| !p.is_question())
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("No pending OpenCode approval."),
                        );
                        continue;
                    };
                    let norm = decision.trim().to_ascii_lowercase();
                    let is_cancel = matches!(norm.as_str(), "escape" | "cancel" | "\x1b");
                    let is_deny = matches!(
                        norm.as_str(),
                        "deny"
                            | "reject"
                            | "reject_once"
                            | "reject_always"
                            | "decline"
                            | "n"
                            | "no"
                    );
                    let is_session_grant = matches!(
                        norm.as_str(),
                        "always" | "allow_always" | "accept_for_session" | "2"
                    );

                    let res = if is_cancel {
                        proc.http.session(
                            &req.session_id,
                            &format!("/permission/{}/reply", req.request_id),
                            "POST",
                            Some(&json!({ "decision": "reject" })),
                            HTTP_TIMEOUT,
                        )
                    } else if is_deny {
                        // Steer a synthetic declined note before rejecting so the model continues
                        // gracefully without retrying the declined tool call.
                        let _ = proc.http.session(
                            &req.session_id,
                            "/synthetic",
                            "POST",
                            Some(&json!({
                                "text": DECLINED_SYNTHETIC_NOTE,
                                "delivery": "steer",
                                "resume": false,
                            })),
                            HTTP_TIMEOUT,
                        );
                        proc.http.session(
                            &req.session_id,
                            &format!("/permission/{}/reply", req.request_id),
                            "POST",
                            Some(&json!({
                                "decision": "reject",
                                "message": DECLINED_PERMISSION_MESSAGE,
                            })),
                            HTTP_TIMEOUT,
                        )
                    } else {
                        if is_session_grant {
                            if let PendingOpenCodeKind::Permission {
                                ref action,
                                ref resources,
                            } = req.kind
                            {
                                let (main_sid, rules) = {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    if resources.is_empty() {
                                        s.session_grants.push((action.clone(), "*".to_string()));
                                    } else {
                                        for r in resources {
                                            if !s
                                                .session_grants
                                                .iter()
                                                .any(|(a, res)| a == action && res == r)
                                            {
                                                s.session_grants.push((action.clone(), r.clone()));
                                            }
                                        }
                                    }
                                    let rules = build_session_rules(
                                        s.approval_level,
                                        s.read_only,
                                        &s.plan_agent_paths,
                                        &s.session_grants,
                                    );
                                    (s.session_id.clone(), rules)
                                };
                                if let Some(msid) = main_sid {
                                    let _ = proc.http.session(
                                        &msid,
                                        "",
                                        "PATCH",
                                        Some(&json!({ "permissions": rules })),
                                        HTTP_TIMEOUT,
                                    );
                                }
                            }
                        }
                        // Always reply "once" (never OpenCode's persistent "always") so no
                        // project-wide permission is silently saved to disk.
                        proc.http.session(
                            &req.session_id,
                            &format!("/permission/{}/reply", req.request_id),
                            "POST",
                            Some(&json!({ "decision": "once" })),
                            HTTP_TIMEOUT,
                        )
                    };

                    match res {
                        Ok(_) => {
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
                            Some("OpenCode server is not running."),
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
                                .position(|p| p.is_question() && p.request_id == wanted)
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
                    let PendingOpenCodeKind::Form { ref form } = req.kind else {
                        ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                        continue;
                    };
                    let body = match build_form_reply_answers(form, &selections) {
                        Ok(body) => body,
                        Err(err) => {
                            state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .pending_requests
                                .push_front(req);
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    match proc.http.session(
                        &req.session_id,
                        &format!("/form/{}/reply", req.request_id),
                        "POST",
                        Some(&body),
                        HTTP_TIMEOUT,
                    ) {
                        Ok(_) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode server is not running."),
                        );
                        continue;
                    };
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
                    let sid = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .session_id
                        .clone();
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode session not initialized."),
                        );
                        continue;
                    };
                    let effective_model = if !model.is_empty() {
                        model
                    } else {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .current_model
                            .clone()
                            .unwrap_or_default()
                    };
                    let Some((provider_id, model_id)) = parse_model_reference(&effective_model)
                    else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("An OpenCode model must include its provider (provider/model)."),
                        );
                        continue;
                    };
                    let validated_variant = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let req_eff = if effort.is_empty() {
                            None
                        } else {
                            Some(effort.as_str())
                        };
                        s.validated_variant(&effective_model, req_eff)
                    };
                    let mut model_ref = json!({
                        "providerID": provider_id,
                        "id": model_id,
                    });
                    if let Some(ref v) = validated_variant {
                        model_ref["variant"] = json!(v);
                    }
                    match proc.http.session(
                        &sid,
                        "/model",
                        "POST",
                        Some(&json!({ "model": model_ref })),
                        HTTP_TIMEOUT,
                    ) {
                        Ok(_) => {
                            let snap = {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.current_model = Some(effective_model);
                                s.current_effort = validated_variant;
                                s.update_window_for_current_model();
                                s.snapshot_json()
                            };
                            ep.ack(id, true, snap, None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "mode" => {
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode server is not running."),
                        );
                        continue;
                    };
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
                    if let Err(err) = apply_approval_level(&proc.http, &state, &activity, level) {
                        ep.ack(id, false, Value::Null, Some(&err));
                        continue;
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "rewind" => {
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode server is not running."),
                        );
                        continue;
                    };
                    let message_id = params
                        .get("messageId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    if message_id.is_empty() {
                        ep.ack(id, false, Value::Null, Some("REWIND_NOT_FOUND"));
                        continue;
                    }
                    match handle_rewind(&proc.http, &state, &activity, &message_id) {
                        Ok(result) => ep.ack(id, true, result, None),
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "steer" | "send" => {
                    let Some(proc) = proc_opt.as_ref() else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode server is not running."),
                        );
                        continue;
                    };
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

                    let files = match build_opencode_file_attachments(&gx_paths, &text, &images) {
                        Ok(f) => f,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    let (sid, is_working, has_pending, known_commands, known_skills) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.session_id.clone(),
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.known_commands.clone(),
                            s.known_skills.clone(),
                        )
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode session not initialized."),
                        );
                        continue;
                    };

                    if has_pending && !interrupt {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }

                    let skills = extract_skill_references(&text, &known_skills);

                    // Mid-turn steer without cancelling the active turn.
                    if is_working && !interrupt {
                        let mut steer_body = json!({
                            "text": text,
                            "files": files,
                            "delivery": "steer",
                        });
                        if !skills.is_empty() {
                            steer_body["skills"] = Value::Array(skills.clone());
                        }
                        match proc.http.session(
                            &sid,
                            "/prompt",
                            "POST",
                            Some(&steer_body),
                            HTTP_TIMEOUT,
                        ) {
                            Ok(_) => {
                                println!("\r\n[OpenCode Native] Steered active turn...");
                                let _ = std::io::stdout().flush();
                                ep.ack(
                                    id,
                                    true,
                                    json!({
                                        "accepted": true,
                                        "midTurn": true,
                                        "steered": true,
                                        "sessionId": sid,
                                    }),
                                    None,
                                );
                                continue;
                            }
                            Err(err) => {
                                // Wait briefly in case the turn just finished; otherwise report busy.
                                thread::sleep(Duration::from_millis(250));
                                let still_working =
                                    state.lock().unwrap_or_else(|p| p.into_inner()).working;
                                if still_working {
                                    eprintln!(
                                        "[OpenCode Native] Warning: steer failed while active: {err}"
                                    );
                                    ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                                    continue;
                                }
                            }
                        }
                    }

                    if is_working && interrupt {
                        interrupt_active_turn(&proc.http, &state, &cwd);
                    }
                    cancel_all_pending(&proc.http, &state);

                    let trimmed = text.trim();

                    // `!cmd` runs an OpenCode shell command asynchronously so the poll loop stays
                    // responsive to `abort` and `state`.
                    if let Some(shell_cmd) = text.strip_prefix('!') {
                        let shell_cmd = shell_cmd.trim_start().to_string();
                        if shell_cmd.is_empty() {
                            ep.ack(
                                id,
                                false,
                                Value::Null,
                                Some("Enter a shell command after !."),
                            );
                            continue;
                        }
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            s.last_turn_interrupted = false;
                            s.last_turn_stop_error = None;
                            s.last_error = None;
                            s.clear_live_turn_output();
                        }
                        activity.sync();
                        let http_bg = proc.http.clone();
                        let sid_bg = sid.clone();
                        let state_bg = state.clone();
                        let act_bg = activity.clone();
                        thread::spawn(move || {
                            let res = http_bg.session(
                                &sid_bg,
                                "/shell",
                                "POST",
                                Some(&json!({ "command": shell_cmd })),
                                SHELL_TIMEOUT,
                            );
                            if let Err(err) = res {
                                let mut s = state_bg.lock().unwrap_or_else(|p| p.into_inner());
                                s.working = false;
                                s.last_turn_stop_error = Some("error".to_string());
                                s.last_error = Some((None, err));
                            }
                            reconcile_session(&http_bg, &state_bg, &act_bg, false);
                        });
                        ep.ack(
                            id,
                            true,
                            json!({ "accepted": true, "sessionId": sid }),
                            None,
                        );
                        continue;
                    }

                    // Local slash commands: `/compact`, `/model`, `/agent`, or server-listed commands.
                    if let Some((command, argument)) =
                        crate::session_chat_local_command::parse_session_chat_local_command(&text)
                    {
                        let cmd_name = command.trim_start_matches('/');
                        match cmd_name {
                            "compact" if images.is_empty() => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.working = true;
                                    s.compacting = true;
                                    s.compacting_started_at = Some(now_iso_rfc3339());
                                    s.last_turn_interrupted = false;
                                    s.last_turn_stop_error = None;
                                    s.last_error = None;
                                    s.clear_live_turn_output();
                                }
                                activity.sync();
                                println!("\r\n[OpenCode Native] Compacting session...");
                                let _ = std::io::stdout().flush();
                                match proc.http.session(
                                    &sid,
                                    "/compact",
                                    "POST",
                                    Some(&json!({})),
                                    HTTP_LONG_TIMEOUT,
                                ) {
                                    Ok(_) => {
                                        ep.ack(
                                            id,
                                            true,
                                            json!({ "accepted": true, "compacting": true, "sessionId": sid }),
                                            None,
                                        );
                                    }
                                    Err(err) => {
                                        {
                                            let mut s =
                                                state.lock().unwrap_or_else(|p| p.into_inner());
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
                            "model" if images.is_empty() => {
                                let mut words = argument.split_whitespace();
                                let raw_model = words.next().unwrap_or_default();
                                let raw_variant = words.next();
                                if raw_model.is_empty() || words.next().is_some() {
                                    ep.ack(
                                        id,
                                        false,
                                        Value::Null,
                                        Some("Use /model provider/model [variant], or pick from the model menu."),
                                    );
                                    continue;
                                }
                                let Some((provider_id, model_id)) =
                                    parse_model_reference(raw_model)
                                else {
                                    ep.ack(
                                        id,
                                        false,
                                        Value::Null,
                                        Some("An OpenCode model must include its provider (provider/model)."),
                                    );
                                    continue;
                                };
                                let variant = state
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .validated_variant(raw_model, raw_variant);
                                let mut model_ref = json!({
                                    "providerID": provider_id,
                                    "id": model_id,
                                });
                                if let Some(ref v) = variant {
                                    model_ref["variant"] = json!(v);
                                }
                                match proc.http.session(
                                    &sid,
                                    "/model",
                                    "POST",
                                    Some(&json!({ "model": model_ref })),
                                    HTTP_TIMEOUT,
                                ) {
                                    Ok(_) => {
                                        {
                                            let mut s =
                                                state.lock().unwrap_or_else(|p| p.into_inner());
                                            s.current_model = Some(raw_model.to_string());
                                            s.current_effort = variant;
                                            s.update_window_for_current_model();
                                        }
                                        ep.ack(
                                            id,
                                            true,
                                            json!({ "accepted": true, "sessionId": sid }),
                                            None,
                                        );
                                    }
                                    Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                                }
                                continue;
                            }
                            "agent" if images.is_empty() => {
                                let target_agent = argument.trim();
                                if target_agent.is_empty() {
                                    ep.ack(
                                        id,
                                        false,
                                        Value::Null,
                                        Some("Use /agent build or /agent plan."),
                                    );
                                    continue;
                                }
                                match proc.http.session(
                                    &sid,
                                    "/agent",
                                    "POST",
                                    Some(&json!({ "agent": target_agent })),
                                    HTTP_TIMEOUT,
                                ) {
                                    Ok(_) => {
                                        let rules = {
                                            let mut s =
                                                state.lock().unwrap_or_else(|p| p.into_inner());
                                            s.current_agent = target_agent.to_string();
                                            s.read_only = target_agent.eq_ignore_ascii_case("plan");
                                            build_session_rules(
                                                s.approval_level,
                                                s.read_only,
                                                &s.plan_agent_paths,
                                                &s.session_grants,
                                            )
                                        };
                                        let _ = proc.http.session(
                                            &sid,
                                            "",
                                            "PATCH",
                                            Some(&json!({ "permissions": rules })),
                                            HTTP_TIMEOUT,
                                        );
                                        ep.ack(
                                            id,
                                            true,
                                            json!({ "accepted": true, "sessionId": sid }),
                                            None,
                                        );
                                    }
                                    Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                                }
                                continue;
                            }
                            other if known_commands.contains(other) => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.working = true;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    s.last_turn_interrupted = false;
                                    s.last_turn_stop_error = None;
                                    s.last_error = None;
                                    s.last_auth_error = None;
                                    s.clear_live_turn_output();
                                }
                                activity.sync();
                                let body = json!({
                                    "name": other,
                                    "text": argument,
                                    "files": files,
                                });
                                match proc.http.session(
                                    &sid,
                                    "/command",
                                    "POST",
                                    Some(&body),
                                    HTTP_LONG_TIMEOUT,
                                ) {
                                    Ok(_) => {
                                        reconcile_session(&proc.http, &state, &activity, false);
                                        ep.ack(
                                            id,
                                            true,
                                            json!({ "accepted": true, "sessionId": sid }),
                                            None,
                                        );
                                    }
                                    Err(err) => {
                                        {
                                            let mut s =
                                                state.lock().unwrap_or_else(|p| p.into_inner());
                                            s.working = false;
                                            s.last_turn_stop_error = Some("error".to_string());
                                            s.last_error = Some((None, err.clone()));
                                        }
                                        activity.sync();
                                        ep.ack(id, false, Value::Null, Some(&err));
                                    }
                                }
                                continue;
                            }
                            _ => {}
                        }
                    }

                    // Standard prompt delivery.
                    if trimmed.is_empty() && files.is_empty() {
                        ep.ack(id, false, Value::Null, Some("Prompt cannot be empty."));
                        continue;
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.compacting = false;
                        s.compacting_started_at = None;
                        s.last_turn_interrupted = false;
                        s.last_turn_stop_error = None;
                        s.last_error = None;
                        s.last_auth_error = None;
                        s.clear_live_turn_output();
                    }
                    activity.sync();
                    println!("\r\n[OpenCode Native] Turn started...");
                    let _ = std::io::stdout().flush();

                    let mut prompt_body = json!({
                        "text": text,
                        "files": files,
                    });
                    if !skills.is_empty() {
                        prompt_body["skills"] = Value::Array(skills);
                    }
                    match proc.http.session(
                        &sid,
                        "/prompt",
                        "POST",
                        Some(&prompt_body),
                        HTTP_LONG_TIMEOUT,
                    ) {
                        Ok(_) => {
                            if let Ok(msgs) = proc.http.fetch_messages(&sid) {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.messages = msgs;
                                s.write_mirror_now();
                            }
                            ep.ack(
                                id,
                                true,
                                json!({ "accepted": true, "sessionId": sid }),
                                None,
                            );
                        }
                        Err(err) => {
                            let is_auth = is_auth_error(None, &err);
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.working = false;
                                if is_auth {
                                    s.last_auth_error = Some(err.clone());
                                }
                                s.last_turn_stop_error = Some("error".to_string());
                                s.last_error = Some((None, err.clone()));
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
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.session_id.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    if (is_working || has_pending) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode is working on a turn. Wait or interrupt before switching to Terminal."),
                        );
                        continue;
                    }
                    if let Some(proc) = proc_opt.as_ref() {
                        cancel_all_pending(&proc.http, &state);
                        if is_working {
                            interrupt_active_turn(&proc.http, &state, &cwd);
                        }
                    }
                    shutting_down.store(true, Ordering::Release);
                    process_gen.fetch_add(1, Ordering::AcqRel);
                    if let Some(mut old) = proc_opt.take() {
                        stop_opencode_gracefully(&mut old.child, &mut old.lifetime, &child_pid);
                    }
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
                        Some(&format!("Unsupported OpenCode Native command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, approval_level, read_only, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.session_id.clone(),
                s.approval_level,
                s.read_only,
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
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
        stop_opencode_gracefully(&mut old.child, &mut old.lifetime, &child_pid);
    }
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
