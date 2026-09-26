//! The agent launcher and the Actions: `agents`, `globalCommands` and
//! `commandsByProject`, from gxserver's `/api/readSidebarHud` of this computer and of every remote
//! machine (the app runtime's `createGpuiSidebarHudState`, deleted).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::keys::ProjectKey;

/// `createSidebarAgentButtons([], [])`: the default agents, which the launcher shows until this
/// computer's HUD has been read once. The hidden-by-default ones (Rovo Dev, CodeBuddy, Qoder) are
/// left out, as there. Order and ids are packages/shared/sidebar-agents.ts's.
const DEFAULT_AGENTS: [(&str, &str, &str, &str); 14] = [
    ("codex", "codex", "codex", "Codex"),
    ("claude", "claude", "claude", "Claude"),
    ("cursor", "cursor-agent", "cursor-cli", "Cursor CLI"),
    ("pi", "pi", "pi", "Pi Agent"),
    ("opencode", "opencode", "opencode", "OpenCode"),
    ("gemini", "gemini", "gemini", "Gemini"),
    ("copilot", "copilot", "copilot", "Copilot"),
    ("droid", "droid", "factory-droid", "Factory Droid"),
    ("grok", "grok", "grok-build", "Grok Build"),
    ("antigravity", "agy", "antigravity-cli", "Antigravity CLI"),
    ("amp", "amp", "amp-cli", "Amp CLI"),
    ("hermes-agent", "hermes", "hermes-agent", "Hermes Agent"),
    ("mastra", "mastracode", "mastra", "Mastra Code"),
    ("zcode", "zcode", "zcode", "ZCode"),
];

pub(crate) fn agents(sidebar_hud: Option<&Value>) -> Value {
    match sidebar_hud {
        Some(hud) => hud
            .get("agents")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
        None => Value::Array(
            DEFAULT_AGENTS
                .iter()
                .map(|(agent_id, command, icon, name)| {
                    let mut agent = Map::new();
                    agent.insert("agentId".into(), Value::from(*agent_id));
                    agent.insert("command".into(), Value::from(*command));
                    agent.insert("icon".into(), Value::from(*icon));
                    agent.insert("isDefault".into(), Value::Bool(true));
                    agent.insert("name".into(), Value::from(*name));
                    Value::Object(agent)
                })
                .collect(),
        ),
    }
}

/// `normalizeHudCommands`: a daemon older than the app drops `showOnProjectRow`, so it is made a
/// real boolean at the boundary.
fn normalize_commands(commands: Option<&Value>) -> Value {
    Value::Array(
        commands
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|command| {
                let mut command = command.clone();
                if let Some(object) = command.as_object_mut() {
                    let shown = object.get("showOnProjectRow") == Some(&Value::Bool(true));
                    object.insert("showOnProjectRow".into(), Value::Bool(shown));
                }
                command
            })
            .collect(),
    )
}

pub(crate) fn global_commands(sidebar_hud: Option<&Value>) -> Value {
    normalize_commands(sidebar_hud.and_then(|hud| hud.get("globalCommands")))
}

/// This computer's per-project Actions, then every remote machine's re-keyed under the
/// machine-scoped project id the sidebar rows use.
///
/// CDXC:RemoteMachines 2026-08-29:
/// A remote machine's per-project Actions arrive keyed by that machine's own project ids, which
/// mean nothing to this app on their own: two machines can hand out the same `P1` id. Re-key them
/// under the machine-scoped project id the sidebar rows already use, so a remote row resolves its
/// own Actions through exactly the same lookup a local row does. The remote machine's Global
/// Actions stay on that machine: `globalCommands` is a flat app-wide list here.
pub(crate) fn commands_by_project(
    sidebar_hud: Option<&Value>,
    remote_huds: &BTreeMap<String, Value>,
) -> Option<Value> {
    let local = sidebar_hud
        .and_then(|hud| hud.get("commandsByProject"))
        .and_then(Value::as_object);
    let mut remote = Map::new();
    for (machine_id, hud) in remote_huds {
        let Some(rows) = hud.get("commandsByProject").and_then(Value::as_object) else {
            continue;
        };
        for (project_id, commands) in rows {
            remote.insert(
                ProjectKey::remote(machine_id.as_str(), project_id.as_str())
                    .to_workspace_project_id(),
                normalize_commands(Some(commands)),
            );
        }
    }
    if local.is_none() && remote.is_empty() {
        return None;
    }
    let mut merged = Map::new();
    for (project_id, commands) in local.into_iter().flatten() {
        merged.insert(project_id.clone(), normalize_commands(Some(commands)));
    }
    merged.extend(remote);
    Some(Value::Object(merged))
}
