use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use serde_json::Value;

use crate::{domain::DomainRepository, session_chat_notice::SessionChatTerminalNotice};

fn cleared() -> &'static Mutex<HashMap<String, String>> {
    static CLEARED: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CLEARED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(project: &str, session: &str) -> String {
    crate::server::session_observer_key(project, session)
}

fn instance(notice: &SessionChatTerminalNotice) -> String {
    format!("{}\n{}", notice.identity(), notice.detected_at)
}

pub(crate) fn has_cleared_error(
    state: &crate::server::AppState,
    project: &str,
    session: &str,
) -> bool {
    state.session_chat_option_cache.lock().is_ok_and(|entries| {
        entries
            .get(&key(project, session))
            .and_then(|entry| entry.value.notice.as_ref())
            .is_some_and(|notice| !visible(project, session, notice))
    })
}

pub(crate) fn visible(project: &str, session: &str, notice: &SessionChatTerminalNotice) -> bool {
    cleared()
        .lock()
        .ok()
        .and_then(|entries| entries.get(&key(project, session)).cloned())
        .as_deref()
        != Some(instance(notice).as_str())
}

/// CDXC:AgentScreenDetection 2026-09-08 DECISION:
/// User: when AI messages arrive in the transcript after an error, that error is old and must no longer be shown.
/// Keep the raw screen notice in the detection cache so repeated captures retain the cleared instance's identity, while UI and recovery consumers receive only the visible notice.
pub(crate) fn refresh(
    repository: &DomainRepository<'_>,
    project: &str,
    session: &str,
    agent: Option<&str>,
    notice: &SessionChatTerminalNotice,
) {
    if !crate::accounts::recovery::retryable(notice) || notice.is_answerable() {
        return;
    }
    let Some(transcript_agent) = crate::session_chat::resolve_session_chat_transcript_agent(agent)
    else {
        return;
    };
    let Ok(Some(row)) = repository.get_session(project, session) else {
        return;
    };
    let Some(path) = crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        row.pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str),
        row.pointer("/runtimeSettings/agentSessionPath")
            .and_then(Value::as_str),
    ) else {
        return;
    };
    let Ok(text) = crate::session_chat_options::transcript_tail_text(&path) else {
        return;
    };
    let Some(observed_at) = chrono::DateTime::parse_from_rfc3339(&notice.detected_at)
        .ok()
        .map(|t| t.timestamp_millis())
    else {
        return;
    };
    let mut response_at = None;
    let mut error_at = None;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record["isSidechain"] == true {
            continue;
        }
        let Some(timestamp) = crate::session_chat::parse_timestamp(record.get("timestamp")) else {
            continue;
        };
        let payload = &record["payload"];
        let error_message = if record["type"] == "event_msg"
            && matches!(payload["type"].as_str(), Some("error" | "stream_error"))
        {
            payload["message"].as_str().map(str::to_owned)
        } else if record["isApiErrorMessage"] == true {
            record["message"]["content"].as_array().map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        } else {
            None
        };
        if let Some(message) = error_message {
            if notice
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(message.trim()))
                && !message.trim().is_empty()
            {
                error_at = Some(timestamp);
            }
        }
        let codex_response = (record["type"] == "response_item"
            && payload["type"] == "message"
            && payload["role"] == "assistant")
            || (record["type"] == "event_msg" && payload["type"] == "agent_message")
            || (record["type"] == "event_msg"
                && payload["type"] == "item_completed"
                && payload["item"]["type"] == "AgentMessage");
        let claude_response = record["type"] == "assistant"
            && record["isApiErrorMessage"] != true
            && record["message"]["model"]
                .as_str()
                .is_some_and(|model| model != "<synthetic>");
        if codex_response || claude_response {
            response_at = Some(timestamp);
        }
    }
    // CDXC:AgentProviders 2026-09-11 WHY:
    // A limit the transcript records again after the account switch is the new login running out, even though the screen wording is identical to the hidden one. Drop the suppression, in memory and in the row, so the notice can show and the switch pass can act on it.
    if notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT {
        if let Some((identity, since)) =
            crate::session_chat_notice::account_usage_notice_suppression(project, session)
        {
            if identity == notice.identity()
                && error_at.is_some_and(|error| error > since.timestamp_millis())
            {
                crate::session_chat_notice::lift_account_usage_notice_suppression(project, session);
                let mut runtime = row["runtimeSettings"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                runtime.remove("accountSuppressedUsageNotice");
                runtime.remove("accountSuppressedUsageNoticeAt");
                let _ = crate::accounts::endpoint::update_session(repository, &row, runtime);
            }
        }
    }
    let recovered = response_at.is_some_and(|response| response > error_at.unwrap_or(observed_at));
    if let Ok(mut entries) = cleared().lock() {
        if recovered {
            entries.insert(key(project, session), instance(notice));
        } else if error_at.is_some_and(|error| error > observed_at) {
            // The same wording can describe a new failed attempt after progress.
            entries.remove(&key(project, session));
        }
    }
}
