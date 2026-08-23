use std::{
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

#[cfg(not(target_os = "windows"))]
pub(crate) fn source_code_server_spawn_runtime(
    target: &SourceCodeServerRuntimeTarget,
    settings: &SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    if matches!(
        target.endpoint,
        SourceCodeServerRuntimeEndpoint::Remote { .. }
    ) {
        #[cfg(target_os = "macos")]
        return source_code_server_spawn_remote_runtime(target, startup_deadline);
        #[cfg(not(target_os = "macos"))]
        return Err(
            "Remote Source runtime is available only from the macOS SSH owner.".to_string(),
        );
    }
    if source_code_server_health_check() {
        let remaining = startup_deadline.saturating_duration_since(Instant::now());
        let _ = source_code_server_wait_until_not_responsive(
            SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL.min(remaining),
        );
    }

    let repo_root = source_code_server_resolve_repo_root()?;
    let entrypoint = repo_root.join("out/node/entry.js");
    let node_path = source_code_server_resolve_node_path(&repo_root)?;
    let (user_data_dir, extensions_dir) = source_code_server_runtime_storage()?;
    if source_code_server_should_seed_default_theme(settings) {
        source_code_server_ensure_default_theme(&user_data_dir)?;
    }
    if Instant::now() >= startup_deadline {
        return Err("Source runtime startup timed out".to_string());
    }

    let mut command = Command::new(&node_path);
    command
        .arg(&entrypoint)
        .stdin(Stdio::null())
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
            SOURCE_CODE_SERVER_EDITOR_HOST, SOURCE_CODE_SERVER_EDITOR_PORT
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

    let started_at = Instant::now();
    let child = command
        .spawn()
        .map_err(|_| "failed to start Source runtime".to_string())?;
    let readiness = source_code_server_wait_until_responsive(
        startup_deadline.saturating_duration_since(Instant::now()),
    );
    Ok(SourceCodeServerRuntimeStartOutput {
        child,
        runtime_origin: SOURCE_CODE_SERVER_EDITOR_ORIGIN.to_string(),
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
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    /*
    CDXC:GPUISourceWindowsWsl 2026-07-26:
    Windows projects and their authoritative paths live inside the selected
    WSL2 distribution. Launch code-server there as well; a native Windows
    Node child cannot use the WSL project path or the Linux runtime payload.
    The packaged Linux runtime is activated in WSL during startup; project and
    launch parameters cross the boundary as argv values, while the fixed WSL
    script owns Linux storage, environment, and working-directory setup.
    */
    if source_code_server_health_check() {
        let remaining = startup_deadline.saturating_duration_since(Instant::now());
        let _ = source_code_server_wait_until_not_responsive(
            SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL.min(remaining),
        );
    }

    if Instant::now() >= startup_deadline {
        return Err("Source runtime startup timed out".to_string());
    }
    let bind_address = format!(
        "{}:{}",
        SOURCE_CODE_SERVER_EDITOR_HOST, SOURCE_CODE_SERVER_EDITOR_PORT
    );
    let mut command = windows_terminal_backend::source_code_server_command(
        &target.project_path,
        SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR,
        &bind_address,
        settings.link_vscode_user_config,
        settings.use_vscode_insiders_user_config,
    )?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let started_at = Instant::now();
    let child = command
        .spawn()
        .map_err(|_| "failed to start Source runtime in WSL".to_string())?;
    let readiness = source_code_server_wait_until_responsive(
        startup_deadline.saturating_duration_since(Instant::now()),
    );
    Ok(SourceCodeServerRuntimeStartOutput {
        child,
        runtime_origin: SOURCE_CODE_SERVER_EDITOR_ORIGIN.to_string(),
        prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
        started_at,
        http_runtime_ready: readiness.http_runtime_ready,
    })
}

