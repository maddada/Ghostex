// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the raw remote SSH run helpers, terminal
// attachment/clipboard-image upload, and SSH client option/target-argument
// construction. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::app::helpers::*;
use crate::*;

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_ssh_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let command = gpui_remote_command_for_execution_target(execution_target, remote_command);
    gpui_run_remote_ssh_raw(config, command.as_str(), timeout)
}

/// CDXC:CodeEditor 2026-09-23 WHY:
/// Native Windows rejects sufficiently large encoded commands before PowerShell starts. Code file/folder scripts have no input payload, so send the script through SSH stdin with a fixed reader instead of depending on compression to fit cmd's limit.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_ssh_script_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    script: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    if !matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        return gpui_run_remote_ssh_in_execution_target(config, execution_target, script, timeout);
    }
    let target_arguments = match gpui_remote_ssh_target_arguments(config) {
        Ok(arguments) => arguments,
        Err(message) => return gpui_remote_ssh_refused_process_result(message),
    };
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(message) => return gpui_remote_ssh_refused_process_result(message),
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_shared_connection_arguments(config));
    arguments.extend(target_arguments);
    arguments.push(gpui_remote_command_for_execution_target(
        execution_target,
        "$gxScript=[Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($gxScript))",
    ));
    gpui_run_remote_process_with_stdin_bytes(
        &gpui_remote_ssh_executable(),
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        script.as_bytes().to_vec(),
        timeout,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_ssh_in_windows_wsl(
    config: &GpuiRemoteMachineConfig,
    distribution: Option<&str>,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let command = gpui_remote_command_for_windows_wsl(distribution, remote_command);
    gpui_run_remote_ssh_raw(config, command.as_str(), timeout)
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_ssh_raw(
    config: &GpuiRemoteMachineConfig,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let target_arguments = match gpui_remote_ssh_target_arguments(config) {
        Ok(target_arguments) => target_arguments,
        Err(message) => return gpui_remote_ssh_refused_process_result(message),
    };
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(message) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: message,
                stdout: String::new(),
            };
        }
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_shared_connection_arguments(config));
    arguments.extend(target_arguments);
    arguments.push(remote_command.to_string());
    gpui_run_remote_process(
        &gpui_remote_ssh_executable(),
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        timeout,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_ssh_with_stdin_file_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    stdin_path: &Path,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let target_arguments = match gpui_remote_ssh_target_arguments(config) {
        Ok(target_arguments) => target_arguments,
        Err(message) => return gpui_remote_ssh_refused_process_result(message),
    };
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(message) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: message,
                stdout: String::new(),
            };
        }
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_shared_connection_arguments(config));
    arguments.extend(target_arguments);
    arguments.push(gpui_remote_command_for_execution_target(
        execution_target,
        remote_command,
    ));
    gpui_run_remote_process_with_stdin_file(
        &gpui_remote_ssh_executable(),
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        stdin_path,
        timeout,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
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
    let quoted_filename = gpui_shell_single_quote(filename.as_str());
    let remote_command = if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        gpui_remote_windows_upload_command(
            &format!("{}-{unique_id}-{filename}", std::process::id()),
            kind == GpuiTerminalAttachmentKind::Folder,
        )
    } else {
        /*
        CDXC:RemoteMachines 2026-10-25 WHY:
        Uploads used to go into a fixed /tmp/ghostex-gpui-attachments folder made with `mkdir -p`, whose owner, mode and symlinks were never checked, so another account on the remote host could create it first and read or swap what the user attached (security audit 2026-10-25). Each upload now gets its own folder from `mktemp -d` (created exclusively, mode 0700, owned by the uploader) and reports the path behind the same marker the Windows upload uses.
        */
        let make_private_dir = "umask 077; d=$(mktemp -d \"${TMPDIR:-/tmp}/ghostex-gpui-attachments.XXXXXXXX\") || exit 1; p=\"$d\"/";
        match kind {
            GpuiTerminalAttachmentKind::Folder => format!(
                "{make_private_dir}{quoted_filename}; mkdir -- \"$p\" && tar -xzf - -C \"$p\" --strip-components=1 && printf '__GHOSTEX_UPLOAD__%s\\n' \"$p\""
            ),
            GpuiTerminalAttachmentKind::Image | GpuiTerminalAttachmentKind::File => format!(
                "{make_private_dir}{quoted_filename}; cat > \"$p\" && chmod 600 \"$p\" && printf '__GHOSTEX_UPLOAD__%s\\n' \"$p\""
            ),
        }
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
        let archive_status = gpui_remote_background_command(&gpui_remote_tar_executable())
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
    let uploaded_path = upload_result
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("__GHOSTEX_UPLOAD__"));
    let remote_path = if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        uploaded_path
            .filter(|path| gpui_is_windows_remote_path(path))
            .ok_or_else(|| "Windows did not return the uploaded path.".to_string())?
    } else {
        uploaded_path
            .filter(|path| path.starts_with('/') && path.ends_with(&format!("/{filename}")))
            .ok_or_else(|| "The remote machine did not return the uploaded path.".to_string())?
    }
    .to_string();
    Ok(GpuiTerminalAttachmentReference {
        kind,
        path: remote_path,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_upload_terminal_attachment_to_remote(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _local_path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    Err("Remote attachments are unavailable in this GPUI build.".to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_upload_terminal_clipboard_image_to_remote(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    payload: TerminalClipboardImagePayload,
) -> Result<Vec<GpuiTerminalAttachmentReference>, String> {
    match payload {
        TerminalClipboardImagePayload::FilePaths(paths) => paths
            .iter()
            .map(|path| {
                let format = match path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "bmp" => Some(gpui::ImageFormat::Bmp),
                    "tif" | "tiff" => Some(gpui::ImageFormat::Tiff),
                    "png" => Some(gpui::ImageFormat::Png),
                    "jpg" | "jpeg" => Some(gpui::ImageFormat::Jpeg),
                    "webp" => Some(gpui::ImageFormat::Webp),
                    "gif" => Some(gpui::ImageFormat::Gif),
                    _ => None,
                };
                if let Some(format) = format {
                    let bytes = fs::read(path)
                        .map_err(|_| "Could not read the pasted image.".to_string())?;
                    let prepared = crate::app::helpers::clipboard_image::prepare_clipboard_image(
                        &bytes, format,
                    )?;
                    gpui_upload_prepared_terminal_image_to_remote(
                        config,
                        execution_target,
                        &prepared.bytes,
                        prepared.extension,
                    )
                } else {
                    gpui_upload_terminal_attachment_to_remote(
                        config,
                        execution_target,
                        path.as_path(),
                    )
                }
            })
            .collect(),
        TerminalClipboardImagePayload::Bytes { bytes, extension } => {
            gpui_upload_prepared_terminal_image_to_remote(
                config,
                execution_target,
                &bytes,
                extension,
            )
            .map(|reference| vec![reference])
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn gpui_upload_prepared_terminal_image_to_remote(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    bytes: &[u8],
    extension: &str,
) -> Result<GpuiTerminalAttachmentReference, String> {
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staged_image = env::temp_dir().join(format!(
        "ghostex-gpui-clipboard-image-{}-{unique_id}.{extension}",
        std::process::id()
    ));
    fs::write(staged_image.as_path(), bytes)
        .map_err(|_| "Could not stage the pasted image for upload.".to_string())?;
    let result =
        gpui_upload_terminal_attachment_to_remote(config, execution_target, staged_image.as_path());
    let _ = fs::remove_file(staged_image.as_path());
    result
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
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

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_ssh_client_options(has_saved_password: bool) -> Vec<String> {
    /*
    CDXC:RemoteMachines 2026-06-24-14:34:
    Password-backed GPUI Remote machines use the same SSH askpass boundary as Swift: disable BatchMode, allow exactly one password prompt, and have the helper read the saved credential from Keychain. Key-only machines keep BatchMode so SSH cannot hang waiting for interactive input.
    */
    let mut arguments = vec![
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
    #[cfg(target_os = "macos")]
    arguments.extend(["-o".to_string(), "UseKeychain=yes".to_string()]);
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

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
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

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_ssh_target_arguments(
    config: &GpuiRemoteMachineConfig,
) -> Result<Vec<String>, String> {
    gpui_remote_ssh_dial_refusal(config)?;
    let mut arguments = Vec::new();
    if let Some(identity_file) = config.ssh_identity_file.as_ref() {
        arguments.extend(["-i".to_string(), identity_file.clone()]);
    }
    if let Some(port) = config.ssh_port {
        arguments.extend(["-p".to_string(), port.to_string()]);
    }
    arguments.extend(gpui_remote_ssh_host_key_alias_arguments(config));
    arguments.push(config.ssh_target_host());
    Ok(arguments)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_remote_ssh_target_arguments(
    config: &GpuiRemoteMachineConfig,
) -> Result<Vec<String>, String> {
    gpui_remote_ssh_dial_refusal(config)?;
    let mut arguments = Vec::new();
    if let Some(identity_file) = config.ssh_identity_file.as_ref() {
        arguments.extend(["-i".to_string(), identity_file.clone()]);
    }
    if let Some(port) = config.ssh_port {
        arguments.extend(["-p".to_string(), port.to_string()]);
    }
    arguments.extend(gpui_remote_ssh_host_key_alias_arguments(config));
    arguments.push(config.ssh_target_host());
    Ok(arguments)
}

/*
CDXC:RemotePairing 2026-09-03:
`ssh ""` is not a no-op: OpenSSH resolves an empty host to the local sshd
(`[::1]:22`), so a config without a host — an Easy Connect machine whose
forwarder is not running, or a corrupted SSH machine — must be refused here,
before any argument list is built, rather than handed to ssh.
*/
pub(crate) fn gpui_remote_ssh_dial_refusal(config: &GpuiRemoteMachineConfig) -> Result<(), String> {
    if config.ssh_host.trim().is_empty() {
        return Err(if config.uses_easy_connect() {
            "The Easy Connect forwarder for that machine is not running. Reconnect the machine first."
                .to_string()
        } else {
            "The saved remote machine has no SSH host.".to_string()
        });
    }
    // CDXC:RemoteMachines 2026-10-25 WHY:
    // The user and host become ssh's destination argument (`user@host`), and a value starting with `-` is read as an option there (`-oProxyCommand=…` runs a command), while whitespace or control characters split or corrupt it. Both can arrive from an Easy Connect code or a hand-edited machine, so they are refused before any argument list is built (security audit 2026-10-25).
    let user = config.ssh_user.as_deref().unwrap_or("");
    if [config.ssh_host.as_str(), user].iter().any(|value| {
        value.starts_with('-')
            || value
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
    }) {
        return Err(
            "The saved remote machine's SSH user or host is not valid. Edit the machine in Settings > Remote."
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn gpui_remote_ssh_refused_process_result(message: String) -> GpuiRemoteProcessResult {
    GpuiRemoteProcessResult {
        exit_code: 126,
        stderr: message,
        stdout: String::new(),
    }
}

/*
CDXC:RemotePairing 2026-09-03:
An Easy Connect machine is dialed as `127.0.0.1:<forwarder port>`, and that
port is not stable across reconnects nor unique across machines. Without an
alias, `StrictHostKeyChecking=accept-new` would record `[127.0.0.1]:<port>`
and refuse the next machine (or the same machine after a port change) as a
changed host key. The alias keys known_hosts by the saved machine instead.
*/
pub(crate) fn gpui_remote_ssh_host_key_alias_arguments(
    config: &GpuiRemoteMachineConfig,
) -> Vec<String> {
    if !config.uses_easy_connect() {
        return Vec::new();
    }
    vec![
        "-o".to_string(),
        format!(
            "HostKeyAlias=ghostex-easy-connect-{}",
            config.remote_machine_id
        ),
    ]
}

pub(crate) fn gpui_login_shell_remote_command(command: &str) -> String {
    let quoted_command = gpui_shell_single_quote(command);
    format!(
        "if [ -x /bin/zsh ]; then exec /bin/zsh -lic {quoted_command}; elif command -v zsh >/dev/null 2>&1; then exec zsh -lic {quoted_command}; else exec /bin/sh -lc {quoted_command}; fi"
    )
}

/// CDXC:RemoteMachines 2026-09-18 WHY:
/// A managed tunnel must own its SSH process and forwarding socket. Reusing a user-configured multiplexing master can make the child exit successfully before the readiness probe and leaves tunnel shutdown tied to an unrelated master.
/// SEE-ALSO: tunnel_and_auth.rs, browser_tunnel.rs, source_code_server.rs.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_ssh_tunnel_options(has_saved_password: bool) -> Vec<String> {
    let mut arguments = gpui_remote_ssh_client_options(has_saved_password);
    arguments.extend([
        "-o".to_string(),
        "ControlMaster=no".to_string(),
        "-o".to_string(),
        "ControlPath=none".to_string(),
        "-o".to_string(),
        "ForkAfterAuthentication=no".to_string(),
    ]);
    arguments
}
