use std::{
    path::Path,
    time::Duration,
};

use anyhow::Result;

use crate::app::helpers::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum GpuiGhostexCliSettingsAction {
    InstallGhostexCli,
    InstallBrowserControl,
    InstallBrowserUseSkill,
    InstallComputerUseSkill,
    InstallAgentOrchestrationSkill,
    InstallFable56OrchestrationSkill,
    InstallFindPrevSessionSkill,
    InstallGenerateTitleSkill,
    InstallMoveCodexSessionSkill,
    FinishDesktopControlSetup {
        driver_installed: bool,
        was_update: bool,
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
            Self::InstallAgentOrchestrationSkill => "installAgentOrchestrationSkill",
            Self::InstallFable56OrchestrationSkill => "installFable56OrchestrationSkill",
            Self::InstallFindPrevSessionSkill => "installFindPrevSessionSkill",
            Self::InstallGenerateTitleSkill => "installGenerateTitleSkill",
            Self::InstallMoveCodexSessionSkill => "installMoveCodexSessionSkill",
            Self::FinishDesktopControlSetup { .. } => "installCuaDriver",
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
            Self::InstallAgentOrchestrationSkill => "Ghostex Agent Orchestration installed",
            Self::InstallFable56OrchestrationSkill => "Ghostex Fable 5.6 Orchestration installed",
            Self::InstallFindPrevSessionSkill => "Ghostex Find Previous Session installed",
            Self::InstallGenerateTitleSkill => "Ghostex Auto Rename Session installed",
            Self::InstallMoveCodexSessionSkill => "Ghostex Move Codex Session installed",
            Self::FinishDesktopControlSetup {
                was_update: true, ..
            } => "Cua Driver updated",
            Self::FinishDesktopControlSetup { .. } => "Desktop Control installed",
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
            Self::InstallAgentOrchestrationSkill => "Ghostex Agent Orchestration install failed",
            Self::InstallFable56OrchestrationSkill => {
                "Ghostex Fable 5.6 Orchestration install failed"
            }
            Self::InstallFindPrevSessionSkill => "Ghostex Find Previous Session install failed",
            Self::InstallGenerateTitleSkill => "Ghostex Auto Rename Session install failed",
            Self::InstallMoveCodexSessionSkill => "Ghostex Move Codex Session install failed",
            Self::FinishDesktopControlSetup {
                was_update: true, ..
            } => "Cua Driver update failed",
            Self::FinishDesktopControlSetup { .. } => "Desktop Control setup incomplete",
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
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["browser-use", "install-skill"],
                "Ghostex Browser Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallComputerUseSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["computer-use", "install-skill"],
                "Ghostex Computer Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallAgentOrchestrationSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["agent-orchestration", "install-skill"],
                "Ghostex Agent Orchestration",
            )
        }
        GpuiGhostexCliSettingsAction::InstallFable56OrchestrationSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["fable-5.6-orchestration", "install-skill"],
                "Ghostex Fable 5.6 Orchestration",
            )
        }
        GpuiGhostexCliSettingsAction::InstallFindPrevSessionSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["find-prev-session", "install-skill"],
                "Ghostex Find Previous Session",
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
        GpuiGhostexCliSettingsAction::FinishDesktopControlSetup {
            driver_installed,
            was_update,
        } => {
            match gpui_finish_desktop_control_setup(driver_installed, was_update) {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GpuiGteInstallActionResult {
    pub(crate) available: bool,
    pub(crate) message: &'static str,
    pub(crate) toast_level: &'static str,
    pub(crate) toast_title: &'static str,
}

pub(crate) fn gpui_gte_homebrew_install_command() -> (&'static str, [&'static str; 2], Duration) {
    (
        "/bin/zsh",
        ["-lc", GPUI_GTE_HOMEBREW_INSTALL_SCRIPT],
        Duration::from_secs(5 * 60),
    )
}

pub(crate) fn gpui_install_gte_from_homebrew() -> GpuiGteInstallActionResult {
    /*
    CDXC:GtePromptEditing 2026-06-24-13:28:
    GPUI Settings must use the same fixed Homebrew resolution order and `maddada/tap/gte` install operation as the macOS Settings button, bounded to five minutes with stdout/stderr suppressed. Installing the binary is separate from selecting the promptEditorBackend, and failures must report generic copy instead of raw Homebrew output, paths, command output, URLs, tokens, or environment.
    */
    let (command, args, timeout) = gpui_gte_homebrew_install_command();
    let result = gpui_run_command_with_timeout(Path::new(command), &args, timeout);
    gpui_gte_install_result_from_command_result(result)
}

pub(crate) fn gpui_gte_install_result_from_command_result(
    result: Result<bool, String>,
) -> GpuiGteInstallActionResult {
    match result {
        Ok(true) => GpuiGteInstallActionResult {
            available: true,
            message: GPUI_GTE_INSTALL_SUCCESS_MESSAGE,
            toast_level: "success",
            toast_title: GPUI_GTE_INSTALL_SUCCESS_MESSAGE,
        },
        Ok(false) | Err(_) => GpuiGteInstallActionResult {
            available: false,
            message: GPUI_GTE_INSTALL_FAILURE_MESSAGE,
            toast_level: "warning",
            toast_title: "gte install failed",
        },
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) const GPUI_CUA_DRIVER_RELEASES_URL: &str =
    "https://github.com/trycua/cua/releases?q=cua-driver-rs&expanded=true";
pub(crate) const GPUI_CUA_DRIVER_INSTALL_COMMAND_ID: &str = "ghostex.gpui.installCuaDriver";
pub(crate) const GPUI_CUA_DRIVER_UPDATE_COMMAND_ID: &str = "ghostex.gpui.updateCuaDriver";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_INSTALL_TAB_TITLE: &str = "Install Cua Driver";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_UPDATE_TAB_TITLE: &str = "Update Cua Driver";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_INSTALL_RUNNING_MESSAGE: &str = "The official Cua Driver installer is running in a command terminal tab. Plugin status updates when it finishes.";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_UPDATE_RUNNING_MESSAGE: &str = "Cua Driver is checking for and applying the latest official update in a command terminal tab. Plugin status updates when it finishes.";

/*
CDXC:GPUIDesktopControlSettings 2026-08-09:
macOS owns the in-app Cua Driver lifecycle. A missing driver runs trycua's
official installer; an existing driver performs a fresh update check and then
uses its canonical self-updater. Windows and Linux intentionally do not run an
installer from Ghostex yet and open the Cua GitHub releases page instead.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_POSIX_INSTALL_COMMAND: &str =
    "/bin/bash -c \"$(curl -fsSL https://cua.ai/driver/install.sh)\"";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_START_COMMAND: &str = "/usr/bin/open -n -g -a CuaDriver --args serve";

#[cfg(target_os = "macos")]
pub(crate) struct GpuiCuaDriverCommandAction {
    pub(crate) command: String,
    pub(crate) command_id: &'static str,
    pub(crate) running_message: &'static str,
    pub(crate) tab_title: &'static str,
    pub(crate) toast_title: &'static str,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_cua_driver_command_action() -> GpuiCuaDriverCommandAction {
    if let Some(cua_driver_path) = gpui_cua_driver_executable_path() {
        let executable = gpui_shell_single_quote_path(&cua_driver_path);
        GpuiCuaDriverCommandAction {
            command: format!(
                "{executable} check-update --no-cache && {executable} update --apply && {GPUI_CUA_DRIVER_START_COMMAND}"
            ),
            command_id: GPUI_CUA_DRIVER_UPDATE_COMMAND_ID,
            running_message: GPUI_CUA_DRIVER_UPDATE_RUNNING_MESSAGE,
            tab_title: GPUI_CUA_DRIVER_UPDATE_TAB_TITLE,
            toast_title: "Updating Cua Driver",
        }
    } else {
        GpuiCuaDriverCommandAction {
            command: format!(
                "{GPUI_CUA_DRIVER_POSIX_INSTALL_COMMAND} && {GPUI_CUA_DRIVER_START_COMMAND}"
            ),
            command_id: GPUI_CUA_DRIVER_INSTALL_COMMAND_ID,
            running_message: GPUI_CUA_DRIVER_INSTALL_RUNNING_MESSAGE,
            tab_title: GPUI_CUA_DRIVER_INSTALL_TAB_TITLE,
            toast_title: "Installing Cua Driver",
        }
    }
}

