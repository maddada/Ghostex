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

use crate::agent_hooks::probing::{
    CliPresence, ShellCommandProbe, agent_cli_presence, cached_shell_command_found,
    probe_command_in_user_shell, refresh_cli_environment,
};
use crate::domain::DomainStateError;

/// The error code a launch refused for a missing CLI carries; clients show their Install / pick
/// another agent notice on it.
pub(crate) const AGENT_NOT_INSTALLED_CODE: &str = "agentNotInstalled";

#[cfg(test)]
thread_local! {
    static TEST_CHECK_LAUNCH_CLI: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether `create_agent_session_params_for_project` enforces the host computer's CLI presence
/// check. Under `cargo test`, existing unit tests build launch plans without Claude or Codex
/// installed on the runner; tests exercising the check call `launch_cli_missing` directly.
pub(crate) fn should_check_launch_cli() -> bool {
    #[cfg(test)]
    {
        TEST_CHECK_LAUNCH_CLI.with(|enabled| enabled.get())
    }
    #[cfg(not(test))]
    {
        true
    }
}

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

/// The built-in CLI binary `family` running `command` would invoke, or `None` for a custom
/// command (wrapper, alias, explicit path, env prefix) that Ghostex never refuses.
fn builtin_launch_binary(family: &str, command: Option<&str>) -> Option<&'static str> {
    let definition = crate::agent_cli::catalog::CATALOG
        .iter()
        .find(|definition| definition.agent_id == family)?;
    let command = command
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .or_else(|| super::default_agent_command(family));
    let program = command.and_then(command_program)?;
    (program == definition.binary).then_some(definition.binary.as_str())
}

/// Cached missing-CLI check for UI list filtering (model picker tabs, draft agent menu, Chief
/// status). Never spawns a fresh login-shell probe on every poll.
pub(crate) fn launch_cli_missing_cached(
    family: &str,
    command: Option<&str>,
    home_dir: &Path,
) -> bool {
    let Some(binary) = builtin_launch_binary(family, command) else {
        return false;
    };
    agent_cli_presence(family, binary, home_dir, false) == CliPresence::Missing
        && !cached_shell_command_found(binary, home_dir)
}

/// Whether a launch of agent family `family` running `command` (the agent's own command when
/// `None`) would find no CLI on this computer.
///
/// Self-heals before ever refusing (or moving an unstarted Ghostex AI chat off its agent):
/// 1. Fast cached check (PATH, install dirs, or recent positive shell probe).
/// 2. Refresh CLI environment (login-shell PATH + live registry PATH) and run fresh PATH/installDirs check.
/// 3. Probe the user's real shell (seeing PATH, aliases, functions, and PowerShell `$PROFILE`).
///    Only returns `true` when the shell cleanly completes and confirms the command is missing;
///    timeouts, spawn failures, and inconclusive results fail open so working agents always launch.
pub(crate) fn launch_cli_missing(family: &str, command: Option<&str>, home_dir: &Path) -> bool {
    let Some(binary) = builtin_launch_binary(family, command) else {
        return false;
    };
    if agent_cli_presence(family, binary, home_dir, false) != CliPresence::Missing
        || cached_shell_command_found(binary, home_dir)
    {
        return false;
    }
    refresh_cli_environment(home_dir);
    if agent_cli_presence(family, binary, home_dir, true) != CliPresence::Missing {
        return false;
    }
    match probe_command_in_user_shell(binary, home_dir) {
        ShellCommandProbe::Found | ShellCommandProbe::Inconclusive => false,
        ShellCommandProbe::Missing => true,
    }
}

fn launcher_row_target<'a>(row: &'a Value) -> Option<(&'static str, Option<&'a str>)> {
    let text = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    // Custom agents (`custom-...`) are never hidden or refused by the built-in CLI check.
    if text("agentId").is_some_and(|id| id.starts_with("custom-")) {
        return None;
    }
    let family = text("icon")
        .and_then(super::default_agent_icon_to_id)
        .or_else(|| text("agentId").and_then(super::default_agent_icon_to_id))?;
    Some((family, text("command")))
}

/// Cached [`launch_cli_missing_cached`] for a launcher row of `/api/readSidebarHud` (`icon`,
/// `agentId`, `command`): its family is the built-in agent its icon names.
pub(crate) fn launcher_row_cli_missing(row: &Value, home_dir: &Path) -> bool {
    let Some((family, command)) = launcher_row_target(row) else {
        return false;
    };
    launch_cli_missing_cached(family, command, home_dir)
}

/// Fresh [`launch_cli_missing`] for a launcher row, used before acting on a missing CLI (such as
/// moving an unstarted Ghostex AI chat off its agent).
pub(crate) fn launcher_row_cli_missing_fresh(row: &Value, home_dir: &Path) -> bool {
    let Some((family, command)) = launcher_row_target(row) else {
        return false;
    };
    launch_cli_missing(family, command, home_dir)
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
