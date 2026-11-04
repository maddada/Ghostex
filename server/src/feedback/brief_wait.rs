//! Start Agent answers only once the feedback agent has taken its brief, or says why it could not.
//!
//! CDXC:Feedback 2026-10-26 WHY:
//! Start Agent answered as soon as the session existed, and the pop-up closed onto a chat whose brief waited as a startup send; a startup send never gives up while its session lives (CDXC:SessionChat 2026-10-11 in session_chat_send_wake.rs), so an agent that never showed its input box left the tester on "Waiting for agent…" for good (GitHub #323). gxserver now holds the answer until the brief leaves the chat queue. A start that fails (the agent is missing, not signed in, or its brief was refused) or is not ready after `BRIEF_WAIT` answers `feedbackAgentNotReady` in plain words, the session it created is closed so nothing waits behind it, and the client keeps the report so it can be sent without the agent. A dialog the user can answer in the chat (a trust or update prompt) opens the session on its card instead of failing.
//! SEE-ALSO: apps/desktop/src/app/window/feedback_modal/state.rs (`start_agent` turns the switch off on this code), apps/mobile/app/src/screens/FeedbackScreen.tsx (shows the message).

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{schedule_presentation_session_delta, AppState};
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SESSION_CHAT_NOTICE_AGENT_EXITED, SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
};
use crate::session_chat_queue::{
    read_session_chat_queue_snapshot, SESSION_CHAT_QUEUE_STATE_FAILED,
};
use crate::storage::open_gxserver_database;
use crate::zmx::dispatch_zmx_lifecycle_endpoint;

/// The error code a client recognises to offer sending the report without the agent.
pub(super) const FEEDBACK_AGENT_NOT_READY: &str = "feedbackAgentNotReady";

/// How long an agent may take to show its input box and take the brief. Agents come up in a few
/// seconds; a first start that updates itself or loads many MCP servers can take most of a minute.
const BRIEF_WAIT: Duration = Duration::from_secs(90);
const LOOK_EVERY: Duration = Duration::from_secs(1);
/// Looks in a row a failed start must show before it counts, so a screen caught mid-redraw does not.
const FAILURE_LOOKS: u32 = 2;
/// Looks in a row a dialog must stay before the session opens on it.
const DIALOG_LOOKS: u32 = 3;

const SEND_WITHOUT: &str = "You can still send your report without the agent.";

/// What one look at the new session says about its brief.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BriefStatus {
    /// The brief left the chat queue: the agent has it.
    Taken,
    /// The agent shows a dialog the user can answer from the chat: open the session on its card.
    Dialog,
    Waiting,
    /// Why the agent cannot take the report, as the pop-up shows it.
    Failed(String),
}

/// The startup send's row, `None` once it left the queue.
pub(super) struct BriefRow<'a> {
    pub(super) state: &'a str,
    pub(super) error: Option<&'a str>,
}

/// Counts consecutive looks, so one odd screen neither fails nor opens anything.
#[derive(Default)]
pub(super) struct BriefWatch {
    failing: u32,
    dialog: u32,
}

impl BriefWatch {
    pub(super) fn look(
        &mut self,
        agent: &str,
        row: Option<BriefRow<'_>>,
        notice: Option<&SessionChatTerminalNotice>,
        waited: Duration,
    ) -> BriefStatus {
        let Some(row) = row else {
            return BriefStatus::Taken;
        };
        if row.state == SESSION_CHAT_QUEUE_STATE_FAILED {
            let reason = row.error.map(sentence).unwrap_or_default();
            return BriefStatus::Failed(if reason.is_empty() {
                format!("{agent} couldn't take your report. {SEND_WITHOUT}")
            } else {
                format!("{agent} couldn't take your report: {reason}. {SEND_WITHOUT}")
            });
        }
        let blocking = notice.filter(|notice| notice.blocks_input());
        let failure = blocking.and_then(|notice| start_failure(agent, notice));
        self.failing = if failure.is_some() {
            self.failing + 1
        } else {
            0
        };
        self.dialog = if blocking.is_some() && failure.is_none() {
            self.dialog + 1
        } else {
            0
        };
        if let Some(failure) = failure.filter(|_| self.failing >= FAILURE_LOOKS) {
            return BriefStatus::Failed(failure);
        }
        if self.dialog >= DIALOG_LOOKS {
            return BriefStatus::Dialog;
        }
        if waited >= BRIEF_WAIT {
            return BriefStatus::Failed(format!(
                "{agent} wasn't ready for your report after {} seconds. Try again in a moment. {SEND_WITHOUT}",
                BRIEF_WAIT.as_secs()
            ));
        }
        BriefStatus::Waiting
    }
}

