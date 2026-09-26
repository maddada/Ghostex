//! CDXC:SessionChat 2026-09-25 DECISION:
//! User: OpenCode v2 chat should match Claude Code support in the GPUI chat with the Rust engine.
//! CDXC:AgentProviders 2026-09-25 WHY:
//! OpenCode v2 owns mutable messages and session controls in its shared service, not a JSONL transcript or a per-terminal agent process. Read its authenticated service registration and public v2 API; never infer conversation identity from another client's server process.

use crate::domain::DomainStateError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};

pub(crate) fn error(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "dependencyUnavailable",
        message: message.into(),
    }
}

pub(crate) fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

pub(crate) fn state_directory() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::resume_lookup::home_dir().join(".local/state"))
        .join("opencode")
}

#[derive(Clone)]
pub(crate) struct Client {
    url: String,
    authorization: String,
    agent: ureq::Agent,
}

impl Client {
    pub(crate) fn discover() -> Result<Self, DomainStateError> {
        let registration: Value = fs::read(state_directory().join("service.json"))
            .ok().and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| error("OpenCode v2's local service is unavailable. Start OpenCode and open a session."))?;
        let url = registration["url"]
            .as_str()
            .ok_or_else(|| error("OpenCode service registration has no URL."))?;
        let parsed = url::Url::parse(url)
            .map_err(|_| error("OpenCode service registration has an invalid URL."))?;
        if parsed.scheme() != "http"
            || !matches!(
                parsed.host_str(),
                Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
            )
        {
            return Err(error(
                "OpenCode service registration must name a local HTTP service.",
            ));
        }
        let password = registration["password"]
            .as_str()
            .ok_or_else(|| error("OpenCode service registration has no credential."))?;
        Ok(Self {
            url: url.trim_end_matches('/').to_string(),
            authorization: format!("Basic {}", STANDARD.encode(format!("opencode:{password}"))),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(5))
                .redirects(0)
                .build(),
        })
    }

    pub(crate) fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, DomainStateError> {
        let mut request = self
            .agent
            .request(method, &format!("{}{path}", self.url))
            .set("Authorization", &self.authorization);
        if method == "POST" && path.ends_with("/shell") {
            request = request.timeout(Duration::from_secs(3600));
        }
        let response = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        }
        .map_err(|failure| match failure {
            ureq::Error::Status(status, _) => {
                error(format!("OpenCode rejected the request (HTTP {status})."))
            }
            _ => error("Could not reach the OpenCode v2 service."),
        })?;
        if response.status() == 204 {
            return Ok(Value::Null);
        }
        response
            .into_json()
            .map_err(|_| error("OpenCode returned an invalid response."))
    }

    pub(crate) fn session(
        &self,
        id: &str,
        suffix: &str,
        method: &str,
        body: Option<Value>,
    ) -> Result<Value, DomainStateError> {
        if !safe_id(id) {
            return Err(error("Invalid OpenCode session ID."));
        }
        self.request(method, &format!("/api/session/{id}{suffix}"), body)
    }

    pub(crate) fn messages(&self, id: &str) -> Result<Vec<Value>, DomainStateError> {
        let mut messages = Vec::new();
        let mut query = "?limit=200&order=asc".to_string();
        let mut cursors = std::collections::HashSet::new();
        loop {
            let page = self.session(id, &format!("/message{query}"), "GET", None)?;
            let rows = page["data"]
                .as_array()
                .ok_or_else(|| error("OpenCode returned an invalid message page."))?;
            messages.extend(rows.iter().cloned());
            let Some(next) = page["cursor"]["next"].as_str() else {
                break;
            };
            if !cursors.insert(next.to_string()) {
                return Err(error("OpenCode repeated a message pagination cursor."));
            }
            let encoded: String = url::form_urlencoded::byte_serialize(next.as_bytes()).collect();
            query = format!("?limit=200&cursor={encoded}");
        }
        Ok(messages)
    }

    pub(crate) fn send(
        &self,
        id: &str,
        text: &str,
        image_paths: &[String],
        message_id: Option<&str>,
    ) -> Result<(), DomainStateError> {
        let pending = super::snapshot(id)?;
        if !pending.permissions.is_empty() || !pending.forms.is_empty() {
            return Err(error(
                "Answer the pending OpenCode prompt before sending another message.",
            ));
        }
        let mut files = Vec::new();
        let mut paths = image_paths.to_vec();
        for path in super::attachments::image_references(text) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        for path in &paths {
            let path = std::path::Path::new(path);
            let metadata =
                fs::metadata(path).map_err(|_| error("An attached image could not be read."))?;
            if metadata.len() > 10 * 1024 * 1024 {
                return Err(error("OpenCode attachments must be 10 MB or smaller."));
            }
            let mime = match path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                _ => {
                    return Err(error(
                        "OpenCode image attachments must be PNG, JPEG, WebP, or GIF.",
                    ));
                }
            };
            let data = STANDARD
                .encode(fs::read(path).map_err(|_| error("An attached image could not be read."))?);
            files.push(json!({"uri": format!("data:{mime};base64,{data}"), "name": path.file_name().and_then(|s| s.to_str())}));
        }
        if let Some(command) = text.strip_prefix('!') {
            if command.trim().is_empty() {
                return Err(error("Enter a shell command after !."));
            }
            self.start_shell(id, command.trim_start(), message_id)?;
        } else if let Some((command, argument)) =
            crate::session_chat_local_command::parse_session_chat_local_command(text)
        {
            let name = command.trim_start_matches('/');
            match name {
                "compact" => {
                    self.session(id, "/compact", "POST", Some(json!({})))?;
                }
                "model" => {
                    if argument.trim().is_empty() {
                        return Err(error(
                            "Choose a model from the chat model picker, or use /model provider/model.",
                        ));
                    }
                    super::select(
                        id,
                        json!({"model":argument.trim(),"scope":"session"})
                            .as_object()
                            .unwrap(),
                    )?;
                }
                "agent" => {
                    if argument.trim().is_empty() {
                        return Err(error(
                            "Choose Build or Plan from the chat options, or use /agent followed by an agent name.",
                        ));
                    }
                    self.session(id, "/agent", "POST", Some(json!({"agent":argument.trim()})))?;
                }
                _ => {
                    self.session(
                        id,
                        "/command",
                        "POST",
                        Some(json!({"name":name,"text":argument,"files":files})),
                    )?;
                }
            }
        } else {
            let mut prompt = json!({"text": text, "files": files});
            if let Some(id) = message_id {
                prompt["id"] = json!(id);
            }
            self.session(id, "/prompt", "POST", Some(prompt))?;
        }
        Ok(())
    }

    /// The shell endpoint waits for completion; chat acknowledges its durable start instead.
    fn start_shell(
        &self,
        id: &str,
        command: &str,
        message_id: Option<&str>,
    ) -> Result<(), DomainStateError> {
        let message_id = message_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("msg_gx_{}", uuid::Uuid::new_v4().simple()));
        if self.messages(id)?.iter().any(|row| row["id"] == message_id) {
            return Ok(());
        }
        let client = self.clone();
        let session = id.to_string();
        let body = json!({"id":message_id,"command":command});
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(client.session(&session, "/shell", "POST", Some(body)));
        });
        let started = std::time::Instant::now();
        loop {
            if let Ok(result) = rx.try_recv() {
                return result.map(|_| ());
            }
            let page = self.session(id, "/message?limit=50&order=desc", "GET", None)?;
            if page["data"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == message_id))
            {
                return Ok(());
            }
            if started.elapsed() > Duration::from_secs(10) {
                return Err(error(
                    "OpenCode has not confirmed the shell command. Check its terminal before retrying.",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

pub(crate) fn session_id(session: &Value) -> Result<String, DomainStateError> {
    crate::server::read_runtime_text(session, "agentSessionId")
        .filter(|id| safe_id(id)).ok_or_else(|| error("Open an OpenCode conversation in the terminal first so Ghostex can connect its chat."))
}
