// A Cloud Box session's terminal pane: the remote-machine attach path with `ghostex cloud attach`
// in place of `ssh … zmx attach`.
//
// CDXC:CloudBoxes 2026-11-02 WHY:
// A box is a hidden machine with no saved SSH machine behind it (apps/desktop/src/app/gx_store/
// cloud_boxes.rs), and its gxserver serves each session's terminal on `/api/terminal`, the web
// build's path. The pane therefore runs the bundled `ghostex cloud attach <box> <project>
// <session>` (server/src/ghostex_cli/cloud_attach.rs), which asks home gxserver how to reach the
// box and pumps the pane's bytes over that socket, so the rest of the remote open (tab, chat
// launch intent, focus, visibility sequences) is the same code as for any remote machine. The
// attach metadata (title, agent icon, restore guard) is still read from the box's own gxserver.

use std::path::PathBuf;
use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

/// The machine config a box's remote open is handed: a box has no SSH host, and nothing on the
/// box path reads one.
pub(crate) fn gpui_cloud_box_machine_config(remote_machine_id: &str) -> GpuiRemoteMachineConfig {
    GpuiRemoteMachineConfig {
        remote_machine_id: remote_machine_id.to_string(),
        transport: GpuiRemoteMachineTransport::Ssh,
        easy_connect_address: None,
        easy_connect_ssh_port: GPUI_EASY_CONNECT_DEFAULT_SSH_PORT,
        ssh_host: String::new(),
        ssh_identity_file: None,
        ssh_port: None,
        ssh_user: None,
        has_saved_password: false,
        wsl_distribution: None,
        disabled: false,
        ssh_control_path: None,
    }
}

/// The machine config a remote session's pane (re)attaches with: a box's stand-in, else the
/// saved machine's. Every attach path (a row click, a tab click, a restored pane) asks this, so
/// a box session is never refused for want of SSH settings.
pub(crate) fn gpui_remote_attach_machine_config(
    settings: &serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
) -> Option<GpuiRemoteMachineConfig> {
    if gpui_is_cloud_box_machine(remote_machine_id) {
        return Some(gpui_cloud_box_machine_config(remote_machine_id));
    }
    gpui_remote_machine_config_from_settings(settings, remote_machine_id)
}

/// Whether a remote machine id is a Cloud Box's hidden machine.
pub(crate) fn gpui_is_cloud_box_machine(remote_machine_id: &str) -> bool {
    ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(remote_machine_id).is_some()
}

/// The bundled `ghostex` the pane runs.
fn ghostex_cli_path() -> Result<PathBuf, String> {
    let name = if cfg!(target_os = "windows") {
        "ghostex.exe"
    } else {
        "ghostex"
    };
    if let Ok(directory) = gpui_bundled_ghostex_cli_resource_dir() {
        let path = directory.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    let current = std::env::current_exe().map_err(|_| "Ghostex CLI is unavailable.".to_string())?;
    let sibling = current.with_file_name(name);
    sibling
        .is_file()
        .then_some(sibling)
        .ok_or_else(|| "Ghostex CLI is unavailable.".to_string())
}

/// The plan a box session's pane opens with. `wake_session` picks the RPC as on any remote
/// machine (it wakes the session's agent on the box, never the box itself).
pub(crate) fn gpui_prepare_cloud_box_attach_terminal_plan(
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
    box_id: &str,
    wake_session: bool,
) -> Result<GpuiRemoteAttachTerminalPlan, String> {
    let path = if wake_session {
        "/api/wakeSession"
    } else {
        "/api/attachSessionMetadata"
    };
    let result = gpui_remote_gxserver_rpc_result(
        target,
        path,
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(15),
    )?;
    gpui_validate_remote_attach_metadata(&result)?;
    let attach = result
        .get("attach")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    let ghostex = ghostex_cli_path()?;
    Ok(GpuiRemoteAttachTerminalPlan {
        agent_icon: gpui_workspace_attach_agent_icon(attach),
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        askpass: None,
        terminal_command: cloud_box_attach_command(
            &ghostex,
            box_id,
            &reference.project_id,
            &reference.session_id,
        ),
        title: gpui_workspace_attach_title(attach),
    })
}

/// The pane's command: the ready title the remote pane waits for, then the attach, run again
/// after a dropped link (a stopped box refuses until a send wakes it) and left when the session's
/// terminal ends.
#[cfg(not(target_os = "windows"))]
fn cloud_box_attach_command(
    ghostex: &std::path::Path,
    box_id: &str,
    project_id: &str,
    session_id: &str,
) -> String {
    let attach = format!(
        "{} cloud attach {} {} {}",
        gpui_shell_single_quote(&ghostex.to_string_lossy()),
        gpui_shell_single_quote(box_id),
        gpui_shell_single_quote(project_id),
        gpui_shell_single_quote(session_id),
    );
    format!(
        "printf '\\033]2;{TEMP_REMOTE_SSH_READY_TITLE}\\007'; while :; do {attach} && break; sleep 2; done"
    )
}

#[cfg(target_os = "windows")]
fn cloud_box_attach_command(
    ghostex: &std::path::Path,
    box_id: &str,
    project_id: &str,
    session_id: &str,
) -> String {
    let quote = |value: &str| format!("'{}'", value.replace('\'', "''"));
    format!(
        "[Console]::Write([string][char]27 + ']2;{TEMP_REMOTE_SSH_READY_TITLE}' + [char]7); while ($true) {{ & {} cloud attach {} {} {}; if ($LASTEXITCODE -eq 0) {{ break }}; Start-Sleep -Seconds 2 }}",
        quote(&ghostex.to_string_lossy()),
        quote(box_id),
        quote(project_id),
        quote(session_id),
    )
}
