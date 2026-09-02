use super::*;

pub(crate) fn schedule_agent_title_metadata_check(
    state: AppState,
    project_id: String,
    session_id: String,
) {
    /*
    CDXC:GxserverAgentTitles 2026-06-21-15:35:
    Agent CLI renames are accepted asynchronously after Ghostex submits `/rename`. Match TypeScript gxserver's three-second trailing metadata check so Rust promotes the agent's own session-metadata title (Codex `thread_name`, Claude `custom-title`) and broadcasts a presentation delta after the CLI writes it.
    */
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS,
        ))
        .await;
        let Ok(db) =
            open_gxserver_database_with_busy_timeout(&state.paths, Duration::from_secs(10))
        else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(changed) = reconcile_agent_metadata_title_for_session(
            &repository,
            &project_id,
            &session_id,
            &state.paths.home_dir,
            "metadata-mismatch",
        ) else {
            return;
        };
        if changed {
            let _ = schedule_presentation_session_delta(
                &state,
                &db,
                &repository,
                &project_id,
                &session_id,
            );
        }
    });
}

pub(crate) fn schedule_agent_title_metadata_checks_for_sessions(
    state: &AppState,
    sessions: &[Value],
) {
    for session in sessions {
        if !should_check_agent_metadata_title_for_project_status(session) {
            continue;
        }
        let Some(project_id) = read_session_text(session, "projectId") else {
            continue;
        };
        let Some(session_id) = read_session_text(session, "sessionId") else {
            continue;
        };
        schedule_agent_title_metadata_check(state.clone(), project_id, session_id);
    }
}

pub(crate) fn should_check_agent_metadata_title_for_project_status(session: &Value) -> bool {
    if !is_agent_associated_session_for_project_status(session) {
        return false;
    }
    if read_runtime_text(session, "pendingAgentTitleRequestStatus").as_deref() == Some("pending") {
        return true;
    }
    read_runtime_text(session, "titleMetadataSource").as_deref() != Some("agent-metadata")
        && trusted_resume_title_for_project_status(session).is_none()
}

pub(crate) fn is_agent_associated_session_for_project_status(session: &Value) -> bool {
    read_session_text(session, "kind").as_deref() == Some("agent")
        || read_session_text(session, "agentId").is_some()
        || read_runtime_text(session, "agentName").is_some()
        || read_runtime_text(session, "agentId").is_some()
        || read_runtime_text(session, "agentSessionId").is_some()
        || read_runtime_text(session, "agentSessionPath").is_some()
}

pub(crate) fn trusted_resume_title_for_project_status(session: &Value) -> Option<String> {
    let title = read_session_text(session, "title")?;
    let title_source = normalize_project_status_title_source(
        read_runtime_text(session, "titleSource")
            .or_else(|| read_runtime_text(session, "restoreTitleSource"))
            .as_deref(),
        &title,
    );
    if title_source == "placeholder" || is_terminal_auto_working_directory_title(session) {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_project_status_resume_title(&visible)).then_some(visible)
}

pub(crate) fn normalize_project_status_title_source(
    value: Option<&str>,
    title: &str,
) -> &'static str {
    match value {
        Some("browser-auto") => "browser-auto",
        Some("generated") => "generated",
        Some("placeholder") => "placeholder",
        Some("terminal-auto") => "terminal-auto",
        Some("user") => "user",
        _ if is_temporary_project_status_title(title) => "placeholder",
        _ => "user",
    }
}

pub(crate) fn is_rejected_project_status_resume_title(title: &str) -> bool {
    let normalized = title.trim();
    let lower = normalized.to_ascii_lowercase();
    normalized == "ð^ß^Ñ»"
        || is_temporary_project_status_title(normalized)
        || normalized.starts_with('ð') && normalized.ends_with('»')
        || is_gxserver_session_id(normalized)
        || normalized.chars().any(char::is_control)
        || lower.starts_with("codex ")
        || lower.starts_with("claude ")
        || lower.starts_with("cursor-agent ")
        || lower.starts_with("opencode ")
}

pub(crate) fn is_temporary_project_status_title(title: &str) -> bool {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .eq_ignore_ascii_case("search by text")
}

#[derive(Clone)]
pub(crate) struct ForkInitialRenameTarget {
    agent_name: String,
    project_id: String,
    session_id: String,
    title: String,
}

pub(crate) fn fork_initial_rename_target(
    endpoint_path: &str,
    result: &Value,
) -> Option<ForkInitialRenameTarget> {
    if endpoint_path != "/api/forkSession" {
        return None;
    }
    let fork = result.get("fork")?;
    let session = fork.get("session")?;
    let agent_name = fork
        .get("plan")
        .and_then(|plan| plan.get("agentId"))
        .and_then(Value::as_str)
        .or_else(|| session.get("agentId").and_then(Value::as_str))?
        .trim();
    /*
    Codex 0.150 names an unnamed fork from its first user turn. Do not install
    Ghostex's provisional `/rename Fork: ...` first, because that makes the
    provider thread non-empty and suppresses Codex's own automatic title.
    */
    if normalize_agent_name(Some(agent_name)).as_deref() == Some("codex") {
        return None;
    }
    Some(ForkInitialRenameTarget {
        agent_name: agent_name.to_string(),
        project_id: read_session_text(session, "projectId")?,
        session_id: read_session_text(session, "sessionId")?,
        title: read_session_text(session, "title")?,
    })
}

