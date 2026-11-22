use super::identity::text;
use crate::ghostex_cli::{
    args::Flags,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};
use serde_json::{json, Value};

/// The launcher row of `agent_id` (the Agents roster's row for a hidden one; `Null` from a gxserver
/// without the roster), or the refusal.
/// CDXC:AgentLauncher 2026-10-14 DECISION:
/// User: `ghostex create-agent`, and every other CLI path that starts an agent by id (`run-agent`, `agents create`, `orchestrator start-thread`, `board start-work --agent`), refuses an agent that is turned off in Settings › Agents unless `--allow-hidden` is passed, with a message that names the agent and lists the visible ones to use instead. A test agent ran `create-agent claude`; that built-in was turned off and its command was the user's own wrapper bound to an account with no saved login, so the session never started and sat in the sidebar as an untitled "Claude Session".
pub(crate) fn require_launchable_agent(
    agent_id: &str,
    allow_hidden: bool,
    flags: &Flags,
) -> CliResult<Value> {
    let hud = call_gxserver_rpc(
        "/api/readSidebarHud",
        &json!({ "includeAgentRoster": true }),
        flags,
    )?;
    let visible = hud["agents"].as_array().cloned().unwrap_or_default();
    if let Some(row) = visible.iter().find(|row| text(row, "agentId") == agent_id) {
        return Ok(row.clone());
    }
    let choices = visible
        .iter()
        .map(|row| format!("{} ({})", text(row, "agentId"), text(row, "name")))
        .collect::<Vec<_>>()
        .join(", ");
    // A gxserver older than the Agents roster cannot tell a hidden agent from an unknown one.
    let Some(roster) = hud.get("agentRoster").and_then(Value::as_array) else {
        return if allow_hidden {
            Ok(Value::Null)
        } else {
            Err(CliError::Other(format!(
                "Unknown or hidden agent type: {agent_id}. Use a visible agent: {choices}. Pass --allow-hidden to start a hidden one anyway."
            )))
        };
    };
    match roster.iter().find(|row| text(row, "agentId") == agent_id) {
        Some(row) if allow_hidden => Ok(row.clone()),
        Some(row) => Err(CliError::Other(format!(
            "Agent \"{agent_id}\" ({}) is turned off in Settings > Agents, so it was not started. Use a visible agent instead: {choices}. Pass --allow-hidden to start it anyway.",
            text(row, "name")
        ))),
        None => Err(CliError::Other(format!(
            "Unknown agent type: {agent_id}. Use a visible agent: {choices}."
        ))),
    }
}
