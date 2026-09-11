use super::endpoint;
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    session_chat_send::{SessionChatSendError, SessionChatSendStep},
    storage::open_gxserver_database,
};
use serde_json::{json, Map, Value};
use std::{sync::Arc, time::Instant};

/// The slash command both Claude Code and Codex accept to leave the terminal.
const EXIT_COMMAND: &str = "/exit";
/// How long the switch waits for the CLI to hand the terminal back to its shell after `/exit`.
const EXIT_TO_SHELL_TIMEOUT_MS: u64 = 15_000;
const SOURCE: &str = "account-switch-restart";
const ACCOUNT_SETTINGS: &[&str] = &[
    "accountId",
    "accountName",
    "accountColor",
    "accountProvider",
    "accountSlot",
    "accountBaseCommand",
    "accountCommand",
    "agentCommand",
    "accountSuppressedUsageNotice",
    "accountSuppressedUsageNoticeAt",
];

/// CDXC:AgentProviders 2026-09-11 WHY:
/// An open Claude Usage screen prevented /exit, but the selected account was already saved, so the UI showed account 3's limits while account 1 was still running.
/// Keep the current binding until the restart sequence succeeds; a failed or cancelled switch must not relabel the running CLI.
pub(crate) fn retain_current_account(runtime: &mut Map<String, Value>, session: &Value) {
    apply_account_settings(runtime, &session["runtimeSettings"]);
}

fn apply_account_settings(runtime: &mut Map<String, Value>, settings: &Value) {
    for &key in ACCOUNT_SETTINGS {
        if let Some(value) = settings.get(key) {
            runtime.insert(key.to_string(), value.clone());
        } else {
            runtime.remove(key);
        }
    }
}

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: an account switch on a running Claude or Codex session must not sleep and wake the daemon: "we don't need to close the terminal, we just do /exit on the claude/codex session in the terminal and then write the resume command".
/// The daemon and every attached terminal stay alive. The CLI is told to exit from its own composer, its process is confirmed gone from the daemon's process tree, and the resume command a wake would have used is typed into that same shell.
/// Nothing closes on the desktop or the web, so the chat page and the terminal tab survive the switch, and the cycle that took a minute on a session with running subagents is gone.
/// The sleep-and-wake cycle remains for a running session whose daemon is already gone: there is no CLI to exit there.
/// SEE-ALSO: server/src/accounts/endpoint.rs (select), server/src/accounts/continuation.rs (the dot after the restart), server/src/agents/drafts.rs (the draft variant of the same in-place switch), server/src/session_chat_send.rs (WaitForAgentExit).
pub(crate) struct RestartPlan {
    agent: String,
    resume_command: String,
    /// False when the daemon is at its shell already (the CLI exited earlier, or a previous restart stopped after `/exit`): the resume command is typed without an exit.
    cli_running: bool,
    dismiss_claude_settings: bool,
    account_settings: Value,
}

/// Whether the session's zmx daemon is alive, so the CLI inside it can be exited and resumed in place.
pub(crate) fn provider_is_live(repository: &DomainRepository<'_>, session: &Value) -> bool {
    let lifecycle = crate::zmx::LifecycleParams {
        project_id: session["projectId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        session_id: session["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    };
    crate::zmx::probe_and_cache_session_provider(repository, &lifecycle)
        .map(|(probe, _, _, _)| probe.lifecycle_state == "exists")
        .unwrap_or(false)
}

/// Resolves everything the restart needs from the session as it will be saved, so a session that cannot be resumed is refused before its row changes.
pub(crate) fn plan(
    state: &AppState,
    repository: &DomainRepository<'_>,
    project: &Value,
    session: &Value,
) -> Result<RestartPlan, DomainStateError> {
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .filter(|agent| matches!(agent.as_str(), "claude" | "codex"))
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Only Claude and Codex sessions can switch accounts while running.",
            )
        })?;
    let settings = crate::agents::read_agent_settings(repository.db)?;
    let resume_command =
        crate::zmx::get_provider_restart_startup_text_for_session(project, session, &settings)
            .map(|text| text.trim_end_matches(['\r', '\n']).trim().to_string())
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request(
                    "No saved conversation is available to resume on the selected account.",
                )
            })?;
    let zmx_name = crate::zmx::provider_zmx_session_name(session)?;
    let cli_running = crate::zmx::read_zmx_session_process_identities(
        std::slice::from_ref(&zmx_name),
        &state.paths.home_dir,
    )
    .map(|identities| identities.contains_key(&zmx_name))
    .unwrap_or(true);
    let dismiss_claude_settings = cli_running
        && crate::session_chat_options::SessionChatOptionDetector::new(state)
            .detect_blocking(
                session["projectId"].as_str().unwrap_or_default(),
                session["sessionId"].as_str().unwrap_or_default(),
                Some(&agent),
                true,
            )
            .composer
            .should_dismiss_with_escape();
    let mut account_settings = Map::new();
    apply_account_settings(&mut account_settings, &session["runtimeSettings"]);
    Ok(RestartPlan {
        agent,
        resume_command,
        cli_running,
        dismiss_claude_settings,
        account_settings: Value::Object(account_settings),
    })
}

