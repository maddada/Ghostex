//! Startup-failure tracking for Native (`acp-host`) sessions.
//!
//! When `ghostex acp-host` fails before opening `/api/agentChatChannel` (for example, Cursor or
//! Grok not signed in, missing binary, or initialize/session-open failure), the host posts its
//! error via `/api/agentChatChannel` (`action: "startupError"`) before exiting. Option detection
//! surfaces it as the chat's existing notice instead of leaving the chat stuck at `"starting"`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource, SESSION_CHAT_NOTICE_ACTION_RETRY_START,
    SESSION_CHAT_NOTICE_AGENT_EXITED, SESSION_CHAT_NOTICE_LOGIN_EXPIRED, START_FAILURE_DETAIL,
};

#[derive(Clone, Debug)]
pub(crate) struct AcpStartupError {
    pub family: String,
    pub message: String,
    pub is_auth: bool,
}

static STARTUP_ERRORS: LazyLock<Mutex<HashMap<(String, String), AcpStartupError>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn record(
    project_id: &str,
    session_id: &str,
    family: &str,
    message: &str,
    is_auth: bool,
) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut map) = STARTUP_ERRORS.lock() {
        if map.len() >= 512 {
            map.clear();
        }
        map.insert(
            (project_id.to_string(), session_id.to_string()),
            AcpStartupError {
                family: family.trim().to_string(),
                message: trimmed.to_string(),
                is_auth,
            },
        );
    }
}

pub(crate) fn get(project_id: &str, session_id: &str) -> Option<AcpStartupError> {
    STARTUP_ERRORS.lock().ok().and_then(|map| {
        map.get(&(project_id.to_string(), session_id.to_string()))
            .cloned()
    })
}

pub(crate) fn clear(project_id: &str, session_id: &str) {
    if let Ok(mut map) = STARTUP_ERRORS.lock() {
        map.remove(&(project_id.to_string(), session_id.to_string()));
    }
}

/// The first non-empty line of a startup error as the `piChatChannel.startupError` log line
/// carries it: the home folder shown as `~`, words that look like a credential hidden, and cut to
/// a sentence or two (`displayable_reason`).
///
/// CDXC:AgentProviders 2026-11-27 WHY:
/// GitHub #782: a Codex Native chat on Linux stayed at "Waiting to send" and the log said only
/// `piChatChannel.startupError family=codex isAuth=false`, so nobody could tell from a report why
/// Codex would not start. The log now carries what the agent said first, without its secrets or
/// the user's home folder.
pub(crate) fn first_line_for_log(message: &str) -> String {
    let line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let home = crate::resume_lookup::home_dir();
    let home = home.to_string_lossy();
    let line = if home.len() > 1 {
        line.replace(home.as_ref(), "~")
    } else {
        line.to_string()
    };
    crate::session_chat_send_failure::displayable_reason(&line)
}

/// The failed message's reason for a Native agent that could not start: its name and the first
/// line of its error.
pub(crate) fn failed_start_reason(err: &AcpStartupError) -> String {
    let label = agent_label(&err.family);
    let first = first_line_for_log(&err.message);
    let first = first.trim_end_matches('.');
    if first.is_empty() {
        format!("{label} couldn't start, so the message was not sent.")
    } else {
        format!("{label} couldn't start, so the message was not sent: {first}.")
    }
}

pub(crate) fn looks_like_auth_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("authentication required")
        || lower.contains("not authenticated")
        || lower.contains("authentication_failed")
        || lower.contains("unauthorized")
        || lower.contains("http 401")
        || lower.contains("sign in")
        || lower.contains("not logged in")
        || lower.contains("login expired")
        || lower.contains("run 'agent login'")
        || lower.contains("run 'cursor-agent login'")
        || lower.contains("invalid api key")
        || lower.contains("missing api key")
        || lower.contains("no api key")
        || lower.contains("has no stored credentials")
}

/// The first agy release that offers every model to every ACP client (`agy acp`).
const ANTIGRAVITY_OPEN_MODELS: [u64; 3] = [1, 5, 1];
/// What the refusal of an older agy in a Native chat ends with; `startup_error_notice` shows it as
/// the card's text, with Switch to Terminal and Try again.
const ANTIGRAVITY_TOO_OLD_TAIL: &str = "Or switch this chat to Terminal.";

