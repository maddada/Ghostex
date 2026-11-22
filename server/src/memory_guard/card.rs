//! The guard's chat card, and the record a stopped session keeps of its stop.
//!
//! CDXC:MemoryGuard 2026-11-14 DECISION:
//! User (Q3124), on a session the guard stopped: "show it as red dot (blocked) and when i click i see the error inside the session". The stop is kept in the session's `runtimeSettings.memoryGuardStop` (when, the card's words, what was running), so it survives a gxserver restart: while the session sleeps it is the row's blocked state (the red dot and its reason, server/src/presentation/attention_alerts.rs, drawn by every client), and its chat shows the card with Resume (server/src/session_chat_read.rs). The guard's tick clears the record once the session runs again, however it was woken (server/src/server/memory_guard_runtime.rs).
//! Then (Q3190), asked whether that red dot should also play the blocked sound and banner on top of the guard's own notification: "only the guard's alert". The row carries `blockedAlertedBy: memoryGuard`, so clients draw the dot without the blocked rule's sound and banner (packages/gx-core/src/alert_rule.rs `blocked_alerts`), and Notify me when it steps in controls every alert about the stop.

use serde_json::{json, Value};

use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource,
};

use super::words::CardWords;

/// The `runtimeSettings` member that records a stop.
pub(crate) const STOP_RECORD: &str = "memoryGuardStop";

/// Who alerted for a stopped session's blocked state (`blockedAlertedBy`): the guard itself.
pub(crate) const BLOCKED_ALERTED_BY: &str = "memoryGuard";

/// The blocked reason a stopped session's row says on hover and in its banner.
pub(crate) const BLOCKED_REASON: &str =
    "Stopped: this computer was running out of memory. Open it to see what was stopped and resume it.";

/// The card for `words`; a stopped session's card has Resume.
pub(crate) fn card(
    words: &CardWords,
    severity: SessionChatTerminalNoticeSeverity,
    resume: bool,
) -> SessionChatTerminalNotice {
    let actions = if resume {
        vec![SessionChatTerminalNoticeAction {
            id: super::ACTION_RESUME.to_string(),
            label: "Resume".to_string(),
            kind: SessionChatTerminalNoticeActionKind::RestartAgent,
            send: None,
        }]
    } else {
        Vec::new()
    };
    SessionChatTerminalNotice::new(
        super::NOTICE_KIND,
        severity,
        SessionChatTerminalNoticeSource::Watchdog,
        words.title.clone(),
    )
    .with_detail(words.detail.clone())
    .with_details((!words.details.is_empty()).then(|| words.details.clone()))
    .with_actions(actions)
    .with_input_blocking(false)
}

/// What a stopped session keeps: the card's words, the commands named to the agent on Resume, and
/// when (which is also the card's dismissal key, so a closed card stays closed).
pub(crate) fn stop_record(words: &CardWords, parts: &str, at: &str) -> Value {
    json!({
        "at": at,
        "title": words.title,
        "detail": words.detail,
        "details": words.details,
        "parts": parts,
    })
}

fn record(session: &Value) -> Option<&Value> {
    session
        .pointer(&format!("/runtimeSettings/{STOP_RECORD}"))
        .filter(|record| record.is_object())
}

/// The card a stopped session's chat shows, rebuilt from its record.
pub(crate) fn card_from_record(session: &Value) -> Option<SessionChatTerminalNotice> {
    let record = record(session)?;
    let text = |key: &str| record[key].as_str().unwrap_or_default().to_string();
    let words = CardWords {
        title: text("title"),
        detail: text("detail"),
        details: text("details"),
    };
    let mut notice = card(&words, SessionChatTerminalNoticeSeverity::Warning, true);
    notice.detected_at = text("at");
    Some(notice)
}

/// The commands named on the stopped session's card, for its Resume message.
pub(crate) fn stopped_parts(session: &Value) -> String {
    record(session)
        .and_then(|record| record["parts"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// (since, reason) while `session` sleeps after the guard stopped it.
pub(crate) fn blocked_by_memory_guard(session: &Value) -> Option<(String, String)> {
    let at = record(session)?["at"].as_str()?;
    (crate::presentation::effective_lifecycle_state(session) == "sleeping")
        .then(|| (at.to_string(), BLOCKED_REASON.to_string()))
}
