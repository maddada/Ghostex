//! The safety net under hook-free tracking: a running Claude, Codex or OpenCode that its watcher
//! cannot see gets its hook back for its session, and the chat says so once; a Codex too old to
//! report its approvals is asked to update instead. Also the upkeep that keeps the hooks matching
//! Use agent hooks: the start cleanup, and the switch turned on or off.
//!
//! CDXC:AgentHooks 2026-11-12 DECISION:
//! The user's safety net (agent_hooks/hook_free.rs): "Ghostex switches back to a hook automatically when its watcher can't see a running agent (for example after an agent update changes its files), and says so in the chat." A session counts as unseen only while its agent's own process runs in its terminal (the zmx process scan) and for `UNSEEN_FOR` in a row, far longer than any watcher takes on a normal start: Claude writes its session record as it starts, Codex reports OSC 7501 on its first frame, OpenCode's service stream connects within seconds. Then: Claude with no session record Ghostex can read for that terminal; Codex reporting no OSC 7501 status (0.162 and older never does: that one is asked to update, the user's "Ask to update", and gets no hook) or working with no rollout Ghostex can find; OpenCode whose service stream is down or whose conversation Ghostex does not know. Ghostex installs that agent's hook, records the session (its hook drives it from then on and its watcher stops, drive.rs), raises one card in the session's chat ("Claude's hooks turned on", with Restart, which starts the agent again so the hook takes over at once; the chat's own Dismiss closes it, and it is not raised again for that session), and logs it in the Hook-free activity comparison area (and once, unconditionally, without paths). Agents read their hooks when they start, so the hook takes over the next time the agent starts in that terminal.
//! SEE-ALSO: server/src/agent_hooks/hook_free.rs (the records), server/src/agent_hooks/hook_free_cleanup.rs (the removal), server/src/session_chat_codex_update.rs (the update the Codex card runs).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::claude::ClaudeObserver;
use super::codex::CodexObserver;
use crate::agent_hooks::hook_free;
use crate::domain::DomainStateError;
use crate::logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel};
use crate::server::{read_session_text, AppState};
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource,
};

/// How often the running sessions are checked.
const CHECK_EVERY: Duration = Duration::from_secs(5);
/// How long a running agent must stay unseen before its hook comes back.
const UNSEEN_FOR: Duration = Duration::from_secs(90);
/// How often the upkeep reads the setting and the CLI versions.
const UPKEEP_EVERY: Duration = Duration::from_secs(5);

/// The Codex card's Update button (answered in session_chat_agent_restart.rs).
pub(crate) const ACTION_UPDATE_CODEX: &str = "updateCodexForHookFree";
/// The Codex card's kind.
pub(crate) const NOTICE_KIND: &str = "codexTooOldForHookFree";
/// The fallback card's kind and its Restart button (the chat adds Dismiss to every info card).
pub(crate) const FALLBACK_NOTICE_KIND: &str = "agentHookFallback";
pub(crate) const ACTION_RESTART_FOR_HOOK: &str = "restartForAgentHook";

/// Codex sessions running a Codex too old to report OSC 7501, with its version.
static CODEX_TOO_OLD: LazyLock<Mutex<HashMap<(String, String), String>>> =
    LazyLock::new(Default::default);
/// Sessions whose fallback card is up: the agent, and why it fell back.
static FALLBACK_CARDS: LazyLock<Mutex<HashMap<(String, String), (String, Unseen)>>> =
    LazyLock::new(Default::default);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Why a running agent's watcher cannot see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Unseen {
    /// Claude: no session record Ghostex can read for this terminal.
    NoSessionRecord,
    /// Codex: no OSC 7501 status from the terminal.
    NoProgramStatus,
    /// Codex: working, but no rollout Ghostex follows.
    NoRollout,
    /// OpenCode: the service's event stream is not connected.
    ServiceUnreachable,
    /// OpenCode: Ghostex does not know which conversation the terminal shows.
    NoConversation,
}

