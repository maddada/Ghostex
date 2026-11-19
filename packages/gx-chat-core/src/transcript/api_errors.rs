//! The error card a failed API call becomes, and the automatic retry it shows.
//!
//! CDXC:SessionChat 2026-10-15 DECISION:
//! User: "make errors like this one have a nice design please" (the plain "API Error: Can't reach the API server … (ENOTFOUND)" line a Claude turn ended with) and "i noticed you didn't retry when i got this error please fix! should've retried!". A connection or server failure the agent recorded is drawn as a compact card: an icon, a short human title, a plain sentence under it with the agent's own words behind Details (`kind_body`, 2026-11-03), and the retry gxserver runs for it (waiting for the network, the countdown, the hourly tries after the quick ones, stopped). Retry now and Stop retrying sit on the card. Once anything follows the failed row (the continuation, or the user's own message) the card collapses to one quiet line.
//! SEE-ALSO: server/src/accounts/api_errors.rs (detection, the quick tries and `apiRetry`), apps/desktop/src/app/native_chat/api_error_card.rs and apps/mobile/app/src/chat/native/cards/ApiErrorCard.tsx (the two renderers).

use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::state::{ChatContext, ChatState};

/// The code Claude records on a failed call that a later try can get past: no connection, a
/// dropped stream, a timeout, a 5xx or an overloaded API. Limits, logins, refusals and bad models
/// carry other codes and keep their own cards.
pub const TRANSIENT_API_ERROR_CODE: &str = "server_error";

/// The quick tries gxserver makes before it falls back to its slow schedule; the card counts them.
const QUICK_TRIES: u64 = 5;

/// Which failure a message describes, from the agent's own words, as `(kind, title)`.
pub fn classify_api_error(text: &str) -> (&'static str, &'static str) {
    let lower = text.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    if has(&["econnrefused", "connection refused"]) {
        ("refused", "Connection refused")
    } else if has(&[
        "enotfound",
        "can't reach the api server",
        "can\u{2019}t reach the api server",
        "unable to connect to api",
        "getaddrinfo",
        "error sending request",
        "connection failed",
        "waiting for network",
    ]) {
        ("offline", "Can't reach the API server")
    } else if has(&[
        "server error",
        "internal server error",
        "exceeded retry limit",
    ]) {
        ("server", "The API had a server error")
    } else if has(&["stalled", "stopped arriving", "timed out", "etimedout"]) {
        ("stalled", "The response stalled")
    } else if has(&[
        "mid-response",
        "econnreset",
        "stream disconnected",
        "error while reading the server response",
        "connection error",
        "socket hang up",
    ]) {
        ("lost", "Connection lost")
    } else if has(&["529", "overloaded", "high demand", "at capacity"]) {
        ("overloaded", "The API is overloaded")
    } else {
        ("server", "The API had a server error")
    }
}

/// What the card says about a failure kind, in plain words; the agent's own line goes behind the
/// card's Details toggle.
/// CDXC:SessionChat 2026-11-03 DECISION: User (Q1118): one plain sentence per kind replaces the agent's raw line, which moves behind Details, and "pls show the buttons on separate row from retrying in 12s... line", so the retry line sits in the card's body and Retry now / Stop retrying get the footer to themselves.
/// SEE-ALSO: apps/desktop/src/app/native_chat/api_error_card.rs, apps/mobile/app/src/chat/native/cards/ApiErrorCard.tsx.
fn kind_body(kind: &str) -> &'static str {
    match kind {
        "offline" => "Check your internet connection.",
        "refused" => {
            "Something on this computer, like a firewall or proxy, blocked the connection."
        }
        "lost" => "The connection dropped before the reply finished.",
        "stalled" => "The reply stopped arriving.",
        "overloaded" => "The service is busy right now.",
        _ => "The service had a problem on its side.",
    }
}

/// The glyph a failure kind is drawn with (a `titlebar/<icon>.svg` on the desktop; the phone maps
/// the same names).
fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "offline" | "refused" | "lost" => "wifi-off",
        "stalled" => "clock",
        _ => "server",
    }
}

fn message_text(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn is_transient(message: &ChatMessage) -> bool {
    message.role == ChatRole::Assistant
        && message.api_error.as_deref() == Some(TRANSIENT_API_ERROR_CODE)
}

/// The static part of a failed-call row, attached to its projected message as `apiError`, or
/// `null` for every other row.
pub fn api_error_presentation(message: &ChatMessage) -> Value {
    if !is_transient(message) {
        return Value::Null;
    }
    let detail = message_text(message);
    let (kind, title) = classify_api_error(&detail);
    json!({ "kind": kind, "title": title, "detail": detail, "icon": kind_icon(kind) })
}

/// The failed-call row nothing has answered yet: the newest user or agent row is the failure
/// itself. A continuation, a message the user sent, or a reply closes it.
pub(crate) fn open_failure(messages: &[ChatRow]) -> Option<&ChatMessage> {
    messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, ChatRole::User | ChatRole::Assistant))
        .filter(|message| is_transient(message))
        .map(|message| &**message)
}

