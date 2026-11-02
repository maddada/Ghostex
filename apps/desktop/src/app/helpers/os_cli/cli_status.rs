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
    pub(crate) spaceo_skill_path: Option<String>,
    pub(crate) embedded_browser_skill_path: Option<String>,
    pub(crate) agents_orchestration_skill_path: Option<String>,
    pub(crate) manage_beads_skill_path: Option<String>,
    pub(crate) generate_title_skill_path: Option<String>,
    pub(crate) ghostex_path: Option<String>,
    pub(crate) ghostex_usable: bool,
    pub(crate) gx_blocked_by_existing_command: bool,
    pub(crate) gx_path: Option<String>,
    pub(crate) gx_usable: bool,
    pub(crate) move_codex_session_skill_path: Option<String>,
    pub(crate) help_skill_path: Option<String>,
    pub(crate) visuals_skill_path: Option<String>,
    pub(crate) secrets_skill_path: Option<String>,
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_ghostex_cli_probe() -> Result<GpuiGhostexCliProbe, String> {
    let status = crate::windows_terminal_backend::ghostex_cli_status()?;
    Ok(GpuiGhostexCliProbe {
        cli_skill_path: status.cli_skill_path,
        browser_skill_path: status.browser_skill_path,
        computer_use_skill_path: status.computer_use_skill_path,
        // SpaceO runs only on macOS, so Windows never offers its skill.
        spaceo_skill_path: None,
        embedded_browser_skill_path: status.embedded_browser_skill_path,
        agents_orchestration_skill_path: status.agents_orchestration_skill_path,
        manage_beads_skill_path: status.manage_beads_skill_path,
        generate_title_skill_path: status.generate_title_skill_path,
        ghostex_usable: status.ghostex_path.is_some(),
        ghostex_path: status.ghostex_path,
        gx_blocked_by_existing_command: status.gx_blocked_by_existing_command,
        gx_path: status.gx_path,
        gx_usable: status.gx_usable,
        move_codex_session_skill_path: status.move_codex_session_skill_path,
        help_skill_path: status.help_skill_path,
        visuals_skill_path: status.visuals_skill_path,
        secrets_skill_path: status.secrets_skill_path,
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
        spaceo_skill_path: skill_path("ghostex-spaceo"),
        embedded_browser_skill_path: skill_path("ghostex-embedded-browser-use"),
        agents_orchestration_skill_path: skill_path("ghostex-agents"),
        manage_beads_skill_path: skill_path("ghostex-manage-beads"),
        generate_title_skill_path: skill_path("ghostex-auto-rename-session"),
        ghostex_path: ghostex_path.as_ref().map(|path| gpui_path_string(path)),
        ghostex_usable,
        gx_blocked_by_existing_command: gx_path.is_some() && !gx_usable,
        gx_path: gx_path.as_ref().map(|path| gpui_path_string(path)),
        gx_usable,
        move_codex_session_skill_path: skill_path("ghostex-move-codex-session"),
        help_skill_path: skill_path("ghostex-help"),
        visuals_skill_path: skill_path("ghostex-visuals"),
        secrets_skill_path: skill_path("ghostex-secrets"),
    })
}

/// What the Settings "Command line tool" row says when `ghostex` is not usable: the problem in
/// plain words and, when it is a PATH problem, the line that fixes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCliProblem {
    pub(crate) summary: String,
    pub(crate) fix: Option<GpuiCliPathFix>,
}

