//! Run in terminal beside a failed install: the command of an install Ghostex runs itself with one
//! click (Fast Computer & Browser Use, SpaceO, Hex and the agent CLIs), offered only once that
//! install failed, so the user can run it in a Main Hub terminal and answer what it asks.
//!
//! CDXC:ManagedTools 2026-11-25 DECISION: The user chose "Yes, as a fallback after a failed install" for the install commands Ghostex already runs with a button: Run in terminal shows beside a failed install only, never while the install works. gxserver advertises the id on the failed job (`job.runInTerminal`), so a computer on an older gxserver shows no button, and resolves the text itself: a tool's official installer line (the one its install plan names), or the command the failed agent CLI job ran. Not on Windows, where these jobs run commands for PowerShell or cmd that a Main Hub terminal may not run as they are.
//! SEE-ALSO: registry.rs (the id table), apps/desktop/src/app/window/settings_modal/tabs/integrations.rs and integrations/tools.rs, tabs/agents/cli.rs, onboarding/agent_cli.rs, tabs/voice_dictate/hex.rs (the desktop), apps/mobile/app/src/machines/machineToolsView.ts (the phone).
use serde_json::Value;

use super::registry::ShownCommand;
use crate::domain::DomainStateError;
use crate::managed_tools::{
    desktop_control,
    platform::{self, Os},
    tools::ToolId,
};
use crate::server::AppState;

const TOOL_INSTALL: &str = "toolInstall:";
const AGENT_CLI: &str = "agentCli:";

/// Hex's own installer as a command line (apps/desktop/src/app/window/settings_modal/tabs/voice_dictate/hex_install_plan.rs shows the same).
const HEX_INSTALL_MAC: &str = "brew install --cask anomalyco/tap/hex";
const HEX_INSTALL_LINUX: &str = "curl -fsSL https://anoma.ly/hex/install-linux.sh | sh";

/// The official install line of a tool that offers Run in terminal after a failed install.
fn tool_install_command(tool: ToolId) -> Option<&'static str> {
    match (tool, platform::os()) {
        (_, Os::Windows) => None,
        (ToolId::Trycua, _) => Some(desktop_control::TRYCUA_INSTALL),
        (ToolId::Spaceo, Os::Mac) => Some(desktop_control::SPACEO_INSTALL),
        (ToolId::Hex, Os::Mac) => Some(HEX_INSTALL_MAC),
        (ToolId::Hex, Os::Linux) => Some(HEX_INSTALL_LINUX),
        _ => None,
    }
}

/// The id a tool's failed install or update carries (`job.runInTerminal`), `None` otherwise.
pub(crate) fn failed_tool_job_id(tool: ToolId, job: &Value) -> Option<String> {
    let failed_install = job["status"] == "failed"
        && matches!(
            job["operation"].as_str(),
            Some("install" | "update" | "reinstall")
        );
    (failed_install && tool_install_command(tool).is_some())
        .then(|| format!("{TOOL_INSTALL}{}", tool.id()))
}

/// The id a failed agent CLI install or update carries (`job.runInTerminal`), `None` otherwise.
pub(crate) fn failed_agent_cli_job_id(agent_id: &str, job: &Value) -> Option<String> {
    let failed_install = job["status"] == "failed"
        && matches!(job["operation"].as_str(), Some("install" | "update"))
        && job["command"]
            .as_str()
            .is_some_and(|command| !command.trim().is_empty());
    (failed_install && platform::os() != Os::Windows).then(|| format!("{AGENT_CLI}{agent_id}"))
}

/// Resolves a failed-install id, or `None` when `id` is not one.
pub(crate) fn resolve(
    state: &AppState,
    id: &str,
) -> Option<Result<ShownCommand, DomainStateError>> {
    if let Some(tool) = id.strip_prefix(TOOL_INSTALL) {
        let resolved = ToolId::parse(tool)
            .and_then(|tool| Some((tool, tool_install_command(tool)?)))
            .map(|(tool, command)| ShownCommand {
                title: format!("Install {}", tool.label()),
                command: command.to_string(),
            })
            .ok_or_else(|| unknown(id));
        return Some(resolved);
    }
    let agent_id = id.strip_prefix(AGENT_CLI)?;
    let home = state.paths.agent_config_home_dir();
    let resolved = crate::agent_cli::endpoint::failed_job_command(home, agent_id)
        .filter(|_| platform::os() != Os::Windows)
        .map(|(operation, command)| {
            let name = crate::sidebar_hud::default_agent_name(agent_id).unwrap_or(agent_id);
            ShownCommand {
                title: format!(
                    "{} {name}",
                    if operation == "update" {
                        "Update"
                    } else {
                        "Install"
                    }
                ),
                command,
            }
        })
        .ok_or_else(|| unknown(id));
    Some(resolved)
}

fn unknown(id: &str) -> DomainStateError {
    DomainStateError::bad_request(format!("Ghostex has no command called \"{id}\" to run."))
}