/// Exits the CLI and resumes it on the saved account. The continuation dot starts only once the resume command has been typed, so it can never reach the CLI that is being replaced.
pub(crate) fn start(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    plan: RestartPlan,
) -> Result<(), DomainStateError> {
    let project_id = session["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let session_id = session["sessionId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let attempt = uuid::Uuid::new_v4().to_string();
    runtime.insert("accountSwitchAttempt".into(), json!(attempt));
    runtime.insert("accountRecoverySuppressed".into(), json!(false));
    runtime.insert(
        "accountRecovery".into(),
        json!({
            "status": "retrying", "trigger": "accountSwitch", "attempt": 0,
            "reason": "Restarting the agent on the selected account.",
            "updatedAt": chrono::Utc::now().to_rfc3339()
        }),
    );
    endpoint::update_session(repository, session, runtime)?;
    // Pending sends belong to the CLI that is about to exit; the new generation also makes this sequence the only writer until it finishes.
    crate::session_chat_send::cancel_session_chat_sends(&project_id, &session_id);
    let mut steps = if plan.cli_running {
        let mut steps = crate::session_chat_send::build_session_chat_message_steps(
            Some(plan.agent.as_str()),
            EXIT_COMMAND,
            &[],
            plan.dismiss_claude_settings,
        );
        steps.push(SessionChatSendStep::WaitForAgentExit {
            home_dir: state.paths.home_dir.clone(),
            timeout_ms: EXIT_TO_SHELL_TIMEOUT_MS,
        });
        steps
    } else {
        Vec::new()
    };
    steps.push(SessionChatSendStep::Write(plan.resume_command));
    steps.push(SessionChatSendStep::SleepMs(
        crate::session_chat_send::SESSION_CHAT_SUBMIT_DELAY_MS,
    ));
    steps.push(SessionChatSendStep::Write(
        crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
    ));
    let completion = crate::session_chat_send::enqueue_session_write_sequence_with_completion(
        session,
        &project_id,
        &session_id,
        SOURCE,
        steps,
    );
    let completion = match completion {
        Ok(completion) => completion,
        Err(error) => {
            fail(state, repository, session, &error.message);
            return Err(error);
        }
    };
    let state = Arc::new(state.clone());
    let started = Instant::now();
    tokio::spawn(async move {
        let result = completion.await.unwrap_or_else(|_| {
            Err(SessionChatSendError {
                failure: crate::session_chat_send::SessionChatSendFailure::NotAttempted,
                message: "The session write worker stopped before the agent was restarted."
                    .to_string(),
            })
        });
        finish(
            state,
            project_id,
            session_id,
            attempt,
            plan.account_settings,
            result,
            started,
        )
        .await;
    });
    Ok(())
}

async fn finish(
    state: Arc<AppState>,
    project_id: String,
    session_id: String,
    attempt: String,
    account_settings: Value,
    result: Result<(), SessionChatSendError>,
    started: Instant,
) {
    let Ok(_gate) = state.accounts.mutations.lock() else {
        return;
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(row)) = repo.get_session(&project_id, &session_id) else {
        return;
    };
    if row
        .pointer("/runtimeSettings/accountSwitchAttempt")
        .and_then(Value::as_str)
        != Some(attempt.as_str())
    {
        return;
    }
    let duration_ms = started.elapsed().as_millis();
    match result {
        Ok(()) => {
            let mut runtime = row["runtimeSettings"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let continue_turn = runtime
                .get("accountRecoverySuppressed")
                .and_then(Value::as_bool)
                != Some(true);
            runtime.remove("accountSwitchAttempt");
            apply_account_settings(&mut runtime, &account_settings);
            let Ok(row) = endpoint::update_session(&repo, &row, runtime) else {
                return;
            };
            if let Some(since) = account_settings["accountSuppressedUsageNoticeAt"]
                .as_str()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            {
                crate::session_chat_notice::suppress_account_usage_notice(
                    &project_id,
                    &session_id,
                    since.with_timezone(&chrono::Utc),
                );
            }
            if let (Some(id), Ok(mut registry)) = (
                account_settings["accountId"].as_str(),
                super::store::read(&db),
            ) {
                if let Some(provider) = registry
                    .accounts
                    .iter()
                    .find(|account| account.id == id)
                    .map(|account| account.provider)
                {
                    registry.last_used_accounts.insert(provider, id.to_string());
                    let _ = super::store::write(&db, &registry);
                }
            }
            log(
                &state,
                LogLevel::Info,
                "accountSwitchRestarted",
                None,
                duration_ms,
                &project_id,
                &session_id,
            );
            if continue_turn {
                let _ = super::continuation::start(&state, &repo, &row);
            }
            let _ = endpoint::publish(&state, &repo, &row);
        }
        Err(error) => {
            log(
                &state,
                LogLevel::Warn,
                "accountSwitchRestartFailed",
                Some(error.message.clone()),
                duration_ms,
                &project_id,
                &session_id,
            );
            fail(&state, &repo, &row, &error.message);
        }
    }
}

fn fail(state: &AppState, repo: &DomainRepository<'_>, row: &Value, error: &str) {
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.remove("accountSwitchAttempt");
    runtime.insert("accountRecoverySuppressed".into(), json!(true));
    runtime.insert(
        "accountRecovery".into(),
        json!({
            "status": "needsAttention", "trigger": "accountSwitch", "attempt": 0,
            "reason": format!("{error} The account was not changed. Select it again to retry."),
            "updatedAt": chrono::Utc::now().to_rfc3339()
        }),
    );
    if endpoint::update_session(repo, row, runtime).is_ok() {
        let _ = endpoint::publish(state, repo, row);
    }
}

fn log(
    state: &AppState,
    level: LogLevel,
    event: &str,
    error: Option<String>,
    duration_ms: u128,
    project_id: &str,
    session_id: &str,
) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: Some(duration_ms),
        error,
        details: Some(json!({ "projectId": project_id, "sessionId": session_id })),
    });
}
