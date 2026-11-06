use super::*;

// ---------------------------------------------------------------------------
// The watchdog task
// ---------------------------------------------------------------------------

pub(super) struct WatchdogCursor {
    pub(super) path: Option<PathBuf>,
    /// Where the send happened; the raw scan always re-reads from here.
    pub(super) base_offset: u64,
    /*
    CDXC:AgentScreenDetection 2026-08-24:
    Whether `base_offset` really marks THIS send. The mismatch tier below reads
    every user turn past it as "recorded after the message was typed", so a
    baseline that was never sampled (the transcript resolved only later) or that
    was reset when the file was rewritten under us would make the whole file
    look post-send and turn a session's own history into a false alarm. The
    delivery tiers do not care — finding the sent text anywhere still proves
    delivery — so this gates only the mismatch scan.
    */
    pub(super) baseline_trusted: bool,
    pub(super) state: SessionChatIncrementalState,
    /*
    Sticky affirmative evidence, monotone once set: the first post-send user
    turn that is not the message we sent (`true` ⇒ it was empty), and whether
    the agent produced output after it. Re-derived from the whole window on
    every poll, so ordering is never carried across polls; only the verdict is.
    */
    pub(super) mismatched_input: Option<bool>,
    pub(super) agent_answered_mismatch: bool,
}

