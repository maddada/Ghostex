// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the gxserver connect flow, token/process
// failure classification, execution-target probing, and version/token
// extraction. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{fs, io::Write, path::Path};

use flate2::{Compression, write::GzEncoder};
use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_machine_ssh_port(value: Option<&serde_json::Value>) -> Option<u16> {
    value
        .and_then(serde_json::Value::as_u64)
        .filter(|port| (1..=u16::MAX as u64).contains(port))
        .map(|port| port as u16)
}

pub(crate) fn gpui_expand_remote_identity_file(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" {
        return gpui_path_string(&home_dir());
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/") {
        return gpui_path_string(&home_dir().join(relative_path));
    }
    trimmed.to_string()
}

pub(crate) fn gpui_connect_remote_gxserver(
    config: GpuiRemoteMachineConfig,
    install_approved: bool,
    progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    gpui_connect_remote_gxserver_platform(config, install_approved, progress_tx)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_connect_remote_gxserver_platform(
    _config: GpuiRemoteMachineConfig,
    _install_approved: bool,
    _progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    GpuiRemoteGxserverConnectResult::without_connection(
        GpuiRemoteGxserverConnectState::Unsupported,
        "Remote gxserver connect from Settings is only available in the macOS GPUI build.",
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_connect_remote_gxserver_platform(
    config: GpuiRemoteMachineConfig,
    install_approved: bool,
    progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    // macOS RemoteGxserverInstallDebugLog parity: record the connect/install
    // lifecycle with bounded machine id + state enums only (no hosts, users,
    // ports, paths, tokens, or process output).
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.connectStarted",
        serde_json::json!({
            "installApproved": install_approved,
            "machineId": config.remote_machine_id,
        }),
    );
    let result = gpui_connect_remote_gxserver_platform_inner(config, install_approved, progress_tx);
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        if matches!(result.state, GpuiRemoteGxserverConnectState::Connected) {
            "gpui.remoteGxserver.connectFinished"
        } else {
            "gpui.remoteGxserver.connectFailed"
        },
        serde_json::json!({ "state": result.state.support_log_state() }),
    );
    result
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_connect_remote_gxserver_platform_inner(
    config: GpuiRemoteMachineConfig,
    install_approved: bool,
    progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    if config.ssh_host.trim().is_empty() || config.remote_machine_id.trim().is_empty() {
        return GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::Invalid,
            "The saved remote machine is missing required SSH settings.",
        );
    }
    let mut execution_target = GpuiRemoteExecutionTarget::PosixHost;
    let mut token_result = gpui_run_remote_ssh(
        &config,
        gpui_remote_token_read_command(),
        GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
    );
    if token_result.exit_code != 0 {
        /*
        CDXC:GPUIRemoteWindowsWsl 2026-07-26:
        Keep the established Unix SSH command byte-for-byte as the first
        attempt. When that cannot run, identify the SSH host boundary and, for
        Windows OpenSSH, move every Linux runtime operation into the selected
        or default WSL distro before interpreting the gxserver exit contract.
        */
        match gpui_probe_remote_execution_target(&config) {
            Ok(target @ GpuiRemoteExecutionTarget::WindowsWsl { .. }) => {
                token_result = gpui_run_remote_ssh_in_execution_target(
                    &config,
                    &target,
                    gpui_remote_token_read_command(),
                    GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
                );
                execution_target = target;
            }
            Ok(GpuiRemoteExecutionTarget::PosixHost) => {}
            Err(GpuiRemoteExecutionTargetProbeError::Ssh(probe_result)) => {
                return GpuiRemoteGxserverConnectResult::without_connection(
                    GpuiRemoteGxserverConnectState::SshFailed,
                    gpui_remote_sanitized_process_failure(
                        "Remote gxserver SSH setup failed.",
                        &probe_result,
                    )
                    .as_str(),
                );
            }
            Err(GpuiRemoteExecutionTargetProbeError::Unsupported(message)) => {
                if gpui_remote_process_failure_is_ssh_transport(&token_result) {
                    return GpuiRemoteGxserverConnectResult::without_connection(
                        GpuiRemoteGxserverConnectState::SshFailed,
                        gpui_remote_sanitized_process_failure(
                            "Remote gxserver SSH setup failed.",
                            &token_result,
                        )
                        .as_str(),
                    );
                }
                return GpuiRemoteGxserverConnectResult::without_connection(
                    GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform,
                    message.as_str(),
                );
            }
        }
    }
    let installed_managed_package_needs_update = token_result.exit_code == 0
        && gpui_remote_managed_gxserver_package_needs_update(&config, &execution_target);
    if (token_result.exit_code == 127 && install_approved) || installed_managed_package_needs_update
    {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-20:08:
        Approved GPUI Remote installs must be native-owned and packaged-only: after SSH reports gxserver missing, Rust probes the remote OS/CPU, selects a matching app-bundled gxserver package, uploads it over the saved SSH configuration, installs/starts it, and then reuses the existing token/Keychain/tunnel path. Development checkout paths and renderer-provided SSH details are not runtime fallbacks.

        An existing Ghostex-managed package is updated without another install
        approval when its sealed build identity differs from this app's bundled
        package. Leaving a protocol-compatible but stale CLI active can make
        interactive attach use storage contracts that no longer match the
        daemon that produced the token.
        */
        match gpui_install_bundled_remote_gxserver_and_read_token(
            &config,
            &execution_target,
            progress_tx.as_ref(),
        ) {
            Ok(install_result) => {
                if install_result.exit_code != 0 {
                    return GpuiRemoteGxserverConnectResult::without_connection(
                        GpuiRemoteGxserverConnectState::InstallFailed,
                        gpui_remote_sanitized_process_failure(
                            "Remote gxserver install failed.",
                            &install_result,
                        )
                        .as_str(),
                    );
                }
                token_result = install_result;
            }
            Err(result) => return result,
        }
    }
    if let Some(state) =
        gpui_remote_token_read_failure_state(token_result.exit_code, install_approved)
    {
        return GpuiRemoteGxserverConnectResult::without_connection(
            state,
            gpui_remote_token_read_failure_message(state, &token_result).as_str(),
        );
    }

    let token = gpui_extract_remote_gxserver_token(token_result.stdout.as_str());
    if !gpui_is_valid_remote_gxserver_token(token.as_deref().unwrap_or_default()) {
        return GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::TokenUnavailable,
            "Remote gxserver token was not readable after SSH start.",
        );
    }
    let token = token.unwrap_or_default();
    match gpui_save_remote_gxserver_token_to_keychain(&config.remote_machine_id, token.as_str()) {
        GpuiRemoteTokenKeychainResult::Success => {}
        GpuiRemoteTokenKeychainResult::Unsupported => {
            return GpuiRemoteGxserverConnectResult::without_connection(
                GpuiRemoteGxserverConnectState::Unsupported,
                "Remote gxserver token storage is only available on macOS.",
            );
        }
        GpuiRemoteTokenKeychainResult::Failed => {
            return GpuiRemoteGxserverConnectResult::without_connection(
                GpuiRemoteGxserverConnectState::KeychainFailed,
                "Could not store the remote gxserver token in Keychain.",
            );
        }
    }

    match gpui_open_remote_gxserver_tunnel(&config, &execution_target, token.as_str()) {
        Ok(connection) => GpuiRemoteGxserverConnectResult::connected(connection),
        Err(message) => GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::TunnelFailed,
            message.as_str(),
        ),
    }
}