fn clock(ms: i64, context: &ChatContext) -> String {
    crate::transcript::message_time::clock_label(ms, context)
}

fn command(operation: &str) -> Value {
    json!({ "type": "accounts", "request": { "operation": operation } })
}

fn action(id: &str, label: &str, operation: &str) -> Value {
    json!({ "id": id, "label": label, "command": command(operation) })
}

/// What the card says about the retry, and which buttons it offers.
fn retry_status(retry: Option<&Value>, now_ms: i64, context: &ChatContext) -> (Value, Vec<Value>) {
    let retry_now = action("retryNow", "Retry now", "retryNow");
    let stop = action("stopRetrying", "Stop retrying", "stopRecovery");
    let Some(retry) = retry.filter(|retry| retry.is_object()) else {
        return (Value::Null, vec![retry_now]);
    };
    let attempt = retry["attempt"].as_u64().unwrap_or(0);
    let next_at = retry["nextAttemptAt"].as_i64();
    let last_at = retry["lastAttemptAt"].as_i64();
    let status = |tone: &str, label: String| json!({ "tone": tone, "label": label });
    match retry["status"].as_str().unwrap_or_default() {
        "stopped" => (
            status("muted", "Stopped retrying".to_string()),
            vec![retry_now],
        ),
        "retrying" => (
            status("busy", "Retrying now\u{2026}".to_string()),
            vec![stop],
        ),
        "resumed" => (
            status(
                "done",
                match last_at {
                    Some(at) => format!("Retried at {}", clock(at, context)),
                    None => "Retried".to_string(),
                },
            ),
            vec![retry_now],
        ),
        // Something on the agent's screen holds the next try back; it does not count as a try
        // (CDXC:AgentProviders 2026-10-16 in server/src/accounts/recovery.rs).
        "waiting"
            if retry["blockedBy"]
                .as_str()
                .is_some_and(|block| !block.trim().is_empty()) =>
        {
            let mut value = status(
                "waiting",
                format!(
                    "Waiting for the agent's screen: {}",
                    retry["blockedBy"].as_str().unwrap_or_default().trim()
                ),
            );
            if attempt < QUICK_TRIES {
                value["note"] = json!(format!("Try {} of {QUICK_TRIES}", attempt + 1));
            }
            (value, vec![retry_now, stop])
        }
        "waiting" if retry["waitingForNetwork"] == true => (
            status("waiting", "Waiting for the network\u{2026}".to_string()),
            vec![retry_now, stop],
        ),
        "waiting" if attempt >= QUICK_TRIES => (
            status(
                "waiting",
                match next_at {
                    Some(at) => format!(
                        "Gave up after {QUICK_TRIES} quick tries, next try at {}",
                        clock(at, context)
                    ),
                    None => format!("Gave up after {QUICK_TRIES} quick tries"),
                },
            ),
            vec![retry_now, stop],
        ),
        "waiting" => {
            let mut value = status("waiting", "Retrying now\u{2026}".to_string());
            if let Some(at) = next_at.filter(|at| *at > now_ms) {
                // The renderer counts down to `countdownTo` and says "Retrying in 12s".
                value["label"] = json!("Retrying in");
                value["countdownTo"] = json!(at);
            }
            value["note"] = json!(format!("Try {} of {QUICK_TRIES}", attempt + 1));
            (value, vec![retry_now, stop])
        }
        "needsAttention" => (
            status(
                "muted",
                retry["reason"]
                    .as_str()
                    .filter(|reason| !reason.is_empty())
                    .unwrap_or("Not retrying")
                    .to_string(),
            ),
            vec![retry_now],
        ),
        _ => (Value::Null, vec![retry_now]),
    }
}

/// Whether the error card stands for the session's failure, so the screen's `streamError` notice
/// about the same failure is not drawn as a second card. That holds from the failure until the
/// agent really replies: the screen keeps the old error line while a continuation is running, and
/// gxserver retires the notice itself once a reply arrives.
pub fn card_covers_stream_notice(state: &ChatState) -> bool {
    let messages = &state.messages.composed;
    let unanswered = messages
        .iter()
        .rposition(|message| is_transient(message))
        .is_some_and(|failure| {
            !messages[failure + 1..]
                .iter()
                .any(|message| message.role == ChatRole::Assistant && message.api_error.is_none())
        });
    unanswered
        || state.session.api_retry.value().is_some_and(|retry| {
            matches!(retry["status"].as_str(), Some("waiting" | "retrying"))
                && retry["errorText"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())
        })
}

