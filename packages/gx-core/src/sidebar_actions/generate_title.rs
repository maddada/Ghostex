//! Generate Name: the `/api/generateSessionTitle` call a session's Rename dialog or its menu's
//! Generate Title makes, planned from the launcher agent the user picked.
//!
//! CDXC:SessionTitles 2026-09-15 WHY:
//! The picker returns a launcher configuration id, but title generation needs its CLI family. Passing a custom Claude id previously selected Codex flags and ran `claude --yolo exec ...`, which exits immediately. Keep the selected configuration's command so its account and arguments survive the family resolution.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/session_edits.rs (the caller),
//! packages/shared/ghostex-settings/session-title-generation.ts (the agents that can generate).

use serde_json::{json, Map, Value};

use crate::session_create::{default_agent_id_for_icon, resolve_sidebar_agent};
use crate::sidebar_view::agents::default_agent_id;

/// `SESSION_TITLE_GENERATION_AGENT_OPTIONS`.
const TITLE_GENERATION_AGENTS: [&str; 7] = [
    "codex",
    "cursor",
    "claude",
    "grok",
    "pi",
    "antigravity",
    "custom",
];

/// The parameters of `/api/generateSessionTitle`, or the sentence the user is told when the
/// picked agent cannot generate a name.
pub fn plan_generate_session_title(
    hud: Option<&Value>,
    agent_id: Option<&str>,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<Value, &'static str> {
    let agent = resolve_sidebar_agent(hud, agent_id.unwrap_or_default());
    let command = agent
        .as_ref()
        .and_then(|agent| agent.command.as_deref())
        .map(str::trim)
        .filter(|command| !command.is_empty());
    let family = agent.as_ref().and_then(|agent| {
        default_agent_id(&agent.agent_id)
            .or_else(|| default_agent_id_for_icon(agent.icon.as_deref()))
    });
    if agent_id.is_some_and(|agent_id| !agent_id.is_empty())
        && (command.is_none()
            || !family.is_some_and(|family| TITLE_GENERATION_AGENTS.contains(&family)))
    {
        return Err("Choose a configured agent that supports name generation.");
    }
    let mut params = Map::new();
    if let Some(family) = family {
        params.insert("agentId".into(), json!(family));
    }
    if let Some(command) = command {
        params.insert("command".into(), json!(command));
    }
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    params.insert("text".into(), json!(text));
    Ok(Value::Object(params))
}