pub(super) async fn run_session_chat_send_watchdog(
    probe: Arc<SessionChatSendProbe>,
    publish: SessionChatWatchdogPublisher,
    read_state: SessionChatWatchdogStateReader,
    returned_prompt: SessionChatReturnedPromptTrigger,
    heal: SessionChatSendHealer,
    generation: Arc<AtomicU64>,
    my_generation: u64,
) {
    let superseded = || generation.load(Ordering::SeqCst) != my_generation;
    if superseded() {
        return;
    }
    // A new send is itself proof the session took input, so the previous
    // watchdog's verdict is stale the moment this one starts.
    if clear_session_chat_watchdog_notice(&probe.project_id, &probe.session_id).is_some() {
        publish();
    }

    let decoder = session_chat_line_decoder(probe.transcript_agent);
    let needle = normalize_watchdog_text(&probe.text);
    /*
    Everything the raw scan may look for: what the user typed, plus whatever
    record the CLI writes when it intercepts the input instead of sending it.
    */
    let raw_needles: Vec<String> = json_escaped_needle(&probe.text)
        .into_iter()
        .chain(
            probe
                .intercepted
                .iter()
                .flat_map(InterceptedInput::delivery_needles),
        )
        .collect();
    let mut cursor = WatchdogCursor {
        path: probe.transcript_path.clone(),
        base_offset: probe.transcript_offset,
        baseline_trusted: probe.transcript_path.is_some(),
        state: SessionChatIncrementalState::new(),
        mismatched_input: None,
        agent_answered_mismatch: false,
    };
    cursor.state.rebase(probe.transcript_offset);

    let started = Instant::now();
    // Polls observed since the affirmative evidence became complete.
    let mut mismatch_polls = 0u32;
    // CDXC:AgentScreenDetection 2026-09-26 WHY:
    // A published notice that a late delivery disproves (the queued-input card and the delivery warning) keeps the transcript under watch until delivery, notice retirement/expiry, or a newer send; terminal capture still happens only at escalation. This extends the 2026-09-13 rule for the queued card: a delivery warning whose message arrived later (the user pressed Enter in the terminal) stayed up for ten minutes, and the prompt queue failed the next queued message on it without trying to send it.
    let mut published_kind: Option<String> = None;
    loop {
        tokio::time::sleep(WATCHDOG_POLL_INTERVAL).await;
        if superseded() {
            return;
        }
        if let Some(kind) = published_kind.as_deref() {
            if session_chat_watchdog_notice(&probe.project_id, &probe.session_id)
                .is_none_or(|notice| notice.kind != kind)
            {
                return;
            }
        }
        let poll_probe = probe.clone();
        let poll_needle = needle.clone();
        let poll_raw_needles = raw_needles.clone();
        let Ok((delivered, returned)) = tokio::task::spawn_blocking(move || {
            let mut cursor = cursor;
            let delivered = poll_transcript_for_send(
                &poll_probe,
                &mut cursor,
                decoder,
                &poll_needle,
                &poll_raw_needles,
            );
            (delivered, cursor)
        })
        .await
        else {
            return;
        };
        cursor = returned;
        if delivered {
            if superseded() {
                return;
            }
            if clear_session_chat_watchdog_notice(&probe.project_id, &probe.session_id).is_some() {
                publish();
            }
            return;
        }
        if published_kind.is_some() {
            continue;
        }
        // Affirmative non-delivery: the composer was submitted past our message
        // and the agent is answering what it submitted instead. Nothing the
        // remaining deadline could observe would change that verdict.
        let mismatch_ready = cursor.mismatched_input.is_some() && cursor.agent_answered_mismatch;
        if started.elapsed() < WATCHDOG_DEADLINE
            && (!mismatch_ready || mismatch_polls < WATCHDOG_MISMATCH_GRACE_POLLS)
        {
            if mismatch_ready {
                mismatch_polls += 1;
            }
            continue;
        }

        if superseded() {
            return;
        }
        let reason = match cursor.mismatched_input {
            Some(submitted_empty) => UndeliveredSendReason::MismatchedInput { submitted_empty },
            None => UndeliveredSendReason::TranscriptSilent,
        };
        // Silence plus the sent text back in Claude's composer is an Escape this
        // daemon never saw (a web or mobile terminal), not a lost message.
        if reason == UndeliveredSendReason::TranscriptSilent {
            if let Some(screen) =
                crate::session_chat_send::capture_session_terminal_text(&probe.zmx_name).await
            {
                if crate::session_chat_returned_prompt::screen_shows_returned_session_chat_send(
                    &probe.project_id,
                    &probe.session_id,
                    probe.agent.as_deref(),
                    &screen,
                ) {
                    returned_prompt();
                    return;
                }
            }
        }
        escalate_undelivered_send(
            &probe,
            cursor.path.is_some(),
            &publish,
            &read_state,
            Some(&heal),
            reason,
        )
        .await;
        published_kind = session_chat_watchdog_notice(&probe.project_id, &probe.session_id)
            .map(|notice| notice.kind)
            .filter(|kind| {
                kind == SESSION_CHAT_NOTICE_QUEUED_INPUT
                    || kind == SESSION_CHAT_NOTICE_DELIVERY_FAILED
            });
        if published_kind.is_none() {
            return;
        }
    }
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Both escalations take the same single capture and the same verdict order; they
differ in how much is already known, and the suppressions below exist only for
the half that is reasoning from silence.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UndeliveredSendReason {
    /// The terminal took the message, but nothing appeared in the transcript
    /// before the deadline. Delivery is UNPROVEN, so a plausible innocent
    /// explanation (a client-side queue, a turn still running, no transcript to
    /// watch at all) has to win over the alarm.
    TranscriptSilent,
    /*
    CDXC:AgentScreenDetection 2026-08-24:
    The terminal took the message and then recorded a DIFFERENT user turn —
    usually an empty one, submitted by the send's trailing Enter before the
    paste had been ingested. Non-delivery is evidenced rather than inferred, so
    the "already working, still working" suppression must not apply: the turn it
    points at is the one this mismatched input started.
    */
    MismatchedInput {
        submitted_empty: bool,
    },
    /// The send itself failed, so the message never reached the agent.
    /// Non-delivery is a FACT here, so there is nothing to suppress against and
    /// exactly one of the three verdicts is always published.
    WriteFailed(SendWriteFailure),
}

/*
CDXC:AgentScreenDetection 2026-08-19:
The message is undelivered — the deadline passed with nothing in the transcript,
the transcript recorded a different prompt in its place, or the send itself
failed (see `UndeliveredSendReason`). Exactly one
terminal capture happens here — never in the poll loop — and the verdict is
decided in suppression-first order, because a false "your message was lost" is
worse than a missed one:
  1. Codex queued the input client-side (nothing is written until the running
     turn ends) — say so, at severity info. (Silence only.)
  2. The screen explains itself (login expired, trust dialog, ...) — publish THAT
     notice so the client renders the specific card, re-sourced to the watchdog.
     Skipped for input the CLI intercepted: the screen then belongs to the
     command that just ran (`/model`'s picker, `/usage`'s quota view), and
     narrating it as "what the terminal is showing INSTEAD of your message" is
     wrong twice over. A genuinely blocking screen is still published by the
     screen detector on the next read, which does not need the watchdog.
  3. Clean screen: consult Claude's own session registry before blaming the
     process.
  4. The agent is working at the deadline — normal, stay silent: either the
     turn that was already running is holding the input queued, or the send
     itself started the turn. (Silence only, and NOT for `MismatchedInput`:
     there the running turn is the one the mismatched input started, so reading
     it as an innocent explanation is exactly what swallowed this alarm before.)
  5. Otherwise the honest verdict — after one last-chance scan of freshly
     re-resolved transcript candidates, which clears a send whose watched file
     rotated out from under the watchdog: nothing proves the message arrived
     (a WARNING suggestion, not an error) — or, for `MismatchedInput`, positive
     proof that something else was submitted.
*/
pub(super) async fn escalate_undelivered_send(
    probe: &SessionChatSendProbe,
    transcript_watched: bool,
    publish: &SessionChatWatchdogPublisher,
    read_state: &SessionChatWatchdogStateReader,
    heal: Option<&SessionChatSendHealer>,
    reason: UndeliveredSendReason,
) {
    /*
    Two different questions, deliberately not one flag:
      - `typed_into_terminal` — the message DID reach the terminal, so an
        innocent explanation (a client-side queue, a command the CLI ran itself,
        a session with no transcript yet) can still outrank the alarm. A send
        that failed has none of those.
      - `reasoning_from_silence` — the verdict rests on nothing having been
        recorded. `MismatchedInput` does not: it recorded the wrong thing.
    */
    let typed_into_terminal = !matches!(reason, UndeliveredSendReason::WriteFailed(_));
    let reasoning_from_silence = reason == UndeliveredSendReason::TranscriptSilent;
    let screen = crate::session_chat_send::capture_session_terminal_text(&probe.zmx_name).await;
    // CDXC:AgentScreenDetection 2026-09-13 DECISION:
    // User: do not show a delivery warning for a message waiting behind compaction.
    // This only explains transcript silence; failed writes and mismatched submissions still report their evidence.
    if reasoning_from_silence
        && screen.as_deref().is_some_and(|screen| {
            crate::session_chat_terminal_activity::is_session_chat_compacting_activity(
                crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                    probe.agent.as_deref(),
                    screen,
                )
                .as_ref(),
            )
        })
    {
        return;
    }
    if let Some(screen) = screen.as_deref().filter(|_| reasoning_from_silence) {
        // A queued preview explains silence, never a failed write or a different submitted prompt.
        if session_chat_screen_shows_queued_input(probe.agent.as_deref(), screen, &probe.text) {
            // Delivery may have happened while the terminal capture was in flight.
            if delivered_elsewhere_at_deadline(probe).await {
                return;
            }
            publish_watchdog_notice(
                probe,
                SessionChatTerminalNotice::new(
                    SESSION_CHAT_NOTICE_QUEUED_INPUT,
                    SessionChatTerminalNoticeSeverity::Info,
                    SessionChatTerminalNoticeSource::Watchdog,
                    "Message waiting",
                )
                .with_detail(
                    "The agent reads it when it finishes what it's doing now. Nothing to do.",
                )
                .with_screen_tail(session_chat_terminal_screen_tail(screen))
                .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
                    "Open terminal",
                )]),
                publish,
            );
            return;
        }
    }

    let screen_tail = screen
        .as_deref()
        .and_then(session_chat_terminal_screen_tail);
    /*
    A blocking screen outranks the still-working suppression below. Hook
    activity is a claim about a turn that STARTED; when the CLI dies or blocks
    mid-turn nothing ever clears it, and that stuck "working" is exactly the
    state this feature exists to explain.
    */
    let screen_explains_the_send = !(typed_into_terminal && probe.intercepted.is_some());
    if let Some(screen) = screen.as_deref().filter(|_| screen_explains_the_send) {
        if let Some(mut notice) =
            classify_session_chat_terminal_notice(probe.agent.as_deref(), screen)
        {
            notice.source = SessionChatTerminalNoticeSource::Watchdog;
            let prefix = undelivered_prefix(reason);
            notice.detail = Some(match notice.detail {
                Some(detail) => format!("{prefix} {detail}"),
                None => prefix.to_string(),
            });
            publish_watchdog_notice(probe, notice, publish);
            return;
        }
    }

    let reader = read_state.clone();
    let live = tokio::task::spawn_blocking(move || reader())
        .await
        .unwrap_or_default();
    /*
    CDXC:SessionChat 2026-10-09 WHY:
    Claude's session registry alone used to decide "exited", and it was wrong twice on 2026-10-09: a cswap account keeps its records in its own folder, and on Windows a pid cannot be checked at all, so a Claude that was running got "Claude Code is no longer running in this terminal". The session's own process tree (the snapshot account switching already trusts) now has to agree before anything is called exited, and an exited agent is restarted on its own conversation with the message held for it (the 2026-10-09 DECISION in session_chat_queue_runtime/send_heal.rs) instead of being reported.
    */
    let agent_exited = live.running
        && probe_claude_agent_liveness(probe.agent.as_deref(), probe.agent_session_id.as_deref())
            == ClaudeAgentLiveness::Exited
        && !crate::session_chat_send::session_agent_process_running(
            &probe.zmx_name,
            &crate::resume_lookup::home_dir(),
        )
        .await;
    if agent_exited {
        // The healer restarts the agent and holds the message for it, or leaves the message as a
        // failed row (session_chat_send_failure.rs), so nothing more is said here. A send that
        // never reached the terminal has its failed row from the send path already.
        if let Some(heal) = heal.filter(|_| typed_into_terminal) {
            heal().await;
        }
        if clear_session_chat_watchdog_notice(&probe.project_id, &probe.session_id).is_some() {
            publish();
        }
        return;
    }

    if typed_into_terminal {
        /*
        The agent is working at the deadline, with a clean screen and a live
        process: the silent transcript has an innocent explanation either way.
        A turn that was already running when the message was typed is holding
        the input queued behind it (the Codex queue banner above is the same
        story with a visible witness); a turn that STARTED after the send is
        the send itself being processed, because the only submission recorded
        since the send is ours-or-nothing — a different one would have produced
        `MismatchedInput` above. The started-after case used to alarm (the old
        rule also required hook activity at send time), which fired falsely
        whenever the watched transcript file had gone stale under the watchdog.

        CDXC:AgentScreenDetection 2026-08-24: `MismatchedInput` is
        excluded, and that exclusion is the whole fix. There the transcript
        recorded a user turn AFTER the send that is not ours and the agent went
        to work on it, so "still working" describes the turn that ATE the send.
        Treating it as an innocent explanation is what let an empty submit lose
        a message with no notice at all.
        */
        if reasoning_from_silence && live.working {
            return;
        }
        /*
        The generic verdict is "nothing was recorded", so it needs a transcript
        to have been recorded INTO. A session whose transcript never resolved
        (the agent has not reported its session id yet) gives no evidence either
        way, and guessing there would fire on every healthy first message.
        Positive evidence — a blocking screen, a dead process — still alarms
        above. Neither suppression applies to a send that FAILED: there the
        message provably never left Ghostex.
        */
        if !transcript_watched {
            return;
        }
        /*
        CDXC:AgentScreenDetection 2026-08-20:
        The CLI executes this input itself rather than sending it to the model,
        so a silent transcript is its NORMAL outcome — `/usage`, `/model`,
        `!ls` and the rest write nothing a watcher can see, and the records that
        do exist are matched as delivery above. Reasoning from silence here only
        ever produced a false "your message did not reach the agent" for a
        command that plainly ran. Hard evidence above — an exited CLI — still
        alarms, and a send that FAILED never reaches this suppression.
        */
        if probe.intercepted.is_some() {
            return;
        }
        /*
        CDXC:AgentScreenDetection 2026-08-28:
        Last chance before alarming from silence: re-resolve the transcript and
        scan the tail of every candidate file written since the send. The poll
        loop watches ONE path resolved at send time, and that path can go stale
        under it — Codex rotates its rollout, a recorded agentSessionPath
        outlives the file it named — after which every poll reads a file the
        agent no longer writes and a perfectly delivered message looks silent.
        */
        if reasoning_from_silence && delivered_elsewhere_at_deadline(probe).await {
            return;
        }
    }

    let notice = match reason {
        // The send path already tried to recover this one (session_chat_queue_runtime/send_heal.rs)
        // and the message is now a failed row with its own card (session_chat_send_failure.rs).
        UndeliveredSendReason::WriteFailed(failure) => {
            crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                "sessionChatSendFailedStep",
                &probe.project_id,
                &probe.session_id,
                failure.detail(),
                &[],
            );
            return;
        }
        // The affirmative verdict has its own card, built in the notice catalog
        // because it is the one that tells the user where their text went.
        UndeliveredSendReason::MismatchedInput { submitted_empty } => {
            session_chat_delivery_mismatch_notice(submitted_empty, screen_tail)
        }
        /*
        CDXC:AgentScreenDetection 2026-08-28: this verdict rests on
        NOT having observed something, so it is a suggestion, not a failure:
        a yellow "go make sure" card, never a red alarm. The red card is
        reserved for the two verdicts above/below with affirmative evidence.
        */
        UndeliveredSendReason::TranscriptSilent => SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_DELIVERY_FAILED,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Watchdog,
            "Did your message arrive?",
        )
        .with_detail("Ghostex couldn't confirm the agent got it. Check the terminal.")
        .with_screen_tail(screen_tail)
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Terminal view",
        )]),
    };
    publish_watchdog_notice(probe, notice, publish);
}

