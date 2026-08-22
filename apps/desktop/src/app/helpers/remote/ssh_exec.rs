// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the raw remote SSH run helpers, terminal
// attachment/clipboard-image upload, and SSH client option/target-argument
// construction. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    path::Path,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh(
    config: &GpuiRemoteMachineConfig,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    gpui_run_remote_ssh_in_execution_target(
        config,
        &GpuiRemoteExecutionTarget::PosixHost,
        remote_command,
        timeout,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let command = gpui_remote_command_for_execution_target(execution_target, remote_command);
    gpui_run_remote_ssh_raw(config, command.as_str(), timeout)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_in_windows_wsl(
    config: &GpuiRemoteMachineConfig,
    distribution: Option<&str>,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let command = gpui_remote_command_for_windows_wsl(distribution, remote_command);
    gpui_run_remote_ssh_raw(config, command.as_str(), timeout)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_raw(
    config: &GpuiRemoteMachineConfig,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not prepare SSH password helper.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    arguments.push(remote_command.to_string());
    gpui_run_remote_process(
        "/usr/bin/ssh",
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        timeout,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_with_stdin_file_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    stdin_path: &Path,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not prepare SSH password helper.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    arguments.push(gpui_remote_command_for_execution_target(
        execution_target,
        remote_command,
    ));
    gpui_run_remote_process_with_stdin_file(
        "/usr/bin/ssh",
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        stdin_path,
        timeout,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_terminal_attachment_to_remote(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    local_path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    let metadata = fs::metadata(local_path)
        .map_err(|_| "The selected file or folder is no longer available.".to_string())?;
    let kind = if metadata.is_dir() {
        GpuiTerminalAttachmentKind::Folder
    } else if metadata.is_file() && is_project_board_image_file_path(local_path) {
        GpuiTerminalAttachmentKind::Image
    } else if metadata.is_file() {
        GpuiTerminalAttachmentKind::File
    } else {
        return Err("The selected item is not a file or folder.".to_string());
    };
    let filename = gpui_terminal_attachment_sanitized_filename(local_path)?;
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let remote_path = format!(
        "/tmp/ghostex-gpui-attachments/{}-{unique_id}-{filename}",
        std::process::id()
    );
    let quoted_remote_path = gpui_shell_single_quote(remote_path.as_str());
    let remote_command = match kind {
        GpuiTerminalAttachmentKind::Folder => format!(
            "umask 077; mkdir -p /tmp/ghostex-gpui-attachments; mkdir -- {quoted_remote_path} && tar -xzf - -C {quoted_remote_path} --strip-components=1"
        ),
        GpuiTerminalAttachmentKind::Image | GpuiTerminalAttachmentKind::File => format!(
            "umask 077; mkdir -p /tmp/ghostex-gpui-attachments; cat > {quoted_remote_path} && chmod 600 {quoted_remote_path}"
        ),
    };

    let staged_archive;
    let upload_path = if kind == GpuiTerminalAttachmentKind::Folder {
        let parent = local_path
            .parent()
            .ok_or_else(|| "The selected folder has no containing directory.".to_string())?;
        let directory_name = local_path
            .file_name()
            .ok_or_else(|| "The selected folder has no usable name.".to_string())?;
        staged_archive = env::temp_dir().join(format!(
            "ghostex-gpui-terminal-attachment-{}-{unique_id}.tar.gz",
            std::process::id()
        ));
        let archive_status = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(staged_archive.as_path())
            .arg("-C")
            .arg(parent)
            .arg(directory_name)
            .status()
            .map_err(|_| "Could not prepare the selected folder for upload.".to_string())?;
        if !archive_status.success() {
            let _ = fs::remove_file(staged_archive.as_path());
            return Err("Could not prepare the selected folder for upload.".to_string());
        }
        staged_archive.as_path()
    } else {
        local_path
    };

    let upload_result = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        remote_command.as_str(),
        upload_path,
        Duration::from_secs(300),
    );
    if kind == GpuiTerminalAttachmentKind::Folder {
        let _ = fs::remove_file(upload_path);
    }
    if upload_result.exit_code != 0 {
        return Err("Could not upload the selected item to the remote machine.".to_string());
    }
    Ok(GpuiTerminalAttachmentReference {
        kind,
        path: remote_path,
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_upload_terminal_attachment_to_remote(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _local_path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    Err("Remote attachments are unavailable in this GPUI build.".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_terminal_clipboard_image_to_remote(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    payload: TerminalClipboardImagePayload,
) -> Result<Vec<GpuiTerminalAttachmentReference>, String> {
    match payload {
        TerminalClipboardImagePayload::FilePaths(paths) => paths
            .iter()
            .map(|path| {
                gpui_upload_terminal_attachment_to_remote(config, execution_target, path.as_path())
            })
            .collect(),
        TerminalClipboardImagePayload::Bytes { bytes, extension } => {
            let unique_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let staged_image = env::temp_dir().join(format!(
                "ghostex-gpui-clipboard-image-{}-{unique_id}.{extension}",
                std::process::id()
            ));
            fs::write(staged_image.as_path(), &bytes)
                .map_err(|_| "Could not stage the pasted image for upload.".to_string())?;
            let result = gpui_upload_terminal_attachment_to_remote(
                config,
                execution_target,
                staged_image.as_path(),
            );
            let _ = fs::remove_file(staged_image.as_path());
            result.map(|reference| vec![reference])
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_upload_terminal_clipboard_image_to_remote(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _payload: TerminalClipboardImagePayload,
) -> Result<Vec<GpuiTerminalAttachmentReference>, String> {
    Err("Remote image paste is unavailable in this GPUI build.".to_string())
}

pub(crate) fn gpui_terminal_attachment_sanitized_filename(path: &Path) -> Result<String, String> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The selected item has no usable filename.".to_string())?;
    let sanitized = filename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized
        .trim_matches(|character| matches!(character, '.' | '-' | '_'))
        .chars()
        .take(96)
        .collect::<String>();
    if sanitized.is_empty() {
        Ok("attachment".to_string())
    } else {
        Ok(sanitized)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_client_options(has_saved_password: bool) -> Vec<String> {
    /*
    CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
    Password-backed GPUI Remote machines use the same SSH askpass boundary as Swift: disable BatchMode, allow exactly one password prompt, and have the helper read the saved credential from Keychain. Key-only machines keep BatchMode so SSH cannot hang waiting for interactive input.
    */
    let mut arguments = vec![
        "-o".to_string(),
        "UseKeychain=yes".to_string(),
        "-o".to_string(),
        "AddKeysToAgent=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-o".to_string(),
        "TCPKeepAlive=yes".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];
    if has_saved_password {
        arguments.extend([
            "-o".to_string(),
            "BatchMode=no".to_string(),
            "-o".to_string(),
            "NumberOfPasswordPrompts=1".to_string(),
            "-o".to_string(),
            "PreferredAuthentications=publickey,password,keyboard-interactive".to_string(),
            "-o".to_string(),
            "PasswordAuthentication=yes".to_string(),
        ]);
    } else {
        arguments.extend(["-o".to_string(), "BatchMode=yes".to_string()]);
    }
    arguments
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_remote_ssh_client_options(has_saved_password: bool) -> Vec<String> {
    let mut arguments = vec![
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-o".to_string(),
        "TCPKeepAlive=yes".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];
    if has_saved_password {
        arguments.extend(["-o".to_string(), "BatchMode=no".to_string()]);
    } else {
        arguments.extend(["-o".to_string(), "BatchMode=yes".to_string()]);
    }
    arguments
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_target_arguments(config: &GpuiRemoteMachineConfig) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(identity_file) = config.ssh_identity_file.as_ref() {
        arguments.extend(["-i".to_string(), identity_file.clone()]);
    }
    if let Some(port) = config.ssh_port {
        arguments.extend(["-p".to_string(), port.to_string()]);
    }
    arguments.push(config.ssh_target_host());
    arguments
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_remote_ssh_target_arguments(config: &GpuiRemoteMachineConfig) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(identity_file) = config.ssh_identity_file.as_ref() {
        arguments.extend(["-i".to_string(), identity_file.clone()]);
    }
    if let Some(port) = config.ssh_port {
        arguments.extend(["-p".to_string(), port.to_string()]);
    }
    arguments.push(config.ssh_target_host());
    arguments
}

pub(crate) fn gpui_login_shell_remote_command(command: &str) -> String {
    let quoted_command = gpui_shell_single_quote(command);
    format!(
        "if [ -x /bin/zsh ]; then exec /bin/zsh -lic {quoted_command}; elif command -v zsh >/dev/null 2>&1; then exec zsh -lic {quoted_command}; else exec /bin/sh -lc {quoted_command}; fi"
    )
}

