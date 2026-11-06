//! A Pi or OMP session whose agent runs but never told Ghostex which conversation it is: the chat
//! says why instead of staying empty.
//!
//! CDXC:SessionChat 2026-11-07 FIX:
//! Discord (Fardiaz138, #linux, 2026-11-05, Ubuntu 24.04): "I can use Pi as terminal but not as chat. I use Codex subscription in my Pi setup, but can't start any chat session using it. But if I switch to terminal session, it works, just like using it in native terminal." Reproduced on Ubuntu 24.04 with the released Linux gxserver: Pi was on the PATH of the user's shell but not on the one gxserver searches, so Ghostex installed no Pi extension and launched Pi without `--session-id`. Pi ran and answered in the terminal, while the chat showed "What should we build with Pi Agent?" forever, with nothing to say why. The coordinator's brief: "Make a failure say why in plain words." Ghostex now finds such a Pi (`node_tool_dirs` in packages/paths/src/tool_path.rs), and a Pi or OMP session that still has no conversation once its agent has run for `SETTLE` gets this card: what is missing, why, and one button that fixes it (install the extension and restart, or restart when it is installed but not loaded), plus Open terminal, where the messages did arrive.
//! SEE-ALSO: session_chat_start_failure.rs (an agent that never came up), session_chat_agent_restart.rs (the card's buttons ride its restart answer).

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{read_runtime_text, AppState};
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource,
};

/// The card's kind: a chat that cannot see its agent's conversation.
pub(crate) const NOTICE_KIND: &str = "chatNotConnected";
/// Install Ghostex's extension for the session's agent, then restart the session.
pub(crate) const ACTION_INSTALL_AND_RESTART: &str = "installChatExtension";
/// Restart a session whose extension is installed but was not loaded.
pub(crate) const ACTION_RESTART: &str = "restartForChatExtension";

/// Pi's and OMP's extensions report the conversation within a second or two of the agent's start;
/// this leaves room for a slow start before the card appears.
const SETTLE: Duration = Duration::from_secs(20);

/// How long one check of the agent's process and hook status is reused: the chat's detection runs
/// every few seconds, and both read outside gxserver (the process table, the agent's config).
const RECHECK: Duration = Duration::from_secs(10);

/// Per session (`project:session`): when it was first seen running without a conversation, and the
/// last check after `SETTLE`.
static WAITING: LazyLock<Mutex<HashMap<String, Waiting>>> = LazyLock::new(Default::default);

struct Waiting {
    since: Instant,
    checked: Option<(Instant, Option<SessionChatTerminalNotice>)>,
}

/// The agent's id, its name in the card, its command, and the variable that moves its config folder.
struct ExtensionAgent {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    config_dir_variable: &'static str,
}

/// Why the conversation is unknown, read from the agent's hook status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cause {
    /// gxserver cannot find the agent's command, so it never installed the extension.
    CommandNotFound,
    /// The extension is not installed (or is out of date).
    NotInstalled,
    /// The extension is installed, but this agent process never loaded it or never reported.
    NotLoaded,
}

/// The agents whose chat learns its conversation only from Ghostex's extension inside the agent.
fn extension_agent(session: &Value) -> Option<ExtensionAgent> {
    match crate::session_chat_follower::session_chat_agent_for_session(session).as_deref() {
        Some("pi") => Some(ExtensionAgent {
            id: "pi",
            name: "Pi",
            command: "pi",
            config_dir_variable: "PI_CODING_AGENT_DIR",
        }),
        Some("omp") => Some(ExtensionAgent {
            id: "omp",
            name: "OMP",
            command: "omp",
            config_dir_variable: "PI_CONFIG_DIR",
        }),
        _ => None,
    }
}

/// The card for a running Pi or OMP session that has had no conversation for `SETTLE`.
pub(crate) fn detect(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let key = format!("{project_id}:{session_id}");
    let session = repository
        .get_session(project_id, session_id)
        .ok()
        .flatten();
    let waiting = session.as_ref().is_some_and(waiting_for_conversation);
    let mut sessions = WAITING.lock().unwrap_or_else(|error| error.into_inner());
    if !waiting {
        sessions.remove(&key);
        return None;
    }
    let entry = sessions.entry(key.clone()).or_insert_with(|| Waiting {
        since: Instant::now(),
        checked: None,
    });
    if entry.since.elapsed() < SETTLE {
        return None;
    }
    if let Some((at, notice)) = &entry.checked {
        if at.elapsed() < RECHECK {
            return notice.clone();
        }
    }
    drop(sessions);
    let session = session?;
    let notice = agent_process_running(&session)
        .then(|| extension_agent(&session))
        .flatten()
        .map(|agent| notice(&agent, cause(agent.id)));
    if let Some(entry) = WAITING
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get_mut(&key)
    {
        entry.checked = Some((Instant::now(), notice.clone()));
    }
    notice
}

