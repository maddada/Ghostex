//! What gxserver does on its own when the agent's terminal refuses a chat message, before the
//! user ever sees a card: prove whether the agent is still alive, repaint a damaged Claude screen
//! and type the message once more, and restart a broken or exited agent on its own conversation
//! so the message waits for it as a startup send.

use std::{
    sync::OnceLock,
    time::{Duration as StdDuration, Instant},
};

use super::*;

/// One automatic restart per session in this window; a second refusal inside it gets the card,
/// so a cause a restart cannot fix (a broken hook, a missing tool) never loops.
const SEND_HEAL_RESTART_COOLDOWN: StdDuration = StdDuration::from_secs(10 * 60);
/// How long the repaint gives Claude's input box to come back before the message is typed again.
const SEND_HEAL_REPAINT_WAIT_MS: u64 = 4_000;

static SEND_HEAL_RESTARTS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

/// How a refused send ended after gxserver tried to recover it.
pub(crate) enum SendHeal {
    /// The message reached the agent: the transcript already had it, or the retry went through.
    Delivered,
    /// The agent is restarting on its own conversation; the message waits for its input box.
    Restarting,
    /// Nothing safe was left to try. The words say why (they also go to the send-failure log), and
    /// the message becomes a failed row whose card names them (session_chat_send_failure.rs).
    GaveUp(Option<String>),
}

/// CDXC:SessionChat 2026-10-09 DECISION:
/// User: "why do i see this dumb error as the user?? Please auto heal better! i shouldn't need to do anything! You fix it if you notice this error. Don't surface things like this to the user as much as possible." A send the terminal refused (paste never shown, doubled, kept after Enter, input box not cleared, terminal not answering) is recovered here before any card: the transcript is asked whether the message already arrived; a live Claude gets one Ctrl+L repaint and the message typed once more; a broken or exited agent is restarted on its own conversation (the sleep and wake a Full Reload does) and the message waits for its input box as a startup send. A card is shown only when none of that was safe or possible.
/// WHY: the 2026-10-09 report was a coordinator Claude that was alive, with its input box showing a stray `❯�` and "Press Ctrl-C again to exit" while Claude Code's hook errors filled the screen; every send failed until the user did a Full Reload. A live agent is restarted only when it is idle (hooks, transcript and compaction all quiet, no question waiting), has a conversation to resume, and was not restarted automatically in the last ten minutes, so a running turn is never killed and a cause a restart cannot fix ends in one card instead of a restart loop.
pub(super) async fn heal_refused_send(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    terminal_agent: Option<&str>,
    text: &str,
    retry_steps: Vec<crate::session_chat_send::SessionChatSendStep>,
    send_started_ms: i64,
    agent_stalled: bool,
    kept_in_box: bool,
) -> SendHeal {
    if paste_already_recorded(&target.session, text, send_started_ms).await {
        record(
            state,
            target,
            "sessionChatSendHealRecorded",
            "The agent's transcript already had the message; nothing was typed again.",
        );
        return SendHeal::Delivered;
    }
    let screen = crate::session_chat_send::capture_session_terminal_text(&target.zmx_name).await;
    let notice = screen.as_deref().and_then(|screen| {
        crate::session_chat_notice::classify_session_chat_terminal_notice(terminal_agent, screen)
    });
    let agent_exited_on_screen = notice.as_ref().is_some_and(|notice| {
        notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_AGENT_EXITED
    });
    // A login screen, a usage limit or a dialog explains itself, and neither a retry nor a restart
    // answers it; the card that names it is the useful outcome. One an account switch is hiding is
    // the previous login's repaint, not something to give up on.
    if notice.as_ref().is_some_and(|notice| {
        !agent_exited_on_screen
            && (notice.is_answerable() || notice.blocks_queued_delivery())
            && !crate::session_chat_notice::session_chat_notice_hidden(
                &target.project_id,
                &target.session_id,
                notice,
            )
    }) {
        record(
            state,
            target,
            "sessionChatSendHealGaveUp",
            "The terminal is showing something only the user can answer.",
        );
        let title = notice.as_ref().map(|notice| notice.title.clone());
        return SendHeal::GaveUp(title.map(|title| format!("The terminal is showing: {title}.")));
    }
    // The process tree decides, not the screen: an exit message can outlive the agent the user
    // already started again.
    let running = crate::session_chat_send::session_agent_process_running(
        &target.zmx_name,
        &crate::resume_lookup::home_dir(),
    )
    .await;
    // A turn that sat on the send's input for five minutes (session_chat_send/busy_input_wait.rs)
    // would take a Ctrl+L and a second copy just as late as the first.
    if running
        && !agent_stalled
        && crate::agents::identity::normalize_agent_id(terminal_agent).as_deref() == Some("claude")
        && repaint_claude(target, terminal_agent).await
    {
        let retried = crate::session_chat_send::execute_session_chat_send(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            "session-chat-message",
            with_single_paste_guard(
                with_paste_watch_of_at_least(retry_steps, PASTE_RETRY_WATCH_MS),
                terminal_agent,
            ),
        )
        .await;
        match retried {
            Ok(()) => {
                record(state, target, "sessionChatSendHealedAfterRepaint", "Claude's screen was repainted and the message was typed once more; it went through.");
                return SendHeal::Delivered;
            }
            Err(error) if error.cancelled() => return SendHeal::GaveUp(None),
            Err(_) => {}
        }
        if paste_already_recorded(&target.session, text, send_started_ms).await {
            return SendHeal::Delivered;
        }
    }
    // The message reached the input box, so the terminal and the agent are taking input; a
    // restart would cost the agent its background work without answering why Enter was kept.
    if running && kept_in_box {
        record(
            state,
            target,
            "sessionChatSendHealGaveUp",
            "The agent took the message into its input box but kept it there; it was not restarted.",
        );
        return SendHeal::GaveUp(None);
    }
    match restart_agent_for_send(state, target, running).await {
        Ok(()) => SendHeal::Restarting,
        Err(reason) => {
            record(state, target, "sessionChatSendHealGaveUp", reason);
            SendHeal::GaveUp(Some(reason.to_string()))
        }
    }
}

