// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds remote-machine settings parsing and
// remote sidebar command/repo-clone param builders. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_machine_config_from_settings(
    object: &serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
) -> Option<GpuiRemoteMachineConfig> {
    /*
    CDXC:RemoteMachines 2026-06-24-14:34:
    Remote reconnect must source SSH host/user/port/identity/password marker from the normalized shared Settings snapshot, not from the React command. This keeps the modal command bounded to an id/approval flag and prevents injected hostnames, paths, passwords, tokens, or shell text from crossing the app-modal bridge.
    */
    let machines = object.get("remoteMachines")?.as_array()?;
    let machine = machines.iter().find_map(|machine| {
        (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
            .then_some(machine.as_object())
            .flatten()
    })?;
    let transport =
        if machine.get("transport").and_then(serde_json::Value::as_str) == Some("easyConnect") {
            GpuiRemoteMachineTransport::EasyConnect
        } else {
            GpuiRemoteMachineTransport::Ssh
        };
    let easy_connect_address = match transport {
        GpuiRemoteMachineTransport::EasyConnect => Some(gpui_remote_machine_string_field(
            machine,
            "easyConnectAddress",
        )?),
        GpuiRemoteMachineTransport::Ssh => None,
    };
    let easy_connect_ssh_port = gpui_remote_machine_ssh_port(machine.get("sshPort"))
        .unwrap_or(GPUI_EASY_CONNECT_DEFAULT_SSH_PORT);
    /*
    CDXC:RemotePairing 2026-09-03:
    An Easy Connect machine's SSH endpoint is the loopback forwarder this app
    runs for it, so the host and port come from the forwarder registry rather
    than from Settings. While no forwarder is running the host stays empty;
    `gpui_remote_ssh_dial_refusal` turns that into an explicit error in every
    SSH builder, because ssh itself would resolve an empty host to this
    computer's own sshd.
    */
    let (ssh_host, ssh_port) = match transport {
        GpuiRemoteMachineTransport::Ssh => (
            gpui_remote_machine_string_field(machine, "sshHost")?,
            gpui_remote_machine_ssh_port(machine.get("sshPort")),
        ),
        GpuiRemoteMachineTransport::EasyConnect => {
            match gpui_easy_connect_forward_port(remote_machine_id) {
                Some(port) => (GPUI_EASY_CONNECT_FORWARD_HOST.to_string(), Some(port)),
                None => (String::new(), None),
            }
        }
    };
    let wsl_distribution = gpui_remote_machine_string_field(machine, "wslDistribution");
    if wsl_distribution
        .as_deref()
        .is_some_and(|distribution| !gpui_remote_wsl_distribution_is_valid(distribution))
    {
        return None;
    }
    Some(GpuiRemoteMachineConfig {
        remote_machine_id: remote_machine_id.to_string(),
        transport,
        easy_connect_address,
        easy_connect_ssh_port,
        ssh_host,
        ssh_identity_file: gpui_remote_machine_string_field(machine, "sshIdentityFile")
            .map(|path| gpui_expand_remote_identity_file(path.as_str())),
        ssh_port,
        ssh_user: gpui_remote_machine_string_field(machine, "sshUser"),
        has_saved_password: machine
            .get("sshPasswordSaved")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        // An Easy Connect machine is dialed on this computer's loopback forwarder and a Windows
        // WSL machine runs gxserver inside its distribution, so neither reuses an SSH connection.
        reuse_ssh_connection: transport == GpuiRemoteMachineTransport::Ssh
            && wsl_distribution.is_none()
            && machine
                .get("reuseSshConnection")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        wsl_distribution,
        disabled: machine.get("disabled").and_then(serde_json::Value::as_bool) == Some(true),
        ssh_control_path: None,
    })
}

pub(crate) fn gpui_disabled_remote_machine_ids(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    object
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .map(|machines| {
            machines
                .iter()
                .filter_map(|machine| {
                    let machine_object = machine.as_object()?;
                    if machine_object
                        .get("disabled")
                        .and_then(serde_json::Value::as_bool)
                        != Some(true)
                    {
                        return None;
                    }
                    gpui_remote_machine_id_from_value(machine)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The live connections a settings save ends: machines turned off, and machines no longer saved.
///
/// CDXC:RemoteMachines 2026-10-28 WHY: Removing a machine in Settings left its SSH tunnel to the
/// machine's gxserver running for as long as the app stayed open, because only machines turned
/// off had their connection stopped (found testing a loopback machine on Linux; this code is the
/// same on macOS, Linux and Windows). A removed machine's connection ends the same way.
pub(crate) fn gpui_remote_machine_connections_to_stop<'a>(
    next_settings: &serde_json::Map<String, serde_json::Value>,
    connected_ids: impl IntoIterator<Item = &'a String>,
) -> Vec<String> {
    let mut ids = gpui_disabled_remote_machine_ids(next_settings);
    for connected in connected_ids {
        if !ids.contains(connected)
            && !super::tunnel_and_auth::gpui_settings_object_has_remote_machine_id(
                next_settings,
                connected,
            )
        {
            ids.push(connected.clone());
        }
    }
    ids
}

/// The saved machines a settings save changed the connection of (host, user, port, key, WSL
/// distribution, Easy Connect code or saved password), still shown in the sidebar.
///
/// CDXC:RemoteMachines 2026-10-28 WHY: editing a machine's host, user or key kept the old
/// connection, and its old error, until the user pressed Reconnect (found testing a loopback
/// machine on Linux; every platform). Those machines reconnect with the new details as soon as
/// they are saved. A rename or the sidebar switch alone is not a connection change.
pub(crate) fn gpui_remote_machines_with_changed_connection(
    previous_settings: &serde_json::Map<String, serde_json::Value>,
    next_settings: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let connection = |machine: &serde_json::Value| {
        let mut fields = machine.as_object().cloned().unwrap_or_default();
        fields.remove("name");
        // The sidebar pill's short name is a label too (gx-core machine_short_names.rs).
        fields.remove("shortName");
        fields.remove("disabled");
        // Agent access is renewed in place (remote_conn/connected_computers.rs), never a reconnect.
        fields.remove("agentAccess");
        fields
    };
    let machines = |settings: &serde_json::Map<String, serde_json::Value>| {
        settings
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let previous = machines(previous_settings);
    machines(next_settings)
        .iter()
        .filter(|machine| {
            machine.get("disabled").and_then(serde_json::Value::as_bool) != Some(true)
        })
        .filter_map(|machine| {
            let id = gpui_remote_machine_id_from_value(machine)?;
            let before = previous.iter().find(|previous| {
                gpui_remote_machine_id_from_value(previous).as_deref() == Some(id.as_str())
            })?;
            (connection(before) != connection(machine)).then_some(id)
        })
        .collect()
}

pub(crate) fn gpui_remote_wsl_distribution_is_valid(distribution: &str) -> bool {
    let distribution = distribution.trim();
    !distribution.is_empty()
        && distribution.chars().count() <= 120
        && !distribution.starts_with('-')
        && distribution.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '+' | '-' | '(' | ')' | ' ')
        })
}

pub(crate) fn gpui_remote_machine_name_from_settings(remote_machine_id: &str) -> Option<String> {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .and_then(|machines| {
            machines.iter().find_map(|machine| {
                let object = machine.as_object()?;
                (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
                    .then(|| gpui_remote_machine_string_field(object, "name"))
                    .flatten()
            })
        })
}

pub(crate) fn gpui_remote_machine_string_field(
    machine: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    machine
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn gpui_remote_request_id_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .map(str::to_string)
}

pub(crate) fn gpui_remote_gxserver_presentation_client_id(remote_machine_id: &str) -> String {
    format!("{GPUI_SIDEBAR_GXSERVER_CLIENT_ID}:{remote_machine_id}")
}

pub(crate) fn gpui_remote_path_like_string_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    allow_empty: bool,
) -> Option<String> {
    command
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| allow_empty || !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .map(str::to_string)
}

/*
CDXC:AddProject 2026-07-30:
Every server round trip the shared add-project dialog performs crosses this one
bridge command. The renderer sends a bounded operation name plus the fields that
operation is allowed to carry; Rust owns the endpoint map, the timeouts, and the
local-vs-remote routing, so the CEF modal host never learns a host, a tunnel
port, or a token — it only ever names a machine by its bounded id.
*/

#[cfg(test)]
mod connections_to_stop_tests {
    use super::gpui_remote_machine_connections_to_stop;

    #[test]
    fn a_removed_or_turned_off_machine_loses_its_connection() {
        let settings = serde_json::json!({"remoteMachines": [
            {"id": "remote-kept", "name": "Kept", "sshHost": "a"},
            {"id": "remote-off", "name": "Off", "sshHost": "b", "disabled": true},
        ]});
        let settings = settings.as_object().unwrap();
        let connected = ["remote-kept", "remote-off", "remote-gone"].map(String::from);
        let mut stop = gpui_remote_machine_connections_to_stop(settings, connected.iter());
        stop.sort();
        assert_eq!(stop, ["remote-gone", "remote-off"]);
        let nothing_saved = serde_json::Map::new();
        assert_eq!(
            gpui_remote_machine_connections_to_stop(&nothing_saved, connected[..1].iter()),
            ["remote-kept"]
        );
    }
}

#[cfg(test)]
mod changed_connection_tests {
    use super::gpui_remote_machines_with_changed_connection;

    #[test]
    fn a_saved_host_user_or_key_change_reconnects_but_a_rename_does_not() {
        let previous = serde_json::json!({"remoteMachines": [
            {"id": "remote-a", "name": "A", "sshHost": "bad.invalid", "sshUser": "x"},
            {"id": "remote-b", "name": "B", "sshHost": "b"},
            {"id": "remote-c", "name": "C", "sshHost": "c"},
        ]});
        let next = serde_json::json!({"remoteMachines": [
            {"id": "remote-a", "name": "A", "sshHost": "127.0.0.1", "sshUser": "madda", "sshIdentityFile": "~/.ssh/k"},
            {"id": "remote-b", "name": "B renamed", "sshHost": "b"},
            {"id": "remote-c", "name": "C", "sshHost": "c2", "disabled": true},
            {"id": "remote-new", "name": "N", "sshHost": "n"},
        ]});
        assert_eq!(
            gpui_remote_machines_with_changed_connection(
                previous.as_object().unwrap(),
                next.as_object().unwrap()
            ),
            ["remote-a"]
        );
    }
}