/// The sentence for a screen that says the agent cannot start at all; `None` for a dialog.
fn start_failure(agent: &str, notice: &SessionChatTerminalNotice) -> Option<String> {
    match notice.kind.as_str() {
        SESSION_CHAT_NOTICE_LOGIN_EXPIRED => Some(format!(
            "{agent} needs you to sign in before it can start ({}). Sign in, then try again. {SEND_WITHOUT}",
            sentence(&notice.title)
        )),
        SESSION_CHAT_NOTICE_AGENT_EXITED => {
            let printed = notice
                .error_lines
                .as_deref()
                // Only a line that reads as an error is quoted: a killed start leaves the launcher's
                // own lines (an account switch naming its email) above the prompt instead.
                .and_then(|lines| {
                    lines
                        .lines()
                        .map(str::trim)
                        .filter(|line| reads_as_error(line))
                        .last()
                })
                .map(|line| format!(" It printed: {}.", sentence(line)))
                .unwrap_or_default();
            Some(format!(
                "{agent} couldn't start on this computer.{printed} Check that it is installed and signed in, then try again. {SEND_WITHOUT}"
            ))
        }
        _ => None,
    }
}

/// Whether a line the start left on screen says what went wrong.
fn reads_as_error(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    [
        "not found",
        "error",
        "no such",
        "denied",
        "failed",
        "cannot",
        "can't",
        "unable",
    ]
    .iter()
    .any(|word| line.contains(word))
}

/// `text` trimmed, without a closing full stop, to sit inside a sentence.
fn sentence(text: &str) -> String {
    text.trim().trim_end_matches('.').trim().to_string()
}

/// Waits until the new session's agent takes its brief. `Err` closes the session first.
pub(super) async fn wait_for_feedback_agent_brief(
    state: &AppState,
    started: &Value,
    agent_name: &str,
) -> Result<(), DomainStateError> {
    let text = |key: &str| started[key].as_str().unwrap_or_default().to_string();
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let outcome = if started["promptQueued"] == json!(false) {
        Err(format!(
            "Ghostex couldn't hand your report to {agent_name}: {}. {SEND_WITHOUT}",
            sentence(
                started["promptQueueError"]
                    .as_str()
                    .unwrap_or("unknown error")
            )
        ))
    } else {
        watch_brief(state, &project_id, &session_id, agent_name).await
    };
    let Err(message) = outcome else {
        return Ok(());
    };
    eprintln!("Feedback agent {project_id}/{session_id} did not take its brief: {message}");
    let close_state = state.clone();
    let _ = tokio::task::spawn_blocking(move || {
        close_feedback_session(&close_state, &project_id, &session_id)
    })
    .await;
    Err(DomainStateError {
        code: FEEDBACK_AGENT_NOT_READY,
        message,
    })
}

async fn watch_brief(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    agent_name: &str,
) -> Result<(), String> {
    let since = Instant::now();
    let mut watch = BriefWatch::default();
    loop {
        let snapshot = read_session_chat_queue_snapshot(&state.paths, project_id, session_id);
        let row = snapshot
            .queue
            .iter()
            .find(|prompt| prompt.startup_send)
            .map(|prompt| BriefRow {
                state: prompt.state.as_str(),
                error: prompt.error_message.as_deref(),
            });
        let notice = crate::session_chat_options::cached_session_chat_terminal_notice(
            state, project_id, session_id,
        );
        match watch.look(agent_name, row, notice.as_ref(), since.elapsed()) {
            BriefStatus::Taken | BriefStatus::Dialog => return Ok(()),
            BriefStatus::Failed(message) => return Err(message),
            BriefStatus::Waiting => tokio::time::sleep(LOOK_EVERY).await,
        }
    }
}

