//! A Claude launch that names an effort level starts with Claude's Ultracode switch off.

use serde_json::Value;

use crate::session_chat_options::{launch_command_selection, SessionChatOptionAgent};

/// The levels on Claude's effort rail. Ultracode is not one of them: `--effort ultracode` turns
/// the switch on and stays as it is.
const CLAUDE_RAIL_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// CDXC:AgentProviders 2026-11-20 WHY:
/// GitHub #694: "The model and effort chosen for New session aren't what the session starts with." Claude Code 2.1.284 made Ultracode a switch beside the effort level, and a user or project `settings.json` with `"ultracode": true` turns it on in every new session whatever `--effort` says: `claude --model sonnet --effort high` started as "high · ultracode" (Claude Code 2.1.329, checked with that key in a throwaway project's `.claude/settings.json`). In Ghostex Ultracode is the level after Max, so picking High means High with the switch off, and that session ran dynamic workflows on every task at about 3.5 times the cost. A Claude launch or resume whose command names a rail level gets `"ultracode": false` in its one `--settings` JSON, which outranks the settings files; `/effort` and the chat card can still turn it on in that session (checked: Tab then `s` answered "Ultracode on (this session only)"). A command with no `--effort`, or `--effort ultracode`, leaves Claude's own settings in charge, and so does one naming its own `--settings` file.
/// SEE-ALSO: server/src/session_chat_claude_effort_slider.rs (the chat's own Ultracode switch), server/src/agents/home_secret_folders.rs (`with_claude_settings`).
pub(crate) fn with_claude_launch_ultracode(family: Option<&str>, command: &str) -> String {
    if family != Some("claude") {
        return command.to_string();
    }
    let rail_level = launch_command_selection(SessionChatOptionAgent::Claude, command)
        .and_then(|selection| selection.effort)
        .is_some_and(|effort| CLAUDE_RAIL_LEVELS.contains(&effort.value.as_str()));
    if !rail_level {
        return command.to_string();
    }
    super::home_secret_folders::with_claude_settings(command, |settings| {
        settings.insert("ultracode".to_string(), Value::Bool(false));
        Some(())
    })
    .unwrap_or_else(|| command.to_string())
}