/// Why a Native Antigravity chat won't run on the agy that answered `initialize`, or `None`.
///
/// CDXC:AgentProviders 2026-11-26 DECISION:
/// User, on the Antigravity compliance page (Q5149, "Refuse Native"): agy 1.5.0 and older offer
/// Claude and GPT over ACP only to allowlisted editors, and Ghostex never poses as one, so a
/// Native chat refuses those versions before `session/new` and asks for 1.5.1 or newer; Terminal
/// chats run agy's own screen and still work.
/// CDXC:AgentProviders 2026-11-27 DECISION:
/// User (Q5211, "Refuse"), reversing Q5149's let-through: an unknown or unreadable agy version
/// gets the "update Antigravity to 1.5.1 or newer" message and does not start, in Native chats
/// and at sign-in. No `agentInfo`, a version that isn't a string, and one with no
/// `major.minor` in it are all refused.
/// CDXC:AgentProviders 2026-11-27: the card says to update Antigravity to 1.5.1 or newer and try
/// again, in the same words as setup's sign-in check (`antigravity_update_needed`).
pub(crate) fn antigravity_too_old(initialize: &serde_json::Value) -> Option<String> {
    let version = initialize
        .pointer("/agentInfo/version")
        .and_then(serde_json::Value::as_str);
    antigravity_update_needed(version)
        .map(|message| format!("{message} {ANTIGRAVITY_TOO_OLD_TAIL}"))
}

/// The update instruction for an agy older than 1.5.1 or whose version can't be read, or `None`
/// for 1.5.1 and newer. `text` is anything that names the version: ACP's `agentInfo.version`
/// (`1.5.0`, `v1.5.0`, `agy_acp_server_1.5.0`) or what `agy --version` prints (`agy 1.5.0`,
/// `Antigravity 1.5.0`); `None` when there was nothing to read.
pub(crate) fn antigravity_update_needed(text: Option<&str>) -> Option<String> {
    let Some(version) = text.and_then(AgyVersion::read) else {
        return Some(
            "Ghostex couldn't read which Antigravity CLI version this computer has, and it needs 1.5.1 or newer. Update Antigravity to 1.5.1 or newer (Settings > Updates), then try again.".to_string(),
        );
    };
    version.too_old().then(|| {
        format!(
            "This computer has Antigravity CLI {}, and Ghostex needs 1.5.1 or newer. Update Antigravity to 1.5.1 or newer (Settings > Updates), then try again.",
            version.shown
        )
    })
}

/// An agy version as read from a text.
#[derive(Debug, PartialEq, Eq)]
struct AgyVersion {
    /// `major.minor.patch`, the patch 0 when the text names only `major.minor`.
    numbers: [u64; 3],
    /// A pre-release (`1.5.1-rc1`), which comes before the release of the same numbers.
    prerelease: bool,
    /// The version as the text wrote it, for the card.
    shown: String,
}

