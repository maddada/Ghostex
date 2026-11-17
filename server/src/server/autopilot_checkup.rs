//! Autopilot's supervisor check-up: every few minutes (Settings > Autopilot "Supervisor
//! check-up", 10 by default) Ghostex's own code looks for covered sessions that seem stuck or off
//! track in ways the rules (autopilot_runtime.rs) don't settle. Only when it finds one does it hand
//! them to the supervisor (autopilot_judge.rs), which nudges or heals them through
//! `ghostex autopilot decide`; with none, nothing wakes and no tokens are spent.
//!
//! CDXC:Autopilot 2026-11-18 DECISION:
//! User: "I wanna wake it up programmatically only when it needs to actually review something, not just all the time. also maybe we can actually run it every 10 mins or something based on a setting so it can nudge threads that need nudging that code doesn't catch or heal them". What counts, all read from what Ghostex already has (no agent call): the rules left it for the user (still silent after an interrupt and a reload, or at a rule's hourly cap); Autopilot told it to carry on and it wrote nothing since; Autopilot's log shows two or more failures for it in the last hour; it said the same thing three times in a row (looping on one error); or it ended its turn saying it waits on something while nothing runs, for longer than a check-up. A session is handed at most twice an hour (`Kind::Checkup`'s cap in autopilot_runtime.rs), and one the supervisor already decided on is not handed again until it writes something new; only failures after that decision count. Nothing found is not logged: the log keeps only what Autopilot did.
//! SEE-ALSO: server/src/autopilot/config.rs (`checkup_minutes`), server/src/autopilot/notes.rs (the playbook's check-up section).

use super::*;

use crate::coordinators::{IdleWaitReason, SessionKey};
use crate::session_chat::{SessionChatBlock, SessionChatRole};
use crate::session_chat_queue_runtime::SessionChatTranscriptGate;

/// Messages read back for a session's newest words.
const TAIL_MESSAGES: usize = 24;
/// The same words this many times in a row count as a loop.
const LOOP_REPEATS: usize = 3;
/// Failures in Autopilot's log for one session within the hour that count.
const FAILURES: usize = 2;
const HOUR_MS: i64 = 60 * 60 * 1000;

/// What the rules already know about one covered session.
pub(super) struct Look<'a> {
    pub session: &'a Value,
    pub gate: &'a mut SessionChatTranscriptGate,
    pub idle_path: &'a mut Option<std::path::PathBuf>,
    /// The rules left it for the user after an interrupt and a reload.
    pub parked_quiet: bool,
    /// A rule reached its hourly cap on it (its words).
    pub capped: Option<&'static str>,
    /// When Autopilot last told it to carry on.
    pub carried_on_at: Option<i64>,
}

/// What Autopilot's log says about one session in the last hour.
#[derive(Clone, Copy, Default)]
pub(super) struct History {
    /// Failed tries since the supervisor last looked at it.
    pub failures: usize,
    /// When the supervisor last decided on it.
    pub judged_at: Option<i64>,
}

/// Autopilot's log of the last hour, by session.
pub(super) fn recent_history(state: &AppState, now: i64) -> HashMap<SessionKey, History> {
    let mut history: HashMap<SessionKey, History> = HashMap::new();
    // Newest first: failures count until the supervisor's newest decision.
    for entry in crate::autopilot::read_log(&state.paths, 400) {
        let Some(at) = chrono::DateTime::parse_from_rfc3339(&entry.at)
            .ok()
            .map(|at| at.timestamp_millis())
            .filter(|at| now - at < HOUR_MS)
        else {
            continue;
        };
        if entry.session_id.is_empty() {
            continue;
        }
        let seen = history
            .entry((entry.project_id.clone(), entry.session_id.clone()))
            .or_default();
        if entry.rule == "supervisor" {
            seen.judged_at = seen.judged_at.or(Some(at));
        } else if entry.outcome == crate::autopilot::Outcome::Failed && seen.judged_at.is_none() {
            seen.failures += 1;
        }
    }
    history
}