/// A send that finds the agent gone from its terminal (Codex after "Update now" or /logout, a
/// Claude that quit or crashed to the shell) restarts it on its own conversation and holds the
/// message for it, instead of refusing with "the input box is not on screen". Only the process
/// tree can say the agent is gone: Claude leaves no exit text to classify, just a shell prompt.
/// `None` leaves the send to the ordinary gates: the agent is still running, the session is still
/// starting or runs in a box, or the restart was not possible.
pub(super) async fn restart_exited_agent_before_send(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    detection: &crate::session_chat_options::SessionChatTerminalDetection,
    terminal_agent: Option<&str>,
    source: SessionChatMessageSource,
    text: &str,
    image_paths: &[String],
) -> Option<std::result::Result<usize, DomainStateError>> {
    if matches!(
        source,
        SessionChatMessageSource::AutomaticRecovery | SessionChatMessageSource::AccountSwitch(_)
    ) || crate::agentbox::is_agentbox_session(&target.session)
    {
        return None;
    }
    // CDXC:SessionChat 2026-10-31 WHY: an agent's own exit words are not always known: Antigravity CLI's `/exit` prints a resume hint and the shell prompt, which read as no notice, and a chat message was then typed at the shell ("zsh: command not found: Reply", reproduced 2026-10-31). A screen ending in a shell prompt is no agent's input box, whichever agent it was; the process tree below still has the last word.
    let exit_on_screen = detection.notice.as_ref().is_some_and(|notice| {
        notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_AGENT_EXITED
    })
        || crate::session_chat_notice::session_chat_screen_ends_with_shell_prompt(
            &detection.composer.screen_tail.join("\n"),
        );
    // Claude and Codex are the agents whose process the snapshot is known to name on every
    // platform; any other agent needs its own exit screen before its absence is believed.
    let input_box_missing = detection.captured
        && detection.notice.is_none()
        && detection.composer.blocks_message_for(terminal_agent)
        && matches!(
            crate::agents::identity::normalize_agent_id(terminal_agent).as_deref(),
            Some("claude" | "codex")
        );
    let starting =
        crate::session_chat_send_wake::screen_shows_agent_starting(&detection.composer.screen_tail);
    if !(exit_on_screen || input_box_missing) || starting {
        return None;
    }
    if crate::session_chat_send::session_agent_process_running(
        &target.zmx_name,
        &crate::resume_lookup::home_dir(),
    )
    .await
    {
        return None;
    }
    // The agent is gone and cannot be brought back, so the ordinary gates could only hold the
    // message behind an input box that will never come, with its spinner turning for good.
    if let Err(reason) = restart_agent_for_send(state, target, false).await {
        record(state, target, "sessionChatSendHealGaveUp", reason);
        return Some(Err(DomainStateError {
            code: "dependencyUnavailable",
            message: crate::session_chat_send_failure::send_failure_reason(
                "The agent in this session is not running, so the message was not sent.",
                Some(reason),
            ),
        }));
    }
    Some(hold_for_restarted_agent(
        state,
        target,
        source,
        text,
        image_paths,
    ))
}

