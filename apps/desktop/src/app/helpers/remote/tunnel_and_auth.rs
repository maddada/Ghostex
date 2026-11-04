// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote gxserver tunnel spawn,
// authenticated-health wait, and SSH password/token keychain persistence.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashSet,
    process::Stdio,
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::app::helpers::*;
use crate::*;

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_open_remote_gxserver_tunnel(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    token: &str,
) -> Result<GpuiRemoteGxserverConnection, String> {
    let code_server_component_platform =
        gpui_remote_code_server_component_platform(config, execution_target);
    for local_port in gpui_remote_gxserver_candidate_ports() {
        let mut tunnel =
            match gpui_spawn_remote_gxserver_tunnel(config, execution_target, local_port) {
                Ok(tunnel) => tunnel,
                Err(_) => continue,
            };
        if let Some(capabilities) =
            gpui_wait_for_remote_authenticated_health(local_port, token, &mut tunnel.child)
        {
            return Ok(GpuiRemoteGxserverConnection {
                _base_url: format!("http://127.0.0.1:{local_port}"),
                capabilities,
                code_server_component_platform,
                execution_target: execution_target.clone(),
                local_port,
                presentation_stream_cancel: None,
                presentation_stream_generation: None,
                token: token.to_string(),
                child: tunnel.child,
                liveness: GpuiRemoteTunnelLiveness::default(),
                this_computer: false,
                reverse_tunnel: None,
            });
        }
        // A tunnel that is still running bound its port (ExitOnForwardFailure), so another port
        // would only be another SSH login over the same link (GitHub #361).
        let tunnel_was_running = matches!(tunnel.child.try_wait(), Ok(None));
        gpui_forget_remote_tunnel(tunnel.child.id());
        let _ = tunnel.child.kill();
        let _ = tunnel.child.wait();
        if tunnel_was_running {
            break;
        }
    }
    Err(match execution_target {
        GpuiRemoteExecutionTarget::PosixHost | GpuiRemoteExecutionTarget::WindowsPowerShell => {
            "Could not open an authenticated SSH tunnel to remote gxserver.".to_string()
        }
        GpuiRemoteExecutionTarget::WindowsWsl { .. } => {
            "gxserver started inside WSL2, but the SSH tunnel could not reach it through the Windows host's localhost forwarding. Enable WSL localhost forwarding, then reconnect.".to_string()
        }
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
/// CDXC:CodeEditor 2026-10-16 WHY:
/// A Mac remote returned no platform, so the Code view never started for it from any client (reported from a Windows PC). A Mac remote runs the editor its own Ghostex has (`source_code_server_remote_macos_editor_link_command`); a client cannot supply one, since only macOS manifests carry the darwin component and an NTFS extraction would lose its modes and links.
pub(crate) fn gpui_remote_code_server_component_platform(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> Option<String> {
    let probe = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_platform_probe_command_for(execution_target),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    let target = (probe.exit_code == 0)
        .then(|| gpui_extract_remote_install_target(probe.stdout.as_str()))
        .flatten()?;
    if target.normalized_os() == "windows" && target.normalized_arch() == "x64" {
        return Some("windows-x64".to_string());
    }
    if target.normalized_os() == "darwin" {
        return match target.normalized_arch().as_str() {
            "arm64" => Some("darwin-arm64".to_string()),
            "x64" => Some("darwin-x64".to_string()),
            _ => None,
        };
    }
    if target.normalized_os() != "linux" {
        return None;
    }
    match target.normalized_arch().as_str() {
        "x64" => Some("linux-x64".to_string()),
        "arm64" => Some("linux-arm64".to_string()),
        _ => None,
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_spawn_remote_gxserver_tunnel(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    local_port: u16,
) -> Result<GpuiRemoteSpawnedTunnel, String> {
    let target_arguments = gpui_remote_ssh_target_arguments(config)?;
    let askpass = gpui_remote_ssh_askpass_script(config)?;
    let mut arguments = Vec::new();
    let remote_port = gpui_remote_gxserver_port(execution_target);
    // Bound to 127.0.0.1 explicitly, like the code-server tunnel: without a bind address ssh can
    // succeed on ::1 alone while another account already holds 127.0.0.1:<port>, which is where
    // the health probe then sends the token. The registry matches this exact `-L` value.
    let forward = format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}");
    if !matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsWsl { .. }
    ) {
        arguments.push("-N".to_string());
    }
    arguments.extend(gpui_remote_ssh_tunnel_options(config.has_saved_password));
    arguments.extend([
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-L".to_string(),
        forward.clone(),
    ]);
    arguments.extend(target_arguments);
    if let GpuiRemoteExecutionTarget::WindowsWsl { distribution } = execution_target {
        arguments.push(gpui_remote_command_for_windows_wsl(
            Some(distribution.as_str()),
            gpui_remote_windows_wsl_gxserver_owner_command(),
        ));
    }
    let mut command = gpui_remote_background_command(&gpui_remote_ssh_executable());
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
        command.envs(environment);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start the SSH tunnel.".to_string())?;
    gpui_record_remote_tunnel(child.id(), forward.as_str());
    thread::sleep(GPUI_REMOTE_GXSERVER_TUNNEL_STARTUP_DELAY);
    if !matches!(child.try_wait(), Ok(None)) {
        gpui_forget_remote_tunnel(child.id());
        let _ = child.kill();
        let _ = child.wait();
        return Err("SSH tunnel exited before remote gxserver became reachable.".to_string());
    }
    Ok(GpuiRemoteSpawnedTunnel {
        child,
        _askpass: askpass,
    })
}

pub(crate) fn gpui_remote_gxserver_candidate_ports() -> Vec<u16> {
    let range =
        u64::from(GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MAX - GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MIN + 1);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id());
    let mut ports = Vec::with_capacity(GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS);
    let mut seen = HashSet::new();
    for attempt in 0..(GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS * 2) {
        let offset = seed
            .wrapping_add((attempt as u64).wrapping_mul(7_919))
            .wrapping_rem(range);
        let port = GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MIN + offset as u16;
        if seen.insert(port) {
            ports.push(port);
        }
        if ports.len() == GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS {
            break;
        }
    }
    ports
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
/// Waits for the new tunnel to carry a healthy answer. The port refuses until SSH has logged in
/// and bound it, which takes several round trips on a slow link, so the wait lasts
/// [`GPUI_REMOTE_GXSERVER_HEALTH_DEADLINE`] and each probe may take
/// [`GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT`]; an SSH process that exits ends it at once, so a refused
/// login or an unreachable host is still reported as soon as SSH gives up (GitHub #361).
pub(crate) fn gpui_wait_for_remote_authenticated_health(
    local_port: u16,
    token: &str,
    tunnel: &mut std::process::Child,
) -> Option<GpuiRemoteGxserverCapabilities> {
    let deadline = Instant::now() + GPUI_REMOTE_GXSERVER_HEALTH_DEADLINE;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || !matches!(tunnel.try_wait(), Ok(None)) {
            return None;
        }
        if let GpuiRemoteHealthProbe::Healthy(capabilities) = gpui_remote_health_probe(
            local_port,
            token,
            remaining.min(GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT),
        ) {
            return Some(capabilities);
        }
        thread::sleep(GPUI_REMOTE_GXSERVER_TUNNEL_RETRY_DELAY);
    }
}

/// Reads the fixed capability names GPUI selects remote operations with out of a
/// healthy `/api/health/server` body. A daemon that predates a capability simply
/// omits it, so an unparsable or capability-less answer means "not supported"
/// rather than a failed connection.
pub(crate) fn gpui_remote_gxserver_capabilities_from_health_response(
    response: &str,
) -> GpuiRemoteGxserverCapabilities {
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return GpuiRemoteGxserverCapabilities::default();
    };
    let Ok(body) = gxserver_http_response_body(headers, body) else {
        return GpuiRemoteGxserverCapabilities::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body.as_str()) else {
        return GpuiRemoteGxserverCapabilities::default();
    };
    let advertises = |capability: &str| {
        value
            .get("capabilities")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|capabilities| {
                capabilities
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .any(|advertised| advertised == capability)
            })
    };
    GpuiRemoteGxserverCapabilities {
        code_server_prompt_editor: advertises(GPUI_GXSERVER_CODE_SERVER_PROMPT_EDITOR_CAPABILITY),
        browser_tcp_websocket_v1: advertises("browserTcpWebSocketV1"),
        coordinator_handoff_draft: advertises("coordinatorHandoffDraft"),
        zmx_missing: value
            .get("tools")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .any(|tool| {
                tool.get("tool").and_then(serde_json::Value::as_str) == Some("zmx")
                    && tool
                        .get("availability")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|availability| availability != "available")
            }),
    }
}

