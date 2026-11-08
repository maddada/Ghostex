use base64::{Engine, engine::general_purpose::STANDARD};
use std::path::{Path, PathBuf};

/// PowerShell 7 wherever it is installed, otherwise Windows PowerShell 5.1: the same lookup
/// gxserver uses for sessions (`ghostex_paths::powershell`).
pub(super) fn shell() -> String {
    ghostex_paths::powershell::preferred_powershell(
        &ghostex_paths::powershell::Environment::from_process(),
        || {
            std::env::var_os("PATH")
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default()
        },
    )
    .to_string_lossy()
    .into_owned()
}

pub(super) fn invocation(command: Option<String>, cwd: Option<&Path>) -> (String, Vec<String>) {
    let mut args = vec!["-NoLogo".into()];
    if command.is_none() && cwd.is_some() {
        args.push("-NoExit".into());
    }
    if command.is_some() || cwd.is_some() {
        let script = format!(
            "{}{}",
            cwd.map(|path| format!(
                "Set-Location -LiteralPath '{}'; ",
                crate::app::helpers::gpui_powershell_single_quoted_content(&path.to_string_lossy())
            ))
            .unwrap_or_default(),
            command.unwrap_or_default()
        );
        args.push("-EncodedCommand".into());
        args.push(
            STANDARD.encode(
                script
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        );
    }
    (shell(), args)
}

pub(super) fn auth_token() -> Option<String> {
    std::fs::read_to_string(
        crate::shared_settings::ghostex_storage_paths()
            .gxserver_state_dir()
            .join("auth/token"),
    )
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

pub(super) fn path(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute Windows folder path.".into());
    }
    Ok(path.to_string_lossy().into_owned())
}

pub(super) fn cli_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("resources/native/ghostex.exe"))
        })
        .filter(|path| path.is_file())
}

pub(super) fn cli_status() -> super::WindowsWslGhostexCliStatus {
    let storage = crate::shared_settings::ghostex_storage_paths();
    let home = if crate::app::helpers::gpui_uses_isolated_storage() {
        &storage.data_dir
    } else {
        &storage.home_dir
    };
    let skill_path = |name: &str| {
        let path = home.join(".agents/skills").join(name).join("SKILL.md");
        path.is_file().then(|| path.to_string_lossy().into_owned())
    };
    let gx_path = crate::app::helpers::gpui_which_command("gx");
    let gx_usable = gx_path
        .as_ref()
        .is_some_and(|path| crate::app::helpers::gpui_is_probably_ghostex_command(path, "gx"));
    super::WindowsWslGhostexCliStatus {
        ghostex_path: cli_path().map(|path| path.to_string_lossy().into_owned()),
        gx_blocked_by_existing_command: gx_path.is_some() && !gx_usable,
        gx_path: gx_path.map(|path| path.to_string_lossy().into_owned()),
        gx_usable,
        cli_skill_path: skill_path("ghostex-cli"),
        browser_skill_path: skill_path("ghostex-browser-use"),
        computer_use_skill_path: skill_path("ghostex-computer-use"),
        embedded_browser_skill_path: skill_path("ghostex-embedded-browser-use"),
        agents_orchestration_skill_path: skill_path("ghostex-agents"),
        manage_beads_skill_path: skill_path("ghostex-manage-beads"),
        generate_title_skill_path: skill_path("ghostex-auto-rename-session"),
        move_codex_session_skill_path: skill_path("ghostex-move-codex-session"),
        help_skill_path: skill_path("ghostex-help"),
        agent_guide_skill_path: skill_path("ghostex-agent-guide"),
        visuals_skill_path: skill_path("ghostex-visuals"),
        secrets_skill_path: skill_path("ghostex-secrets"),
    }
}
