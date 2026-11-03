//! The HTTP requests the provider adapters build and the answers they read. No transport here:
//! gxserver sends them with its HTTP client, the phone with `fetch`.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HttpBody {
    Json(Value),
    /// `application/x-www-form-urlencoded`.
    Form(Vec<(String, String)>),
    Bytes {
        content_type: String,
        bytes: Vec<u8>,
    },
}

/// One request. Header values may hold secrets: never log a request.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<HttpBody>,
    /// How long the call may take, in seconds (long-polls and exec need more than the default).
    pub timeout_seconds: u64,
}

impl HttpRequest {
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout_seconds: 30,
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    pub fn json(mut self, body: Value) -> Self {
        self.body = Some(HttpBody::Json(body));
        self
    }

    pub fn form(mut self, fields: &[(&str, &str)]) -> Self {
        self.body = Some(HttpBody::Form(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ));
        self
    }

    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }
}

/// The answer: status and body text.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// A provider call that did not give what was asked.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderError {
    /// HTTP status; 0 when the request never got an answer.
    pub status: u16,
    /// The provider's error code (`quota_exceeded`, `claim_required`), when it sent one.
    pub code: Option<String>,
    pub message: String,
    pub retryable: bool,
}

impl ProviderError {
    pub fn transport(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            code: None,
            message: message.into(),
            retryable: true,
        }
    }

    pub fn unexpected(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            code: None,
            message: message.into(),
            retryable: false,
        }
    }

    /// The key was refused: connect again.
    pub fn is_auth(&self) -> bool {
        self.status == 401
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// A 2xx body as JSON, or the provider's error. `label` names the provider in messages.
pub fn json_or_error(response: &HttpResponse, label: &str) -> Result<Value, ProviderError> {
    if (200..300).contains(&response.status) {
        if response.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(&response.body)
            .map_err(|_| ProviderError::unexpected(format!("{label} sent an unreadable answer.")));
    }
    Err(error_from_body(response, label))
}

/// Reads `{"error":{"code","message","retryable"}}` (Instabox, Boat's envelope), `{"code","message"}`,
/// or OAuth's `{"error","error_description"}`.
pub fn error_from_body(response: &HttpResponse, label: &str) -> ProviderError {
    let value: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    let error = value
        .get("error")
        .filter(|error| error.is_object())
        .unwrap_or(&value);
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let code = text(error, "code")
        .or_else(|| text(&value, "code"))
        .or_else(|| text(&value, "error"));
    let message = text(error, "message")
        .or_else(|| text(&value, "message"))
        .or_else(|| text(&value, "error_description"))
        .unwrap_or_else(|| format!("{label} answered HTTP {}.", response.status));
    let retryable = error
        .get("retryable")
        .and_then(Value::as_bool)
        .unwrap_or(matches!(response.status, 429 | 500..=599));
    ProviderError {
        status: response.status,
        code,
        message: format!("{label}: {message}"),
        retryable,
    }
}

/// Percent-encodes a query value.
pub fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
