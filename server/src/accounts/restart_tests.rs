//! The account switch's way out when the agent keeps the terminal (accounts/restart.rs): a switch
//! whose exit gives up restarts the session on the new account like Full Reload, and one whose
//! agent exits never does.

use super::*;
use crate::session_chat_send::SessionChatSendFailure;
use std::sync::Mutex;

/// Every sleep and wake a test's stand-in daemon was asked for, as `<sessionId> <path>`.
static CYCLES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record_cycle(
    _: &AppState,
    _: &DomainRepository<'_>,
    session: &Value,
    path: &str,
) -> Result<(), DomainStateError> {
    CYCLES.lock().unwrap().push(format!(
        "{} {path}",
        session["sessionId"].as_str().unwrap_or_default()
    ));
    Ok(())
}

fn refuse_sleep(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    path: &str,
) -> Result<(), DomainStateError> {
    record_cycle(state, repo, session, path)?;
    Err(DomainStateError::bad_request("zmx is not answering"))
}

fn cycles_of(session: &str) -> Vec<String> {
    CYCLES
        .lock()
        .unwrap()
        .iter()
        .filter_map(|line| line.strip_prefix(&format!("{session} ")))
        .map(str::to_string)
        .collect()
}

struct Switching {
    state: Arc<AppState>,
    project: String,
    session: String,
    _temp: tempfile::TempDir,
}

const ATTEMPT: &str = "switch-1";

impl Switching {
    /// A running Claude session on account 4 whose in-place switch to account 5 is under way, as
    /// `select` and `start` leave it.
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = crate::server::tests::helpers::test_app_state(
            crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf())),
        );
        let db = open_gxserver_database(&state.paths).expect("db");
        let repo = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let params = |value: Value| value.as_object().cloned().expect("params");
        let project = repo
            .create_project(&params(json!({ "name": "Switch", "path": temp.path() })))
            .expect("project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let row = repo
            .create_session(
                &params(json!({
                    "projectId": project, "kind": "agent", "agentId": "claude",
                    "title": "Main Coordinator", "lifecycleState": "running",
                })),
                false,
            )
            .expect("session");
        let session = row["sessionId"].as_str().expect("sessionId").to_string();
        let mut runtime = row["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        runtime.insert("accountId".into(), json!("claude-4"));
        runtime.insert("accountName".into(), json!("maddada123spam"));
        runtime.insert("accountCommand".into(), json!("cswap run 4 -- claude"));
        runtime.insert("accountSwitchAttempt".into(), json!(ATTEMPT));
        runtime.insert(
            "accountSwitch".into(),
            json!({
                "id": ATTEMPT, "provider": "claude", "source": "manual", "phase": "switching",
                "fromAccountId": "claude-4", "toAccountId": "claude-5",
                "updatedAt": chrono::Utc::now().to_rfc3339(),
            }),
        );
        endpoint::update_session(&repo, &row, runtime).expect("switching");
        Self {
            state,
            project,
            session,
            _temp: temp,
        }
    }

    fn stored(&self) -> Value {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        DomainRepository::new(&db, self.state.metadata.server_id.as_str())
            .get_session(&self.project, &self.session)
            .expect("read")
            .expect("session")
    }

    async fn finish(&self, result: Result<(), SessionChatSendError>, cycle: Cycle) {
        finish(
            self.state.clone(),
            self.project.clone(),
            self.session.clone(),
            ATTEMPT.to_string(),
            selected_account(),
            StartedLogin {
                zmx_name: format!("gx-test-{}", self.session),
                agent: "claude".into(),
                expected_identity: "sharptabs.com@gmail.com".into(),
                previous_process_id: None,
            },
            SwitchSource::Manual,
            result,
            Instant::now(),
            cycle,
        )
        .await;
    }
}

fn selected_account() -> Value {
    json!({
        "accountId": "claude-5",
        "accountName": "sharptabs",
        "accountCommand": "cswap run 5 -- claude",
    })
}

/// The exit's own failure: Claude Code's usage-limit screen kept the terminal for 15 seconds.
fn stuck_on_limit_screen() -> SessionChatSendError {
    SessionChatSendError {
        failure: SessionChatSendFailure::ComposerNotReady,
        message: super::super::exit::AGENT_DID_NOT_EXIT.to_string(),
    }
}

#[tokio::test]
async fn an_agent_that_never_returns_to_the_shell_is_reloaded_on_the_new_account() {
    let switch = Switching::new();
    switch
        .finish(Err(stuck_on_limit_screen()), record_cycle)
        .await;
    assert_eq!(
        cycles_of(&switch.session),
        ["/api/sleepSession", "/api/wakeSession"],
        "Full Reload's sleep and wake, in that order"
    );
    let runtime = &switch.stored()["runtimeSettings"];
    assert_eq!(runtime["accountId"], "claude-5");
    assert_eq!(runtime["accountCommand"], "cswap run 5 -- claude");
    assert_eq!(runtime["accountSwitch"]["phase"], "success");
    assert_eq!(runtime["accountSwitch"]["reloaded"], true);
    assert!(runtime["accountSwitch"].get("reason").is_none());
    assert!(runtime.get("accountSwitchAttempt").is_none());
    assert_ne!(
        runtime.pointer("/accountRecovery/status"),
        Some(&json!("needsAttention")),
        "no failure card"
    );
    let log = std::fs::read_to_string(&switch.state.paths.log_file).unwrap_or_default();
    assert!(log.contains("accountSwitchReloaded"), "the heal is logged");
    assert!(!log.contains("accountSwitchRestartFailed"));
}

#[tokio::test]
async fn an_agent_that_exits_switches_in_place_without_a_reload() {
    let switch = Switching::new();
    switch.finish(Ok(()), record_cycle).await;
    assert!(cycles_of(&switch.session).is_empty(), "the fast path");
    let runtime = &switch.stored()["runtimeSettings"];
    assert_eq!(runtime["accountId"], "claude-5");
    assert_eq!(runtime["accountSwitch"]["phase"], "success");
    assert!(runtime["accountSwitch"].get("reloaded").is_none());
}

#[test]
fn a_reload_that_cannot_stop_the_agent_keeps_the_account_it_runs_on() {
    let switch = Switching::new();
    let outcome = reload_on_selected_account(
        &switch.state,
        &switch.project,
        &switch.session,
        ATTEMPT,
        &selected_account(),
        refuse_sleep,
    );
    assert!(
        matches!(outcome, Err(ReloadOutcome::Failed(reason)) if reason.contains("could not be stopped"))
    );
    assert_eq!(cycles_of(&switch.session), ["/api/sleepSession"]);
    let runtime = &switch.stored()["runtimeSettings"];
    assert_eq!(runtime["accountId"], "claude-4");
    assert_eq!(runtime["accountCommand"], "cswap run 4 -- claude");
}

#[test]
fn a_newer_switch_is_left_alone() {
    let switch = Switching::new();
    let outcome = reload_on_selected_account(
        &switch.state,
        &switch.project,
        &switch.session,
        "an-older-switch",
        &selected_account(),
        record_cycle,
    );
    assert!(matches!(outcome, Err(ReloadOutcome::Superseded)));
    assert!(cycles_of(&switch.session).is_empty());
    assert_eq!(switch.stored()["runtimeSettings"]["accountId"], "claude-4");
}
