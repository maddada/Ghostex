use std::{collections::HashMap, time::Duration};

use serde_json::{json, Value};

use super::registration::is_printable_id;
use super::store::RECEIPT_ID_MAX;

const EXPO_ORIGIN: &str = "https://exp.host";
const SEND_PATH: &str = "/--/api/v2/push/send";
const RECEIPTS_PATH: &str = "/--/api/v2/push/getReceipts";
/// The whole request, connect to last byte. A stop waits at most this long for a request in
/// flight, well inside gxserver's 8 s exit deadline.
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_BYTES_MAX: u64 = 256 * 1024;
const RETRY_AFTER_MAX_SECS: u64 = 3600;
const CODE_CHARS_MAX: usize = 48;

/// A refusal as the push service named it: its code (`InvalidCredentials`, `DeviceNotRegistered`, `MessageTooBig`, or
/// gxserver's own `malformedResponse`, `timeout`, `http500` …) and the service's own words for it, if it gave any, on
/// one line with every push token cut out (`crate::logging::cut_push_tokens`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) code: String,
    pub(crate) message: Option<String>,
}

impl Failure {
    pub(crate) fn code(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: None,
        }
    }

    /// `code` with the `message` field of `answer` (a request error, a ticket or a receipt).
    fn of(code: impl Into<String>, answer: &Value) -> Self {
        Self {
            code: code.into(),
            message: answer
                .get("message")
                .and_then(Value::as_str)
                .map(crate::logging::cut_push_tokens)
                .filter(|message| !message.is_empty()),
        }
    }
}

