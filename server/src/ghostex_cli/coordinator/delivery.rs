//! `start-thread` waits until the new thread's transcript records its brief.

use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::ghostex_cli::{
    agents,
    args::Flags,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// How long `start-thread` waits for the brief to start a turn: an agent boots, then its input
/// box takes the brief.
const BRIEF_CONFIRM_WINDOW: Duration = Duration::from_secs(60);
const BRIEF_CONFIRM_POLL: Duration = Duration::from_secs(1);
/// How many looks in a row must show the failed-start card before `start-thread` calls it failed.
const START_FAILED_LOOKS: u32 = 3;
const START_ERROR_MAX_CHARS: usize = 400;

/// The card on the thread's screen: its kind, its title with the agent's own error behind it.
struct Notice {
    kind: String,
    shown: String,
}

/// CDXC:Coordinators 2026-11-28 WHY:
/// GitHub #789: a thread whose agent could not start answered `pending` after a minute with only
/// the card's title ("Claude Code couldn't start"); the error the agent printed (`bwrap: Can't
/// find source path`) sat behind the card's Details toggle and in the host's spool. The card's
/// `details` (a Native agent's stderr, acp/startup_error.rs) or `errorLines` (what a terminal
/// agent printed, session_chat_notice/start_failure.rs) now follow the title, and a start that
/// failed is an error after a few looks instead of a minute's wait.
fn notice_of(chat: &Value) -> Option<Notice> {
    let notice = &chat["terminalNotice"];
    let title = agents::text(notice, "title");
    if title.is_empty() {
        return None;
    }
    let why = ["details", "errorLines"]
        .iter()
        .map(|key| agents::text(notice, key))
        .find(|text| !text.is_empty())
        .map(|text| {
            let line = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" | ");
            if line.chars().count() > START_ERROR_MAX_CHARS {
                let mut cut: String = line.chars().take(START_ERROR_MAX_CHARS - 1).collect();
                cut.push('…');
                cut
            } else {
                line
            }
        });
    Some(Notice {
        kind: agents::text(notice, "kind").to_string(),
        shown: match why {
            Some(why) => format!("{title}: {why}"),
            None => title.to_string(),
        },
    })
}

pub(super) enum BriefOutcome {
    /// The thread's transcript records the brief: it is working on it.
    Started,
    /// Not yet, for the reason given; gxserver keeps watching and reports to the coordinator.
    Pending(String),
}

/// CDXC:Coordinators 2026-10-04 WHY:
/// `start-thread` printed "Started thread" as soon as the brief was queued, so a brief that never reached the thread looked like work in progress until someone noticed the idle thread. It now answers `started` only once the thread's transcript records the brief. A queued brief the send marked failed is retried once, unless the thread's screen shows something only a person can answer (a trust question, a login), which would fail it again; a second failure is an error. Anything else still unconfirmed after a minute is `pending` with the reason, and the supervisor's delivery watch (endpoint.rs `watch_pending_message`) tells the coordinator later if the brief never arrives.
pub(super) fn confirm_brief_started(
    thread: &Value,
    task: &str,
    prompt_id: Option<&str>,
    flags: &Flags,
) -> CliResult<BriefOutcome> {
    let target = json!({
        "globalRef": thread["globalRef"], "projectId": thread["projectId"], "sessionId": thread["sessionId"],
    });
    let mut read = target.clone();
    read["limit"] = json!(30);
    // A thread created for this brief has no transcript rows of its own before it.
    let before = agents::TranscriptRows::Known(Default::default());
    let started = Instant::now();
    let mut retried = false;
    let mut failed_looks = 0;
    loop {
        std::thread::sleep(BRIEF_CONFIRM_POLL);
        let chat = call_gxserver_rpc("/api/readSessionChat", &read, flags).ok();
        if chat
            .as_ref()
            .is_some_and(|chat| agents::chat_shows(chat, task, &before))
        {
            return Ok(BriefOutcome::Started);
        }
        let notice = chat.as_ref().and_then(notice_of);
        let start_failed = notice.as_ref().is_some_and(|notice| {
            notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_AGENT_EXITED
        });
        failed_looks = if start_failed { failed_looks + 1 } else { 0 };
        let notice = notice.map(|notice| notice.shown);
        let row = prompt_id.and_then(|prompt_id| queued_row(&target, prompt_id, flags));
        let row_state = row
            .as_ref()
            .map(|row| agents::text(row, "state").to_string());
        // The card stayed for a few looks, or the brief's send already failed on it.
        if start_failed
            && (failed_looks >= START_FAILED_LOOKS || row_state.as_deref() == Some("failed"))
        {
            return Err(CliError::Other(format!(
                "Created thread {}, but its agent did not start: {}. Its brief waits in its chat queue until the agent starts. Fix the error (or tell the user) before trying again; do not start another thread.",
                agents::text(thread, "globalRef"),
                notice.unwrap_or_default().trim_end_matches('.')
            )));
        }
        if row_state.as_deref() == Some("failed") {
            let error = row
                .as_ref()
                .map(|row| agents::text(row, "errorMessage").to_string())
                .unwrap_or_default();
            if let Some(notice) = notice {
                return Ok(BriefOutcome::Pending(format!(
                    "its screen shows \"{notice}\", so its brief is held in its chat queue, marked not delivered ({error}). Tell the user; once that is answered, the brief can be retried from the thread's chat"
                )));
            }
            if retried {
                return Err(CliError::Other(format!(
                    "Created thread {}, but its brief could not be delivered, also after one retry: {error} It is kept in the thread's chat queue, marked not delivered. Read the thread's chat before doing anything else; do not start another thread.",
                    agents::text(thread, "globalRef")
                )));
            }
            let mut retry = target.clone();
            retry["promptId"] = json!(prompt_id);
            retry["retry"] = json!(true);
            call_gxserver_rpc("/api/updateSessionChatQueuedPrompt", &retry, flags)?;
            retried = true;
            continue;
        }
        if started.elapsed() < BRIEF_CONFIRM_WINDOW {
            continue;
        }
        let reason = match (notice, row_state.as_deref()) {
            (Some(notice), _) => format!("its screen shows \"{notice}\""),
            (None, Some(_)) => "its agent is still starting; Ghostex types the brief as soon as its input box appears".to_string(),
            (None, None) => "Ghostex typed the brief, but the thread's transcript does not show it yet".to_string(),
        };
        return Ok(BriefOutcome::Pending(reason));
    }
}

/// The brief's row in the thread's chat queue, while it is there.
fn queued_row(target: &Value, prompt_id: &str, flags: &Flags) -> Option<Value> {
    call_gxserver_rpc("/api/readSessionChatQueue", target, flags)
        .ok()?
        .get("queue")?
        .as_array()?
        .iter()
        .find(|row| agents::text(row, "id") == prompt_id)
        .cloned()
}