/// Closes the session the way a user's Close does, so its held brief is retired with it.
fn close_feedback_session(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(agent_settings) = crate::agents::read_agent_settings(&db) else {
        return;
    };
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("closeSource".to_string(), json!("server.feedbackFiled"));
    let outcome = match dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/killSession",
        &params,
        &super::agent::zmx_context(state),
        &agent_settings,
    ) {
        Ok(_) => "stopped",
        Err(error) => {
            eprintln!(
                "Could not close the feedback agent session {project_id}/{session_id}: {error:?}"
            );
            "failed"
        }
    };
    crate::session_close_log::log_session_close(
        &state.logger,
        &state.metadata.server_id,
        project_id,
        session_id,
        &params,
        "/api/killSession",
        outcome,
    );
    crate::session_chat_send_wake::retire_held_messages_of_closed_session(
        state, project_id, session_id,
    );
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat_notice::{
        SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
        SESSION_CHAT_NOTICE_TRUST_PROMPT,
    };

    const QUEUED: BriefRow<'static> = BriefRow {
        state: "queued",
        error: None,
    };

    fn notice(kind: &str, title: &str) -> SessionChatTerminalNotice {
        SessionChatTerminalNotice::new(
            kind,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Screen,
            title,
        )
    }

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    #[test]
    fn a_brief_that_left_the_queue_is_taken() {
        let mut watch = BriefWatch::default();
        assert_eq!(
            watch.look("Claude", None, None, secs(3)),
            BriefStatus::Taken
        );
    }

    #[test]
    fn a_quiet_start_waits_then_gives_up_in_plain_words() {
        let mut watch = BriefWatch::default();
        assert_eq!(
            watch.look("Claude", Some(QUEUED), None, secs(30)),
            BriefStatus::Waiting
        );
        let BriefStatus::Failed(message) = watch.look("Claude", Some(QUEUED), None, BRIEF_WAIT)
        else {
            panic!("a start that never got ready must fail");
        };
        assert!(message.starts_with("Claude wasn't ready for your report after 90 seconds."));
        assert!(message.ends_with(SEND_WITHOUT));
    }

    #[test]
    fn a_missing_agent_fails_once_the_screen_holds() {
        let exited = notice(SESSION_CHAT_NOTICE_AGENT_EXITED, "The agent couldn't start")
            .with_error_lines(Some(
                "Account-4 is already the active default login\nzsh: command not found: claude\n"
                    .to_string(),
            ));
        assert!(exited.blocks_input());
        let mut watch = BriefWatch::default();
        assert_eq!(
            watch.look("Claude", Some(QUEUED), Some(&exited), secs(2)),
            BriefStatus::Waiting
        );
        assert_eq!(
            watch.look("Claude", Some(QUEUED), Some(&exited), secs(3)),
            BriefStatus::Failed(format!(
                "Claude couldn't start on this computer. It printed: zsh: command not found: claude. Check that it is installed and signed in, then try again. {SEND_WITHOUT}"
            ))
        );
    }

    #[test]
    fn a_killed_start_quotes_no_launcher_chatter() {
        let exited = notice(SESSION_CHAT_NOTICE_AGENT_EXITED, "The agent couldn't start")
            .with_error_lines(Some(
                "Launching Account-5 (someone@example.com) [session mode]".to_string(),
            ));
        let mut watch = BriefWatch::default();
        watch.look("Claude", Some(QUEUED), Some(&exited), secs(2));
        assert_eq!(
            watch.look("Claude", Some(QUEUED), Some(&exited), secs(3)),
            BriefStatus::Failed(format!(
                "Claude couldn't start on this computer. Check that it is installed and signed in, then try again. {SEND_WITHOUT}"
            ))
        );
    }

    #[test]
    fn a_missing_login_fails_and_says_so() {
        let login = notice(
            SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
            "Claude couldn't start: account 5 has no saved login on this computer",
        );
        let mut watch = BriefWatch::default();
        watch.look("Claude", Some(QUEUED), Some(&login), secs(2));
        let BriefStatus::Failed(message) =
            watch.look("Claude", Some(QUEUED), Some(&login), secs(3))
        else {
            panic!("a missing login must fail");
        };
        assert!(message.starts_with("Claude needs you to sign in before it can start"));
    }

    #[test]
    fn one_odd_screen_does_not_fail_the_start() {
        let exited = notice(SESSION_CHAT_NOTICE_AGENT_EXITED, "The agent couldn't start");
        let mut watch = BriefWatch::default();
        watch.look("Claude", Some(QUEUED), Some(&exited), secs(2));
        assert_eq!(
            watch.look("Claude", Some(QUEUED), None, secs(3)),
            BriefStatus::Waiting
        );
        assert_eq!(
            watch.look("Claude", Some(QUEUED), Some(&exited), secs(4)),
            BriefStatus::Waiting
        );
    }

    #[test]
    fn a_dialog_the_user_can_answer_opens_the_session() {
        let trust = notice(SESSION_CHAT_NOTICE_TRUST_PROMPT, "Trust this folder?");
        assert!(trust.blocks_input());
        let mut watch = BriefWatch::default();
        for second in 1..DIALOG_LOOKS {
            assert_eq!(
                watch.look("Codex", Some(QUEUED), Some(&trust), secs(second.into())),
                BriefStatus::Waiting
            );
        }
        assert_eq!(
            watch.look("Codex", Some(QUEUED), Some(&trust), secs(5)),
            BriefStatus::Dialog
        );
    }

    #[test]
    fn a_refused_brief_fails_with_its_reason() {
        let failed = BriefRow {
            state: SESSION_CHAT_QUEUE_STATE_FAILED,
            error: Some("The session was closed."),
        };
        assert_eq!(
            BriefWatch::default().look("Codex", Some(failed), None, secs(1)),
            BriefStatus::Failed(format!(
                "Codex couldn't take your report: The session was closed. {SEND_WITHOUT}"
            ))
        );
    }
}