impl Unseen {
    fn code(self) -> &'static str {
        match self {
            Unseen::NoSessionRecord => "noSessionRecord",
            Unseen::NoProgramStatus => "noProgramStatus",
            Unseen::NoRollout => "noRollout",
            Unseen::ServiceUnreachable => "serviceUnreachable",
            Unseen::NoConversation => "noConversation",
        }
    }

    /// The end of "Ghostex is using …'s hooks for this session because …".
    fn because(self) -> &'static str {
        match self {
            Unseen::NoSessionRecord => {
                "it can't read the status file Claude keeps for this terminal"
            }
            Unseen::NoProgramStatus => "this Codex doesn't report its status to the terminal",
            Unseen::NoRollout => "it can't find the file this Codex writes its work to",
            Unseen::ServiceUnreachable => "it can't reach OpenCode's background service",
            Unseen::NoConversation => "it doesn't know which OpenCode chat this terminal shows",
        }
    }
}

/// What a running session shows its watcher, as the watch reads it each check.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Seen {
    pub claude_followed: bool,
    pub codex_followed: bool,
    /// The daemon keeps OSC 7501 records for the session.
    pub program_status_supported: bool,
    /// A program in the session reported OSC 7501 status.
    pub program_status_reported: bool,
    /// Its OSC 7501 root record says it is working.
    pub program_working: bool,
    pub opencode_connected: bool,
    pub opencode_followed: bool,
}

/// Why `agent` is unseen, or `None` when its watcher sees it (or cannot tell).
pub(super) fn unseen(agent: &str, seen: Seen) -> Option<Unseen> {
    match agent {
        "claude" => (!seen.claude_followed).then_some(Unseen::NoSessionRecord),
        // A daemon that keeps no OSC 7501 records cannot tell a Codex that reports none.
        "codex" if !seen.program_status_supported => None,
        "codex" if !seen.program_status_reported => Some(Unseen::NoProgramStatus),
        "codex" => (seen.program_working && !seen.codex_followed).then_some(Unseen::NoRollout),
        "opencode" if !seen.opencode_connected => Some(Unseen::ServiceUnreachable),
        "opencode" => (!seen.opencode_followed).then_some(Unseen::NoConversation),
        _ => None,
    }
}

/// What to do about a session unseen for `UNSEEN_FOR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Remedy {
    /// Install the agent's hook for the session.
    Hook,
    /// Ask the user to update this Codex (0.162 and older): no hook.
    UpdateCodex(String),
}

pub(super) fn remedy(agent: &str, why: Unseen, codex_version: Option<&str>) -> Remedy {
    match (agent, why, codex_version) {
        ("codex", Unseen::NoProgramStatus, Some(version))
            if hook_free::version_before(version, hook_free::CODEX_PROGRAM_STATUS_SINCE) =>
        {
            Remedy::UpdateCodex(version.to_string())
        }
        _ => Remedy::Hook,
    }
}

#[derive(Default)]
pub(super) struct FallbackWatch {
    checked_at: Option<Instant>,
    /// When each session was first seen unseen, and why.
    unseen_since: HashMap<(String, String), (Unseen, Instant)>,
    /// Sessions whose hook did not install, so the install is not retried every check.
    failed: HashSet<(String, String)>,
}

