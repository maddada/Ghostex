use super::*;

#[derive(Clone)]
pub(crate) struct ForkInitialRenameTarget {
    agent_name: String,
    project_id: String,
    session_id: String,
    title: String,
}

type ForkRenameKey = (String, String, String);
static FORK_RENAMES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashSet<ForkRenameKey>>,
> = std::sync::OnceLock::new();

struct ForkRenameGuard(ForkRenameKey);

impl Drop for ForkRenameGuard {
    fn drop(&mut self) {
        if let Some(mut jobs) = FORK_RENAMES.get().and_then(|jobs| jobs.lock().ok()) {
            jobs.remove(&self.0);
        }
    }
}

pub(crate) fn fork_initial_rename_is_current(session: &Value, expected_title: &str) -> bool {
    read_session_text(session, "lifecycleState").as_deref() == Some("running")
        && matches!(
            read_runtime_text(session, "gxserverForkInitialRenameStatus").as_deref(),
            Some("pending" | "failed")
        )
        && crate::agents::provisional_fork_title(session).as_deref() == Some(expected_title)
        && read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus").is_none()
        && read_runtime_text(session, "pendingAgentTitleRequestTitle")
            .is_none_or(|title| title == expected_title)
        && read_runtime_text(session, "pendingAgentTitleRequestTitleSource")
            .is_none_or(|source| source == "placeholder")
}

/// CDXC:SessionFork 2026-09-24 WHY:
/// Codex startup questions can outlast the initial rename attempt. Its inherited parent name is deliberately not adopted, so a permanently failed attempt also hides the completed fork from closed history. The server-owned metadata sweep retries the owned provisional rename before its index-revision gate because clearing a startup question need not change that index or trigger client polling; confirmation still comes from the agent's metadata.
pub(crate) fn fork_initial_rename_retry_target(session: &Value) -> Option<ForkInitialRenameTarget> {
    let agent_name = crate::agents::session_agent_family_id(&Value::Null, session)?;
    if agent_name != "codex" {
        return None;
    }
    let target = ForkInitialRenameTarget {
        agent_name,
        project_id: read_session_text(session, "projectId")?,
        session_id: read_session_text(session, "sessionId")?,
        title: crate::agents::provisional_fork_title(session)?,
    };
    fork_initial_rename_is_current(session, &target.title).then_some(target)
}

/// CDXC:SessionFork 2026-09-15 DECISION:
/// User: Fork must persist `Fork: <original name>` through the agent's own rename command.
/// This supersedes the Codex exception that left its provider thread unnamed for first-turn auto-titling.
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
    // A fork created already named (Empryo's copy, `fork_empryo.rs`) has nothing to type.
    if read_runtime_text(session, "gxserverForkInitialRenameStatus").as_deref() == Some("applied") {
        return None;
    }
    Some(ForkInitialRenameTarget {
        agent_name: agent_name.to_string(),
        project_id: read_session_text(session, "projectId")?,
        session_id: read_session_text(session, "sessionId")?,
        title: read_session_text(session, "title")?,
    })
}

