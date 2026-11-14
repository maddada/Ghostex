//! CDXC:SessionChat 2026-09-29 WHY:
//! Codex exits to the shell after "Update now", after /logout and when it crashes, and the chat could only say it was no longer running and point at the terminal, where a new user had to know to type a resume command. Restart puts the session to sleep and wakes it, the same cycle the sidebar uses, so the agent comes back on its own conversation with the resume command Ghostex already keeps for it.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, session_chat_screen_ends_with_shell_prompt,
    SESSION_CHAT_NOTICE_ACTION_SIGN_IN, SESSION_CHAT_NOTICE_AGENT_EXITED,
};
use crate::session_chat_send::resolve_session_chat_send_target;
use crate::storage::open_gxserver_database;

fn invalid(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: message.into(),
    }
}

/// Restarts the agent of a session whose agent has exited to the shell. The screen is read again first: a
/// card left over from before the agent came back must not restart a running conversation.
pub(crate) async fn restart(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    if let Some(answered) =
        crate::session_chat_send_failure::answer_card_action(state, params).await
    {
        return answered;
    }
    match params.get("actionId").and_then(Value::as_str) {
        Some(SESSION_CHAT_NOTICE_ACTION_SIGN_IN) => return sign_in(state, params).await,
        Some(crate::memory_guard::ACTION_RESUME) => {
            return crate::server::resume_after_memory_stop(state, params).await
        }
        Some(crate::session_chat_notice::SESSION_CHAT_NOTICE_ACTION_RECONNECT) => {
            return crate::session_chat_account_sign_in::reconnect(state, params).await
        }
        Some(crate::agent_cli::opencode::ACTION_INSTALL) => {
            return install_opencode2(state, params).await
        }
        Some(crate::session_chat_notice::SESSION_CHAT_NOTICE_ACTION_INSTALL_AGENT) => {
            return install_agent_cli(state, params).await
        }
        Some(crate::agent_cli::opencode::ACTION_RESTART) => {
            return restart_on_opencode2(state, params).await
        }
        Some(crate::session_chat_conversation_unreported::ACTION_INSTALL_AND_RESTART) => {
            return crate::session_chat_conversation_unreported::answer(state, params, true).await
        }
        Some(crate::session_chat_conversation_unreported::ACTION_RESTART) => {
            return crate::session_chat_conversation_unreported::answer(state, params, false).await
        }
        Some(crate::hookless_activity::ACTION_UPDATE_CODEX) => {
            return crate::hookless_activity::answer_codex_update(state, params).await
        }
        Some(crate::hookless_activity::ACTION_TURN_ON_HOOK) => {
            return crate::hookless_activity::answer_ask_notice(state, params).await
        }
        Some(crate::hookless_activity::ACTION_RESTART_FOR_HOOK) => {
            return crate::hookless_activity::answer_fallback_notice(state, params).await
        }
        _ => {}
    }
    let start_fresh = params.get("actionId").and_then(Value::as_str)
        == Some(crate::session_chat_notice::SESSION_CHAT_NOTICE_ACTION_START_FRESH);
    let state = state.clone();
    let params = params.clone();
    tokio::task::spawn_blocking(move || {
        let target = resolve_session_chat_send_target(&state, &params, "restartAgent")?;
        let db =
            open_gxserver_database(&state.paths).map_err(|error| invalid(error.to_string()))?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let screen = crate::zmx::read_zmx_session_history_capture(
            &repository,
            &target.project_id,
            &target.session_id,
        )
        .map_err(|_| invalid("Could not read the terminal. Nothing was restarted."))?;
        let agent = crate::session_chat_composer::session_chat_composer_agent_id(&target.session);
        // Try again on the card of an agent that could not start on its account is the same
        // restart; the wake moves the session to a working account when it may (wake_account.rs).
        let exited =
            classify_session_chat_terminal_notice(agent.as_deref(), &screen.text).is_some_and(
                |notice| notice.kind == SESSION_CHAT_NOTICE_AGENT_EXITED || notice.missing_login(),
            ) || crate::session_chat_start_failure::agent_start_failure(
                &repository,
                &target.project_id,
                &target.session_id,
                &screen.text,
            )
            .is_some();
        if !exited {
            return Err(invalid(
                "The agent is running in this terminal again, so it was not restarted.",
            ));
        }
        // Start fresh on "Couldn't find this chat's conversation": the wake starts a new
        // conversation instead of looking the old one up again (`forget_conversation`).
        if start_fresh {
            crate::session_chat_send_failure::forget_conversation(
                &state,
                &target.project_id,
                &target.session_id,
            );
        }
        let session = repository
            .get_session(&target.project_id, &target.session_id)?
            .ok_or_else(|| invalid("This session no longer exists."))?;
        crate::accounts::endpoint::cycle(&state, &repository, &session, "/api/sleepSession")?;
        crate::accounts::endpoint::cycle(&state, &repository, &session, "/api/wakeSession")
    })
    .await
    .map_err(|error| invalid(error.to_string()))?
}

