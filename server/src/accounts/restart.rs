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
    cleanup_paths: TempFilesGuard,
    account_settings: Value,
    expected_identity: String,
    previous_process_id: Option<i64>,
    background: Option<super::claude_background::BackgroundSession>,
}

/// Removes any temporary one-shot channel secret files and account-switch scripts if a switch
/// fails before being handed off to the shell. Once the shell command is enqueued, callers
/// [`disarm`](Self::disarm) the guard so the script (`rm -f`) and `acp-host`
/// (`read_and_remove_channel_secret_file`) self-unlink without racing startup; daemon startup
/// sweep cleans up any crash leftovers.
pub(crate) struct TempFilesGuard(pub(crate) Vec<std::path::PathBuf>);

impl TempFilesGuard {
    pub(crate) fn disarm(&mut self) {
        self.0.clear();
    }
}

impl Drop for TempFilesGuard {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
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
    let (resume_command, cleanup_paths) = typed_resume_command(
        state,
        session,
        super::default_login::route_around_signed_out_default(&resume_command),
    )?;
    let zmx_name = crate::zmx::provider_zmx_session_name(session)?;
    // CDXC:AgentProviders 2026-11-16 WHY: this read of the session's own processes is the check
    // before anything is stopped (GitHub #630); the exit loop reads the same way
    // (zmx/session_tree.rs), so a session this read answers for is one the switch can follow.
    let current_process = crate::zmx::read_zmx_session_tree(&zmx_name)
        .map_err(|_| {
            DomainStateError::bad_request(
                "Could not inspect the current agent before switching accounts. Nothing was stopped.",
            )
        })?
        .and_then(|tree| tree.agent_identity());
    let project_id = session["projectId"].as_str().unwrap_or_default();
    let session_id = session["sessionId"].as_str().unwrap_or_default();
    let acp_child_pid = crate::acp::session_is_acp(session)
        .then(|| crate::acp::channel::channel_state(project_id, session_id))
        .flatten()
        .and_then(|snap| snap.get("childPid").and_then(Value::as_i64))
        .filter(|&pid| pid > 0);
    let previous_process_id = acp_child_pid.or_else(|| current_process.and_then(|i| i.process_id));
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
        cleanup_paths,
        account_settings: Value::Object(account_settings),
        expected_identity,
        previous_process_id,
        background,
    })
}

/// The longest resume command typed into the shell as it is.
const TYPED_COMMAND_LIMIT: usize = 768;

/// Prepare an in-place shell launch command for a Non-CLI (`acp-host`) session:
/// - Writes the session's host channel secret atomically to a private (`0600` under `0700` dir
///   inside `paths.auth_dir`, which is in `gxserver_private_paths()`) random-UUID one-shot file
///   and appends `--channel-secret-file <path>` to `ghostex acp-host` (which reads and unlinks it
///   immediately before spawning the agent). The secret is never written into a script, PTY, or env.
/// - Writes a random-UUID wrapper script atomically (`0600` under `0700` `account-switch/`) that
///   self-unlinks and invokes `exec <command>` via `/bin/sh` on Unix so POSIX syntax works in
///   `zsh`, `bash`, and `fish`.
/// - Returns `(command_to_type, TempFilesGuard)`. Callers must call `guard.disarm()` once the
///   shell write sequence is enqueued so files are never deleted before `acp-host` reads them.
pub(crate) fn prepare_non_cli_in_place_launch_command(
    paths: &crate::paths::GxserverPaths,
    auth_token: &str,
    project_id: &str,
    session_id: &str,
    command: String,
) -> Result<(String, TempFilesGuard), DomainStateError> {
    let trimmed = command.trim().to_string();
    let mut guard = TempFilesGuard(Vec::new());

    let effective_command = if let Some(secret_path) =
        crate::acp::channel::write_one_shot_channel_secret_file(
            &paths.auth_dir,
            auth_token,
            project_id,
            session_id,
        )
        .map_err(super::store::error)?
    {
        let quoted_secret = crate::agents::quote_shell_arg(&secret_path.to_string_lossy());
        guard.0.push(secret_path);
        format!("{trimmed} --channel-secret-file {quoted_secret}")
    } else {
        trimmed
    };

    let dir = paths.root_dir.join("account-switch");
    std::fs::create_dir_all(&dir).map_err(super::store::error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let file = dir.join(format!(
        "{project_id}-{session_id}-{}.{}",
        uuid::Uuid::new_v4(),
        if cfg!(windows) { "ps1" } else { "sh" }
    ));
    let quoted_file = crate::agents::quote_shell_arg(&file.to_string_lossy());
    let script = if cfg!(windows) {
        format!(
            "Remove-Item -LiteralPath {quoted_file} -ErrorAction SilentlyContinue\n{effective_command}\n"
        )
    } else {
        format!("rm -f {quoted_file}\nexec {effective_command}\n")
    };
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        use std::io::Write as _;
        let mut f = opts.open(&file).map_err(super::store::error)?;
        f.write_all(script.as_bytes()).map_err(super::store::error)?;
        f.flush().map_err(super::store::error)?;
    }
    guard.0.push(file);
    let typed = if cfg!(windows) {
        format!(". {quoted_file}")
    } else {
        format!("/bin/sh {quoted_file}")
    };
    Ok((typed, guard))
}

