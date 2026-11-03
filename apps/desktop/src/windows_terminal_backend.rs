//! Windows terminal/backend integration.
//!
//! Windows selects native PowerShell projects or a WSL2 workspace explicitly.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // used by the windows path
pub(crate) enum WindowsTerminalBackendPreference {
    Automatic,
    Wsl,
    PowerShell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // used by the windows path
pub(crate) enum ResolvedWindowsTerminalBackend {
    Wsl { distribution: String },
    PowerShell,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WindowsWslReadiness {
    Ready,
    MissingWsl,
    MissingDistribution,
    ChooseDistribution(Vec<String>),
    ConfiguredDistributionUnavailable(String),
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsWslSetupPhase {
    Checking,
    Downloading,
    Installing,
    Starting,
    Connecting,
    Ready,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowsWslGhostexCliStatus {
    pub(crate) cli_skill_path: Option<String>,
    pub(crate) browser_skill_path: Option<String>,
    pub(crate) computer_use_skill_path: Option<String>,
    pub(crate) embedded_browser_skill_path: Option<String>,
    pub(crate) agents_orchestration_skill_path: Option<String>,
    pub(crate) manage_beads_skill_path: Option<String>,
    pub(crate) generate_title_skill_path: Option<String>,
    pub(crate) ghostex_path: Option<String>,
    pub(crate) gx_blocked_by_existing_command: bool,
    pub(crate) gx_path: Option<String>,
    pub(crate) gx_usable: bool,
    pub(crate) move_codex_session_skill_path: Option<String>,
    pub(crate) help_skill_path: Option<String>,
    pub(crate) visuals_skill_path: Option<String>,
    pub(crate) secrets_skill_path: Option<String>,
}

#[allow(dead_code)] // used by the windows path
#[cfg(windows)]
static PREFERENCE: std::sync::Mutex<Option<WindowsTerminalBackendPreference>> =
    std::sync::Mutex::new(None);

pub(crate) fn current_preference() -> WindowsTerminalBackendPreference {
    #[cfg(windows)]
    {
        let mut preference = PREFERENCE.lock().unwrap_or_else(|error| error.into_inner());
        *preference.get_or_insert_with(|| {
            let configured = std::env::var("GHOSTEX_WINDOWS_TERMINAL_BACKEND")
                .ok()
                .or_else(|| {
                    crate::shared_settings::shared_sidebar_settings_snapshot()
                        .object()
                        .get("windowsTerminalBackend")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                });
            if configured.as_deref() == Some("wsl") {
                WindowsTerminalBackendPreference::Wsl
            } else {
                WindowsTerminalBackendPreference::PowerShell
            }
        })
    }
    #[cfg(not(windows))]
    {
        WindowsTerminalBackendPreference::Wsl
    }
}

#[cfg(windows)]
pub(crate) fn reload_preference_for_setup() {
    *PREFERENCE.lock().unwrap_or_else(|error| error.into_inner()) = None;
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
mod native_package;
#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub(crate) fn mark_package_update_required() {
    platform::mark_package_update_required();
}

#[cfg(target_os = "windows")]
pub(crate) fn reset() {
    platform::reset();
}

#[cfg(target_os = "windows")]
pub(crate) fn auth_token() -> Option<String> {
    if current_preference() == WindowsTerminalBackendPreference::PowerShell {
        return native::auth_token();
    }
    platform::auth_token()
}

#[cfg(target_os = "windows")]
pub(crate) fn ghostex_cli_status() -> Result<WindowsWslGhostexCliStatus, String> {
    if current_preference() == WindowsTerminalBackendPreference::PowerShell {
        return Ok(native::cli_status());
    }
    platform::ghostex_cli_status()
}

/// CDXC:AgentSkills 2026-09-27 WHY:
/// Settings must run the CLI in the same environment its status probe inspects: the bundled executable for PowerShell, or the owned CLI in the selected WSL distribution. The desktop's PATH cannot identify both.
#[cfg(target_os = "windows")]
pub(crate) fn ghostex_cli_invocation(
    args: &[&str],
) -> Result<(std::path::PathBuf, Vec<String>), String> {
    if current_preference() == WindowsTerminalBackendPreference::PowerShell {
        let path = native::cli_path().ok_or_else(|| {
            "The bundled Ghostex CLI is missing. Reinstall Ghostex before installing agent skills."
                .to_string()
        })?;
        return Ok((path, args.iter().map(|arg| (*arg).to_string()).collect()));
    }
    platform::ghostex_cli_invocation(args)
}

#[cfg(target_os = "windows")]
pub(crate) fn resolve_current() -> Result<ResolvedWindowsTerminalBackend, String> {
    platform::resolve(current_preference())
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_gxserver_for_current_settings()
-> Result<ResolvedWindowsTerminalBackend, String> {
    platform::prepare_gxserver(current_preference(), &mut |_| {})
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_gxserver_for_current_settings_with_progress(
    progress: &mut dyn FnMut(WindowsWslSetupPhase),
) -> Result<ResolvedWindowsTerminalBackend, String> {
    platform::prepare_gxserver(current_preference(), progress)
}

#[cfg(target_os = "windows")]
pub(crate) fn wsl_readiness() -> WindowsWslReadiness {
    if current_preference() == WindowsTerminalBackendPreference::PowerShell {
        return WindowsWslReadiness::Ready;
    }
    platform::readiness()
}

#[cfg(target_os = "windows")]
pub(crate) fn terminal_invocation(
    command: Option<String>,
    working_directory: Option<&std::path::Path>,
) -> (String, Vec<String>) {
    platform::terminal_invocation(command, working_directory)
}

#[cfg(windows)]
pub(crate) fn native_terminal_invocation(
    command: Option<String>,
    working_directory: Option<&std::path::Path>,
) -> (String, Vec<String>) {
    native::invocation(command, working_directory)
}

#[cfg(target_os = "windows")]
pub(crate) fn spawn_zmx_refresh(
    distribution: &str,
    session_name: &str,
    rows: u16,
    columns: u16,
) -> Result<std::process::Child, String> {
    platform::spawn_zmx_refresh(distribution, session_name, rows, columns)
}

#[cfg(target_os = "windows")]
pub(crate) fn resource_process_snapshot() -> Option<String> {
    platform::resource_process_snapshot()
}

#[cfg(target_os = "windows")]
pub(crate) fn resource_server_snapshot() -> Option<String> {
    platform::resource_server_snapshot()
}

#[cfg(target_os = "windows")]
pub(crate) fn resource_process_cwd_snapshot(pids: &[u32]) -> Option<String> {
    platform::resource_process_cwd_snapshot(pids)
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_command(
    project_path: &std::path::Path,
    required_node_major: u64,
    bind_address: &str,
    link_vscode_user_config: bool,
    use_vscode_insiders_user_config: bool,
    owned_node_entry: &str,
) -> Result<std::process::Command, String> {
    platform::source_code_server_command(
        project_path,
        required_node_major,
        bind_address,
        link_vscode_user_config,
        use_vscode_insiders_user_config,
        owned_node_entry,
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn stop_leftover_source_code_server(port: u16) -> Result<bool, String> {
    platform::stop_leftover_source_code_server(port)
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_port_owners(port: u16) -> Result<Vec<(u32, String)>, String> {
    platform::source_code_server_port_owners(port)
}

#[cfg(target_os = "windows")]
pub(crate) fn end_source_code_server_port_owners(port: u16) -> Result<bool, String> {
    platform::end_source_code_server_port_owners(port)
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_open_file_command(
    file_path: &std::path::Path,
    line: Option<u32>,
    column: Option<u32>,
    workspace_folder: &std::path::Path,
    required_node_major: u64,
) -> Result<std::process::Command, String> {
    platform::source_code_server_open_file_command(
        file_path,
        line,
        column,
        workspace_folder,
        required_node_major,
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn wsl_path_for_windows_path(path: &std::path::Path) -> Result<String, String> {
    platform::wsl_path_for_windows_path(path)
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_path_for_wsl_path(
    path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    platform::windows_path_for_wsl_path(path)
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)] // non-windows no-op stub; only the windows path calls this
pub(crate) fn auth_token() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)] // non-windows no-op stub; only the windows path calls this
pub(crate) fn mark_package_update_required() {}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)] // non-windows no-op stub; only the windows path calls this
pub(crate) fn reset() {}