/// Install OpenCode 2 on the card of an OpenCode chat that cannot connect (session_chat_opencode/mirror.rs):
/// runs the catalog's official installer in the background, then restarts the session, whose new
/// terminal runs the OpenCode 2 just installed (agent_cli/opencode.rs). The card shows the install
/// while it runs and its error if it fails.
async fn install_opencode2(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let target = resolve_session_chat_send_target(state, params, "installOpenCode2")?;
    crate::agent_cli::opencode::begin_install().map_err(invalid)?;
    let state = state.clone();
    tokio::spawn(async move {
        let home = crate::resume_lookup::home_dir();
        if crate::agent_cli::opencode::install(&home).await.is_ok() {
            let _ = crate::session_chat_queue_runtime::cycle_session(
                &state,
                &target.project_id,
                &target.session_id,
            )
            .await;
        }
    });
    Ok(())
}

/// Install on the card of an agent whose command is not on this computer
/// (`agent_command_missing_notice`): installs the CLI the screen named with the method Settings >
/// Agents would pick, in the background, then restarts the session. The job shows on the Agents page
/// like any other install.
async fn install_agent_cli(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let target = resolve_session_chat_send_target(state, params, "installAgentCli")?;
    let read_state = state.clone();
    let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
    let command = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&read_state.paths).ok()?;
        let repository = DomainRepository::new(&db, &read_state.metadata.server_id);
        let screen =
            crate::zmx::read_zmx_session_history_capture(&repository, &project_id, &session_id)
                .ok()?;
        let notice = crate::session_chat_start_failure::agent_start_failure(
            &repository,
            &project_id,
            &session_id,
            &screen.text,
        )?;
        notice
            .error_lines
            .as_deref()
            .and_then(crate::session_chat_notice::missing_start_command)
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| {
        invalid("The terminal no longer shows a missing agent command, so nothing was installed.")
    })?;
    let agent_id = crate::agent_cli::catalog::CATALOG
        .iter()
        .find(|definition| definition.binary == command)
        .map(|definition| definition.agent_id.clone())
        .ok_or_else(|| invalid(format!("Ghostex doesn't know how to install {command}.")))?;
    let state = state.clone();
    tokio::spawn(async move {
        match crate::agent_cli::endpoint::install_default_and_wait(&state.paths, &agent_id).await {
            Ok(_) => {
                let _ = crate::session_chat_queue_runtime::cycle_session(
                    &state,
                    &target.project_id,
                    &target.session_id,
                )
                .await;
            }
            Err(reason) => {
                eprintln!("gxserver: installing {agent_id} from a chat card failed: {reason}")
            }
        }
    });
    Ok(())
}

/// Restart on OpenCode 2: a session still running OpenCode 1 while OpenCode 2 is installed too.
async fn restart_on_opencode2(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let target = resolve_session_chat_send_target(state, params, "restartOnOpenCode2")?;
    let home = crate::resume_lookup::home_dir();
    let found = tokio::task::spawn_blocking(move || {
        crate::agent_cli::opencode::launch_executable(&home).is_ok()
    })
    .await
    .unwrap_or(false);
    if !found {
        return Err(invalid(
            "OpenCode 2 is no longer installed, so the session was not restarted.",
        ));
    }
    crate::session_chat_queue_runtime::cycle_session(state, &target.project_id, &target.session_id)
        .await
        .map_err(|reason| invalid(format!("Ghostex could not restart this session: {reason}.")))
}