impl FallbackWatch {
    pub(super) fn poll(
        &mut self,
        state: &AppState,
        sessions: &[Value],
        claude: &ClaudeObserver,
        codex: &CodexObserver,
    ) {
        if self.checked_at.is_some_and(|at| at.elapsed() < CHECK_EVERY) {
            return;
        }
        self.checked_at = Some(Instant::now());
        let mut candidates = Vec::new();
        for session in sessions {
            let Some(agent) = crate::session_chat_composer::session_chat_composer_agent_id(session)
            else {
                continue;
            };
            let Some(agent) = hook_free::HOOK_FREE_AGENTS
                .iter()
                .copied()
                .find(|known| *known == agent)
            else {
                continue;
            };
            if !hook_free::hook_free(&state.paths, agent)
                || crate::agentbox::is_agentbox_session(session)
            {
                continue;
            }
            let (Some(project_id), Some(session_id), Some(zmx_name)) = (
                read_session_text(session, "projectId"),
                read_session_text(session, "sessionId"),
                read_session_text(session, "zmxName"),
            ) else {
                continue;
            };
            if hook_free::session_fell_back(&state.paths, &project_id, &session_id) {
                continue;
            }
            candidates.push((agent, (project_id, session_id), zmx_name));
        }
        let names: Vec<String> = candidates.iter().map(|(_, _, name)| name.clone()).collect();
        let identities = match crate::zmx::read_cached_zmx_session_process_identities(
            &names,
            &state.paths.home_dir,
        ) {
            Ok(identities) => identities,
            // A failed scan cannot say what runs: nothing counts as unseen this time.
            Err(_) => return,
        };
        let claude_panes: HashSet<&(String, String)> = claude.panes().collect();
        let codex_panes: HashSet<&(String, String)> = codex.followed().collect();
        let opencode_panes: HashSet<(String, String)> =
            super::opencode::followed().into_iter().collect();
        let opencode_connected = crate::session_chat_opencode::events::connected();
        let mut live = HashSet::new();
        for (agent, pane, name) in candidates {
            let running = identities
                .get(&name)
                .is_some_and(|identity| identity.agent_id.as_deref() == Some(agent));
            if !running {
                continue;
            }
            let status = crate::program_status::program_status_entry(&pane.0, &pane.1);
            let seen = Seen {
                claude_followed: claude_panes.contains(&pane),
                codex_followed: codex_panes.contains(&pane),
                program_status_supported: status.as_ref().is_some_and(|entry| entry.supported),
                program_status_reported: status.as_ref().is_some_and(|entry| entry.reported),
                program_working: status.as_ref().is_some_and(|entry| {
                    entry
                        .records
                        .iter()
                        .any(|record| record.id.is_empty() && record.state == "working")
                }),
                opencode_connected,
                opencode_followed: opencode_panes.contains(&pane),
            };
            let Some(why) = unseen(agent, seen) else {
                if agent == "codex" {
                    lock(&CODEX_TOO_OLD).remove(&pane);
                }
                continue;
            };
            live.insert(pane.clone());
            let since = match self.unseen_since.get(&pane) {
                Some((known, since)) if *known == why => *since,
                _ => {
                    self.unseen_since
                        .insert(pane.clone(), (why, Instant::now()));
                    continue;
                }
            };
            if since.elapsed() < UNSEEN_FOR || self.failed.contains(&pane) {
                continue;
            }
            let codex_version = hook_free::known_cli_version("codex");
            match remedy(agent, why, codex_version.as_deref()) {
                Remedy::UpdateCodex(version) => {
                    let mut too_old = lock(&CODEX_TOO_OLD);
                    if too_old.insert(pane.clone(), version.clone()).is_none() {
                        drop(too_old);
                        log_fallback(state, agent, &pane, why, "askedToUpdate", None);
                        crate::session_chat_options::session_chat_terminal_notice_publisher(
                            state, &pane.0, &pane.1,
                        )();
                    }
                }
                Remedy::Hook => {
                    if !self.fall_back(state, agent, &pane, why) {
                        self.failed.insert(pane.clone());
                    }
                }
            }
        }
        self.unseen_since.retain(|pane, _| live.contains(pane));
        self.failed.retain(|pane| live.contains(pane));
    }

    /// Installs `agent`'s hook for the session and says so in its chat. Returns `false` when the
    /// hook could not be installed.
    fn fall_back(
        &mut self,
        state: &AppState,
        agent: &str,
        pane: &(String, String),
        why: Unseen,
    ) -> bool {
        match crate::agent_hooks::install_agent_hooks_for_running_agent(&state.paths, agent) {
            Ok(_) => {}
            Err(error) => {
                log_fallback(
                    state,
                    agent,
                    pane,
                    why,
                    "installFailed",
                    Some(error.message),
                );
                return false;
            }
        }
        if hook_free::record_fallback(&state.paths, agent, &pane.0, &pane.1, why.code()) {
            lock(&FALLBACK_CARDS).insert(pane.clone(), (agent.to_string(), why));
            crate::session_chat_options::session_chat_terminal_notice_publisher(
                state, &pane.0, &pane.1,
            )();
            log_fallback(state, agent, pane, why, "hookInstalled", None);
        }
        true
    }
}