/// CDXC:AgentProviders 2026-10-14 WHY:
/// A Codex resume command carries Marked handovers inline (`-c developer_instructions=…`, Codex has no file form for it) and ran past 1 KB. Typed into the shell right after Codex exited, it arrived while the terminal was still in line mode, which keeps at most 1,024 bytes of a line on macOS: the shell got the command cut inside its quote, the agent never started and the switch failed. A command longer than `TYPED_COMMAND_LIMIT` is written to a script file in gxserver's state and the shell is given only `source '<file>'` (`. '<file>'` in PowerShell), which every shell Ghostex runs reads in full.
fn typed_resume_command(
    state: &AppState,
    session: &Value,
    command: String,
) -> Result<(String, TempFilesGuard), DomainStateError> {
    let is_acp = crate::acp::session_is_acp(session) || command.trim().contains(" acp-host ");
    if is_acp {
        let project_id = session["projectId"].as_str().unwrap_or_default();
        let session_id = session["sessionId"].as_str().unwrap_or_default();
        return prepare_non_cli_in_place_launch_command(
            &state.paths,
            &state.auth_token,
            project_id,
            session_id,
            command,
        );
    }
    if command.len() <= TYPED_COMMAND_LIMIT {
        return Ok((command, TempFilesGuard(Vec::new())));
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
    Ok((
        format!(
            "{} {}",
            if cfg!(windows) { "." } else { "source" },
            crate::agents::quote_shell_arg(&file.to_string_lossy())
        ),
        TempFilesGuard(Vec::new()),
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
    super::api_errors::switch_recovery(
        &mut runtime,
        Some(json!({
            "status": "retrying", "trigger": "accountSwitch", "attempt": 0,
            "reason": "Restarting the agent on the selected account.",
            "updatedAt": chrono::Utc::now().to_rfc3339()
        })),
        source == SwitchSource::Automatic,
    );
    endpoint::update_session_changes(repository, session, &runtime, Map::new())?;
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
        crate::agent_folder_trust::carry_claude_folder_trust_for_switch(session, &cwd);
    }
    crate::agent_folder_trust::allow_ghostex_in_codex_before_launch(session);
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
            fail(state, repository, session, &error.message, source);
            return Err(error);
        }
    };
    let mut cleanup_paths = plan.cleanup_paths;
    // The shell write sequence is now running; disarm the early-error guard so the wrapper script
    // (`rm -f`) and `acp-host` (`read_and_remove_channel_secret_file`) self-unlink without racing
    // startup. Any leftover on failure or crash is swept on daemon startup.
    cleanup_paths.disarm();
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
            endpoint::cycle,
        )
        .await;
    });
    Ok(())
}

/// Puts a session to sleep or wakes it (`endpoint::cycle`); tests stand in for the zmx daemon.
type Cycle = fn(&AppState, &DomainRepository<'_>, &Value, &str) -> Result<(), DomainStateError>;

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
    cycle: Cycle,
) {
    let (result, account_settings) = match result {
        Err(error) if super::exit::reload_recovers(&error) => {
            let reload_state = state.clone();
            let (project, session, reload_attempt) =
                (project_id.clone(), session_id.clone(), attempt.clone());
            let settings = account_settings.clone();
            let reloaded = tokio::task::spawn_blocking(move || {
                reload_on_selected_account(
                    &reload_state,
                    &project,
                    &session,
                    &reload_attempt,
                    &settings,
                    cycle,
                )
            })
            .await
            .unwrap_or_else(|_| Err(ReloadOutcome::Failed("the reload stopped".to_string())));
            match reloaded {
                Ok(settings) => {
                    log(
                        &state,
                        LogLevel::Info,
                        "accountSwitchReloaded",
                        Some(error.message),
                        started.elapsed().as_millis(),
                        &project_id,
                        &session_id,
                    );
                    (Ok(()), settings)
                }
                Err(ReloadOutcome::Superseded) => return,
                Err(ReloadOutcome::Failed(reason)) => (
                    Err(SessionChatSendError {
                        failure: error.failure,
                        message: format!(
                            "{} Restarting the session on the new account failed too: {reason}.",
                            error.message
                        ),
                    }),
                    account_settings,
                ),
            }
        }
        other => (other, account_settings),
    };
    let runs_on_selected_login = result.is_err()
        && super::restart_verification::runs_on_selected_login(
            &project_id,
            &session_id,
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
            super::api_errors::switch_recovery(
                &mut runtime,
                None,
                source == SwitchSource::Automatic,
            );
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
            let Ok(row) = endpoint::update_session_changes(&repo, &row, &runtime, Map::new())
            else {
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
            fail(&state, &repo, &row, &error.message, source);
        }
    }
}

