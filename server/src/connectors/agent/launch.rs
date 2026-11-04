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

/// The variables the `ghostex connectors mcp` process reads to learn its session and reach
/// gxserver.
const MCP_SESSION_ENV: [&str; 4] = [
    "GHOSTEX_SESSION_ID",
    "GHOSTEX_GLOBAL_SESSION_REF",
    "GHOSTEX_GXSERVER_BASE_URL",
    "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
];

/// The `ghostex` CLI shipped beside this gxserver, else `ghostex` on PATH.
fn cli_executable() -> String {
    let sibling = std::env::current_exe().ok().map(|gxserver| {
        let gxserver = fs::canonicalize(&gxserver).unwrap_or(gxserver);
        crate::ghostex_cli::launchers::package_bin_dir(&gxserver)
            .unwrap_or_default()
            .join(if cfg!(windows) {
                "ghostex.exe"
            } else {
                "ghostex"
            })
    });
    sibling
        .filter(|cli| crate::ghostex_cli::launchers::is_executable_file_sync(cli))
        .map(|cli| cli.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ghostex".to_string())
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
    let paths = get_gxserver_paths(None);
    if !super::super::connectors_enabled(&paths) {
        return command.to_string();
    }
    let command = command.trim_end();
    match family.as_str() {
        "claude" => {
            let config = mcp_config_file(&paths);
            if write_mcp_config(&config).is_err() {
                return command.to_string();
            }
            format!(
                "{command} --mcp-config {} --allowedTools mcp__{CONNECTORS_MCP_SERVER_NAME}",
                quote_shell_arg(&config.to_string_lossy())
            )
        }
        _ => {
            let setting = |key: &str, value: String| {
                format!(
                    "-c {}",
                    quote_shell_arg(&format!(
                        "mcp_servers.{CONNECTORS_MCP_SERVER_NAME}.{key}={value}"
                    ))
                )
            };
            let json_text = |value: Value| serde_json::to_string(&value).unwrap_or_default();
            format!(
                "{command} {} {} {} {}",
                setting("command", json_text(json!(cli_executable()))),
                setting("args", json_text(json!(MCP_ARGS))),
                setting("env_vars", json_text(json!(MCP_SESSION_ENV))),
                setting("tool_timeout_sec", "1900".to_string()),
            )
        }
    }
}