pub(crate) fn gpui_remote_token_read_failure_state(
    exit_code: i32,
    install_approved: bool,
) -> Option<GpuiRemoteGxserverConnectState> {
    match (exit_code, install_approved) {
        (0, _) => None,
        (127, false) => Some(GpuiRemoteGxserverConnectState::InstallApprovalRequired),
        (127, true) => Some(GpuiRemoteGxserverConnectState::InstallFailed),
        _ => Some(GpuiRemoteGxserverConnectState::SshFailed),
    }
}

pub(crate) fn gpui_remote_token_read_failure_message(
    state: GpuiRemoteGxserverConnectState,
    result: &GpuiRemoteProcessResult,
) -> String {
    match state {
        GpuiRemoteGxserverConnectState::InstallApprovalRequired => {
            "gxserver is not installed on that machine. Ask before installing the remote gxserver package.".to_string()
        }
        GpuiRemoteGxserverConnectState::InstallFailed => {
            "Remote gxserver install failed.".to_string()
        }
        GpuiRemoteGxserverConnectState::SshFailed => gpui_remote_sanitized_process_failure(
            "Remote gxserver SSH setup failed.",
            result,
        ),
        _ => "Remote gxserver connect failed.".to_string(),
    }
}

pub(crate) fn gpui_remote_sanitized_process_failure(
    default_message: &str,
    result: &GpuiRemoteProcessResult,
) -> String {
    let stderr = result.stderr.trim().to_ascii_lowercase();
    if stderr.contains("saved ssh password") || stderr.contains("ssh password helper") {
        return "Ghostex could not read the saved SSH password from Keychain. Open Remote settings and save the password again.".to_string();
    }
    if stderr.is_empty() {
        if result.exit_code == 124 {
            return "The SSH connection to the remote machine timed out.".to_string();
        }
        return default_message.to_string();
    }
    if stderr.contains("permission denied") {
        return "SSH authentication failed for the remote machine.".to_string();
    }
    if stderr.contains("could not resolve hostname") {
        return "SSH could not resolve the remote host.".to_string();
    }
    if result.exit_code == 124
        || stderr.contains("operation timed out")
        || stderr.contains("connection timed out")
        || stderr.contains("command timed out")
    {
        return "SSH connection to the remote machine timed out.".to_string();
    }
    default_message.to_string()
}

