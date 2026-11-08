//! What gxserver does on its own when the agent's terminal refuses a chat message, before the
//! user ever sees a card: prove whether the agent is still alive, repaint a damaged Claude screen
//! and type the message once more, and restart a broken or exited agent on its own conversation
//! so the message waits for it as a startup send.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    time::{Duration as StdDuration, Instant},
};

use super::*;

/// One automatic restart per session in this window; a second refusal inside it gets the card,
/// so a cause a restart cannot fix (a broken hook, a missing tool) never loops.
const SEND_HEAL_RESTART_COOLDOWN: StdDuration = StdDuration::from_secs(10 * 60);
/// How long the repaint gives Claude's input box to come back before the message is typed again.
const SEND_HEAL_REPAINT_WAIT_MS: u64 = 4_000;

static SEND_HEAL_RESTARTS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
/// How many automatic restarts in a row may run the same wake on an agent that is gone, with its
/// conversation unchanged in between, before the next one is refused for good.
const MAX_SAME_DEAD_RESTARTS: u32 = 3;
/// Per session: what the last automatic restart ran (`wake_fingerprint`) and how many restarts in
/// a row ran it on an agent that was gone again.
static SEND_HEAL_SAME_RESTARTS: OnceLock<Mutex<HashMap<String, (String, u32)>>> = OnceLock::new();

/// A refused restart that waiting cannot change: the held message is marked Not sent with Retry.
pub(crate) const RESTART_SAME_RESUME_FAILED: &str =
    "the agent could not find its conversation, and restarting it the same way would fail again";
/// See `RESTART_SAME_RESUME_FAILED`.
pub(crate) const RESTART_REPEATED_WITHOUT_AGENT: &str =
    "restarting the agent the same way did not bring it back after 3 tries";

/// Whether `reason` (from `restart_agent_for_send`) means no later automatic restart will run.
pub(crate) fn restart_refusal_is_final(reason: &str) -> bool {
    reason == RESTART_SAME_RESUME_FAILED || reason == RESTART_REPEATED_WITHOUT_AGENT
}

/// The user's Retry on a Not sent row gives the automatic restart a fresh count.
pub(crate) fn forget_repeated_restarts(project_id: &str, session_id: &str) {
    if let Some(restarts) = SEND_HEAL_SAME_RESTARTS.get() {
        if let Ok(mut restarts) = restarts.lock() {
            restarts.remove(&format!("{project_id}:{session_id}"));
        }
    }
}

/// How a refused send ended after gxserver tried to recover it.
pub(crate) enum SendHeal {
    /// The message reached the agent: the transcript already had it, or the retry went through.
    Delivered,
    /// The agent is restarting on its own conversation; the message waits for its input box.
    Restarting,
    /// gxserver cannot reach its own session tools (zmx); the message waits until they are back.
    ToolsMissing,
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
    if gxserver_tools_missing(state, target).await {
        return SendHeal::ToolsMissing;
    }
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
    // GitHub #456: the last wake's resume could not find its conversation (resume_missing.rs).
    let rejected = if agent_running {
        None
    } else {
        match missing_resume_restart(state, target, &session).await {
            Some(MissingResumeRestart::GiveUp) => return Err(RESTART_SAME_RESUME_FAILED),
            Some(MissingResumeRestart::Reject(reference)) => Some(reference),
            Some(MissingResumeRestart::Restart) | None => None,
        }
    };
    if let Some(reference) = rejected {
        return restart_without_missing_resume(state, target, &reference).await;
    }
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
    let fingerprint = if agent_running {
        None
    } else {
        wake_fingerprint(state, &session).await
    };
    claim_automatic_restart(target, fingerprint)?;
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

/// Takes the session's one automatic restart per [`SEND_HEAL_RESTART_COOLDOWN`], refused when a
/// restart already ran in it or the same restart of a gone agent keeps repeating.
fn claim_automatic_restart(
    target: &crate::session_chat_send::SessionChatSendTarget,
    fingerprint: Option<String>,
) -> Result<(), &'static str> {
    let key = format!("{}:{}", target.project_id, target.session_id);
    let restarts = SEND_HEAL_RESTARTS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut restarts) = restarts.lock() else {
        return Err("the restart record could not be read");
    };
    restarts.retain(|_, at| at.elapsed() < SEND_HEAL_RESTART_COOLDOWN);
    if restarts.contains_key(&key) {
        return Err("the agent was already restarted automatically a few minutes ago");
    }
    if !count_same_restart(&key, fingerprint) {
        return Err(RESTART_REPEATED_WITHOUT_AGENT);
    }
    restarts.insert(key, Instant::now());
    Ok(())
}

