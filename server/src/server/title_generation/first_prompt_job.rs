use super::*;

pub(crate) fn schedule_first_prompt_auto_title_job(
    state: AppState,
    project_id: String,
    session_id: String,
    attempt_id: String,
) {
    /*
    CDXC:SessionTitles 2026-06-21-19:26:
    Rust server must finish the same first-prompt auto-title flow as TypeScript gxserver after hooks claim a job: decide eligibility centrally, generate or stage the provider rename command, and persist applied/skipped/failed status.

    CDXC:SessionTitles 2026-07-02-15:10:
    gxserver submits the staged rename command itself with a separate zmx `\r` write instead of asking clients to send a native Enter on the running→applied presentation transition. Client-side submission only worked for currently visible native panes: `sessions[sessionId]` has no Ghostty surface for background/automation-started sessions, so their staged `/rename` sat unsubmitted in the agent composer forever. A separate PTY-level CR after a settle delay is a real Enter keypress to agent prompt editors (a CR appended to the same text payload is treated as a pasted newline), works for invisible panes, remote daemons, and GPUI, and removes the fragile transition-observation race entirely.

    CDXC:SessionTitles 2026-08-03:
    A cancelled prompt can be explicitly submitted again with identical text.
    Bind every spawned job to its claim attempt so the cancelled subprocess
    cannot apply or fail the replacement job after it eventually exits.
    */
    tokio::spawn(async move {
        let _ = run_first_prompt_auto_title_job(
            state.clone(),
            project_id.clone(),
            session_id.clone(),
            attempt_id.clone(),
        )
        .await;
        // CDXC:SessionTitles 2026-09-10 WHY:
        // A prompt replaced during the await exits successfully without applying a title. Every finished worker must retire a claim it still owns; completed, cancelled and superseded attempts are already excluded by the attempt check.
        mark_first_prompt_auto_title_failed_if_current_attempt(
            &state,
            &project_id,
            &session_id,
            &attempt_id,
        );
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
    let (project_path, session, decision) = {
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

    let title = generate_first_prompt_session_title(
        &state,
        Some(&project_path),
        decision.normalized_prompt.as_deref().ok_or(())?,
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
        &session,
    )
    .await
    .map_err(|_| ())?;
    let command_text =
        agent_session_title_command(first_prompt_agent_name(&session).as_deref(), &title);
    // A Pi with Ghostex's chat channel open is named through its own API instead of a typed
    // `/name` (CDXC:PiChatChannel 2026-11-05 in pi_chat_channel.rs).
    let named_by_channel = normalize_agent_name(first_prompt_agent_name(&session).as_deref())
        .as_deref()
        == Some("pi")
        && crate::pi_chat_channel::set_session_name(&project_id, &session_id, &title).await;
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
        /*
        CDXC:SessionChat 2026-08-24:
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

        CDXC:SessionChat 2026-08-26:
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
        if !named_by_channel {
            crate::session_chat_send::enqueue_session_write_sequence(
                &latest_session,
                &project_id,
                &session_id,
                "auto-title",
                auto_title_steps,
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

    /*
    CDXC:SessionTitles 2026-07-02-15:10:
    The staged command is submitted by a separate zmx `\r` write after a settle
    delay so agent prompt editors read it as a real Enter keypress rather than
    as part of a pasted payload. Both writes and the delay between them are now
    steps of the queued job above (CDXC:SessionChat); this wait
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
    schedule_delta_for_ids(&state, &project_id, &session_id);
    Ok(())
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
