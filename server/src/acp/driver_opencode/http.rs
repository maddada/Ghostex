//! Local HTTP client, process helpers, and provider error normalization for OpenCode 2.

use std::collections::HashSet;
use std::net::TcpListener;
use std::time::Duration;

use base64::Engine as _;
use serde_json::Value;

use super::{HTTP_TIMEOUT, SSE_SILENT_TIMEOUT};

pub(super) fn reserve_ephemeral_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("Could not bind local port for OpenCode server: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("Could not read bound port for OpenCode server: {err}"))?
        .port();
    drop(listener);
    Ok(port)
}

pub(super) fn generate_server_password() -> String {
    let a = uuid::Uuid::new_v4().simple().to_string();
    let b = uuid::Uuid::new_v4().simple().to_string();
    format!("{a}{b}")
}

pub(super) fn resolve_opencode_binary() -> String {
    let home = crate::resume_lookup::home_dir();
    if let Some(program) = crate::agent_cli::opencode::opencode2_program(&home) {
        return program.to_string_lossy().to_string();
    }
    "opencode".to_string()
}

pub(super) fn encode_query_component(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[derive(Clone)]
pub(super) struct OpenCodeHttp {
    base_url: String,
    authorization: String,
}

impl OpenCodeHttp {
    pub(super) fn new(base_url: String, password: &str) -> Self {
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

    pub(super) fn request_raw(
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

    pub(super) fn request(
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

    pub(super) fn request_in(
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

    pub(super) fn session(
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

    pub(super) fn fetch_messages(&self, session_id: &str) -> Result<Vec<Value>, String> {
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

    pub(super) fn open_event_stream(&self) -> Result<ureq::http::Response<ureq::Body>, String> {
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
pub(super) fn readable_provider_error_message(error: &Value) -> (Option<String>, String) {
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

pub(super) fn is_auth_error(code: Option<&str>, message: &str) -> bool {
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
