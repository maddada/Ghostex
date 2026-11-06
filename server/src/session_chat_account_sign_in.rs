//! CDXC:AgentProviders 2026-10-26 DECISION:
//! User: "i'm seeing this appear on a thread because my account was logged out. please add a button to allow me to login there!" The sign-in error card (Claude's "Login expired · Please run /login", Codex's failed token refresh) offers a sign-in button, labeled Sign in again since the user picked the simpler card text on 2026-11-03 (Q1102; it was Reconnect, the label Settings > Accounts gives the same sign-in). On a session that runs on a saved account it runs that account's Settings sign-in (accounts/setup.rs: `ghostex account-login claude --account N`, or `xswap login N`) in the background, so the browser opens on the computer, and once the login is saved it restarts the session on its own conversation the way Switch account does: the card goes away and nothing is retyped. Other sessions on that account showing the same card restart with it. A Claude session on its own login (a plain `claude`) gets Sign in as `/login` typed into the agent, whose sign-in screens the chat already shows as a card; Codex has no /login, so a Codex session on its own login keeps Terminal view only.
//! WHY: Typing /login into a session on a saved account would sign in its shared session profile only, leaving claude-swap's saved login (which every later start of the account reads) as broken as before, and could sign in a different account into that slot.
//! SEE-ALSO: session_chat_agent_restart.rs (the `restartAgent` answer carries `reconnectAccount`), accounts/login_expired.rs (the account shows Needs sign-in until then), packages/gx-chat-core/src/questions/terminal_prompts.rs (click only, never a shortcut).

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::accounts::model::{Provider, SavedAccount};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SESSION_CHAT_NOTICE_ACTION_RECONNECT,
};
use crate::storage::open_gxserver_database;

const LABEL: &str = "Sign in again";
/// The button on the card of a Claude session on its own login, which types `/login`.
const OWN_LOGIN_LABEL: &str = "Sign in";

/// How long a Reconnect waits for the browser sign-in before giving up, as the Sign in restart does.
const SIGN_IN_WAIT: Duration = Duration::from_secs(30 * 60);

/// A Reconnect started from a session's card, by `projectId:sessionId`.
struct SignIn {
    job_id: Option<String>,
    /// The account as the card names it.
    label: String,
    error: Option<String>,
}

static SIGN_INS: LazyLock<Mutex<HashMap<String, SignIn>>> = LazyLock::new(Default::default);

fn key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

fn invalid(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: message.into(),
    }
}

/// The saved account a session runs on, or `None` for a session on its own login.
fn session_account(
    db: &rusqlite::Connection,
    provider: Provider,
    session: &Value,
) -> Option<SavedAccount> {
    if crate::accounts::launch::session_uses_own_login(provider, session) {
        return None;
    }
    let registry = crate::accounts::store::read(db).ok()?;
    let home = crate::resume_lookup::home_dir();
    let id =
        crate::accounts::session_identity::display_account_id(&registry, provider, session, &home)?
            .to_string();
    registry
        .accounts
        .into_iter()
        .find(|account| account.id == id)
}

/// How the log names the account: "account 4 (name@example.com)".
fn account_label(account: &SavedAccount) -> String {
    match account_email(account) {
        Some(email) => format!("account {} ({email})", account.selector),
        None => format!("account {}", account.selector),
    }
}

fn account_email(account: &SavedAccount) -> Option<String> {
    match account.provider {
        Provider::Claude => crate::session_chat_notice::claude_slot_email(&account.selector)
            .or_else(|| Some(account.name.clone())),
        Provider::Codex => Some(account.name.clone()),
    }
    .filter(|email| email.contains('@'))
}

/// How the card names the account: as Settings > Accounts does (`card_account_name`).
fn card_name(account: &SavedAccount) -> String {
    crate::session_chat_notice::card_account_name(
        &account.selector,
        Some(&account.name),
        account_email(account).as_deref(),
    )
}