fn log_fallback(
    state: &AppState,
    agent: &str,
    pane: &(String, String),
    why: Unseen,
    outcome: &str,
    error: Option<String>,
) {
    let details = json!({
        "agent": agent,
        "projectId": pane.0,
        "sessionId": pane.1,
        "reason": why.code(),
        "outcome": outcome,
    });
    let entry = |level| GxserverLogInput {
        level,
        event: "hooklessActivity.fallback".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: error.clone(),
        details: Some(details.clone()),
    };
    let _ = state.logger.log_routine(
        DiagnosticLogScenario::HooklessActivityComparison,
        entry(LogLevel::Info),
    );
    // A change to the user's agent config is worth a line even with diagnostics off (it names no
    // file, only the agent, the session and why).
    let _ = if error.is_some() {
        state.logger.log(entry(LogLevel::Warn))
    } else {
        state.logger.log_record(entry(LogLevel::Info))
    };
}

/// The card that says a session went back to its agent's hook, until it is dismissed or the agent
/// is restarted from it (once: it is not raised again after a gxserver restart).
pub(crate) fn fallback_notice(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let (agent, why) = lock(&FALLBACK_CARDS)
        .get(&(project_id.to_string(), session_id.to_string()))
        .cloned()?;
    Some(fallback_card(&agent, why))
}

fn fallback_card(agent: &str, why: Unseen) -> SessionChatTerminalNotice {
    let name = crate::agent_hooks::api::hook_free_agent_name(agent);
    let (noun, verb) = if agent == "opencode" {
        ("plugin", "It takes")
    } else {
        ("hooks", "They take")
    };
    SessionChatTerminalNotice::new(
        FALLBACK_NOTICE_KIND,
        SessionChatTerminalNoticeSeverity::Info,
        SessionChatTerminalNoticeSource::Screen,
        format!("{name}'s {noun} turned on"),
    )
    .with_detail(format!(
        "Ghostex is using {name}'s {noun} for this session because {}. {verb} over the next time {name} starts here.",
        why.because()
    ))
    .with_actions(vec![
        SessionChatTerminalNoticeAction {
            id: ACTION_RESTART_FOR_HOOK.to_string(),
            label: format!("Restart {name}"),
            kind: SessionChatTerminalNoticeActionKind::RestartAgent,
            send: None,
        },
    ])
}

/// The fallback card's Restart: starts the agent again on its conversation so the hook takes over
/// now (the sleep and wake every restart uses).
pub(crate) async fn answer_fallback_notice(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let target =
        crate::session_chat_send::resolve_session_chat_send_target(state, params, "restartAgent")?;
    let pane = (target.project_id.clone(), target.session_id.clone());
    lock(&FALLBACK_CARDS).remove(&pane);
    crate::session_chat_options::session_chat_terminal_notice_publisher(state, &pane.0, &pane.1)();
    crate::session_chat_queue_runtime::cycle_session(state, &pane.0, &pane.1)
        .await
        .map_err(|reason| DomainStateError {
            code: "invalidState",
            message: format!("The hook is on, but {reason}."),
        })
}

