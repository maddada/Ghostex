use super::*;

pub(super) async fn run_session_chat_send_worker(
    mut rx: mpsc::UnboundedReceiver<SessionChatSendJob>,
    generation: Arc<AtomicU64>,
) {
    while let Some(job) = rx.recv().await {
        let SessionChatSendJob {
            on_step,
            mut completion,
            mut captured_draft,
            project_id,
            session_id,
            source,
            zmx_name,
            generation: job_generation,
            steps,
        } = job;
        if job_generation != generation.load(Ordering::SeqCst) {
            if let Some(completion) = completion.take() {
                let _ = completion.send(Err(SessionChatSendError::not_attempted(
                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                )));
            }
            continue; // cancelled while queued
        }
        let mut outcome = Ok(());
        let mut composer_agent: Option<String> = None;
        let mut clear_pending = false;
        let mut mid_turn_probe: Option<AgentMidTurnProbe> = None;
        for step in steps {
            if job_generation != generation.load(Ordering::SeqCst) {
                outcome = Err(SessionChatSendError::not_attempted(
                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                ));
                break; // cancelled mid-sequence
            }
            // The existing separate clear write and settle remain one sequence.
            // Prove their result before any following paste or Enter can run.
            if clear_pending && !matches!(&step, SessionChatSendStep::SleepMs(_)) {
                if let Some(agent) = composer_agent.as_deref() {
                    if let Err(error) = clear_session_chat_composer(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        agent,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                clear_pending = false;
            }
            if let Some(observer) = &on_step {
                observer(&step);
            }
            match step {
                SessionChatSendStep::RetryCodexConversation => {
                    if let Err(message) = crate::session_chat_codex_lock::retry(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::AlignCodexQuestion { question, row } => {
                    if let Err(message) =
                        crate::session_chat_codex_question_align::align_codex_question(
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                            question,
                            row,
                            &|| job_generation != generation.load(Ordering::SeqCst),
                        )
                        .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::AlignQuestionRow(target) => {
                    if let Err(message) =
                        crate::session_chat_question_row_align::align_question_row(
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                            &target,
                            &|| job_generation != generation.load(Ordering::SeqCst),
                        )
                        .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::PrepareClaudeQuestion(prep) => {
                    if let Err(message) =
                        crate::session_chat_claude_question_prep::prepare_claude_question(
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                            &prep,
                            &|| job_generation != generation.load(Ordering::SeqCst),
                        )
                        .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::CancelClaudeQuestion(questions) => {
                    if let Err(message) =
                        crate::session_chat_claude_question_answer::cancel_claude_question(
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                            &questions,
                            &|| job_generation != generation.load(Ordering::SeqCst),
                        )
                        .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::DriveCodexAsyncQuestion(answer) => {
                    if let Err(message) = crate::session_chat_codex_async_answer::run(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &answer,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::WaitForAccountReady {
                    agent,
                    expected_identity,
                    home_dir,
                    previous_process_id,
                    timeout_ms,
                    accept_remembered_trust,
                } => {
                    let cancelled = || job_generation != generation.load(Ordering::SeqCst);
                    if let Err(error) =
                        crate::accounts::restart_verification::wait_for_account_ready(
                            crate::accounts::restart_verification::AccountReadyTarget {
                                project_id: &project_id,
                                session_id: &session_id,
                                zmx_name: &zmx_name,
                                accept_remembered_trust,
                            },
                            &agent,
                            &expected_identity,
                            &home_dir,
                            previous_process_id,
                            timeout_ms,
                            &cancelled,
                        )
                        .await
                    {
                        outcome = Err(if cancelled() {
                            SessionChatSendError::not_attempted(
                                SESSION_CHAT_SEND_CANCELLED.to_string(),
                            )
                        } else {
                            SessionChatSendError::new(
                                SessionChatSendFailure::ComposerNotReady,
                                error,
                            )
                        });
                        break;
                    }
                }
                SessionChatSendStep::StopClaudeBackground {
                    background,
                    home_dir,
                } => {
                    if let Err(error) =
                        crate::accounts::claude_background::stop(&background, &home_dir, &|| {
                            job_generation != generation.load(Ordering::SeqCst)
                        })
                        .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::InterruptAgentForAccountSwitch {
                    home_dir,
                    timeout_ms,
                } => {
                    if let Err(error) = crate::accounts::exit::interrupt_until_exited(
                        &home_dir,
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        timeout_ms,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::RunShellCommand {
                    command,
                    home_dir,
                    timeout_ms,
                } => {
                    if let Err(error) = super::launch_command::run_shell_command(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &command,
                        &home_dir,
                        timeout_ms,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::GuardForkRename {
                    agent,
                    title,
                    state_db_file,
                    server_id,
                    first_user_message,
                    rename_requested_at,
                    expected_composer,
                } => {
                    let matches_composer = capture_session_terminal_text_vt(&zmx_name)
                        .await
                        .and_then(|screen| {
                            crate::session_chat_composer::session_chat_composer_input(
                                &agent, &screen,
                            )
                        })
                        .is_some_and(|input| match expected_composer.as_deref() {
                            Some(expected) => !input.is_empty() && input.text == expected,
                            None => input.is_empty(),
                        });
                    let owned = rusqlite::Connection::open_with_flags(
                        state_db_file,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )
                    .ok()
                    .and_then(|db| {
                        DomainRepository::new(&db, &server_id)
                            .get_session(&project_id, &session_id)
                            .ok()
                            .flatten()
                    })
                    .is_some_and(|session| {
                        crate::server::fork_initial_rename_is_current(&session, &title)
                            && read_runtime_text(&session, "firstUserMessage") == first_user_message
                            && read_runtime_text(&session, "pendingAgentTitleRequestRequestedAt")
                                == rename_requested_at
                    });
                    if !matches_composer || !owned {
                        outcome = Err(SessionChatSendError::not_attempted(
                            "The fork rename no longer owns the expected agent composer."
                                .to_string(),
                        ));
                        break;
                    }
                }
                SessionChatSendStep::ClearComposer { agent } => {
                    let cancelled = || job_generation != generation.load(Ordering::SeqCst);
                    if let Err(error) = clear_session_chat_composer(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &agent,
                        &cancelled,
                    )
                    .await
                    {
                        // The clear's one key may still be on its way to a mid-turn agent; another
                        // would land on the box it empties (busy_input_wait.rs).
                        let wait = match (&error.failure, &mid_turn_probe) {
                            (SessionChatSendFailure::ComposerNotCleared, Some(probe)) => {
                                let (zmx, agent): (&str, &str) = (&zmx_name, &agent);
                                wait_out_busy_agent(probe, &cancelled, move || {
                                    composer_reads_empty(zmx, agent)
                                })
                                .await
                            }
                            _ => BusyAgentWait::NotMidTurn,
                        };
                        match wait {
                            BusyAgentWait::Settled => record_busy_agent_wait(
                                &project_id,
                                &session_id,
                                "its input box cleared",
                            ),
                            BusyAgentWait::Stalled => {
                                outcome = Err(busy_agent_stalled());
                                break;
                            }
                            BusyAgentWait::NotMidTurn
                            | BusyAgentWait::TurnEnded
                            | BusyAgentWait::Cancelled => {
                                outcome = Err(error);
                                break;
                            }
                        }
                    }
                }
                SessionChatSendStep::GuardQueuedPromptEscape {
                    requested_at,
                    agent,
                    transcript,
                } => {
                    if queued_prompt_escape_is_stale(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        agent.as_deref(),
                        requested_at,
                        transcript.as_ref(),
                    )
                    .await
                    {
                        crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                            "sessionChatSendDroppedStaleEscape",
                            &project_id,
                            &session_id,
                            "The play button's Escape was dropped: since the click a message was submitted or the agent started a prompt, or no prompt waits in the agent's queue.",
                            &[],
                        );
                        break;
                    }
                }
                SessionChatSendStep::GuardClaudeInterrupt => {
                    if !crate::session_chat_claude_interrupt::claim_claude_interrupt_escape(
                        &project_id,
                        &session_id,
                    ) {
                        break;
                    }
                }
                SessionChatSendStep::GuardCursorInterrupt => {
                    // `session_chat_interrupt_escape_steps` says why Cursor's Ctrl+C waits.
                    let deadline = Instant::now() + Duration::from_secs(8);
                    let mut processing = false;
                    // CDXC:SessionChat 2026-10-17 WHY: a decision panel (a command approval)
                    // ignores Ctrl+C. Its Escape row (Skip) opens Cursor's "tell the agent what to
                    // do instead" box, where an empty Enter skips the command and the agent runs
                    // on (Cursor 2026.10.16), so Stop answers both and then stops that run with
                    // Ctrl+C. Escape in the box returns to the approval, so each key goes once
                    // per panel.
                    let mut dismissed = false;
                    loop {
                        if let Some(screen) = capture_session_terminal_text(&zmx_name).await {
                            // The box also shows "ctrl+c to stop", and Ctrl+C there sends the skip.
                            let asks =
                                crate::session_chat_cursor_blocking::cursor_asks_instead_of_skipped(
                                    &screen,
                                );
                            if !asks
                                && crate::session_chat_cursor_blocking::cursor_is_processing(
                                    &screen,
                                )
                            {
                                processing = true;
                                break;
                            }
                            let key = if asks {
                                Some("\r")
                            } else if !dismissed {
                                crate::session_chat_cursor_decision::cursor_decision_dismiss_key(
                                    &screen,
                                )
                            } else {
                                None
                            };
                            if let Some(key) = key {
                                if let Err(error) = write_session_chat_payload(
                                    &project_id,
                                    &session_id,
                                    &zmx_name,
                                    &source,
                                    key,
                                )
                                .await
                                {
                                    outcome = Err(SessionChatSendError::new(
                                        SessionChatSendFailure::Write,
                                        error,
                                    ));
                                    break;
                                }
                                dismissed = key != "\r";
                            }
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    if !processing {
                        break;
                    }
                }
                SessionChatSendStep::SkipCursorInsteadBox => {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    loop {
                        if capture_session_terminal_text(&zmx_name)
                            .await
                            .is_some_and(|screen| {
                                crate::session_chat_cursor_blocking::cursor_asks_instead_of_skipped(
                                    &screen,
                                )
                            })
                        {
                            let reason = crate::session_chat_send::build_session_chat_paste_bytes(
                                crate::session_chat_cursor_decision::CURSOR_SKIP_REASON,
                            );
                            let written = match write_session_chat_payload(
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                &reason,
                            )
                            .await
                            {
                                Ok(_) => {
                                    // Cursor takes a burst of text and its Enter as one paste.
                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                    write_session_chat_payload(
                                        &project_id,
                                        &session_id,
                                        &zmx_name,
                                        &source,
                                        "\r",
                                    )
                                    .await
                                }
                                Err(error) => Err(error),
                            };
                            if let Err(error) = written {
                                outcome = Err(SessionChatSendError::new(
                                    SessionChatSendFailure::Write,
                                    error,
                                ));
                            }
                            break;
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(150)).await;
                    }
                }
                SessionChatSendStep::GuardCopilotInterrupt => {
                    // A Stop right after a send may come before Copilot shows the turn running.
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut interruptible = false;
                    loop {
                        if let Some(screen) = capture_session_terminal_text(&zmx_name).await {
                            if crate::session_chat_copilot_interrupt::copilot_turn_is_interruptible(
                                &screen,
                            ) {
                                interruptible = true;
                                break;
                            }
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    if !interruptible
                        || !crate::session_chat_copilot_interrupt::claim_copilot_interrupt(
                            &project_id,
                            &session_id,
                        )
                    {
                        break;
                    }
                }
                SessionChatSendStep::GuardGrokInterrupt => {
                    // A Stop right after a send may come before Grok shows the turn running.
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut interruptible = false;
                    loop {
                        if let Some(screen) = capture_session_terminal_text(&zmx_name).await {
                            if crate::session_chat_grok_interrupt::grok_turn_is_interruptible(
                                &screen,
                            ) {
                                interruptible = true;
                                break;
                            }
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    if !interruptible
                        || !crate::session_chat_grok_interrupt::claim_grok_interrupt(
                            &project_id,
                            &session_id,
                        )
                    {
                        break;
                    }
                }
                SessionChatSendStep::GuardHermesInterrupt => {
                    // A Stop right after a send may come before Hermes shows the turn running.
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut interruptible = false;
                    loop {
                        if let Some(screen) = capture_session_terminal_text(&zmx_name).await {
                            if crate::session_chat_hermes_interrupt::hermes_turn_is_interruptible(
                                &screen,
                            ) {
                                interruptible = true;
                                break;
                            }
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    if !interruptible
                        || !crate::session_chat_hermes_interrupt::claim_hermes_interrupt(
                            &project_id,
                            &session_id,
                        )
                    {
                        break;
                    }
                }
                SessionChatSendStep::GuardCodexInterrupt => {
                    let Some(screen) = capture_session_terminal_text(&zmx_name).await else {
                        outcome = Err(SessionChatSendError::not_attempted(
                            "The Codex terminal could not be read, so Escape was not sent."
                                .to_string(),
                        ));
                        break;
                    };
                    if crate::session_chat_codex_pager::codex_escape_would_open_transcript_pager(
                        &screen,
                    ) {
                        break;
                    }
                    // A selection, a search and a scrolled-up view each take one Escape before the turn does.
                    let mut screen = screen;
                    for _ in 0..3 {
                        if !crate::session_chat_codex_pager::codex_escape_would_dismiss_transcript_interaction(&screen) {
                            break;
                        }
                        if let Err(error) = write_session_chat_payload(
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                            SESSION_CHAT_INTERRUPT,
                        )
                        .await
                        {
                            outcome = Err(SessionChatSendError::new(
                                SessionChatSendFailure::Write,
                                error,
                            ));
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(120)).await;
                        let Some(next) = capture_session_terminal_text(&zmx_name).await else {
                            break;
                        };
                        screen = next;
                    }
                    if outcome.is_err()
                        || crate::session_chat_codex_pager::codex_escape_would_open_transcript_pager(
                            &screen,
                        )
                    {
                        break;
                    }
                }
                SessionChatSendStep::StopLocalCommandOutput => {
                    crate::session_chat_app_command::stop_local_command_output(
                        &project_id,
                        &session_id,
                    );
                }
                SessionChatSendStep::BeginLocalCommandOutput {
                    agent,
                    command,
                    durable_id,
                } => {
                    if let Some(screen) = capture_session_terminal_text(&zmx_name).await {
                        crate::session_chat_app_command::begin_local_command_output(
                            &project_id,
                            &session_id,
                            agent.as_deref(),
                            &command,
                            durable_id,
                            screen,
                        );
                    }
                }
                SessionChatSendStep::FinishLocalCommandOutput => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    if let Some(screen) = capture_session_terminal_text(&zmx_name).await {
                        crate::session_chat_app_command::refresh_local_command_output(
                            &project_id,
                            &session_id,
                            &screen,
                        );
                    }
                }
                SessionChatSendStep::VerifyTerminalDialog { agent, id } => {
                    // Claude's dialog is read the way its chat card was (session_chat_claude_dialog.rs).
                    let current = match agent.as_str() {
                        "claude" => {
                            crate::session_chat_claude_dialog::capture_claude_dialog(&zmx_name)
                                .await
                        }
                        _ => capture_session_terminal_text(&zmx_name)
                            .await
                            .and_then(|screen| {
                                crate::session_chat_codex_dialog::detect_codex_dialog(&screen)
                            }),
                    };
                    if current.is_none_or(|dialog| dialog.id != id) {
                        outcome = Err(SessionChatSendError::not_attempted(
                            "The dialog changed before the answer could be sent.".to_string(),
                        ));
                        break;
                    }
                }
                SessionChatSendStep::VerifyTerminalPicker { agent } => {
                    let open =
                        capture_session_terminal_text(&zmx_name)
                            .await
                            .is_some_and(|screen| {
                                crate::session_chat_resume_picker::screen_takes_picker_keys(
                                    &agent, &screen,
                                )
                            });
                    if !open {
                        outcome = Err(SessionChatSendError::not_attempted(
                            "The agent's list closed before the key could be sent.".to_string(),
                        ));
                        break;
                    }
                }
                SessionChatSendStep::CloseUnwatchedCodexTranscriptPager => {
                    if let Err(error) = crate::session_chat_codex_pager::close_unwatched_pager(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            error,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::ReadClaudeSideAnswer => {
                    crate::session_chat_claude_panel::read_whole_side_answer(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                }
                SessionChatSendStep::SleepMs(delay_ms) => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                SessionChatSendStep::PreserveTerminalDraft {
                    state_dir,
                    prompt_editor_input,
                    replacement,
                } => {
                    match preserve_terminal_draft(
                        &state_dir,
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &prompt_editor_input,
                        &generation,
                        job_generation,
                    )
                    .await
                    {
                        Ok(draft) => {
                            if let (Some(expected), Some(existing)) =
                                (replacement.as_deref(), draft.content.as_deref())
                            {
                                if !existing.is_empty() && existing != expected {
                                    // The editor handshake already saved and cleared this text. Put it back,
                                    // then refuse the stale replacement without issuing any further clear.
                                    let restored = write_session_chat_payload(
                                        &project_id,
                                        &session_id,
                                        &zmx_name,
                                        &source,
                                        &build_session_chat_paste_bytes(existing),
                                    )
                                    .await;
                                    outcome = Err(SessionChatSendError::new(SessionChatSendFailure::Write, restored.err().unwrap_or_else(|| "The terminal has different unsent text. Both drafts have been kept; the Chat draft is in Recovered.".to_string())));
                                    break;
                                }
                            }
                            if let Some(sink) = captured_draft.take() {
                                let _ = sink.send(draft);
                            }
                        }
                        Err(error) => {
                            outcome = Err(error);
                            break;
                        }
                    }
                }
                SessionChatSendStep::Write(payload) => {
                    if let Err(error) = write_session_chat_payload(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &payload,
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            error,
                        ));
                        break;
                    }
                    clear_pending = composer_agent.is_some()
                        && !payload.is_empty()
                        && payload.chars().all(|ch| matches!(ch, '\u{15}' | '\u{b}'));
                }
                SessionChatSendStep::WriteFrom {
                    source: write_source,
                    payload,
                } => {
                    if let Err(error) = write_session_chat_payload(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &write_source,
                        &payload,
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            error,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::SelectEmpryoTab { wait_ms } => {
                    if let Err(message) = crate::session_chat_empryo_tabs::select_empryo_own_tab(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        wait_ms,
                    )
                    .await
                    {
                        // Nothing was typed and the window is not showing this session yet, which
                        // a queue holds and retries like an input box that is not up.
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::ComposerNotReady,
                            message,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::ReadInputBoxDraft { agent } => {
                    let Some(input) = capture_session_terminal_text_vt(&zmx_name)
                        .await
                        .filter(|screen| {
                            agent != "empryo"
                                || !crate::session_chat_composer::empryo_input_unfocused(screen)
                        })
                        .and_then(|screen| {
                            crate::session_chat_composer::session_chat_composer_input(
                                &agent, &screen,
                            )
                        })
                    else {
                        outcome = Err(SessionChatSendError::not_attempted(
                            SESSION_CHAT_COMPOSER_NOT_READY.to_string(),
                        ));
                        break;
                    };
                    // Text the capture cannot tell from Empryo's tip is not carried, and the
                    // handoff then clears nothing, so a typed draft stays in the terminal.
                    if let Some(sink) = captured_draft.take() {
                        let _ = sink.send(CapturedTerminalDraft {
                            content: (!input.text_is_empty() && !input.text_unreadable())
                                .then_some(input.text),
                            ..CapturedTerminalDraft::default()
                        });
                    }
                }
                SessionChatSendStep::PullClaudeQueue { transcript } => {
                    match pull_claude_queue(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &transcript,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        Ok(queued_messages) => {
                            if let Some(sink) = captured_draft.take() {
                                let _ = sink.send(CapturedTerminalDraft {
                                    queued_messages,
                                    ..CapturedTerminalDraft::default()
                                });
                            }
                        }
                        Err(error) => {
                            outcome = Err(error);
                            break;
                        }
                    }
                }
                SessionChatSendStep::PullGrokQueue { text } => {
                    match pull_grok_queue(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &text,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await
                    {
                        Ok(taken) => {
                            if let Some(sink) = captured_draft.take() {
                                let _ = sink.send(CapturedTerminalDraft {
                                    queued_messages: if taken { vec![text] } else { Vec::new() },
                                    ..CapturedTerminalDraft::default()
                                });
                            }
                        }
                        Err(error) => {
                            outcome = Err(error);
                            break;
                        }
                    }
                }
                SessionChatSendStep::GuardEmpryoDraft { replacement } => {
                    let input =
                        capture_session_terminal_text_vt(&zmx_name)
                            .await
                            .and_then(|screen| {
                                crate::session_chat_composer::session_chat_composer_input(
                                    "empryo", &screen,
                                )
                            });
                    if input.as_ref().is_some_and(|input| input.text_unreadable()) {
                        outcome = Err(SessionChatSendError::not_attempted(
                            EMPRYO_DRAFT_UNREADABLE.to_string(),
                        ));
                        break;
                    }
                    let held = input
                        .filter(|input| !input.text_is_empty())
                        .map(|input| input.text);
                    if held.is_some_and(|held| held != replacement.trim()) {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            "The terminal has different unsent text. Both drafts have been kept; the Chat draft is in Recovered.".to_string(),
                        ));
                        break;
                    }
                }
                SessionChatSendStep::VerifySubmitted {
                    agent,
                    text,
                    submit,
                } => {
                    if let Err(error) = crate::session_chat_send_submit::confirm_submitted(
                        &agent,
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &text,
                        &submit,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                        mid_turn_probe.as_ref(),
                    )
                    .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::DismissClaudeFeedbackCard => {
                    if let Err(error) =
                        crate::session_chat_claude_feedback_card::dismiss_feedback_card_step(
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                        )
                        .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::DismissClaudePanel { agent, timeout_ms } => {
                    // Claude can drop a key that lands the instant a panel opens, so Escape is
                    // pressed again only while an Escape-safe panel is still what the screen
                    // shows; any other screen stops the presses, so no stray Escape reaches the
                    // input box.
                    // Codex's side conversation closes on Ctrl+C (a first press clears a draft
                    // typed there), and Ctrl+C anywhere else in Codex interrupts or quits, so each
                    // press follows a fresh capture that still shows the side conversation.
                    let codex = crate::agents::identity::normalize_agent_id(agent.as_deref())
                        .as_deref()
                        == Some("codex");
                    // Codex's async question editor is left with the keys its own footer shows,
                    // each one checked against a fresh capture.
                    if codex
                        && capture_session_terminal_text_vt(&zmx_name)
                            .await
                            .is_some_and(|screen| {
                                crate::session_chat_codex_async_answer::question_editor_on_screen(
                                    &screen,
                                )
                            })
                    {
                        if let Err(message) =
                            crate::session_chat_codex_async_answer::leave_question_editor(
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                &|| job_generation != generation.load(Ordering::SeqCst),
                            )
                            .await
                        {
                            outcome = Err(SessionChatSendError::new(
                                SessionChatSendFailure::ComposerNotReady,
                                message,
                            ));
                            break;
                        }
                    }
                    let attempt_ms = (timeout_ms / SESSION_CHAT_CLAUDE_PANEL_ESCAPES)
                        .max(crate::session_chat_composer::SESSION_CHAT_COMPOSER_POLL_MS);
                    let mut presses = 0;
                    let failure = loop {
                        if codex
                            && !capture_session_terminal_text_vt(&zmx_name).await.is_some_and(
                                |screen| {
                                    crate::session_chat_codex_side::codex_side_conversation_on_screen(
                                        &screen,
                                    )
                                },
                            )
                        {
                            break None;
                        }
                        let mut key = Some(if codex {
                            SESSION_CHAT_CODEX_CLOSE_SIDE
                        } else {
                            SESSION_CHAT_INTERRUPT
                        });
                        if !codex {
                            let screen = capture_session_terminal_text_vt(&zmx_name).await;
                            // The send's reading can be older than the screen: the panel may have
                            // closed meanwhile (gxserver closes Claude's feedback card on its own),
                            // and an Escape on Claude's idle input box starts its double-Escape
                            // rewind.
                            if presses == 0 && screen.as_deref().is_some_and(|screen| {
                                !crate::session_chat_composer::detect_session_chat_composer_ready(
                                    agent.as_deref(),
                                    screen,
                                )
                                .should_dismiss()
                            }) {
                                break None;
                            }
                            // Claude's rating survey closes on its own `0`, never on Escape, and
                            // only once it has been up past its mount delay; a survey gone by then
                            // gets no key (session_chat_claude_rating_survey.rs).
                            if screen.as_deref().is_some_and(
                                crate::session_chat_claude_rating_survey::rating_survey_on_screen,
                            ) {
                                tokio::time::sleep(
                                    crate::session_chat_claude_rating_survey::SURVEY_SETTLE,
                                )
                                .await;
                                key = capture_session_terminal_text_vt(&zmx_name)
                                    .await
                                    .is_some_and(|screen| {
                                        crate::session_chat_claude_rating_survey::rating_survey_on_screen(
                                            &screen,
                                        )
                                    })
                                    .then_some(
                                        crate::session_chat_claude_rating_survey::SURVEY_DISMISS_KEY,
                                    );
                            }
                        }
                        presses += 1;
                        if let Some(key) = key {
                            if let Err(error) = write_session_chat_payload(
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                key,
                            )
                            .await
                            {
                                break Some(SessionChatSendError::new(
                                    SessionChatSendFailure::Write,
                                    error,
                                ));
                            }
                        }
                        let wait = crate::session_chat_composer::wait_for_session_chat_composer(
                            &zmx_name,
                            agent.as_deref(),
                            crate::session_chat_composer::SessionChatComposerWaitPolicy {
                                settle_ms: 0,
                                timeout_ms: attempt_ms,
                                // Once Escape has been sent, only positive composer
                                // evidence may release the message writes.
                                unknown_hold_ms: attempt_ms,
                            },
                            &|| job_generation != generation.load(Ordering::SeqCst),
                        )
                        .await;
                        let reason = match wait {
                            crate::session_chat_composer::SessionChatComposerWait::Ready => {
                                break None;
                            }
                            crate::session_chat_composer::SessionChatComposerWait::Cancelled => {
                                break Some(SessionChatSendError::not_attempted(
                                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                                ));
                            }
                            crate::session_chat_composer::SessionChatComposerWait::NotReady(
                                readiness,
                            ) if readiness.should_dismiss()
                                && presses < SESSION_CHAT_CLAUDE_PANEL_ESCAPES =>
                            {
                                continue;
                            }
                            crate::session_chat_composer::SessionChatComposerWait::NotReady(
                                readiness,
                            ) => readiness
                                .reason
                                .unwrap_or_else(|| SESSION_CHAT_COMPOSER_NOT_READY.to_string()),
                            crate::session_chat_composer::SessionChatComposerWait::Unknown
                                if codex =>
                            {
                                SESSION_CHAT_CODEX_SIDE_NOT_CLOSED.to_string()
                            }
                            crate::session_chat_composer::SessionChatComposerWait::Unknown => {
                                SESSION_CHAT_CLAUDE_PANEL_NOT_DISMISSED.to_string()
                            }
                        };
                        log_session_chat_paste_verification(
                            LogLevel::Error,
                            "sessionChatClaudePanelDismissFailed",
                            &project_id,
                            &session_id,
                            &zmx_name,
                            &source,
                            0,
                            timeout_ms,
                            &reason,
                        );
                        break Some(SessionChatSendError::new(
                            SessionChatSendFailure::ComposerNotReady,
                            reason,
                        ));
                    };
                    if let Some(error) = failure {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::WaitForComposer {
                    agent,
                    settle_ms,
                    timeout_ms,
                } => {
                    composer_agent = crate::agents::identity::normalize_agent_id(agent.as_deref())
                        .as_deref()
                        .filter(|agent| {
                            matches!(
                                *agent,
                                "claude" | "openclaude" | "codex" | "grok" | "empryo"
                            )
                        })
                        .map(str::to_string);
                    let wait = composer_repaint::wait_for_send_composer(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        agent.as_deref(),
                        crate::session_chat_composer::SessionChatComposerWaitPolicy {
                            settle_ms,
                            timeout_ms,
                            // Grok's wait requires positive readiness. For other agents,
                            // a screen this worker cannot read, or an agent with
                            // no measured signature, releases the sequence on
                            // the FIRST probe. The send is what matters; the
                            // gate is only allowed to help.
                            unknown_hold_ms: 0,
                        },
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                    let wait = match wait {
                        Ok(wait) => wait,
                        Err(error) => {
                            outcome = Err(error);
                            break;
                        }
                    };
                    match wait {
                        crate::session_chat_composer::SessionChatComposerWait::Ready
                        | crate::session_chat_composer::SessionChatComposerWait::Unknown => {}
                        crate::session_chat_composer::SessionChatComposerWait::Cancelled => {
                            outcome = Err(SessionChatSendError::not_attempted(
                                SESSION_CHAT_SEND_CANCELLED.to_string(),
                            ));
                            break;
                        }
                        crate::session_chat_composer::SessionChatComposerWait::NotReady(
                            readiness,
                        ) => {
                            let reason = readiness
                                .reason
                                .clone()
                                .unwrap_or_else(|| SESSION_CHAT_COMPOSER_NOT_READY.to_string());
                            log_session_chat_paste_verification(
                                LogLevel::Error,
                                "sessionChatComposerNotReady",
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                0,
                                timeout_ms,
                                &reason,
                            );
                            outcome = Err(SessionChatSendError::new(
                                SessionChatSendFailure::ComposerNotReady,
                                reason,
                            ));
                            break;
                        }
                    }
                }
                SessionChatSendStep::VerifyPasteLanded {
                    text,
                    settle_ms,
                    timeout_ms,
                } => {
                    let mut verification = verify_session_chat_paste_landed(
                        &zmx_name,
                        composer_agent.as_deref(),
                        &text,
                        settle_ms,
                        timeout_ms,
                        &generation,
                        job_generation,
                    )
                    .await;
                    // A mid-turn agent's paste is late, not lost: typing it again would stack a
                    // second copy behind it (busy_input_wait.rs).
                    if let (SessionChatPasteVerification::Absent, Some(probe)) =
                        (verification, &mid_turn_probe)
                    {
                        let (zmx, agent, body, current): (&str, Option<&str>, &str, &AtomicU64) =
                            (&zmx_name, composer_agent.as_deref(), &text, &generation);
                        let shown = wait_out_busy_agent(
                            probe,
                            &|| job_generation != current.load(Ordering::SeqCst),
                            move || async move {
                                verify_session_chat_paste_landed(
                                    zmx,
                                    agent,
                                    body,
                                    0,
                                    0,
                                    current,
                                    job_generation,
                                )
                                .await
                                    == SessionChatPasteVerification::Landed
                            },
                        )
                        .await;
                        verification = match shown {
                            BusyAgentWait::Settled => {
                                record_busy_agent_wait(
                                    &project_id,
                                    &session_id,
                                    "its paste showed in the input box",
                                );
                                SessionChatPasteVerification::Landed
                            }
                            BusyAgentWait::Stalled => {
                                outcome = Err(busy_agent_stalled());
                                break;
                            }
                            BusyAgentWait::NotMidTurn | BusyAgentWait::TurnEnded => {
                                SessionChatPasteVerification::Absent
                            }
                            BusyAgentWait::Cancelled => SessionChatPasteVerification::Cancelled,
                        };
                    }
                    match verification {
                        SessionChatPasteVerification::Landed => {}
                        SessionChatPasteVerification::Unreadable => {
                            /*
                            The observation channel itself is down, so neither
                            "landed" nor "absent" can be shown. Submitting is
                            the pre-2026-08-24 behaviour and the only choice
                            that still delivers messages on a host whose screen
                            captures do not work — but it is never silent.
                            */
                            log_session_chat_paste_verification(
                                LogLevel::Warn,
                                "sessionChatPasteVerificationSkipped",
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                text.len(),
                                timeout_ms,
                                "The session screen could not be read, so the pasted message could not be verified before Enter.",
                            );
                        }
                        SessionChatPasteVerification::Absent => {
                            log_session_chat_paste_verification(
                                LogLevel::Error,
                                "sessionChatPasteVerificationFailed",
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                text.len(),
                                timeout_ms,
                                SESSION_CHAT_PASTE_NOT_ACCEPTED,
                            );
                            outcome = Err(SessionChatSendError::new(
                                SessionChatSendFailure::Write,
                                SESSION_CHAT_PASTE_NOT_ACCEPTED.to_string(),
                            ));
                            break;
                        }
                        SessionChatPasteVerification::Cancelled => {
                            outcome = Err(SessionChatSendError::not_attempted(
                                SESSION_CHAT_SEND_CANCELLED.to_string(),
                            ));
                            break;
                        }
                    }
                }
                SessionChatSendStep::WaitOutBusyAgent(probe) => {
                    mid_turn_probe = Some(probe);
                }
                SessionChatSendStep::KeepSinglePaste { agent, text } => {
                    if let Err(error) = keep_single_session_chat_paste(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &agent,
                        &text,
                        &generation,
                        job_generation,
                    )
                    .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::DriveSessionChatRewind { job_id } => {
                    /*
                    The driver owns its own failure taxonomy (which dialog step
                    disagreed with the screen) and publishes it to the waiting
                    HTTP handler through its job registry, so nothing is mapped
                    onto the send failures here. It is the only step of its job,
                    so there is no later write for an error to have to abort.
                    */
                    crate::session_chat_rewind::run_session_chat_rewind_job(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        job_id,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                }
                SessionChatSendStep::DriveCodexModelPicker { job_id } => {
                    crate::session_chat_codex_picker::run_codex_model_picker_job(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        job_id,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                }
                SessionChatSendStep::DriveProviderModelPicker { job_id } => {
                    crate::session_chat_codex_picker::run_provider_model_picker_job(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        job_id,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                }
                SessionChatSendStep::DriveClaudeModelPicker { job_id } => {
                    crate::session_chat_codex_picker::run_claude_model_picker_job(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        job_id,
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                }
            }
        }
        if let Some(completion) = completion.take() {
            let _ = completion.send(outcome);
        }
    }
}