/// Ctrl+L makes Claude redraw itself whole (session_chat_composer_repaint.rs); a repaint is
/// worth a retry only when the input box comes back ready afterwards.
async fn repaint_claude(
    target: &crate::session_chat_send::SessionChatSendTarget,
    agent: Option<&str>,
) -> bool {
    if crate::session_chat_send::write_session_chat_payload(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-heal",
        "\u{c}",
    )
    .await
    .is_err()
    {
        return false;
    }
    crate::session_chat_composer::wait_for_session_chat_composer(
        &target.zmx_name,
        agent,
        crate::session_chat_composer::SessionChatComposerWaitPolicy {
            settle_ms: 300,
            timeout_ms: SEND_HEAL_REPAINT_WAIT_MS,
            unknown_hold_ms: 0,
        },
        &|| false,
    )
    .await
        == crate::session_chat_composer::SessionChatComposerWait::Ready
}

/// Sleeps and wakes the session, which resumes the agent on its own conversation: what the
/// user's Full Reload did. Refused for a live agent that is doing anything.
pub(crate) async fn restart_agent_for_send(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    agent_running: bool,
) -> Result<(), &'static str> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(target.project_id));
    params.insert("sessionId".to_string(), json!(target.session_id));
    let session = resolve_session_chat_send_target(state, &params, "sendHeal")
        .map(|current| current.session)
        .map_err(|_| "the session could not be read")?;
    if !can_start_on_its_conversation(state, &session).await {
        return Err("the agent has no conversation to resume");
    }
    if agent_running && agent_is_busy(&session).await {
        return Err("the agent is in the middle of a turn");
    }
    if agent_running {
        if let Some(reason) = work_a_restart_would_end(state, target).await {
            return Err(reason);
        }
    }
    let key = format!("{}:{}", target.project_id, target.session_id);
    {
        let restarts = SEND_HEAL_RESTARTS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut restarts) = restarts.lock() else {
            return Err("the restart record could not be read");
        };
        restarts.retain(|_, at| at.elapsed() < SEND_HEAL_RESTART_COOLDOWN);
        if restarts.contains_key(&key) {
            return Err("the agent was already restarted automatically a few minutes ago");
        }
        restarts.insert(key, Instant::now());
    }
    cycle_session(state, &target.project_id, &target.session_id).await?;
    record(
        state,
        target,
        "sessionChatSendRestartedAgent",
        if agent_running {
            "The agent was running but its terminal kept refusing the message; restarted it on its own conversation."
        } else {
            "The agent was no longer running; restarted it on its own conversation."
        },
    );
    Ok(())
}

/// Sleep, then wake: the order the sidebar's Full Reload uses. The screen readings of the old
/// process are dropped so the queue waits for the new input box rather than the broken one.
pub(crate) async fn cycle_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(), &'static str> {
    let body = json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    for path in ["/api/sleepSession", "/api/wakeSession"] {
        let ok = crate::server::handle_zmx_lifecycle_http(
            state,
            path.to_string(),
            uuid::Uuid::new_v4().to_string(),
            &body,
        )
        .await
        .response
        .status()
        .is_success();
        crate::session_chat_options::forget_session_chat_options(state, project_id, session_id);
        if !ok {
            return Err(if path == "/api/sleepSession" {
                "the session could not be put to sleep"
            } else {
                "the session could not be started again"
            });
        }
    }
    Ok(())
}

