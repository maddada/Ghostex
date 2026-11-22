//! CDXC:SessionChat 2026-11-05 WHY:
//! Issue #418: an OpenCode chat sent nothing until the terminal's TUI plugin had created and
//! reported a conversation, so a session nobody opened in a terminal (asleep, deferred, or with
//! its terminal closed) answered "Open an OpenCode conversation in the terminal first". OpenCode 2
//! runs conversations in its background service, which creates one with a chosen id, so the chat
//! needs no terminal: a new session already carries the id Ghostex gave it at launch
//! (agents/opencode_session_id.rs), an older row without one gets one here when its terminal is
//! not running, and the first send (or model pick) starts the service when it is down and creates
//! that conversation in the session's folder. The terminal opens the same conversation later with
//! `opencode --session <id>`. A running terminal with no id yet still reports its own conversation.
//! SEE-ALSO: server/src/session_chat_queue_runtime/send.rs, server/src/session_chat_follower/registry.rs (followability).

use super::{error, safe_id, Client};
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
    storage::open_gxserver_database,
};
use serde_json::{json, Map, Value};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// The conversation a chat send or model pick goes to, with the folder it belongs in.
pub(crate) struct ChatConversation {
    pub(crate) id: String,
    pub(crate) directory: Option<String>,
}

/// The session's conversation id, given one first when it has none and its terminal is not
/// running OpenCode (a running TUI creates and reports its own).
pub(crate) fn chat_conversation(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
) -> Result<ChatConversation, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|e| error(e.to_string()))?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository.get_project(project_id)?;
    let directory = crate::session_git_status::effective_session_git_cwd(session, project.as_ref());
    let id = match super::session_id(session) {
        Ok(id) => id,
        Err(refusal) => {
            if crate::server::read_session_text(session, "lifecycleState").as_deref()
                == Some("running")
            {
                return Err(refusal);
            }
            let id = crate::agents::opencode_session_id::new_opencode_session_id();
            let mut runtime = session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            runtime.insert("agentSessionId".to_string(), json!(id));
            let mut update = Map::new();
            update.insert("projectId".to_string(), json!(project_id));
            update.insert("sessionId".to_string(), json!(session_id));
            update.insert("runtimeSettings".to_string(), Value::Object(runtime));
            repository.update_session(&update)?;
            // Clients and the chat follower learn the conversation it now follows.
            let _ = crate::server::schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                project_id,
                session_id,
            );
            id
        }
    };
    Ok(ChatConversation { id, directory })
}

/// A client of a running OpenCode service, started first when none answers.
pub(crate) fn connect() -> Result<Client, DomainStateError> {
    if let Ok(client) = Client::discover() {
        if client.request("GET", "/api/info", None).is_ok() {
            return Ok(client);
        }
    }
    start_service()?;
    let started = Instant::now();
    loop {
        if let Ok(client) = Client::discover() {
            if client.request("GET", "/api/info", None).is_ok() {
                return Ok(client);
            }
        }
        if started.elapsed() > Duration::from_secs(20) {
            return Err(error(
                "OpenCode's background service did not start. Open the terminal to see why.",
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// `opencode service start`, which returns once the service has registered itself; one start
/// at a time, so two chats sending at once do not race for the service's port.
fn start_service() -> Result<(), DomainStateError> {
    static STARTING: Mutex<()> = Mutex::new(());
    let _turn = STARTING.lock().unwrap_or_else(|e| e.into_inner());
    if Client::discover().is_ok_and(|client| client.request("GET", "/api/info", None).is_ok()) {
        return Ok(());
    }
    let home = crate::resume_lookup::home_dir();
    let program = crate::agent_cli::opencode::opencode2_program(&home).ok_or_else(|| {
        error("OpenCode 2 is not installed. Install it from Settings > Agents, then send again.")
    })?;
    // The service it starts outlives the command, so nothing waits on output it may keep open.
    let mut child = crate::platform::process::background_command(&program)
        .args(["service", "start"])
        .current_dir(&home)
        // The service runs every tool call of every conversation, so it gets the user's PATH.
        .env(
            "PATH",
            crate::agent_hooks::probing::normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                &home,
            ),
        )
        // The Ghostex plugin only reports for a terminal Ghostex started.
        .env_remove("GHOSTEX_SESSION_ID")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| error("OpenCode's background service could not be started."))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(30) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    Err(error(
        "OpenCode's background service did not start. Open the terminal to see why.",
    ))
}

/// A client whose service holds conversation `id`, created in `directory` when it does not exist
/// yet (OpenCode's `session.create` takes the id; creating an existing one changes nothing).
pub(crate) fn ensure_conversation(
    id: &str,
    directory: Option<&str>,
) -> Result<Client, DomainStateError> {
    if !safe_id(id) {
        return Err(error("Invalid OpenCode session ID."));
    }
    let client = connect()?;
    if client.exists(id)? {
        return Ok(client);
    }
    let mut body = json!({ "id": id });
    if let Some(directory) = directory {
        body["location"] = json!({ "directory": directory });
    }
    client.request("POST", "/api/session", Some(body))?;
    super::invalidate(id);
    Ok(client)
}

/// Creates the conversation a terminal is about to open with `opencode --session <id>` (the
/// session's own id, in the folder the chat sends to), before the TUI starts.
///
/// CDXC:SessionChat 2026-11-10 WHY:
/// GitHub #538: OpenCode 2.2.6's TUI started with `--session <id>` for a conversation that does not exist yet shows its empty start page and stays there, and the first chat send then creates that conversation through the service (`ensure_conversation`), so the work ran in a conversation the terminal never showed. A coordinator's thread starts its agent and then sends its task, so it always lost that race (reproduced on macOS too; it looked like a remote-only bug). Creating the conversation first makes the TUI open it, as it does when the send comes first. When the service cannot be reached the launch goes on as before; the TUI starts the service itself.
pub(crate) fn create_before_terminal_launch(
    session: &Value,
    project: Option<&Value>,
    startup_text: &str,
) {
    if crate::session_chat_composer::session_chat_composer_agent_id(session).as_deref()
        != Some("opencode")
    {
        return;
    }
    let Ok(id) = super::session_id(session) else {
        return;
    };
    let opens_it = startup_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair[0] == "--session" && pair[1].trim_matches(['"', '\'']) == id);
    if !opens_it {
        return;
    }
    let directory = crate::session_git_status::effective_session_git_cwd(session, project);
    let _ = ensure_conversation(&id, directory.as_deref());
}

/// Whether a chat with no readable conversation can still take its first message: the service
/// has no conversation by that id yet, or it is not running and an OpenCode 2 can start it.
pub(crate) fn awaits_first_message(id: &str) -> bool {
    // A registration left by a service that has since exited reads as not running.
    match Client::discover().and_then(|client| client.exists(id)) {
        Ok(exists) => !exists,
        Err(_) => crate::agent_cli::opencode::opencode2_program(&crate::resume_lookup::home_dir())
            .is_some(),
    }
}
