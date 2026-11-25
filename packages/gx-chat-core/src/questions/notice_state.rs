//! Which notice the user closed, and how long closing it keeps the same words off the screen.
//!
//! Port of `packages/shared/session-chat-controller/notice-state.ts`. The stored record is
//! unchanged, including the pre-2026-09-03 form that held only the dismissed key.

use serde::{Deserialize, Serialize};

use crate::questions::model::TerminalNotice;

/// How long the same screen-state notice stays hidden after being dismissed, even when it comes
/// back under a new `detectedAt`.
///
/// A usage limit or an expired login lasts far longer than this, and a user who closed the card
/// knows about it; after this long a re-detection is worth mentioning again.
pub const NOTICE_REDISPLAY_COOLDOWN_MS: i64 = 30 * 60 * 1000;

/// The dismissal one session remembers.
///
/// The field order is the stored key order, because this is written straight to client storage.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissedNotice {
    /// Wall-clock millis of the dismissal.
    #[serde(default)]
    pub dismissed_at: i64,
    /// Whether the cooldown applies: only screen-state notices re-detect continuously.
    #[serde(default)]
    pub from_screen: bool,
    /// `kind:title` of the dismissed notice.
    pub identity: String,
    /// Exact detection dismissed: `kind:detectedAt`.
    pub key: String,
}

/// The exact detection a dismissal names.
pub fn notice_dismiss_key(notice: Option<&TerminalNotice>) -> Option<String> {
    notice.map(|notice| format!("{}:{}", notice.kind, notice.detected_at))
}

/// What a notice says, independent of when it was detected.
pub fn notice_identity(notice: &TerminalNotice) -> String {
    format!("{}:{}", notice.kind, notice.title)
}

/// True when `notice` is the detection the user dismissed, or the same screen-state words
/// re-detected within the cooldown. An approval is only ever the one request dismissed.
pub fn is_notice_dismissed(notice: &TerminalNotice, dismissed: Option<&DismissedNotice>) -> bool {
    let Some(dismissed) = dismissed else {
        return false;
    };
    if notice_dismiss_key(Some(notice)).as_deref() == Some(dismissed.key.as_str()) {
        return true;
    }
    if !dismissed.from_screen || notice.source != "screen" {
        return false;
    }
    // CDXC:SessionChat 2026-11-21 WHY: every permission picker has the title "Allow this?" (server/src/session_chat_notice/classify.rs `notice_from_picker`), so the cooldown by words hid every later approval of that session for 30 minutes once one had been closed, and the next approval waited with no card. An approval waits on the user until answered, so only the request closed stays hidden; a later one is a new request and shows its card. The pass-through card's release already keeps to its one detection for the same reason (questions/actions.rs `dismiss_notice`).
    if crate::extras::dock_approvals::approval_kind(notice).is_some() {
        return false;
    }
    if notice_identity(notice) != dismissed.identity {
        return false;
    }
    match parse_iso_millis(&notice.detected_at) {
        // An unreadable stamp cannot be compared, so the cooldown holds rather than flashing the
        // card the user just closed.
        None => true,
        Some(detected_at) => detected_at < dismissed.dismissed_at + NOTICE_REDISPLAY_COOLDOWN_MS,
    }
}

/// The record written when the user closes a notice.
pub fn dismissed_notice_state(notice: &TerminalNotice, now_ms: i64) -> DismissedNotice {
    DismissedNotice {
        dismissed_at: now_ms,
        from_screen: notice.source == "screen",
        identity: notice_identity(notice),
        key: notice_dismiss_key(Some(notice)).unwrap_or_default(),
    }
}

/// Reads a stored dismissal, tolerating the record shape that predates the cooldown.
pub fn decode_dismissed_notice(raw: &str) -> Option<DismissedNotice> {
    if raw.is_empty() {
        return None;
    }
    if !raw.starts_with('{') {
        // Pre-2026-09-03 entries stored the bare key; they still hide that one detection.
        return Some(DismissedNotice {
            dismissed_at: 0,
            from_screen: false,
            identity: String::new(),
            key: raw.to_string(),
        });
    }
    serde_json::from_str(raw).ok()
}

/// `Date.parse` for the ISO-8601 stamps gxserver writes, in epoch milliseconds.
///
/// One rule for the whole crate since 2026-09-22 (`crate::jstime`). The core reads no timezone
/// here, so a stamp without an offset is still taken as UTC; gxserver always writes `Z`.
fn parse_iso_millis(text: &str) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(text).map(|millis| millis as i64)
}