#[cfg(test)]
mod zmx_health_tests {
    use super::gpui_remote_gxserver_capabilities_from_health_response;

    fn capabilities(body: &str) -> crate::app::model::GpuiRemoteGxserverCapabilities {
        gpui_remote_gxserver_capabilities_from_health_response(&format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ))
    }

    #[test]
    fn a_health_answer_says_when_zmx_cannot_be_resolved() {
        let tool = |availability: &str| {
            format!(
                r#"{{"tools":[{{"tool":"bd","availability":"missing"}},{{"tool":"zmx","availability":"{availability}"}}]}}"#
            )
        };
        assert!(capabilities(&tool("missing")).zmx_missing);
        // A missing `bd` is the project board's business, not the machine's.
        assert!(!capabilities(&tool("available")).zmx_missing);
        // An older daemon's answer says nothing either way.
        assert!(!capabilities(r#"{"capabilities":[]}"#).zmx_missing);
    }

    #[test]
    fn only_a_daemon_that_advertises_it_takes_an_orchestrator_handoff_draft() {
        assert!(
            capabilities(r#"{"capabilities":["health","coordinatorHandoffDraft"]}"#)
                .coordinator_handoff_draft
        );
        assert!(!capabilities(r#"{"capabilities":["health"]}"#).coordinator_handoff_draft);
        assert!(!capabilities("{}").coordinator_handoff_draft);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteSshPasswordKeychainResult {
    Unsupported,
    Failed,
    Success,
}

pub(crate) fn gpui_remote_machine_password_failure_title(has_password: bool) -> &'static str {
    if has_password {
        "SSH password not saved"
    } else {
        "SSH password not removed"
    }
}

pub(crate) fn gpui_normalize_remote_machine_id(input: &str) -> Option<String> {
    let id = input.trim();
    if id.is_empty() || id.chars().count() > GPUI_REMOTE_MACHINE_ID_MAX_CHARS {
        return None;
    }
    let prefix_len = "remote-".len();
    if id.len() <= prefix_len
        || !id
            .get(..prefix_len)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("remote-"))
    {
        return None;
    }
    let Some(suffix) = id.get(prefix_len..) else {
        return None;
    };
    if suffix
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Some(id.to_string());
    }
    None
}

pub(crate) fn gpui_remote_machine_id_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_object()
        .and_then(|machine| machine.get("id"))
        .and_then(serde_json::Value::as_str)
        .and_then(gpui_normalize_remote_machine_id)
}

pub(crate) fn gpui_settings_object_has_remote_machine_id(
    object: &serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
) -> bool {
    object
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|machines| {
            machines.iter().any(|machine| {
                gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id)
            })
        })
}