/// Sign in on the card of an agent that could not start because its account has no saved login
/// (session_chat_notice/start_failure.rs): types `ghostex account-login claude --account <slot>`
/// into the shell the failed start left, then restarts the agent once that login finishes.
async fn sign_in(state: &AppState, params: &Map<String, Value>) -> Result<(), DomainStateError> {
    let target = resolve_session_chat_send_target(state, params, "signInAccount")?;
    let screen = crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
        .await
        .ok_or_else(|| invalid("Could not read the terminal. Nothing was typed."))?;
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&target.session);
    let slot = classify_session_chat_terminal_notice(agent.as_deref(), &screen)
        .and_then(|notice| notice.missing_login_slot())
        .ok_or_else(|| {
            invalid("This terminal is no longer waiting for a sign-in, so nothing was typed.")
        })?;
    let home = crate::resume_lookup::home_dir();
    let ghostex = crate::accounts::helpers::executable(&home, "ghostex")
        .ok_or_else(|| invalid("The ghostex command was not found on this computer."))?;
    // PowerShell runs a quoted path only through the call operator (accounts/launch.rs).
    let invoke = if cfg!(windows) { "& " } else { "" };
    let command = format!(
        "{invoke}{} account-login claude --account {slot}",
        crate::agents::quote_shell_arg(&ghostex.to_string_lossy())
    );
    crate::session_chat_send::write_session_chat_payload(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-sign-in",
        &crate::agents::as_atuin_ignored_shell_input(&command),
    )
    .await
    .map_err(invalid)?;
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionChatStartSignInTyped",
        &target.project_id,
        &target.session_id,
        &format!("Typed the sign-in for account {slot} into the session's shell."),
        &[],
    );
    let state = state.clone();
    tokio::spawn(async move {
        restart_after_sign_in(
            &state,
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
        )
        .await
    });
    Ok(())
}

/// How long the restart after Sign in waits for the login in the terminal to finish.
const SIGN_IN_WAIT: Duration = Duration::from_secs(30 * 60);

/// Waits until the shell prompt is back (the login finished, failed or was cancelled), then
/// restarts the agent once: on the new login it comes up and a held message goes out; without
/// it, the start fails again and the card comes back.
async fn restart_after_sign_in(state: &AppState, project_id: &str, session_id: &str, zmx: &str) {
    let started = Instant::now();
    while started.elapsed() < SIGN_IN_WAIT {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let Some(screen) = crate::session_chat_send::capture_session_terminal_text(zmx).await
        else {
            // No daemon: the session was slept or restarted meanwhile.
            return;
        };
        if !session_chat_screen_ends_with_shell_prompt(&screen) {
            continue;
        }
        if crate::session_chat_queue_runtime::cycle_session(state, project_id, session_id)
            .await
            .is_ok()
        {
            crate::session_chat_send_diagnostics::record_send_recovery(
                state,
                "sessionChatStartRestartedAfterSignIn",
                project_id,
                session_id,
                "The sign-in in the terminal finished; restarted the agent.",
                &[],
            );
        }
        return;
    }
}

/// One automatic retry of a start that failed on a missing login per session in this window, so
/// a login cswap reports as saved but the CLI still refuses cannot restart the agent in a loop.
const START_RETRY_COOLDOWN: Duration = Duration::from_secs(5 * 60);

static START_RETRIES: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);

fn claim_start_retry(project_id: &str, session_id: &str) -> bool {
    let Ok(mut retries) = START_RETRIES.lock() else {
        return false;
    };
    retries.retain(|_, at| at.elapsed() < START_RETRY_COOLDOWN);
    retries
        .insert(format!("{project_id}:{session_id}"), Instant::now())
        .is_none()
}

