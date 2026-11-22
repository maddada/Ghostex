/*
CDXC:AgentScreenDetection 2026-10-06 DECISION:
"Blocking screens. The `/trust` gate, the login screen and the 429 rate-limit screen become chat notices." Read against Empryo 3.9.0-beta's TUI bundle and live screens:

  * The trust gate is not a dialog. A repository whose `.empryo/config.json` declares settings Empryo withholds until the repo is trusted (MCP servers, hooks, providers, `yolo`) gets the row "This repo's config declares {…}. Empryo does not run any of it until you trust this repo. Type /trust to trust it." and Empryo keeps taking input, so the card informs and offers `/trust` instead of holding sends. `/trust` prints "Trusted <path>. Restart to apply: …", which retires the card.
  * Signing in happens in Empryo's `/login` panel; the screen that stops a chat is the "No model configured yet; chat is disabled until you pick one" row, the "No model selected. Press Ctrl+L or type /model …" row a prompt gets when no model is picked (seen live 2026-10-06 with `"defaultModel": "none"`: the prompt is dropped), or an account whose grant was revoked.
  * A rate limit is a retry row inside the running turn ("HTTP 429 rate limited — …", then "Retry N · …: Retrying in Ns"); Empryo keeps waiting on its own, so the card explains the silence while the turn runs.

Tool approvals come from the shared choice panel (session_chat_empryo_question.rs).
*/

use crate::session_chat_notice::{
    session_chat_terminal_screen_tail, SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
    SESSION_CHAT_NOTICE_EMPRYO_INPUT_BLOCKED, SESSION_CHAT_NOTICE_EMPRYO_SIGN_IN,
    SESSION_CHAT_NOTICE_TRUST_PROMPT, SESSION_CHAT_NOTICE_USAGE_LIMIT,
};
use crate::session_chat_options::strip_ansi_sgr;

const TRUST_WITHHELD: &str = "Empryo does not run any of it until you trust this repo.";
const TRUST_DECLARES: &str = "This repo's config declares ";
const TRUSTED_PREFIX: &str = "Trusted ";
const TRUSTED_RESTART: &str = "Restart to apply";
const NO_MODEL: &str = "No model configured yet; chat is disabled until you pick one.";
const NO_MODEL_SELECTED: &str = "No model selected.";
const GRANT_REVOKED: &str = "Access revoked; sign in again to use this account";
const RATE_LIMITED: &str = "HTTP 429 rate limited";

fn screen_lines(screen_text: &str) -> Vec<String> {
    screen_text
        .lines()
        .map(|line| strip_ansi_sgr(line).trim().to_string())
        .collect()
}

/// A screen-read card with the given actions.
fn screen_notice(
    kind: &str,
    severity: SessionChatTerminalNoticeSeverity,
    title: &str,
    detail: String,
    screen_text: &str,
    actions: Vec<SessionChatTerminalNoticeAction>,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        kind,
        severity,
        SessionChatTerminalNoticeSource::Screen,
        title,
    )
    .with_detail(detail)
    .with_screen_tail(session_chat_terminal_screen_tail(screen_text))
    .with_actions(actions)
}

/// The rows since the newest prompt header (`◆  You · 07:14 PM`): what the current turn, or the
/// idle screen after it, printed. A row above it is history and must not raise a card.
fn since_last_prompt(lines: &[String]) -> &[String] {
    let start = lines
        .iter()
        .rposition(|line| line.starts_with('◆') && line.contains(" · "))
        .map_or(0, |header| header + 1);
    &lines[start..]
}

fn open_terminal() -> SessionChatTerminalNoticeAction {
    SessionChatTerminalNoticeAction::switch_to_terminal("Open terminal")
}

/// The open trust gate: the newest withheld-config row with no "Trusted" row after it.
fn trust_notice(screen_text: &str, lines: &[String]) -> Option<SessionChatTerminalNotice> {
    let withheld = lines
        .iter()
        .rposition(|line| line.contains(TRUST_WITHHELD))?;
    if lines[withheld + 1..]
        .iter()
        .any(|line| line.contains(TRUSTED_PREFIX) && line.contains(TRUSTED_RESTART))
    {
        return None;
    }
    let declared = lines[withheld]
        .split_once(TRUST_DECLARES)
        .and_then(|(_, rest)| rest.split_once(". "))
        .map(|(declared, _)| declared.trim())
        .filter(|declared| !declared.is_empty())
        .unwrap_or("settings");
    // Ctrl+U empties the input box first, once per line a draft can span, so `/trust` starts the
    // box and runs as a command; Ctrl+K would open Empryo's command palette.
    let trust_keys = format!(
        "{}/trust{}",
        crate::session_chat_send::AGENT_TUI_CLEAR_INPUT_LINE
            .repeat(crate::session_chat_send::AGENT_TUI_CLEAR_MAX_LINES),
        crate::session_chat_send::SESSION_CHAT_SUBMIT
    );
    Some(
        screen_notice(
            SESSION_CHAT_NOTICE_TRUST_PROMPT,
            SessionChatTerminalNoticeSeverity::Warning,
            "Empryo is ignoring this repository's config until you trust it",
            format!("This repository's .empryo/config.json declares {declared}, which Empryo ignores until you trust the repository. Trust it only if you know where that config came from; Empryo applies it after a restart."),
            screen_text,
            vec![
                SessionChatTerminalNoticeAction::send_keys("trust", "Trust", &trust_keys),
                open_terminal(),
            ],
        )
        .with_input_blocking(false),
    )
}