/// Puts Reconnect on a sign-in error card (session_chat_options/detection.rs), and says what a
/// Reconnect already running or just failed is doing.
pub(crate) fn enrich_notice(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    notice: &mut SessionChatTerminalNotice,
) {
    if !notice.sign_in_error() && !notice.account_disabled() {
        return;
    }
    let (Ok(Some(project)), Ok(Some(session))) = (
        repository.get_project(project_id),
        repository.get_session(project_id, session_id),
    ) else {
        return;
    };
    let Some(provider) = crate::accounts::launch::provider(&project, &session) else {
        return;
    };
    let Some(account) = session_account(repository.db, provider, &session) else {
        if provider == Provider::Claude && notice.sign_in_error() {
            notice.actions.insert(
                0,
                SessionChatTerminalNoticeAction::send_keys(
                    SESSION_CHAT_NOTICE_ACTION_RECONNECT,
                    OWN_LOGIN_LABEL,
                    "/login\r",
                ),
            );
        }
        return;
    };
    if notice.account_disabled() {
        notice.title = format!("{} can't use Claude", card_name(&account));
        return;
    }
    let agent = match provider {
        Provider::Claude => "Claude Code",
        Provider::Codex => "Codex",
    };
    let name = card_name(&account);
    let running = SIGN_INS.lock().ok().and_then(|sign_ins| {
        sign_ins.get(&key(project_id, session_id)).map(|sign_in| {
            (
                sign_in.job_id.is_some(),
                sign_in.label.clone(),
                sign_in.error.clone(),
            )
        })
    });
    // CDXC:AgentProviders 2026-11-03 DECISION: User picked the simpler card text (Q1102 to Q1104): "<account> is signed out" with what happened and what to do; while the browser sign-in is open the card is a wait ("Finish signing in") without its sign-in button; after a failed one it says "Sign-in didn't finish" and offers Try again. The account is named as Settings > Accounts names it (`card_account_name`).
    let (title, detail, button) = match running {
        Some((true, _, _)) => {
            notice.severity = SessionChatTerminalNoticeSeverity::Warning;
            (
                "Finish signing in".to_string(),
                format!("Finish signing in to {name} in your browser. The chat continues as soon as you're done."),
                None,
            )
        }
        Some((false, _, Some(error))) => (
            "Sign-in didn't finish".to_string(),
            format!("{name} is still signed out. {error} Try again, or switch to another account."),
            Some("Try again"),
        ),
        _ => (
            format!("{name} is signed out"),
            format!("{agent}'s login for this account expired. Sign in again, or switch this chat to another account. Queued messages wait until then."),
            Some(LABEL),
        ),
    };
    notice.title = title;
    notice.detail = Some(detail);
    if let Some(button) = button {
        notice.actions.insert(
            0,
            SessionChatTerminalNoticeAction {
                id: SESSION_CHAT_NOTICE_ACTION_RECONNECT.to_string(),
                label: button.to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
        );
    }
}

/// What a Reconnect signs in to.
struct Plan {
    provider: Provider,
    account: SavedAccount,
}

fn plan(state: &AppState, project_id: &str, session_id: &str) -> Result<Plan, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| invalid(error.to_string()))?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| invalid("This session no longer exists."))?;
    let project = repository
        .get_project(project_id)?
        .ok_or_else(|| invalid("This session's project no longer exists."))?;
    let provider = crate::accounts::launch::provider(&project, &session)
        .ok_or_else(|| invalid("This agent has no Ghostex account to sign in to."))?;
    let account = session_account(&db, provider, &session).ok_or_else(|| {
        invalid("This session uses its own login, not a saved account. Open the terminal and sign in there.")
    })?;
    Ok(Plan { provider, account })
}

