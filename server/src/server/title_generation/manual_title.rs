use super::*;

/*
CDXC:SessionTitles 2026-07-29:
Rename-modal "Generate Name" reuses the first-prompt auto-title machinery for
an existing session: the same generation agent command summarizes the pasted
text into a short title, the same `gxserverFirstPromptAutoTitleStatus:
"running"` state drives the session card's generating chrome, and the same
staged zmx command text plus delayed Enter renames the Agent CLI thread. The
manual path intentionally skips first-prompt eligibility gates (the user asked
explicitly), kills any composer draft with Ctrl+U before staging and restores
it with Ctrl+Y after the submit, and applies the generated title with
`titleSource: "generated"`.
*/
pub(crate) async fn handle_generate_session_title_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "generateSessionTitle requires projectId and sessionId.".to_string(),
            },
        );
    }
    let generation_agent = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let generation_command = params
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let attempt_id = Uuid::new_v4().to_string();
    {
        let db = match open_gxserver_database(&state.paths) {
            Ok(db) => db,
            Err(error) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "internalError",
                        message: format!("SQLite gxserver state error: {error}"),
                    },
                );
            }
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = match repository.get_session(&project_id, &session_id) {
            Ok(Some(session)) => session,
            Ok(None) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "notFound",
                        message: "The session no longer exists.".to_string(),
                    },
                );
            }
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
        /*
        CDXC:SessionTitles 2026-07-29:
        An empty `text` asks the job to summarize the session's recent
        transcript user prompts. Only agents with a known local transcript
        format support that, so other agents keep requiring pasted text.
        */
        if text.is_empty() {
            let session_agent =
                crate::session_chat_follower::session_chat_agent_for_session(&session);
            if !crate::agent_transcripts::agent_supports_session_history_title_source(
                session_agent.as_deref(),
            ) {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message:
                            "generateSessionTitle requires text for this agent; only Claude Code, Codex, Cursor CLI, Antigravity CLI, and Empryo sessions can generate from recent messages."
                                .to_string(),
                    },
                );
            }
        }
        let mut runtime_settings = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("running"),
        );
        runtime_settings.insert(
            FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
            json!(attempt_id.clone()),
        );
        runtime_settings.insert(
            "gxserverManualTitleGenerationRequestedAt".to_string(),
            json!(now_iso()),
        );
        if let Some(agent) = generation_agent.as_deref() {
            runtime_settings.insert("firstPromptTitleGenerationAgent".to_string(), json!(agent));
        }
        if let Some(command) = generation_command.as_deref() {
            runtime_settings.insert(
                "firstPromptTitleGenerationCommand".to_string(),
                json!(command),
            );
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id.clone()));
        update.insert("sessionId".to_string(), json!(session_id.clone()));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        if let Err(error) = repository.update_session(&update) {
            return domain_error_response(endpoint_path, request_id, error);
        }
    }
    schedule_delta_for_ids(state, &project_id, &session_id);
    let job_state = state.clone();
    let job_project_id = project_id.clone();
    let job_session_id = session_id.clone();
    tokio::spawn(async move {
        let _ = run_manual_session_title_generation_job(
            job_state.clone(),
            job_project_id.clone(),
            job_session_id.clone(),
            text,
            attempt_id.clone(),
        )
        .await;
        mark_first_prompt_auto_title_failed_if_current_attempt(
            &job_state,
            &job_project_id,
            &job_session_id,
            &attempt_id,
        );
    });
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "started": true })),
    )
}

