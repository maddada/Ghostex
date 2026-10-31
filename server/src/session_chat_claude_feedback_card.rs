//! Claude Code's card for a feedback report the model drafted by itself (its SendFeedback tool),
//! which gxserver closes with Escape.
//!
//! Claude Code 2.1.309 draws it between the transcript and the input box once the turn ends:
//!
//! ```text
//! ╭──────────────────────────────────────────────────────────╮
//! │ ✻ Bug report drafted: Orchestrator waited all night on … │
//! │ │ - What happened: Before sleeping, the user asked for … │
//! │ 1 to review · 2 to send · 0 to dismiss · +1 more queued  │
//! ╰──────────────────────────────────────────────────────────╯
//! ```
//!
//! Its preview runs up to six rows, so on a short terminal the card pushes the input box off the
//! bottom of the screen and the screen ends inside the card.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::{
    capture_session_terminal_text_vt, execute_session_chat_send, write_session_chat_payload,
    SessionChatSendError, SessionChatSendFailure, SessionChatSendStep, SESSION_CHAT_INTERRUPT,
};

/// What Claude writes before " drafted: " for each report type (`$do` in its source; "Feedback"
/// when the type is unknown).
const CARD_KINDS: &[&str] = &[
    "Bug report",
    "Product feedback",
    "Feature request",
    "Feedback",
];

/// The question Claude asks after the first dismissals; Escape answers "keep".
const FOLLOW_UP: &str = "Turn off Claude-drafted feedback?";

/// Rows Claude may draw between the card and the input box's top rule ("Update available! Run: …").
const ROWS_BEFORE_INPUT_BOX: usize = 2;

/// Escapes one dismissal spends: the card, the follow-up question, and one spare for a key Claude
/// drops while it repaints.
const DISMISS_PRESSES: usize = 3;

/// Claude Code reads two Escapes within 800 ms as "rewind" (session_chat_claude_interrupt.rs), so
/// presses stay further apart than that.
const ESCAPE_SPACING: Duration = Duration::from_millis(1_200);

/// Claude ignores Escape on the card for 600 ms after its turn ends, and a card that is still
/// there a moment later is not a repaint passing through.
const DISMISS_SETTLE: Duration = Duration::from_millis(1_000);

/// When a turn-end probe looks for the card: after Claude has drawn it.
const TURN_END_PROBE_DELAY: Duration = Duration::from_millis(1_500);

/// How long a session whose card would not close is left alone before another try.
const RETRY_INTERVAL: Duration = Duration::from_secs(60);

const DIAGNOSTIC_SOURCE: &str = "claude-feedback-card";