/// Whether a wake brings the agent back on this session's conversation. Claude and Pi need only
/// the session id: a wake resumes a conversation that was written and starts a fresh one under
/// the same id otherwise (agents/resume_plan.rs), since a session with no conversation yet has
/// nothing to lose. Any other agent resumes by id alone, so its transcript must exist; on
/// 2026-10-09 a wake that resumed an unwritten Claude conversation printed "No conversation found"
/// and went back to the shell.
pub(crate) async fn has_conversation_to_resume(session: &Value) -> bool {
    let Some(agent_session_id) = read_runtime_text(session, "agentSessionId") else {
        return false;
    };
    let agent = session_chat_agent_for_session(session);
    if matches!(
        crate::agents::identity::normalize_agent_id(agent.as_deref()).as_deref(),
        Some("claude" | "pi")
    ) {
        return true;
    }
    let Some(transcript_agent) = resolve_session_chat_transcript_agent(agent.as_deref()) else {
        return true;
    };
    let agent_session_path = read_runtime_text(session, "agentSessionPath");
    tokio::task::spawn_blocking(move || {
        resolve_session_chat_transcript_path(
            transcript_agent,
            Some(&agent_session_id),
            agent_session_path.as_deref(),
        )
        .is_some_and(|path| path.is_file())
    })
    .await
    .unwrap_or(false)
}

/// Whether a restart of this session is safe: it resumes a conversation, or the wake's own rule
/// (`agent_conversation_written`, GitHub #366) says nobody ever prompted this agent, so it starts
/// fresh with nothing to lose. A Codex that was never prompted has no rollout to find, and calling
/// that "no conversation to resume" left its first message stuck.
pub(crate) async fn can_start_on_its_conversation(state: &AppState, session: &Value) -> bool {
    if has_conversation_to_resume(session).await {
        return true;
    }
    let Some(project_id) = session.get("projectId").and_then(Value::as_str) else {
        return false;
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let settings = crate::agents::read_agent_settings(&db)
        .unwrap_or_else(|_| crate::agents::normalize_agent_settings(None));
    let project = DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_project(project_id)
        .ok()
        .flatten();
    project.is_some_and(|project| {
        crate::agents::agent_conversation_written(&project, session, &settings) == Some(false)
    })
}

/// CDXC:SessionChat 2026-10-10 WHY:
/// On 2026-10-09 a send refused by a mastro_ed orchestrator restarted it on its conversation, which ended its 7 background tasks and monitors while 11 of its threads were still open; an idle turn says nothing about work that runs between turns. A live agent that coordinates threads, or whose Claude footer counts background tasks (`⧉ N`), is never restarted automatically; the send gets its card instead.
pub(crate) async fn work_a_restart_would_end(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
) -> Option<&'static str> {
    let coordinating = open_gxserver_database(&state.paths).ok().is_some_and(|db| {
        crate::coordinators::read_coordinator(&db, &target.project_id, &target.session_id)
            .ok()
            .flatten()
            .is_some()
            || crate::coordinators::list_threads_for(&db, &target.project_id, &target.session_id)
                .is_ok_and(|threads| threads.iter().any(|thread| !thread.is_resolved()))
    });
    if coordinating {
        return Some("the agent coordinates threads, and a restart would end its background work");
    }
    let screen = crate::session_chat_send::capture_session_terminal_text(&target.zmx_name).await;
    if screen.as_deref().is_some_and(shows_claude_background_tasks) {
        return Some("the agent has background tasks a restart would end");
    }
    None
}

/// Claude's footer counts the session's running background tasks as `⧉ N`
/// (`⏵⏵ bypass permissions on (shift+tab to cycle) · ⧉ 7 · ← for agents`).
fn shows_claude_background_tasks(screen: &str) -> bool {
    screen.lines().rev().take(8).any(|line| {
        line.split('·').any(|part| {
            part.trim()
                .strip_prefix('⧉')
                .and_then(|count| count.trim().parse::<u32>().ok())
                .is_some_and(|count| count > 0)
        })
    })
}

/// Whether a live agent is doing anything a restart would cut short.
pub(crate) async fn agent_is_busy(session: &Value) -> bool {
    if crate::session_chat_follower::session_chat_hook_working(session)
        || crate::session_chat_compacting::session_chat_compacting_detected_at(session).is_some()
    {
        return true;
    }
    let session = session.clone();
    tokio::task::spawn_blocking(move || {
        crate::session_chat_send::transcript_pending_question_prompt(&session).is_some()
            || SessionChatTranscriptGate::default().is_working(&session)
    })
    .await
    .unwrap_or(true)
}

