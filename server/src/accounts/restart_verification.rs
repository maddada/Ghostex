use crate::{
    session_chat_composer::{detect_session_chat_composer_readiness, SessionChatComposerState},
    session_chat_notice::{classify_session_chat_terminal_notice, SESSION_CHAT_NOTICE_USAGE_LIMIT},
    session_chat_send::capture_session_terminal_text,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: verify the new process's account identity and ready input before marking a switch successful or sending its continuation dot.
/// The live process's configured login is the evidence: the saved account binding and shared transcript folder can both still describe a different account.
pub(crate) async fn wait_for_account_ready(
    zmx_name: &str,
    agent: &str,
    expected_identity: &str,
    home_dir: &Path,
    previous_process_id: Option<i64>,
    timeout_ms: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut reason = "The selected account has not started yet.".to_string();
    loop {
        if cancelled() {
            return Err("The account switch was cancelled.".to_string());
        }
        let name = zmx_name.to_string();
        let home = home_dir.to_path_buf();
        let provider = agent.to_string();
        let observation = tokio::task::spawn_blocking(move || {
            let identities =
                crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(&name), &home)
                    .ok()?;
            let process = identities.get(&name)?;
            let pid = process
                .process_id
                .filter(|pid| Some(*pid) != previous_process_id)?;
            if process.agent_id.as_deref() != Some(provider.as_str()) {
                return None;
            }
            Some(super::process_login::identity(pid, &provider, &home))
        })
        .await
        .ok()
        .flatten();
        if let Some(identity) = observation {
            match identity {
                Some(identity) if identity == expected_identity => {
                    if let Some(screen) = capture_session_terminal_text(zmx_name).await {
                        // A resumed transcript can repaint the old account's usage limit. The switch already suppresses that replay.
                        let notice = classify_session_chat_terminal_notice(Some(agent), &screen)
                            .filter(|notice| notice.kind != SESSION_CHAT_NOTICE_USAGE_LIMIT);
                        let composer = detect_session_chat_composer_readiness(
                            Some(agent),
                            &screen,
                            notice.as_ref(),
                        );
                        if composer.state == SessionChatComposerState::Ready && !cancelled() {
                            return Ok(());
                        }
                    }
                    reason = "The selected account started, but its input box is not ready. Check the terminal for a login or setup prompt.".to_string();
                }
                Some(_) => {
                    return Err(
                        "The agent started with a different login than the selected account."
                            .to_string(),
                    )
                }
                None => {
                    reason = "Could not verify the new agent process's account login.".to_string()
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(reason);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
