use std::{process::Stdio, time::Instant};

use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

/// CDXC:CodeEditor 2026-10-16 WHY:
/// An editor orphaned by an app crash kept port 3777 occupied, so every subsequent Code launch failed.
/// Keep its stdin pipe owned by the Rust Child and shut down through code-server's SIGTERM handler when that pipe closes, including when a pending launch is dropped or the app exits without running destructors.
/// The WSL launch needs it as much: ending the Windows `wsl.exe` leaves the Linux code-server running under WSL's init with the port still bound (so every Retry failed, issue #232), but it does close that process's stdin. This extends the 2026-09-15 reason to WSL.
const SOURCE_CODE_SERVER_OWNED_ENTRY: &str = r#"
process.stdin.on('end', () => process.kill(process.pid, 'SIGTERM'));
process.stdin.resume();
require(process.argv[1]);
"#;

/// Why a Code editor launch ended before the editor ran.
pub(crate) enum SourceCodeServerSpawnError {
    /// Another program answers on the editor's port; the card names it and offers to close it.
    PortInUse(SourceCodeServerPortInUse),
    Message(String),
}

impl From<String> for SourceCodeServerSpawnError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

/// The editor's port is still answering after the wait for a previous editor to let go.
fn source_code_server_port_in_use() -> SourceCodeServerSpawnError {
    let port = source_code_server_editor_port();
    SourceCodeServerSpawnError::PortInUse(SourceCodeServerPortInUse {
        port,
        holder: source_code_server_port_holder(port),
        closing: false,
        close_problem: None,
    })
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn source_code_server_spawn_runtime(
    target: &SourceCodeServerRuntimeTarget,
    settings: &SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, SourceCodeServerSpawnError> {
    source_code_server_spawn_host_runtime(target, settings, startup_deadline)
}

fn source_code_server_spawn_host_runtime(
    target: &SourceCodeServerRuntimeTarget,
    settings: &SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, SourceCodeServerSpawnError> {
    if matches!(
        target.endpoint,
        SourceCodeServerRuntimeEndpoint::Remote { .. }
    ) {
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        return Ok(source_code_server_spawn_remote_runtime(
            target,
            startup_deadline,
        )?);
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        return Err("Remote Source runtime is unavailable from this SSH host."
            .to_string()
            .into());
    }
    let readiness_timeout = startup_deadline.saturating_duration_since(Instant::now());
    if source_code_server_health_check() {
        let remaining = startup_deadline.saturating_duration_since(Instant::now());
        if !source_code_server_wait_until_not_responsive(
            SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL.min(remaining),
        ) {
            return Err(source_code_server_port_in_use());
        }
    }

    let repo_root = source_code_server_resolve_repo_root()?;
    let entrypoint = repo_root.join("out/node/entry.js");
    let node_path = source_code_server_resolve_node_path(&repo_root)?;
    let (user_data_dir, extensions_dir) = source_code_server_runtime_storage()?;
    if source_code_server_should_seed_default_theme(settings) {
        source_code_server_ensure_default_theme(&user_data_dir)?;
    }

    let mut command = gpui_background_command(&node_path);
    command
        .arg("--eval")
        .arg(SOURCE_CODE_SERVER_OWNED_ENTRY)
        .arg("--")
        .arg(&entrypoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&target.project_path)
        .envs(source_code_server_runtime_environment(&repo_root));
    if let Some(vscode_user_config_dir) = settings.linked_vscode_user_config_dir() {
        command
            .arg("--link-vscode-user-config")
            .arg("--vscode-user-config-dir")
            .arg(vscode_user_config_dir);
    }
    command
        .arg("--auth")
        .arg("none")
        .arg("--bind-addr")
        .arg(format!(
            "{}:{}",
            SOURCE_CODE_SERVER_EDITOR_HOST,
            source_code_server_editor_port()
        ))
        .arg("--disable-telemetry")
        .arg("--disable-update-check")
        .arg("--disable-workspace-trust")
        .arg("--disable-getting-started-override")
        .arg("--ignore-last-opened")
        .arg("--app-name")
        .arg("ghostex Code")
        .arg("--user-data-dir")
        .arg(&user_data_dir)
        .arg("--extensions-dir")
        .arg(&extensions_dir);
    // In the environment, not on the command line other accounts can read.
    let runtime_key = source_code_server_new_launch_key();
    command.env(SOURCE_CODE_SERVER_KEY_ENV, &runtime_key);

    #[cfg(windows)]
    {
        command
            .arg("--session-socket")
            .arg(source_code_server_native_session_socket(&user_data_dir));
    }

    // CDXC:CodeEditor 2026-10-19 WHY:
    // Resolving the payload runs `node --version` and checks the install (about 0.8s on a Windows PC), and a cold first launch there took 6.3s to answer, so preparation inside the 7s budget could fail an editor that was about to come up. The budget starts at spawn, as for WSL and remote launches.
    let startup_deadline = Instant::now() + readiness_timeout;
    let started_at = Instant::now();
    let child = command
        .spawn()
        .map_err(|_| "failed to start Source runtime".to_string())?;
    let readiness = source_code_server_wait_until_responsive(
        startup_deadline.saturating_duration_since(Instant::now()),
    );
    Ok(SourceCodeServerRuntimeStartOutput {
        child,
        forward: None,
        runtime_origin: source_code_server_editor_origin(),
        runtime_key: Some(runtime_key),
        prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
        started_at,
        http_runtime_ready: readiness.http_runtime_ready,
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_spawn_runtime(
    target: &SourceCodeServerRuntimeTarget,
    settings: &SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, SourceCodeServerSpawnError> {
    if matches!(
        target.endpoint,
        SourceCodeServerRuntimeEndpoint::Remote { .. }
    ) {
        return Ok(source_code_server_spawn_remote_runtime(
            target,
            startup_deadline,
        )?);
    }
    if windows_terminal_backend::current_preference()
        == windows_terminal_backend::WindowsTerminalBackendPreference::PowerShell
    {
        return source_code_server_spawn_host_runtime(target, settings, startup_deadline);
    }

    /*
    CDXC:PlatformSupport 2026-07-26:
    Windows projects and their authoritative paths live inside the selected
    WSL2 distribution. Launch code-server there as well; a native Windows
    Node child cannot use the WSL project path or the Linux runtime payload.
    The packaged Linux runtime is activated in WSL during startup; project and
    launch parameters cross the boundary as argv values, while the fixed WSL
    script owns Linux storage, environment, and working-directory setup.
    */
    let readiness_timeout = startup_deadline.saturating_duration_since(Instant::now());
    if source_code_server_health_check() {
        let remaining = startup_deadline.saturating_duration_since(Instant::now());
        // A leftover Ghostex editor in WSL is stopped (windows_terminal_backend::stop_leftover_source_code_server).
        if !source_code_server_wait_until_not_responsive(
            SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL.min(remaining),
        ) && !(windows_terminal_backend::stop_leftover_source_code_server(
            source_code_server_editor_port(),
        )
        .is_ok_and(|stopped| stopped)
            && source_code_server_wait_until_not_responsive(
                SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL,
            ))
        {
            return Err(source_code_server_port_in_use());
        }
    }

    let bind_address = format!(
        "{}:{}",
        SOURCE_CODE_SERVER_EDITOR_HOST,
        source_code_server_editor_port()
    );
    let mut command = windows_terminal_backend::source_code_server_command(
        &target.project_path,
        SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR,
        &bind_address,
        settings.link_vscode_user_config,
        settings.use_vscode_insiders_user_config,
        SOURCE_CODE_SERVER_OWNED_ENTRY,
    )?;
    // WSL passes only the Windows variables WSLENV names into Linux.
    let runtime_key = source_code_server_new_launch_key();
    let wslenv = match std::env::var("WSLENV") {
        Ok(existing) if !existing.is_empty() => format!("{existing}:{SOURCE_CODE_SERVER_KEY_ENV}"),
        _ => SOURCE_CODE_SERVER_KEY_ENV.to_string(),
    };
    command
        .env(SOURCE_CODE_SERVER_KEY_ENV, &runtime_key)
        .env("WSLENV", wslenv)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // CDXC:CodeEditor 2026-10-16 WHY:
    // Building the WSL command installs or checks the Linux runtime inside WSL (several wsl.exe round trips, and on first use a 240MB archive verified and unpacked), which took longer than the whole readiness budget, so the editor was started with no time left and every open failed with "VS Code didn't start in time" (issue #232). The budget starts when code-server is spawned, as for remote launches.
    let startup_deadline = Instant::now() + readiness_timeout;
    let started_at = Instant::now();
    let child = command
        .spawn()
        .map_err(|_| "failed to start Source runtime in WSL".to_string())?;
    let readiness = source_code_server_wait_until_responsive(
        startup_deadline.saturating_duration_since(Instant::now()),
    );
    Ok(SourceCodeServerRuntimeStartOutput {
        child,
        forward: None,
        runtime_origin: source_code_server_editor_origin(),
        runtime_key: Some(runtime_key),
        prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
        started_at,
        http_runtime_ready: readiness.http_runtime_ready,
    })
}
