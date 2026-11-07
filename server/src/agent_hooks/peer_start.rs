//! The hook for a session another computer's agent starts here (`ghostex coordinator
//! start-thread --computer`, `ghostex agents create --computer`): installed before the create
//! answers, so the agent finds it when it starts.
//!
//! CDXC:AgentHooks 2026-11-07 FIX:
//! Reporter (GitHub issue #431, a Mac driving two headless Linux computers that run only gxserver): "the thread runs on the remote machine fine, but its turn-end report never reaches the coordinator … The remote `~/.claude/settings.json` has no Ghostex hooks." Only the desktop's Hooks Required dialog installed a hook before a launch, and an agent on another computer never sees that dialog, so a headless computer never got one. A start from another computer now installs the agent's hook when it is missing (never over a hook the user turned off or another Ghostex owns, and never when the CLI isn't there); Settings > Agents with the computer picked and `ghostex agent-hooks install --computer` install the rest.
//! SEE-ALSO: server/src/server/route_http/dispatch.rs (calls this for a connected computer's token), apps/desktop/src/app/gx_store/create/agent.rs (the Hooks Required dialog a start from the desktop goes through).

use serde_json::{json, Map, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainRepository},
    logging::{GxserverLogInput, LogLevel},
    paths::GxserverPaths,
    server::AppState,
    storage::open_gxserver_database,
};

use super::config::HOOK_DEFINITIONS;

/// The endpoints with which another computer's agent starts an agent session here.
const PEER_AGENT_STARTS: &[&str] = &["/api/createAgentSession", "/api/createWorktreeSession"];

/// Installs the hook of the agent a connected computer's create starts, when it is missing.
/// A failure is logged and never refuses the create: the session still runs without its hook.
pub(crate) async fn install_for_peer_agent_start(
    state: &AppState,
    path: &str,
    body: &Value,
    computer: &str,
) {
    if !PEER_AGENT_STARTS.contains(&path) {
        return;
    }
    let Ok(params) = read_domain_rpc_params(body) else {
        return;
    };
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let outcome =
        tokio::task::spawn_blocking(move || install_missing_hook(&paths, &server_id, &params))
            .await
            .map_err(|error| error.to_string())
            .and_then(|outcome| outcome);
    let (level, event, error, details) = match outcome {
        Ok(None) => return,
        Ok(Some((agent_id, installed_paths))) => (
            LogLevel::Info,
            "peerAgentStartHookInstalled",
            None,
            json!({ "agentId": agent_id, "computer": computer, "installedPaths": installed_paths }),
        ),
        Err(error) => (
            LogLevel::Warn,
            "peerAgentStartHookInstallFailed",
            Some(error),
            json!({ "computer": computer }),
        ),
    };
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.into(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error,
        details: Some(details),
    });
}

/// The agent's id and the paths written, or `None` when there was nothing to install.
fn install_missing_hook(
    paths: &GxserverPaths,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Option<(String, Vec<String>)>, String> {
    let db = open_gxserver_database(paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, server_id);
    // A create this gxserver will refuse anyway installs nothing.
    let Ok(project) = repository.resolve_create_session_project(params) else {
        return Ok(None);
    };
    let Some(agent_id) = crate::agents::requested_agent_family_id(&db, &project, params) else {
        return Ok(None);
    };
    drop(db);
    if !HOOK_DEFINITIONS
        .iter()
        .any(|definition| definition.agent_id == agent_id)
    {
        return Ok(None);
    }
    let mut scoped = Map::new();
    scoped.insert("agentIds".into(), json!([agent_id]));
    let status = super::read_agent_hook_status(paths, &scoped).map_err(|error| error.message)?;
    // Only a hook that isn't there at all: `updateRequired` is repaired by the read above or is a
    // switch the user turned off (Claude's disableAllHooks, a Codex trust record), and
    // `cliMissing` has nothing to hook into.
    if row_status(&status, &agent_id) != Some("missing") {
        return Ok(None);
    }
    let installed = super::install_agent_hooks(paths, &scoped).map_err(|error| error.message)?;
    if row_status(&installed, &agent_id) != Some("installed") {
        return Err(format!("{agent_id}'s hook did not install."));
    }
    let written = installed["installedPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    Ok(Some((agent_id, written)))
}

fn row_status<'a>(status: &'a Value, agent_id: &str) -> Option<&'a str> {
    status["agents"]
        .as_array()?
        .iter()
        .find(|row| row["agentId"] == agent_id)?
        .get("status")?
        .as_str()
}
