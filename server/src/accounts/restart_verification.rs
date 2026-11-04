use crate::{
    session_chat_composer::{detect_session_chat_composer_readiness, SessionChatComposerState},
    session_chat_notice::{
        classify_session_chat_terminal_notice, SESSION_CHAT_NOTICE_TRUST_PROMPT,
        SESSION_CHAT_NOTICE_USAGE_LIMIT,
    },
    session_chat_send::capture_session_terminal_text,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// CDXC:AgentProviders 2026-10-04 WHY:
/// When the account command dies before an agent appears (cswap refusing a flag, a missing login), the switch used to wait out its timeout and say only "has not started yet"; the last terminal lines carry the command's own error.
fn terminal_tail(screen: &str) -> Option<String> {
    let lines: Vec<&str> = screen
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" · ");
    if tail.is_empty() {
        return None;
    }
    Some(tail.chars().take(400).collect())
}

/// The session a switch is waiting on.
pub(crate) struct AccountReadyTarget<'a> {
    pub project_id: &'a str,
    pub session_id: &'a str,
    pub zmx_name: &'a str,
    /// The session's folders are remembered as trusted (session_chat_trust_memory.rs).
    pub accept_remembered_trust: bool,
}

/// How long an accepted trust prompt gets to close before it is accepted again.
const TRUST_ANSWER_RETRY: Duration = Duration::from_secs(4);

/// CDXC:AgentProviders 2026-10-14 WHY:
/// Every cswap account is its own Claude profile with its own list of trusted folders, so a session switched to an account that never ran in its folder opens on Claude's "trust this folder" prompt. Ghostex accepts that prompt for remembered folders only from its screen-detection funnel, which nothing ran while this check waited, so the prompt sat there until the 30-second check gave up and the switch failed with "its input box is not ready" (the remembered answer was typed the same millisecond, on G7v5a twice and on the throwaway G9ubo on 2026-10-13). The check now accepts a remembered folder's prompt itself, under the funnel's own claim so it is typed once (`accept_remembered_trust_once` in session_chat_trust_memory.rs).
async fn accept_remembered_trust(target: &AccountReadyTarget<'_>, agent: &str) {
    crate::session_chat_trust_memory::accept_remembered_trust_once(
        target.project_id,
        target.session_id,
        target.zmx_name,
        agent,
    )
    .await;
}

/// The login of the agent the switch started: `None` while no new agent of this kind runs in the
/// session, `Some(None)` when its login cannot be read.
async fn observe_new_login(
    zmx_name: &str,
    agent: &str,
    home_dir: &Path,
    previous_process_id: Option<i64>,
) -> Option<Option<String>> {
    let name = zmx_name.to_string();
    let home = home_dir.to_path_buf();
    let provider = agent.to_string();
    tokio::task::spawn_blocking(move || {
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
    .flatten()
}

fn is_expected_login(agent: &str, expected_identity: &str, identity: &str) -> bool {
    super::launch::agent_provider(agent, None).is_some_and(|provider| {
        super::codex_identity::identity_matches(provider, expected_identity, identity)
    })
}

/// Whether the agent the switch started runs on the selected login, whatever its screen shows.
pub(crate) async fn runs_on_selected_login(
    zmx_name: &str,
    agent: &str,
    expected_identity: &str,
    home_dir: &Path,
    previous_process_id: Option<i64>,
) -> bool {
    observe_new_login(zmx_name, agent, home_dir, previous_process_id)
        .await
        .flatten()
        .is_some_and(|identity| is_expected_login(agent, expected_identity, &identity))
}

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: verify the new process's account identity and ready input before marking a switch successful or sending its continuation dot.
/// The live process's configured login is the evidence: the saved account binding and shared transcript folder can both still describe a different account.
/// CDXC:AgentProviders 2026-11-01 WHY:
/// A login verified to be the selected account that opens on Claude's usage-limit chooser has switched: that account is at its limit, and the chooser is its own live prompt, not the previous login's repaint. Waiting for the input box there failed the switch with "its input box is not ready" and kept the old account recorded while Claude ran on the new one (account 4 to account 5 at its 7d limit, 2026-11-01). The switch now succeeds and the chat shows the new account's limit card; an automatic switch's continuation dot keeps waiting for the input box as before.
pub(crate) async fn wait_for_account_ready(
    target: AccountReadyTarget<'_>,
    agent: &str,
    expected_identity: &str,
    home_dir: &Path,
    previous_process_id: Option<i64>,
    timeout_ms: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    let zmx_name = target.zmx_name;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut reason = "The selected account has not started yet.".to_string();
    let mut saw_agent = false;
    let mut trust_answered_at: Option<Instant> = None;
    loop {
        if cancelled() {
            return Err("The account switch was cancelled.".to_string());
        }
        let observation = observe_new_login(zmx_name, agent, home_dir, previous_process_id).await;
        if let Some(identity) = observation {
            saw_agent = true;
            match identity {
                Some(identity) if is_expected_login(agent, expected_identity, &identity) => {
                    if let Some(screen) = capture_session_terminal_text(zmx_name).await {
                        let notice = classify_session_chat_terminal_notice(Some(agent), &screen);
                        if notice
                            .as_ref()
                            .is_some_and(|notice| notice.usage_limit_chooser())
                            && !cancelled()
                        {
                            return Ok(());
                        }
                        // A resumed transcript can repaint the old account's usage limit. The switch already suppresses that replay.
                        let notice =
                            notice.filter(|notice| notice.kind != SESSION_CHAT_NOTICE_USAGE_LIMIT);
                        let composer = detect_session_chat_composer_readiness(
                            Some(agent),
                            &screen,
                            notice.as_ref(),
                        );
                        if composer.state == SessionChatComposerState::Ready && !cancelled() {
                            return Ok(());
                        }
                        if target.accept_remembered_trust
                            && notice.as_ref().is_some_and(|notice| {
                                notice.kind == SESSION_CHAT_NOTICE_TRUST_PROMPT
                            })
                            && trust_answered_at.is_none_or(|at| at.elapsed() >= TRUST_ANSWER_RETRY)
                            && !cancelled()
                        {
                            trust_answered_at = Some(Instant::now());
                            accept_remembered_trust(&target, agent).await;
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
            if !saw_agent {
                if let Some(tail) = capture_session_terminal_text(zmx_name)
                    .await
                    .and_then(|screen| terminal_tail(&screen))
                {
                    reason =
                        format!("The selected account did not start. The terminal shows: {tail}");
                }
            }
            return Err(reason);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