/// Leads the detail of a classified screen notice with what the send did.
fn undelivered_prefix(reason: UndeliveredSendReason) -> &'static str {
    match reason {
        UndeliveredSendReason::TranscriptSilent => {
            "Your message was not recorded by the agent. This is what its terminal is showing instead."
        }
        UndeliveredSendReason::MismatchedInput { .. } => {
            "Your message was not delivered. The agent recorded a different prompt in its place, and this is what its terminal is showing."
        }
        UndeliveredSendReason::WriteFailed(_) => {
            "Your message could not be sent to the agent. This is what its terminal is showing instead."
        }
    }
}

/*
CDXC:AgentScreenDetection 2026-08-28:
Deadline-time delivery proof that does not trust the send-time path. Resolution
runs twice — once as the send did (recorded path first) and once by session id
alone — so a stale recorded path cannot mask the live file the id sweep finds.
Every candidate written since the send gets its tail scanned for the message,
both as raw JSON-escaped bytes and as decoded composer submissions (which is
what covers a send too short for the raw scan). A hit means the message IS in
a transcript the agent is writing, so the alarm would be a lie; the residual
risk — an identical earlier message in the same tail masking a real loss — is
the cheap side of "a false 'your message was lost' is worse than a missed one".
*/
async fn delivered_elsewhere_at_deadline(probe: &SessionChatSendProbe) -> bool {
    let transcript_agent = probe.transcript_agent;
    let agent_session_id = probe.agent_session_id.clone();
    let agent_session_path = probe.agent_session_path.clone();
    let text = probe.text.clone();
    let sent_at = probe.sent_at;
    tokio::task::spawn_blocking(move || {
        transcript_tail_proves_delivery(
            transcript_agent,
            agent_session_id.as_deref(),
            agent_session_path.as_deref(),
            &text,
            sent_at,
        )
    })
    .await
    .unwrap_or(false)
}