pub(crate) fn schedule_fork_initial_rename(state: AppState, target: ForkInitialRenameTarget) {
    /*
    CDXC:GxserverForkTitles 2026-07-11:
    Fork provider startup already owns the resumed CLI process. Wait for its
    composer, then submit the provisional `Fork: <old title>` through zmx's
    separate text/Enter path. Pi uses `/name`, Hermes Agent uses `/title`, and
    Claude uses `/rename`; Codex keeps the fork unnamed so its own first-turn
    title generation can name it.
    If the user has already sent the fork's first prompt, its generated-title
    job wins and this provisional rename is skipped.

    CDXC:SessionChatComposerReady 2026-08-26: this used to be a blind four-second
    sleep. A rename typed before the composer exists is not merely lost — the
    slash command lands as literal text in whatever screen IS up.
    */
    tokio::spawn(async move {
        crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
            &state.paths,
            state.metadata.server_id.as_str(),
            &target.project_id,
            &target.session_id,
            crate::session_chat_composer::SessionChatComposerWaitPolicy {
                settle_ms: 0,
                timeout_ms: GXSERVER_PROVIDER_COMPOSER_WAIT_TIMEOUT_MS,
                unknown_hold_ms: GXSERVER_FORK_INITIAL_RENAME_READY_DELAY_MS,
            },
        )
        .await;
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(Some(session)) = repository.get_session(&target.project_id, &target.session_id)
        else {
            return;
        };
        if read_runtime_text(&session, "gxserverForkInitialRenameStatus").as_deref()
            != Some("pending")
            || read_runtime_text(&session, "gxserverFirstPromptAutoTitleStatus").is_some()
        {
            return;
        }
        let command = agent_session_title_command(Some(&target.agent_name), &target.title);
        // CDXC:SessionChatAppCommands 2026-08-23: see the auto-title dispatch.
        crate::session_chat_app_command::record_session_chat_app_command(
            &target.project_id,
            &target.session_id,
            &command,
        );
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(target.project_id.clone()));
        params.insert("sessionId".to_string(), json!(target.session_id.clone()));
        params.insert(
            "diagnosticInputSource".to_string(),
            json!("fork-title-command"),
        );
        params.insert("submit".to_string(), Value::Bool(true));
        params.insert("text".to_string(), Value::String(command));
        let status = if dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionMessage",
            &params,
        )
        .is_ok()
        {
            "applied"
        } else {
            "failed"
        };
        let Ok(Some(latest_session)) =
            repository.get_session(&target.project_id, &target.session_id)
        else {
            return;
        };
        let mut runtime_settings = latest_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert("gxserverForkInitialRenameStatus".to_string(), json!(status));
        runtime_settings.insert(
            "gxserverForkInitialRenameUpdatedAt".to_string(),
            json!(now_iso()),
        );
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(target.project_id.clone()));
        update.insert("sessionId".to_string(), json!(target.session_id.clone()));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let _ = repository.update_session(&update);
        schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
    });
}

#[derive(Clone)]
pub(crate) struct FirstUserInputDraftTarget {
    draft: String,
    project_id: String,
    session_id: String,
}

/*
CDXC:GxserverFirstUserInputDraft 2026-08-20:
A session created with `runtimeSettings.firstUserInputDraft` gets that text
typed into its agent CLI composer once, and never sent. The claim happens
synchronously while the provider start is still being answered — the marker
flips from `pending` to `typing` before the readiness delay — so a second
provider start (wake, re-attach, restart, a concurrent attach) finds a
non-pending marker and types nothing.
*/
pub(crate) fn claim_first_user_input_draft(
    repository: &DomainRepository<'_>,
    endpoint_path: &str,
    result: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<FirstUserInputDraftTarget> {
    if endpoint_path != "/api/startSessionProvider"
        || result.get("started").and_then(Value::as_bool) != Some(true)
    {
        return None;
    }
    let session = repository.get_session(project_id, session_id).ok()??;
    if read_runtime_text(&session, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref() != Some("pending")
    {
        return None;
    }
    let draft = read_first_user_input_draft(&session)?;
    update_first_user_input_draft_status(repository, project_id, session_id, "typing").ok()?;
    Some(FirstUserInputDraftTarget {
        draft,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    })
}

pub(crate) fn update_first_user_input_draft_status(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    status: &str,
) -> Result<(), DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(());
    };
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert(FIRST_USER_INPUT_DRAFT_STATUS_KEY.to_string(), json!(status));
    runtime_settings.insert(
        FIRST_USER_INPUT_DRAFT_UPDATED_AT_KEY.to_string(),
        json!(now_iso()),
    );
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)?;
    Ok(())
}

pub(crate) fn schedule_first_user_input_draft(state: AppState, target: FirstUserInputDraftTarget) {
    /*
    CDXC:GxserverFirstUserInputDraft 2026-08-20:
    Same readiness window the fork's initial rename uses: the provider start
    owns a freshly launched CLI whose composer is not accepting input yet. The
    draft then goes through zmx's text path with `submit: false`, so no Enter
    and no trailing carriage return is ever produced — the composer keeps the
    text staged for the user to write around.

    CDXC:SessionChatComposerReady 2026-08-26: the readiness window is now a real
    composer wait rather than a blind four-second sleep. Staging a draft into a
    trust dialog types the user's text at it and then discards it on the next
    repaint, with nothing anywhere to recover it from.
    */
    tokio::spawn(async move {
        crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
            &state.paths,
            state.metadata.server_id.as_str(),
            &target.project_id,
            &target.session_id,
            crate::session_chat_composer::SessionChatComposerWaitPolicy {
                settle_ms: 0,
                timeout_ms: GXSERVER_PROVIDER_COMPOSER_WAIT_TIMEOUT_MS,
                unknown_hold_ms: GXSERVER_FIRST_USER_INPUT_DRAFT_READY_DELAY_MS,
            },
        )
        .await;
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        /*
        CDXC:SessionChatLaunchDraft 2026-09-02:
        The marker was claimed (`typing`) before this wait, and a Chat launch
        may have taken the draft for the chat composer in the meantime
        (`claim_first_user_input_draft_for_chat`). Re-read it here so a draft
        that already lives in the chat composer is never also typed into the
        parked terminal, where it would sit unseen and later be stashed as a
        stray Saved Prompt by the chat send.
        */
        let still_typing = repository
            .get_session(&target.project_id, &target.session_id)
            .ok()
            .flatten()
            .is_some_and(|session| {
                read_runtime_text(&session, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref()
                    == Some("typing")
            });
        if !still_typing {
            return;
        }
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(target.project_id.clone()));
        params.insert("sessionId".to_string(), json!(target.session_id.clone()));
        params.insert(
            "diagnosticInputSource".to_string(),
            json!("first-user-input-draft"),
        );
        params.insert("submit".to_string(), Value::Bool(false));
        params.insert("text".to_string(), Value::String(target.draft));
        let status = if dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionMessage",
            &params,
        )
        .is_ok()
        {
            "applied"
        } else {
            "failed"
        };
        let _ = update_first_user_input_draft_status(
            &repository,
            &target.project_id,
            &target.session_id,
            status,
        );
        schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
    });
}

