use crate::app::helpers::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum GpuiGhostexCliSettingsAction {
    InstallGhostexCli,
    InstallBrowserControl,
    InstallBrowserUseSkill,
    InstallComputerUseSkill,
    InstallSpaceoSkill,
    InstallCliSkill,
    InstallAgentsOrchestrationSkill,
    InstallManageBeadsSkill,
    InstallGenerateTitleSkill,
    InstallMoveCodexSessionSkill,
    InstallHelpSkill,
    InstallAgentGuideSkill,
    InstallVisualsSkill,
    InstallSecretsSkill,
    InstallCuaDriverSkill,
    FinishDesktopControlSetup {
        driver_installed: bool,
        was_update: bool,
    },
    FinishTrycuaUninstall {
        succeeded: bool,
    },
    FinishSpaceoSetup {
        installed: bool,
        was_update: bool,
    },
    FinishSpaceoUninstall {
        succeeded: bool,
    },
    UninstallBundledAgentSkill(&'static str),
    UninstallBundledAgentSkills,
}

impl GpuiGhostexCliSettingsAction {
    pub(crate) fn action_id(self) -> &'static str {
        match self {
            Self::InstallGhostexCli => "installGhostexCli",
            Self::InstallBrowserControl => "installBrowserControl",
            Self::InstallBrowserUseSkill => "installBrowserUseSkill",
            Self::InstallComputerUseSkill => "installComputerUseSkill",
            Self::InstallSpaceoSkill => "installSpaceoSkill",
            Self::InstallCliSkill => "installCliSkill",
            Self::InstallAgentsOrchestrationSkill => "installAgentsOrchestrationSkill",
            Self::InstallManageBeadsSkill => "installManageBeadsSkill",
            Self::InstallGenerateTitleSkill => "installGenerateTitleSkill",
            Self::InstallMoveCodexSessionSkill => "installMoveCodexSessionSkill",
            Self::InstallHelpSkill => "installHelpSkill",
            Self::InstallAgentGuideSkill => "installAgentGuideSkill",
            Self::InstallVisualsSkill => "installVisualsSkill",
            Self::InstallSecretsSkill => "installSecretsSkill",
            Self::InstallCuaDriverSkill => "installCuaDriverSkill",
            Self::FinishDesktopControlSetup { .. } => "installCuaDriver",
            Self::FinishTrycuaUninstall { .. } => "uninstallCuaDriver",
            Self::FinishSpaceoSetup { .. } => "installSpaceo",
            Self::FinishSpaceoUninstall { .. } => "uninstallSpaceo",
            Self::UninstallBundledAgentSkill(_) => "uninstallBundledAgentSkill",
            Self::UninstallBundledAgentSkills => "uninstallBundledAgentSkills",
        }
    }

    pub(crate) fn success_toast_title(self) -> &'static str {
        match self {
            Self::InstallGhostexCli => "Ghostex CLI linked",
            Self::InstallBrowserControl => "Ghostex Embedded Browser Use installed",
            Self::InstallBrowserUseSkill => "Ghostex Browser Use installed",
            Self::InstallComputerUseSkill => "Ghostex Computer Use installed",
            Self::InstallSpaceoSkill => "Ghostex SpaceO installed",
            Self::InstallCliSkill => "Ghostex CLI skill installed",
            Self::InstallAgentsOrchestrationSkill => "Ghostex Agents installed",
            Self::InstallManageBeadsSkill => "Ghostex Manage Beads installed",
            Self::InstallGenerateTitleSkill => "Ghostex Auto Rename Session installed",
            Self::InstallMoveCodexSessionSkill => "Ghostex Move Codex Session installed",
            Self::InstallHelpSkill => "Ghostex Help installed",
            Self::InstallAgentGuideSkill => "Ghostex Agent Guide installed",
            Self::InstallVisualsSkill => "Ghostex Visuals installed",
            Self::InstallSecretsSkill => "Ghostex Secrets installed",
            Self::InstallCuaDriverSkill => "Cua Driver skill installed",
            Self::FinishDesktopControlSetup {
                was_update: true, ..
            } => "Fast Computer & Browser Use updated",
            Self::FinishDesktopControlSetup { .. } => "Desktop Control installed",
            Self::FinishTrycuaUninstall { .. } => "Fast Computer & Browser Use uninstalled",
            Self::FinishSpaceoSetup {
                was_update: true, ..
            } => "SpaceO updated",
            Self::FinishSpaceoSetup { .. } => "SpaceO installed",
            Self::FinishSpaceoUninstall { .. } => "SpaceO uninstalled",
            Self::UninstallBundledAgentSkill(_) => "Agent skill uninstalled",
            Self::UninstallBundledAgentSkills => "Bundled agent skills uninstalled",
        }
    }

    pub(crate) fn failure_toast_title(self) -> &'static str {
        match self {
            Self::InstallGhostexCli => "Ghostex CLI repair unavailable",
            Self::InstallBrowserControl => "Ghostex Embedded Browser Use install failed",
            Self::InstallBrowserUseSkill => "Ghostex Browser Use install failed",
            Self::InstallComputerUseSkill => "Ghostex Computer Use install failed",
            Self::InstallSpaceoSkill => "Ghostex SpaceO install failed",
            Self::InstallCliSkill => "Ghostex CLI skill install failed",
            Self::InstallAgentsOrchestrationSkill => "Ghostex Agents install failed",
            Self::InstallManageBeadsSkill => "Ghostex Manage Beads install failed",
            Self::InstallGenerateTitleSkill => "Ghostex Auto Rename Session install failed",
            Self::InstallMoveCodexSessionSkill => "Ghostex Move Codex Session install failed",
            Self::InstallHelpSkill => "Ghostex Help install failed",
            Self::InstallAgentGuideSkill => "Ghostex Agent Guide install failed",
            Self::InstallVisualsSkill => "Ghostex Visuals install failed",
            Self::InstallSecretsSkill => "Ghostex Secrets install failed",
            Self::InstallCuaDriverSkill => "Cua Driver skill install failed",
            Self::FinishDesktopControlSetup {
                was_update: true, ..
            } => "Fast Computer & Browser Use update failed",
            Self::FinishDesktopControlSetup { .. } => "Desktop Control setup incomplete",
            Self::FinishTrycuaUninstall { .. } => "Fast Computer & Browser Use uninstall failed",
            Self::FinishSpaceoSetup {
                was_update: true, ..
            } => "SpaceO update failed",
            Self::FinishSpaceoSetup { .. } => "SpaceO setup incomplete",
            Self::FinishSpaceoUninstall { .. } => "SpaceO uninstall failed",
            Self::UninstallBundledAgentSkill(_) => "Bundled agent skill uninstall failed",
            Self::UninstallBundledAgentSkills => "Bundled agent skill uninstall failed",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiGhostexCliActionResult {
    pub(crate) action_id: &'static str,
    pub(crate) available: bool,
    pub(crate) message: String,
    pub(crate) toast_level: &'static str,
    pub(crate) toast_title: &'static str,
}

impl GpuiGhostexCliActionResult {
    pub(crate) fn success(action: GpuiGhostexCliSettingsAction, message: String) -> Self {
        Self {
            action_id: action.action_id(),
            available: true,
            message,
            toast_level: "success",
            toast_title: action.success_toast_title(),
        }
    }

    pub(crate) fn failure(action: GpuiGhostexCliSettingsAction, message: String) -> Self {
        Self {
            action_id: action.action_id(),
            available: false,
            message,
            toast_level: "warning",
            toast_title: action.failure_toast_title(),
        }
    }
}

pub(crate) fn gpui_run_ghostex_cli_settings_action(
    action: GpuiGhostexCliSettingsAction,
) -> GpuiGhostexCliActionResult {
    match action {
        GpuiGhostexCliSettingsAction::InstallGhostexCli => {
            match gpui_repair_ghostex_cli_commands() {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::InstallBrowserControl => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["browser", "install-skill"],
                "Ghostex Embedded Browser Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallBrowserUseSkill => {
            gpui_install_cua_driver_wrapper_skill_action(
                action,
                &["browser-use", "install-skill"],
                "Ghostex Browser Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallComputerUseSkill => {
            gpui_install_cua_driver_wrapper_skill_action(
                action,
                &["computer-use", "install-skill"],
                "Ghostex Computer Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallSpaceoSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["spaceo", "install-skill"],
                "Ghostex SpaceO",
            )
        }
        GpuiGhostexCliSettingsAction::InstallCliSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["cli", "install-skill"],
                "Ghostex CLI skill",
            )
        }
        GpuiGhostexCliSettingsAction::InstallAgentsOrchestrationSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["agents-orchestration", "install-skill"],
                "Ghostex Agents",
            )
        }
        GpuiGhostexCliSettingsAction::InstallManageBeadsSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["manage-beads", "install-skill"],
                "Ghostex Manage Beads",
            )
        }
        GpuiGhostexCliSettingsAction::InstallGenerateTitleSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["generate-title", "install-skill"],
                "Ghostex Auto Rename Session",
            )
        }
        GpuiGhostexCliSettingsAction::InstallMoveCodexSessionSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["move-codex-session", "install-skill"],
                "Ghostex Move Codex Session",
            )
        }
        GpuiGhostexCliSettingsAction::InstallHelpSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["guide", "install-skill"],
                "Ghostex Help",
            )
        }
        GpuiGhostexCliSettingsAction::InstallAgentGuideSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["agent-guide", "install-skill"],
                "Ghostex Agent Guide",
            )
        }
        GpuiGhostexCliSettingsAction::InstallVisualsSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["visual", "install-skill"],
                "Ghostex Visuals",
            )
        }
        GpuiGhostexCliSettingsAction::InstallSecretsSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["secret", "install-skill"],
                "Ghostex Secrets",
            )
        }
        GpuiGhostexCliSettingsAction::InstallCuaDriverSkill => {
            match gpui_install_cua_driver_skill() {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::FinishDesktopControlSetup {
            driver_installed,
            was_update,
        } => {
            match gpui_finish_desktop_control_setup(driver_installed, was_update) {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::FinishTrycuaUninstall { succeeded: true } => {
            GpuiGhostexCliActionResult::success(
                action,
                if cfg!(target_os = "macos") {
                    "Fast Computer & Browser Use was removed. Its Accessibility and Screen Recording permissions were kept for a later reinstall."
                } else {
                    "Fast Computer & Browser Use was removed. You can reinstall it from Settings."
                }
                .to_string(),
            )
        }
        GpuiGhostexCliSettingsAction::FinishTrycuaUninstall { succeeded: false } => {
            GpuiGhostexCliActionResult::failure(
                action,
                "The Fast Computer & Browser Use uninstaller did not finish successfully. Settings shows its last output; plugin status was refreshed."
                    .to_string(),
            )
        }
        GpuiGhostexCliSettingsAction::FinishSpaceoSetup {
            installed,
            was_update,
        } => match gpui_finish_spaceo_setup(installed, was_update) {
            Ok(message) => GpuiGhostexCliActionResult::success(action, message),
            Err(message) => GpuiGhostexCliActionResult::failure(action, message),
        },
        GpuiGhostexCliSettingsAction::FinishSpaceoUninstall { succeeded: true } => {
            GpuiGhostexCliActionResult::success(
                action,
                "SpaceO was removed. Its Accessibility and Screen Recording permissions were kept for a later reinstall."
                    .to_string(),
            )
        }
        GpuiGhostexCliSettingsAction::FinishSpaceoUninstall { succeeded: false } => {
            GpuiGhostexCliActionResult::failure(
                action,
                "The SpaceO uninstaller did not finish successfully. Settings shows its last output; plugin status was refreshed."
                    .to_string(),
            )
        }
        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkill(skill_name) => {
            match gpui_uninstall_bundled_agent_skill(skill_name) {
                Ok(true) => GpuiGhostexCliActionResult::success(
                    action,
                    "Bundled Ghostex agent skill uninstalled. You can install it again from Settings."
                        .to_string(),
                ),
                Ok(false) => GpuiGhostexCliActionResult::success(
                    action,
                    "That bundled Ghostex agent skill was already uninstalled. Current integration status was refreshed."
                        .to_string(),
                ),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkills => {
            match gpui_uninstall_bundled_agent_skills() {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
    }
}

pub(crate) const GPUI_CUA_DRIVER_INSTALL_RUNNING_MESSAGE: &str = "The official Fast Computer & Browser Use installer is running in the background. Settings shows its progress.";
pub(crate) const GPUI_CUA_DRIVER_REINSTALL_RUNNING_MESSAGE: &str = "The official Fast Computer & Browser Use installer is reinstalling the latest release in the background. Settings shows its progress.";
pub(crate) const GPUI_CUA_DRIVER_UNINSTALL_RUNNING_MESSAGE: &str = "The official Fast Computer & Browser Use uninstaller is running in the background. Settings shows its progress.";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_UPDATE_RUNNING_MESSAGE: &str = "Fast Computer & Browser Use is checking for and applying the latest official update in the background. Settings shows its progress.";

/*
CDXC:Extensions 2026-08-09:
macOS owns the in-app Trycua lifecycle. A missing driver runs trycua's
official installer; an existing driver performs a fresh update check and then
uses its canonical self-updater.
*/
/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User: "1 click installs it for them as much as possible", with a tooltip explaining how Ghostex installs it; the agreed plan moves Trycua "out of its terminal tab into the same job runner". The official installer, updater and uninstaller now run as a background job of the desktop app with their output shown in Settings, instead of in a command-pane terminal tab. Supersedes CDXC:Extensions 2026-08-24, which ran them in a command-pane tab (and on Windows crossed from the WSL shell through `powershell.exe`): the desktop app itself is a native Windows process, so it runs Windows PowerShell directly. This computer's own Desktop control rows still run it as a desktop job (they also start CuaDriver and ask for its grants on this screen); since CDXC:ManagedTools 2026-10-26 (server/src/managed_tools/desktop_control.rs, the user's "Allow it on a remote Mac.") a remote Mac's gxserver runs the same official commands for the machine picker and the phone, so change both together.
#[cfg(not(target_os = "windows"))]
pub(crate) const GPUI_TRYCUA_INSTALL_COMMAND: &str =
    "/bin/bash -c \"$(curl -fsSL https://cua.ai/driver/install.sh)\"";
#[cfg(target_os = "windows")]
pub(crate) const GPUI_TRYCUA_INSTALL_COMMAND: &str = "irm https://cua.ai/driver/install.ps1 | iex";
/// What the macOS and Linux job runs: the official command above, or the same installer from GitHub when `cua.ai` does not answer.
///
/// CDXC:ManagedTools 2026-11-07 FIX: Gregg Housh (Discord, 2026-11-06): "On multiple networks and with multiple DNS providers i've found cua.ai wont resolve to an IP ... the normal install command just fails"; the user: "Will switch it to make node first". Node cannot be the first step: Cua publishes `@trycua/cua-driver` on npm only as a Node SDK (no `cua-driver` command, no CuaDriver.app, no `bin` in any version). `cua.ai` serves only the two installer scripts (the driver downloads from github.com/trycua/cua releases), so on a failed `cua.ai` fetch the job runs `_install-rust.sh` (plus its optional `_install-common.sh`) straight from raw.githubusercontent.com, side by side in a temporary folder.
/// SEE-ALSO: server/src/managed_tools/desktop_control.rs (`TRYCUA_INSTALL_JOB` and `TRYCUA_UNINSTALL`, the same scripts; change both together). Windows does the same with `install.ps1` (CDXC:ManagedTools 2026-11-07 FIX, the user: "add the same GitHub fallback for Windows install.ps1 (and its uninstall), so Gregg's networks work on every OS"): `install.ps1` loads `_install-common.psm1` from cua.ai, so the fallback downloads both from raw.githubusercontent.com into a temp folder and runs the script from there, where it finds the module beside it; `uninstall.ps1` is standalone, so only its source changes.
#[cfg(not(target_os = "windows"))]
pub(crate) const GPUI_TRYCUA_INSTALL_JOB_COMMAND: &str = r#"{
if cua_installer="$(curl -fsSL --connect-timeout 10 https://cua.ai/driver/install.sh)"; then
  /bin/bash -c "$cua_installer"
else
  echo "cua.ai did not answer, so the installer comes from GitHub (trycua/cua) instead."
  cua_dir="$(mktemp -d)" && curl -fsSL -o "$cua_dir/_install-rust.sh" https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/_install-rust.sh && { curl -fsSL -o "$cua_dir/_install-common.sh" https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/_install-common.sh || true; } && /bin/bash "$cua_dir/_install-rust.sh"
  cua_status=$?
  rm -rf "$cua_dir"
  (exit "$cua_status")
fi
}"#;
/*
CDXC:Extensions 2026-09-26 DECISION:
Settings' Uninstall Trycua button runs the official uninstaller with `--keep-tcc`: the user chose to keep the Accessibility and Screen Recording grants (the uninstaller revokes them by default) so a later reinstall needs no new permission prompts. Windows has no such grants to keep.
*/
#[cfg(not(target_os = "windows"))]
pub(crate) const GPUI_TRYCUA_UNINSTALL_COMMAND: &str = r#"if cua_uninstaller="$(curl -fsSL --connect-timeout 10 https://cua.ai/driver/uninstall.sh)"; then
  /bin/bash -c "$cua_uninstaller" -- --keep-tcc
else
  echo "cua.ai did not answer, so the uninstaller comes from GitHub (trycua/cua) instead."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/uninstall.sh)" -- --keep-tcc
fi"#;
#[cfg(target_os = "windows")]
pub(crate) const GPUI_TRYCUA_UNINSTALL_COMMAND: &str = concat!(
    "$cuaScript = $null; ",
    "try { $cuaScript = Invoke-RestMethod -UseBasicParsing -TimeoutSec 15 -Uri 'https://cua.ai/driver/uninstall.ps1' } catch { }; ",
    "if (-not $cuaScript) { ",
    "Write-Host 'cua.ai did not answer, so the uninstaller comes from GitHub (trycua/cua) instead.'; ",
    "$cuaScript = Invoke-RestMethod -UseBasicParsing -Uri 'https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/uninstall.ps1' }; ",
    "Invoke-Expression $cuaScript"
);
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_START_COMMAND: &str =
    "/usr/bin/open -n -g -a CuaDriver --args serve";

/// CDXC:ManagedTools 2026-11-07 FIX: Gregg Housh's networks answer NXDOMAIN for cua.ai. `cua-driver update --apply` runs `curl -fsSL https://cua.ai/driver/install.sh | bash` without pipefail, so there it ran an empty script, printed "Installed cua-driver <new>", exited 0 and left the old version (seen in an NXDOMAIN container, 0.41.2 stayed 0.41.2). Update now runs the same install job as Install (cua.ai first, GitHub otherwise); the installer keeps the saved release channel itself, so this is the same update the driver's own command would apply.
/// SEE-ALSO: server/src/managed_tools/desktop_control.rs (`script`, the remote Mac's update; change both together).
#[cfg(target_os = "macos")]
pub(crate) fn gpui_cua_driver_update_script(executable: &str) -> String {
    format!(
        "{executable} check-update --no-cache && {GPUI_TRYCUA_INSTALL_JOB_COMMAND} && {GPUI_CUA_DRIVER_START_COMMAND}"
    )
}

pub(crate) fn gpui_cua_driver_command_action() -> GpuiInstallJobAction {
    #[cfg(target_os = "macos")]
    if let Some(cua_driver_path) = gpui_cua_driver_executable_path() {
        let executable = gpui_shell_single_quote_path(&cua_driver_path);
        return GpuiInstallJobAction {
            script: gpui_cua_driver_update_script(&executable),
            operation: "update",
            running_message: GPUI_CUA_DRIVER_UPDATE_RUNNING_MESSAGE,
            toast_title: "Updating Fast Computer & Browser Use",
        };
    }

    gpui_cua_driver_installer_command_action()
}

/// Reinstall runs the official installer even when Trycua is present, so its
/// exit finishes Desktop Control setup exactly like a first install.
pub(crate) fn gpui_cua_driver_reinstall_command_action() -> GpuiInstallJobAction {
    GpuiInstallJobAction {
        operation: "reinstall",
        running_message: GPUI_CUA_DRIVER_REINSTALL_RUNNING_MESSAGE,
        toast_title: "Reinstalling Fast Computer & Browser Use",
        ..gpui_cua_driver_installer_command_action()
    }
}

pub(crate) fn gpui_cua_driver_uninstall_command_action() -> GpuiInstallJobAction {
    GpuiInstallJobAction {
        script: GPUI_TRYCUA_UNINSTALL_COMMAND.to_string(),
        operation: "uninstall",
        running_message: GPUI_CUA_DRIVER_UNINSTALL_RUNNING_MESSAGE,
        toast_title: "Uninstalling Fast Computer & Browser Use",
    }
}

/// CDXC:ManagedTools 2026-10-06 WHY:
/// The official Windows installer stops at `Read-Host "Trigger UAC prompt to kill the stale daemon now? [Y/n]"` when a cua-driver daemon is alive but its pipe is dead, and Read-Host throws in the job's headless PowerShell, so the install failed. Its `Repair-CuaDriverStaleDaemon -AutoConfirm` switch is upstream's answer for automated runs (UAC still asks for consent), and the installer exposes no other way to pass it, so the job sets it as a default parameter value before running the installer.
/// Windows twin of the macOS job command above, run by Windows PowerShell 5.1 (`-Command`, so single quotes only).
#[cfg(target_os = "windows")]
pub(crate) const GPUI_TRYCUA_INSTALL_JOB_COMMAND: &str = concat!(
    "$cuaScript = $null; ",
    "try { $cuaScript = Invoke-RestMethod -UseBasicParsing -TimeoutSec 15 -Uri 'https://cua.ai/driver/install.ps1' } catch { }; ",
    "if ($cuaScript) { Invoke-Expression $cuaScript } else { ",
    "Write-Host 'cua.ai did not answer, so the installer comes from GitHub (trycua/cua) instead.'; ",
    "$cuaRaw = 'https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts'; ",
    "$cuaDir = Join-Path ([IO.Path]::GetTempPath()) ('cua-driver-install-' + [Guid]::NewGuid().ToString('N')); ",
    "New-Item -ItemType Directory -Path $cuaDir -Force | Out-Null; ",
    "try { ",
    "Invoke-WebRequest -UseBasicParsing -Uri ($cuaRaw + '/install.ps1') -OutFile (Join-Path $cuaDir 'install.ps1'); ",
    "Invoke-WebRequest -UseBasicParsing -Uri ($cuaRaw + '/_install-common.psm1') -OutFile (Join-Path $cuaDir '_install-common.psm1'); ",
    "& (Join-Path $cuaDir 'install.ps1'); ",
    "if (-not $?) { throw 'The Cua Driver installer failed.' } ",
    "} finally { Remove-Item -LiteralPath $cuaDir -Recurse -Force -ErrorAction SilentlyContinue } }"
);
#[cfg(target_os = "windows")]
const GPUI_TRYCUA_JOB_PRELUDE: &str =
    "$PSDefaultParameterValues['Repair-CuaDriverStaleDaemon:AutoConfirm'] = $true; ";

fn gpui_cua_driver_installer_command_action() -> GpuiInstallJobAction {
    #[cfg(target_os = "macos")]
    let script = format!("{GPUI_TRYCUA_INSTALL_JOB_COMMAND} && {GPUI_CUA_DRIVER_START_COMMAND}");
    #[cfg(target_os = "windows")]
    let script = format!("{GPUI_TRYCUA_JOB_PRELUDE}{GPUI_TRYCUA_INSTALL_JOB_COMMAND}");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let script = GPUI_TRYCUA_INSTALL_JOB_COMMAND.to_string();

    GpuiInstallJobAction {
        script,
        operation: "install",
        running_message: GPUI_CUA_DRIVER_INSTALL_RUNNING_MESSAGE,
        toast_title: "Installing Fast Computer & Browser Use",
    }
}

#[cfg(all(test, target_os = "macos"))]
mod cua_driver_update_tests {
    use super::*;

    /// Update must work where cua.ai does not resolve: the install job (cua.ai, else GitHub), not
    /// `cua-driver update --apply`, which only knows cua.ai (CDXC:ManagedTools 2026-11-07 FIX).
    #[test]
    fn the_update_installs_through_the_job_that_falls_back_to_github() {
        let script = gpui_cua_driver_update_script("'/opt/cua-driver'");
        assert!(!script.contains("update --apply"), "{script}");
        assert!(script.starts_with("'/opt/cua-driver' check-update --no-cache && "));
        assert!(script.contains(GPUI_TRYCUA_INSTALL_JOB_COMMAND));
        assert!(script.contains("raw.githubusercontent.com/trycua/cua"));
        assert!(script.ends_with(GPUI_CUA_DRIVER_START_COMMAND));
    }
}