/// The card of a Codex session running a Codex too old to show its approvals in the chat.
pub(crate) fn codex_update_notice(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let version = lock(&CODEX_TOO_OLD)
        .get(&(project_id.to_string(), session_id.to_string()))
        .cloned()?;
    Some(
        SessionChatTerminalNotice::new(
            NOTICE_KIND,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Screen,
            "Update Codex",
        )
        .with_detail(format!(
            "Codex {version} is too old for Ghostex to show its approvals and questions here. Update Codex and Ghostex starts it again once it is idle."
        ))
        .with_actions(vec![
            SessionChatTerminalNoticeAction {
                id: ACTION_UPDATE_CODEX.to_string(),
                label: "Update Codex".to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            SessionChatTerminalNoticeAction::switch_to_terminal("Open terminal"),
        ]),
    )
}

/// The card's Update Codex: Ghostex's own updater for the copy a new terminal runs, then a
/// restart once Codex is idle (session_chat_codex_update.rs).
pub(crate) async fn answer_codex_update(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let invalid = |message: String| DomainStateError {
        code: "invalidState",
        message,
    };
    let target =
        crate::session_chat_send::resolve_session_chat_send_target(state, params, "restartAgent")?;
    let home = state.paths.agent_config_home_dir().to_path_buf();
    let agent = crate::agent_cli::endpoint::read_agent(&home, "codex").await?;
    // Without a known latest release, the first one that reports its approvals is the bar.
    let latest = agent["latestVersion"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| hook_free::CODEX_PROGRAM_STATUS_SINCE.to_string());
    let prompt = crate::session_chat_codex_dialog::CodexUpdatePrompt {
        current: String::new(),
        latest: latest.clone(),
        command: String::new(),
    };
    match crate::session_chat_codex_update::plan(&agent, &prompt, |_| false) {
        crate::session_chat_codex_update::UpdatePlan::Explain(message) => Err(invalid(message)),
        crate::session_chat_codex_update::UpdatePlan::CodexOwn => Err(invalid(
            "Ghostex can't update this Codex. Update it with the tool that installed it, then start it again.".into(),
        )),
        plan => {
            lock(&CODEX_TOO_OLD).remove(&(target.project_id.clone(), target.session_id.clone()));
            crate::session_chat_codex_update::PendingUpdate::new(plan, latest, String::new())
                .spawn(state, &target);
            Ok(())
        }
    }
}

/// Keeps the hooks matching Use agent hooks: reads the CLI versions the rules need, removes
/// Ghostex's hooks at start and when the setting turns off, and installs them when it turns on.
pub(super) fn spawn_upkeep(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut last: Option<bool> = None;
        loop {
            let pass_state = state.clone();
            let previous = last;
            last = tokio::task::spawn_blocking(move || upkeep(&pass_state, previous))
                .await
                .ok()
                .or(last);
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(UPKEEP_EVERY) => {}
            }
        }
    })
}

