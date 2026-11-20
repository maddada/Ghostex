//! The SpaceO install, update, reinstall or uninstall job the desktop app runs in the background
//! (install_job.rs runs it), what each button runs, and the step that finishes an install.

use crate::app::helpers::*;

pub(crate) static SPACEO_JOB: GpuiInstallJob = GpuiInstallJob::new("SpaceO");

/// CDXC:ManagedTools 2026-10-01 DECISION:
/// User: integrate SpaceO (github.com/ParthJadhav/SpaceO) "exactly like Cua Driver was": Settings > Integrations installs, updates, reinstalls and uninstalls it with one click through its official installer, run as the same background job as Trycua's, then installs the Ghostex SpaceO skill, and shows it only where it can work (Apple Silicon Macs on macOS 14 or later).
/// CDXC:ManagedTools 2026-10-01 WHY:
/// `--no-clients` keeps the installer from registering SpaceO's MCP server with Claude Code, Codex, Cursor and Claude Desktop, matching Trycua's CLI-only skill; agents use the `spaceo` CLI through `$ghostex-spaceo`.
pub(crate) const GPUI_SPACEO_INSTALL_COMMAND: &str = "curl -fsSL https://raw.githubusercontent.com/ParthJadhav/SpaceO/main/install.sh | bash -s -- --no-clients";
/// The installer stops the daemon (ending its sessions) and removes the LaunchAgent only with
/// `--yes` when no terminal can ask; it never resets the Accessibility and Screen Recording grants.
pub(crate) const GPUI_SPACEO_UNINSTALL_COMMAND: &str = "curl -fsSL https://raw.githubusercontent.com/ParthJadhav/SpaceO/main/install.sh | bash -s -- --uninstall --yes";
/// CDXC:ManagedTools 2026-10-01 WHY:
/// SpaceO's daemon does the clicking and capturing, and a daemon started on demand takes the macOS grants of whatever started it (an agent's terminal, or Ghostex itself when started from this job, whose output pipes it would then hold open). Running it as SpaceO's own `com.spaceo.daemon` LaunchAgent gives it one stable identity to grant once, the role CuaDriver.app plays for Trycua. When the LaunchAgent already exists, an update drains the daemon (`daemon restart --operator` waits for running sessions to end) and launchd starts the new build; otherwise any hand-started daemon is stopped before the LaunchAgent takes its socket.
const GPUI_SPACEO_SERVICE_COMMAND: &str = "spaceo=\"$HOME/.local/bin/spaceo\"; if [ -f \"$HOME/Library/LaunchAgents/com.spaceo.daemon.plist\" ]; then \"$spaceo\" daemon restart --operator; else \"$spaceo\" daemon stop --operator >/dev/null 2>&1 || true; \"$spaceo\" daemon install --yes; fi";

pub(crate) const GPUI_SPACEO_INSTALL_RUNNING_MESSAGE: &str =
    "The official SpaceO installer is running in the background. Settings shows its progress.";
pub(crate) const GPUI_SPACEO_UPDATE_RUNNING_MESSAGE: &str = "The official SpaceO installer is updating SpaceO in the background; SpaceO restarts once agents using it finish. Settings shows its progress.";
pub(crate) const GPUI_SPACEO_REINSTALL_RUNNING_MESSAGE: &str = "The official SpaceO installer is reinstalling the latest release in the background. Settings shows its progress.";
pub(crate) const GPUI_SPACEO_UNINSTALL_RUNNING_MESSAGE: &str =
    "The official SpaceO uninstaller is running in the background. Settings shows its progress.";

/// Adds the SpaceO job and install plan to a `ghostexCliStatus` payload.
pub(crate) fn gpui_decorate_spaceo_status(payload: &mut serde_json::Value) {
    if !gpui_spaceo_supported() {
        return;
    }
    payload["spaceoJob"] = SPACEO_JOB.json();
    payload["spaceoInstallPlan"] = serde_json::json!(gpui_spaceo_install_plan());
}

