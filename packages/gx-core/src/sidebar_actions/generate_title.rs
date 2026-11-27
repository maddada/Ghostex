//! Generate Name: the `/api/generateSessionTitle` call a session's Rename dialog or its menu's
//! Generate Title makes, planned from the launcher agent the user picked.
//!
//! CDXC:SessionTitles 2026-09-15 WHY:
//! The picker returns a launcher configuration id, but title generation needs its CLI family. Passing a custom Claude id previously selected Codex flags and ran `claude --yolo exec ...`, which exits immediately. Keep the selected configuration's command so its account and arguments survive the family resolution.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/session_edits.rs (the caller),
//! packages/shared/ghostex-settings/session-title-generation.ts (deleted 2026-10-01) (the agents that can generate).

use serde_json::{json, Map, Value};

use crate::session_create::{default_agent_id_for_icon, resolve_sidebar_agent};
use crate::sidebar_view::agents::default_agent_id;

/// `SESSION_TITLE_GENERATION_AGENT_OPTIONS` (no Antigravity since 2026-11-27, Q5212).
const TITLE_GENERATION_AGENTS: [&str; 7] =
    ["codex", "cursor", "claude", "grok", "pi", "empryo", "custom"];

/// Why a title command that runs agy doesn't run, shown in Settings and used by gxserver.
pub const TITLE_COMMAND_AGY_REFUSAL: &str = "Ghostex doesn't run agy for titles, since it uses Antigravity only through agy over ACP. Titles use Claude instead.";

/// Whether a title command runs agy anywhere in it (`agy -p`, `/opt/bin/agy`, `agy.exe`,
/// `sh -c 'agy -p'`, `FOO=1 agy`).
///
/// CDXC:AgentProviders 2026-11-27 DECISION:
/// User (Q5229, "Refuse it"): the Custom option of Title Generation Agent ran whatever the user
/// typed, so `agy -p ...` there still ran agy outside ACP. A title command that runs agy is
/// refused with a short reason and titles fall back to Claude. Every word of the command is
/// checked, not only the first, so a wrapper (`sh -c`, `env`) doesn't hide it.
pub fn title_command_runs_agy(command: &str) -> bool {
    command
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ';' | '|' | '&' | '(' | ')' | '`' | '\'' | '"' | '$' | '=' | '{' | '}' | '<' | '>'
                )
        })
        .filter(|word| !word.is_empty())
        .any(|word| {
            let name = word
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(word)
                .to_ascii_lowercase();
            let stem = [".exe", ".cmd", ".bat", ".ps1"]
                .iter()
                .find_map(|extension| name.strip_suffix(extension))
                .unwrap_or(&name);
            stem == "agy"
        })
}

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

#[cfg(test)]
mod agy_title_command_tests {
    use super::title_command_runs_agy;

    #[test]
    fn a_title_command_that_runs_agy_is_found() {
        for command in [
            "agy",
            "agy -p --model x",
            "/opt/homebrew/bin/agy -p",
            "C:\\Users\\sam\\AppData\\Local\\agy\\bin\\agy.exe -p",
            "& 'C:\\agy\\bin\\agy.cmd' -p",
            "FOO=1 agy -p",
            "env AGY_X=1 agy",
            "sh -c 'agy -p'",
            "cat | AGY -p",
        ] {
            assert!(title_command_runs_agy(command), "{command}");
        }
    }

    #[test]
    fn other_title_commands_run() {
        for command in ["claude -p", "my-titler --short", "agyle", "python3 titles.py", ""] {
            assert!(!title_command_runs_agy(command), "{command}");
        }
    }
}
