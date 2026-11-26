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
/// What the refusal of an older agy says; `startup_error_notice` shows it as the card's text.
const ANTIGRAVITY_TOO_OLD_TAIL: &str =
    "Update it in Settings > Updates, or switch this chat to Terminal.";

/// Why a Native Antigravity chat won't run on the agy that answered `initialize`, or `None`.
///
/// CDXC:AgentProviders 2026-11-26 DECISION:
/// User, on the Antigravity compliance page (Q5149, "Refuse Native"): agy 1.5.0 and older offer
/// Claude and GPT over ACP only to allowlisted editors, and Ghostex never poses as one, so a
/// Native chat refuses those versions and asks for 1.5.1 or newer; Terminal chats run agy's own
/// screen and still work. A version Ghostex can't read is let through.
pub(crate) fn antigravity_too_old(initialize: &serde_json::Value) -> Option<String> {
    let version = initialize
        .pointer("/agentInfo/version")
        .and_then(serde_json::Value::as_str)?
        .trim();
    let parsed = parse_agy_version(version)?;
    (parsed < ANTIGRAVITY_OPEN_MODELS).then(|| {
        format!(
            "This computer has Antigravity CLI {}.{}.{}, and Native chats need 1.5.1 or newer. {ANTIGRAVITY_TOO_OLD_TAIL}",
            parsed[0], parsed[1], parsed[2]
        )
    })
}

/// `1.5.0`, `v1.5.0` or `agy_acp_server_1.5.0`.
fn parse_agy_version(version: &str) -> Option<[u64; 3]> {
    let digits = version
        .strip_prefix("agy_acp_server_")
        .or_else(|| version.strip_prefix('v'))
        .unwrap_or(version);
    let mut parts = digits.splitn(3, '.');
    let mut number = |last: bool| -> Option<u64> {
        let part = parts.next()?;
        let part = if last {
            &part[..part
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(part.len())]
        } else {
            part
        };
        part.parse().ok()
    };
    Some([number(false)?, number(false)?, number(true)?])
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