/// The session's account when its saved login is proven to work after the card's error was first
/// seen (`login_expired::proven_since`); `None` when Reconnect has to sign in.
fn proven_plan(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<Option<Plan>, DomainStateError> {
    let plan = plan(state, project_id, session_id)?;
    let since = crate::session_chat_options::cached_session_chat_terminal_notice(
        state, project_id, session_id,
    )
    .filter(|notice| notice.sign_in_error())
    .and_then(|notice| chrono::DateTime::parse_from_rfc3339(&notice.detected_at).ok())
    .map(|at| at.timestamp_millis())
    // No card to date the error by: nothing can be proven after it.
    .unwrap_or(i64::MAX);
    Ok(crate::accounts::login_expired::proven_since(&plan.account, since).then_some(plan))
}

fn owner(project_id: &str, session_id: &str) -> String {
    format!("sessionChat:{project_id}:{session_id}")
}

/// One setup-job operation for this Reconnect's owner; the jobs it returns.
fn setup(state: &AppState, params: Value) -> Result<Vec<Value>, DomainStateError> {
    let params = params.as_object().cloned().unwrap_or_default();
    let response = crate::accounts::setup::dispatch(state, &params)?;
    Ok(response["setupJobs"]
        .as_array()
        .cloned()
        .unwrap_or_default())
}

async fn lifecycle(state: &AppState, project_id: &str, session_id: &str, path: &str) -> bool {
    let body = json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    let ok = crate::server::handle_zmx_lifecycle_http(
        state,
        path.to_string(),
        uuid::Uuid::new_v4().to_string(),
        &body,
    )
    .await
    .response
    .status()
    .is_success();
    crate::session_chat_options::forget_session_chat_options(state, project_id, session_id);
    ok
}

/// Reconnect on a sign-in error card: starts the account's Settings sign-in, then restarts the
/// session once the login is saved (`finish`).
pub(crate) async fn reconnect(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let target = crate::session_chat_send::resolve_session_chat_send_target(
        state,
        params,
        SESSION_CHAT_NOTICE_ACTION_RECONNECT,
    )?;
    let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
    let session_key = key(&project_id, &session_id);
    {
        let mut sign_ins = SIGN_INS.lock().map_err(|_| invalid("Try again."))?;
        // A second click while the browser is open changes nothing.
        if sign_ins
            .get(&session_key)
            .is_some_and(|sign_in| sign_in.job_id.is_some())
        {
            return Ok(());
        }
        sign_ins.insert(
            session_key.clone(),
            SignIn {
                job_id: Some(String::new()),
                label: String::new(),
                error: None,
            },
        );
    }
    // CDXC:AgentProviders 2026-10-26 DECISION:
    // User (on Q1): the 12:27:06Z "Updated credentials for account 4" that looked like proof MS's saved login had survived was their own sign-in from Settings > Accounts, so "I'm not sure if it was actually just needed a reconnect". Reconnect restarts without a browser sign-in only when the saved login is proven to work after the card appeared (a sign-in through Ghostex, or a good claude-swap usage reading of that login, taken afterwards); otherwise it opens the sign-in.
    let owned = state.clone();
    let (pid, sid) = (project_id.clone(), session_id.clone());
    let proven = tokio::task::spawn_blocking(move || proven_plan(&owned, &pid, &sid))
        .await
        .map_err(|error| invalid(error.to_string()))?;
    if let Ok(Some(plan)) = proven {
        if let Ok(mut sign_ins) = SIGN_INS.lock() {
            sign_ins.remove(&session_key);
        }
        crate::session_chat_send_diagnostics::record_send_recovery(
            state,
            "sessionChatReconnectRestartOnly",
            &project_id,
            &session_id,
            &format!(
                "The saved login of {} worked after the sign-in error, so Reconnect restarted the session without a sign-in.",
                account_label(&plan.account)
            ),
            &[],
        );
        let state = state.clone();
        tokio::spawn(async move {
            restart_sessions(&state, &project_id, &session_id, &plan, false).await
        });
        return Ok(());
    }
    let started = start(state, &project_id, &session_id).await;
    let (plan, job_id, slept) = match started {
        Ok(started) => started,
        Err(error) => {
            if let Ok(mut sign_ins) = SIGN_INS.lock() {
                sign_ins.remove(&session_key);
            }
            return Err(error);
        }
    };
    if let Ok(mut sign_ins) = SIGN_INS.lock() {
        sign_ins.insert(
            session_key,
            SignIn {
                job_id: Some(job_id.clone()),
                label: account_label(&plan.account),
                error: None,
            },
        );
    }
    crate::session_chat_options::forget_session_chat_options(state, &project_id, &session_id);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionChatReconnectStarted",
        &project_id,
        &session_id,
        &format!(
            "Started the sign-in of {} from the chat's sign-in error card.",
            account_label(&plan.account)
        ),
        &[],
    );
    let state = state.clone();
    tokio::spawn(
        async move { finish(&state, &project_id, &session_id, plan, job_id, slept).await },
    );
    Ok(())
}