/*
CDXC:SessionChatLaunchDraft 2026-09-02:
The Chat-side half of the first-input draft. A session that opens straight in
Chat (Default Agent View = Chat, or a Handoff / Export conversation for a Chat
user) must find the staged text in the CHAT composer, not typed into the
terminal that Chat parks behind it. The terminal typing waits for the CLI
composer to become ready, so at the moment Chat activates the marker is still
`pending` or `typing` and the text has not reached any composer yet. Claiming
it here flips the marker to `transferred`, which the typing task honours by
doing nothing, and hands the verbatim text back for the chat composer.

Only a marker in one of those two states is claimable: `applied` means the
terminal already holds the text and the ordinary terminal → chat capture is
the right tool, and anything else means there is nothing to hand over.
*/
pub(crate) fn claim_first_user_input_draft_for_chat(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(None);
    };
    if !matches!(
        read_runtime_text(&session, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref(),
        Some("pending" | "typing")
    ) {
        return Ok(None);
    }
    let Some(draft) = read_first_user_input_draft(&session) else {
        return Ok(None);
    };
    update_first_user_input_draft_status(repository, project_id, session_id, "transferred")?;
    Ok(Some(draft))
}

/// `/api/claimSessionChatLaunchDraft`: the launch-time claim above as an
/// endpoint of its own. It is deliberately NOT a flag on
/// `/api/handoffSessionChatDraft`: that endpoint's fallback is the
/// prompt-editor handshake against the live CLI, which must never run against
/// a CLI that is still booting, and a daemon predating this endpoint would
/// silently ignore the flag and do exactly that. An unknown endpoint fails
/// closed instead.
pub(crate) fn handle_claim_session_chat_launch_draft_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match crate::domain::read_domain_rpc_params(body) {
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
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "claimSessionChatLaunchDraft requires projectId and sessionId."
                    .to_string(),
            },
        );
    }
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
            )
        }
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let content = match claim_first_user_input_draft_for_chat(&repository, &project_id, &session_id)
    {
        Ok(content) => content.unwrap_or_default(),
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if !content.is_empty() {
        schedule_delta_for_ids(state, &project_id, &session_id);
    }
    routed_json(
        Some(endpoint_path),
        axum::http::StatusCode::OK,
        crate::protocol::rpc_success(
            request_id,
            json!({ "content": content, "transferred": !content.is_empty() }),
        ),
    )
}

pub(crate) fn schedule_first_prompt_auto_title_job(
    state: AppState,
    project_id: String,
    session_id: String,
    attempt_id: String,
) {
    /*
    CDXC:GxserverSessionTitle 2026-06-21-19:26:
    Rust server must finish the same first-prompt auto-title flow as TypeScript gxserver after hooks claim a job: decide eligibility centrally, generate or stage the provider rename command, and persist applied/skipped/failed status.

    CDXC:GxserverSessionTitle 2026-07-02-15:10:
    gxserver submits the staged rename command itself with a separate zmx `\r` write instead of asking clients to send a native Enter on the running→applied presentation transition. Client-side submission only worked for currently visible native panes: `sessions[sessionId]` has no Ghostty surface for background/automation-started sessions, so their staged `/rename` sat unsubmitted in the agent composer forever. A separate PTY-level CR after a settle delay is a real Enter keypress to agent prompt editors (a CR appended to the same text payload is treated as a pasted newline), works for invisible panes, remote daemons, and GPUI, and removes the fragile transition-observation race entirely.

    CDXC:GxserverSessionTitle 2026-08-03:
    A cancelled prompt can be explicitly submitted again with identical text.
    Bind every spawned job to its claim attempt so the cancelled subprocess
    cannot apply or fail the replacement job after it eventually exits.
    */
    tokio::spawn(async move {
        if let Err(()) = run_first_prompt_auto_title_job(
            state.clone(),
            project_id.clone(),
            session_id.clone(),
            attempt_id.clone(),
        )
        .await
        {
            mark_first_prompt_auto_title_failed_if_current_attempt(
                &state,
                &project_id,
                &session_id,
                &attempt_id,
            );
        }
    });
}

#[derive(Clone)]
pub(crate) struct FirstPromptAutoTitleDecision {
    pub(crate) normalized_prompt: Option<String>,
    pub(crate) reason: String,
    pub(crate) should_run: bool,
    pub(crate) strategy: Option<&'static str>,
}