/// CDXC:SessionChat 2026-11-08 DECISION:
/// User, after five thread reports sat "Waiting for agent" on an idle coordinator for four hours:
/// "had to do full reload to wake you up and get these moving.. wtf? Please fix". A message held
/// because the agent's input box is not on screen, on an agent idle by its hooks and transcript,
/// is healed by the Full Reload the user did (sleep, then wake on the same conversation) once the
/// fresh reads, Ctrl+L and resize of the input-box ladder have not brought the box back
/// (held_reload.rs). Unlike a refused send's restart, a coordinator is reloaded too: the user's
/// own fix was a coordinator's reload, its threads run in their own sessions and go on, and a
/// coordinator that cannot take their reports is stuck anyway. Background tasks Claude's footer
/// counts (`· 1 shell`, `⧉ N`) or its transcript shows still running (`transcript_background_jobs`) refuse it, as do a running turn, compaction, a pending question and the
/// shared ten-minute limit on automatic restarts.
pub(crate) async fn reload_idle_agent_for_input_box(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
) -> Result<(), ReloadRefusal> {
    let passing = |reason: String| ReloadRefusal {
        reason,
        lasting: false,
    };
    let session = &target.session;
    if !can_start_on_its_conversation(state, session).await {
        return Err(ReloadRefusal {
            reason: "the agent has no conversation to resume".to_string(),
            lasting: true,
        });
    }
    if agent_is_busy(session).await {
        return Err(passing("the agent is in the middle of a turn".to_string()));
    }
    let screen = crate::session_chat_send::capture_session_terminal_text(&target.zmx_name).await;
    if screen.as_deref().is_some_and(shows_claude_background_tasks) {
        return Err(passing(
            "the agent has background tasks a reload would end".to_string(),
        ));
    }
    let jobs = transcript_background_jobs(session).await;
    if !jobs.is_empty() {
        let names = jobs
            .iter()
            .map(|job| format!("{} ({})", job.title, job.id))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(passing(format!(
            "its transcript shows {} background job{} still running a reload would end: {names}",
            jobs.len(),
            if jobs.len() == 1 { "" } else { "s" }
        )));
    }
    claim_automatic_restart(target, None).map_err(|reason| passing(reason.to_string()))?;
    // A sleep or wake that failed is not retried on its own: the stuck words and bell take over.
    cycle_session(state, &target.project_id, &target.session_id)
        .await
        .map_err(|reason| ReloadRefusal {
            reason: reason.to_string(),
            lasting: true,
        })
}

/// Why an input-box reload did not run; a `lasting` one is not tried again for the same message.
pub(crate) struct ReloadRefusal {
    pub(crate) reason: String,
    pub(crate) lasting: bool,
}

/// CDXC:SessionChat 2026-11-07 WHY:
/// GitHub #456 asked that no restart loop run forever. A restart whose agent never came up (it
/// failed in a way no screen rule names) was retried every ten minutes for as long as a message
/// waited. Each automatic restart of an agent that is gone records what the wake will run and how
/// big the conversation is; the fourth one in a row with both unchanged (so no message got
/// through in between) is refused for good and the message is marked Not sent with Retry. A
/// restart of a live agent, a different wake command, a conversation that grew, or the user's
/// Retry starts the count again. Returns false when the restart must not run.
fn count_same_restart(key: &str, fingerprint: Option<String>) -> bool {
    let restarts = SEND_HEAL_SAME_RESTARTS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut restarts) = restarts.lock() else {
        return true;
    };
    let Some(fingerprint) = fingerprint else {
        restarts.remove(key);
        return true;
    };
    let count = match restarts.get(key) {
        Some((last, count)) if *last == fingerprint => count + 1,
        _ => 1,
    };
    if count > MAX_SAME_DEAD_RESTARTS {
        return false;
    }
    restarts.insert(key.to_string(), (fingerprint, count));
    true
}

