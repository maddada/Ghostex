use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;

use crate::app::helpers::*;

pub(crate) fn gpui_status_generated_at() -> String {
    gpui_iso8601_utc(SystemTime::now())
}

pub(crate) fn gpui_iso8601_utc(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_seconds = duration.as_secs() as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = gpui_civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    )
}

pub(crate) fn gpui_civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiGhostexCliProbe {
    pub(crate) cli_skill_path: Option<String>,
    pub(crate) browser_skill_path: Option<String>,
    pub(crate) computer_use_skill_path: Option<String>,
    pub(crate) embedded_browser_skill_path: Option<String>,
    pub(crate) fable56_orchestration_skill_path: Option<String>,
    pub(crate) manage_beads_skill_path: Option<String>,
    pub(crate) generate_title_skill_path: Option<String>,
    pub(crate) ghostex_path: Option<String>,
    pub(crate) ghostex_usable: bool,
    pub(crate) gx_blocked_by_existing_command: bool,
    pub(crate) gx_path: Option<String>,
    pub(crate) gx_usable: bool,
    pub(crate) manage_beads_skill_path: Option<String>,
    pub(crate) move_codex_session_skill_path: Option<String>,
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_ghostex_cli_probe() -> Result<GpuiGhostexCliProbe, String> {
    let status = windows_terminal_backend::ghostex_cli_status()?;
    Ok(GpuiGhostexCliProbe {
        cli_skill_path: status.cli_skill_path,
        browser_skill_path: status.browser_skill_path,
        computer_use_skill_path: status.computer_use_skill_path,
        embedded_browser_skill_path: status.embedded_browser_skill_path,
        fable56_orchestration_skill_path: status.fable56_orchestration_skill_path,
        manage_beads_skill_path: status.manage_beads_skill_path,
        generate_title_skill_path: status.generate_title_skill_path,
        ghostex_usable: status.ghostex_path.is_some(),
        ghostex_path: status.ghostex_path,
        gx_blocked_by_existing_command: status.gx_blocked_by_existing_command,
        gx_path: status.gx_path,
        gx_usable: status.gx_usable,
        manage_beads_skill_path: status.manage_beads_skill_path,
        move_codex_session_skill_path: status.move_codex_session_skill_path,
    })
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_ghostex_cli_probe() -> Result<GpuiGhostexCliProbe, String> {
    let home = gpui_home_dir();
    let ghostex_path = gpui_which_command("ghostex");
    let gx_path = gpui_which_command("gx");
    let ghostex_usable = ghostex_path
        .as_ref()
        .map(|path| gpui_is_probably_ghostex_command(path, "ghostex"))
        .unwrap_or(false);
    let gx_usable = gx_path
        .as_ref()
        .map(|path| gpui_is_probably_ghostex_command(path, "gx"))
        .unwrap_or(false);
    let skill_path = |name: &str| {
        let path = home.join(".agents/skills").join(name).join("SKILL.md");
        gpui_is_file(&path).then(|| gpui_path_string(&path))
    };
    Ok(GpuiGhostexCliProbe {
        cli_skill_path: skill_path("ghostex-cli"),
        browser_skill_path: skill_path("ghostex-browser-use"),
        computer_use_skill_path: skill_path("ghostex-computer-use"),
        embedded_browser_skill_path: skill_path("ghostex-embedded-browser-use"),
        fable56_orchestration_skill_path: skill_path("ghostex-fable-5.6-orchestration"),
        manage_beads_skill_path: skill_path("ghostex-manage-beads"),
        generate_title_skill_path: skill_path("ghostex-auto-rename-session"),
        ghostex_path: ghostex_path.as_ref().map(|path| gpui_path_string(path)),
        ghostex_usable,
        gx_blocked_by_existing_command: gx_path.is_some() && !gx_usable,
        gx_path: gx_path.as_ref().map(|path| gpui_path_string(path)),
        gx_usable,
        manage_beads_skill_path: skill_path("ghostex-manage-beads"),
        move_codex_session_skill_path: skill_path("ghostex-move-codex-session"),
    })
}

pub(crate) fn gpui_ghostex_cli_status_message(detail_override: Option<&str>) -> serde_json::Value {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:36:
    GPUI Settings must answer CLI/status refreshes with the shared React contract so integration rows stop loading. The read-only GPUI probe may inspect PATH, fixed Ghostex-owned skill paths, the app bundle/local CEF resources, and Cua Driver presence, but it must not run installers, repair commands, permission prompts, or log raw paths.
    */
    let (probe, probe_error) = match gpui_ghostex_cli_probe() {
        Ok(probe) => (probe, None),
        Err(message) => (GpuiGhostexCliProbe::default(), Some(message)),
    };
    let ghostex_usable = probe.ghostex_usable;
    let gx_usable = probe.gx_usable;
    let gx_blocked = probe.gx_blocked_by_existing_command;
    let browser_skill_installed = probe.browser_skill_path.is_some();
    let embedded_browser_skill_installed = probe.embedded_browser_skill_path.is_some();
    let computer_use_skill_installed = probe.computer_use_skill_path.is_some();
    let cli_skill_installed = probe.cli_skill_path.is_some();
    let fable56_orchestration_skill_installed = probe.fable56_orchestration_skill_path.is_some();
    let manage_beads_skill_installed = probe.manage_beads_skill_path.is_some();
    let generate_title_skill_installed = probe.generate_title_skill_path.is_some();
    let manage_beads_skill_installed = probe.manage_beads_skill_path.is_some();
    let move_codex_session_skill_installed = probe.move_codex_session_skill_path.is_some();
    let cua_driver_path = gpui_cua_driver_executable_path();
    let cua_app_installed = gpui_is_dir(Path::new("/Applications/CuaDriver.app"));
    let cua_driver_installed = cua_driver_path.is_some() || cua_app_installed;
    let desktop_control_installed = cua_driver_installed && computer_use_skill_installed;
    let cua_driver_update_status = gpui_cua_driver_update_status(cua_driver_path.as_deref());
    let cua_permission_status =
        gpui_cua_driver_permission_status(cua_driver_path.as_deref(), cua_app_installed);
    let detail = detail_override
        .map(str::to_string)
        .unwrap_or_else(|| {
            let mut parts = Vec::new();
            if let Some(probe_error) = probe_error.as_ref() {
                parts.push(probe_error.clone());
            } else if ghostex_usable {
                #[cfg(target_os = "windows")]
                parts.push(
                    "Ghostex CLI is installed in the selected WSL2 distribution and matches this app's managed gxserver package."
                        .to_string(),
                );
                #[cfg(not(target_os = "windows"))]
                parts.push(
                    "Ghostex CLI was found on PATH and appears to be Ghostex-owned.".to_string(),
                );
            } else if probe.ghostex_path.is_some() {
                parts.push("A ghostex command was found on PATH, but GPUI could not prove it is the Ghostex-owned wrapper or app command.".to_string());
            } else {
                #[cfg(target_os = "windows")]
                parts.push(
                    "Ghostex CLI was not found in the selected WSL2 distribution.".to_string(),
                );
                #[cfg(not(target_os = "windows"))]
                parts.push("Ghostex CLI was not found on PATH.".to_string());
            }
            if gx_usable {
                #[cfg(target_os = "windows")]
                parts.push("The gx alias in WSL is linked to the managed Ghostex CLI.".to_string());
                #[cfg(not(target_os = "windows"))]
                parts.push("The gx alias appears to be Ghostex-owned.".to_string());
            } else if gx_blocked {
                parts.push("A gx command exists on PATH, but GPUI could not prove it belongs to Ghostex.".to_string());
            }
            parts.push(if browser_skill_installed {
                "Ghostex Browser Use skill is installed.".to_string()
            } else {
                "Ghostex Browser Use skill is not installed.".to_string()
            });
            parts.push(if computer_use_skill_installed {
                "Ghostex Computer Use skill is installed.".to_string()
            } else {
                "Ghostex Computer Use skill is not installed.".to_string()
            });
            parts.push(if embedded_browser_skill_installed {
                "Ghostex Embedded Browser Use skill is installed.".to_string()
            } else {
                "Ghostex Embedded Browser Use skill is not installed.".to_string()
            });
            parts.push(if cli_skill_installed {
                "Ghostex CLI skill is installed.".to_string()
            } else {
                "Ghostex CLI skill is not installed.".to_string()
            });
            parts.push(if fable56_orchestration_skill_installed {
                "Ghostex Fable 5.6 Orchestration skill is installed.".to_string()
            } else {
                "Ghostex Fable 5.6 Orchestration skill is not installed.".to_string()
            });
            parts.push(if manage_beads_skill_installed {
                "Ghostex Manage Beads skill is installed.".to_string()
            } else {
                "Ghostex Manage Beads skill is not installed.".to_string()
            });
            parts.push(if generate_title_skill_installed {
                "Ghostex Auto Rename Session skill is installed.".to_string()
            } else {
                "Ghostex Auto Rename Session skill is not installed.".to_string()
            });
            parts.push(if manage_beads_skill_installed {
                "Ghostex Project Board Beads skill is installed.".to_string()
            } else {
                "Ghostex Project Board Beads skill is not installed.".to_string()
            });
            parts.push(if move_codex_session_skill_installed {
                "Ghostex Move Codex Session skill is installed.".to_string()
            } else {
                "Ghostex Move Codex Session skill is not installed.".to_string()
            });
            /*
            CDXC:GPUIDesktopControlSettings 2026-06-24-13:14:
            Desktop Control readiness in Settings requires both Cua Driver and the Ghostex Computer Use skill. GPUI status refreshes must probe Cua Driver privacy grants read-only with `prompt:false`, parse the two boolean contract fields, and keep permission detail generic instead of forwarding raw command output.
            */
            parts.push(if desktop_control_installed {
                "Desktop Control is installed.".to_string()
            } else {
                "Desktop Control is not installed yet.".to_string()
            });
            parts.push(cua_permission_status.detail.clone());
            parts.join(" ")
        });

    serde_json::json!({
        "cliSkillInstalled": cli_skill_installed,
        "cliSkillPath": probe.cli_skill_path,
        "browserSkillInstalled": browser_skill_installed,
        "browserSkillPath": probe.browser_skill_path,
        "computerUseSkillInstalled": computer_use_skill_installed,
        "computerUseSkillPath": probe.computer_use_skill_path,
        "embeddedBrowserSkillInstalled": embedded_browser_skill_installed,
        "embeddedBrowserSkillPath": probe.embedded_browser_skill_path,
        "cuaAppInstalled": cua_app_installed,
        "cuaDriverAccessibilityPermissionGranted": cua_permission_status.accessibility_granted,
        "cuaDriverInstalled": cua_driver_installed,
        /*
        CDXC:TrycuaPrerequisite 2026-08-24:
        Settings shows the exact command its Install Trycua button runs, so the
        command string is published by the host that owns it instead of being
        guessed per platform in React.
        */
        "cuaDriverInstallCommand": GPUI_TRYCUA_INSTALL_COMMAND,
        "cuaDriverLatestVersion": cua_driver_update_status.latest_version,
        "cuaDriverManagedUpdatesSupported": cfg!(target_os = "macos"),
        "cuaDriverPermissionDetail": cua_permission_status.detail,
        "cuaDriverPath": cua_driver_path.as_ref().map(|path| gpui_path_string(path)),
        "cuaDriverScreenRecordingPermissionGranted": cua_permission_status.screen_recording_granted,
        "cuaDriverUpdateAvailable": cua_driver_update_status.update_available,
        "cuaDriverVersion": cua_driver_update_status.current_version,
        "detail": detail,
        "fable56OrchestrationSkillInstalled": fable56_orchestration_skill_installed,
        "fable56OrchestrationSkillPath": probe.fable56_orchestration_skill_path,
        "manageBeadsSkillInstalled": manage_beads_skill_installed,
        "manageBeadsSkillPath": probe.manage_beads_skill_path,
        "generatedAt": gpui_status_generated_at(),
        "generateTitleSkillInstalled": generate_title_skill_installed,
        "generateTitleSkillPath": probe.generate_title_skill_path,
        "ghostexPath": probe.ghostex_path,
        "gxBlockedByExistingCommand": gx_blocked,
        "gxPath": probe.gx_path,
        "gxUsable": gx_usable,
        "installed": ghostex_usable,
        "manageBeadsSkillInstalled": manage_beads_skill_installed,
        "manageBeadsSkillPath": probe.manage_beads_skill_path,
        "moveCodexSessionSkillInstalled": move_codex_session_skill_installed,
        "moveCodexSessionSkillPath": probe.move_codex_session_skill_path,
        "type": "ghostexCliStatus",
    })
}