fn login_notice(screen_text: &str, lines: &[String]) -> Option<SessionChatTerminalNotice> {
    let lines = since_last_prompt(lines);
    let (title, detail) = if lines
        .iter()
        .any(|line| line.contains(NO_MODEL) || line.contains(NO_MODEL_SELECTED))
    {
        (
            "Empryo has no model to use",
            "Sign in with /login or add a key with /keys in the terminal, then pick a model.",
        )
    } else if lines.iter().any(|line| line.contains(GRANT_REVOKED)) {
        (
            "Empryo needs you to sign in again",
            "The account's access was revoked. Sign in again with /login in the terminal.",
        )
    } else {
        return None;
    };
    Some(
        screen_notice(
            SESSION_CHAT_NOTICE_EMPRYO_SIGN_IN,
            SessionChatTerminalNoticeSeverity::Error,
            title,
            detail.to_string(),
            screen_text,
            vec![open_terminal()],
        )
        // The row stays on screen after a model is picked, so it never holds a send or the queue.
        .with_input_blocking(false),
    )
}

/// A 429 retry row in the turn that is still running, which the caller vouches for.
fn rate_limit_notice(
    screen_text: &str,
    lines: &[String],
    turn_running: bool,
) -> Option<SessionChatTerminalNotice> {
    let evidence = since_last_prompt(lines)
        .iter()
        .rev()
        .find(|line| line.contains(RATE_LIMITED))?;
    if !turn_running {
        return None;
    }
    let evidence = evidence
        .split_once(RATE_LIMITED)
        .map_or(evidence.as_str(), |(_, rest)| rest)
        .trim_start_matches([' ', '—', '-'])
        .trim();
    let quoted = if evidence.is_empty() {
        String::new()
    } else {
        format!(": \"{evidence}\"")
    };
    Some(
        screen_notice(
            SESSION_CHAT_NOTICE_USAGE_LIMIT,
            SessionChatTerminalNoticeSeverity::Warning,
            "Empryo is rate limited and waiting to retry",
            format!("The model's provider answered HTTP 429{quoted}. Empryo retries on its own; switch models to keep working sooner."),
            screen_text,
            vec![open_terminal()],
        )
        .with_input_blocking(false),
    )
}

/// CDXC:SessionChat 2026-10-06 WHY:
/// Asked by the Empryo build coordinator for Sven: a slash command that opens an interactive Empryo panel "must not leave the chat looking dead: show the row plus a clear one-line note that the panel is open in the terminal, with the existing way to reveal the terminal", and the panel must be closable. The card offers Close (Escape) and Open terminal, and a message sent from the chat closes the panel first (the dismissible readiness in session_chat_composer.rs); the prompt queue waits instead, since the user opened the panel.
fn panel_notice(screen_text: &str) -> Option<SessionChatTerminalNotice> {
    if !crate::session_chat_composer::empryo_input_unfocused(screen_text) {
        return None;
    }
    Some(
        screen_notice(
            SESSION_CHAT_NOTICE_EMPRYO_INPUT_BLOCKED,
            SessionChatTerminalNoticeSeverity::Info,
            "An Empryo panel is open in the terminal",
            "Use it in the terminal, or close it here. A message sent from the chat closes it first; queued messages wait until it is closed.".to_string(),
            screen_text,
            vec![
                SessionChatTerminalNoticeAction::send_keys(
                    "closePanel",
                    "Close panel",
                    crate::session_chat_send::SESSION_CHAT_INTERRUPT,
                ),
                open_terminal(),
            ],
        )
        .with_input_blocking(true),
    )
}

/// The notice Empryo's screen carries, most blocking first.
pub fn classify_empryo_terminal_notice(screen_text: &str) -> Option<SessionChatTerminalNotice> {
    crate::session_chat_empryo_question::detect_empryo_approval_notice(screen_text)
        .or_else(|| panel_notice(screen_text))
        .or_else(|| {
            let lines = screen_lines(screen_text);
            login_notice(screen_text, &lines)
                .or_else(|| trust_notice(screen_text, &lines))
                .or_else(|| {
                    let busy = crate::session_chat_composer::empryo_composer_busy(screen_text)
                        == Some(true);
                    rate_limit_notice(screen_text, &lines, busy)
                })
        })
}

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// Empryo 3.9.1-beta no longer marks a running turn on its prompt glyph, so the screen alone never shows the rate-limit card there. The chat's own detection holds the session and asks its transcript mirror whether a turn is in flight, and shows the card for that turn when the screen-only classifier found nothing.
pub(crate) fn empryo_rate_limit_notice_for_session(
    repository: &crate::domain::DomainRepository<'_>,
    session: &serde_json::Value,
    screen_text: &str,
) -> Option<SessionChatTerminalNotice> {
    let log = crate::session_chat_pi_models::empryo_session_log(repository, session)?;
    let mirror =
        crate::session_chat_empryo_mirror::resolve_empryo_chat_transcript_path(None, Some(&log))?;
    crate::session_chat_empryo_mirror::empryo_turn_in_flight(&mirror)?;
    rate_limit_notice(screen_text, &screen_lines(screen_text), true)
}
