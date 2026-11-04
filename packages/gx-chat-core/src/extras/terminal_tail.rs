//! The session's terminal screen, read only when the user asks for it, ported from
//! `packages/shared/session-chat-controller/native-terminal-tail.ts`.
//!
//! Both readers are the ones React had (the Terminal View hover and the `composerNotReady` refusal
//! card), with their two rules kept: `unknown` is not "not ready", and a failed hover read keeps
//! the last verdict.

use serde_json::Value;

use crate::document::{TerminalTail, TerminalTailNotice};
use crate::extras::terminal_tail_format::format_terminal_tail_preview;
use crate::state::TerminalTailState;

/// `retire`: the refusal is gone, so the expanded excerpt under it goes with it.
pub fn retire(tail: &mut TerminalTailState) {
    if !tail.notice_open && tail.notice_tail.is_none() && tail.notice_error.is_none() {
        return;
    }
    tail.notice_open = false;
    tail.notice_tail = None;
    tail.notice_error = None;
    tail.notice_loading = false;
}

/// Drops the hover verdict once a turn starts or ends or a question or approval opens or closes.
///
/// CDXC:SessionChat 2026-10-28 WHY: The verdict is only read on hover, so a "not ready" read while
/// Codex's approval box covered its input box stayed red, with Copy Report beside it, long after
/// the box closed and the agent was waiting for input again (found testing Codex on Linux; every
/// desktop and the phone draw this document). Neutral is the honest state for an unmeasured screen.
pub fn expire(tail: &mut TerminalTailState, now: (bool, bool)) {
    if tail.tail.is_some() && tail.tail_measured_during.is_some_and(|then| then != now) {
        tail.tail = None;
        tail.tail_measured_during = None;
        tail.recheck_at_ms = None;
    }
}

/// How long after a "not ready" verdict the screen is read again, then how often while it stays so.
pub const RECHECK_FIRST_MS: f64 = 15_000.0;
pub const RECHECK_EVERY_MS: f64 = 30_000.0;

/// Schedules the next look after a hover read answered: one while the verdict is "not ready", none
/// otherwise. `rechecked` is whether this answer was itself a recheck.
///
/// CDXC:SessionChat 2026-10-29 DECISION:
/// User, on a red Copy Report pill beside an idle coordinator's Terminal View button: "jsut got this?? pls fix why do i see this". The verdict came from one hover read that caught Claude's screen half drawn (no rules around `❯`, a stray "48" under it), and a verdict is only read again on the next hover, so it stayed red. A "not ready" verdict now reads the screen again by itself, 15 s later and then every 30 s while it stays red, and clears as soon as the input box reads again; a ready or neutral verdict schedules nothing. gxserver repairs the half-drawn frame on the read itself (server/src/session_chat_queue_runtime/held_redraw.rs), so a transient frame rarely reaches here at all.
pub fn schedule_recheck(tail: &mut TerminalTailState, now_ms: f64, rechecked: bool) {
    let not_ready = tail
        .tail
        .as_ref()
        .filter(|read| captured(read))
        .and_then(|read| read.get("composerState").and_then(Value::as_str))
        == Some("notReady");
    tail.recheck_at_ms = not_ready.then(|| {
        now_ms
            + if rechecked {
                RECHECK_EVERY_MS
            } else {
                RECHECK_FIRST_MS
            }
    });
}

/// Whether the scheduled recheck has come due; taking it clears the schedule.
pub fn recheck_due(tail: &mut TerminalTailState, now_ms: f64) -> bool {
    match tail.recheck_at_ms {
        Some(due) if now_ms >= due && tail.hover_request.is_none() => {
            tail.recheck_at_ms = None;
            true
        }
        _ => false,
    }
}

