//! Whether the CLI an agent launch would run is missing on this computer.
//!
//! CDXC:AgentLauncher 2026-11-21 DECISION:
//! User (Q4370 "check first", Q4369 "both", Q4371 "installed tabs"): before any agent starts,
//! Ghostex checks its CLI is installed and refuses the launch with a notice naming the agent
//! (Install, or pick another agent) instead of typing a command the shell can't find; Ask Ghostex
//! AI only picks an installed agent, and the chat's model picker shows tabs for installed agents
//! only. A Windows user's first chat started on Codex, which they never installed, and PowerShell
//! failed with "could not run the agent's startup command".
//! WHY: the answer is only "missing" when Ghostex knows what the launch runs: a built-in agent
//! whose command starts with its own CLI. A custom command (a wrapper, an alias, a path, an
//! environment prefix) runs something this check can't see, so it is never refused here. An
//! install-on-demand launcher counts as installed, because starting it installs the agent.
//! SEE-ALSO: `launch_plan.rs` (`create_agent_session_params_for_project`, the refusal),
//! `chief/model.rs` (`default_agent`), `session_chat_read.rs` (the picker's tab list),
//! `apps/desktop/src/app/gx_store/create/agent.rs` (the desktop's notice).

use std::path::Path;

use serde_json::Value;

use crate::agent_hooks::probing::{agent_cli_presence, CliPresence};
use crate::domain::DomainStateError;

/// The error code a launch refused for a missing CLI carries; clients show their Install / pick
/// another agent notice on it.
pub(crate) const AGENT_NOT_INSTALLED_CODE: &str = "agentNotInstalled";

/// The CLI `command` starts with, without quotes or a Windows `.cmd`/`.exe` suffix. `None` for an
/// environment prefix (`FOO=1 claude`) or an empty command.
fn command_program(command: &str) -> Option<&str> {
    let first = command.split_whitespace().next()?;
    let first = first.trim_matches(|c| c == '"' || c == '\'');
    if first.is_empty() || first.contains('=') {
        return None;
    }
    let lower = first.to_ascii_lowercase();
    for suffix in [".cmd", ".exe", ".ps1", ".bat"] {
        if lower.ends_with(suffix) {
            return Some(&first[..first.len() - suffix.len()]);
        }
    }
    Some(first)
}

/// Whether a launch of agent family `family` running `command` (the agent's own command when
/// `None`) would find no CLI on this computer.
pub(crate) fn launch_cli_missing(family: &str, command: Option<&str>, home_dir: &Path) -> bool {
    let Some(definition) = crate::agent_cli::catalog::CATALOG
        .iter()
        .find(|definition| definition.agent_id == family)
    else {
        return false;
    };
    let command = command
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .or_else(|| super::default_agent_command(family));
    let Some(program) = command.and_then(command_program) else {
        return false;
    };
    if program != definition.binary {
        return false;
    }
    agent_cli_presence(family, &definition.binary, home_dir, false) == CliPresence::Missing
}

/// [`launch_cli_missing`] for a launcher row of `/api/readSidebarHud` (`icon`, `agentId`,
/// `command`): its family is the built-in agent its icon names.
pub(crate) fn launcher_row_cli_missing(row: &Value, home_dir: &Path) -> bool {
    let text = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    let Some(family) = text("icon")
        .and_then(super::default_agent_icon_to_id)
        .or_else(|| text("agentId").and_then(super::default_agent_icon_to_id))
    else {
        return false;
    };
    launch_cli_missing(family, text("command"), home_dir)
}

/// The home folder agent CLIs are looked for under: the one hook status reads (an isolated
/// `GHOSTEX_HOME` profile's own folder, else the user's), so this check and Settings > Agents agree.
pub(crate) fn agent_probe_home() -> std::path::PathBuf {
    crate::paths::get_gxserver_paths(None)
        .agent_config_home_dir()
        .to_path_buf()
}

/// The refusal of a launch whose CLI is missing: says which agent, and what to do.
pub(crate) fn agent_not_installed_error(agent_name: &str) -> DomainStateError {
    DomainStateError {
        code: AGENT_NOT_INSTALLED_CODE,
        message: format!(
            "{agent_name} isn't installed on this computer. Install it in Settings > Agents, or pick another agent."
        ),
    }
}