/// CDXC:SessionChat 2026-10-14 DECISION:
/// User: a chat message queued for a session whose agent cannot start must not sit "queued" forever with no explanation; "failures time can fix retry on their own; a missing login can't, so it must be shown, never silently retried forever". While a session holds a message the user sent (a startup send) and its daemon is up but no input box appears, the queue scheduler calls this on the startup send's wake backoff (every 10 s, settling at every 60 s). An agent that exited to the shell is restarted on its own conversation (at most once in ten minutes, `restart_agent_for_send`). An agent that could not start because its account has no saved login shows its card (session_chat_notice/start_failure.rs) and is restarted only when that can now work: the login was saved meanwhile (Settings > Accounts, the card's Sign in, or `ghostex account-login`), or the wake may move the session to an account that works (accounts/wake_account.rs). The message stays held through all of it and goes out once the input box appears.
/// SEE-ALSO: session_chat_send_wake.rs (`wake_session_holding_startup_send` calls this), session_chat_queue_runtime/scheduler.rs.
pub(crate) async fn heal_failed_start(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<&'static str> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    let Ok(target) = resolve_session_chat_send_target(state, &params, "startHeal") else {
        return None;
    };
    if crate::agentbox::is_agentbox_session(&target.session) {
        return None;
    }
    // Offline, a restarted agent could not reach its API either, and the restart would spend the
    // ten-minute allowance: the message stays held and the next check (10 to 60 s) tries again
    // (CDXC:OfflineHold in network_status).
    if crate::network_status::session_offline(&target.session) {
        return None;
    }
    let notice = crate::session_chat_options::cached_session_chat_terminal_notice(
        state, project_id, session_id,
    );
    if let Some(slot) = notice
        .as_ref()
        .and_then(|notice| notice.missing_login_slot())
    {
        let owned = state.clone();
        let (pid, sid) = (project_id.to_string(), session_id.to_string());
        let fixable = tokio::task::spawn_blocking(move || {
            claude_slot_has_login(&slot) || wake_moves_account(&owned, &pid, &sid)
        })
        .await
        .unwrap_or(false);
        if !fixable || !claim_start_retry(project_id, session_id) {
            return None;
        }
        if crate::session_chat_queue_runtime::cycle_session(state, project_id, session_id)
            .await
            .is_ok()
        {
            crate::session_chat_send_diagnostics::record_send_recovery(
                state,
                "sessionChatStartRetried",
                project_id,
                session_id,
                "The agent could not start on an account without a saved login; a login or a working account is available now, so it was restarted.",
                &[],
            );
        }
        return None;
    }
    let composer = crate::session_chat_options::cached_session_chat_composer_readiness(
        state, project_id, session_id,
    );
    let exited = match notice.as_ref() {
        Some(notice) => notice.kind == SESSION_CHAT_NOTICE_AGENT_EXITED,
        None => session_chat_screen_ends_with_shell_prompt(&composer.screen_tail.join("\n")),
    };
    if !exited
        || crate::session_chat_send::session_agent_process_running(
            &target.zmx_name,
            &crate::resume_lookup::home_dir(),
        )
        .await
    {
        return None;
    }
    // Records its own outcome; a refusal (restarted a few minutes ago, nothing to resume) waits
    // for the next check, unless no later check could restart it either (GitHub #456).
    crate::session_chat_queue_runtime::restart_agent_for_send(state, &target, false)
        .await
        .err()
        .filter(|reason| crate::session_chat_queue_runtime::restart_refusal_is_final(reason))
}

/// Whether claude-swap now holds a usable login for `slot`.
fn claude_slot_has_login(slot: &str) -> bool {
    let home = crate::resume_lookup::home_dir();
    crate::accounts::helpers::json_command(&home, "cswap", &["list", "--json"]).is_ok_and(|list| {
        list["accounts"].as_array().is_some_and(|rows| {
            rows.iter().any(|row| {
                row["number"]
                    .as_u64()
                    .map(|number| number.to_string())
                    .as_deref()
                    == Some(slot)
                    && matches!(
                        row["usageStatus"].as_str(),
                        // `token_expired`: a working login whose refresh is deferred (accounts/helpers.rs).
                        Some("ok" | "unavailable" | "api_key" | "token_expired")
                    )
            })
        })
    })
}

/// Whether waking the session now would move it to another account (accounts/wake_account.rs).
fn wake_moves_account(state: &AppState, project_id: &str, session_id: &str) -> bool {
    // The wake reads the shared usage snapshot; keep it no older than the usual two minutes.
    state.accounts.refresh(&state.paths.home_dir, false);
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let (Ok(Some(project)), Ok(Some(session))) = (
        repository.get_project(project_id),
        repository.get_session(project_id, session_id),
    ) else {
        return false;
    };
    crate::accounts::wake_account::rebind_target(&repository, &project, &session)
        .is_ok_and(|target| target.is_some())
}