pub(crate) async fn run_manual_session_title_generation_job(
    state: AppState,
    project_id: String,
    session_id: String,
    text: String,
    attempt_id: String,
) -> Result<(), ()> {
    let (project_path, session) = {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        let Some(project) = repository.get_project(&project_id).map_err(|_| ())? else {
            return Ok(());
        };
        (
            read_session_text(&project, "path")
                .unwrap_or_else(|| state.paths.home_dir.to_string_lossy().to_string()),
            session,
        )
    };
    if !is_current_first_prompt_auto_title_attempt(&session, &attempt_id) {
        return Ok(());
    }
    /*
    CDXC:SessionTitles 2026-07-29:
    Empty text means "name this session from what the user recently asked it".
    Resolve the provider transcript via the hook-captured session identity and
    summarize the last few visible user prompts; failing to find any is a real
    failure so the card's generating state resolves instead of hanging.
    */
    let (source_text, source_max_length) = if text.trim().is_empty() {
        let Some(source) = session_history_title_source(&session) else {
            return Err(());
        };
        (source, GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MAX_LENGTH)
    } else {
        (text, GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH)
    };
    let title = generate_first_prompt_session_title(
        &state,
        Some(&project_path),
        &source_text,
        source_max_length,
        &session,
    )
    .await
    .map_err(|_| ())?;
    let command_text =
        agent_session_title_command(first_prompt_agent_name(&session).as_deref(), &title);
    // A Pi or OMP with Ghostex's chat channel open is named through its own API instead of a typed
    // `/name` or `/rename` (CDXC:PiChatChannel 2026-11-05 in pi_chat_channel.rs); OMP's extension
    // rename is stored as user-set, exactly like its `/rename`.
    // OpenCode is renamed in its service (`PATCH /api/session/{id}`), not with a typed `/rename`.
    let named_by_channel = match normalize_agent_name(first_prompt_agent_name(&session).as_deref())
        .as_deref()
    {
        Some("pi" | "omp") => {
            crate::pi_chat_channel::set_session_name(&project_id, &session_id, &title).await
        }
        Some("opencode") => {
            let target = session.clone();
            let name = title.clone();
            tokio::task::spawn_blocking(move || {
                crate::session_chat_opencode::session_id(&target)
                    .and_then(|id| crate::session_chat_opencode::rename(&id, &name))
                    .is_ok()
            })
            .await
            .unwrap_or(false)
        }
        _ => false,
    };
    {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(latest_session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        if !is_current_first_prompt_auto_title_attempt(&latest_session, &attempt_id) {
            return Ok(());
        }
        /*
        CDXC:SessionChat 2026-08-24:
        Clear the composer, stage the rename command, and submit it — as ONE
        queued job. These were separate zmx dispatches spread across
        `tokio::time::sleep`s, and the first of them is a composer kill: landing
        that between another sequence's paste and its Enter deletes the user's
        message and submits an empty line.

        Like the auto-title job, folding gives up the mid-delay attempt re-check
        that used to gate the Enter; a staged `/rename …` left unsent would be
        worse than submitting it. The re-check below still gates persistence.

        CDXC:SessionChat 2026-08-26:
        The clear is the measured burst now, and there is no Ctrl+Y restore.
        Constraint: the command must reach an EMPTY composer, multi-line drafts
        included. The single Ctrl+U this job used to send kills one logical
        line, so a two-line draft left its first line in place and the rename
        was submitted glued to it — which is exactly what the 2N-1 burst sized
        for the command text (its own write, then the settle) fixes.

        A burst and a Ctrl+Y are incompatible by construction: the yank returns
        the LAST kill, so after 2N-1 kills it restores at most a fragment of a
        multi-line draft, and after the trailing Ctrl+K kills it restores
        nothing at all. Restoring a fragment is worse than not restoring, so the
        yank is gone rather than left as decoration. Real preservation would
        mean the Ctrl+G prompt-editor handshake
        (`SessionChatSendStep::PreserveTerminalDraft`), and that is not usable
        here: a CLI that does not answer it fails the step, which aborts the
        rest of the sequence and would leave the session unrenamed for up to the
        16s handshake timeout, and it publishes the draft as a user-facing Saved
        Prompt. So this follows the chat-send policy instead — an app-owned
        write owns the composer and discards residue, and terminal→chat view
        switching stays the loss-safe path for text the user wants to keep.
        */
        let agent_name = first_prompt_agent_name(&latest_session);
        // Empryo opens its command palette on the burst's Ctrl+K, so it takes the chat send's own
        // Ctrl+U clear and submit, as `/api/sendSessionMessage` does (zmx/endpoint.rs).
        let manual_title_steps =
            if normalize_agent_name(agent_name.as_deref()).as_deref() == Some("empryo") {
                crate::session_chat_send::build_session_chat_message_steps(
                    agent_name.as_deref(),
                    &command_text,
                    &[],
                    false,
                )
            } else {
                let mut steps = crate::session_chat_send::build_agent_tui_clear_input_steps(
                    Some("manual-title-draft-kill"),
                    &command_text,
                );
                steps.extend([
                    crate::session_chat_send::SessionChatSendStep::WriteFrom {
                        source: "manual-title-command".to_string(),
                        payload: command_text.clone(),
                    },
                    crate::session_chat_send::SessionChatSendStep::SleepMs(
                        GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS,
                    ),
                    crate::session_chat_send::SessionChatSendStep::WriteFrom {
                        source: "manual-title-submit".to_string(),
                        payload: crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
                    },
                ]);
                steps
            };
        if !named_by_channel {
            crate::session_chat_send::enqueue_session_write_sequence(
                &latest_session,
                &project_id,
                &session_id,
                "manual-title",
                manual_title_steps,
            )
            .map_err(|_| ())?;
        }
        /*
        CDXC:SessionChat 2026-08-23:
        Recorded beside the dispatch rather than inside the zmx path, because
        the same path also carries the Ctrl+U draft kill and the bare `\r`
        submit, and neither is something to tell the reader about. Codex writes
        NOTHING to its rollout for a command it intercepts, so without this row
        a session that renamed itself mid-conversation left no trace in chat.
        */
        crate::session_chat_app_command::record_session_chat_app_command(
            &project_id,
            &session_id,
            &command_text,
        );
    }
    tokio::time::sleep(Duration::from_millis(
        GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS,
    ))
    .await;
    {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(latest_session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        if !is_current_first_prompt_auto_title_attempt(&latest_session, &attempt_id) {
            return Ok(());
        }
        let mut runtime_settings = latest_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.remove(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY);
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleAppliedAt".to_string(),
            json!(now_iso()),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleReason".to_string(),
            json!("manual-generate-name"),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("applied"),
        );
        runtime_settings.insert("titleSource".to_string(), json!("generated"));
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id.clone()));
        update.insert("sessionId".to_string(), json!(session_id.clone()));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        update.insert("title".to_string(), json!(title));
        repository.update_session(&update).map_err(|_| ())?;
    }
    schedule_delta_for_ids(&state, &project_id, &session_id);
    Ok(())
}
