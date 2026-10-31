//! CDXC:SessionChat 2026-09-25 DECISION:
//! User: OpenCode chat should match Claude Code support in the GPUI chat with the Rust engine.
//! CDXC:AgentProviders 2026-09-25 WHY:
//! OpenCode owns mutable messages and session controls in its shared service, not a JSONL transcript or a per-terminal agent process. Read its authenticated service registration and public API; never infer conversation identity from another client's server process.

use crate::domain::DomainStateError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::{fs, io::Cursor, path::PathBuf, time::Duration};

/// CDXC:Clipboard 2026-11-01 DECISION: User chose to shrink OpenCode images at send to about 7.5 MB raw, after the shared upload accepts up to 32 MiB.
pub(crate) const OPENCODE_IMAGE_TARGET_BYTES: usize = 7_500_000;

pub(crate) fn opencode_image_bytes(
    bytes: Vec<u8>,
) -> Result<(Vec<u8>, &'static str), DomainStateError> {
    if bytes.len() <= OPENCODE_IMAGE_TARGET_BYTES {
        return Ok((bytes, "original"));
    }
    let mut image = image::load_from_memory(&bytes)
        .map_err(|_| error("The attached image could not be decoded for OpenCode."))?;
    let longest = image.width().max(image.height());
    if longest > 6000 {
        let width = (u64::from(image.width()) * 6000 / u64::from(longest)).max(1) as u32;
        let height = (u64::from(image.height()) * 6000 / u64::from(longest)).max(1) as u32;
        image = image.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    }
    let mut output = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut output), image::ImageFormat::Png)
        .map_err(|_| error("The attached image could not be reduced for OpenCode."))?;
    if output.len() <= OPENCODE_IMAGE_TARGET_BYTES {
        return Ok((output, "png"));
    }
    let rgb = image.to_rgb8();
    for quality in [85, 70, 55, 40] {
        output.clear();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
        encoder
            .encode(
                &rgb,
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|_| error("The attached image could not be reduced for OpenCode."))?;
        if output.len() <= OPENCODE_IMAGE_TARGET_BYTES {
            return Ok((output, "jpg"));
        }
    }
    Err(error(
        "The attached image could not be reduced below OpenCode's 7.5 MB limit.",
    ))
}

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
            .ok_or_else(|| error("OpenCode's background service is unavailable. Start OpenCode and open a session."))?;
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
            agent: crate::http_agent::config()
                .timeout_global(Some(Duration::from_secs(5)))
                .max_redirects(0)
                .build()
                .into(),
        })
    }

    pub(crate) fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, DomainStateError> {
        let request = ureq::http::Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.url))
            .header("Authorization", &self.authorization);
        let long = method == "POST" && path.ends_with("/shell");
        let response = match body {
            Some(body) => self.run(
                request
                    .header("Content-Type", "application/json")
                    .body(body.to_string()),
                long,
            ),
            None => self.run(request.body(()), long),
        }?;
        if response.status() == 204 {
            return Ok(Value::Null);
        }
        crate::http_agent::read_json(response)
            .map_err(|_| error("OpenCode returned an invalid response."))
    }

    /// Sends a request built with a method chosen at run time; `long` lifts the time limit to an hour.
    fn run(
        &self,
        request: Result<ureq::http::Request<impl ureq::AsSendBody>, ureq::http::Error>,
        long: bool,
    ) -> Result<ureq::http::Response<ureq::Body>, DomainStateError> {
        let request = request.map_err(|_| error("Invalid OpenCode request."))?;
        let mut config = self.agent.configure_request(request);
        if long {
            config = config.timeout_global(Some(Duration::from_secs(3600)));
        }
        self.agent
            .run(config.build())
            .map_err(|failure| match failure {
                ureq::Error::StatusCode(status) => {
                    error(format!("OpenCode rejected the request (HTTP {status})."))
                }
                _ => error("Could not reach the OpenCode service."),
            })
    }

    /// A request answered for `directory` (`x-opencode-directory`): the service's shells and MCP
    /// servers are per project folder, and without it the service uses its own folder's (a
    /// `DELETE /api/shell/<id>` of another folder's shell then answers 204 and stops nothing).
    pub(crate) fn request_in(
        &self,
        method: &str,
        path: &str,
        directory: &str,
    ) -> Result<Value, DomainStateError> {
        let response = self.run(
            ureq::http::Request::builder()
                .method(method)
                .uri(format!("{}{path}", self.url))
                .header("Authorization", &self.authorization)
                .header("x-opencode-directory", directory)
                .body(()),
            false,
        )?;
        if response.status() == 204 {
            return Ok(Value::Null);
        }
        crate::http_agent::read_json(response)
            .map_err(|_| error("OpenCode returned an invalid response."))
    }

    /// `/api/event` stays open for as long as the service runs, so it gets no read timeout.
    pub(crate) fn event_stream(
        &self,
    ) -> Result<ureq::http::Response<ureq::Body>, DomainStateError> {
        ureq::Agent::from(
            crate::http_agent::config()
                .timeout_connect(Some(Duration::from_secs(5)))
                .max_redirects(0)
                .build(),
        )
        .get(format!("{}/api/event", self.url))
        .header("Authorization", &self.authorization)
        .header("Accept", "text/event-stream")
        .call()
        .map_err(|_| error("Could not reach the OpenCode service."))
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
                "bmp" | "tif" | "tiff" => "image/png",
                _ => {
                    return Err(error(
                        "OpenCode image attachments must be PNG, JPEG, WebP, or GIF.",
                    ));
                }
            };
            let bytes =
                fs::read(path).map_err(|_| error("An attached image could not be read."))?;
            let (bytes, encoding) = if matches!(
                path.extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str(),
                "bmp" | "tif" | "tiff"
            ) {
                let image = image::load_from_memory(&bytes)
                    .map_err(|_| error("The attached image could not be decoded for OpenCode."))?;
                let mut png = Vec::new();
                image
                    .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                    .map_err(|_| {
                        error("The attached image could not be converted for OpenCode.")
                    })?;
                opencode_image_bytes(png)?
            } else {
                opencode_image_bytes(bytes)?
            };
            let mime = match encoding {
                "png" => "image/png",
                "jpg" => "image/jpeg",
                _ => mime,
            };
            let data = STANDARD.encode(bytes);
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