/// The failed call gxserver is retrying that the agent keeps no record of (Codex shows it only on
/// its screen), as the transcript's own failed-call row, so its card is drawn at the transcript's
/// end like a recorded one instead of over the chat box; its words are gxserver's `errorText`.
/// `None` while a recorded failure stands for it, or nothing is being retried.
///
/// CDXC:SessionChat 2026-11-20 DECISION:
/// User: "the dock/dock bar should be the only one there" (no bars between the dock and the chat box); their coordinator then had the remaining bars moved as the design page proposed, this card into the transcript.
/// SEE-ALSO: session/composition.rs (where it joins the tail), extras/dock_stack.rs.
pub(crate) fn unrecorded_failure(state: &ChatState, transcript: &[ChatRow]) -> Option<ChatMessage> {
    if open_failure(transcript).is_some() {
        return None;
    }
    let retry = state
        .session
        .api_retry
        .value()
        .filter(|retry| matches!(retry["status"].as_str(), Some("waiting" | "retrying")))?;
    let text = retry["errorText"].as_str().unwrap_or_default().trim();
    if text.is_empty() {
        return None;
    }
    Some(ChatMessage {
        id: UNRECORDED_FAILURE_ID.to_string(),
        role: ChatRole::Assistant,
        blocks: vec![ghostex_gx_protocol::ChatBlock::Text {
            text: text.to_string(),
        }],
        async_questions: None,
        timestamp: retry["lastAttemptAt"]
            .as_str()
            .and_then(crate::jstime::parse_iso_millis_utc)
            .map(|at| at as i64),
        source: ghostex_gx_protocol::ChatSource::Hook,
        turn_id: None,
        byte_offset: None,
        queued: false,
        deferred_work: None,
        startup_delivery: None,
        api_error: Some(TRANSIENT_API_ERROR_CODE.to_string()),
        mention_paths: None,
        custom_message: None,
    })
}

/// The id of [`unrecorded_failure`]'s row.
pub const UNRECORDED_FAILURE_ID: &str = "api-error:unrecorded";

/// The open failure's card for the document (`apiErrorCard`), or `null`.
///
/// `messageId` names the row the card replaces: the failed call's own row, or the row
/// [`unrecorded_failure`] adds when the agent keeps no record of it. It is `null` only for a
/// retry the transcript has no row for at all, which the renderers no longer draw.
pub fn api_error_card(state: &ChatState, context: &ChatContext) -> Value {
    let retry = state
        .session
        .api_retry
        .value()
        .filter(|retry| retry.is_object());
    let now_ms = context.now_millis();
    let (message_id, detail) = match open_failure(&state.messages.composed) {
        Some(message) => (json!(message.id), message_text(message)),
        None => {
            // Without a row, only a retry gxserver is still running is worth a card.
            let Some(retry) = retry
                .filter(|retry| matches!(retry["status"].as_str(), Some("waiting" | "retrying")))
            else {
                return Value::Null;
            };
            let text = retry["errorText"].as_str().unwrap_or_default().trim();
            if text.is_empty() {
                return Value::Null;
            }
            (Value::Null, text.to_string())
        }
    };
    let (kind, title) = classify_api_error(&detail);
    // A session On a CLIProxyAPI hub reaches only the hub (server/src/accounts/api_errors.rs `hub`).
    let on_hub = retry.is_some_and(|retry| retry["hub"] == true)
        && matches!(kind, "offline" | "refused" | "lost");
    let title = if on_hub { "Can't reach the hub" } else { title };
    let (mut status, actions) = retry_status(retry, now_ms, context);
    if on_hub && retry.is_some_and(|retry| retry["waitingForNetwork"] == true) {
        status["label"] = json!("Waiting for the hub\u{2026}");
    }
    // Amber while Ghostex is still on it, red once nothing more will happen by itself.
    let tone = match status["tone"].as_str() {
        Some("waiting" | "busy" | "done") => "warning",
        _ => "error",
    };
    // Only while a try is still coming: a stale "Retried at" (`done`) is amber too, but nothing is retrying.
    let said = if on_hub {
        "Check that the CLIProxyAPI hub is running and that this computer can reach it."
    } else {
        kind_body(kind)
    };
    let body = if matches!(status["tone"].as_str(), Some("waiting" | "busy")) {
        format!("{said} Ghostex keeps retrying.")
    } else {
        said.to_string()
    };
    json!({
        "messageId": message_id,
        "kind": kind,
        "icon": kind_icon(kind),
        "tone": tone,
        "title": title,
        "body": body,
        "detail": detail,
        "status": status,
        "actions": actions,
    })
}

