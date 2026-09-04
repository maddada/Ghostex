use super::*;

/*
CDXC:SessionChat 2026-08-21:
THE internal chat-message send. `/api/sendSessionChatMessage` is one caller;
the prompt queue ("Send now" and the scheduler) is the other, which is why it
lives here instead of inside the HTTP handler. Everything a chat send needs
travels with it — the per-session send mutex in session_chat_send.rs, the
answerable-picker refusal, the terminal-input clear, the delivery watchdog and
the option re-detect — so a queued prompt is indistinguishable from one the user
typed and can never interleave with a Delayed Send.
Returns the number of text bytes handed to zmx.
*/
///
/// `prompt_source` is the analytics attribution for this send: `chat` when the
/// user typed it into the composer, `queue` when the prompt queue delivered it.
/// The two are indistinguishable from inside this function (that is the whole
/// point of the queue), so the caller is the only place that knows.
pub(crate) async fn send_session_chat_message_internal(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    prompt_source: &'static str,
) -> std::result::Result<usize, DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    let target = resolve_session_chat_send_target(state, &params, "sendSessionChatMessage")?;
    let agent = session_chat_agent_for_session(&target.session);
    let terminal_agent =
        crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
            .or_else(|| agent.clone());
    /*
    CDXC:AgentScreenDetection 2026-08-19:
    Sample where the transcript ends BEFORE the message is enqueued: everything
    written past this offset is a candidate for "the agent recorded it". Sampling
    afterwards would race the agent's own write. This is the only work the
    watchdog puts in front of a send — a path resolve plus one `metadata()`, both
    on a blocking thread — and it is skipped entirely for agents the watchdog
    does not cover.
    */
    /*
    CDXC:SessionChat 2026-08-21:
    Claude Code's resume-usage picker owns the input line when a large session
    is resumed. This send used to answer it automatically ("Resume full session
    as-is") before typing, which was wrong twice over: the summary-vs-full
    trade-off is the user's to make, and whenever the walk missed, the message's
    own trailing Enter confirmed the HIGHLIGHTED row instead — silently
    compacting the conversation the user was continuing, with nothing to show
    for it but a delivery-failed banner.

    So the send refuses instead. The same capture that proves the picker is up
    caches the notice carrying its rows, and publishing it puts the answer
    picker in front of the user on every subscribed client. Only an ANSWERABLE
    state stops a send here: the catalog's other blocking dialogs still go
    through to the delivery watchdog, which is the only thing that can explain
    them.
    */
    /*
    CDXC:SessionChat 2026-08-26:
    Terminal capture must use the concrete CLI identity, not the normalized
    transcript family. Omp shares Pi's transcript decoder but paints different
    composer and statusline chrome, so folding it to Pi here loses both screen
    readings. Agents without a concrete screen identity fall back to the
    transcript family.
    */
    let detection = SessionChatOptionDetector::new(state)
        .detect(
            &target.project_id,
            &target.session_id,
            terminal_agent.as_deref(),
            true,
        )
        .await;
    if let Some(blocking) = detection
        .notice
        .as_ref()
        .filter(|notice| notice.is_answerable())
    {
        session_chat_terminal_notice_publisher(state, &target.project_id, &target.session_id)();
        return Err(DomainStateError {
            code: "invalidState",
            message: format!("{}. Answer it in chat before sending.", blocking.title),
        });
    }
    /*
    CDXC:SessionChat 2026-08-26:
    The positive gate. Everything above is "is a screen we RECOGNISE in the
    way?"; this is "did the CLI paint an input box at all?". It catches the
    states no notice rule covers — a CLI still booting, an auth screen shipped
    after our catalog, a dialog we have never seen — which are exactly the ones
    that used to eat a message and answer with a delivery-failed banner minutes
    later.

    `Unknown` FAILS OPEN and is by far the common case for unmeasured agents, so
    this can only ever refuse a send it has positive evidence about. The screen
    tail behind the verdict is not squeezed into the error (DomainStateError
    carries a code and a message and nothing else, at 169 construction sites);
    clients read it from /api/readSessionTerminalTail instead.
    */
    let dismiss_claude_settings = detection.composer.should_dismiss_with_escape();
    if detection.composer.is_not_ready() && !dismiss_claude_settings {
        return Err(DomainStateError {
            code: "composerNotReady",
            message: detection
                .composer
                .reason
                .clone()
                .unwrap_or_else(|| "The agent's input box is not accepting input yet.".to_string()),
        });
    }
    let send_probe = crate::session_chat_watchdog::SessionChatSendProbe::sample(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        agent.as_deref(),
        read_runtime_text(&target.session, "agentSessionId").as_deref(),
        read_runtime_text(&target.session, "agentSessionPath").as_deref(),
        text,
    )
    .await;
    /*
    Chat owns the terminal composer when it sends. Discard anything already
    sitting on that hidden input line instead of turning it into a user-facing
    Saved Prompt: `build_session_chat_message_steps` starts with the measured
    Ctrl+U/Ctrl+K clear burst, settles it, and only then pastes this message.
    Terminal -> Chat view switching remains the separate, loss-safe draft
    transfer path for text the user actually wants to carry between views.
    */
    let steps = crate::session_chat_send::build_session_chat_message_steps(
        terminal_agent.as_deref(),
        text,
        image_paths,
        dismiss_claude_settings,
    );
    crate::session_chat_follower::returned_prompt::record_session_chat_send_started(
        &target.project_id,
        &target.session_id,
        text,
        image_paths,
    );
    if let Err(error) = crate::session_chat_send::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-message",
        steps,
    )
    .await
    {
        /*
        CDXC:AgentScreenDetection 2026-08-19:
        The case this feature exists for — the agent CLI in this pane is dead —
        fails HERE, not at the delivery watchdog: zmx refuses the clear or paste,
        or the terminal screen proves that the paste never landed, so the user
        would otherwise see only a generic toast. When the TERMINAL is what
        refused the message (as opposed to the send being superseded or
        cancelled), escalate once with the same one-capture verdict the watchdog
        takes at its deadline. It runs as its own task so the error response is
        not made to wait for the capture, it never retries the send, and the
        response below is unchanged.
        */
        if error.terminal_refused() {
            if let Some(send_probe) = send_probe {
                crate::session_chat_watchdog::escalate_failed_session_chat_send(
                    send_probe,
                    session_chat_terminal_notice_publisher(
                        state,
                        &target.project_id,
                        &target.session_id,
                    ),
                    session_chat_watchdog_state_reader(
                        state,
                        &target.project_id,
                        &target.session_id,
                    ),
                );
            }
        }
        // CDXC:SessionChat 2026-08-26: the in-worker wait raises
        // the same code the pre-send gate does, so a client has one case to
        // handle whichever of the two caught it.
        if error.composer_not_ready() {
            return Err(DomainStateError {
                code: "composerNotReady",
                message: error.message,
            });
        }
        // The user's own Escape stopped this send before Enter: not a failure
        // the composer should announce, and the text is still theirs.
        if error.cancelled() {
            return Err(DomainStateError {
                code: "sendCancelled",
                message: error.message,
            });
        }
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: error.message,
        });
    }
    crate::session_chat_follower::returned_prompt::record_session_chat_send_submitted(
        &target.project_id,
        &target.session_id,
    );
    /*
    CDXC:AgentScreenDetection 2026-08-19:
    The bytes reached zmx, which says nothing about the agent having received
    them: a message typed into a login screen, a trust dialog or a shell where
    the CLI already exited is accepted and lost. The watchdog verifies delivery
    against the transcript and surfaces the terminal's own explanation when it
    cannot. It never retries and never writes to the terminal.
    */
    if let Some(send_probe) = send_probe {
        let returned_prompt_state = state.clone();
        let returned_prompt_target = crate::session_chat_send::SessionChatSendTarget {
            project_id: target.project_id.clone(),
            session_id: target.session_id.clone(),
            zmx_name: target.zmx_name.clone(),
            session: target.session.clone(),
        };
        crate::session_chat_watchdog::start_session_chat_send_watchdog(
            send_probe,
            session_chat_terminal_notice_publisher(state, &target.project_id, &target.session_id),
            session_chat_watchdog_state_reader(state, &target.project_id, &target.session_id),
            std::sync::Arc::new(move || {
                crate::session_chat_follower::returned_prompt::schedule_session_chat_returned_prompt_detection(
                    &returned_prompt_state,
                    &returned_prompt_target,
                    "session-chat-watchdog",
                );
            }),
        );
    }
    /*
    CDXC:Telemetry 2026-08-26:
    Counted after the bytes reached zmx and only on the success path, so a
    refused send is not reported as a prompt. The COUNT and the resolved agent
    are all that leave; the prompt text is not in scope for the emitter and
    cannot be.
    */
    crate::telemetry::prompt_sent(&target.session, prompt_source);
    /*
    An option command changes what the statusline reports: read it back. It is
    also NOT a user prompt — Ghostex itself typed it on the user's behalf when
    they picked a model or an effort level out of the composer's dropdown.
    */
    let is_option_readback_command =
        crate::session_chat_options::is_session_chat_option_command_text(
            terminal_agent.as_deref(),
            text,
        );
    let is_option_command = is_option_readback_command
        || crate::session_chat_options::is_session_chat_activity_command_text(
            terminal_agent.as_deref(),
            text,
        );
    /*
    CDXC:Drafts 2026-08-28:
    THE chat half of the draft promotion choke point. Both callers reach the
    agent through this one function — the user's composer and the prompt queue's
    scheduler — so clearing the marker here covers every Ghostex-delivered first
    prompt, and it happens only after the bytes were accepted: a send refused by
    the composer gate or by zmx returns above and leaves the row a draft.
    (The terminal-direct half lives in `agents/drafts.rs`.)

    Option commands are carved out of BOTH halves. `/model` is delivered through
    this same function, and the dropdown that sends it is the one that will host
    the draft's own Agents section — so promoting on it would let a user destroy
    their draft's agent switcher by picking a model in it. The carve-out has to
    cover the terminal churn those bytes cause as well, which is why the
    non-promoting branch re-arms the draft's launch suppression window instead
    of doing nothing: the command's spinner is then folded back to idle exactly
    like a startup spinner, and cannot promote the draft through the
    activity-based half a second later.
    */
    if is_option_command {
        suppress_draft_activity_after_app_command(state, &target.project_id, &target.session_id);
    } else {
        promote_draft_session_after_send(state, &target.project_id, &target.session_id);
    }
    retire_sent_session_chat_draft(state, &target.project_id, &target.session_id, text);
    // A `/compact` is NOT re-read here: the follower's transcript-keyed probe
    // burst covers it for chat-sent and terminal-typed commands alike
    // (CDXC:AgentScreenDetection), and a second publisher of the same
    // activity only let a later follower frame overwrite what this one showed.
    if is_option_readback_command {
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            terminal_agent.as_deref(),
        );
    }
    Ok(text.len())
}