impl AgyVersion {
    /// The oldest `digits.digits[.digits]` anywhere in `text`, wherever it stands: `1.5.0`,
    /// `v1.5.0`, `V1.5.0`, `agy 1.5.0`, `agy-1.5.0`, `agy_acp_server_v1.5.0` and `1.5` all read.
    ///
    /// CDXC:AgentProviders 2026-11-27: the Fable review found the gate let `agy 1.5.0`, `V1.5.0`,
    /// `agy-1.5.0` and `1.5` through, because the parser only knew three prefixes and three
    /// parts. Reading the first version anywhere closes that, and the sign-in check uses the same
    /// reader on `agy --version`. The re-review found the first number pair let
    /// `macos-15.1 agy 1.5.0` and `2026.11 agy 1.5.0` through, and only a `-` marked a
    /// pre-release, so `1.5.1rc1`, `1.5.1_beta` and `1.5.1~rc1` read as the release: every
    /// version in the text is read and the oldest one counts, and a letter, `_` or `~` right after
    /// the numbers marks a pre-release too.
    fn read(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let digits_from = |at: usize| -> usize {
            bytes[at..]
                .iter()
                .position(|b| !b.is_ascii_digit())
                .map_or(bytes.len(), |n| at + n)
        };
        let mut oldest: Option<Self> = None;
        let mut at = 0;
        while at < bytes.len() {
            if !bytes[at].is_ascii_digit() || (at > 0 && bytes[at - 1].is_ascii_digit()) {
                at += 1;
                continue;
            }
            let major_end = digits_from(at);
            let minor_end = (bytes.get(major_end) == Some(&b'.')
                && bytes.get(major_end + 1).is_some_and(u8::is_ascii_digit))
            .then(|| digits_from(major_end + 1));
            let Some(minor_end) = minor_end else {
                at = major_end;
                continue;
            };
            let patch_end = (bytes.get(minor_end) == Some(&b'.')
                && bytes.get(minor_end + 1).is_some_and(u8::is_ascii_digit))
            .then(|| digits_from(minor_end + 1));
            let end = patch_end.unwrap_or(minor_end);
            let number = |from: usize, to: usize| text[from..to].parse::<u64>().ok();
            let numbers = [
                number(at, major_end),
                number(major_end + 1, minor_end),
                match patch_end {
                    Some(patch_end) => number(minor_end + 1, patch_end),
                    None => Some(0),
                },
            ];
            // A number too long to hold is no version Ghostex can compare: unreadable.
            let [Some(major), Some(minor), Some(patch)] = numbers else {
                return None;
            };
            // `-rc1`, `_beta` and `~rc1` name a pre-release, as does a letter right after the
            // numbers (`1.5.1rc1`).
            let suffix_from = match bytes.get(end) {
                Some(b'-' | b'_' | b'~')
                    if bytes.get(end + 1).is_some_and(u8::is_ascii_alphanumeric) =>
                {
                    Some(end + 1)
                }
                Some(b) if b.is_ascii_alphabetic() => Some(end),
                _ => None,
            };
            let shown_end = match suffix_from {
                Some(from) => text[from..]
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
                    .map_or(text.len(), |n| from + n),
                None => end,
            };
            let version = Self {
                numbers: [major, minor, patch],
                prerelease: suffix_from.is_some(),
                shown: text[at..shown_end].to_string(),
            };
            if oldest
                .as_ref()
                .is_none_or(|oldest| version.order() < oldest.order())
            {
                oldest = Some(version);
            }
            at = shown_end.max(end);
        }
        oldest
    }

    /// A pre-release comes before the release of the same numbers.
    fn order(&self) -> ([u64; 3], bool) {
        (self.numbers, !self.prerelease)
    }

    fn too_old(&self) -> bool {
        self.numbers < ANTIGRAVITY_OPEN_MODELS
            || (self.numbers == ANTIGRAVITY_OPEN_MODELS && self.prerelease)
    }
}

pub(crate) fn agent_label(family: &str) -> &'static str {
    match family {
        "codex" => "Codex",
        "antigravity" => "Antigravity",
        "claude" => "Claude Code",
        "opencode" => "OpenCode",
        "grok" => "Grok Build",
        "cursor" => "Cursor CLI",
        "pi" => "Pi Agent",
        "omp" => "OMP",
        "hermes" => "Hermes Agent",
        _ => "Agent",
    }
}

