//! A "not moving" or "idle" thread report that waited in its coordinator's queue is checked
//! against the thread again when it is about to go out, and dropped when it no longer holds.
//!
//! CDXC:Coordinators 2026-11-08 DECISION:
//! User (for 11.1.1): "drop late 'not moving' / 'idle' thread reports when they're finally
//! delivered after the thread has moved on, finished or been closed. Today I got a pile of stale
//! 'no new output for 8 minutes' reports for a release thread hours after it finished, because
//! they sat in my queue while my input box wasn't seen. At delivery time, re-check the thread's
//! state and drop (or collapse into one line) reports that no longer hold." The coordinator
//! runtime already re-checks a report right before it sends it (coordinator_runtime.rs), but a
//! send to a coordinator whose input box cannot be read becomes a queued row, and the row went out
//! hours later as it was. So the queue scheduler reads the head row of a coordinator's queue once
//! it has waited a while: a stall report ("still working, but no new output…", "still not
//! moving…") holds only while the thread still works and has done nothing since the quiet time it
//! names; an idle report ("idle for…", "still idle…") holds only while the thread still sits
//! between turns and has done nothing since the time it names. A thread that finished, was closed
//! or resolved, was reloaded, or wrote a row since makes the report stale, and the row is dropped
//! without a trace in the chat (each drop is logged as `sessionChatQueueDroppedStaleReport`).
//! Its finished or closed report says what happened instead. Reports on a thread of another
//! computer are left alone: this computer cannot read that thread.
//! SEE-ALSO: brief.rs (`report_body` words these reports; the prefixes below must follow it),
//! stall.rs (`moved_since`), session_chat_queue_runtime/scheduler.rs (when the check runs).

use serde_json::Value;

use super::records::ThreadRecord;
use super::state::{ThreadProgress, ThreadState};

/// The openings `report_body` gives a stall report, then the words before its quiet time.
const STALL_OPENINGS: [&str; 2] = [
    "Ghostex thread report: still working, but no new output for ",
    "Ghostex thread report: still not moving: no new output for ",
];
const STALL_SINCE: &str = "(quiet since ";

/// The openings `report_body` gives an idle report, then the words before its idle time.
const IDLE_OPENINGS: [&str; 2] = [
    "Ghostex thread report: idle for ",
    "Ghostex thread report: still idle: no new turn for ",
];
const IDLE_SINCE: &str = "(since ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuietReportKind {
    /// "No new output for N minutes" while the thread works.
    Stalled,
    /// "Idle for N minutes" though its last turn left work undone.
    Idle,
}

/// A queued stall or idle report, as far as its text says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuietReport {
    pub kind: QuietReportKind,
    /// The thread's global reference (`Reply to:` of the sender block).
    pub thread_ref: String,
    /// The quiet or idle time the report names (ms).
    pub since: i64,
}

/// The stall or idle report a queued message carries, `None` for any other message.
pub fn parse_quiet_report(text: &str) -> Option<QuietReport> {
    let parts = super::split_agent_message(text)?;
    let body = parts.body.trim_start();
    let (kind, marker) = if STALL_OPENINGS
        .iter()
        .any(|opening| body.starts_with(opening))
    {
        (QuietReportKind::Stalled, STALL_SINCE)
    } else if IDLE_OPENINGS
        .iter()
        .any(|opening| body.starts_with(opening))
    {
        (QuietReportKind::Idle, IDLE_SINCE)
    } else {
        return None;
    };
    let first_line = body.lines().next()?;
    let (_, after) = first_line.split_once(marker)?;
    let (iso, _) = after.split_once(')')?;
    let since = chrono::DateTime::parse_from_rfc3339(iso.trim())
        .ok()?
        .timestamp_millis();
    Some(QuietReport {
        kind,
        thread_ref: parts.reply_to.to_string(),
        since,
    })
}

/// The project and session of a thread reference on this computer (`<serverId>:<projectId>:
/// <sessionId>`), `None` for another computer's thread.
pub fn local_thread_of(thread_ref: &str, server_id: &str) -> Option<(String, String)> {
    let mut parts = thread_ref.trim().split(':');
    let (server, project, session) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none() && server == server_id && !project.is_empty() && !session.is_empty())
        .then(|| (project.to_string(), session.to_string()))
}

/// BLOCKING (a transcript tail read). Why `report` no longer holds for the thread as it is now,
/// `None` while it still does.
pub fn quiet_report_stale_reason(
    report: &QuietReport,
    thread: Option<&ThreadRecord>,
    session: Option<&Value>,
    now_iso: &str,
) -> Option<&'static str> {
    let Some(thread) = thread else {
        return Some("it is no longer a thread");
    };
    if thread.is_resolved() {
        return Some("the thread was marked done");
    }
    let Some(session) = session else {
        return Some("the thread's session was closed");
    };
    let state = super::classify_thread_live(Some(session), ThreadProgress::of(thread), now_iso);
    let holds = match report.kind {
        QuietReportKind::Stalled => state == ThreadState::Working,
        QuietReportKind::Idle => matches!(state, ThreadState::Finished | ThreadState::Sleeping),
    };
    if !holds {
        return Some(match state {
            ThreadState::Working => "the thread is working again",
            ThreadState::Waiting => "the thread is waiting for an answer",
            ThreadState::Finished => "the thread finished its turn",
            ThreadState::Sleeping => "the thread is asleep",
            ThreadState::Closed => "the thread's session was closed",
            ThreadState::Done => "the thread was marked done",
        });
    }
    super::stall::moved_since(session, report.since)
        .then_some("the thread moved on after the time the report names")
}