/// One upkeep pass; returns the setting it saw.
fn upkeep(state: &AppState, previous: Option<bool>) -> bool {
    let home = state.paths.agent_config_home_dir().to_path_buf();
    for agent in ["claude", "codex"] {
        hook_free::refresh_cli_version(agent, &home);
    }
    hook_free::forget_cached_setting();
    let use_hooks = hook_free::use_agent_hooks(&state.paths);
    let (event, details) = match (previous, use_hooks) {
        (Some(false), false) | (Some(true), true) | (None, true) => return use_hooks,
        (_, false) => {
            let cleanup = crate::agent_hooks::remove_hook_free_agent_hooks(&state.paths);
            if cleanup.removed_paths.is_empty() && cleanup.errors.is_empty() {
                return use_hooks;
            }
            ("hookFreeCleanup", cleanup.to_value())
        }
        (Some(false), true) => {
            // The hooks drive everything again: fallbacks no longer mean anything.
            hook_free::clear_fallbacks(&state.paths);
            let mut params = Map::new();
            params.insert("agentIds".into(), json!(hook_free::HOOK_FREE_AGENTS));
            let result = crate::agent_hooks::install_agent_hooks(&state.paths, &params);
            (
                "useAgentHooksInstalled",
                match result {
                    Ok(status) => json!({
                        "agents": hook_free::HOOK_FREE_AGENTS,
                        "installedPaths": status["installedPaths"],
                    }),
                    Err(error) => json!({
                        "agents": hook_free::HOOK_FREE_AGENTS,
                        "error": error.message,
                    }),
                },
            )
        }
    };
    let entry = |details: Value| GxserverLogInput {
        level: LogLevel::Info,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    };
    // The files changed go to the Hook-free activity comparison area; the record that it happened
    // (which agents, how many files) is kept whatever the diagnostics switches say.
    let _ = state.logger.log_routine(
        DiagnosticLogScenario::HooklessActivityComparison,
        entry(details.clone()),
    );
    let count = |key: &str| details[key].as_array().map_or(0, Vec::len);
    let _ = state.logger.log_record(entry(json!({
        "agents": details["agents"],
        "filesChanged": count("removedPaths") + count("installedPaths"),
        "errors": count("errors") + usize::from(details.get("error").is_some()),
    })));
    use_hooks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_running_agent_is_seen() {
        let seen = Seen {
            claude_followed: true,
            codex_followed: true,
            program_status_supported: true,
            program_status_reported: true,
            program_working: true,
            opencode_connected: true,
            opencode_followed: true,
        };
        for agent in ["claude", "codex", "opencode"] {
            assert_eq!(unseen(agent, seen), None, "{agent}");
        }
        // An idle Codex has no rollout to follow before its first prompt, and that is normal.
        assert_eq!(
            unseen(
                "codex",
                Seen {
                    codex_followed: false,
                    program_working: false,
                    ..seen
                }
            ),
            None
        );
    }

    #[test]
    fn each_agent_is_unseen_for_its_own_reason() {
        let none = Seen::default();
        assert_eq!(unseen("claude", none), Some(Unseen::NoSessionRecord));
        // A daemon without OSC 7501 cannot tell.
        assert_eq!(unseen("codex", none), None);
        let supported = Seen {
            program_status_supported: true,
            ..none
        };
        assert_eq!(unseen("codex", supported), Some(Unseen::NoProgramStatus));
        let working = Seen {
            program_status_reported: true,
            program_working: true,
            ..supported
        };
        assert_eq!(unseen("codex", working), Some(Unseen::NoRollout));
        assert_eq!(unseen("opencode", none), Some(Unseen::ServiceUnreachable));
        assert_eq!(
            unseen(
                "opencode",
                Seen {
                    opencode_connected: true,
                    ..none
                }
            ),
            Some(Unseen::NoConversation)
        );
        assert_eq!(unseen("pi", none), None);
    }

    #[test]
    fn an_old_codex_is_asked_to_update_instead_of_getting_a_hook() {
        assert_eq!(
            remedy("codex", Unseen::NoProgramStatus, Some("codex-cli 0.162.1")),
            Remedy::UpdateCodex("codex-cli 0.162.1".to_string())
        );
        assert_eq!(
            remedy("codex", Unseen::NoProgramStatus, Some("codex-cli 0.167.0")),
            Remedy::Hook
        );
        assert_eq!(remedy("codex", Unseen::NoProgramStatus, None), Remedy::Hook);
        assert_eq!(
            remedy("codex", Unseen::NoRollout, Some("codex-cli 0.162.1")),
            Remedy::Hook
        );
        assert_eq!(
            remedy("claude", Unseen::NoSessionRecord, None),
            Remedy::Hook
        );
    }

    #[test]
    fn the_fallback_card_says_why_in_the_users_words() {
        let card = fallback_card("claude", Unseen::NoSessionRecord);
        assert_eq!(card.kind, FALLBACK_NOTICE_KIND);
        assert_eq!(card.title, "Claude's hooks turned on");
        assert!(card
            .detail
            .as_deref()
            .unwrap_or_default()
            .starts_with("Ghostex is using Claude's hooks for this session because it can't read"));
        let opencode = fallback_card("opencode", Unseen::ServiceUnreachable);
        assert_eq!(opencode.title, "OpenCode's plugin turned on");
        assert!(fallback_notice("Py", "Gy").is_none());
    }

    #[test]
    fn the_codex_card_shows_only_for_a_session_asked_to_update() {
        assert!(codex_update_notice("Pz", "Gz").is_none());
        lock(&CODEX_TOO_OLD).insert(("Pz".into(), "Gz".into()), "0.162.0".into());
        let notice = codex_update_notice("Pz", "Gz").expect("card");
        assert_eq!(notice.kind, NOTICE_KIND);
        lock(&CODEX_TOO_OLD).remove(&("Pz".to_string(), "Gz".to_string()));
    }
}
