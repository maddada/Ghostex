use std::time::Instant;

use super::*;

/// How long one reading of the agent's process stays good: each reading runs a process snapshot.
const PROCESS_READING_TTL: Duration = Duration::from_secs(20);

#[derive(Default)]
struct MidTurnReadings {
    transcript: SessionChatTranscriptGate,
    process: Option<(Instant, bool)>,
}

/// The probe a chat send carries in `SessionChatSendStep::WaitOutBusyAgent`
/// (session_chat_send/busy_input_wait.rs). Mid-turn means the agent's hooks, compaction or
/// transcript say it is working, no question or dialog is waiting on screen, and its process is
/// still running. A question the user answers in chat is typed by a job queued behind the waiting
/// send, so the wait must end as soon as one shows; a crashed agent is never waited for.
pub(crate) fn agent_mid_turn_probe(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    terminal_agent: Option<&str>,
) -> crate::session_chat_send::AgentMidTurnProbe {
    let state = state.clone();
    let (project_id, session_id, zmx_name) = (
        target.project_id.clone(),
        target.session_id.clone(),
        target.zmx_name.clone(),
    );
    let agent = terminal_agent.map(str::to_string);
    let readings = Arc::new(Mutex::new(MidTurnReadings::default()));
    crate::session_chat_send::AgentMidTurnProbe::new(move || {
        let (state, project_id, session_id, zmx_name, agent, readings) = (
            state.clone(),
            project_id.clone(),
            session_id.clone(),
            zmx_name.clone(),
            agent.clone(),
            readings.clone(),
        );
        Box::pin(async move {
            agent_mid_turn(
                state,
                project_id,
                session_id,
                &zmx_name,
                agent.as_deref(),
                &readings,
            )
            .await
        })
    })
}

async fn agent_mid_turn(
    state: AppState,
    project_id: String,
    session_id: String,
    zmx_name: &str,
    agent: Option<&str>,
    readings: &Arc<Mutex<MidTurnReadings>>,
) -> bool {
    let project_id_for_acp = project_id.clone();
    let session_id_for_acp = session_id.clone();
    let transcript_readings = readings.clone();
    let (working, is_acp) = tokio::task::spawn_blocking(move || {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        let Ok(target) = resolve_session_chat_send_target(&state, &params, "sendMidTurn") else {
            return (false, false);
        };
        let session = target.session;
        let is_acp = crate::acp::session_is_acp(&session);
        if crate::session_chat_send::transcript_pending_question_prompt(&session).is_some() {
            return (false, is_acp);
        }
        let working = crate::session_chat_follower::session_chat_hook_working(&session)
            || crate::session_chat_compacting::session_chat_compacting_detected_at(&session)
                .is_some()
            || transcript_readings
                .lock()
                .is_ok_and(|mut readings| readings.transcript.is_working(&session));
        (working, is_acp)
    })
    .await
    .unwrap_or((false, false));
    if !working {
        return false;
    }
    if is_acp {
        // Non-CLI sessions report pending approvals/questions and liveness over their structured
        // channel; never scrape their host PTY as a CLI terminal screen.
        return crate::acp::channel::channel_state(&project_id_for_acp, &session_id_for_acp)
            .is_some_and(|st| {
                st.get("pendingPermissionCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    == 0
                    && st.get("childExited").and_then(Value::as_bool) != Some(true)
            });
    }
    let dialog_on_screen = crate::session_chat_send::capture_session_terminal_text(zmx_name)
        .await
        .and_then(|screen| {
            crate::session_chat_notice::classify_session_chat_terminal_notice(agent, &screen)
        })
        .is_some_and(|notice| notice.is_answerable() || notice.blocks_queued_delivery());
    if dialog_on_screen {
        return false;
    }
    let cached = readings
        .lock()
        .ok()
        .and_then(|readings| readings.process)
        .filter(|(read_at, _)| read_at.elapsed() < PROCESS_READING_TTL);
    if let Some((_, running)) = cached {
        return running;
    }
    let running = crate::session_chat_send::session_agent_process_running(
        zmx_name,
        &crate::resume_lookup::home_dir(),
    )
    .await;
    if let Ok(mut readings) = readings.lock() {
        readings.process = Some((Instant::now(), running));
    }
    running
}