/// Starts the sign-in job; returns it and whether the session was put to sleep for it.
async fn start(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(Plan, String, bool), DomainStateError> {
    let owned = state.clone();
    let (pid, sid) = (project_id.to_string(), session_id.to_string());
    let plan = tokio::task::spawn_blocking(move || plan(&owned, &pid, &sid))
        .await
        .map_err(|error| invalid(error.to_string()))??;
    // xswap refuses to replace a login while Codex runs on it, and this session's Codex does.
    let slept = plan.provider == Provider::Codex;
    if slept && !lifecycle(state, project_id, session_id, "/api/sleepSession").await {
        return Err(invalid(
            "Ghostex could not stop this session's Codex to sign in again. Nothing was changed.",
        ));
    }
    let params = json!({
        "operation": "setupStart",
        "owner": owner(project_id, session_id),
        "provider": plan.provider,
        "shareHistory": true,
        "accountId": plan.account.id,
        "selector": plan.account.selector,
    });
    let owned = state.clone();
    let jobs = tokio::task::spawn_blocking(move || setup(&owned, params))
        .await
        .map_err(|error| invalid(error.to_string()));
    let job_id = jobs.and_then(|jobs| jobs).and_then(|jobs| {
        jobs.iter()
            .rev()
            .find(|job| job["acknowledged"] != true)
            .and_then(|job| job["id"].as_str().map(str::to_string))
            .ok_or_else(|| invalid("The sign-in did not start."))
    });
    match job_id {
        Ok(job_id) => Ok((plan, job_id, slept)),
        Err(error) => {
            if slept {
                lifecycle(state, project_id, session_id, "/api/wakeSession").await;
            }
            Err(error)
        }
    }
}

/// Waits for the sign-in to end. Saved: restarts the session (and the account's other sessions
/// showing the same card) on their own conversations. Failed or abandoned: the card says why.
async fn finish(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    plan: Plan,
    job_id: String,
    slept: bool,
) {
    let owner = owner(project_id, session_id);
    let started = Instant::now();
    let outcome = loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if started.elapsed() > SIGN_IN_WAIT {
            let owned = state.clone();
            let params = json!({"operation": "setupCancel", "owner": owner, "jobId": job_id});
            let _ = tokio::task::spawn_blocking(move || setup(&owned, params)).await;
            break Err(
                "The browser sign-in was still open after 30 minutes, so it was stopped."
                    .to_string(),
            );
        }
        let owned = state.clone();
        let params = json!({"operation": "setupStatus", "owner": owner});
        let Ok(Ok(jobs)) = tokio::task::spawn_blocking(move || setup(&owned, params)).await else {
            continue;
        };
        let Some(job) = jobs.iter().find(|job| job["id"] == job_id.as_str()) else {
            break Err("The sign-in stopped before it finished.".to_string());
        };
        match job["status"].as_str() {
            Some("complete") => break Ok(()),
            Some("failed") => {
                break Err(job["error"]
                    .as_str()
                    .unwrap_or("The sign-in did not finish.")
                    .to_string())
            }
            _ => {}
        }
    };
    let owned = state.clone();
    let params = json!({"operation": "setupAcknowledge", "owner": owner, "jobId": job_id});
    let _ = tokio::task::spawn_blocking(move || setup(&owned, params)).await;
    let session_key = key(project_id, session_id);
    match outcome {
        Ok(()) => {
            if let Ok(mut sign_ins) = SIGN_INS.lock() {
                sign_ins.remove(&session_key);
            }
            restart_sessions(state, project_id, session_id, &plan, slept).await;
        }
        Err(error) => {
            if slept {
                lifecycle(state, project_id, session_id, "/api/wakeSession").await;
            }
            if let Ok(mut sign_ins) = SIGN_INS.lock() {
                sign_ins.insert(
                    session_key,
                    SignIn {
                        job_id: None,
                        label: account_label(&plan.account),
                        error: Some(error.clone()),
                    },
                );
            }
            crate::session_chat_options::forget_session_chat_options(state, project_id, session_id);
            crate::session_chat_send_diagnostics::record_send_recovery(
                state,
                "sessionChatReconnectFailed",
                project_id,
                session_id,
                &format!(
                    "The sign-in of {} did not finish: {error}",
                    account_label(&plan.account)
                ),
                &[],
            );
        }
    }
}

/// The running sessions on `account` whose card still shows the sign-in error, the clicked one first.
fn sessions_to_restart(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    account: &SavedAccount,
) -> Vec<(String, String)> {
    let mut sessions = vec![(project_id.to_string(), session_id.to_string())];
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return sessions;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let home = crate::resume_lookup::home_dir();
    let Ok(registry) = crate::accounts::store::read(&db) else {
        return sessions;
    };
    for session in repository.list_sessions(None).unwrap_or_default() {
        let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        if (pid, sid) == (project_id, session_id)
            || session["lifecycleState"].as_str() != Some("running")
            || crate::accounts::session_identity::display_account_id(
                &registry,
                account.provider,
                &session,
                &home,
            ) != Some(account.id.as_str())
            || !crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid)
                .is_some_and(|notice| notice.sign_in_error())
        {
            continue;
        }
        sessions.push((pid.to_string(), sid.to_string()));
    }
    sessions
}

async fn restart_sessions(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    plan: &Plan,
    slept: bool,
) {
    let owned = state.clone();
    let (pid, sid, account) = (
        project_id.to_string(),
        session_id.to_string(),
        plan.account.clone(),
    );
    let sessions =
        tokio::task::spawn_blocking(move || sessions_to_restart(&owned, &pid, &sid, &account))
            .await
            .unwrap_or_else(|_| vec![(project_id.to_string(), session_id.to_string())]);
    // Every one sleeps before any wakes: claude-swap seeds an account's shared session profile
    // from the new login only once no Claude runs on it.
    for (index, (pid, sid)) in sessions.iter().enumerate() {
        if index == 0 && slept {
            continue;
        }
        lifecycle(state, pid, sid, "/api/sleepSession").await;
    }
    for (pid, sid) in &sessions {
        let woke = lifecycle(state, pid, sid, "/api/wakeSession").await;
        crate::session_chat_send_diagnostics::record_send_recovery(
            state,
            if woke {
                "sessionChatReconnectRestarted"
            } else {
                "sessionChatReconnectRestartFailed"
            },
            pid,
            sid,
            &format!(
                "{} signed in again from the chat's card; {} the agent on its own conversation.",
                account_label(&plan.account),
                if woke {
                    "restarted"
                } else {
                    "could not restart"
                }
            ),
            &[],
        );
    }
}