pub(crate) async fn run_first_prompt_auto_title_job(
    state: AppState,
    project_id: String,
    session_id: String,
    attempt_id: String,
) -> Result<(), ()> {
    let (project_path, session, prompt, decision) = {
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
        let prompt = read_runtime_text(&session, "firstUserMessage");
        let decision = decide_first_prompt_auto_title(&session, prompt.as_deref(), true);
        (
            read_session_text(&project, "path")
                .unwrap_or_else(|| state.paths.home_dir.to_string_lossy().to_string()),
            session,
            prompt,
            decision,
        )
    };

    if !is_current_first_prompt_auto_title_attempt(&session, &attempt_id) {
        return Ok(());
    }
    if !decision.should_run || decision.normalized_prompt.is_none() || decision.strategy.is_none() {
        mark_first_prompt_auto_title_skipped(
            &state,
            &project_id,
            &session_id,
            &attempt_id,
            &decision.reason,
        );
        return Ok(());
    }

    let title = if matches!(
        decision.strategy,
        Some("generateTitleAndRename" | "generateTitleAndName")
    ) {
        Some(
            generate_first_prompt_session_title(
                &state,
                Some(&project_path),
                decision.normalized_prompt.as_deref().ok_or(())?,
                GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
                &session,
            )
            .await
            .map_err(|_| ())?,
        )
    } else {
        None
    };

    let command_text = match decision.strategy {
        Some("sendBareRenameCommand") => "/rename".to_string(),
        Some("generateTitleAndName") => format!("/name {}", title.as_deref().ok_or(())?),
        _ => format!("/rename {}", title.as_deref().ok_or(())?),
    };
    let uses_bare_rename = decision.strategy == Some("sendBareRenameCommand");
    /*
    CDXC:FirstPromptRealMessage 2026-08-28:
    A bare `/rename` asks Claude to name the session from the conversation, so
    submitting it before the conversation holds a real user message makes
    Claude name the session after startup noise — `/model` and `/effort` local
    commands produced sticky names like "set-default-model-opus". Gate the send
    on the transcript itself: wait briefly for the first visible user prompt to
    land, and if none arrives mark the attempt cancelled (re-armable by the
    next explicit prompt submit), never skipped/applied, so the session still
    gets its auto title once the user actually says something.
    */
    if uses_bare_rename
        && !wait_for_visible_first_user_message(&state, &project_id, &session_id, &attempt_id)
            .await?
    {
        mark_first_prompt_auto_title_cancelled_without_user_message(
            &state,
            &project_id,
            &session_id,
            &attempt_id,
        );
        return Ok(());
    }
    {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(latest_session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        if !is_current_first_prompt_auto_title_attempt_for_prompt(
            &latest_session,
            &attempt_id,
            decision.normalized_prompt.as_deref(),
        ) {
            return Ok(());
        }
        let title_metadata_baseline = uses_bare_rename
            .then(|| {
                crate::agents::agent_metadata_title_observation(
                    &state.paths.home_dir,
                    &latest_session,
                )
            })
            .flatten();
        /*
        CDXC:SessionChatSerializedWriters 2026-08-24:
        Command text, settle, and Enter are ONE queued job. They used to be two
        separate zmx dispatches around a bare `tokio::time::sleep`, which is
        exactly the shape that let this job's bytes land inside another
        sequence: it is triggered by the FIRST user prompt, so it runs while
        that prompt may still be mid-delivery. A single job also means no other
        writer can slip between the staged command and its submit.

        What the fold gives up is the re-check that used to sit inside the
        delay: the Enter is no longer skipped when a newer auto-title attempt
        supersedes this one mid-settle. Submitting the staged command is the
        better of the two outcomes anyway — the alternative left `/rename …`
        sitting unsent in the user's composer. The re-check below still gates
        everything that is persisted.

        CDXC:SessionChatSerializedWriters 2026-08-26:
        The command lands on the SAME composer line the user types into, so the
        job opens with the measured clear burst every other writer of that line
        uses — as its own write, followed by the settle, because a burst glued
        to the command text is inserted as literal text instead of read as kill
        keys. This job used to carry no clear at all, so a draft in the composer
        got `/rename …` appended to it and the rename was submitted as one
        corrupted line. Discarding that residue is right here rather than merely
        tolerable: the job is triggered by the user's FIRST prompt having just
        been submitted, so the composer is expected empty and anything left on
        it belongs to the prompt that already went out. No restore, for the same
        reason.
        */
        let mut auto_title_steps = crate::session_chat_send::build_agent_tui_clear_input_steps(
            Some("auto-title-clear"),
            &command_text,
        );
        auto_title_steps.extend([
            crate::session_chat_send::SessionChatSendStep::WriteFrom {
                source: "auto-title-command".to_string(),
                payload: command_text.clone(),
            },
            crate::session_chat_send::SessionChatSendStep::SleepMs(
                GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS,
            ),
            crate::session_chat_send::SessionChatSendStep::WriteFrom {
                source: "auto-title-submit".to_string(),
                payload: crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
            },
        ]);
        crate::session_chat_send::enqueue_session_write_sequence(
            &latest_session,
            &project_id,
            &session_id,
            "auto-title",
            auto_title_steps,
        )
        .map_err(|_| ())?;
        /*
        CDXC:SessionChatAppCommands 2026-08-23:
        Recorded beside the dispatch rather than inside the zmx path, because
        the same path also carries the Ctrl+U draft kill and the bare `\r`
        submit, and neither is something to tell the reader about. Codex writes
        NOTHING to its rollout for a command it intercepts, so without this row
        a session that renamed itself mid-conversation left no trace in chat.
        */
        if uses_bare_rename {
            crate::session_chat_app_command::record_session_chat_app_command_with_title_metadata_baseline(
                &project_id,
                &session_id,
                &command_text,
                title_metadata_baseline,
            );
        } else {
            crate::session_chat_app_command::record_session_chat_app_command(
                &project_id,
                &session_id,
                &command_text,
            );
        }
    }

    /*
    CDXC:GxserverSessionTitle 2026-07-02-15:10:
    The staged command is submitted by a separate zmx `\r` write after a settle
    delay so agent prompt editors read it as a real Enter keypress rather than
    as part of a pasted payload. Both writes and the delay between them are now
    steps of the queued job above (CDXC:SessionChatSerializedWriters); this wait
    mirrors that delay so the status/title below is persisted only once the
    submit has had its window, and the database handle is reopened after it
    because rusqlite connections cannot be held across await points.
    */
    tokio::time::sleep(Duration::from_millis(
        GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS,
    ))
    .await;

    let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(latest_session) = repository
        .get_session(&project_id, &session_id)
        .map_err(|_| ())?
    else {
        return Ok(());
    };
    if !is_current_first_prompt_auto_title_attempt_for_prompt(
        &latest_session,
        &attempt_id,
        decision.normalized_prompt.as_deref(),
    ) {
        return Ok(());
    }
    let mut runtime_settings = latest_session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.remove("forkFirstPromptAutoTitlePending");
    runtime_settings.remove("gxserverForkInitialRenameStatus");
    runtime_settings.remove("gxserverForkInitialRenameUpdatedAt");
    runtime_settings.remove(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY);
    runtime_settings.insert("autoTitleFromFirstPrompt".to_string(), Value::Bool(true));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleAppliedAt".to_string(),
        json!(now_iso()),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleReason".to_string(),
        json!(decision.reason),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("applied"),
    );
    if title.is_some() {
        runtime_settings.insert("titleSource".to_string(), json!("generated"));
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id.clone()));
    update.insert("sessionId".to_string(), json!(session_id.clone()));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if let Some(title) = title {
        update.insert("title".to_string(), json!(title));
    }
    repository.update_session(&update).map_err(|_| ())?;
    schedule_delta_for_ids(&state, &project_id, &session_id);
    let _ = prompt;
    Ok(())
}

/*
CDXC:FirstPromptRealMessage 2026-08-28:
The transcript is the source of truth for "the user actually said something":
`recent_session_user_prompts` already filters out `<command-name>` wrappers,
tool results, and staged slash commands, so a `/model` or `/effort` exchange
counts as nothing. Polls with the database handle dropped before every await
(rusqlite connections cannot be held across await points). Returns `true`
when a visible user prompt exists or when this attempt was superseded — the
caller's own attempt re-check then decides what to persist.
*/
async fn wait_for_visible_first_user_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    attempt_id: &str,
) -> Result<bool, ()> {
    const POLL_ATTEMPTS: u32 = 10;
    const POLL_DELAY_MS: u64 = 1_000;
    for attempt in 0..POLL_ATTEMPTS {
        let identity = {
            let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            let Some(session) = repository
                .get_session(project_id, session_id)
                .map_err(|_| ())?
            else {
                return Ok(false);
            };
            if !is_current_first_prompt_auto_title_attempt(&session, attempt_id) {
                return Ok(true);
            }
            (
                read_runtime_text(&session, "agentSessionId"),
                read_runtime_text(&session, "agentSessionPath"),
            )
        };
        if !crate::agent_transcripts::recent_session_user_prompts(
            "claude",
            identity.0.as_deref(),
            identity.1.as_deref(),
        )
        .is_empty()
        {
            return Ok(true);
        }
        if attempt + 1 < POLL_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(POLL_DELAY_MS)).await;
        }
    }
    Ok(false)
}