/// BLOCKING (filesystem walk + bounded tail reads).
fn transcript_tail_proves_delivery(
    agent: SessionChatTranscriptAgent,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
    text: &str,
    sent_at: std::time::SystemTime,
) -> bool {
    let needle = normalize_watchdog_text(text);
    if needle.is_empty() {
        return false;
    }
    let raw_needles: Vec<String> = json_escaped_needle(text).into_iter().collect();
    let mut candidates: Vec<PathBuf> = Vec::new();
    for path in [
        resolve_session_chat_transcript_path(agent, agent_session_id, agent_session_path),
        // Session-id sweep with the recorded path ignored: this is the lookup
        // that finds the newest live file after a rotation.
        resolve_session_chat_transcript_path(agent, agent_session_id, None),
    ]
    .into_iter()
    .flatten()
    {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    for path in candidates {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        // Only a file the agent wrote AFTER the send can testify about it.
        if !metadata
            .modified()
            .is_ok_and(|modified| modified >= sent_at)
        {
            continue;
        }
        let offset = metadata.len().saturating_sub(WATCHDOG_RAW_SCAN_LIMIT_BYTES);
        let Some(tail) = read_appended_text(&path, offset, WATCHDOG_RAW_SCAN_LIMIT_BYTES) else {
            continue;
        };
        if raw_needles.iter().any(|raw| tail.contains(raw)) {
            return true;
        }
        // The first line of a mid-file tail is truncated; its parse just fails.
        for line in tail.lines() {
            if let Some(WatchdogRecord::UserSubmission(submitted)) =
                classify_watchdog_record(agent, line)
            {
                if watchdog_text_matches(&submitted, &needle) {
                    return true;
                }
            }
        }
    }
    false
}

/// Stores the verdict and pushes a frame only when it actually says something
/// new: a repeat notice must keep its original `detectedAt`, which is the
/// client's dismissal key.
fn publish_watchdog_notice(
    probe: &SessionChatSendProbe,
    notice: SessionChatTerminalNotice,
    publish: &SessionChatWatchdogPublisher,
) {
    let previous = session_chat_watchdog_notice(&probe.project_id, &probe.session_id);
    if notice.same_notice(previous.as_ref()) {
        return;
    }
    // See CDXC:SessionChat 2026-09-25 in session_chat_send_diagnostics.rs: a delivery card the
    // user sees is a break in sending, so it is logged with the screen it was built from.
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatDeliveryNotice",
        &probe.project_id,
        &probe.session_id,
        &notice.title,
        &notice
            .screen_tail
            .as_deref()
            .map(|tail| tail.lines().map(str::to_string).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    set_session_chat_watchdog_notice(&probe.project_id, &probe.session_id, notice);
    publish();
}