#[cfg(test)]
mod tests {
    use super::{classify_api_error, retry_status};
    use crate::state::ChatContext;
    use serde_json::json;

    /// 2026-10-15 18:04 UTC, read in UTC.
    const NOW: i64 = 1_792_087_440_000;

    fn status(retry: serde_json::Value) -> (String, Vec<String>, serde_json::Value) {
        let context = ChatContext {
            now_ms: NOW as f64,
            ..ChatContext::default()
        };
        let (status, actions) = retry_status(Some(&retry), NOW, &context);
        let ids = actions
            .iter()
            .map(|action| action["id"].as_str().unwrap_or_default().to_string())
            .collect();
        (
            status["label"].as_str().unwrap_or_default().to_string(),
            ids,
            status,
        )
    }

    /// Every state the card shows, from the `apiRetry` gxserver publishes.
    #[test]
    fn the_card_says_where_the_retry_stands() {
        let (label, ids, value) =
            status(json!({"status": "waiting", "attempt": 1, "nextAttemptAt": NOW + 12_000}));
        assert_eq!(label, "Retrying in");
        assert_eq!(value["countdownTo"], json!(NOW + 12_000));
        assert_eq!(value["note"], json!("Try 2 of 5"));
        assert_eq!(ids, ["retryNow", "stopRetrying"]);

        let (label, ids, value) = status(
            json!({"status": "waiting", "attempt": 1, "nextAttemptAt": NOW + 5_000, "blockedBy": "Claude Code is waiting for folder trust"}),
        );
        assert_eq!(
            label,
            "Waiting for the agent's screen: Claude Code is waiting for folder trust"
        );
        assert_eq!(
            (value["note"].clone(), value["countdownTo"].clone()),
            (json!("Try 2 of 5"), json!(null))
        );
        assert_eq!(ids, ["retryNow", "stopRetrying"]);

        let (label, ..) = status(json!({"status": "waiting", "waitingForNetwork": true}));
        assert_eq!(label, "Waiting for the network\u{2026}");

        let (label, ..) =
            status(json!({"status": "waiting", "attempt": 5, "nextAttemptAt": NOW + 600_000}));
        assert_eq!(label, "Gave up after 5 quick tries, next try at 6:14 PM");

        let (label, ids, _) = status(json!({"status": "retrying", "attempt": 2}));
        assert_eq!(
            (label.as_str(), ids),
            ("Retrying now\u{2026}", vec!["stopRetrying".to_string()])
        );

        let (label, ..) = status(json!({"status": "resumed", "lastAttemptAt": NOW - 60_000}));
        assert_eq!(label, "Retried at 6:03 PM");

        let (label, ids, _) = status(json!({"status": "stopped"}));
        assert_eq!(
            (label.as_str(), ids),
            ("Stopped retrying", vec!["retryNow".to_string()])
        );
    }

    /// The wordings recorded in real transcripts (Claude 2.1.296 and earlier) and Codex's own.
    #[test]
    fn titles_follow_the_agent_words() {
        let cases = [
            ("API Error: Can't reach the API server \u{2014} check your internet or DNS (ENOTFOUND)", "offline"),
            ("API Error: Unable to connect to API (ENOTFOUND)", "offline"),
            ("API Error: Connection refused \u{2014} a firewall or proxy may be blocking it (ECONNREFUSED)", "refused"),
            ("API Error: Connection lost mid-response. The response above may be incomplete.", "lost"),
            ("API Error: Connection closed mid-response. The response above may be incomplete.", "lost"),
            ("API Error: Unable to connect to API (ECONNRESET)", "offline"),
            ("API Error: Response stalled mid-stream. The response above may be incomplete.", "stalled"),
            ("API Error: The response stopped arriving. The response above may be incomplete.", "stalled"),
            ("API Error: 529 Overloaded. This is a server-side issue, usually temporary", "overloaded"),
            ("API Error: 500 Internal server error. This is a server-side issue", "server"),
            ("API Error: Server error mid-response. The response above may be incomplete.", "server"),
            ("stream disconnected before completion: error sending request", "offline"),
            ("We\u{2019}re currently experiencing high demand, which may cause temporary errors.", "overloaded"),
        ];
        for (text, kind) in cases {
            assert_eq!(classify_api_error(text).0, kind, "{text}");
        }
    }
}
