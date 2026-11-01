use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use serde_json::Value;

use crate::{domain::DomainRepository, session_chat_notice::SessionChatTerminalNotice};

struct ClearedNotice {
    instance: String,
    response_at: i64,
}

fn cleared() -> &'static Mutex<HashMap<String, ClearedNotice>> {
    static CLEARED: OnceLock<Mutex<HashMap<String, ClearedNotice>>> = OnceLock::new();
    CLEARED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(project: &str, session: &str) -> String {
    crate::server::session_observer_key(project, session)
}

fn instance(notice: &SessionChatTerminalNotice) -> String {
    format!("{}\n{}", notice.identity(), notice.detected_at)
}

/// Whether a transcript error record is the agent reporting a usage limit: Claude's "You've hit your session limit", "You've reached your weekly limit" and "You're out of usage credits", Codex's "hit your usage limit".
fn transcript_usage_limit_message(message: &str) -> bool {
    let text = message.to_lowercase();
    text.contains("usage limit")
        || text.contains("out of usage credits")
        || ((text.contains("hit your") || text.contains("reached your")) && text.contains("limit"))
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
        .and_then(|entries| {
            entries
                .get(&key(project, session))
                .map(|entry| entry.instance.clone())
        })
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
    if !(crate::accounts::recovery::retryable(notice) || notice.sign_in_error())
        || (notice.is_answerable()
            && notice.kind != crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT)
    {
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
    let Some(stamp) = crate::session_chat_options::TranscriptStamp::read(&path) else {
        return;
    };
    let Some(observed_at) = chrono::DateTime::parse_from_rfc3339(&notice.detected_at)
        .ok()
        .map(|t| t.timestamp_millis())
    else {
        return;
    };
    let detail = notice.detail.as_deref();
    let scan = match cached_scan(&path, &stamp, detail) {
        Some(scan) => scan,
        None => {
            let Ok(text) = crate::session_chat_options::transcript_tail_text(&path) else {
                return;
            };
            let scan = scan_transcript(&text, detail);
            remember_scan(path, stamp, detail, scan);
            scan
        }
    };
    let TranscriptScan {
        response_at,
        mut error_at,
        usage_limit_error_at,
        account_disabled_error_at,
        sign_in_error_at,
    } = scan;
    // CDXC:AgentProviders 2026-09-11 WHY:
    // A limit the transcript records after the account switch is the new login running out, whatever the screen says: the resumed CLI repaints the old limit with other glyphs and wording, so the screen text cannot tell old from new. Only a transcript error written after the switch time lifts the suppression, in memory and in the row, so the notice can show and the switch pass can act on it.
    if notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT {
        lift_switch_suppression_after(repository, &row, project, session, usage_limit_error_at);
    }
    // A disabled account's error repainted after a switch is the previous login's; one the transcript records after the switch is the new login refusing too, so it lifts the suppression the same way a new usage limit does and the recovery pass can move on to the next account.
    if notice.account_disabled() {
        lift_switch_suppression_after(
            repository,
            &row,
            project,
            session,
            account_disabled_error_at,
        );
        error_at = account_disabled_error_at.or(error_at);
    }
    // A sign-in error works the same way: the repaint of the previous login's "Login expired" stays hidden, and only one the transcript records after the switch is the new login failing (see `account_usage_notice_suppressed`).
    if notice.sign_in_error() {
        lift_switch_suppression_after(repository, &row, project, session, sign_in_error_at);
        error_at = sign_in_error_at.or(error_at);
    }
    // The limit chooser's body contains options, not the transcript's error
    // text. Its quota error still anchors progress and lifts switch suppression.
    if notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT {
        error_at = usage_limit_error_at.or(error_at);
    }
    let recovered = response_at.filter(|response| *response > error_at.unwrap_or(observed_at));
    if let Ok(mut entries) = cleared().lock() {
        if let Some(response_at) = recovered {
            entries.insert(
                key(project, session),
                ClearedNotice {
                    instance: instance(notice),
                    response_at,
                },
            );
        } else if error_at.is_some_and(|error| {
            entries
                .get(&key(project, session))
                .is_some_and(|entry| error > entry.response_at)
        }) {
            // CDXC:AgentProviders 2026-09-22 WHY:
            // The screen can report a new limit before its transcript record arrives. Comparing the later record with screen-detection time kept that limit hidden forever; compare it with the response that actually cleared the old error instead.
            entries.remove(&key(project, session));
        }
    }
}

/// Lifts an account switch's suppression, in memory and in the row, when the transcript recorded
/// the error after the switch time: the new login failing, not the previous one's repaint.
fn lift_switch_suppression_after(
    repository: &DomainRepository<'_>,
    row: &Value,
    project: &str,
    session: &str,
    recorded_at: Option<i64>,
) {
    let Some(since) =
        crate::session_chat_notice::account_usage_notice_suppression(project, session)
    else {
        return;
    };
    if !recorded_at.is_some_and(|error| error > since.timestamp_millis()) {
        return;
    }
    crate::session_chat_notice::lift_account_usage_notice_suppression(project, session);
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.remove("accountSuppressedUsageNotice");
    runtime.remove("accountSuppressedUsageNoticeAt");
    let _ = crate::accounts::endpoint::update_session(repository, row, runtime);
}

/// What one transcript tail says about a notice: when the agent last answered, and when it last recorded this error, a usage limit, a disabled account or a sign-in error.
#[derive(Clone, Copy)]
struct TranscriptScan {
    response_at: Option<i64>,
    error_at: Option<i64>,
    usage_limit_error_at: Option<i64>,
    account_disabled_error_at: Option<i64>,
    sign_in_error_at: Option<i64>,
}

/// `detail` is the notice's detail text, which an error record must quote to count as this error.
fn scan_transcript(text: &str, detail: Option<&str>) -> TranscriptScan {
    let mut response_at = None;
    let mut error_at = None;
    let mut usage_limit_error_at = None;
    let mut account_disabled_error_at = None;
    let mut sign_in_error_at = None;
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
        } else if record["type"] == "event_msg" && payload["type"] == "task_complete" {
            // CDXC:AgentProviders 2026-09-17 WHY:
            // Codex also records usage limits in task_complete.error, without a separate error event. Missing that record leaves a fresh limit hidden by the previous account switch or mistaken for an error the agent already recovered from.
            payload["error"]["message"].as_str().map(str::to_owned)
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
        if crate::accounts::disabled::transcript_record_disabled(&record) {
            account_disabled_error_at = Some(timestamp);
        } else if crate::accounts::login_expired::transcript_record_login_expired(&record) {
            sign_in_error_at = Some(timestamp);
        }
        if let Some(message) = error_message {
            if record["type"] == "event_msg" && codex_sign_in_error_message(&message) {
                sign_in_error_at = Some(timestamp);
            }
            if detail.is_some_and(|detail| detail.contains(message.trim()))
                && !message.trim().is_empty()
            {
                error_at = Some(timestamp);
            }
            if transcript_usage_limit_message(&message) {
                usage_limit_error_at = Some(timestamp);
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
    TranscriptScan {
        response_at,
        error_at,
        usage_limit_error_at,
        account_disabled_error_at,
        sign_in_error_at,
    }
}

/// Whether a Codex error event is the sign-in failure its screen shows as "Codex reported a sign-in
/// error" (the same wordings the screen rule matches).
fn codex_sign_in_error_message(message: &str) -> bool {
    let text = message.to_lowercase();
    text.contains("access token could not be refreshed")
        || (text.contains("login expired") && text.contains("/login"))
}

struct CachedScan {
    stamp: crate::session_chat_options::TranscriptStamp,
    detail: Option<String>,
    scan: TranscriptScan,
}

/// More transcripts than any machine shows a notice for at once; past it the cache starts over.
const SCAN_CACHE_LIMIT: usize = 64;

/// CDXC:AgentScreenDetection 2026-10-23 WHY:
/// Every detection that finds a retryable notice (a usage limit, an API error) re-read and re-parsed the last 6 MB of the session's transcript, on every probe for as long as the notice stayed on screen. The scan depends only on the file's bytes and the notice's detail text, so its result is kept per transcript until the file's stamp or the detail changes; what is done with it (lifting a switch suppression, clearing the notice) still runs on every call.
fn scan_cache() -> &'static Mutex<HashMap<std::path::PathBuf, CachedScan>> {
    static SCANS: OnceLock<Mutex<HashMap<std::path::PathBuf, CachedScan>>> = OnceLock::new();
    SCANS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_scan(
    path: &std::path::Path,
    stamp: &crate::session_chat_options::TranscriptStamp,
    detail: Option<&str>,
) -> Option<TranscriptScan> {
    let cache = scan_cache().lock().ok()?;
    let entry = cache.get(path)?;
    (&entry.stamp == stamp && entry.detail.as_deref() == detail).then_some(entry.scan)
}

fn remember_scan(
    path: std::path::PathBuf,
    stamp: crate::session_chat_options::TranscriptStamp,
    detail: Option<&str>,
    scan: TranscriptScan,
) {
    let Ok(mut cache) = scan_cache().lock() else {
        return;
    };
    if cache.len() >= SCAN_CACHE_LIMIT && !cache.contains_key(&path) {
        cache.clear();
    }
    cache.insert(
        path,
        CachedScan {
            stamp,
            detail: detail.map(str::to_string),
            scan,
        },
    );
}