/// What became of one message.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SendOutcome {
    /// Expo took it; the ticket id is where its receipt will be.
    Accepted(String),
    /// The token is dead; delete it.
    DeviceNotRegistered,
    /// Worth another try: rate limited, a server error, or no answer.
    Retry {
        after: Option<Duration>,
        code: Failure,
    },
    /// Refused for good (bad request, missing access token, malformed answer).
    Failed(Failure),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReceiptState {
    Delivered,
    DeviceNotRegistered,
    Failed(Failure),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReceiptsOutcome {
    /// Receipts Expo has; ids it does not list are not ready yet.
    Read(HashMap<String, ReceiptState>),
    Retry(Failure),
    Failed(Failure),
}

/// CDXC:Notifications 2026-10-31 WHY:
/// gxserver sends only to Expo's fixed push origin; no setting, environment variable or caller can point it elsewhere, and only the tests build a client for a local fake. Requests and answers follow Expo's page "Send notifications with the Expo Push Service" (docs.expo.dev/push-notifications/sending-notifications, read 2026-10-31): send at `/--/api/v2/push/send`, receipts at `/--/api/v2/push/getReceipts` with `ids`, `DeviceNotRegistered` on a ticket or receipt for a dead token, `TOO_MANY_REQUESTS` when rate limited, and `UNAUTHORIZED` when enhanced security asks for an access token gxserver does not have, which no retry can fix.
/// A refusal leaves this module as its code and Expo's own words for it (`Failure`), with every push token cut out of those words: Expo's messages can quote the token. An answer that cannot be read counts as a failure and is not retried, since Expo may already have taken the message.
/// CDXC:Notifications 2026-11-11 DECISION: User (push follow-ups, fault 3): log the push service's failure code and message as they are, while still never logging a token; a failed push showed only `[redacted:secret]` and could not be diagnosed. This supersedes "only a short error code leaves this module".
pub(crate) struct ExpoClient {
    origin: String,
    agent: ureq::Agent,
}

impl ExpoClient {
    pub(crate) fn new() -> Self {
        Self::with_origin(EXPO_ORIGIN.to_string())
    }

    #[cfg(test)]
    pub(crate) fn for_test_origin(origin: String) -> Self {
        Self::with_origin(origin)
    }

    fn with_origin(origin: String) -> Self {
        let agent = crate::http_agent::config()
            .timeout_global(Some(HTTP_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Self { origin, agent }
    }

    /// Sends one message (a JSON object) and reads its ticket.
    pub(crate) fn send(&self, message: &str) -> SendOutcome {
        match self.post(SEND_PATH, format!("[{message}]")) {
            Ok(answer) => send_outcome(answer),
            Err(code) => SendOutcome::Retry {
                after: None,
                code: Failure::code(code),
            },
        }
    }

    /// Reads the receipts of up to 1000 ticket ids.
    pub(crate) fn receipts(&self, ids: &[&str]) -> ReceiptsOutcome {
        match self.post(RECEIPTS_PATH, json!({ "ids": ids }).to_string()) {
            Ok(answer) => receipts_outcome(answer),
            Err(code) => ReceiptsOutcome::Retry(Failure::code(code)),
        }
    }

    fn post(&self, path: &str, body: String) -> Result<Answer, String> {
        let mut response = self
            .agent
            .post(format!("{}{path}", self.origin))
            .header("accept", "application/json")
            .content_type("application/json")
            .send(body)
            .map_err(|error| transport_code(&error).to_string())?;
        let status = response.status().as_u16();
        let retry_after = crate::http_agent::header(&response, "retry-after")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|seconds| Duration::from_secs(seconds.min(RETRY_AFTER_MAX_SECS)));
        let body = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_BYTES_MAX)
            .read_to_vec()
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        Ok(Answer {
            status,
            retry_after,
            body,
        })
    }
}

struct Answer {
    status: u16,
    retry_after: Option<Duration>,
    body: Option<Value>,
}

impl Answer {
    /// The first request error Expo named, if any.
    fn error_code(&self) -> Option<Failure> {
        let error = self.body.as_ref()?.get("errors")?.as_array()?.first()?;
        let code = error
            .get("code")
            .and_then(Value::as_str)
            .map(bounded_code)?;
        Some(Failure::of(code, error))
    }

    /// Rate limited, a server error, or refused for good; `None` for a 2xx answer.
    fn refusal(&self) -> Option<Refusal> {
        let code = self.error_code();
        match self.status {
            200..=299 if code.is_none() => None,
            429 => Some(Refusal::Retry(
                code.unwrap_or_else(|| Failure::code("TOO_MANY_REQUESTS")),
            )),
            500..=599 => {
                Some(Refusal::Retry(code.unwrap_or_else(|| {
                    Failure::code(format!("http{}", self.status))
                })))
            }
            401 | 403 => Some(Refusal::Failed(
                code.unwrap_or_else(|| Failure::code("UNAUTHORIZED")),
            )),
            _ => {
                Some(Refusal::Failed(code.unwrap_or_else(|| {
                    Failure::code(format!("http{}", self.status))
                })))
            }
        }
    }
}

enum Refusal {
    Retry(Failure),
    Failed(Failure),
}

fn send_outcome(answer: Answer) -> SendOutcome {
    match answer.refusal() {
        Some(Refusal::Retry(code)) => {
            return SendOutcome::Retry {
                after: answer.retry_after,
                code,
            }
        }
        Some(Refusal::Failed(code)) => return SendOutcome::Failed(code),
        None => {}
    }
    let tickets = answer
        .body
        .as_ref()
        .and_then(|body| body.get("data"))
        .and_then(Value::as_array);
    let Some([ticket]) = tickets.map(Vec::as_slice) else {
        return SendOutcome::Failed(Failure::code("malformedResponse"));
    };
    match ticket.get("status").and_then(Value::as_str) {
        Some("ok") => match ticket.get("id").and_then(Value::as_str) {
            Some(id) if is_printable_id(id, RECEIPT_ID_MAX) => SendOutcome::Accepted(id.into()),
            _ => SendOutcome::Failed(Failure::code("malformedResponse")),
        },
        Some("error") => match detail_code(ticket).as_deref() {
            Some("DeviceNotRegistered") => SendOutcome::DeviceNotRegistered,
            Some("MessageRateExceeded") => SendOutcome::Retry {
                after: answer.retry_after,
                code: Failure::of("MessageRateExceeded", ticket),
            },
            Some(code) => SendOutcome::Failed(Failure::of(code, ticket)),
            None => SendOutcome::Failed(Failure::code("unknown")),
        },
        _ => SendOutcome::Failed(Failure::code("malformedResponse")),
    }
}

fn receipts_outcome(answer: Answer) -> ReceiptsOutcome {
    match answer.refusal() {
        Some(Refusal::Retry(code)) => return ReceiptsOutcome::Retry(code),
        Some(Refusal::Failed(code)) => return ReceiptsOutcome::Failed(code),
        None => {}
    }
    let Some(receipts) = answer
        .body
        .as_ref()
        .and_then(|body| body.get("data"))
        .and_then(Value::as_object)
    else {
        return ReceiptsOutcome::Failed(Failure::code("malformedResponse"));
    };
    let mut states = HashMap::new();
    for (id, receipt) in receipts {
        let state = match receipt.get("status").and_then(Value::as_str) {
            Some("ok") => ReceiptState::Delivered,
            Some("error") => match detail_code(receipt).as_deref() {
                Some("DeviceNotRegistered") => ReceiptState::DeviceNotRegistered,
                Some(code) => ReceiptState::Failed(Failure::of(code, receipt)),
                None => ReceiptState::Failed(Failure::code("unknown")),
            },
            _ => ReceiptState::Failed(Failure::code("malformedResponse")),
        };
        states.insert(id.clone(), state);
    }
    ReceiptsOutcome::Read(states)
}

fn detail_code(ticket: &Value) -> Option<String> {
    ticket
        .get("details")?
        .get("error")?
        .as_str()
        .map(bounded_code)
}

/// A code made only of letters, digits and underscores, short enough for a log line.
fn bounded_code(code: &str) -> String {
    if (1..=CODE_CHARS_MAX).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        code.to_string()
    } else {
        "unknown".to_string()
    }
}

fn transport_code(error: &ureq::Error) -> &'static str {
    match error {
        ureq::Error::Timeout(_) => "timeout",
        error if crate::http_agent::never_connected(error) => "connectFailed",
        _ => "transport",
    }
}