/// What a wake of `session` runs, and how big its conversation is: two restarts with the same
/// fingerprint ran the same command and nothing reached the agent in between.
async fn wake_fingerprint(state: &AppState, session: &Value) -> Option<String> {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let session = session.clone();
    tokio::task::spawn_blocking(move || {
        let project_id = session.get("projectId").and_then(Value::as_str)?;
        let db = open_gxserver_database(&paths).ok()?;
        let settings = crate::agents::read_agent_settings(&db)
            .unwrap_or_else(|_| crate::agents::normalize_agent_settings(None));
        let project = DomainRepository::new(&db, server_id.as_str())
            .get_project(project_id)
            .ok()??;
        let plan = crate::agents::build_agent_resume_plan(&project, &session, &settings);
        let command = plan
            .get("primaryCommand")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let agent = session_chat_agent_for_session(&session);
        let size = resolve_session_chat_transcript_agent(agent.as_deref())
            .and_then(|transcript_agent| {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    read_runtime_text(&session, "agentSessionId").as_deref(),
                    read_runtime_text(&session, "agentSessionPath").as_deref(),
                )
            })
            .and_then(|path| std::fs::metadata(path).ok())
            .map_or(0, |metadata| metadata.len());
        Some(format!("{command}\u{0}{size}"))
    })
    .await
    .ok()
    .flatten()
}

/// What the screen of an agent that is gone says about the conversation its last wake tried to
/// resume, and what a restart does about it (resume_missing.rs). `None` when it says nothing.
async fn missing_resume_restart(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    session: &Value,
) -> Option<MissingResumeRestart> {
    // The CLI in the terminal: `session_chat_agent_for_session` names the transcript family, which
    // is Pi for an OMP session.
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
    let screen = crate::session_chat_send::capture_session_terminal_text(&target.zmx_name).await?;
    let missing = missing_resume_on_screen(agent.as_deref(), &screen)?;
    let runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let rejected = rejected_resume_references(&runtime);
    let id = read_runtime_text(session, "agentSessionId");
    let path = read_runtime_text(session, "agentSessionPath");
    let stored: Vec<&str> = [id.as_deref(), path.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let written = conversation_written(state, session).await;
    let decision =
        restart_after_missing_resume(agent.as_deref(), &missing, &stored, &rejected, written);
    record(
        state,
        target,
        "sessionChatResumeMissing",
        &format!(
            "The agent could not find the conversation it was asked to resume ({}); {}.",
            missing.reference.as_deref().unwrap_or("no reference named"),
            match &decision {
                MissingResumeRestart::Reject(_) =>
                    "it is set aside: the agent reopens its saved file if there is one, or starts fresh",
                MissingResumeRestart::Restart => "the next start does not use it",
                MissingResumeRestart::GiveUp =>
                    "the same restart would fail again, so it was not run",
            }
        ),
    );
    Some(decision)
}

/// The wake's own transcript rule for Claude and Codex (`agent_conversation_written`).
async fn conversation_written(state: &AppState, session: &Value) -> Option<bool> {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let session = session.clone();
    tokio::task::spawn_blocking(move || {
        let project_id = session.get("projectId").and_then(Value::as_str)?;
        let db = open_gxserver_database(&paths).ok()?;
        let settings = crate::agents::read_agent_settings(&db)
            .unwrap_or_else(|_| crate::agents::normalize_agent_settings(None));
        let project = DomainRepository::new(&db, server_id.as_str())
            .get_project(project_id)
            .ok()??;
        crate::agents::agent_conversation_written(&project, &session, &settings)
    })
    .await
    .ok()
    .flatten()
}

/// Sets `reference` aside on the session (`agentResumeRejected`) and restarts it, so the wake
/// reopens the conversation another way or starts the agent fresh. It is a different command
/// from the one that failed, so the ten-minute cooldown does not hold it back; each reference is
/// set aside once, so this cannot loop.
async fn restart_without_missing_resume(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    reference: &str,
) -> Result<(), &'static str> {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
    let reference_owned = reference.to_string();
    tokio::task::spawn_blocking(move || -> Option<()> {
        let db = open_gxserver_database(&paths).ok()?;
        let repository = DomainRepository::new(&db, server_id.as_str());
        let session = repository.get_session(&project_id, &session_id).ok()??;
        let mut runtime = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        with_rejected_resume_reference(&mut runtime, &reference_owned);
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(session_id));
        update.insert("runtimeSettings".to_string(), Value::Object(runtime));
        repository.update_session(&update).ok().map(|_| ())
    })
    .await
    .ok()
    .flatten()
    .ok_or("the session could not be updated")?;
    let key = format!("{}:{}", target.project_id, target.session_id);
    if let Ok(mut restarts) = SEND_HEAL_RESTARTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        restarts.insert(key.clone(), Instant::now());
    }
    forget_repeated_restarts(&target.project_id, &target.session_id);
    cycle_session(state, &target.project_id, &target.session_id).await?;
    record(
        state,
        target,
        "sessionChatSendRestartedAgent",
        "The agent could not find its conversation by that reference; restarted it without it (its saved file if there is one, or fresh), so the message goes to its next start.",
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
    if !transcript_background_jobs(&target.session).await.is_empty() {
        return Some("the agent's transcript shows background jobs still running");
    }
    None
}