fn mark_first_prompt_auto_title_cancelled_without_user_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    attempt_id: &str,
) {
    let did_update =
        update_first_prompt_auto_title_runtime(state, project_id, session_id, |runtime| {
            if read_text_from_map(runtime, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY).as_deref()
                != Some(attempt_id)
                || read_text_from_map(runtime, "gxserverFirstPromptAutoTitleStatus").as_deref()
                    != Some("running")
            {
                return false;
            }
            runtime.remove(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY);
            if let Some(prompt) = read_text_from_map(runtime, "firstUserMessage") {
                runtime.insert(
                    "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
                    json!(prompt),
                );
            }
            runtime.insert(
                "gxserverFirstPromptAutoTitleCancelledAt".to_string(),
                json!(now_iso()),
            );
            runtime.insert(
                "gxserverFirstPromptAutoTitleReason".to_string(),
                json!("noVisibleUserMessage"),
            );
            runtime.insert(
                "gxserverFirstPromptAutoTitleStatus".to_string(),
                json!("cancelled"),
            );
            true
        });
    if did_update {
        schedule_delta_for_ids(state, project_id, session_id);
    }
}

pub(crate) fn mark_first_prompt_auto_title_skipped(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    attempt_id: &str,
    reason: &str,
) {
    let did_update =
        update_first_prompt_auto_title_runtime(state, project_id, session_id, |runtime| {
            if read_text_from_map(runtime, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY).as_deref()
                != Some(attempt_id)
                || read_text_from_map(runtime, "gxserverFirstPromptAutoTitleStatus").as_deref()
                    != Some("running")
            {
                return false;
            }
            runtime.remove(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY);
            runtime.insert(
                "gxserverFirstPromptAutoTitleReason".to_string(),
                json!(reason),
            );
            runtime.insert(
                "gxserverFirstPromptAutoTitleStatus".to_string(),
                json!("skipped"),
            );
            true
        });
    if did_update {
        schedule_delta_for_ids(state, project_id, session_id);
    }
}

pub(crate) fn is_current_first_prompt_auto_title_attempt(
    session: &Value,
    attempt_id: &str,
) -> bool {
    read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus").as_deref() == Some("running")
        && read_runtime_text(session, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY).as_deref()
            == Some(attempt_id)
}

pub(crate) fn is_current_first_prompt_auto_title_attempt_for_prompt(
    session: &Value,
    attempt_id: &str,
    normalized_prompt: Option<&str>,
) -> bool {
    is_current_first_prompt_auto_title_attempt(session, attempt_id)
        && normalize_first_prompt_title_prompt(
            read_runtime_text(session, "firstUserMessage").as_deref(),
        )
        .as_deref()
            == normalized_prompt
}

pub(crate) fn mark_first_prompt_auto_title_failed_if_current_attempt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    attempt_id: &str,
) {
    let did_update =
        update_first_prompt_auto_title_runtime(state, project_id, session_id, |runtime| {
            if read_text_from_map(runtime, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY).as_deref()
                != Some(attempt_id)
                || read_text_from_map(runtime, "gxserverFirstPromptAutoTitleStatus").as_deref()
                    != Some("running")
            {
                return false;
            }
            runtime.remove(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY);
            runtime.insert(
                "gxserverFirstPromptAutoTitleFailedAt".to_string(),
                json!(now_iso()),
            );
            runtime.insert(
                "gxserverFirstPromptAutoTitleStatus".to_string(),
                json!("failed"),
            );
            true
        });
    if did_update {
        schedule_delta_for_ids(state, project_id, session_id);
    }
}

