//! How a Claude or Codex session reaches its workspace's connector tools: the `ghostex connectors
//! mcp` server, added as launch flags while Connectors is on.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! The flags are added when the launch or resume command is resolved (like Marked handovers), not saved in the session's base command, so turning Connectors on or off reaches each session the next time it starts or resumes. Every Claude and Codex session gets the server whether or not its workspace has connectors yet, because the tool list is read live: connecting an app or turning Executor on reaches sessions already running. Claude's `--allowedTools` lets the tools run without Claude's own prompt, since Ghostex's card asks before anything changes; it and `--mcp-config` both take several values, so they go last, where only flags follow. Codex stops an MCP call after 60 seconds by default and a card may wait 30 minutes, so its server gets a longer `tool_timeout_sec`; Codex also starts MCP servers with a short environment allowlist, so the session variables are passed through with `env_vars` or the server never learns which session it serves.
//! SEE-ALSO: server/src/agents/launch_plan.rs (`resolve_agent_launch_command` calls this), server/src/agents/marked_handovers.rs (the same pattern).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::agents::{
    default_agent_icon_to_id, default_agent_session_title_name,
    infer_agent_id_from_command_executable, quote_shell_arg,
};
use crate::paths::{get_gxserver_paths, GxserverPaths};

/// The MCP server's name in Claude and Codex: its tools read `mcp__ghostex-connectors__<tool>`.
pub(crate) const CONNECTORS_MCP_SERVER_NAME: &str = "ghostex-connectors";

/// The session a GBots bot's turn serves (`server:project:session`), which the bot's wrapper sets
/// for the connector server alone: the bot's own `ghostex` CLI keeps finding its caller the GBots
/// way (ghostex_cli/agents/identity.rs `gbot_caller`).
pub(crate) const CONNECTORS_SESSION_REF_ENV: &str = "GHOSTEX_CONNECTORS_SESSION_REF";

/// The variables the `ghostex connectors mcp` process reads to learn its session and reach
/// gxserver.
const MCP_SESSION_ENV: [&str; 5] = [
    "GHOSTEX_SESSION_ID",
    "GHOSTEX_GLOBAL_SESSION_REF",
    CONNECTORS_SESSION_REF_ENV,
    "GHOSTEX_GXSERVER_BASE_URL",
    "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
];

/// The `ghostex` CLI shipped beside this gxserver, else `ghostex` on PATH.
fn cli_executable() -> String {
    crate::ghostex_cli::launchers::sibling_cli_executable()
}

const MCP_ARGS: [&str; 2] = ["connectors", "mcp"];

fn mcp_config_file(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("connectors").join("mcp.json")
}

/// Writes Claude's `--mcp-config` file when it is missing or out of date.
fn write_mcp_config(path: &Path) -> std::io::Result<()> {
    let config = json!({
        "mcpServers": {
            CONNECTORS_MCP_SERVER_NAME: { "command": cli_executable(), "args": MCP_ARGS },
        }
    });
    let text = serde_json::to_string_pretty(&config).unwrap_or_default();
    if fs::read_to_string(path).ok().as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

fn family_of(agent_id: &str, icon: Option<&str>, command: &str) -> Option<String> {
    default_agent_session_title_name(agent_id)
        .map(|_| agent_id.to_string())
        .or_else(|| icon.and_then(default_agent_icon_to_id).map(str::to_string))
        .or_else(|| infer_agent_id_from_command_executable(command))
}

/// `command` with the connector tools' flags for Claude and Codex while Connectors is on; every
/// other command unchanged.
pub(crate) fn with_connector_tools(agent_id: &str, icon: Option<&str>, command: &str) -> String {
    // Unit tests assert exact commands, which must not depend on the user's settings.
    if cfg!(test) || command.trim().is_empty() || command.contains(CONNECTORS_MCP_SERVER_NAME) {
        return command.to_string();
    }
    let Some(family) = family_of(agent_id, icon, command) else {
        return command.to_string();
    };
    if !matches!(family.as_str(), "claude" | "codex") {
        return command.to_string();
    }
    match connector_tool_args(&family) {
        Some(args) => format!(
            "{} {}",
            command.trim_end(),
            args.iter()
                .map(|arg| {
                    let plain = arg
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
                    if plain {
                        arg.clone()
                    } else {
                        quote_shell_arg(arg)
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
        None => command.to_string(),
    }
}

/// The arguments that give a `claude` or `codex` (`family`) the connector tools while Connectors
/// is on, one argument per item; `None` for any other family, while Connectors is off, or when
/// Claude's config file can't be written. GBots bots' turns get them this way
/// (gbots/bot_tools.rs), sessions through [`with_connector_tools`].
pub(crate) fn connector_tool_args(family: &str) -> Option<Vec<String>> {
    if !matches!(family, "claude" | "codex") {
        return None;
    }
    let paths = get_gxserver_paths(None);
    if !super::super::connectors_enabled(&paths) {
        return None;
    }
    match family {
        "claude" => {
            let config = mcp_config_file(&paths);
            write_mcp_config(&config).ok()?;
            Some(vec![
                "--mcp-config".to_string(),
                config.to_string_lossy().into_owned(),
                "--allowedTools".to_string(),
                format!("mcp__{CONNECTORS_MCP_SERVER_NAME}"),
            ])
        }
        _ => {
            let json_text = |value: Value| serde_json::to_string(&value).unwrap_or_default();
            let mut args = Vec::new();
            for (key, value) in [
                ("command", json_text(json!(cli_executable()))),
                ("args", json_text(json!(MCP_ARGS))),
                ("env_vars", json_text(json!(MCP_SESSION_ENV))),
                ("tool_timeout_sec", "1900".to_string()),
            ] {
                args.push("-c".to_string());
                args.push(format!(
                    "mcp_servers.{CONNECTORS_MCP_SERVER_NAME}.{key}={value}"
                ));
            }
            Some(args)
        }
    }
}
