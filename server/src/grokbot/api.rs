//! `/api/grokbot` `{op, …}`: `status`, `connect` `{url, key}`, `disconnect`, `test` and
//! `notify` `{text, kind?, from?}`. No answer ever carries the key.

use serde_json::{json, Map, Value};

use super::delivery::{self, files_lock, now_ms, MAX_QUEUE};
use super::store::{self, Connection, QueuedNote};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::security::Principal;

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

/// A session's global reference (`S90:P3lv0:G9jt7`): three parts of letters, digits, `-` and
/// `_`, so a reference can never carry shell syntax into the command the bot runs with it.
fn is_global_ref(value: &str) -> bool {
    let parts = value.split(':').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 64
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        })
}

/// CDXC:GrokBot 2026-11-20 WHY: `connect`, `test` and `disconnect` change or use where the key
/// goes, so only this computer's own callers make them: the desktop app, the token file (the
/// user's terminal, and the Grok Bot desktop app's commands) and this computer's agent sessions.
/// The `full_local` label in protocol.rs doesn't keep paired phones or other computers out (every
/// request reaches the router as a local one), so the caller is checked here.
fn require_this_computer(caller: &Principal) -> Result<(), DomainStateError> {
    match caller {
        Principal::Owner { .. } | Principal::LocalAgent | Principal::Session { .. } => Ok(()),
        _ => Err(DomainStateError {
            code: "forbidden",
            message: "Connect, test and disconnect Grok Bot on the computer itself.".to_string(),
        }),
    }
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
    delivery::note_post();
    // Notes queued before (or under an older key) go out now.
    delivery::kick_and_wake(paths);
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
    if let Some(wait) = delivery::wait_before_next_post() {
        return Err(DomainStateError::bad_request(format!(
            "Ghostex sends Grok Bot at most one call every 20 seconds. Try again in {} seconds.",
            wait.as_secs().max(1)
        )));
    }
    let note = test_note("Test from Ghostex. Reply here to check the bot can reach Ghostex too.");
    delivery::note_post();
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

fn notify(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
    caller: &Principal,
    server_id: &str,
) -> Result<Value, DomainStateError> {
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
    // An agent's own session token names the sender; anyone else's ref is kept only in the
    // global reference shape, since the bot runs `ghostex agents send <ref> …` with it.
    let reply_to = match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => Some(format!("{server_id}:{project_id}:{session_id}")),
        _ => from.and_then(|from| text(from, "ref")),
    }
    .filter(|value| is_global_ref(value));
    let note = QueuedNote {
        at: now_ms(),
        kind,
        text: body,
        project: field("project"),
        agent: field("agent"),
        reply_to,
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
    caller: &Principal,
    server_id: &str,
) -> Result<Value, DomainStateError> {
    let op = text(params, "op").unwrap_or_else(|| "status".to_string());
    if matches!(op.as_str(), "connect" | "disconnect" | "test") {
        require_this_computer(caller)?;
    }
    match op.as_str() {
        "status" => Ok(status(paths)),
        "connect" => connect(paths, params),
        "disconnect" => disconnect(paths),
        "test" => test(paths),
        "notify" => notify(paths, params, caller, server_id),
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