pub(crate) fn startup_error_notice(err: &AcpStartupError) -> SessionChatTerminalNotice {
    let label = agent_label(&err.family);
    let retry_action = SessionChatTerminalNoticeAction {
        id: SESSION_CHAT_NOTICE_ACTION_RETRY_START.to_string(),
        label: "Try again".to_string(),
        kind: SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    };
    if err.family == "antigravity" && err.message.ends_with(ANTIGRAVITY_TOO_OLD_TAIL) {
        return SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_AGENT_EXITED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Watchdog,
            "Antigravity needs an update".to_string(),
        )
        .with_detail(err.message.clone())
        .with_actions(vec![
            SessionChatTerminalNoticeAction::switch_to_terminal("Switch to Terminal"),
            retry_action,
        ])
        .with_input_blocking(true);
    }
    if err.is_auth {
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Watchdog,
            format!("Sign in to {label} again"),
        )
        .with_detail(format!(
            "{label}'s login expired. Sign in again, or switch this chat to another account."
        ))
        .with_details(Some(err.message.clone()))
        .with_actions(vec![
            retry_action,
            SessionChatTerminalNoticeAction::switch_to_terminal("Switch to Terminal"),
        ])
        .with_input_blocking(true)
    } else {
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_AGENT_EXITED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Watchdog,
            format!("{label} couldn't start"),
        )
        .with_detail(START_FAILURE_DETAIL)
        .with_details(Some(err.message.clone()))
        .with_actions(vec![
            retry_action,
            SessionChatTerminalNoticeAction::switch_to_terminal("Open Terminal"),
        ])
        .with_input_blocking(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(text: &str) -> bool {
        antigravity_update_needed(Some(text)).is_some()
    }

    #[test]
    fn agy_older_than_1_5_1_is_refused_however_the_version_is_written() {
        for text in [
            "1.5.0",
            "v1.5.0",
            "V1.5.0",
            "agy 1.5.0",
            "Antigravity 1.5.0",
            "agy-1.5.0",
            "agy_acp_server_1.5.0",
            "agy_acp_server_v1.5.0",
            "1.5",
            "1.4",
            "1.5.beta",
            "1.5.0-rc1",
            "1.5.1-rc1",
            "0.9.9",
            "agy version 1.4.12 (darwin-arm64)",
            "macos-15.1 agy 1.5.0",
            "2026.11 agy 1.5.0",
            "agy 1.5.0 (macos 15.1)",
            "agy 2.0.0, acp server 1.5.0",
            "1.5.1rc1",
            "1.5.1_beta",
            "1.5.1~rc1",
            "agy 1.5.1beta2",
        ] {
            assert!(refused(text), "{text} should be refused");
        }
    }

    #[test]
    fn agy_1_5_1_and_newer_is_allowed() {
        for text in [
            "1.5.1",
            "v1.5.1",
            "agy 1.5.1",
            "agy_acp_server_1.5.2",
            "1.5.10",
            "1.6",
            "2.0.0",
            "1.5.2-rc1",
            "1.5.1+build.7",
            "agy 1.5.13\n",
            "macos-15.1 agy 1.5.1",
            "2026.11 agy 1.5.2",
            "agy 1.5.1 (go1.22.3, darwin-arm64)",
            "1.5.2rc1",
            "1.5.2_beta",
        ] {
            assert!(!refused(text), "{text} should be allowed");
        }
    }

    #[test]
    fn the_card_names_the_version_as_written() {
        let message = antigravity_update_needed(Some("agy 1.5.0-rc1 (darwin)")).unwrap();
        assert!(message.contains("Antigravity CLI 1.5.0-rc1,"), "{message}");
        let message = antigravity_update_needed(Some("1.4")).unwrap();
        assert!(message.contains("Antigravity CLI 1.4,"), "{message}");
        let message = antigravity_update_needed(Some("macos-15.1 agy 1.5.0")).unwrap();
        assert!(message.contains("Antigravity CLI 1.5.0,"), "{message}");
        let message = antigravity_update_needed(Some("agy 1.5.1~rc1")).unwrap();
        assert!(message.contains("Antigravity CLI 1.5.1~rc1,"), "{message}");
    }

    #[test]
    fn an_unreadable_version_is_refused() {
        for text in ["", "agy", "dev", "1", "v2", "build 7"] {
            assert_eq!(AgyVersion::read(text), None, "{text}");
            assert!(refused(text), "{text} should be refused");
        }
        assert!(antigravity_update_needed(None).is_some());
    }

    #[test]
    fn the_native_gate_refuses_an_initialize_without_a_readable_version() {
        use serde_json::json;
        for initialize in [
            json!({}),
            json!({ "agentInfo": { "name": "agy" } }),
            json!({ "agentInfo": { "version": 151 } }),
            json!({ "agentInfo": { "version": "dev" } }),
            json!({ "agentInfo": { "version": "agy 1.5.0" } }),
        ] {
            let refusal = antigravity_too_old(&initialize);
            assert!(
                refusal
                    .as_deref()
                    .is_some_and(|text| text.ends_with(ANTIGRAVITY_TOO_OLD_TAIL)),
                "{initialize}"
            );
        }
        assert_eq!(
            antigravity_too_old(&json!({ "agentInfo": { "version": "1.5.1" } })),
            None
        );
    }
}