pub(crate) fn gpui_remote_process_stderr_category(
    result: &GpuiRemoteProcessResult,
) -> &'static str {
    let stderr = result.stderr.trim().to_ascii_lowercase();
    if stderr.contains("saved ssh password") {
        return "savedPasswordUnavailable";
    }
    if stderr.contains("ssh password helper") {
        return "passwordHelperFailed";
    }
    if stderr.contains("permission denied") {
        return "authenticationFailed";
    }
    if result.exit_code == 124
        || stderr.contains("operation timed out")
        || stderr.contains("connection timed out")
        || stderr.contains("command timed out")
    {
        return "timedOut";
    }
    if stderr.contains("could not resolve hostname") {
        return "hostResolutionFailed";
    }
    if stderr.contains("connection refused") {
        return "connectionRefused";
    }
    if stderr.contains("host key verification failed") {
        return "hostKeyVerificationFailed";
    }
    if stderr.contains("no route to host") {
        return "noRouteToHost";
    }
    if stderr.is_empty() {
        return "none";
    }
    if result.exit_code == 255 {
        return "sshExit255";
    }
    "other"
}

pub(crate) fn gpui_remote_process_failure_is_ssh_transport(
    result: &GpuiRemoteProcessResult,
) -> bool {
    if matches!(result.exit_code, 124 | 255) {
        return true;
    }
    let stderr = result.stderr.trim().to_ascii_lowercase();
    [
        "saved ssh password",
        "ssh password helper",
        "permission denied",
        "could not resolve hostname",
        "connection refused",
        "connection timed out",
        "host key verification failed",
        "no route to host",
        "operation timed out",
    ]
    .iter()
    .any(|needle| stderr.contains(needle))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_probe_remote_execution_target(
    config: &GpuiRemoteMachineConfig,
) -> Result<GpuiRemoteExecutionTarget, GpuiRemoteExecutionTargetProbeError> {
    /*
    A direct SSH endpoint inside WSL can expose cmd.exe through WSL interop.
    Positively identify the SSH login environment first so that endpoint stays
    a POSIX host. Only a login environment that cannot produce the marked
    uname payload is eligible for native Windows detection.
    */
    let posix_probe = gpui_run_remote_ssh_raw(
        config,
        gpui_remote_install_target_probe_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.executionTargetProbe",
        serde_json::json!({
            "exitCode": posix_probe.exit_code,
            "markedOutput": gpui_remote_install_target_probe_is_marked(&posix_probe.stdout),
            "phase": "posix",
            "stderrCategory": gpui_remote_process_stderr_category(&posix_probe),
            "stderrPresent": !posix_probe.stderr.trim().is_empty(),
        }),
    );
    if posix_probe.exit_code == 0 && gpui_remote_install_target_probe_is_marked(&posix_probe.stdout)
    {
        if let Some(target) = gpui_extract_remote_install_target(posix_probe.stdout.as_str()) {
            if matches!(target.normalized_os().as_str(), "darwin" | "linux") {
                return Ok(GpuiRemoteExecutionTarget::PosixHost);
            }
            return Err(GpuiRemoteExecutionTargetProbeError::Unsupported(format!(
                "Remote platform {} is unsupported. Ghostex remote setup supports macOS, Linux, and Windows through WSL2.",
                target.display_label()
            )));
        }
    }
    if gpui_remote_process_failure_is_ssh_transport(&posix_probe) {
        return Err(GpuiRemoteExecutionTargetProbeError::Ssh(posix_probe));
    }

    let windows_probe = gpui_run_remote_ssh_raw(
        config,
        "cmd.exe /d /s /c \"echo __GHOSTEX_REMOTE_WINDOWS__\"",
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.executionTargetProbe",
        serde_json::json!({
            "exitCode": windows_probe.exit_code,
            "markedOutput": windows_probe
                .stdout
                .lines()
                .any(|line| line.trim() == "__GHOSTEX_REMOTE_WINDOWS__"),
            "phase": "windows",
            "stderrCategory": gpui_remote_process_stderr_category(&windows_probe),
            "stderrPresent": !windows_probe.stderr.trim().is_empty(),
        }),
    );
    if windows_probe.exit_code != 0
        || !windows_probe
            .stdout
            .lines()
            .any(|line| line.trim() == "__GHOSTEX_REMOTE_WINDOWS__")
    {
        if gpui_remote_process_failure_is_ssh_transport(&windows_probe) {
            return Err(GpuiRemoteExecutionTargetProbeError::Ssh(windows_probe));
        }
        return Err(GpuiRemoteExecutionTargetProbeError::Unsupported(
            "Ghostex could not identify the remote SSH login environment. Remote setup supports macOS, Linux, and native Windows OpenSSH through WSL2."
                .to_string(),
        ));
    }

    /*
    A blank saved distribution means "the default for a new connection", but
    no command in an active connection may consult that mutable default again.
    Enter it once, read WSL_DISTRO_NAME from that exact instance, validate the
    canonical name, and retain it in the execution target.
    */
    let wsl_probe_command = gpui_remote_wsl_target_probe_command();
    let wsl_probe = gpui_run_remote_ssh_in_windows_wsl(
        config,
        config.wsl_distribution.as_deref(),
        wsl_probe_command.as_str(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.executionTargetProbe",
        serde_json::json!({
            "exitCode": wsl_probe.exit_code,
            "markedOutput": gpui_remote_install_target_probe_is_marked(&wsl_probe.stdout),
            "phase": "wsl",
            "stderrCategory": gpui_remote_process_stderr_category(&wsl_probe),
            "stderrPresent": !wsl_probe.stderr.trim().is_empty(),
        }),
    );
    if wsl_probe.exit_code != 0
        || !gpui_remote_install_target_probe_is_marked(&wsl_probe.stdout)
        || !gpui_extract_remote_install_target(wsl_probe.stdout.as_str())
            .is_some_and(|target| target.normalized_os() == "linux")
    {
        if gpui_remote_process_failure_is_ssh_transport(&wsl_probe) {
            return Err(GpuiRemoteExecutionTargetProbeError::Ssh(wsl_probe));
        }
        return Err(GpuiRemoteExecutionTargetProbeError::Unsupported(
            gpui_remote_wsl_unavailable_message(config),
        ));
    }

    let distribution = gpui_extract_remote_wsl_distribution(wsl_probe.stdout.as_str())
        .filter(|distribution| gpui_remote_wsl_distribution_is_valid(distribution))
        .filter(|distribution| {
            config
                .wsl_distribution
                .as_deref()
                .is_none_or(|requested| requested.eq_ignore_ascii_case(distribution))
        })
        .ok_or_else(|| {
            GpuiRemoteExecutionTargetProbeError::Unsupported(gpui_remote_wsl_unavailable_message(
                config,
            ))
        })?;
    Ok(GpuiRemoteExecutionTarget::WindowsWsl { distribution })
}

pub(crate) fn gpui_remote_wsl_unavailable_message(config: &GpuiRemoteMachineConfig) -> String {
    match config.wsl_distribution.as_deref() {
        Some(distribution) => format!(
            "Windows remote setup could not start the selected WSL distribution '{distribution}'. Initialize that WSL2 distribution or choose its exact name in Remote settings, then reconnect."
        ),
        None => "Windows remote setup requires an initialized default WSL2 Linux distribution. Initialize WSL2 or select a distribution in Remote settings, then reconnect.".to_string(),
    }
}

pub(crate) fn gpui_remote_command_for_execution_target(
    target: &GpuiRemoteExecutionTarget,
    command: &str,
) -> String {
    match target {
        GpuiRemoteExecutionTarget::PosixHost => gpui_login_shell_remote_command(command),
        GpuiRemoteExecutionTarget::WindowsWsl { distribution } => {
            gpui_remote_command_for_windows_wsl(Some(distribution.as_str()), command)
        }
    }
}

pub(crate) fn gpui_remote_command_for_windows_wsl(
    distribution: Option<&str>,
    command: &str,
) -> String {
    /*
    Compress and encode the POSIX login-shell program so PowerShell and cmd
    only parse a short, fixed WSL argv shape. Native Windows OpenSSH rejects
    long non-interactive exec requests before WSL starts them. Decode into a
    private, no-clobber script instead of piping the program into sh: the
    latter would consume SSH stdin and make package streaming or interactive
    attach impossible. Omission of --distribution is allowed only for the
    one-time default-distro probe; retained targets always pass the validated
    canonical name.
    */
    let login_command = gpui_login_shell_remote_command(command);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(login_command.as_bytes())
        .expect("gzip encoding into memory must succeed");
    let compressed_command = encoder
        .finish()
        .expect("gzip encoding into memory must finish");
    let encoded_command = project_board_base64_encode(compressed_command.as_slice());
    let script_path = format!(
        "/tmp/ghostex-remote-{}-{}.sh",
        std::process::id(),
        gpui_remote_install_unique_id()
    );
    /*
    Windows OpenSSH preserves quote characters in the remote command passed
    to wsl.exe, so `--distribution "Name"` asks WSL for a distro whose name
    literally contains quotes. Validated ordinary distro names are already
    safe to place directly in this fixed command shape.
    */
    let distribution_argument = distribution
        .map(|value| format!(" --distribution {value}"))
        .unwrap_or_default();
    format!(
        "wsl.exe{distribution_argument} --exec /bin/sh -lc \"umask 077; set -C; echo {encoded_command} | base64 -d | gzip -dc > {script_path} || exit 126; trap '/bin/rm -f {script_path}' EXIT HUP INT TERM; /bin/sh {script_path}\""
    )
}

pub(crate) fn gpui_remote_wsl_target_probe_command() -> String {
    format!(
        "printf '__GHOSTEX_REMOTE_WSL_DISTRO_START__\\n%s\\n__GHOSTEX_REMOTE_WSL_DISTRO_END__\\n' \"${{WSL_DISTRO_NAME:-}}\"; {}",
        gpui_remote_install_target_probe_command()
    )
}

pub(crate) fn gpui_extract_remote_wsl_distribution(stdout: &str) -> Option<String> {
    let start_marker = "__GHOSTEX_REMOTE_WSL_DISTRO_START__";
    let end_marker = "__GHOSTEX_REMOTE_WSL_DISTRO_END__";
    let start = stdout.find(start_marker)? + start_marker.len();
    let end = stdout[start..].find(end_marker)?;
    let distribution = stdout[start..start + end].trim();
    (!distribution.is_empty()).then(|| distribution.to_string())
}

pub(crate) fn gpui_remote_install_target_probe_is_marked(stdout: &str) -> bool {
    stdout.contains("__GHOSTEX_REMOTE_PLATFORM_START__")
        && stdout.contains("__GHOSTEX_REMOTE_PLATFORM_END__")
}

pub(crate) fn gpui_extract_remote_gxserver_token(stdout: &str) -> Option<String> {
    if let Some(start) = stdout.find(GPUI_REMOTE_GXSERVER_TOKEN_START_MARKER) {
        let token_start = start + GPUI_REMOTE_GXSERVER_TOKEN_START_MARKER.len();
        if let Some(relative_end) =
            stdout[token_start..].find(GPUI_REMOTE_GXSERVER_TOKEN_END_MARKER)
        {
            let token = stdout[token_start..token_start + relative_end].trim();
            return (!token.is_empty()).then(|| token.to_string());
        }
    }
    gpui_first_remote_gxserver_token_like_run(stdout).or_else(|| {
        let trimmed = stdout.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub(crate) fn gpui_first_remote_gxserver_token_like_run(value: &str) -> Option<String> {
    let mut run_start: Option<usize> = None;
    for (index, ch) in value.char_indices() {
        if gpui_is_remote_gxserver_token_char(ch) {
            if run_start.is_none() {
                run_start = Some(index);
            }
            continue;
        }
        if let Some(start) = run_start.take() {
            if index - start >= 32 {
                return Some(value[start..index].to_string());
            }
        }
    }
    let start = run_start?;
    (value.len() - start >= 32).then(|| value[start..].to_string())
}

pub(crate) fn gpui_is_valid_remote_gxserver_token(token: &str) -> bool {
    token.chars().count() >= 32 && token.chars().all(gpui_is_remote_gxserver_token_char)
}

pub(crate) fn gpui_is_remote_gxserver_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

pub(crate) fn gpui_remote_token_read_command() -> &'static str {
    /*
    Remote packages released before the platform-storage migration remain
    runnable from their installed legacy root. Bind the token path to the
    binary that actually owns the remote daemon; current packages use the
    native State/Data contract, while an explicitly legacy-installed binary
    keeps its matching token contract until that remote package is upgraded.
    */
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"$GHOSTEX_HOME/state\"; GHOSTEX_REMOTE_DATA_DIR=\"$GHOSTEX_HOME\";; ",
        "*) case \"${XDG_STATE_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"${XDG_STATE_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_STATE_DIR=\"$HOME/.local/state/ghostex\";; esac; ",
        "case \"${XDG_DATA_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"${XDG_DATA_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_DATA_DIR=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_TOKEN_FILE=\"$GHOSTEX_REMOTE_STATE_DIR/gxserver/auth/token\"; ",
        "GHOSTEX_REMOTE_LEGACY_ROOT=\"$HOME/.ghostex/gxserver\"; ",
        "GXSERVER_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/gxserver\"; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\" ]; then GXSERVER_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$HOME/.local/bin/gxserver\" ]; then GXSERVER_BIN=\"$HOME/.local/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/gxserver\" ]; then GXSERVER_BIN=\"/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/gxserver\"; fi; ",
        "GHOSTEX_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/ghostex\"; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\" ]; then GHOSTEX_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\"; fi; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$HOME/.local/bin/ghostex\" ]; then GHOSTEX_BIN=\"$HOME/.local/bin/ghostex\"; fi; ",
        "GHOSTEX_REMOTE_START_FAILED=0; ",
        "if [ -x \"$GXSERVER_BIN\" ]; then ",
        "GHOSTEX_REMOTE_COMMAND_BIN=\"$GXSERVER_BIN\"; ",
        "\"$GXSERVER_BIN\" start --json >/dev/null 2>&1 || \"$GXSERVER_BIN\" start >/dev/null 2>&1 || GHOSTEX_REMOTE_START_FAILED=1; ",
        "elif [ -x \"$GHOSTEX_BIN\" ]; then ",
        "GHOSTEX_REMOTE_COMMAND_BIN=\"$GHOSTEX_BIN\"; ",
        "\"$GHOSTEX_BIN\" server start --json >/dev/null 2>&1 || \"$GHOSTEX_BIN\" server start >/dev/null 2>&1 || GHOSTEX_REMOTE_START_FAILED=1; ",
        "else exit 127; fi; ",
        "GHOSTEX_REMOTE_COMMAND_LINK=\"$(readlink \"$GHOSTEX_REMOTE_COMMAND_BIN\" 2>/dev/null || true)\"; ",
        "case \"$GHOSTEX_REMOTE_COMMAND_BIN|$GHOSTEX_REMOTE_COMMAND_LINK\" in *\"$GHOSTEX_REMOTE_LEGACY_ROOT/\"*) GHOSTEX_REMOTE_TOKEN_FILE=\"$GHOSTEX_REMOTE_LEGACY_ROOT/auth/token\";; esac; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_TOKEN_FILE\" ]; then if [ \"$GHOSTEX_REMOTE_START_FAILED\" = \"1\" ]; then exit 127; fi; exit 126; fi; ",
        "printf '__GHOSTEX_REMOTE_TOKEN_START__\\n'; ",
        "cat \"$GHOSTEX_REMOTE_TOKEN_FILE\"; ",
        "printf '\\n__GHOSTEX_REMOTE_TOKEN_END__\\n'"
    )
}

pub(crate) fn gpui_remote_windows_wsl_gxserver_owner_command() -> &'static str {
    /*
    Windows WSL2 stops a distribution after its last Windows-owned execution
    ends, even when gxserver detached successfully inside Linux. Keep the SSH
    tunnel's remote command attached to the exact gxserver pid that produced
    the saved token. The command neither restarts nor substitutes for gxserver:
    it exits when that daemon exits, so the tunnel and WSL lifetime share one
    honest owner.
    */
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"$GHOSTEX_HOME/state\";; ",
        "*) case \"${XDG_STATE_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"${XDG_STATE_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_STATE_DIR=\"$HOME/.local/state/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_TOKEN_FILE=\"$GHOSTEX_REMOTE_STATE_DIR/gxserver/auth/token\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_TOKEN_FILE\" ] && [ -r \"$HOME/.ghostex/gxserver/auth/token\" ]; then GHOSTEX_REMOTE_TOKEN_FILE=\"$HOME/.ghostex/gxserver/auth/token\"; fi; ",
        "GHOSTEX_REMOTE_RUNTIME_FILE=\"${GHOSTEX_REMOTE_TOKEN_FILE%/auth/token}/runtime/server.json\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_RUNTIME_FILE\" ]; then exit 1; fi; ",
        "GHOSTEX_REMOTE_GXSERVER_PID=\"$(sed -n 's/.*\"pid\"[[:space:]]*:[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p' \"$GHOSTEX_REMOTE_RUNTIME_FILE\" | head -n 1)\"; ",
        "case \"$GHOSTEX_REMOTE_GXSERVER_PID\" in ''|*[!0-9]*) exit 1;; esac; ",
        "kill -0 \"$GHOSTEX_REMOTE_GXSERVER_PID\" 2>/dev/null || exit 1; ",
        "while kill -0 \"$GHOSTEX_REMOTE_GXSERVER_PID\" 2>/dev/null; do sleep 5; done"
    )
}

pub(crate) fn gpui_remote_managed_gxserver_build_identity_command() -> &'static str {
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"$GHOSTEX_HOME\";; ",
        "*) case \"${XDG_DATA_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"${XDG_DATA_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_DATA_DIR=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_IDENTITY_FILE=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/build-identity.json\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ] && [ -r \"$HOME/.ghostex/gxserver/package/build-identity.json\" ]; then GHOSTEX_REMOTE_IDENTITY_FILE=\"$HOME/.ghostex/gxserver/package/build-identity.json\"; fi; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ]; then exit 3; fi; ",
        "printf '__GHOSTEX_REMOTE_BUILD_IDENTITY_START__\\n'; ",
        "cat \"$GHOSTEX_REMOTE_IDENTITY_FILE\"; ",
        "printf '\\n__GHOSTEX_REMOTE_BUILD_IDENTITY_END__\\n'"
    )
}

pub(crate) fn gpui_extract_remote_managed_gxserver_build_identity(stdout: &str) -> Option<String> {
    let start = stdout.find(GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_START_MARKER)?
        + GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_START_MARKER.len();
    let payload = &stdout[start..];
    let end = payload.find(GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_END_MARKER)?;
    serde_json::from_str::<serde_json::Value>(payload[..end].trim())
        .ok()?
        .get("buildIdentity")?
        .as_str()
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .map(str::to_string)
}

pub(crate) fn gpui_bundled_remote_gxserver_build_identity(package_dir: &Path) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(
        &fs::read_to_string(package_dir.join("build-identity.json")).ok()?,
    )
    .ok()?
    .get("buildIdentity")?
    .as_str()
    .map(str::trim)
    .filter(|identity| !identity.is_empty())
    .map(str::to_string)
}

pub(crate) fn gpui_remote_installed_gxserver_version_command() -> &'static str {
    /*
    CDXC:RemoteMachines 2026-08-19:
    Settings only needs to know whether the saved machine already carries a
    gxserver package and which version that package is, so this probe reads the
    installed package identity (or asks an already installed binary for its
    version) and never starts, installs, or upgrades anything.

    Resolve the same binaries the token command owns, including the copy inside
    an installed macOS Ghostex.app, or a macOS remote that runs gxserver from
    its own app bundle reads as "not installed". The package identity is taken
    from the resolved binary's own package root (following a ~/.local/bin
    symlink) so the reported version always describes the gxserver that would
    actually serve this machine.
    */
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"$GHOSTEX_HOME\";; ",
        "*) case \"${XDG_DATA_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"${XDG_DATA_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_DATA_DIR=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_LEGACY_ROOT=\"$HOME/.ghostex/gxserver\"; ",
        "GHOSTEX_REMOTE_APP_ROOT=\"/Applications/Ghostex.app/Contents/Resources/Web/gxserver\"; ",
        "GXSERVER_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/gxserver\"; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\" ]; then GXSERVER_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$HOME/.local/bin/gxserver\" ]; then GXSERVER_BIN=\"$HOME/.local/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_APP_ROOT/bin/gxserver\" ]; then GXSERVER_BIN=\"$GHOSTEX_REMOTE_APP_ROOT/bin/gxserver\"; fi; ",
        "GHOSTEX_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/ghostex\"; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\" ]; then GHOSTEX_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\"; fi; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$HOME/.local/bin/ghostex\" ]; then GHOSTEX_BIN=\"$HOME/.local/bin/ghostex\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ ! -x \"$GHOSTEX_BIN\" ]; then exit 3; fi; ",
        "GHOSTEX_REMOTE_RESOLVED_BIN=\"$GXSERVER_BIN\"; ",
        "if [ ! -x \"$GHOSTEX_REMOTE_RESOLVED_BIN\" ]; then GHOSTEX_REMOTE_RESOLVED_BIN=\"$GHOSTEX_BIN\"; fi; ",
        "GHOSTEX_REMOTE_BIN_LINK=\"$(readlink \"$GHOSTEX_REMOTE_RESOLVED_BIN\" 2>/dev/null || true)\"; ",
        "case \"$GHOSTEX_REMOTE_BIN_LINK\" in /*) GHOSTEX_REMOTE_RESOLVED_BIN=\"$GHOSTEX_REMOTE_BIN_LINK\";; ?*) GHOSTEX_REMOTE_RESOLVED_BIN=\"$(dirname \"$GHOSTEX_REMOTE_RESOLVED_BIN\")/$GHOSTEX_REMOTE_BIN_LINK\";; esac; ",
        "GHOSTEX_REMOTE_IDENTITY_FILE=\"$(dirname \"$(dirname \"$GHOSTEX_REMOTE_RESOLVED_BIN\")\")/build-identity.json\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ] && [ -r \"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/build-identity.json\" ]; then GHOSTEX_REMOTE_IDENTITY_FILE=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/build-identity.json\"; fi; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ] && [ -r \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/build-identity.json\" ]; then GHOSTEX_REMOTE_IDENTITY_FILE=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/build-identity.json\"; fi; ",
        "printf '__GHOSTEX_REMOTE_GXSERVER_VERSION_START__\\n'; ",
        "if [ -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ]; then cat \"$GHOSTEX_REMOTE_IDENTITY_FILE\"; elif [ -x \"$GXSERVER_BIN\" ]; then \"$GXSERVER_BIN\" --version 2>/dev/null || true; fi; ",
        "printf '\\n__GHOSTEX_REMOTE_GXSERVER_VERSION_END__\\n'"
    )
}

pub(crate) fn gpui_extract_remote_installed_gxserver_version(stdout: &str) -> Option<String> {
    let start = stdout.find(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER)?
        + GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER.len();
    let payload = &stdout[start..];
    let end = payload.find(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_END_MARKER)?;
    let payload = payload[..end].trim();
    if payload.is_empty() {
        return None;
    }
    if let Ok(identity) = serde_json::from_str::<serde_json::Value>(payload) {
        /*
        Ghostex-managed packages ship build-identity.json, where the version is
        either its own field or the middle segment of `gxserver:<version>:<fingerprint>`.
        */
        if let Some(version) = identity
            .get("packageVersion")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_sanitized_remote_gxserver_version)
        {
            return Some(version);
        }
        return identity
            .get("buildIdentity")
            .and_then(serde_json::Value::as_str)
            .and_then(|identity| identity.split(':').nth(1))
            .and_then(gpui_sanitized_remote_gxserver_version);
    }
    gpui_sanitized_remote_gxserver_version(payload.lines().next().unwrap_or_default())
}

pub(crate) fn gpui_sanitized_remote_gxserver_version(raw: &str) -> Option<String> {
    let version = raw
        .trim()
        .strip_prefix("gxserver")
        .unwrap_or(raw)
        .trim()
        .to_string();
    if version.is_empty() || version.len() > GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_MAX_LENGTH {
        return None;
    }
    version
        .chars()
        .all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+')
        })
        .then_some(version)
}