pub(crate) fn gpui_set_remote_machine_password_marker(
    object: &mut serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
    has_password: bool,
) -> bool {
    let Some(machines) = object
        .get_mut("remoteMachines")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    for machine in machines {
        let Some(machine_object) = machine.as_object_mut() else {
            continue;
        };
        if machine_object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
            .as_deref()
            != Some(remote_machine_id)
        {
            continue;
        }
        if has_password {
            machine_object.insert(
                "sshPasswordSaved".to_string(),
                serde_json::Value::Bool(true),
            );
        } else {
            machine_object.remove("sshPasswordSaved");
        }
        return true;
    }
    false
}

pub(crate) fn gpui_removed_remote_machine_password_ids(
    previous_settings: &serde_json::Map<String, serde_json::Value>,
    next_settings: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let next_ids: HashSet<String> = next_settings
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .map(|machines| {
            machines
                .iter()
                .filter_map(gpui_remote_machine_id_from_value)
                .collect()
        })
        .unwrap_or_default();
    let Some(previous_machines) = previous_settings
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut removed = Vec::new();
    for machine in previous_machines {
        let Some(machine_object) = machine.as_object() else {
            continue;
        };
        if machine_object
            .get("sshPasswordSaved")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            continue;
        }
        let Some(remote_machine_id) = gpui_remote_machine_id_from_value(machine) else {
            continue;
        };
        if !next_ids.contains(&remote_machine_id) && seen.insert(remote_machine_id.clone()) {
            removed.push(remote_machine_id);
        }
    }
    removed
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_save_remote_machine_password_to_keychain(
    remote_machine_id: &str,
    password: &str,
) -> GpuiRemoteSshPasswordKeychainResult {
    let Ok(remote_machine_id) = std::ffi::CString::new(remote_machine_id) else {
        return GpuiRemoteSshPasswordKeychainResult::Failed;
    };
    let password_bytes = password.as_bytes();
    match unsafe {
        GhostexGpuiSaveRemoteSshPassword(
            remote_machine_id.as_ptr(),
            password_bytes.as_ptr(),
            password_bytes.len(),
        )
    } {
        1 => GpuiRemoteSshPasswordKeychainResult::Success,
        -1 => GpuiRemoteSshPasswordKeychainResult::Unsupported,
        _ => GpuiRemoteSshPasswordKeychainResult::Failed,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_save_remote_machine_password_to_keychain(
    _remote_machine_id: &str,
    _password: &str,
) -> GpuiRemoteSshPasswordKeychainResult {
    GpuiRemoteSshPasswordKeychainResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_save_remote_gxserver_token_to_keychain(
    remote_machine_id: &str,
    token: &str,
) -> GpuiRemoteTokenKeychainResult {
    let Ok(remote_machine_id) = std::ffi::CString::new(remote_machine_id) else {
        return GpuiRemoteTokenKeychainResult::Failed;
    };
    let token_bytes = token.as_bytes();
    match unsafe {
        GhostexGpuiSaveRemoteGxserverToken(
            remote_machine_id.as_ptr(),
            token_bytes.as_ptr(),
            token_bytes.len(),
        )
    } {
        1 => GpuiRemoteTokenKeychainResult::Success,
        -1 => GpuiRemoteTokenKeychainResult::Unsupported,
        _ => GpuiRemoteTokenKeychainResult::Failed,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_save_remote_gxserver_token_to_keychain(
    _remote_machine_id: &str,
    _token: &str,
) -> GpuiRemoteTokenKeychainResult {
    GpuiRemoteTokenKeychainResult::Unsupported
}
