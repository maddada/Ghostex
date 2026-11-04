use super::{continuation::SwitchSource, endpoint};
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    session_chat_send::{SessionChatSendError, SessionChatSendStep},
    storage::open_gxserver_database,
};
use serde_json::{json, Map, Value};
use std::{sync::Arc, time::Instant};

/// How long the switch drives the CLI back to its shell.
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

/// CDXC:AgentProviders 2026-09-22 DECISION:
/// User: exit regardless of what the CLI shows, repeating Escape and Ctrl+C until back at the shell, then resume the conversation on the other account. This supersedes the 2026-09-11 /exit-only rule and its composer requirement.
/// User also approved stopping the exact Claude background conversation with `claude stop <id>`, preserving history while stopping its remaining jobs; exiting its attached client alone only detaches.
/// The daemon and every attached terminal stay alive. The old agent is stopped, its terminal client is confirmed gone, and one exact account-specific resume command captured before stopping is typed into that same shell.
/// Nothing closes on the desktop or the web, so the chat page and the terminal tab survive the switch, and the cycle that took a minute on a session with running subagents is gone.
/// The sleep-and-wake cycle remains for a running session whose daemon is already gone: there is no CLI to exit there.
/// SEE-ALSO: server/src/accounts/endpoint.rs (select), server/src/accounts/continuation.rs (the dot after automatic switches), server/src/agents/drafts.rs (the draft variant of the same in-place switch), server/src/accounts/exit.rs (interrupt_until_exited), server/src/session_chat_send/launch_command.rs (RunShellCommand).
pub(crate) struct RestartPlan {
    agent: String,
    resume_command: String,
    account_settings: Value,
    expected_identity: String,
    previous_process_id: Option<i64>,
    background: Option<super::claude_background::BackgroundSession>,
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
        crate::agents::account_switch_resume_command(project, session, &settings)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request(
                    "This account switch needs an exact saved conversation and a single-line agent command.",
                )
            })?;
    let resume_command = typed_resume_command(
        state,
        session,
        super::default_login::route_around_signed_out_default(&resume_command),
    )?;
    let zmx_name = crate::zmx::provider_zmx_session_name(session)?;
    let identities = crate::zmx::read_zmx_session_process_identities(
        std::slice::from_ref(&zmx_name),
        &state.paths.home_dir,
    )
    .map_err(|_| {
        DomainStateError::bad_request(
            "Could not inspect the current agent before switching accounts.",
        )
    })?;
    let current_process = identities.get(&zmx_name);
    let previous_process_id = current_process.and_then(|identity| identity.process_id);
    let background = if agent == "claude" {
        previous_process_id.and_then(|pid| {
            super::claude_background::resolve(
                &state.paths.home_dir,
                pid,
                session["runtimeSettings"]["agentSessionId"].as_str()?,
            )
        })
    } else {
        None
    };
    let registry = super::store::read(repository.db)?;
    let expected_identity = registry
        .accounts
        .iter()
        .find(|account| {
            Some(account.id.as_str()) == session["runtimeSettings"]["accountId"].as_str()
        })
        .map(|account| account.identity.clone())
        .ok_or_else(|| DomainStateError::bad_request("Choose a saved account before switching."))?;
    let mut account_settings = Map::new();
    apply_account_settings(&mut account_settings, &session["runtimeSettings"]);
    Ok(RestartPlan {
        agent,
        resume_command,
        account_settings: Value::Object(account_settings),
        expected_identity,
        previous_process_id,
        background,
    })
}

/// The longest resume command typed into the shell as it is.
const TYPED_COMMAND_LIMIT: usize = 768;

/// CDXC:AgentProviders 2026-10-14 WHY:
/// A Codex resume command carries Marked handovers inline (`-c developer_instructions=…`, Codex has no file form for it) and ran past 1 KB. Typed into the shell right after Codex exited, it arrived while the terminal was still in line mode, which keeps at most 1,024 bytes of a line on macOS: the shell got the command cut inside its quote, the agent never started and the switch failed. A command longer than `TYPED_COMMAND_LIMIT` is written to a script file in gxserver's state and the shell is given only `source '<file>'` (`. '<file>'` in PowerShell), which every shell Ghostex runs reads in full.
fn typed_resume_command(
    state: &AppState,
    session: &Value,
    command: String,
) -> Result<String, DomainStateError> {
    if command.len() <= TYPED_COMMAND_LIMIT {
        return Ok(command);
    }
    let dir = state.paths.root_dir.join("account-switch");
    std::fs::create_dir_all(&dir).map_err(super::store::error)?;
    let file = dir.join(format!(
        "{}-{}.{}",
        session["projectId"].as_str().unwrap_or_default(),
        session["sessionId"].as_str().unwrap_or_default(),
        if cfg!(windows) { "ps1" } else { "sh" }
    ));
    std::fs::write(&file, format!("{command}\n")).map_err(super::store::error)?;
    Ok(format!(
        "{} {}",
        if cfg!(windows) { "." } else { "source" },
        crate::agents::quote_shell_arg(&file.to_string_lossy())
    ))
}

