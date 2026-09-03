// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds remote typed-operation posting, project
// path lookup, and opening remote projects/files/PRs in the IDE or browser.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashSet,
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_gxserver_post_typed_operation(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<(u16, String), String> {
    /*
    CDXC:RemoteMachines 2026-06-24-16:48:
    Remote gxserver RPCs use the live SSH tunnel target and in-memory token captured by Rust after Keychain storage. Keep this helper transport-only and response-unlogged: callers own endpoint allowlists and user-facing generic errors, while tokens, URLs, remote paths, params, stdout/stderr, and daemon bodies are never persisted or copied to renderer globals.
    */
    if !path.starts_with("/api/") {
        return Err("Invalid remote gxserver API path.".to_string());
    }
    let address = format!("127.0.0.1:{}", target.local_port);
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "Remote gxserver tunnel is not reachable.".to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "Could not configure remote gxserver read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "Could not configure remote gxserver write timeout.".to_string())?;

    let body = serde_json::json!({
        "protocolVersion": GPUI_GXSERVER_PROTOCOL_VERSION,
        "params": params,
    })
    .to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\nContent-Length: {}\r\n\r\n{body}",
        target.token.as_str(),
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send remote gxserver request.".to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| "Could not read remote gxserver response.".to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "Remote gxserver returned an invalid HTTP response.".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "Remote gxserver returned an invalid HTTP status.".to_string())?;
    Ok((status_code, gxserver_http_response_body(headers, body)?))
}

pub(crate) fn gpui_remote_gxserver_project_path_by_id(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Result<String, String> {
    /*
    CDXC:RemoteMachines 2026-06-24-19:25:
    Remote project path copy must resolve the path from the owning remote gxserver immediately before writing the local clipboard. React may identify only the saved machine and project id; it must not provide or authorize the remote path string.

    CDXC:RemoteMachines 2026-06-24-20:26:
    Remote IDE opens reuse this resolver so project and changed-file editor launches also receive remote paths only from the owning gxserver, never from React payloads, DOM labels, cached presentation text, or renderer-built URI strings.
    */
    gpui_remote_gxserver_project_path_by_id_from_endpoint(
        target,
        "/api/listProjects",
        "projects",
        project_id,
    )?
    .or_else(|| {
        gpui_remote_gxserver_project_path_by_id_from_endpoint(
            target,
            "/api/listRecentProjects",
            "recentProjects",
            project_id,
        )
        .ok()
        .flatten()
    })
    .ok_or_else(|| "GPUI could not resolve that remote project path.".to_string())
}

pub(crate) fn gpui_remote_gxserver_project_path_by_id_from_endpoint(
    target: &GpuiRemoteGxserverRequestTarget,
    endpoint: &str,
    array_key: &str,
    project_id: &str,
) -> Result<Option<String>, String> {
    let result = gpui_remote_gxserver_rpc_result(
        target,
        endpoint,
        &serde_json::json!({}),
        Duration::from_secs(10),
    )?;
    Ok(result
        .get(array_key)
        .and_then(serde_json::Value::as_array)
        .and_then(|projects| {
            projects
                .iter()
                .filter_map(serde_json::Value::as_object)
                .find_map(|project| {
                    (json_string_field(project, "projectId") == Some(project_id))
                        .then(|| gpui_remote_project_path_from_gxserver_row(project))
                        .flatten()
                })
        }))
}

pub(crate) fn gpui_remote_project_path_from_gxserver_row(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let path = json_string_field(project, "path")?.trim();
    if path.is_empty()
        || path.chars().count() > GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS
        || path.contains('\0')
        || path.chars().any(char::is_control)
    {
        return None;
    }
    Some(path.to_string())
}

pub(crate) fn gpui_open_remote_existing_project_pull_request_in_browser(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Result<(), String> {
    /*
    CDXC:RemoteMachines 2026-06-24-19:25:
    Remote PR browser opens must re-run `prView` through the saved-machine gxserver tunnel and open only a validated HTTPS GitHub pull-request URL. Renderer URLs, cached PR payloads, Browser titles, command text, SSH details, tokens, and daemon bodies are not launch authority.
    */
    let result = gpui_remote_gxserver_rpc_result(
        target,
        "/api/runGitHubAction",
        &serde_json::json!({
            "action": "prView",
            "projectId": project_id,
        }),
        Duration::from_secs(15),
    )?;
    if gpui_typed_operation_exit_code(&result) != Some(0) {
        return Err("No open remote pull request is available for this project.".to_string());
    }
    let url = gpui_trusted_github_pull_request_url_from_pr_view_stdout(
        gpui_typed_operation_stdout(&result),
    )
    .ok_or_else(|| "No open remote pull request is available for this project.".to_string())?;
    gpui_spawn_os_open(std::ffi::OsStr::new(&url))
        .map_err(|_| "GPUI could not open the remote pull request.".to_string())
}

pub(crate) fn gpui_open_remote_project_in_ide(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    action: GpuiSidebarNativeProjectPathAction,
    project_id: &str,
) -> Result<(), String> {
    /*
    CDXC:RemoteMachines 2026-06-24-20:26:
    Remote project IDE opens are native-owned fixed editor launches. Resolve the remote path from the owning gxserver immediately before launch, derive SSH targeting from saved Settings, and support only reviewed VS Code/Insiders argv or Zed/Zeditor URI paths so custom Settings text, renderer paths, local Finder, and local filesystem paths never become remote-open authority.

    CDXC:RemoteMachines 2026-06-24-21:33:
    Posix-host Zed/Zeditor opens use Zed's documented SSH URI CLI form.
    Windows-host WSL opens instead execute the fixed editor CLI inside the
    retained distribution, matching each editor's documented WSL integration.
    The launcher still rejects custom command text, renderer URI strings, and
    local Finder paths.
    */
    let editor_target = match action {
        GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInVscode => {
            GPUI_WORKSPACE_EDITOR_VSCODE_TARGET
        }
        GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInZed => {
            GPUI_WORKSPACE_EDITOR_ZED_TARGET
        }
        GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInIde => {
            gpui_remote_workspace_editor_target_from_default_settings()?
        }
        _ => {
            return Err("Configured editor is not supported for GPUI remote IDE open.".to_string());
        }
    };
    let project_path = gpui_remote_gxserver_project_path_by_id(target, project_id)?;
    gpui_open_remote_path_in_editor(
        config,
        &target.execution_target,
        editor_target,
        project_path.as_str(),
        GpuiRemoteIdePathKind::Folder,
    )
}

pub(crate) fn gpui_open_remote_sidebar_git_changed_file_in_ide(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
    file_path: &str,
) -> Result<(), String> {
    /*
    CDXC:RemoteMachines 2026-06-24-19:25:
    Remote changed-file opens still revalidate the project-relative candidate against current remote gxserver Git state before any editor side effect. Never open a local path for a remote file and never accept renderer-controlled SSH, path, or URI fallback data.

    CDXC:RemoteMachines 2026-06-24-20:26:
    Remote changed-file opens now use the same Rust-owned fixed editor path as remote project opens after revalidating the relative file candidate against fresh remote Git state. Keep custom editors, local remote paths, renderer URI strings, and unreviewed editor protocols unsupported.
    */
    let relative_file_path = gpui_normalized_relative_git_file_path(file_path)
        .ok_or_else(|| "Choose a changed file from the current remote Git review.".to_string())?;
    let changed_files = gpui_remote_project_git_changed_file_paths(target, project_id)?;
    if !changed_files.contains(&relative_file_path) {
        return Err("Choose a changed file from the current remote Git state.".to_string());
    }
    let project_path = gpui_remote_gxserver_project_path_by_id(target, project_id)?;
    let remote_file_path =
        gpui_join_remote_project_relative_path(project_path.as_str(), relative_file_path.as_str())
            .ok_or_else(|| {
                "Choose a changed file from the current remote Git state.".to_string()
            })?;
    let editor_target = gpui_remote_workspace_editor_target_from_default_settings()?;
    gpui_open_remote_path_in_editor(
        config,
        &target.execution_target,
        editor_target,
        remote_file_path.as_str(),
        GpuiRemoteIdePathKind::File,
    )
}

pub(crate) fn gpui_remote_workspace_editor_target_from_default_settings()
-> Result<GpuiWorkspaceEditorTarget, String> {
    let settings = shared_settings::shared_sidebar_settings_snapshot().external_editor_settings();
    match settings.default_editor_command() {
        shared_settings::SharedDefaultEditorCommand::Code => {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        shared_settings::SharedDefaultEditorCommand::CodeInsiders => {
            Ok(GpuiWorkspaceEditorTarget {
                command: "code-insiders",
                app_names: GPUI_WORKSPACE_EDITOR_VSCODE_INSIDERS_APP_NAMES,
                launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
            })
        }
        shared_settings::SharedDefaultEditorCommand::Zed => Ok(GPUI_WORKSPACE_EDITOR_ZED_TARGET),
        shared_settings::SharedDefaultEditorCommand::Zeditor => Ok(GpuiWorkspaceEditorTarget {
            command: "zeditor",
            app_names: GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::ZedCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Other
            if settings.editor_command().trim()
                == shared_settings::DEFAULT_DEFAULT_EDITOR_COMMAND =>
        {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        _ => Err("Configured editor is not supported for GPUI remote IDE open.".to_string()),
    }
}

pub(crate) fn gpui_open_remote_path_in_editor(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
    path_kind: GpuiRemoteIdePathKind,
) -> Result<(), String> {
    if !gpui_remote_ide_path_allowed(remote_path) {
        return Err("Remote IDE open could not resolve a valid remote path.".to_string());
    }
    if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsWsl { .. }
    ) {
        return gpui_open_remote_path_in_windows_wsl_editor(
            config,
            execution_target,
            target,
            remote_path,
            path_kind,
        );
    }
    match target.launch_kind {
        GpuiWorkspaceEditorLaunchKind::VscodeCompatible => {
            gpui_open_remote_path_in_vscode_remote_ssh(config, target, remote_path)
        }
        GpuiWorkspaceEditorLaunchKind::ZedCompatible => {
            gpui_open_remote_path_in_zed_remote_ssh(config, target, remote_path)
        }
        GpuiWorkspaceEditorLaunchKind::DirectPath => {
            Err("Configured editor is not supported for GPUI remote IDE open.".to_string())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteIdePathKind {
    Folder,
    File,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_open_remote_path_in_windows_wsl_editor(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
    path_kind: GpuiRemoteIdePathKind,
) -> Result<(), String> {
    let GpuiRemoteExecutionTarget::WindowsWsl { distribution } = execution_target else {
        return Err("Remote IDE open could not resolve the retained WSL target.".to_string());
    };
    let executable = match (target.launch_kind, target.command) {
        (GpuiWorkspaceEditorLaunchKind::VscodeCompatible, "code" | "code-insiders")
        | (GpuiWorkspaceEditorLaunchKind::ZedCompatible, "zed" | "zeditor") => target.command,
        _ => {
            return Err(
                "Configured editor is not supported for GPUI remote WSL IDE open.".to_string(),
            );
        }
    };
    let quoted_executable = gpui_shell_single_quote(executable);
    let launch_command = match target.launch_kind {
        GpuiWorkspaceEditorLaunchKind::VscodeCompatible => {
            /*
            VS Code's Windows CLI accepts a WSL remote URI. Include the pinned
            distro authority and an explicit folder/file switch so neither the
            Windows host nor filename-extension guessing can reinterpret the
            gxserver-owned Linux path.
            */
            let remote_uri = format!(
                "vscode-remote://wsl+{}{}",
                gpui_percent_encode_remote_ssh_path(distribution),
                gpui_percent_encode_remote_ssh_path(remote_path)
            );
            let uri_switch = match path_kind {
                GpuiRemoteIdePathKind::Folder => "--folder-uri",
                GpuiRemoteIdePathKind::File => "--file-uri",
            };
            format!(
                "{quoted_executable} --reuse-window {uri_switch} {}",
                gpui_shell_single_quote(remote_uri.as_str())
            )
        }
        GpuiWorkspaceEditorLaunchKind::ZedCompatible => {
            /*
            Zed's Windows CLI detects WSL from the invoking environment. Run it
            inside the retained distro rather than sending an SSH URI to the
            native Windows host, which Zed does not support as a remote server.
            */
            format!(
                "{quoted_executable} {}",
                gpui_shell_single_quote(remote_path)
            )
        }
        GpuiWorkspaceEditorLaunchKind::DirectPath => {
            return Err(
                "Configured editor is not supported for GPUI remote WSL IDE open.".to_string(),
            );
        }
    };
    let remote_command = format!(
        "command -v {quoted_executable} >/dev/null 2>&1 || exit 127; exec {launch_command}"
    );
    let result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        remote_command.as_str(),
        Duration::from_secs(15),
    );
    match result.exit_code {
        0 => Ok(()),
        127 => {
            Err("Configured editor is not available in the retained remote WSL target.".to_string())
        }
        _ => Err("Configured editor could not open the retained remote WSL target.".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_open_remote_path_in_windows_wsl_editor(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _target: GpuiWorkspaceEditorTarget,
    _remote_path: &str,
    _path_kind: GpuiRemoteIdePathKind,
) -> Result<(), String> {
    Err("Remote IDE open is unavailable on this platform.".to_string())
}

pub(crate) fn gpui_open_remote_path_in_vscode_remote_ssh(
    config: &GpuiRemoteMachineConfig,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
) -> Result<(), String> {
    if !gpui_remote_ide_path_allowed(remote_path) {
        return Err("Remote IDE open could not resolve a valid remote path.".to_string());
    }
    gpui_remote_ide_open_easy_connect_refusal(config)?;
    let remote_authority = gpui_vscode_remote_ssh_authority(config)?;
    if !gpui_command_exists_on_path(target.command) {
        return Err("Configured editor is not available for GPUI remote IDE open.".to_string());
    }
    let mut command = std::process::Command::new("/usr/bin/env");
    command
        .arg(target.command)
        .arg("--reuse-window")
        .arg("--remote")
        .arg(remote_authority)
        .arg(remote_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open the remote target.".to_string())
}

pub(crate) fn gpui_open_remote_path_in_zed_remote_ssh(
    config: &GpuiRemoteMachineConfig,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
) -> Result<(), String> {
    if !gpui_remote_ide_path_allowed(remote_path) {
        return Err("Remote IDE open could not resolve a valid remote path.".to_string());
    }
    gpui_remote_ide_open_easy_connect_refusal(config)?;
    if config.ssh_identity_file.is_some() {
        return Err(
            "Remote IDE open requires a saved machine that the selected editor can address by host, user, and port."
                .to_string(),
        );
    }
    if !gpui_command_exists_on_path(target.command) {
        return Err("Configured editor is not available for GPUI remote IDE open.".to_string());
    }
    let remote_uri = gpui_zed_remote_ssh_uri(config, remote_path)?;
    let mut command = std::process::Command::new("/usr/bin/env");
    command
        .arg(target.command)
        .arg(remote_uri)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open the remote target.".to_string())
}

/*
CDXC:RemotePairing 2026-09-03:
External editors open remote machines by their own SSH authority. An Easy
Connect machine has none — its endpoint is this app's loopback forwarder,
which lives only while the machine is connected and would point the editor at
127.0.0.1 — so the answer is a plain "not supported" rather than the
host/port-shape rejection an SSH machine gets.
*/
pub(crate) fn gpui_remote_ide_open_easy_connect_refusal(
    config: &GpuiRemoteMachineConfig,
) -> Result<(), String> {
    if config.uses_easy_connect() {
        return Err(
            "Opening a remote project in an external editor is not supported over Easy Connect. Add the machine with SSH details to use this."
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn gpui_zed_remote_ssh_uri(
    config: &GpuiRemoteMachineConfig,
    remote_path: &str,
) -> Result<String, String> {
    gpui_remote_ide_open_easy_connect_refusal(config)?;
    let host = gpui_vscode_remote_ssh_authority_part(config.ssh_host.as_str())
        .ok_or_else(|| "Remote IDE open could not resolve the saved machine host.".to_string())?;
    let user = config
        .ssh_user
        .as_deref()
        .and_then(gpui_vscode_remote_ssh_authority_part);
    let mut authority = match user {
        Some(user) => format!("{user}@{host}"),
        None => host,
    };
    if let Some(port) = config.ssh_port {
        authority.push(':');
        authority.push_str(&port.to_string());
    }
    Ok(format!(
        "ssh://{}{}",
        authority,
        gpui_percent_encode_remote_ssh_path(remote_path)
    ))
}

pub(crate) fn gpui_percent_encode_remote_ssh_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

pub(crate) fn gpui_vscode_remote_ssh_authority(
    config: &GpuiRemoteMachineConfig,
) -> Result<String, String> {
    gpui_remote_ide_open_easy_connect_refusal(config)?;
    if config.ssh_identity_file.is_some() || config.ssh_port.is_some() {
        return Err(
            "Remote IDE open requires a saved machine that VS Code Remote-SSH can address by host and user."
                .to_string(),
        );
    }
    let host = gpui_vscode_remote_ssh_authority_part(config.ssh_host.as_str())
        .ok_or_else(|| "Remote IDE open could not resolve the saved machine host.".to_string())?;
    let authority = config
        .ssh_user
        .as_deref()
        .and_then(gpui_vscode_remote_ssh_authority_part)
        .map(|user| format!("{user}@{host}"))
        .unwrap_or(host);
    Ok(format!("ssh-remote+{authority}"))
}

pub(crate) fn gpui_vscode_remote_ssh_authority_part(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return None;
    }
    Some(value.to_string())
}

pub(crate) fn gpui_remote_ide_path_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with('/')
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_join_remote_project_relative_path(
    project_path: &str,
    relative_file_path: &str,
) -> Option<String> {
    if !gpui_remote_ide_path_allowed(project_path)
        || gpui_normalized_relative_git_file_path(relative_file_path).as_deref()
            != Some(relative_file_path)
    {
        return None;
    }
    if project_path == "/" {
        return Some(format!("/{relative_file_path}"));
    }
    Some(format!(
        "{}/{}",
        project_path.trim_end_matches('/'),
        relative_file_path
    ))
}

pub(crate) fn gpui_remote_project_git_changed_file_paths(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Result<HashSet<String>, String> {
    let status = gpui_remote_gxserver_git_action_result(target, project_id, "statusPorcelain")?;
    if gpui_typed_operation_exit_code(&status) != Some(0) {
        return Err("GPUI could not refresh remote changed files.".to_string());
    }
    let mut files = HashSet::new();
    gpui_collect_git_status_porcelain_paths(gpui_typed_operation_stdout(&status), &mut files);

    let diff = gpui_remote_gxserver_git_action_result(target, project_id, "diffNumstat")?;
    if gpui_typed_operation_exit_code(&diff) == Some(0) {
        gpui_collect_git_numstat_paths(gpui_typed_operation_stdout(&diff), &mut files);
    }

    let untracked = gpui_remote_gxserver_git_action_result(target, project_id, "listUntracked")?;
    if gpui_typed_operation_exit_code(&untracked) == Some(0) {
        gpui_collect_git_zero_delimited_paths(gpui_typed_operation_stdout(&untracked), &mut files);
    }
    Ok(files)
}

pub(crate) fn gpui_remote_gxserver_git_action_result(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
    action: &str,
) -> Result<serde_json::Value, String> {
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/runGitAction",
        &serde_json::json!({
            "action": action,
            "projectId": project_id,
        }),
        Duration::from_secs(15),
    )
}
