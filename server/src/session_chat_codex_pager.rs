//! Dedicated handling for Codex's transcript/message-editing pager.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::domain::DomainRepository;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, write_session_chat_payload,
    SessionChatSendStep,
};
use crate::session_chat_terminal_dialog::TerminalDialog;

pub(crate) const CODEX_TRANSCRIPT_PAGER_ID: &str = "codex-transcript-pager";

/// CDXC:SessionChat 2026-09-07 DECISION:
/// User: do not forward Escape to a Codex session while its last terminal rows say "esc again to edit previous message", because it opens the message-editing pager and blocks input until `q`.
pub(crate) fn codex_escape_would_open_transcript_pager(screen: &str) -> bool {
    let screen = strip_ansi_sgr(screen);
    screen.trim_end().lines().rev().take(15).any(|line| {
        normalize_spaces(line)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("esc again to edit previous message")
    })
}

pub(crate) fn transcript_pager_footer(screen: &str) -> Option<String> {
    let footer = screen.lines().rev().find(|line| !line.trim().is_empty())?;
    let footer = normalize_spaces(&strip_ansi_sgr(footer))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if footer.starts_with("Browsing transcript · ")
        && footer.contains(" prompts · ")
        && footer.contains(" rewind · ")
        && footer.ends_with("esc back")
    {
        return Some(footer);
    }
    // The older pager says "q to quit"; current Codex's keymap renderer says "q close".
    let editing = footer
        .strip_prefix("q to quit ")
        .or_else(|| footer.strip_prefix("q close "))?;
    let next = editing
        .strip_prefix("esc/← to edit prev")
        .or_else(|| editing.strip_prefix("esc / ← to edit prev"))
        .or_else(|| editing.strip_prefix("esc to edit prev"))?;
    matches!(next, "" | " → to edit next enter to edit message").then_some(footer)
}

pub(crate) fn detect_codex_transcript_pager(screen: &str) -> Option<TerminalDialog> {
    let footer = transcript_pager_footer(screen)?;
    Some(TerminalDialog {
        id: CODEX_TRANSCRIPT_PAGER_ID.to_string(),
        title: "Restore terminal to chat view".to_string(),
        body: "Codex's transcript viewer is open. It closes automatically when chat is active and no connected client is viewing this session's terminal. Closing it manually also closes it for other clients.".to_string(),
        footer,
        rows: Vec::new(),
        input: None,
        input_value: String::new(),
        actions: vec!["cancel".to_string()],
    })
}

/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: automatically close Codex's transcript pager in chat, but do not close it for another connected client reading it in the terminal.
/// Only positive chat visibility with no visible terminals permits closing; the send worker checks the live pager and all clients again immediately before sending `q`.
pub(crate) fn close_codex_transcript_pager_if_unwatched(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    screen: &str,
) {
    if transcript_pager_footer(screen).is_none() || tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    // Keep the reservation until the queued job finishes so a busy send queue
    // cannot accumulate several closes against the same pager.
    static PENDING: OnceLock<Mutex<HashMap<String, Option<Instant>>>> = OnceLock::new();
    let store = PENDING.get_or_init(Mutex::default);
    let key = crate::server::session_observer_key(project_id, session_id);
    let Ok(mut pending) = store.lock() else {
        return;
    };
    pending.retain(|_, at| at.is_none_or(|at| at.elapsed() < Duration::from_secs(5)));
    if pending.contains_key(&key) {
        return;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    pending.insert(key.clone(), None);
    drop(pending);
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let _ = execute_session_chat_send(
            &project_id,
            &session_id,
            &zmx_name,
            "codex-transcript-pager-close",
            vec![SessionChatSendStep::CloseUnwatchedCodexTranscriptPager],
        )
        .await;
        if let Ok(mut pending) = store.lock() {
            pending.insert(key, Some(Instant::now()));
        }
    });
}

pub(crate) async fn close_unwatched_pager(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    is_cancelled: &(impl Fn() -> bool + Sync),
) -> Result<(), String> {
    let Some(screen) = capture_session_terminal_text(zmx_name).await else {
        return Ok(());
    };
    if transcript_pager_footer(&screen).is_none() {
        return Ok(());
    }
    let name = zmx_name.to_string();
    let chat_only =
        tokio::task::spawn_blocking(move || crate::zmx::zmx_session_has_only_chat_viewers(&name))
            .await
            .ok()
            .flatten();
    if chat_only != Some(true) || is_cancelled() {
        return Ok(());
    }
    write_session_chat_payload(
        project_id,
        session_id,
        zmx_name,
        "codex-transcript-pager-close",
        if screen.contains("Browsing transcript · ") {
            "\x1b[27u"
        } else {
            "q"
        },
    )
    .await
}
