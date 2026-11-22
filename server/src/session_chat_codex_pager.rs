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

/// CDXC:SessionChat 2026-09-28 WHY:
/// Codex's fullscreen view (the default since 0.157) binds Escape to its own transcript while the user has text selected ("esc clear"), a search open ("esc close") or the view scrolled up ("enter/esc latest"), and its footer says so. Chat Stop's single Escape then only cleared the selection or returned to the bottom while the turn kept streaming, so the interrupt dismisses what this footer names first.
pub(crate) fn codex_escape_would_dismiss_transcript_interaction(screen: &str) -> bool {
    let screen = strip_ansi_sgr(screen);
    let Some(footer) = screen.lines().rev().find(|line| !line.trim().is_empty()) else {
        return false;
    };
    let footer = normalize_spaces(footer)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    footer.split(" · ").any(|hint| {
        matches!(hint, "esc clear" | "esc close" | "esc") || hint.ends_with("esc latest")
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

/// CDXC:AgentScreenDetection 2026-10-16 WHY:
/// `/diff` (and other read-only views) open Codex's full-screen pager instead of printing into the transcript, so the screen diff found nothing and chat showed a bare "Slash command" row. The pager's own text is the command's output; the note says where the rest is, because the pager stays open in the terminal until `q`.
pub(crate) fn codex_static_pager_output(screen: &str) -> Option<String> {
    let lines: Vec<String> = screen
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect();
    // Blank rows inside the pager are part of its text; only the frame is found by skipping them.
    let mut framed = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (top, title) = framed.next()?;
    let mut frame = framed.rev();
    let (_, footer) = frame.next()?;
    let (_, hints) = frame.next()?;
    let (rule_at, rule) = frame.next()?;
    let body = lines.get(top + 1..rule_at)?;
    let spaced_title = title
        .strip_prefix("/ ")
        .is_some_and(|rest| rest.split(' ').all(|letter| letter.chars().count() == 1));
    if !spaced_title
        || !matches!(footer.trim(), "q close" | "q to quit")
        || !hints.contains("to scroll")
        || !rule.trim_start().starts_with('─')
    {
        return None;
    }
    let body: Vec<&str> = body
        .iter()
        .map(String::as_str)
        .filter(|line| line.trim() != "~")
        .collect();
    let note = if rule.contains(" 100% ") {
        "Opened in Codex's pager in the terminal; press q there to close it."
    } else {
        "First page only: the rest is in Codex's pager in the terminal; press q there to close it."
    };
    Some(format!("{}\n\n{note}", body.join("\n").trim_matches('\n')))
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
        side_question: None,
        blocks: None,
        picker: None,
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