/// CDXC:Cli 2026-10-28 WHY:
/// The Command line tool row only turned amber with a fixed description, so a launcher sitting in
/// ~/.local/bin off PATH (common on Linux) looked broken with no reason and no way out. The status
/// now carries the reason and, for the PATH case, the shell line and the file it goes in.
pub(crate) fn gpui_cli_problem(
    installed: bool,
    probe_error: Option<&str>,
    foreign_path: Option<&str>,
    local_bin_off_path: bool,
    windows: bool,
    shell: Option<&str>,
) -> Option<GpuiCliProblem> {
    if installed {
        return None;
    }
    let summary = |text: String| {
        Some(GpuiCliProblem {
            summary: text,
            fix: None,
        })
    };
    if let Some(error) = probe_error {
        return summary(format!(
            "Ghostex could not check the ghostex command: {error}"
        ));
    }
    if let Some(path) = foreign_path {
        return summary(format!(
            "Another ghostex command comes first on your PATH ({path}), so terminals run it instead of Ghostex's. Rename or remove it, then press Repair."
        ));
    }
    if windows {
        return summary(
            "The ghostex command is not set up for your terminals yet. Press Repair to link it."
                .to_string(),
        );
    }
    if local_bin_off_path {
        let fix = gpui_cli_path_fix(shell);
        return Some(GpuiCliProblem {
            summary: format!(
                "ghostex and gx are in ~/.local/bin, but that folder is not on your PATH, so terminals cannot find them. {}, then open a new terminal.",
                fix.place
            ),
            fix: Some(fix),
        });
    }
    summary("The ghostex command is not on your PATH. Press Repair to link it.".to_string())
}

pub(crate) fn gpui_ghostex_cli_status_message(detail_override: Option<&str>) -> serde_json::Value {
    gpui_ghostex_cli_status_message_with_cua_update_check(detail_override, false)
}