enum ReloadOutcome {
    /// Another switch took over the session meanwhile; it reports its own result.
    Superseded,
    Failed(String),
}

/// CDXC:AgentProviders 2026-11-08 WHY:
/// User: "it kept failing to switch (said failed to return to shell something something). but after i right clicked on the session and picked full reload it reloaded with sharptabs.com@gmail.com account and all works!!!! pls fix this case! Auto heal this!!" The coordinator G9jt7 hit its weekly limit on account 4; Claude Code showed "Usage limit reached · continuing automatically at 2pm · esc to cancel", the switch's Escape cancelled that (04:28:51 UTC), but the CLI never gave the terminal back to the shell within the 15 seconds, so three switches in a row ended in "The agent did not exit back to the shell" and the session stayed on the exhausted login. When the exit gives up, the switch now does what Full Reload does, on its own: the session is recorded on the selected account, put to sleep (which stops the stuck agent with its daemon) and woken, so it resumes the same conversation on the new login. It then finishes like an in-place switch: the same continuation rules (the "." after an automatic or forced switch, nothing after a manual one) and a quiet "with a reload" on the switch card instead of a failure. Only a reload that fails itself ends in the failure card.
/// SEE-ALSO: server/src/session_chat_agent_restart.rs (the same sleep and wake), server/src/accounts/endpoint.rs (`select`'s cold path for a session whose daemon is gone).
fn reload_on_selected_account(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    attempt: &str,
    account_settings: &Value,
    cycle: Cycle,
) -> Result<Value, ReloadOutcome> {
    let failed = |reason: &dyn std::fmt::Display| ReloadOutcome::Failed(reason.to_string());
    let _gate = state.accounts.mutation_gate();
    let db = open_gxserver_database(&state.paths).map_err(|error| failed(&error))?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let row = repo
        .get_session(project_id, session_id)
        .map_err(|error| failed(&error.message))?
        .ok_or_else(|| failed(&"the session no longer exists"))?;
    if row
        .pointer("/runtimeSettings/accountSwitchAttempt")
        .and_then(Value::as_str)
        != Some(attempt)
    {
        return Err(ReloadOutcome::Superseded);
    }
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let previous = Value::Object(runtime.clone());
    // The wake launches whatever account the row names, so the row names the selected one first.
    apply_account_settings(&mut runtime, account_settings);
    if let Some(progress) = runtime
        .get_mut("accountSwitch")
        .and_then(Value::as_object_mut)
    {
        progress.insert("reloaded".into(), json!(true));
    }
    let row = endpoint::update_session_changes(&repo, &row, &runtime, Map::new())
        .map_err(|error| failed(&error.message))?;
    if let Err(error) = cycle(state, &repo, &row, "/api/sleepSession") {
        // The stuck agent still runs on the previous login, so the row keeps naming it.
        let mut runtime = row["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        apply_account_settings(&mut runtime, &previous);
        let _ = endpoint::update_session_changes(&repo, &row, &runtime, Map::new());
        return Err(failed(&format!(
            "the session could not be stopped ({})",
            error.message
        )));
    }
    crate::session_chat_options::forget_session_chat_options(state, project_id, session_id);
    cycle(state, &repo, &row, "/api/wakeSession").map_err(|error| {
        failed(&format!(
            "the session could not be started again ({})",
            error.message
        ))
    })?;
    // A wake may still move the session to another account with room (wake_account.rs); the switch
    // reports the account the agent was started on.
    let woken = repo
        .get_session(project_id, session_id)
        .ok()
        .flatten()
        .unwrap_or(row);
    let mut started_on = Map::new();
    apply_account_settings(&mut started_on, &woken["runtimeSettings"]);
    Ok(Value::Object(started_on))
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

fn fail(
    state: &AppState,
    repo: &DomainRepository<'_>,
    row: &Value,
    error: &str,
    source: SwitchSource,
) {
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.remove("accountSwitchAttempt");
    super::switch_progress::set_phase(&mut runtime, "failed", Some(error));
    runtime.insert("accountRecoverySuppressed".into(), json!(true));
    super::api_errors::switch_recovery(
        &mut runtime,
        Some(json!({
            "status": "needsAttention", "trigger": "accountSwitch", "attempt": 0,
            "reason": format!("{error} The account switch was not confirmed. Check the terminal before retrying."),
            "updatedAt": chrono::Utc::now().to_rfc3339()
        })),
        source == SwitchSource::Automatic,
    );
    if endpoint::update_session_changes(repo, row, &runtime, Map::new()).is_ok() {
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