/// CDXC:SessionChat 2026-11-08 WHY:
/// Coordinator, after the input-box reload landed: "before reloading any session, also read its
/// transcript for background jobs still running (Claude's background tasks / bash_id entries that
/// haven't reported completion), not only the on-screen footer". The footer's count is what a
/// reload checked, and the screens that need a reload are the ones whose bottom rows (that
/// footer) are cut off. Claude's transcript names every background shell and monitor it started
/// in this process and every one that ended (session_chat_claude_background.rs); one started and
/// not ended refuses the automatic restart, and the input-box reload tries again later.
/// BLOCKING read on a blocking thread. Claude's only: other agents' transcripts name no jobs.
async fn transcript_background_jobs(
    session: &Value,
) -> Vec<crate::session_chat_terminal_activity::SessionChatBackgroundTask> {
    if !matches!(
        crate::agents::identity::normalize_agent_id(
            session_chat_agent_for_session(session).as_deref()
        )
        .as_deref(),
        Some("claude" | "openclaude")
    ) {
        return Vec::new();
    }
    let session = session.clone();
    tokio::task::spawn_blocking(move || {
        crate::session_chat_claude_fleet::read_claude_background_tasks(&session, u64::MAX)
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

/// Claude's screen shows background shells or monitors still running: its footer (`· 1 shell`,
/// `· 2 shells, 1 monitor`, or the older `· ⧉ 7`) or its turn-end row (`… · 1 shell still
/// running`), read by the same recognisers as the chat's background strip and the grey dot
/// (CDXC:SessionStatus 2026-11-08 at `claude_footer_background_activity`).
fn shows_claude_background_tasks(screen: &str) -> bool {
    crate::session_chat_terminal_activity::claude_footer_background_activity(screen).is_some()
        || crate::session_chat_terminal_activity::is_session_chat_background_work_activity(
            crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                Some("claude"),
                screen,
            )
            .as_ref(),
        )
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
    hold_as_startup_send(
        state,
        target,
        source,
        text,
        image_paths,
        "The agent is restarting; the message will be sent as soon as it is ready.",
    )
}

/// CDXC:Zmx 2026-11-05 DECISION:
/// User: "a send that fails because gxserver can't reach its own tools (zmx missing and similar)
/// stays queued and retries by itself instead of offering an agent restart". On 2026-11-05 a
/// gxserver that could not find its bundled zmx from 22:41 to 22:44 refused a coordinator's
/// message as a terminal that did not answer, the heal then refused to restart the coordinator,
/// and the message was Not sent for good. A restart cannot bring zmx back and every retry the user
/// presses fails the same way, so the message is held as a startup send instead: the queue reads
/// the screen again on its backoff and types it the moment the session answers (the 2026-10-11
/// DECISION in session_chat_send_wake.rs). Each outage is logged once as
/// `sessionChatSendHeldForGxserverTools` with the paths gxserver looked in, and its end once as
/// `sessionChatSendGxserverToolsBack`. Box sessions run their own tools and are left to the
/// ordinary send.
pub(super) async fn hold_while_gxserver_tools_missing(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    source: SessionChatMessageSource,
    text: &str,
    image_paths: &[String],
) -> Option<std::result::Result<usize, DomainStateError>> {
    if !gxserver_tools_missing(state, target).await {
        return None;
    }
    Some(hold_while_tools_missing(
        state,
        target,
        source,
        text,
        image_paths,
    ))
}

/// [`hold_as_startup_send`] with the words for a send held until gxserver's tools are back.
pub(super) fn hold_while_tools_missing(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    source: SessionChatMessageSource,
    text: &str,
    image_paths: &[String],
) -> std::result::Result<usize, DomainStateError> {
    hold_as_startup_send(
        state,
        target,
        source,
        text,
        image_paths,
        "Ghostex cannot reach its session tools right now; the message will be sent as soon as they are back.",
    )
}

/// Whether sends are being held because zmx is missing, so each outage is logged once when it
/// starts and once when zmx is back, not on every check of every held send (about every 3 s).
static GXSERVER_TOOLS_MISSING: AtomicBool = AtomicBool::new(false);

/// Whether gxserver cannot resolve its bundled zmx (after the toolchain's own short wait, on a
/// blocking thread); logs when sends start being held for it and when it is back.
async fn gxserver_tools_missing(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
) -> bool {
    if crate::agentbox::is_agentbox_session(&target.session) {
        return false;
    }
    let resolved = tokio::task::spawn_blocking(crate::toolchain::require_bundled_zmx)
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
    match resolved {
        Ok(_) => {
            if GXSERVER_TOOLS_MISSING.swap(false, Ordering::Relaxed) {
                record(
                    state,
                    target,
                    "sessionChatSendGxserverToolsBack",
                    "Bundled zmx is back; held messages are being sent.",
                );
            }
            false
        }
        Err(reason) => {
            if !GXSERVER_TOOLS_MISSING.swap(true, Ordering::Relaxed) {
                record(
                    state,
                    target,
                    "sessionChatSendHeldForGxserverTools",
                    &reason,
                );
            }
            true
        }
    }
}

fn hold_as_startup_send(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    source: SessionChatMessageSource,
    text: &str,
    image_paths: &[String],
    held_message: &str,
) -> std::result::Result<usize, DomainStateError> {
    let restarting = || DomainStateError {
        code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
        message: held_message.to_string(),
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

#[cfg(test)]
mod repeated_restart_tests {
    use super::*;

    /// GitHub #456: the same wake on an agent that stays gone runs three times, never a fourth.
    #[test]
    fn the_same_failing_restart_runs_three_times_at_most() {
        let key = "test-project:repeated-restart";
        forget_repeated_restarts("test-project", "repeated-restart");
        let same = || Some("omp --session \"x\"\u{0}120".to_string());
        assert!(count_same_restart(key, same()));
        assert!(count_same_restart(key, same()));
        assert!(count_same_restart(key, same()));
        assert!(!count_same_restart(key, same()));
        assert!(!count_same_restart(key, same()));
        // A different command (a reference set aside) or a conversation that grew starts over.
        assert!(count_same_restart(key, Some("omp\u{0}120".to_string())));
        assert!(count_same_restart(key, Some("omp\u{0}480".to_string())));
        // A live agent's restart, and the user's Retry, clear the count.
        for _ in 0..3 {
            count_same_restart(key, same());
        }
        assert!(count_same_restart(key, None));
        for _ in 0..3 {
            assert!(count_same_restart(key, same()));
        }
        forget_repeated_restarts("test-project", "repeated-restart");
        assert!(count_same_restart(key, same()));
    }
}