/// Tooltip for Install, Update and Reinstall: exactly what one click runs.
pub(crate) fn gpui_spaceo_install_plan() -> String {
    format!(
        "Runs SpaceO's official installer from GitHub ({GPUI_SPACEO_INSTALL_COMMAND}) in the background. It checks the release's signature and notarization, puts the spaceo command in ~/.local/bin and SpaceO Viewer in ~/Applications, and adds ~/.local/bin to your PATH. Ghostex then keeps SpaceO running in the background and installs the Ghostex SpaceO skill. No password needed; afterwards grant Accessibility and Screen Recording from their rows in Settings."
    )
}

fn gpui_spaceo_installer_script() -> String {
    format!("{GPUI_SPACEO_INSTALL_COMMAND} && {{ {GPUI_SPACEO_SERVICE_COMMAND}; }}")
}

/// Install when SpaceO is missing, otherwise update it to the latest release.
pub(crate) fn gpui_spaceo_command_action() -> GpuiInstallJobAction {
    if gpui_spaceo_executable_path().is_some() {
        return GpuiInstallJobAction {
            script: gpui_spaceo_installer_script(),
            operation: "update",
            running_message: GPUI_SPACEO_UPDATE_RUNNING_MESSAGE,
            toast_title: "Updating SpaceO",
        };
    }
    GpuiInstallJobAction {
        script: gpui_spaceo_installer_script(),
        operation: "install",
        running_message: GPUI_SPACEO_INSTALL_RUNNING_MESSAGE,
        toast_title: "Installing SpaceO",
    }
}

pub(crate) fn gpui_spaceo_reinstall_command_action() -> GpuiInstallJobAction {
    GpuiInstallJobAction {
        script: gpui_spaceo_installer_script(),
        operation: "reinstall",
        running_message: GPUI_SPACEO_REINSTALL_RUNNING_MESSAGE,
        toast_title: "Reinstalling SpaceO",
    }
}

pub(crate) fn gpui_spaceo_uninstall_command_action() -> GpuiInstallJobAction {
    GpuiInstallJobAction {
        script: GPUI_SPACEO_UNINSTALL_COMMAND.to_string(),
        operation: "uninstall",
        running_message: GPUI_SPACEO_UNINSTALL_RUNNING_MESSAGE,
        toast_title: "Uninstalling SpaceO",
    }
}

/// After the installer exits: say SpaceO is ready, or report why setup stopped.
pub(crate) fn gpui_finish_spaceo_setup(
    installed: bool,
    was_update: bool,
) -> Result<String, String> {
    if !installed {
        return Err(
            if was_update {
                "The SpaceO update did not finish successfully. Settings shows its last output; plugin status was refreshed."
            } else {
                "The SpaceO installer did not finish successfully. Settings shows its last output; plugin status was refreshed."
            }
            .to_string(),
        );
    }
    // CDXC:AgentSkills 2026-11-20: the Ghostex SpaceO skill is retired (Q4262); agents read `ghostex spaceo guide`, so nothing is installed after SpaceO.
    Ok(if was_update {
        format!(
            "SpaceO is up to date.{}",
            gpui_spaceo_permissions_lost_after_update()
        )
    } else {
        "SpaceO installed. Grant its Accessibility and Screen Recording permissions in Settings."
            .to_string()
    })
}

/// CDXC:ManagedTools 2026-10-14 WHY:
/// macOS keeps SpaceO's grants across updates because they are tied to its signing identity (`dev.spaceo.cli` from SpaceO's team), not to the file, and the official installer refuses any build signed otherwise. An update is still re-checked from a fresh SpaceO process after its daemon restarted, so the result names any permission the updated SpaceO still lacks (one a release with a new identity lost, or one never granted) instead of leaving agents failing.
fn gpui_spaceo_permissions_lost_after_update() -> String {
    let Some(path) = gpui_spaceo_executable_path() else {
        return String::new();
    };
    let report = gpui_spaceo_permission_report(&path);
    let lost: Vec<&str> = [
        ("Accessibility", report.accessibility),
        ("Screen Recording", report.screen_recording),
    ]
    .into_iter()
    .filter(|(_, state)| *state == GpuiToolPermissionState::NotGranted)
    .map(|(name, _)| name)
    .collect();
    if lost.is_empty() {
        String::new()
    } else {
        format!(
            " The updated SpaceO still needs {}: grant it in Settings > Integrations.",
            lost.join(" and ")
        )
    }
}