/// `project`: the `terminalTail` document value.
pub fn project(tail: &TerminalTailState) -> TerminalTail {
    /*
    Only a MEASURED verdict tints the button: `unknown`, an uncaptured screen and the time before
    the first hover all stay neutral, because the daemon fails open on `unknown` and a red button
    there would accuse a session that sends fine.
    */
    let readiness = tail
        .tail
        .as_ref()
        .filter(|read| captured(read))
        .and_then(|read| read.get("composerState").and_then(Value::as_str))
        .filter(|state| *state != "unknown")
        .map(str::to_string);
    TerminalTail {
        preview: match tail.tail.as_ref().filter(|read| captured(read)) {
            Some(read) => format_terminal_tail_preview(&lines(read)),
            None => String::new(),
        },
        reason: match readiness.as_deref() {
            Some("notReady") => tail
                .tail
                .as_ref()
                .and_then(|read| read.get("reason"))
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        },
        notice: TerminalTailNotice {
            open: tail.notice_open,
            loading: tail.notice_loading,
            error: tail.notice_error.clone(),
            excerpt: excerpt_of(tail.notice_tail.as_ref()),
            empty: if tail.notice_open && !tail.notice_loading && tail.notice_error.is_none() {
                empty_copy(tail.notice_tail.as_ref())
            } else {
                None
            },
        },
        readiness,
    }
}

/// `excerptOf`: the captured rows verbatim, or "".
fn excerpt_of(tail: Option<&Value>) -> String {
    match tail.filter(|read| captured(read)) {
        Some(read) => {
            let lines = lines(read);
            if lines.is_empty() {
                String::new()
            } else {
                lines.join("\n")
            }
        }
        None => String::new(),
    }
}

/// `emptyCopy`: why the sheet has nothing to show, or `null` when it does.
fn empty_copy(tail: Option<&Value>) -> Option<String> {
    if !excerpt_of(tail).is_empty() {
        return None;
    }
    Some(
        if tail.is_some_and(|read| !captured(read)) {
            "Ghostex could not read this session\u{2019}s terminal screen."
        } else {
            "The terminal screen is empty."
        }
        .to_string(),
    )
}

/// `tail.captured === true`.
fn captured(tail: &Value) -> bool {
    tail.get("captured") == Some(&Value::Bool(true))
}

/// `tail.lines`, as the strings the formatter takes.
fn lines(tail: &Value) -> Vec<String> {
    tail.get("lines")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod recheck_tests {
    use super::*;
    use serde_json::json;

    fn verdict(state: &str) -> Value {
        json!({ "captured": true, "composerState": state, "lines": ["❯"] })
    }

    /// A red verdict looks again 15 s later, then every 30 s while it stays red, and stops once the
    /// input box reads (or the read says nothing).
    #[test]
    fn a_not_ready_verdict_rechecks_until_it_clears() {
        let mut tail = TerminalTailState {
            tail: Some(verdict("notReady")),
            ..TerminalTailState::default()
        };
        schedule_recheck(&mut tail, 1_000.0, false);
        assert_eq!(tail.recheck_at_ms, Some(16_000.0));
        assert_eq!(project(&tail).readiness.as_deref(), Some("notReady"));

        assert!(!recheck_due(&mut tail, 15_999.0));
        // A hover read already in flight answers instead.
        tail.hover_request = Some(7);
        assert!(!recheck_due(&mut tail, 16_000.0));
        tail.hover_request = None;
        assert!(recheck_due(&mut tail, 16_000.0));
        assert_eq!(tail.recheck_at_ms, None);

        // Still red: the slower follow-up.
        schedule_recheck(&mut tail, 16_500.0, true);
        assert_eq!(tail.recheck_at_ms, Some(46_500.0));

        // The box reads again: green, nothing scheduled.
        tail.tail = Some(verdict("ready"));
        schedule_recheck(&mut tail, 46_600.0, true);
        assert_eq!(tail.recheck_at_ms, None);
        assert_eq!(project(&tail).readiness.as_deref(), Some("ready"));

        // An uncaptured screen is neutral, never red, and schedules nothing either.
        tail.tail = Some(json!({ "captured": false, "composerState": "notReady" }));
        schedule_recheck(&mut tail, 50_000.0, false);
        assert_eq!(tail.recheck_at_ms, None);
    }

    #[test]
    fn a_turn_changing_drops_the_verdict_and_its_recheck() {
        let mut tail = TerminalTailState {
            tail: Some(verdict("notReady")),
            tail_measured_during: Some((false, false)),
            recheck_at_ms: Some(16_000.0),
            ..TerminalTailState::default()
        };
        expire(&mut tail, (true, false));
        assert_eq!((tail.tail.is_none(), tail.recheck_at_ms), (true, None));
    }
}