/// CDXC:SessionFork 2026-09-23 WHY:
/// The generic 150ms text/Enter path left Windows Codex fork renames in the composer and marked them applied before delivery.
/// Use the chat sender's composer and paste verification, and await its completion inside this background task before reporting the rename applied.
/// Codex can finish this rename before exposing its rollout through a hook or open handle. Record the same pending rename as the manual flow so its exact post-request session-index confirmation also supplies the fork identity and inherited chat history before the first prompt.
pub(crate) fn schedule_fork_initial_rename(state: AppState, target: ForkInitialRenameTarget) {
    let key = (
        state.metadata.server_id.clone(),
        target.project_id.clone(),
        target.session_id.clone(),
    );
    let Ok(mut jobs) = FORK_RENAMES.get_or_init(Default::default).lock() else {
        return;
    };
    if !jobs.insert(key.clone()) {
        return;
    }
    drop(jobs);
    let guard = ForkRenameGuard(key);
    /*
    CDXC:SessionFork 2026-07-11:
    Fork provider startup already owns the resumed CLI process. Wait for its
    composer, then submit the provisional `Fork: <old title>` through zmx's
    separate text/Enter path. Pi uses `/name`, Hermes Agent uses `/title`, and
    Claude and Codex use `/rename` to persist the name in the agent's metadata.
    If the user has already sent the fork's first prompt, its generated-title
    job wins and this provisional rename is skipped.

    CDXC:SessionChat 2026-08-26: this used to be a blind four-second
    sleep. A rename typed before the composer exists is not merely lost — the
    slash command lands as literal text in whatever screen IS up.
    */
    tokio::spawn(async move {
        let _guard = guard;
        let readiness = crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
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
        let mut session = {
            let Ok(db) = open_gxserver_database(&state.paths) else {
                return;
            };
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            let Ok(Some(session)) = repository.get_session(&target.project_id, &target.session_id)
            else {
                return;
            };
            session
        };
        if !fork_initial_rename_is_current(&session, &target.title) {
            return;
        }
        let command = agent_session_title_command(Some(&target.agent_name), &target.title);
        let mut resume_staged_command = false;
        if target.agent_name == "codex" {
            let Some(zmx_name) = read_session_text(&session, "zmxName") else {
                return;
            };
            let input = crate::session_chat_send::capture_session_terminal_text_vt(&zmx_name)
                .await
                .and_then(|screen| {
                    crate::session_chat_composer::session_chat_composer_input("codex", &screen)
                });
            let Some(input) = input else {
                return;
            };
            resume_staged_command = !input.is_empty()
                && input.text == command
                && read_runtime_text(&session, "pendingAgentTitleRequestTitle").as_deref()
                    == Some(target.title.as_str())
                && read_runtime_text(&session, "pendingAgentTitleRequestStatus").as_deref()
                    == Some("pending")
                && read_runtime_text(&session, "pendingAgentTitleRequestRequestedAt").is_some();
            if !input.is_empty() && !resume_staged_command {
                return;
            }
        }
        let completion = if matches!(
            readiness,
            crate::session_chat_composer::SessionChatComposerWait::Ready
                | crate::session_chat_composer::SessionChatComposerWait::Unknown
        ) {
            if target.agent_name == "codex" && !resume_staged_command {
                let Ok(db) = open_gxserver_database(&state.paths) else {
                    return;
                };
                let Ok(transaction) = rusqlite::Transaction::new_unchecked(
                    &db,
                    rusqlite::TransactionBehavior::Immediate,
                ) else {
                    return;
                };
                let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
                let Ok(Some(current)) =
                    repository.get_session(&target.project_id, &target.session_id)
                else {
                    return;
                };
                if !fork_initial_rename_is_current(&current, &target.title)
                    || read_runtime_text(&current, "firstUserMessage")
                        != read_runtime_text(&session, "firstUserMessage")
                {
                    return;
                }
                let Ok(result) = crate::agents::request_session_rename(
                    &repository,
                    &crate::agents::LifecycleParams {
                        project_id: target.project_id.clone(),
                        session_id: target.session_id.clone(),
                    },
                    json!({"title": target.title, "titleSource": "placeholder"})
                        .as_object()
                        .expect("rename parameters"),
                    &state.paths.home_dir,
                ) else {
                    return;
                };
                if transaction.commit().is_err() {
                    return;
                }
                let Some(current) = result.get("session").cloned() else {
                    return;
                };
                session = current;
                if !fork_initial_rename_is_current(&session, &target.title) {
                    schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
                    return;
                }
            }
            // CDXC:SessionChat 2026-08-23: see the auto-title dispatch.
            crate::session_chat_app_command::record_session_chat_app_command(
                &target.project_id,
                &target.session_id,
                &command,
            );
            let mut steps = crate::session_chat_send::build_session_chat_message_steps(
                Some(&target.agent_name),
                &command,
                &[],
                false,
            );
            if target.agent_name == "codex" {
                let guard = |expected_composer| {
                    crate::session_chat_send::SessionChatSendStep::GuardForkRename {
                        agent: target.agent_name.clone(),
                        title: target.title.clone(),
                        state_db_file: state.paths.state_db_file.clone(),
                        server_id: state.metadata.server_id.clone(),
                        first_user_message: read_runtime_text(&session, "firstUserMessage"),
                        rename_requested_at: read_runtime_text(
                            &session,
                            "pendingAgentTitleRequestRequestedAt",
                        ),
                        expected_composer,
                    }
                };
                if resume_staged_command {
                    let paste = crate::session_chat_send::build_session_chat_paste_bytes(&command);
                    steps.retain(|step| step.written_payload() != Some(paste.as_str()));
                }
                for step in &mut steps {
                    if matches!(
                        step,
                        crate::session_chat_send::SessionChatSendStep::ClearComposer { .. }
                    ) {
                        *step = guard(resume_staged_command.then(|| command.clone()));
                    }
                }
                steps.insert(steps.len() - 1, guard(Some(command.clone())));
            }
            crate::session_chat_send::enqueue_session_write_sequence_with_completion(
                &session,
                &target.project_id,
                &target.session_id,
                "fork-title-command",
                steps,
                None,
            )
            .ok()
        } else {
            None
        };
        let status = match completion {
            Some(completion) => {
                if matches!(completion.await, Ok(Ok(()))) {
                    "applied"
                } else {
                    "failed"
                }
            }
            _ => "failed",
        };
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(Some(latest_session)) =
            repository.get_session(&target.project_id, &target.session_id)
        else {
            return;
        };
        if !fork_initial_rename_is_current(&latest_session, &target.title) {
            return;
        }
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
        if status == "applied" && target.agent_name == "codex" {
            schedule_agent_title_metadata_check(
                state.clone(),
                target.project_id.clone(),
                target.session_id.clone(),
            );
        }
        schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
    });
}