/// BLOCKING (its transcript tail). Why this session needs the supervisor's look, and what
/// Ghostex saw, or `None` when it looks fine.
pub(super) fn look(
    look: Look<'_>,
    history: History,
    period_ms: i64,
    now: i64,
    now_iso: &str,
) -> Option<(String, String)> {
    let working = crate::presentation::presentation_activity(look.session, now_iso) == "working";
    // The supervisor already looked at it: again only once it wrote something since.
    if let Some(judged_at) = history.judged_at {
        let wrote_since = look
            .gate
            .tail(look.session, TAIL_MESSAGES)
            .is_some_and(|tail| {
                tail.messages.iter().any(|message| {
                    message.role == SessionChatRole::Assistant
                        && message.timestamp.is_some_and(|at| at > judged_at)
                })
            });
        if !wrote_since {
            return None;
        }
    }
    let failures = history.failures;
    if look.parked_quiet {
        return Some(why(
            "It is still silent after Autopilot interrupted and reloaded it.",
            look.gate,
            look.session,
        ));
    }
    if let Some(rule) = look.capped {
        return Some(why(
            &format!("Autopilot reached its hourly limit for {rule} on it."),
            look.gate,
            look.session,
        ));
    }
    if failures >= FAILURES {
        return Some(why(
            &format!("Autopilot's last {failures} tries on it in the past hour failed."),
            look.gate,
            look.session,
        ));
    }
    let tail = look.gate.tail(look.session, TAIL_MESSAGES)?.clone();
    let words: Vec<(String, Option<i64>)> = tail
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .map(|message| (text_of(&message.blocks), message.timestamp))
        .filter(|(text, _)| !text.is_empty())
        .collect();
    let newest_words = words.iter().filter_map(|(_, at)| *at).max();
    // Told to carry on, and nothing written since, a whole check-up later.
    if !working {
        if let Some(at) = look.carried_on_at {
            if now - at >= period_ms && newest_words.is_none_or(|newest| newest <= at) {
                return Some(why(
                    &format!(
                        "Autopilot told it to carry on {} ago and it hasn't written anything since.",
                        minutes(now - at)
                    ),
                    look.gate,
                    look.session,
                ));
            }
        }
    }
    // The same words, three times running: an agent looping on one error.
    let repeated = words
        .iter()
        .rev()
        .take(LOOP_REPEATS)
        .map(|(text, _)| normalized(text))
        .collect::<Vec<_>>();
    if repeated.len() == LOOP_REPEATS
        && repeated
            .iter()
            .all(|text| text.len() >= 20 && *text == repeated[0])
    {
        return Some(why(
            &format!("It said the same thing {LOOP_REPEATS} times in a row, so it looks stuck in a loop."),
            look.gate,
            look.session,
        ));
    }
    // "I wait on …" with nothing running, for longer than a check-up.
    if !working {
        let last = words.last().map(|(text, _)| text.as_str());
        let ended = newest_words?;
        if now - ended >= period_ms
            && crate::coordinators::session_idle_reason(look.session, last, false, look.idle_path)
                == Some(IdleWaitReason::SaidWaiting)
        {
            return Some(why(
                &format!(
                    "It said it waits on something {} ago, and nothing of it is running.",
                    minutes(now - ended)
                ),
                look.gate,
                look.session,
            ));
        }
    }
    None
}

/// The reason with the session's last words as the detail.
fn why(reason: &str, gate: &mut SessionChatTranscriptGate, session: &Value) -> (String, String) {
    let last = gate
        .tail(session, TAIL_MESSAGES)
        .and_then(|tail| {
            tail.messages
                .iter()
                .rev()
                .find(|message| message.role == SessionChatRole::Assistant)
                .map(|message| text_of(&message.blocks))
        })
        .filter(|text| !text.is_empty())
        .map(|text| {
            let short: String = text.chars().take(600).collect();
            format!("Its last words: \"{short}\"")
        })
        .unwrap_or_else(|| "It has written nothing Ghostex can read.".to_string());
    (format!("Check-up: {reason}"), last)
}

fn text_of(blocks: &[SessionChatBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.trim()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(160)
        .collect()
}

fn minutes(ms: i64) -> String {
    match (ms / 60_000).max(1) {
        1 => "1 minute".to_string(),
        n if n >= 120 => format!("{} hours", n / 60),
        n => format!("{n} minutes"),
    }
}