pub(crate) fn update_first_prompt_auto_title_runtime<F>(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    apply: F,
) -> bool
where
    F: FnOnce(&mut Map<String, Value>) -> bool,
{
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let Ok(transaction) =
        rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)
    else {
        return false;
    };
    let repository = DomainRepository::new(&transaction, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if !apply(&mut runtime_settings) {
        return false;
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if repository.update_session(&update).is_err() {
        return false;
    }
    drop(repository);
    transaction.commit().is_ok()
}

pub(crate) fn schedule_delta_for_ids(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

/*
CDXC:ManualSessionTitleGeneration 2026-07-29:
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
        CDXC:SessionHistoryTitleSource 2026-07-29:
        An empty `text` asks the job to summarize the session's recent
        transcript user prompts. Only agents with a known local transcript
        format support that, so other agents keep requiring pasted text.
        */
        if text.is_empty() {
            let session_agent = normalize_agent_name(first_prompt_agent_name(&session).as_deref());
            if !crate::agent_transcripts::agent_supports_session_history_title_source(
                session_agent.as_deref(),
            ) {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message:
                            "generateSessionTitle requires text for this agent; only Claude Code, Codex, and Cursor CLI sessions can generate from recent messages."
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
        if let Err(()) = run_manual_session_title_generation_job(
            job_state.clone(),
            job_project_id.clone(),
            job_session_id.clone(),
            text,
            attempt_id.clone(),
        )
        .await
        {
            mark_first_prompt_auto_title_failed_if_current_attempt(
                &job_state,
                &job_project_id,
                &job_session_id,
                &attempt_id,
            );
        }
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
    CDXC:SessionHistoryTitleSource 2026-07-29:
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
        CDXC:SessionChatSerializedWriters 2026-08-24:
        Clear the composer, stage the rename command, and submit it — as ONE
        queued job. These were separate zmx dispatches spread across
        `tokio::time::sleep`s, and the first of them is a composer kill: landing
        that between another sequence's paste and its Enter deletes the user's
        message and submits an empty line.

        Like the auto-title job, folding gives up the mid-delay attempt re-check
        that used to gate the Enter; a staged `/rename …` left unsent would be
        worse than submitting it. The re-check below still gates persistence.

        CDXC:SessionChatSerializedWriters 2026-08-26:
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
        let mut manual_title_steps = crate::session_chat_send::build_agent_tui_clear_input_steps(
            Some("manual-title-draft-kill"),
            &command_text,
        );
        manual_title_steps.extend([
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
        crate::session_chat_send::enqueue_session_write_sequence(
            &latest_session,
            &project_id,
            &session_id,
            "manual-title",
            manual_title_steps,
        )
        .map_err(|_| ())?;
        /*
        CDXC:SessionChatAppCommands 2026-08-23:
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

pub(crate) fn decide_first_prompt_auto_title(
    session: &Value,
    prompt: Option<&str>,
    allow_running: bool,
) -> FirstPromptAutoTitleDecision {
    let status = read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus");
    let fork_first_prompt_rearmed = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("forkFirstPromptAutoTitlePending"))
        .and_then(Value::as_bool)
        == Some(true);
    let raw_prompt = prompt;
    let normalized_prompt = normalize_first_prompt_title_prompt(prompt);
    let cancelled_prompt = normalize_first_prompt_title_prompt(
        read_runtime_text(session, "gxserverFirstPromptAutoTitleCancelledPrompt").as_deref(),
    )
    .or_else(|| {
        normalize_first_prompt_title_prompt(
            read_runtime_text(session, "firstUserMessage").as_deref(),
        )
    });
    let is_cancelled_retry_prompt = status.as_deref() == Some("cancelled")
        && normalized_prompt.is_some()
        && normalized_prompt != cancelled_prompt;
    if (status.as_deref() == Some("running") && !allow_running)
        || matches!(status.as_deref(), Some("applied" | "failed" | "skipped"))
        || (status.as_deref() == Some("cancelled") && !is_cancelled_retry_prompt)
    {
        return FirstPromptAutoTitleDecision {
            normalized_prompt,
            reason: format!("already-{}", status.unwrap_or_default()),
            should_run: false,
            strategy: None,
        };
    }
    if !fork_first_prompt_rearmed
        && session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("autoTitleFromFirstPrompt"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        return decision(normalized_prompt, "alreadyAutoNamed", false, None);
    }
    let agent_name = first_prompt_agent_name(session);
    let strategy = first_prompt_auto_title_strategy(agent_name.as_deref());
    if strategy.is_none() {
        return decision(normalized_prompt, "unsupportedAgent", false, None);
    }
    let Some(prompt) = normalized_prompt.clone() else {
        return decision(normalized_prompt, "emptyPrompt", false, strategy);
    };
    if is_first_prompt_meta_prompt(&prompt) {
        return decision(Some(prompt), "metaPrompt", false, strategy);
    }
    if is_first_prompt_slash_command(raw_prompt, &prompt) {
        return decision(Some(prompt), "slashCommand", false, strategy);
    }
    if strategy == Some("agentAutoTitle") {
        /*
        Codex 0.150 persists its own first-turn title in session_index.jsonl.
        The metadata sync task adopts that canonical name, so Ghostex must not
        start a second model request or inject `/rename <generated title>`.
        */
        return decision(Some(prompt), "agentAutoTitle", false, strategy);
    }
    let current_title = read_session_text(session, "title");
    if !fork_first_prompt_rearmed
        && !is_terminal_auto_working_directory_title(session)
        && !is_generic_agent_session_title(agent_name.as_deref(), current_title.as_deref())
    {
        return decision(Some(prompt), "nonGenericCurrentTitle", false, strategy);
    }
    decision(Some(prompt), "eligible", true, strategy)
}

pub(crate) fn decision(
    normalized_prompt: Option<String>,
    reason: &str,
    should_run: bool,
    strategy: Option<&'static str>,
) -> FirstPromptAutoTitleDecision {
    FirstPromptAutoTitleDecision {
        normalized_prompt,
        reason: reason.to_string(),
        should_run,
        strategy,
    }
}

pub(crate) fn first_prompt_agent_name(session: &Value) -> Option<String> {
    read_session_text(session, "agentId").or_else(|| read_runtime_text(session, "agentName"))
}

pub(crate) fn first_prompt_auto_title_strategy(agent_name: Option<&str>) -> Option<&'static str> {
    match normalize_agent_name(agent_name).as_deref() {
        Some("claude") => Some("sendBareRenameCommand"),
        Some("codex") => Some("agentAutoTitle"),
        // Names its own sessions in its state database, in two stages, and the
        // metadata sync adopts both. Generating a second title here would race
        // that with a worse name.
        Some("hermes-agent") => Some("agentAutoTitle"),
        Some("pi") => Some("generateTitleAndName"),
        Some("omp") => Some("generateTitleAndName"),
        _ => None,
    }
}

pub(crate) fn normalize_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "openai codex" | "codex cli" => Some("codex".to_string()),
        "claude code" => Some("claude".to_string()),
        "cursor cli" | "cursor agent" | "cursor-agent" => Some("cursor".to_string()),
        "hermes" | "hermes agent" | "hermes-agent" => Some("hermes-agent".to_string()),
        "π" => Some("pi".to_string()),
        other => Some(other.to_string()),
    }
}

pub(crate) fn agent_session_title_command(agent_name: Option<&str>, title: &str) -> String {
    match normalize_agent_name(agent_name).as_deref() {
        Some("pi") | Some("omp") => format!("/name {title}"),
        Some("hermes-agent") => format!("/title {title}"),
        _ => format!("/rename {title}"),
    }
}

pub(crate) fn requested_agent_title_command_submission(
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) -> Option<(String, String, String)> {
    if endpoint_path != "/api/requestSessionRename"
        || params
            .get("submitAgentRenameCommand")
            .and_then(Value::as_bool)
            != Some(true)
        || result
            .get("shouldSendAgentRenameCommand")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return None;
    }
    let session = result.get("session")?;
    let project_id = read_session_text(session, "projectId")?;
    let session_id = read_session_text(session, "sessionId")?;
    let title = params.get("title")?.as_str()?.trim();
    if title.is_empty() {
        return None;
    }
    let command = agent_session_title_command(first_prompt_agent_name(session).as_deref(), title);
    Some((project_id, session_id, command))
}

