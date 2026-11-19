//! `/api/grokbot` `{op, …}`: `status`, `connect` `{url, key}`, `disconnect`, `test` and
//! `notify` `{text, kind?, from?}`. No answer ever carries the key.

use serde_json::{json, Map, Value};

use super::delivery::{self, files_lock, now_ms, MAX_QUEUE};
use super::store::{self, Connection, QueuedNote};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

pub(crate) const GROKBOT_ENDPOINT: &str = "/api/grokbot";

const MAX_TEXT_CHARS: usize = 4_000;
const MAX_FIELD_CHARS: usize = 200;
const MAX_KEY_CHARS: usize = 4_096;
const KINDS: &[&str] = &["ask", "report", "done", "blocked", "info"];

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `value` without control characters (newlines and tabs kept when `multiline`), cut to `max`.
fn clean(value: &str, max: usize, multiline: bool) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
        .take(max)
        .collect()
}

/// The webhook address: https, or http to this computer only (a stand-in while testing).
fn checked_url(url: &str) -> Result<String, DomainStateError> {
    let parsed = url::Url::parse(url).map_err(|_| {
        DomainStateError::bad_request(
            "That isn't a web address. Copy POST to from the routine's Webhook section.",
        )
    })?;
    let loopback = matches!(
        parsed.host_str(),
        Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
    );
    match parsed.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => {
            return Err(DomainStateError::bad_request(
                "The webhook address must start with https://.",
            ))
        }
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(DomainStateError::bad_request(
            "The webhook address must not carry a user name or password.",
        ));
    }
    Ok(parsed.to_string())
}

fn status(paths: &GxserverPaths) -> Value {
    let connection = store::read_connection(paths);
    let has_key = store::read_key(paths).is_some();
    let queued = store::read_queue(paths).len();
    match connection {
        Some(connection) if has_key => json!({
            "connected": true,
            "url": connection.url,
            "connectedAt": connection.connected_at,
            "lastSentAt": connection.last_sent_at,
            "lastError": connection.last_error,
            "lastErrorAt": connection.last_error_at,
            "sent": connection.sent,
            "queued": queued,
        }),
        _ => json!({ "connected": false, "queued": queued }),
    }
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError::corrupt_state(format!(
        "Ghostex couldn't save the Grok Bot connection: {error}"
    ))
}

/// Saves the address and key, then sends a test note so a wrong key shows at once.
fn connect(paths: &GxserverPaths, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let url = checked_url(&text(params, "url").ok_or_else(|| {
        DomainStateError::bad_request("url is required: the routine's POST to address.")
    })?)?;
    let key = text(params, "key")
        .ok_or_else(|| DomainStateError::bad_request("key is required: the routine's key."))?;
    if key.len() > MAX_KEY_CHARS || key.chars().any(char::is_control) || key.contains(' ') {
        return Err(DomainStateError::bad_request(
            "That doesn't look like the routine's key. Copy key (not header) from its Webhook section.",
        ));
    }
    let test = test_note("Ghostex is connected. Agents' notes will arrive here.");
    let tested = delivery::post(&url, &key, &delivery::body(&[test]));
    {
        let _guard = files_lock();
        store::write_key(paths, &key).map_err(io_error)?;
        let now = now_ms();
        let connection = Connection {
            url,
            connected_at: now,
            last_sent_at: tested.is_ok().then_some(now),
            last_error: tested.as_ref().err().cloned(),
            last_error_at: tested.is_err().then_some(now),
            sent: u64::from(tested.is_ok()),
        };
        store::write_connection(paths, &connection).map_err(io_error)?;
    }
    let mut result = status(paths);
    result["tested"] = json!(tested.is_ok());
    if let Err(message) = tested {
        result["message"] = json!(format!("Saved, but the test didn't arrive: {message}"));
    }
    Ok(result)
}

fn test_note(text: &str) -> QueuedNote {
    QueuedNote {
        at: now_ms(),
        kind: "test".to_string(),
        text: text.to_string(),
        project: None,
        agent: None,
        reply_to: None,
    }
}

fn test(paths: &GxserverPaths) -> Result<Value, DomainStateError> {
    let (Some(connection), Some(key)) = (store::read_connection(paths), store::read_key(paths))
    else {
        return Err(not_connected());
    };
    let note = test_note("Test from Ghostex. Reply here to check the bot can reach Ghostex too.");
    let outcome = delivery::post(&connection.url, &key, &delivery::body(&[note]));
    let _guard = files_lock();
    if let Some(mut connection) = store::read_connection(paths) {
        let now = now_ms();
        match &outcome {
            Ok(()) => {
                connection.last_sent_at = Some(now);
                connection.sent += 1;
                connection.last_error = None;
                connection.last_error_at = None;
            }
            Err(message) => {
                connection.last_error = Some(message.clone());
                connection.last_error_at = Some(now);
            }
        }
        let _ = store::write_connection(paths, &connection);
    }
    match outcome {
        Ok(()) => Ok(json!({ "sent": true })),
        Err(message) => Err(DomainStateError::bad_request(message)),
    }
}

fn not_connected() -> DomainStateError {
    DomainStateError::bad_request(
        "Grok Bot isn't connected. Run `ghostex grokbot connect` in your own terminal first.",
    )
}

fn notify(paths: &GxserverPaths, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    if store::read_connection(paths).is_none() || store::read_key(paths).is_none() {
        return Err(not_connected());
    }
    let body = text(params, "text")
        .map(|value| clean(&value, MAX_TEXT_CHARS, true))
        .ok_or_else(|| DomainStateError::bad_request("text is required."))?;
    let kind = text(params, "kind").unwrap_or_else(|| "info".to_string());
    if !KINDS.contains(&kind.as_str()) {
        return Err(DomainStateError::bad_request(format!(
            "kind must be one of {}.",
            KINDS.join(", ")
        )));
    }
    let from = params.get("from").and_then(Value::as_object);
    let field = |key: &str| {
        from.and_then(|from| text(from, key))
            .map(|value| clean(&value, MAX_FIELD_CHARS, false))
    };
    let note = QueuedNote {
        at: now_ms(),
        kind,
        text: body,
        project: field("project"),
        agent: field("agent"),
        reply_to: field("ref"),
    };
    let queued = {
        let _guard = files_lock();
        let mut queue = store::read_queue(paths);
        queue.push(note);
        if queue.len() > MAX_QUEUE {
            let extra = queue.len() - MAX_QUEUE;
            queue.drain(..extra);
        }
        store::write_queue(paths, &queue).map_err(io_error)?;
        queue.len()
    };
    delivery::kick(paths);
    Ok(json!({ "queued": queued }))
}

fn disconnect(paths: &GxserverPaths) -> Result<Value, DomainStateError> {
    let _guard = files_lock();
    store::remove_all(paths).map_err(io_error)?;
    Ok(json!({ "connected": false }))
}

/// Runs one `/api/grokbot` call (off the async runtime: `connect` and `test` call Grok Bot).
pub(crate) fn handle(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    match text(params, "op").as_deref().unwrap_or("status") {
        "status" => Ok(status(paths)),
        "connect" => connect(paths, params),
        "disconnect" => disconnect(paths),
        "test" => test(paths),
        "notify" => notify(paths, params),
        other => Err(DomainStateError::bad_request(format!(
            "Unknown op \"{other}\": use status, connect, disconnect, test or notify."
        ))),
    }
}

/// Picks up notes a gxserver restart left waiting.
pub(crate) fn resume_delivery(paths: &GxserverPaths) {
    if !store::read_queue(paths).is_empty() {
        delivery::kick(paths);
    }
}