/// `fresh_cua_update_check` bypasses cua-driver's cached update answer, and SpaceO's cached latest
/// release, for an explicit check.
pub(crate) fn gpui_ghostex_cli_status_message_with_cua_update_check(
    detail_override: Option<&str>,
    fresh_cua_update_check: bool,
) -> serde_json::Value {
    /*
    CDXC:StatusPet 2026-06-24-11:36:
    GPUI Settings must answer CLI/status refreshes with the shared React contract so integration rows stop loading. The read-only GPUI probe may inspect PATH, fixed Ghostex-owned skill paths, the app bundle/local CEF resources, and Cua Driver presence, but it must not run installers, repair commands, permission prompts, or log raw paths.
    */
    let (probe, probe_error) = match gpui_ghostex_cli_probe() {
        Ok(probe) => (probe, None),
        Err(message) => (GpuiGhostexCliProbe::default(), Some(message)),
    };
    let ghostex_usable = probe.ghostex_usable;
    let local_bin_off_path = !cfg!(target_os = "windows")
        && gpui_cli_user_bin_path_hint(
            &gpui_home_dir().join(".local/bin"),
            &gpui_cli_path_entries(),
            &gpui_home_dir(),
        )
        .is_some();
    let cli_problem = gpui_cli_problem(
        ghostex_usable,
        probe_error.as_deref(),
        probe.ghostex_path.as_deref(),
        local_bin_off_path,
        cfg!(target_os = "windows"),
        std::env::var("SHELL").ok().as_deref(),
    );
    let gx_usable = probe.gx_usable;
    let gx_blocked = probe.gx_blocked_by_existing_command;
    let browser_skill_installed = probe.browser_skill_path.is_some();
    let embedded_browser_skill_installed = probe.embedded_browser_skill_path.is_some();
    let computer_use_skill_installed = probe.computer_use_skill_path.is_some();
    let cli_skill_installed = probe.cli_skill_path.is_some();
    let agents_orchestration_skill_installed = probe.agents_orchestration_skill_path.is_some();
    let manage_beads_skill_installed = probe.manage_beads_skill_path.is_some();
    let generate_title_skill_installed = probe.generate_title_skill_path.is_some();
    let move_codex_session_skill_installed = probe.move_codex_session_skill_path.is_some();
    let help_skill_installed = probe.help_skill_path.is_some();
    let visuals_skill_installed = probe.visuals_skill_path.is_some();
    let secrets_skill_installed = probe.secrets_skill_path.is_some();
    let cua_driver_path = gpui_cua_driver_executable_path();
    let cua_app_installed = gpui_is_dir(Path::new("/Applications/CuaDriver.app"));
    let cua_driver_installed = cua_driver_path.is_some() || cua_app_installed;
    let cua_driver_skill_path = gpui_cua_driver_skill_path();
    let desktop_control_installed = cua_driver_installed && computer_use_skill_installed;
    // SpaceO's probes (version, latest release, daemon ping) run beside Trycua's instead of after them.
    let (spaceo_status, (cua_driver_update_status, cua_permission_status)) =
        std::thread::scope(|scope| {
            let spaceo = scope.spawn(|| {
                gpui_spaceo_supported().then(|| gpui_spaceo_status(fresh_cua_update_check))
            });
            let cua = (
                gpui_cua_driver_update_status(cua_driver_path.as_deref(), fresh_cua_update_check),
                gpui_cua_driver_permission_status(cua_driver_path.as_deref(), cua_app_installed),
            );
            (spaceo.join().ok().flatten(), cua)
        });
    let detail = detail_override
        .map(str::to_string)
        .unwrap_or_else(|| {
            let mut parts = Vec::new();
            if let Some(probe_error) = probe_error.as_ref() {
                parts.push(probe_error.clone());
            } else if ghostex_usable {
                #[cfg(target_os = "windows")]
                parts.push(if crate::windows_terminal_backend::current_preference()
                    == crate::windows_terminal_backend::WindowsTerminalBackendPreference::PowerShell
                {
                    "The bundled Ghostex CLI is ready for native Windows agents.".to_string()
                } else {
                    "Ghostex CLI is installed in the selected WSL2 distribution and matches this app's managed gxserver package.".to_string()
                });
                #[cfg(not(target_os = "windows"))]
                parts.push(
                    "Ghostex CLI was found on PATH and appears to be Ghostex-owned.".to_string(),
                );
            } else if probe.ghostex_path.is_some() {
                parts.push("A ghostex command was found on PATH, but GPUI could not prove it is the Ghostex-owned wrapper or app command.".to_string());
            } else {
                #[cfg(target_os = "windows")]
                parts.push(if crate::windows_terminal_backend::current_preference()
                    == crate::windows_terminal_backend::WindowsTerminalBackendPreference::PowerShell
                {
                    "The bundled Windows Ghostex CLI is missing. Reinstall Ghostex.".to_string()
                } else {
                    "Ghostex CLI was not found in the selected WSL2 distribution.".to_string()
                });
                #[cfg(not(target_os = "windows"))]
                parts.push(
                    match gpui_cli_user_bin_path_hint(
                        &gpui_home_dir().join(".local/bin"),
                        &gpui_cli_path_entries(),
                        &gpui_home_dir(),
                    ) {
                        Some(hint) => format!("Ghostex CLI was not found on PATH. {hint}"),
                        None => "Ghostex CLI was not found on PATH.".to_string(),
                    },
                );
            }
            if gx_usable {
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
            parts.push(if agents_orchestration_skill_installed {
                "Ghostex Agents skill is installed.".to_string()
            } else {
                "Ghostex Agents skill is not installed.".to_string()
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
            CDXC:Extensions 2026-06-24-13:14:
            Desktop Control readiness in Settings requires both Cua Driver and the Ghostex Computer Use skill. GPUI status refreshes must probe Cua Driver privacy grants read-only with `prompt:false`, parse the two boolean contract fields, and keep permission detail generic instead of forwarding raw command output.
            */
            parts.push(if desktop_control_installed {
                "Desktop Control is installed.".to_string()
            } else {
                "Desktop Control is not installed yet.".to_string()
            });
            parts.push(cua_permission_status.detail.clone());
            if let Some(spaceo_status) = spaceo_status.as_ref() {
                parts.push(if spaceo_status.path.is_some() {
                    spaceo_status.permissions.detail()
                } else {
                    "SpaceO is not installed.".to_string()
                });
            }
            parts.join(" ")
        });

    let mut payload = serde_json::json!({
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
        CDXC:Extensions 2026-08-24:
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
        "cuaDriverSkillInstalled": cua_driver_skill_path.is_some(),
        "cuaDriverSkillPath": cua_driver_skill_path.as_ref().map(|path| gpui_path_string(path)),
        "cuaDriverUpdateAvailable": cua_driver_update_status.update_available,
        "cuaDriverVersion": cua_driver_update_status.current_version,
        "detail": detail,
        "agentsOrchestrationSkillInstalled": agents_orchestration_skill_installed,
        "agentsOrchestrationSkillPath": probe.agents_orchestration_skill_path,
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
        "cliProblem": cli_problem.as_ref().map(|problem| problem.summary.clone()),
        "cliFixCommand": cli_problem
            .as_ref()
            .and_then(|problem| problem.fix.as_ref())
            .map(|fix| fix.command.clone()),
        "moveCodexSessionSkillInstalled": move_codex_session_skill_installed,
        "moveCodexSessionSkillPath": probe.move_codex_session_skill_path,
        "helpSkillInstalled": help_skill_installed,
        "helpSkillPath": probe.help_skill_path,
        "visualsSkillInstalled": visuals_skill_installed,
        "visualsSkillPath": probe.visuals_skill_path,
        "secretsSkillInstalled": secrets_skill_installed,
        "secretsSkillPath": probe.secrets_skill_path,
        "type": "ghostexCliStatus",
    });
    if let (Some(payload), serde_json::Value::Object(spaceo)) = (
        payload.as_object_mut(),
        gpui_spaceo_status_fields(
            &spaceo_status.unwrap_or_default(),
            probe.spaceo_skill_path.as_deref(),
        ),
    ) {
        payload.extend(spaceo);
    }
    gpui_cua_driver_add_permission_fields(&mut payload, &cua_permission_status);
    gpui_decorate_ghostex_cli_status(&mut payload);
    payload
}

#[cfg(test)]
mod cli_problem_tests {
    use super::*;

    #[test]
    fn installed_has_no_problem() {
        assert_eq!(gpui_cli_problem(true, None, None, true, false, None), None);
    }

    #[test]
    fn off_path_local_bin_says_why_and_gives_the_shell_line() {
        let problem =
            gpui_cli_problem(false, None, None, true, false, Some("/usr/bin/zsh")).unwrap();
        assert!(
            problem.summary.contains("not on your PATH"),
            "{}",
            problem.summary
        );
        assert!(problem.summary.contains("~/.zshrc"), "{}", problem.summary);
        assert_eq!(
            problem.fix.unwrap().command,
            "export PATH=\"$HOME/.local/bin:$PATH\""
        );
    }

    #[test]
    fn other_failures_say_what_to_do_without_a_shell_line() {
        let missing = gpui_cli_problem(false, None, None, false, false, Some("/bin/bash")).unwrap();
        assert!(
            missing.summary.contains("Press Repair"),
            "{}",
            missing.summary
        );
        assert_eq!(missing.fix, None);
        let foreign = gpui_cli_problem(
            false,
            None,
            Some("/usr/local/bin/ghostex"),
            true,
            false,
            None,
        )
        .unwrap();
        assert!(
            foreign.summary.contains("/usr/local/bin/ghostex"),
            "{}",
            foreign.summary
        );
        assert_eq!(foreign.fix, None);
        let windows = gpui_cli_problem(false, None, None, false, true, None).unwrap();
        assert!(windows.summary.contains("Press Repair"));
        assert_eq!(windows.fix, None);
        let error = gpui_cli_problem(false, Some("no bundle"), None, true, false, None).unwrap();
        assert!(error.summary.ends_with("no bundle"));
    }
}