pub(crate) fn is_generic_agent_session_title(
    agent_name: Option<&str>,
    title: Option<&str>,
) -> bool {
    let normalized_title = title
        .map(|value| {
            value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if normalized_title.is_empty() {
        return true;
    }
    let normalized_agent = normalize_agent_name(agent_name);
    let generic = [
        "terminal",
        "terminal session",
        "agent",
        "agent session",
        "claude",
        "claude code",
        "claude session",
        "codex",
        "codex cli",
        "codex session",
        "openai codex",
        "openai codex session",
        "pi",
        "π",
        "pi session",
    ];
    if generic.contains(&normalized_title.as_str()) {
        return true;
    }
    let Some(agent) = normalized_agent else {
        return false;
    };
    normalized_title == agent
        || normalized_title == format!("{agent} session")
        || normalized_title == format!("{agent} agent session")
}

pub(crate) fn normalize_first_prompt_title_prompt(prompt: Option<&str>) -> Option<String> {
    let normalized = prompt?.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let stripped = strip_first_prompt_title_prefixes(normalized);
    let cleaned = stripped
        .trim()
        .trim_end_matches(['.', '?', '!', ':', ';', ','])
        .trim();
    Some(
        if cleaned.is_empty() {
            normalized
        } else {
            cleaned
        }
        .to_string(),
    )
}

/*
CDXC:GxserverSessionTitle 2026-06-22-08:12:
First-prompt title eligibility must be decided before Rust claims a background job, using the same prompt-normalization and slash-command rules as TypeScript gxserver. Repeated polite prefixes are stripped only for title generation, while slash-command suppression scans the original prompt by line so short command prompts never enter the title job.
*/
pub(crate) fn strip_first_prompt_title_prefixes(value: &str) -> &str {
    let mut stripped = value;
    loop {
        let lower = stripped.to_lowercase();
        let prefix = [
            "please ",
            "kindly ",
            "hey ",
            "hi ",
            "hello ",
            "can you ",
            "could you ",
            "would you ",
            "will you ",
            "can we ",
            "could we ",
            "would we ",
            "help me ",
            "i need you to ",
            "i need to ",
            "i need ",
            "how do i ",
            "how does ",
            "is there any way to ",
            "is there way to ",
        ]
        .into_iter()
        .find(|prefix| lower.starts_with(prefix));
        let Some(prefix) = prefix else {
            return stripped;
        };
        stripped = &stripped[prefix.len()..];
    }
}

pub(crate) fn is_first_prompt_slash_command(
    raw_prompt: Option<&str>,
    normalized_prompt: &str,
) -> bool {
    if js_string_length(normalized_prompt) > 50 {
        return false;
    }
    let Some(raw_prompt) = raw_prompt else {
        return false;
    };
    raw_prompt
        .split('\n')
        .any(is_first_prompt_slash_command_line)
}

pub(crate) fn is_first_prompt_slash_command_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut consumed_bytes = first.len_utf8();
    for (index, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            consumed_bytes = index + ch.len_utf8();
            continue;
        }
        consumed_bytes = index;
        break;
    }
    let suffix = &rest[consumed_bytes..];
    suffix
        .chars()
        .next()
        .map(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ')' | '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '`'
                )
        })
        .unwrap_or(true)
}

pub(crate) fn is_first_prompt_meta_prompt(prompt: &str) -> bool {
    prompt.starts_with("# AGENTS")
        || prompt.contains("tool_use_id")
        || [
            "<command",
            "<environment_context",
            "<permissions instructions>",
            "<user_instructions>",
            "<INSTRUCTIONS>",
            "<collaboration_mode>",
            "<app-context>",
            "<turn_aborted>",
            "<ide_opened_file>",
            "<local-",
            "[Tool Result]",
            "Caveat:",
        ]
        .iter()
        .any(|prefix| prompt.starts_with(prefix))
}

pub(crate) fn session_history_title_source(session: &Value) -> Option<String> {
    let agent = normalize_agent_name(first_prompt_agent_name(session).as_deref())?;
    if !crate::agent_transcripts::agent_supports_session_history_title_source(Some(agent.as_str()))
    {
        return None;
    }
    let prompts = crate::agent_transcripts::recent_session_user_prompts(
        &agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    );
    build_session_history_title_source(&prompts)
}

pub(crate) fn build_session_history_title_source(prompts: &[String]) -> Option<String> {
    let mut recent: Vec<String> = prompts
        .iter()
        .rev()
        .take(GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_COUNT)
        .map(|prompt| {
            js_string_slice_prefix(
                prompt,
                GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_MAX_LENGTH,
            )
            .trim()
            .to_string()
        })
        .collect();
    recent.reverse();
    let joined = recent.join("\n\n");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(crate) async fn generate_first_prompt_session_title(
    state: &AppState,
    cwd: Option<&str>,
    prompt: &str,
    source_max_length: usize,
    session: &Value,
) -> Result<String, String> {
    let source_text = js_string_slice_prefix(prompt, source_max_length);
    let generation_prompt = build_first_prompt_title_generation_prompt(&source_text);
    let delimiter = format!(
        "ghostex_GXSERVER_SESSION_TITLE_{}",
        chrono::Utc::now().timestamp_millis()
    );
    let agent = normalize_title_generation_agent(
        read_runtime_text(session, "firstPromptTitleGenerationAgent").as_deref(),
    );
    let command = read_title_generation_command(session, &agent)?;
    let shell_command =
        build_title_generation_command(&agent, &command, &delimiter, &generation_prompt)?;
    let shell = command_shell();
    let mut child = Command::new(&shell.executable);
    child.args(shell.interactive_script_args(&shell_command));
    child.current_dir(cwd.unwrap_or_else(|| state.paths.home_dir.to_str().unwrap_or(".")));
    child.envs(internal_prompt_generation_environment(
        &state.paths.home_dir,
    ));
    child.stdout(std::process::Stdio::piped());
    child.stderr(std::process::Stdio::piped());
    let output = tokio::time::timeout(
        Duration::from_millis(GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS),
        child.output(),
    )
    .await
    .map_err(|_| "title generation timed out".to_string())?
    .map_err(|error| format!("title generation failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "title generation exited {:?}",
            output.status.code()
        ));
    }
    parse_generated_session_title_text(&String::from_utf8_lossy(&output.stdout))
}

pub(crate) fn normalize_title_generation_agent(value: Option<&str>) -> String {
    match value {
        Some("cursor" | "claude" | "grok" | "custom") => value.unwrap().to_string(),
        _ => "codex".to_string(),
    }
}

pub(crate) fn read_title_generation_command(
    session: &Value,
    agent: &str,
) -> Result<String, String> {
    if let Some(command) = read_runtime_text(session, "firstPromptTitleGenerationCommand") {
        return Ok(command);
    }
    match agent {
        "codex" => Ok("codex".to_string()),
        "cursor" => Ok("cursor-agent".to_string()),
        "claude" => Ok("claude".to_string()),
        "grok" => Ok("grok".to_string()),
        "custom" => Err("Custom title generation command is not configured.".to_string()),
        _ => Ok("codex".to_string()),
    }
}

pub(crate) fn build_title_generation_command(
    agent: &str,
    command: &str,
    delimiter: &str,
    prompt: &str,
) -> Result<String, String> {
    Ok(match agent {
        "codex" => {
            let command = enforce_required_agent_permission_flag(command, "codex");
            let command = format!(
                "{command} exec --ephemeral --skip-git-repo-check -m gpt-5.6-luna -c 'model_reasoning_effort=\"low\"'"
            );
            create_here_doc_command(&command, delimiter, prompt)
        }
        "cursor" => format!(
            "{command} --print --yolo --trust --model cursor-grok-4.5-low --output-format text {}",
            quote_shell_arg(prompt)
        ),
        "claude" => {
            let command = enforce_required_agent_permission_flag(command, "claude");
            create_here_doc_command(
                &format!("{command} -p --model haiku --effort low"),
                delimiter,
                prompt,
            )
        }
        "grok" => format!(
            "{command} --model grok-4.5 --reasoning-effort low --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 --single {}",
            quote_shell_arg(prompt)
        ),
        "custom" => create_here_doc_command(command, delimiter, prompt),
        other => return Err(format!("Unsupported title generation agent: {other}")),
    })
}

pub(crate) fn create_here_doc_command(command: &str, delimiter: &str, body: &str) -> String {
    format!("{command} <<'{delimiter}'\n{body}\n{delimiter}")
}

pub(crate) fn build_first_prompt_title_generation_prompt(source_text: &str) -> String {
    [
        "Write a concise session title that summarizes the user's text.",
        "Return plain text only.",
        "Rules:",
        "- keep it specific and scannable",
        "- prefer 2 to 4 words when possible",
        &format!(
            "- must be fewer than {} characters",
            GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH + 1
        ),
        "- do not abbreviate with ellipses",
        "- do not use quotes, markdown, or commentary",
        "- do not end with punctuation",
        "- focus on the task, bug, feature, or topic",
        "",
        "User text:",
        source_text,
        "",
        "Output handling:",
        "- Produce only the final session title.",
        "- Do not wrap the result in backticks.",
        "- Print only the final result to stdout.",
    ]
    .join("\n")
}

pub(crate) fn parse_generated_session_title_text(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let normalized = if trimmed.starts_with("```") && trimmed.ends_with("```") {
        trimmed
            .trim_start_matches('`')
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches('`')
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    let Some(line) = normalized.lines().find(|line| !line.trim().is_empty()) else {
        return Err("Title generation returned an empty session title.".to_string());
    };
    let sanitized = line
        .trim()
        .trim_matches(['"', '\'', '`'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '…'])
        .trim()
        .to_string();
    if sanitized.is_empty() {
        return Err("Title generation returned an empty session title.".to_string());
    }
    Ok(clamp_generated_session_title_length(&sanitized))
}

pub(crate) fn clamp_generated_session_title_length(value: &str) -> String {
    if js_string_length(value) <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH {
        return value.to_string();
    }
    let mut candidate = String::new();
    for word in value.split_whitespace() {
        let next = if candidate.is_empty() {
            word.to_string()
        } else {
            format!("{candidate} {word}")
        };
        if js_string_length(&next) > GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH {
            break;
        }
        candidate = next;
    }
    if candidate.is_empty() {
        js_string_slice_prefix(value, GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH)
            .trim()
            .to_string()
    } else {
        candidate
    }
}

/*
CDXC:GxserverSessionTitle 2026-06-22-07:21:
TypeScript title caps use JavaScript string length and slice semantics, so Rust must count UTF-16 code units rather than Unicode scalar values for first-prompt source text and generated session titles. Rust strings cannot store lone surrogate halves; when a JS slice would expose one, use the replacement character that Node writes at the UTF-8 boundary.
*/
pub(crate) fn js_string_length(text: &str) -> usize {
    text.encode_utf16().count()
}

pub(crate) fn js_string_slice_prefix(text: &str, max_code_units: usize) -> String {
    let mut output = String::new();
    let mut code_units = 0usize;
    for ch in text.chars() {
        let width = ch.len_utf16();
        if code_units + width > max_code_units {
            if code_units < max_code_units {
                output.push(char::REPLACEMENT_CHARACTER);
            }
            break;
        }
        output.push(ch);
        code_units += width;
        if code_units == max_code_units {
            break;
        }
    }
    output
}

pub(crate) fn internal_prompt_generation_environment(
    home_dir: &std::path::Path,
) -> Vec<(String, String)> {
    /*
    CDXC:GxserverPromptGeneration 2026-06-24-16:11:
    Background title and commit-message generation must not inherit active
    Ghostex session identity. Clear session-binding variables and mark the
    process as internal so installed agent hooks do not attach generated prompt
    runs to user-restorable terminal sessions.
    */
    let mut environment = std::env::vars().collect::<std::collections::HashMap<_, _>>();
    for key in [
        "ANSI_COLORS_DISABLED",
        "NO_COLOR",
        "NODE_DISABLE_COLORS",
        "GHOSTEX_GLOBAL_SESSION_REF",
        "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
        "GHOSTEX_GXSERVER_BASE_URL",
        "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
        "GHOSTEX_SESSION_ID",
        "GHOSTEX_SESSION_STATE_FILE",
        "GHOSTEX_WORKSPACE_ID",
        "GHOSTEX_WORKSPACE_ROOT",
        "VSMUX_SESSION_ID",
        "VSMUX_SESSION_STATE_FILE",
        "VSMUX_WORKSPACE_ID",
        "VSMUX_WORKSPACE_ROOT",
        "ghostex_SESSION_STATE_FILE",
        "ghostex_WORKSPACE_ID",
        "ghostex_WORKSPACE_ROOT",
    ] {
        environment.remove(key);
    }
    environment.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
    environment.insert(
        "GHOSTEX_INTERNAL_PROMPT_GENERATION".to_string(),
        "1".to_string(),
    );
    environment.insert(
        "GHOSTEX_INTERNAL_TITLE_GENERATION".to_string(),
        "1".to_string(),
    );
    environment.into_iter().collect()
}

pub(crate) fn quote_shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