/// Non-blank rows with their indentation kept: Claude draws the card, the input box and its
/// notices from the first column, and every transcript row (a reply, a quoted prompt, tool output)
/// is indented, so a card quoted in the conversation never reads as the live one.
pub(crate) fn screen_lines(screen_text: &str) -> Vec<String> {
    screen_text
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// Whether the bottom of this screen is Claude's drafted-feedback card.
pub(crate) fn feedback_card_on_screen(screen_text: &str) -> bool {
    card_is_live(&screen_lines(screen_text))
}

fn card_is_live(lines: &[String]) -> bool {
    let Some(title) = lines.iter().rposition(|line| is_card_title(line)) else {
        return false;
    };
    if !title
        .checked_sub(1)
        .is_some_and(|top| lines[top].starts_with('╭'))
    {
        return false;
    }
    let rest = &lines[title + 1..];
    let Some(foot) = rest.iter().position(|line| line.starts_with('╰')) else {
        // The screen ends inside the card: a box still open at the bottom is the prompt area, never
        // transcript, which always has the input box below it.
        return rest.iter().all(|line| line.starts_with('│'));
    };
    rest[..foot].iter().any(|row| is_dismiss_hint(row)) && input_box_follows(&rest[foot + 1..])
}

/// `│ ✻ Bug report drafted: <title> │`, whatever glyph Claude draws first.
fn is_card_title(line: &str) -> bool {
    let Some(inner) = line.strip_prefix('│') else {
        return false;
    };
    let Some((_glyph, rest)) = inner.trim_start().split_once(' ') else {
        return false;
    };
    CARD_KINDS.iter().any(|kind| {
        rest.strip_prefix(kind)
            .is_some_and(|after| after.starts_with(" drafted: "))
    })
}

/// The card's key row ("1 to review · 2 to send · 0 to dismiss", "… · Esc to dismiss" after a
/// failed send, or the "Send without reviewing …?" step). Preview rows carry their own left rule.
fn is_dismiss_hint(row: &str) -> bool {
    let inner = row.trim_start_matches('│').trim_end_matches('│').trim();
    !inner.starts_with('│')
        && (inner.contains("to dismiss") || inner.starts_with("Send without reviewing"))
}

/// The input box's top rule within a couple of rows, its `❯` line under it, with only Claude's
/// right-aligned notices ("Update available! Run: …") between; or nothing at all, because the
/// input box is below the bottom of a short screen.
fn input_box_follows(after: &[String]) -> bool {
    match after.iter().position(|line| is_rule(line)) {
        Some(rule) => {
            rule <= ROWS_BEFORE_INPUT_BOX
                && after[..rule].iter().all(|line| is_right_aligned(line))
                && after
                    .get(rule + 1)
                    .is_some_and(|line| line.starts_with('❯') || line.starts_with('!'))
        }
        None => {
            after.len() <= ROWS_BEFORE_INPUT_BOX && after.iter().all(|line| is_right_aligned(line))
        }
    }
}

/// `──────── title ─`: the input box's top rule, which may carry the session title.
pub(crate) fn is_rule(line: &str) -> bool {
    !line.contains(['│', '╭', '╰'])
        && line.ends_with('─')
        && line.chars().take_while(|&ch| ch == '─').count() >= 4
}

/// A notice Claude right-aligns over its input box: most of the row is leading space.
fn is_right_aligned(line: &str) -> bool {
    let lead = line.len() - line.trim_start().len();
    lead * 3 >= line.chars().count()
}

fn follow_up_on_screen(lines: &[String]) -> bool {
    lines
        .iter()
        .rposition(|line| line.starts_with(FOLLOW_UP))
        .is_some_and(|row| input_box_follows(&lines[row + 1..]))
}

/// The card (or its follow-up question) owns Escape and nothing typed would be lost: Claude listens
/// to Escape on the card only while its input box is empty, so a draft typed in the terminal holds
/// the dismissal. An input box below the bottom of the screen cannot be read and does not hold it.
fn escape_closes_card(vt_screen: &str) -> bool {
    let lines = screen_lines(vt_screen);
    (card_is_live(&lines) || follow_up_on_screen(&lines))
        && crate::session_chat_composer::session_chat_composer_input("claude", vt_screen)
            .is_none_or(|input| input.is_empty())
}

/// The `DismissClaudeFeedbackCard` send step: Escape, one press per fresh capture that still shows
/// the card or its follow-up question, spaced past Claude's double-Escape window. Anything else on
/// screen (the card gone, a turn running, text in the input box) ends it without a key.
pub(crate) async fn dismiss_feedback_card_step(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
) -> Result<(), SessionChatSendError> {
    for _ in 0..DISMISS_PRESSES {
        let Some(screen) = capture_session_terminal_text_vt(zmx_name).await else {
            return Ok(());
        };
        if !escape_closes_card(&screen) {
            return Ok(());
        }
        write_session_chat_payload(
            project_id,
            session_id,
            zmx_name,
            source,
            SESSION_CHAT_INTERRUPT,
        )
        .await
        .map_err(|error| SessionChatSendError::new(SessionChatSendFailure::Write, error))?;
        tokio::time::sleep(ESCAPE_SPACING).await;
    }
    Ok(())
}

/// CDXC:SessionChat 2026-10-25 DECISION:
/// User: Claude's self-drafted bug report card "has to dismiss automatically": it blocked the screen and turned the chat red. Claude Code 2.1.309 opens it on its own after its SendFeedback tool; on a short terminal it pushed the input box off screen, so every message to the orchestrator waited in its queue until the user found the card. Escape on the card is Claude's own "dismiss" (the draft stays saved for /feedback, nothing is sent) and Escape on its "Turn off Claude-drafted feedback?" follow-up is "keep", so no setting changes. The card is closed from the detection funnel whenever a capture sees it on an idle session, and after every Claude turn end.
/// SEE-ALSO: server/src/session_chat_composer.rs (the card makes the input box dismissible for a send), server/src/session_chat_options/detector.rs (the funnel's dispatch).
pub(crate) struct FeedbackCardDismissPlan {
    zmx_name: String,
}

/// A plan when the capture just classified shows the card on a running session that is not working.
pub(crate) fn dismiss_plan(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    composer: &crate::session_chat_composer::SessionChatComposerReadiness,
) -> Option<FeedbackCardDismissPlan> {
    if !composer.shows_claude_feedback_card() {
        return None;
    }
    let session = repository.get_session(project_id, session_id).ok()??;
    if crate::presentation::effective_lifecycle_state(&session) != "running"
        || crate::presentation::presentation_activity(&session, &crate::presentation::now_iso())
            == "working"
    {
        return None;
    }
    let zmx_name = crate::zmx::provider_zmx_session_name(&session).ok()?;
    Some(FeedbackCardDismissPlan { zmx_name })
}

static IN_FLIGHT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
static FAILED: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn claim_key(project_id: &str, session_id: &str) -> String {
    crate::server::session_observer_key(project_id, session_id)
}

/// One dismissal per session at a time, and none for a while after one that left the card up.
pub(crate) fn claim_dismissal(project_id: &str, session_id: &str) -> bool {
    let key = claim_key(project_id, session_id);
    let failed_recently = FAILED
        .get_or_init(Mutex::default)
        .lock()
        .ok()
        .and_then(|failed| failed.get(&key).copied())
        .is_some_and(|at| at.elapsed() < RETRY_INTERVAL);
    if failed_recently {
        return false;
    }
    let Ok(mut in_flight) = IN_FLIGHT.get_or_init(Mutex::default).lock() else {
        return false;
    };
    in_flight.insert(key, Instant::now()).is_none()
}

fn finish_dismissal(project_id: &str, session_id: &str, cleared: bool) {
    let key = claim_key(project_id, session_id);
    if let Ok(mut failed) = FAILED.get_or_init(Mutex::default).lock() {
        failed.retain(|_, at| at.elapsed() < RETRY_INTERVAL);
        if cleared {
            failed.remove(&key);
        } else {
            failed.insert(key.clone(), Instant::now());
        }
    }
    if let Ok(mut in_flight) = IN_FLIGHT.get_or_init(Mutex::default).lock() {
        in_flight.remove(&key);
    }
}

/// Close the card through the session's send worker (so it never lands inside a send), then
/// re-probe and republish so the chat's input box turns ready again.
pub(crate) fn dispatch_dismissal(
    detector: crate::session_chat_options::SessionChatOptionDetector,
    plan: FeedbackCardDismissPlan,
    project_id: &str,
    session_id: &str,
    agent: Option<&str>,
) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        finish_dismissal(project_id, session_id, false);
        return;
    };
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let agent = agent.map(str::to_string);
    handle.spawn(async move {
        tokio::time::sleep(DISMISS_SETTLE).await;
        let showing = capture_session_terminal_text_vt(&plan.zmx_name)
            .await
            .is_some_and(|screen| escape_closes_card(&screen));
        let cleared = if showing {
            let _ = execute_session_chat_send(
                &project_id,
                &session_id,
                &plan.zmx_name,
                DIAGNOSTIC_SOURCE,
                vec![SessionChatSendStep::DismissClaudeFeedbackCard],
            )
            .await;
            !capture_session_terminal_text_vt(&plan.zmx_name)
                .await
                .is_some_and(|screen| feedback_card_on_screen(&screen))
        } else {
            true
        };
        finish_dismissal(&project_id, &session_id, cleared);
        detector
            .detect(&project_id, &session_id, agent.as_deref(), true)
            .await;
        detector.publish_screen_state(&project_id, &session_id);
    });
}

/// After a Claude turn ends, read its screen once, so a card on a session nobody is watching is
/// closed too (the funnel does the closing).
pub(crate) fn observe_turn_end(state: &AppState, params: &Map<String, Value>, result: &Value) {
    if result.get("enteredAttention").and_then(Value::as_bool) != Some(true)
        || result
            .pointer("/activity/attentionSource")
            .and_then(Value::as_str)
            != Some("turnComplete")
    {
        return;
    }
    let Some(session) = result.get("session") else {
        return;
    };
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
    if !matches!(agent.as_deref(), Some("claude" | "openclaude")) {
        return;
    }
    let id = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| params.get(key).and_then(Value::as_str))
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (id("projectId"), id("sessionId")) else {
        return;
    };
    let detector = crate::session_chat_options::SessionChatOptionDetector::new(state);
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    handle.spawn(async move {
        tokio::time::sleep(TURN_END_PROBE_DELAY).await;
        detector
            .detect(&project_id, &session_id, agent.as_deref(), true)
            .await;
    });
}