/// A running local Pi or OMP session with no conversation id yet.
fn waiting_for_conversation(session: &Value) -> bool {
    extension_agent(session).is_some()
        && crate::server::read_session_text(session, "lifecycleState").as_deref() == Some("running")
        && !crate::agentbox::is_agentbox_session(session)
        && read_runtime_text(session, "agentSessionId").is_none()
}

/// Something runs in the session's terminal (an agent that never came up is
/// session_chat_start_failure.rs's card).
fn agent_process_running(session: &Value) -> bool {
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(session) else {
        return false;
    };
    crate::zmx::read_zmx_session_process_identities(
        std::slice::from_ref(&zmx_name),
        &crate::resume_lookup::home_dir(),
    )
    .is_ok_and(|identities| identities.contains_key(&zmx_name))
}

fn cause(agent_id: &str) -> Cause {
    let mut params = Map::new();
    params.insert("agentIds".to_string(), json!([agent_id]));
    let status = crate::agent_hooks::read_agent_hook_status(
        &crate::paths::get_gxserver_paths(None),
        &params,
    )
    .ok();
    let row_status = status
        .as_ref()
        .and_then(|status| status.get("agents"))
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
        })
        .and_then(|row| row.get("status"))
        .and_then(Value::as_str)
        .map(str::to_string);
    match row_status.as_deref() {
        Some("cliMissing") => Cause::CommandNotFound,
        Some("installed") => Cause::NotLoaded,
        _ => Cause::NotInstalled,
    }
}

fn notice(agent: &ExtensionAgent, cause: Cause) -> SessionChatTerminalNotice {
    let (agent_name, command, variable) = (agent.name, agent.command, agent.config_dir_variable);
    let missing = format!(
        "{agent_name} is running in this session's terminal, but it hasn't told Ghostex which conversation this chat is, so nothing shows here. Your messages still reach {agent_name}: Open terminal shows them."
    );
    let (why, action) = match cause {
        Cause::CommandNotFound => (
            format!(
                "Ghostex couldn't find the `{command}` command on its own PATH (only your shell's startup files add it), so it never installed its {agent_name} extension, which tells the chat about the conversation. Install and restart adds the extension and starts {agent_name} again in a new conversation."
            ),
            action(ACTION_INSTALL_AND_RESTART, "Install and restart"),
        ),
        Cause::NotInstalled => (
            format!(
                "Ghostex's {agent_name} extension, which tells the chat about the conversation, isn't installed. Install and restart adds it and starts {agent_name} again in a new conversation."
            ),
            action(ACTION_INSTALL_AND_RESTART, "Install and restart"),
        ),
        Cause::NotLoaded => (
            format!(
                "Ghostex's {agent_name} extension is installed, but this {agent_name} didn't load it: it started before the extension was installed, or it reads its extensions from another folder ({variable}). Restart starts {agent_name} again in a new conversation with the extension."
            ),
            action(ACTION_RESTART, "Restart"),
        ),
    };
    SessionChatTerminalNotice::new(
        NOTICE_KIND,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        format!("This chat can't see {agent_name}'s conversation"),
    )
    .with_detail(format!("{missing} {why}"))
    .with_actions(vec![
        action,
        SessionChatTerminalNoticeAction::switch_to_terminal("Open terminal"),
    ])
}

fn action(id: &str, label: &str) -> SessionChatTerminalNoticeAction {
    SessionChatTerminalNoticeAction {
        id: id.to_string(),
        label: label.to_string(),
        // Rides the answer every chat client already sends (session_chat_agent_restart.rs).
        kind: SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    }
}

fn invalid(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: message.into(),
    }
}

/// The card's Install and restart and Restart buttons: installs the extension when asked, then
/// sleeps and wakes the session so the agent starts with it. Refused when the chat found its
/// conversation in the meantime, so a stale card cannot restart a working chat.
pub(crate) async fn answer(
    state: &AppState,
    params: &Map<String, Value>,
    install: bool,
) -> Result<(), DomainStateError> {
    let target =
        crate::session_chat_send::resolve_session_chat_send_target(state, params, "restartAgent")?;
    if !waiting_for_conversation(&target.session) {
        return Err(invalid(
            "This chat sees its conversation now, so nothing was restarted.",
        ));
    }
    let agent = extension_agent(&target.session)
        .ok_or_else(|| invalid("This session doesn't run Pi or OMP."))?;
    let (agent_id, agent_name) = (agent.id, agent.name);
    if install {
        let paths = state.paths.clone();
        let agent = agent_id.to_string();
        tokio::task::spawn_blocking(move || {
            crate::agent_hooks::install_agent_hooks_for_running_agent(&paths, &agent)
        })
        .await
        .map_err(|error| invalid(error.to_string()))?
        .map_err(|error| {
            invalid(format!(
                "Couldn't install Ghostex's {agent_name} extension: {}",
                error.message
            ))
        })?;
    }
    WAITING
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&format!("{}:{}", target.project_id, target.session_id));
    crate::session_chat_queue_runtime::cycle_session(state, &target.project_id, &target.session_id)
        .await
        .map_err(|reason| invalid(format!("The extension is installed, but {reason}.")))
}
