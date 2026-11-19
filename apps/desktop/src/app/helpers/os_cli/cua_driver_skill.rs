//! The cua-driver skill: Trycua's own agent skill pack, installed and removed by the
//! `cua-driver skills` verbs of the driver Settings already manages.

use std::{path::PathBuf, time::Duration};

use crate::app::helpers::*;

/// `cua-driver skills install` and `update` download the pack from GitHub Releases.
const CUA_DRIVER_SKILLS_TIMEOUT: Duration = Duration::from_secs(180);

/// The local copy `cua-driver skills install` keeps before linking it into each agent.
fn gpui_cua_driver_skill_pack_path() -> PathBuf {
    gpui_home_dir()
        .join(".cua-driver")
        .join("skills")
        .join("cua-driver")
}

/// Agent skills folders `cua-driver skills install` links, relative to home. Keep in step with
/// `TRYCUA_SKILL_LINK_DIRS` in server/src/managed_tools/desktop_control.rs.
const GPUI_CUA_DRIVER_SKILL_LINK_DIRS: &[&str] = &[
    ".agents/skills",
    ".claude/skills",
    ".config/opencode/skills",
    ".factory/skills",
    ".gemini/skills",
    ".gemini/antigravity/skills",
    ".gemini/antigravity-cli/skills",
    ".gemini/config/skills",
    ".hermes/skills",
    ".kiro/skills",
    ".openclaw/skills",
    ".prime/agent/skills",
];

/// Whether any agent links the cua-driver skill, the user's sign that they want it. A dangling
/// link counts: a failed `skills update` deletes the pack and leaves the links (CDXC:AgentSkills
/// 2026-11-19 in server/src/managed_tools/desktop_control.rs).
pub(crate) fn gpui_cua_driver_skill_linked() -> bool {
    let home = gpui_home_dir();
    GPUI_CUA_DRIVER_SKILL_LINK_DIRS
        .iter()
        .any(|dir| std::fs::symlink_metadata(home.join(dir).join("cua-driver")).is_ok())
}

/// Whether `cua-driver skills install` has fetched the pack on this computer.
pub(crate) fn gpui_cua_driver_skill_pack_installed() -> bool {
    gpui_is_file(&gpui_cua_driver_skill_pack_path().join("SKILL.md"))
}

fn gpui_run_cua_driver_skills(args: &[&str]) -> Result<(), String> {
    let Some(cua_driver_path) = gpui_cua_driver_executable_path() else {
        return Err(
            "The cua-driver command was not found. Install Fast Computer & Browser Use first."
                .to_string(),
        );
    };
    match gpui_run_command_with_timeout(&cua_driver_path, args, CUA_DRIVER_SKILLS_TIMEOUT) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "cua-driver {} did not finish successfully. Current integration status was refreshed.",
            args.join(" ")
        )),
        Err(_) => Err(format!(
            "cua-driver {} could not be started. Current integration status was refreshed.",
            args.join(" ")
        )),
    }
}

/// CDXC:AgentSkills 2026-10-05 WHY:
/// `cua-driver skills install` never replaces a pack it already fetched, so a pack left from an older driver is refreshed with `skills update`, which re-fetches it for the installed driver version and relinks every agent.
pub(crate) fn gpui_install_cua_driver_skill() -> Result<String, String> {
    let refresh = gpui_cua_driver_skill_pack_installed();
    gpui_run_cua_driver_skills(&["skills", if refresh { "update" } else { "install" }])?;
    if gpui_cua_driver_skill_path().is_none() {
        return Err(
            "cua-driver finished, but no agent skills folder links the Cua Driver skill yet. Current integration status was refreshed."
                .to_string(),
        );
    }
    Ok(if refresh {
        "Cua Driver skill updated to match the installed driver.".to_string()
    } else {
        "Cua Driver skill installed for every detected agent.".to_string()
    })
}

/// CDXC:AgentSkills 2026-10-06 DECISION:
/// User: "make the browser/computer use ones install cua-driver and also tell the agent to read that skill". Installing Ghostex Computer Use or Ghostex Browser Use also installs Trycua's `cua-driver` skill, which both send the agent to first. The Ghostex skill installs even when the driver is missing or its skill fetch fails; the message then says how to add the cua-driver skill.
pub(crate) fn gpui_install_cua_driver_wrapper_skill_action(
    action: GpuiGhostexCliSettingsAction,
    args: &[&str],
    display_name: &str,
) -> GpuiGhostexCliActionResult {
    let message = match gpui_install_bundled_ghostex_skill(args, display_name) {
        Ok(message) => message,
        Err(message) => return GpuiGhostexCliActionResult::failure(action, message),
    };
    let note = if gpui_cua_driver_executable_path().is_none() {
        " Install Fast Computer & Browser Use to add the cua-driver skill it reads.".to_string()
    } else {
        match gpui_install_cua_driver_skill() {
            Ok(skill_message) => format!(" {skill_message}"),
            Err(_) => " The cua-driver skill could not be installed; use Install skill on the Fast Computer & Browser Use row.".to_string(),
        }
    };
    GpuiGhostexCliActionResult::success(action, format!("{message}{note}"))
}