/// Exits the CLI and resumes it on the saved account. Automatic switches continue only after the new login and its ready composer have been verified.
pub(crate) fn start(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    plan: RestartPlan,
    source: SwitchSource,
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
    let attempt = session
        .pointer("/runtimeSettings/accountSwitch/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
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
    let cwd = session["cwd"].as_str().map(str::to_string).or_else(|| {
        repository
            .get_project(&project_id)
            .ok()
            .flatten()
            .and_then(|project| project["path"].as_str().map(str::to_string))
    });
    if let Some(cwd) = cwd {
        crate::agent_folder_trust::trust_session_folder_before_launch(
            repository,
            &project_id,
            &session_id,
            session,
            &cwd,
        );
    }
    // Pending sends belong to the CLI that is about to exit; the new generation also makes this sequence the only writer until it finishes.
    crate::session_chat_send::cancel_session_chat_sends(&project_id, &session_id);
    let mut steps = if let Some(background) = plan.background {
        vec![SessionChatSendStep::StopClaudeBackground {
            background,
            home_dir: state.paths.home_dir.clone(),
        }]
    } else {
        Vec::new()
    };
    steps.push(SessionChatSendStep::InterruptAgentForAccountSwitch {
        home_dir: state.paths.home_dir.clone(),
        timeout_ms: EXIT_TO_SHELL_TIMEOUT_MS,
    });
    steps.push(SessionChatSendStep::RunShellCommand {
        command: format!(" {}", plan.resume_command),
        home_dir: state.paths.home_dir.clone(),
        timeout_ms: 10_000,
    });
    let started_login = StartedLogin {
        zmx_name: crate::zmx::provider_zmx_session_name(session)?,
        agent: plan.agent.clone(),
        expected_identity: plan.expected_identity.clone(),
        previous_process_id: plan.previous_process_id,
    };
    steps.push(SessionChatSendStep::WaitForAccountReady {
        agent: plan.agent,
        expected_identity: plan.expected_identity,
        home_dir: state.paths.home_dir.clone(),
        previous_process_id: plan.previous_process_id,
        timeout_ms: 30_000,
        accept_remembered_trust: crate::session_chat_trust_memory::session_folders_remembered(
            repository,
            &project_id,
            &session_id,
        ),
    });
    let progress_state = state.clone();
    let progress_project = project_id.clone();
    let progress_session = session_id.clone();
    let progress_attempt = attempt.clone();
    let completion = crate::session_chat_send::enqueue_session_write_sequence_with_completion(
        session,
        &project_id,
        &session_id,
        SOURCE,
        steps,
        Some(Arc::new(move |step| {
            if matches!(step, SessionChatSendStep::WaitForAccountReady { .. }) {
                super::switch_progress::resuming(
                    &progress_state,
                    &progress_project,
                    &progress_session,
                    &progress_attempt,
                );
            }
        })),
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
            started_login,
            source,
            result,
            started,
        )
        .await;
    });
    Ok(())
}

/// The agent a restart started, so a switch that fails after it started can tell which login runs.
struct StartedLogin {
    zmx_name: String,
    agent: String,
    expected_identity: String,
    previous_process_id: Option<i64>,
}

async fn finish(
    state: Arc<AppState>,
    project_id: String,
    session_id: String,
    attempt: String,
    account_settings: Value,
    started_login: StartedLogin,
    source: SwitchSource,
    result: Result<(), SessionChatSendError>,
    started: Instant,
) {
    let runs_on_selected_login = result.is_err()
        && super::restart_verification::runs_on_selected_login(
            &started_login.zmx_name,
            &started_login.agent,
            &started_login.expected_identity,
            &state.paths.home_dir,
            started_login.previous_process_id,
        )
        .await;
    let _gate = state.accounts.mutation_gate();
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
            runtime.remove("accountRecovery");
            apply_account_settings(&mut runtime, &account_settings);
            super::switch_progress::set_phase(
                &mut runtime,
                if continue_turn && source != SwitchSource::Manual {
                    "continuing"
                } else {
                    "success"
                },
                None,
            );
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
                let _ = super::continuation::start(&state, &repo, &row, source);
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
            let row = if runs_on_selected_login {
                record_started_account(&repo, &row, &account_settings, &project_id, &session_id)
                    .unwrap_or(row)
            } else {
                row
            };
            fail(&state, &repo, &row, &error.message);
        }
    }
}

/// CDXC:AgentProviders 2026-11-01 WHY:
/// The session's recorded account must always be the one its agent runs on. A switch that failed after the selected login started (its input box never showed: a login or setup prompt, a dialog) used to keep the previous account recorded while the CLI ran on the new one, so the sidebar, usage and the next switch all described the wrong login. When the new agent is verified to run on the selected login, that account is recorded and the switch still asks for attention.
fn record_started_account(
    repo: &DomainRepository<'_>,
    row: &Value,
    account_settings: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<Value> {
    let mut runtime = row["runtimeSettings"].as_object().cloned()?;
    apply_account_settings(&mut runtime, account_settings);
    let row = endpoint::update_session(repo, row, runtime).ok()?;
    if let Some(since) = account_settings["accountSuppressedUsageNoticeAt"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
    {
        crate::session_chat_notice::suppress_account_usage_notice(
            project_id,
            session_id,
            since.with_timezone(&chrono::Utc),
        );
    }
    Some(row)
}

fn fail(state: &AppState, repo: &DomainRepository<'_>, row: &Value, error: &str) {
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.remove("accountSwitchAttempt");
    super::switch_progress::set_phase(&mut runtime, "failed", Some(error));
    runtime.insert("accountRecoverySuppressed".into(), json!(true));
    runtime.insert(
        "accountRecovery".into(),
        json!({
            "status": "needsAttention", "trigger": "accountSwitch", "attempt": 0,
            "reason": format!("{error} The account switch was not confirmed. Check the terminal before retrying."),
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
