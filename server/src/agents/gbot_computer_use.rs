//! A GBot chat whose GBot may not use the computer's screen and apps (GBot Settings > Computer,
//! server/src/gbots/computer/) is launched without the tools that drive them: cua-driver, SpaceO
//! and Codex's own Computer Use.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! The security review of M12 (finding 7): "computer use" only added a paragraph to the brief, so with it off a GBot chat at Full could still drive the screen. What each agent can be held to: Claude Code's deny rules keep it from those MCP servers' tools and from running their commands through its Bash tool (a prefix rule, so a command that hides the name, such as one inside a script, still runs); Codex turns those MCP servers off for this launch (only the ones its config names, since naming an unknown one stops Codex from starting) but has no rule for a shell command, so a Codex GBot at Full can still start their command-line tools from a shell. GBot Settings says so. The setting is read from the chat's own record (`computerUse` on `runtimeSettings.gbot`, or `gbotHome` on a computer the GBot was given), stamped when the chat is opened, so a change applies to the GBot's next chats.
//! SEE-ALSO: server/src/agents/home_secret_folders.rs (the same two launch hooks), server/src/gbots/session.rs and server/src/gbots/computer/remote_chat.rs (where `computerUse` is stamped).

use serde_json::{json, Map, Value};

use super::quote_shell_arg;

/// The MCP servers that drive the screen, by the names their installers register.
const SCREEN_SERVERS: [&str; 3] = ["cua-driver", "spaceo", "computer-use"];
/// Their command-line tools.
const SCREEN_COMMANDS: [&str; 2] = ["cua-driver", "spaceo"];

/// For a GBot chat, whether its GBot may use the screen and apps; `None` for any other session.
pub(crate) fn gbot_computer_use(runtime_settings: Option<&Map<String, Value>>) -> Option<bool> {
    let runtime = runtime_settings?;
    let record = ["gbot", "gbotHome"]
        .iter()
        .find_map(|key| runtime.get(*key).filter(|value| value.is_object()))?;
    Some(record.get("computerUse").and_then(Value::as_bool) == Some(true))
}

/// `command` without the screen tools when it starts a GBot chat whose GBot may not use them.
pub(crate) fn with_gbot_computer_use(
    family: Option<&str>,
    runtime_settings: Option<&Map<String, Value>>,
    command: &str,
) -> String {
    if cfg!(test) || command.trim().is_empty() || gbot_computer_use(runtime_settings) != Some(false)
    {
        return command.to_string();
    }
    match family {
        Some("claude") => {
            let rules = SCREEN_SERVERS
                .iter()
                .map(|server| json!(format!("mcp__{server}")))
                .chain(
                    SCREEN_COMMANDS
                        .iter()
                        .map(|tool| json!(format!("Bash({tool}:*)"))),
                )
                .collect();
            super::home_secret_folders::with_claude_deny_rules(command, rules)
                .unwrap_or_else(|| command.to_string())
        }
        Some("codex") => {
            let configured = codex_configured_servers(command);
            let mut out = command.trim_end().to_string();
            for server in SCREEN_SERVERS
                .iter()
                .filter(|server| configured.iter().any(|name| name == *server))
            {
                out.push_str(" -c ");
                out.push_str(&quote_shell_arg(&format!(
                    "mcp_servers.{server}.enabled=false"
                )));
            }
            out
        }
        _ => command.to_string(),
    }
}

/// The MCP servers the Codex this command starts has configured: `[mcp_servers.<name>]` headings
/// in the config of its `CODEX_HOME` (the one the command sets, else gxserver's, else `~/.codex`).
fn codex_configured_servers(command: &str) -> Vec<String> {
    let home = command
        .split_whitespace()
        .find_map(|word| word.strip_prefix("CODEX_HOME="))
        .map(|value| value.trim_matches(['\'', '"']).to_string())
        .or_else(|| std::env::var("CODEX_HOME").ok())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| ghostex_paths::user_home_dir().join(".codex"));
    let Ok(text) = std::fs::read_to_string(home.join("config.toml")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let name = line
                .trim()
                .strip_prefix("[mcp_servers.")?
                .strip_suffix(']')?;
            (!name.contains('.')).then(|| name.trim_matches('"').to_string())
        })
        .collect()
}
