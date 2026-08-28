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
    CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
    Remote reconnect must source SSH host/user/port/identity/password marker from the normalized shared Settings snapshot, not from the React command. This keeps the modal command bounded to an id/approval flag and prevents injected hostnames, paths, passwords, tokens, or shell text from crossing the app-modal bridge.
    */
    let machines = object.get("remoteMachines")?.as_array()?;
    let machine = machines.iter().find_map(|machine| {
        (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
            .then_some(machine.as_object())
            .flatten()
    })?;
    let ssh_host = gpui_remote_machine_string_field(machine, "sshHost")?;
    let wsl_distribution = gpui_remote_machine_string_field(machine, "wslDistribution");
    if wsl_distribution
        .as_deref()
        .is_some_and(|distribution| !gpui_remote_wsl_distribution_is_valid(distribution))
    {
        return None;
    }
    Some(GpuiRemoteMachineConfig {
        remote_machine_id: remote_machine_id.to_string(),
        ssh_host,
        ssh_identity_file: gpui_remote_machine_string_field(machine, "sshIdentityFile")
            .map(|path| gpui_expand_remote_identity_file(path.as_str())),
        ssh_port: gpui_remote_machine_ssh_port(machine.get("sshPort")),
        ssh_user: gpui_remote_machine_string_field(machine, "sshUser"),
        has_saved_password: machine
            .get("sshPasswordSaved")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        wsl_distribution,
        disabled: machine.get("disabled").and_then(serde_json::Value::as_bool) == Some(true),
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

pub(crate) fn gpui_remote_presentation_client_id_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    command
        .get("clientId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .filter(|value| !value.chars().any(char::is_control))
        .map(str::to_string)
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
pub(crate) fn gpui_remote_repository_clone_preview_params_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let repository_input =
        gpui_remote_repository_clone_text_field(command, "repositoryInput", 4_096, false)?;
    let folder_path = gpui_remote_path_like_string_from_command(command, "folderPath", false)?;
    let mut params = serde_json::Map::new();
    params.insert("folderPath".to_string(), serde_json::json!(folder_path));
    if let Some(new_folder_name) =
        gpui_remote_repository_clone_text_field(command, "newFolderName", 255, true)
            .filter(|value| !value.is_empty())
    {
        params.insert(
            "newFolderName".to_string(),
            serde_json::json!(new_folder_name),
        );
    }
    params.insert(
        "repositoryInput".to_string(),
        serde_json::json!(repository_input),
    );
    Some(serde_json::Value::Object(params))
}

pub(crate) fn gpui_remote_repository_clone_start_params_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut params = gpui_remote_repository_clone_preview_params_from_command(command)?;
    let object = params.as_object_mut()?;
    if let Some(branch_name) =
        gpui_remote_repository_clone_text_field(command, "branchName", 255, true)
            .filter(|value| !value.is_empty())
    {
        if !gpui_remote_repository_clone_branch_name_allowed(&branch_name) {
            return None;
        }
        object.insert("branchName".to_string(), serde_json::json!(branch_name));
    }
    object.insert(
        "cloneMainOnly".to_string(),
        serde_json::json!(
            command
                .get("cloneMainOnly")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        ),
    );
    object.insert(
        "shallowClone".to_string(),
        serde_json::json!(
            command
                .get("shallowClone")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        ),
    );
    Some(params)
}

pub(crate) fn gpui_remote_repository_clone_text_field(
    command: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
    allow_empty: bool,
) -> Option<String> {
    command
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| allow_empty || !value.is_empty())
        .filter(|value| value.chars().count() <= max_chars)
        .filter(|value| !value.contains('\0'))
        .filter(|value| !value.chars().any(char::is_control))
        .map(str::to_string)
}

pub(crate) fn gpui_remote_repository_clone_branch_name_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 255
        && value != "@"
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '\u{7f}')
                || ch.is_control()
        })
        && value.split('/').all(|segment| {
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
        })
}

pub(crate) fn gpui_remote_repository_clone_job_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(crate) fn gpui_remote_repository_clone_toast_id(request_id: &str) -> String {
    /*
    CDXC:RemoteClone 2026-06-24-19:35:
    Remote clone toast identifiers may be derived from modal request ids for UI replacement, but the id must be bounded and ASCII-sanitized before it crosses back to CEF so renderer-controlled text is never used verbatim as app-modal chrome identity.
    */
    let suffix: String = request_id
        .bytes()
        .take(64)
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
                byte as char
            } else {
                '-'
            }
        })
        .collect();
    let suffix = if suffix.is_empty() {
        "request".to_string()
    } else {
        suffix
    };
    format!("gpui-remote-repository-clone-{suffix}")
}

pub(crate) fn gpui_remote_project_name_from_path(path: &str) -> String {
    path.trim()
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .next_back()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .unwrap_or("Remote Project")
        .chars()
        .take(120)
        .collect()
}