/// `/api/reloadSessionAgent` (`ghostex agents reload`): stops the session's agent and starts it
/// again on its own conversation, whatever its screen shows: a hung turn, a broken screen, a
/// start that failed. It is the sleep-and-wake cycle the send heal and Sign in restart with,
/// asked for by hand, so it runs without their idle and cooldown checks; whatever the agent was
/// doing in that turn is lost. The wake may move the session to a working account when its own
/// cannot start (accounts/wake_account.rs).
/// SEE-ALSO: server/src/accounts/cli.rs (the user's decision behind the CLI verbs).
pub(crate) async fn reload(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let read = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
    };
    let (project_id, session_id) = (read("projectId")?, read("sessionId")?);
    let session = read_session(state, &project_id, &session_id).await?;
    crate::agentbox::refuse_for_agentbox_session(&session, "Reloading the agent")?;
    if !crate::session_chat_queue_runtime::has_conversation_to_resume(&session).await {
        return Err(invalid(
            "This session has no saved conversation to resume yet, so a reload would start a new one. Nothing was reloaded.",
        ));
    }
    crate::session_chat_queue_runtime::cycle_session_explained(state, &project_id, &session_id)
        .await
        .map_err(|failure| invalid(format!("Ghostex could not reload this session: {failure}.")))?;
    note_agent_restarted(state, &project_id, &session_id);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionAgentReloaded",
        &project_id,
        &session_id,
        "Reloaded the agent on its own conversation on request (ghostex agents reload).",
        &[],
    );
    let session = read_session(state, &project_id, &session_id).await?;
    Ok(json!({
        "status": "reloaded",
        "session": {
            "globalRef": session["globalRef"],
            "title": session["title"],
            "lifecycleState": session["lifecycleState"],
            "accountId": session.pointer("/runtimeSettings/accountId"),
            "agentSessionId": session.pointer("/runtimeSettings/agentSessionId"),
        },
    }))
}

/// After a reload or a self-heal restart (session_self_heal.rs): the chat drops the old agent's
/// watchdog notice and shows the new terminal's state.
pub(crate) fn note_agent_restarted(state: &AppState, project_id: &str, session_id: &str) {
    crate::session_chat_notice::clear_session_chat_watchdog_notice(project_id, session_id);
    crate::session_chat_options::session_chat_terminal_notice_publisher(
        state, project_id, session_id,
    )();
}

async fn read_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let state = state.clone();
    let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
    tokio::task::spawn_blocking(move || {
        let db =
            open_gxserver_database(&state.paths).map_err(|error| invalid(error.to_string()))?;
        DomainRepository::new(&db, &state.metadata.server_id)
            .get_session(&project_id, &session_id)?
            .ok_or_else(|| DomainStateError::not_found("Session not found."))
    })
    .await
    .map_err(|error| invalid(error.to_string()))?
}

/// How long [`restart_when_idle`] waits for the agent to finish a turn that started while its update ran.
const RESTART_WHEN_IDLE_WAIT: Duration = Duration::from_secs(30 * 60);

/// Restarts a running session's agent on its own conversation (the sleep and wake above) once it has no
/// turn, question or compaction going, and never on a session whose background work a restart would end.
/// `agent` names the agent in the reasons it gives back; `source` labels the send-target read.
///
/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User (Q74): only restart when there is a conversation to resume and the session is still running; never wake a session the user put to sleep or closed while the update ran. Both are read again on every check, right before the restart.
/// CDXC:Updates 2026-11-05 WHY: Codex's "Update now" (session_chat_codex_update.rs) and Settings > Updates (updates/install.rs, the user's Q1363 "restart when idle") restart sessions onto a new CLI the same way, so the loop lives here.
pub(crate) async fn restart_when_idle(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    agent: &str,
    source: &'static str,
) -> Result<(), String> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    let started = Instant::now();
    loop {
        let target = resolve_session_chat_send_target(state, &params, source)
            .map_err(|_| "the session was closed".to_string())?;
        if target.session["lifecycleState"].as_str() != Some("running") {
            return Err(
                "the session was put to sleep or stopped, so Ghostex left it that way".to_string(),
            );
        }
        if !crate::session_chat_queue_runtime::has_conversation_to_resume(&target.session).await {
            return Err(format!(
                "this session has no conversation yet for {agent} to come back to"
            ));
        }
        if let Some(reason) =
            crate::session_chat_queue_runtime::work_a_restart_would_end(state, &target).await
        {
            return Err(reason.to_string());
        }
        if !crate::session_chat_queue_runtime::agent_is_busy(&target.session).await {
            return crate::session_chat_queue_runtime::cycle_session(state, project_id, session_id)
                .await
                .map_err(|reason| reason.to_string());
        }
        if started.elapsed() > RESTART_WHEN_IDLE_WAIT {
            return Err(format!("{agent} stayed busy for 30 minutes"));
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