fn record(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    event: &str,
    reason: &str,
) {
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        event,
        &target.project_id,
        &target.session_id,
        reason,
        &[],
    );
}

/// The refusals the heal takes on: the terminal refused or never showed the message, or the
/// input box could not be cleared. Account switches and their recovery sends run their own
/// restart, and Empryo's uncleared box is an attached image only the user can remove.
pub(super) fn heals_refused_send(
    error: &crate::session_chat_send::SessionChatSendError,
    source: SessionChatMessageSource,
    agent: Option<&str>,
) -> bool {
    if matches!(
        source,
        SessionChatMessageSource::AutomaticRecovery | SessionChatMessageSource::AccountSwitch(_)
    ) || error.cancelled()
    {
        return false;
    }
    error.terminal_refused()
        || (error.failure == crate::session_chat_send::SessionChatSendFailure::ComposerNotCleared
            && crate::agents::identity::normalize_agent_id(agent).as_deref() != Some("empryo"))
}

/// Where a refused message waits while its agent restarts. A composer send answers
/// `sessionStarting`, which the send endpoint turns into a startup send (session_chat_send_wake.rs);
/// a queued row goes back to the queue as one; a direct send with no row (a Slack request) gets a
/// startup send of its own. Each is typed once, when the new input box appears.
pub(super) fn hold_for_restarted_agent(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    source: SessionChatMessageSource,
    text: &str,
    image_paths: &[String],
) -> std::result::Result<usize, DomainStateError> {
    let restarting = || DomainStateError {
        code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
        message: "The agent is restarting; the message will be sent as soon as it is ready."
            .to_string(),
    };
    if source == SessionChatMessageSource::Composer
        || crate::session_chat_queue::hold_sending_prompt_for_restart(
            &state.paths,
            &target.project_id,
            &target.session_id,
        )
    {
        return Err(restarting());
    }
    queue_restart_startup_send(state, target, text, image_paths)?;
    Ok(text.len())
}

/// A startup send carrying `text` and its pictures, typed by the queue once the restarted agent's
/// input box appears.
fn queue_restart_startup_send(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    text: &str,
    image_paths: &[String],
) -> std::result::Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(target.project_id));
    params.insert("sessionId".to_string(), json!(target.session_id));
    params.insert("text".to_string(), json!(text));
    params.insert("startupSend".to_string(), json!(true));
    if !image_paths.is_empty() {
        params.insert("imagePaths".to_string(), json!(image_paths));
    }
    crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &params,
    )?;
    broadcast_session_chat_queue_state(state, &target.project_id, &target.session_id);
    Ok(())
}

/// The delivery watchdog's way out when it finds the agent gone under a message it typed: the
/// transcript never recorded the message and the agent is dead, so it cannot have arrived; the
/// agent is restarted on its own conversation and the message is held for it.
pub(super) fn exited_agent_healer(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    text: &str,
) -> crate::session_chat_watchdog::SessionChatSendHealer {
    let state = state.clone();
    let (project_id, session_id, zmx_name) = (
        target.project_id.clone(),
        target.session_id.clone(),
        target.zmx_name.clone(),
    );
    let session = target.session.clone();
    let text = text.to_string();
    Arc::new(move || {
        let state = state.clone();
        let target = crate::session_chat_send::SessionChatSendTarget {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            zmx_name: zmx_name.clone(),
            session: session.clone(),
        };
        let text = text.clone();
        Box::pin(async move {
            if let Err(reason) = restart_agent_for_send(&state, &target, false).await {
                record(&state, &target, "sessionChatSendHealGaveUp", reason);
                let reason = crate::session_chat_send_failure::send_failure_reason(
                    "The agent in this session stopped before it took the message.",
                    Some(reason),
                );
                return crate::session_chat_send_wake::queue_failed_send(
                    &state, &target, &text, &reason,
                )
                .is_ok();
            }
            queue_restart_startup_send(&state, &target, &text, &[]).is_ok()
        })
    })
}
