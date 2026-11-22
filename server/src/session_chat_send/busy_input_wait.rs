use std::{future::Future, pin::Pin};

use super::*;

/// The longest a send keeps the input line while it waits for a mid-turn agent to show its paste,
/// take its Return or clear its input box.
const MID_TURN_INPUT_WAIT_MAX: Duration = Duration::from_secs(5 * 60);
/// How often the wait asks again whether the turn is still running.
const MID_TURN_RECHECK: Duration = Duration::from_secs(3);
/// Once the turn has ended, how long the input box gets to catch up with input it is still
/// taking in.
const TURN_END_CATCH_UP: Duration = Duration::from_secs(6);
const MID_TURN_POLL: Duration = Duration::from_millis(500);

type MidTurnProbeFn = dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync;

/// Answers whether the session's agent is in the middle of a turn
/// (session_chat_queue_runtime/mid_turn.rs). Carried into a send job by
/// `SessionChatSendStep::WaitOutBusyAgent`.
#[derive(Clone)]
pub struct AgentMidTurnProbe(Arc<MidTurnProbeFn>);

impl AgentMidTurnProbe {
    pub(crate) fn new(
        probe: impl Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(probe))
    }

    pub(crate) async fn mid_turn(&self) -> bool {
        (self.0)().await
    }
}

impl std::fmt::Debug for AgentMidTurnProbe {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AgentMidTurnProbe")
    }
}

impl PartialEq for AgentMidTurnProbe {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for AgentMidTurnProbe {}

/// The send's answer when a mid-turn agent never caught up.
pub(crate) const SESSION_CHAT_BUSY_AGENT_STALLED: &str =
    "The agent stayed in the middle of a turn for five minutes without taking new input, so the message was not sent.";

/// How waiting out a busy agent ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BusyAgentWait {
    /// The screen caught up.
    Settled,
    /// The agent was not mid-turn, so nothing was waited for; the send keeps its usual checks.
    NotMidTurn,
    /// The turn ended and the input box still had not caught up a few seconds later.
    TurnEnded,
    /// Five minutes passed with the agent still mid-turn.
    Stalled,
    Cancelled,
}

/// CDXC:SessionChat 2026-10-10 WHY:
/// On 2026-10-09 a 5.7 KB thread report reached the coordinator's Claude mid-turn on a loaded Windows machine whose Claude screen had stopped updating (its turn timer stayed at 26s for the next minute). The paste check gave up after 12 s, and the late watch, the clear-and-retype and the heal's repaint-and-retype each typed the message again while the first copy was still on its way; the next sends' Ctrl+C clears queued up behind them. The copies landed in Claude's input box a minute later with no Return, the user saw "not taking messages" for an agent that was working, and earlier that day another agent's message, typed between two of those jobs, had been glued onto such stranded text. Input written to a live agent's terminal is late, not lost, so while the agent is mid-turn a send keeps the input line and waits for its own clear, paste and Return to show on screen, writing nothing more, for up to five minutes.
///
/// Polls `settled` (a screen read) until it answers true.
pub(crate) async fn wait_out_busy_agent<F, Fut>(
    probe: &AgentMidTurnProbe,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
    mut settled: F,
) -> BusyAgentWait
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    if !probe.mid_turn().await {
        return BusyAgentWait::NotMidTurn;
    }
    let started = Instant::now();
    let mut recheck_at = started + MID_TURN_RECHECK;
    let mut turn_ended_at: Option<Instant> = None;
    loop {
        if cancelled() {
            return BusyAgentWait::Cancelled;
        }
        if settled().await {
            return BusyAgentWait::Settled;
        }
        let now = Instant::now();
        match turn_ended_at {
            Some(ended) if now >= ended + TURN_END_CATCH_UP => return BusyAgentWait::TurnEnded,
            Some(_) => {}
            None if now >= started + MID_TURN_INPUT_WAIT_MAX => return BusyAgentWait::Stalled,
            None if now >= recheck_at => {
                if probe.mid_turn().await {
                    recheck_at = Instant::now() + MID_TURN_RECHECK;
                } else {
                    turn_ended_at = Some(Instant::now());
                }
            }
            None => {}
        }
        tokio::time::sleep(MID_TURN_POLL).await;
    }
}

/// Whether the agent's input box reads empty right now (a single capture; an unreadable screen or
/// a missing input box is not empty).
pub(crate) async fn composer_reads_empty(zmx_name: &str, agent: &str) -> bool {
    capture_session_terminal_text_vt(zmx_name)
        .await
        .and_then(|screen| {
            crate::session_chat_composer::session_chat_composer_input(agent, &screen)
        })
        .is_some_and(|input| input.is_empty() && !input.shell_mode)
}

pub(crate) fn busy_agent_stalled() -> SessionChatSendError {
    SessionChatSendError::new(
        SessionChatSendFailure::Write,
        SESSION_CHAT_BUSY_AGENT_STALLED.to_string(),
    )
}

pub(crate) fn record_busy_agent_wait(project_id: &str, session_id: &str, what: &str) {
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendWaitedForBusyAgent",
        project_id,
        session_id,
        &format!("The agent was in the middle of a turn and slow to show input; the send waited until {what}, typing nothing more."),
        &[],
    );
}
