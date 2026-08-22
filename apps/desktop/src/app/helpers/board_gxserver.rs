// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use std::{
    collections::HashSet,
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;
use futures::{StreamExt as _, channel::mpsc};
use gpui::{
    AppContext as _, Asset,
    ClipboardEntry, ClipboardItem, ImageFormat, ParentElement as _, Styled as _, prelude::FluentBuilder as _,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_titlebar_gxserver_daemon_status() -> serde_json::Value {
    match gpui_probe_local_gxserver_health() {
        GpuiLocalGxserverHealthState::Healthy { tools_available } => {
            let mut status = serde_json::json!({
                "ok": tools_available,
                "state": "running",
            });
            if !tools_available {
                status["message"] =
                    serde_json::json!("gxserver is running, but zmx/bd are unavailable.");
            }
            status
        }
        GpuiLocalGxserverHealthState::ProtocolMismatch { reported } => serde_json::json!({
            "message": gpui_gxserver_protocol_mismatch_message(reported),
            "ok": false,
            "state": "protocolMismatch",
        }),
        GpuiLocalGxserverHealthState::BuildMismatch => serde_json::json!({
            "message": "gxserver belongs to a different Ghostex build and must be restarted.",
            "ok": false,
            "state": "buildMismatch",
        }),
        GpuiLocalGxserverHealthState::Unreachable => serde_json::json!({
            "ok": false,
            "state": "stopped",
        }),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_on_demand_gxserver_asset_key(target: &GpuiRemoteInstallTarget) -> Option<&'static str> {
    if target.normalized_os() != "linux" {
        return None;
    }
    match target.normalized_arch().as_str() {
        "x64" => Some("gxserver-linux-x64"),
        "arm64" => Some("gxserver-linux-arm64"),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_on_demand_gxserver_archive(
    target: &GpuiRemoteInstallTarget,
    progress_tx: Option<&mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> Result<PathBuf, GpuiOnDemandArchiveFailure> {
    let Some(asset_key) = gpui_on_demand_gxserver_asset_key(target) else {
        return Err(GpuiOnDemandArchiveFailure {
            message: gpui_unsupported_remote_package_message(target),
            state: GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform,
        });
    };
    let Some(resources_dir) = gpui_app_bundle_resources_dir() else {
        return Err(GpuiOnDemandArchiveFailure {
            message: "Could not locate the app's sealed on-demand resource manifest.".to_string(),
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        });
    };
    let manifest_path = resources_dir.join("Web/on-demand-resources.json");
    let manifest = component_store::OnDemandManifest::load(&manifest_path).map_err(|message| {
        GpuiOnDemandArchiveFailure {
            message,
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        }
    })?;
    if !manifest.assets.contains_key(asset_key) {
        return Err(GpuiOnDemandArchiveFailure {
            message: gpui_unsupported_remote_package_message(target),
            state: GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform,
        });
    }
    let store = component_store::ComponentStore::from_manifest(manifest).map_err(|message| {
        GpuiOnDemandArchiveFailure {
            message,
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        }
    })?;
    let mut report_progress = |event: component_store::ComponentStoreProgress| {
        if matches!(
            event.phase,
            component_store::ComponentStoreProgressPhase::Downloading
        ) {
            if let Some(progress_tx) = progress_tx {
                let _ = progress_tx.unbounded_send(GpuiRemoteGxserverConnectProgress {
                    state: GpuiRemoteGxserverConnectState::DownloadingRemoteServerPackage,
                });
            }
        }
        support_logs::append(
            support_logs::GpuiSupportLog::RemoteGxserverInstall,
            "gpui.remoteGxserver.install.onDemand.progress",
            serde_json::json!({
                "asset": event.component,
                "assetBytes": event.size_bytes,
                "phase": event.phase.as_str(),
            }),
        );
    };
    store
        .download_release_asset(asset_key, &mut report_progress)
        .map_err(|message| GpuiOnDemandArchiveFailure {
            message,
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_install_gxserver_archive_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    archive_path: &Path,
) -> GpuiRemoteProcessResult {
    // Stream the archive through the remote shell so its GHOSTEX_HOME/XDG
    // environment determines the destination on Linux, macOS, and WSL alike.
    let upload_result = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        concat!(
            "set -eu; umask 077; ",
            "case \"${GHOSTEX_HOME:-}\" in /*) ghostex_data_dir=\"$GHOSTEX_HOME\";; ",
            "*) case \"${XDG_DATA_HOME:-}\" in /*) ghostex_data_dir=\"${XDG_DATA_HOME%/}/ghostex\";; *) ghostex_data_dir=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
            "install_root=\"$ghostex_data_dir/gxserver\"; mkdir -p \"$install_root\"; ",
            "cat > \"$install_root/gxserver-upload.tar.gz\""
        ),
        archive_path,
        GPUI_REMOTE_GXSERVER_UPLOAD_TIMEOUT,
    );
    if upload_result.exit_code != 0 {
        return GpuiRemoteProcessResult {
            exit_code: upload_result.exit_code,
            stderr: "Could not upload gxserver package over SSH.".to_string(),
            stdout: String::new(),
        };
    }
    let release_uuid = match gpui_random_uuid_string() {
        Ok(value) => value,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not prepare gxserver install release id.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let release_id = format!("release-{release_uuid}");
    let install_command = gpui_remote_gxserver_install_command(release_id.as_str());
    gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        install_command.as_str(),
        GPUI_REMOTE_GXSERVER_INSTALL_TIMEOUT,
    )
}

pub(crate) fn gpui_create_command_terminal_gxserver_session(
    input: &GpuiCommandTerminalCreateInput,
) -> Result<GpuiLocalWorkspaceSessionKey, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/createSession",
        &gpui_command_terminal_create_session_params(input),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create a command terminal session.".to_string())?;
    let project_id = gpui_trimmed_json_string_field(session, "projectId")
        .ok_or_else(|| "gxserver did not return a command terminal project id.".to_string())?
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return a command terminal session id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("gxserver returned an invalid command terminal session id.".to_string());
    }
    Ok(GpuiLocalWorkspaceSessionKey {
        project_id,
        session_id,
    })
}

pub(crate) fn gpui_close_command_terminal_gxserver_session(key: &GpuiLocalWorkspaceSessionKey) -> bool {
    let _ = gpui_gxserver_rpc_result(
        "/api/transitionSession",
        &serde_json::json!({
            "action": "close",
            "projectId": key.project_id.as_str(),
            "reason": "closeTerminal",
            "sessionId": key.session_id.as_str(),
        }),
        Duration::from_secs(30),
    );
    if gpui_gxserver_rpc_result(
        "/api/removeSession",
        &serde_json::json!({
            "projectId": key.project_id.as_str(),
            "reason": "closeTerminal",
            "sessionId": key.session_id.as_str(),
        }),
        Duration::from_secs(10),
    )
    .is_ok()
    {
        return true;
    }
    gpui_gxserver_rpc_result(
        "/api/listSessions",
        &serde_json::json!({ "projectId": key.project_id.as_str() }),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| {
        result
            .get("sessions")
            .and_then(serde_json::Value::as_array)
            .cloned()
    })
    .is_some_and(|sessions| {
        !sessions.iter().any(|session| {
            session.get("projectId").and_then(serde_json::Value::as_str)
                == Some(key.project_id.as_str())
                && session.get("sessionId").and_then(serde_json::Value::as_str)
                    == Some(key.session_id.as_str())
        })
    })
}

pub(crate) fn gpui_update_command_terminal_gxserver_session_title(
    key: &GpuiLocalWorkspaceSessionKey,
    title: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
    Command-pane renames update the gxserver session row directly so restart
    and sidebar projections use the same title as the local tab. Send only the
    validated title plus gxserver ids; no terminal input, command text, cwd,
    attach command, or renderer payload is part of this update.
    */
    let title = title.trim();
    if title.is_empty()
        || title.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || title.contains('\0')
        || title.chars().any(char::is_control)
    {
        return Err("The command terminal title is invalid.".to_string());
    }
    let result = gpui_gxserver_rpc_result(
        "/api/updateSession",
        &serde_json::json!({
            "projectId": key.project_id.as_str(),
            "sessionId": key.session_id.as_str(),
            "title": title,
        }),
        Duration::from_secs(10),
    )?;
    if result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        Ok(())
    } else {
        Err("gxserver returned an invalid command terminal rename result.".to_string())
    }
}

pub(crate) fn gpui_update_command_terminal_gxserver_session_surface(
    key: &GpuiLocalWorkspaceSessionKey,
    surface: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUICommandWorkspaceTransfer 2026-08-01:
    A command tab dragged into the Agents workspace keeps its live daemon
    session, so the gxserver row has to change surfaces with it. This is not
    cosmetic: the sidebar only projects `workspace` sessions, and an Agents tab
    missing from that projection is reconciled away along with its shell
    session. Send only the fixed surface enum plus gxserver ids; no terminal
    input, command text, cwd, attach command, or renderer payload.
    */
    if surface != "workspace" && surface != "commands" {
        return Err("The command terminal surface is invalid.".to_string());
    }
    let result = gpui_gxserver_rpc_result(
        "/api/updateSession",
        &serde_json::json!({
            "projectId": key.project_id.as_str(),
            "sessionId": key.session_id.as_str(),
            "surface": surface,
        }),
        Duration::from_secs(10),
    )?;
    if result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        Ok(())
    } else {
        Err("gxserver returned an invalid command terminal surface update result.".to_string())
    }
}

pub(crate) fn gpui_prepare_command_terminal_attach_plan(
    input: GpuiCommandTerminalCreateInput,
) -> Result<GpuiCommandTerminalAttachPlan, String> {
    let reusable_key = gpui_reusable_command_terminal_gxserver_session_key(&input)?;
    let (key, created) = match reusable_key {
        Some(key) => (key, false),
        None => (gpui_create_command_terminal_gxserver_session(&input)?, true),
    };
    match gpui_prepare_command_terminal_attach_plan_for_key(
        key.clone(),
        input.startup_text.as_deref(),
        None,
    ) {
        Ok(plan) => Ok(plan),
        Err(message) => {
            if created {
                gpui_close_command_terminal_gxserver_session(&key);
            }
            Err(message)
        }
    }
}

pub(crate) fn gpui_reusable_command_terminal_gxserver_session_key(
    input: &GpuiCommandTerminalCreateInput,
) -> Result<Option<GpuiLocalWorkspaceSessionKey>, String> {
    let Some(command_id) = input.command_id.as_deref() else {
        return Ok(None);
    };
    /*
    CDXC:GPUICommandPaneActions 2026-08-09:
    Command-pane layout is client state, but the zmx-backed command session is
    daemon state and survives a GPUI rebuild/relaunch. When the local tab was
    not restored, reclaim the live command-surface session with the same stable
    Action id instead of creating a second daemon session and losing the old
    pane's terminal history.
    */
    let result = gpui_gxserver_rpc_result(
        "/api/readPresentationSnapshot",
        &serde_json::json!({}),
        Duration::from_secs(15),
    )?;
    let sessions = result
        .get("snapshot")
        .and_then(|snapshot| snapshot.get("sessions"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver did not return command terminal sessions.".to_string())?;
    let session = sessions.iter().find_map(|session| {
        let object = session.as_object()?;
        let same_action = gpui_trimmed_json_string_field(object, "projectId")
            == Some(input.project_id.as_str())
            && gpui_trimmed_json_string_field(object, "commandId") == Some(command_id)
            && gpui_trimmed_json_string_field(object, "kind") == Some("terminal")
            && gpui_trimmed_json_string_field(object, "surface") == Some("commands");
        if !same_action {
            return None;
        }
        let lifecycle = gpui_trimmed_json_string_field(object, "lifecycleState")?;
        if !matches!(lifecycle, "running" | "sleeping") {
            return None;
        }
        let session_id = gpui_trimmed_json_string_field(object, "sessionId")?;
        gpui_remote_sidebar_session_id_allowed(session_id).then(|| GpuiLocalWorkspaceSessionKey {
            project_id: input.project_id.clone(),
            session_id: session_id.to_string(),
        })
    });
    Ok(session)
}

pub(crate) fn gpui_prepare_existing_command_terminal_attach_plan(
    key: GpuiLocalWorkspaceSessionKey,
    initial_input: Option<String>,
) -> Result<GpuiCommandTerminalAttachPlan, String> {
    /*
    CDXC:GPUICommandPaneGxserverRestore 2026-08-13:
    A local rebuild restarts gxserver before opening the replacement app. Shell
    restoration can reach this worker before the daemon has rebound its port or
    republished its token; that is startup readiness, not evidence that the
    persisted command session is gone. Keep the restored tab while retrying only
    those transport-readiness failures, then let authoritative attach metadata
    decide whether the session is valid.
    */
    const STARTUP_ATTEMPTS: usize = 61;
    const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(250);

    let mut last_error = None;
    for attempt in 0..STARTUP_ATTEMPTS {
        match gpui_prepare_command_terminal_attach_plan_for_key(
            key.clone(),
            None,
            initial_input.clone(),
        ) {
            Ok(plan) => return Ok(plan),
            Err(message)
                if attempt + 1 < STARTUP_ATTEMPTS
                    && gpui_command_terminal_attach_startup_not_ready(&message) =>
            {
                last_error = Some(message);
                thread::sleep(STARTUP_RETRY_DELAY);
            }
            Err(message) => return Err(message),
        }
    }
    Err(last_error.unwrap_or_else(|| "gxserver is not ready.".to_string()))
}

pub(crate) fn gpui_command_terminal_attach_startup_not_ready(message: &str) -> bool {
    matches!(
        message,
        "gxserver auth token is unavailable."
            | "gxserver auth token is empty."
            | "gxserver is not reachable on 127.0.0.1:58744."
            | "Could not send gxserver request."
            | "Could not read gxserver response."
    )
}

pub(crate) fn gpui_prepare_command_terminal_attach_plan_for_key(
    key: GpuiLocalWorkspaceSessionKey,
    startup_text: Option<&str>,
    initial_input: Option<String>,
) -> Result<GpuiCommandTerminalAttachPlan, String> {
    let plan = gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(
        &key,
        startup_text,
        GpuiLocalWorkspaceAttachIntent::Wake,
    )?;
    Ok(GpuiCommandTerminalAttachPlan {
        attach_command: plan.attach_command,
        command_id: plan.command_id,
        initial_input,
        key,
        title: plan.title,
        working_directory: plan.working_directory,
        zmx_name: plan.zmx_name,
    })
}

pub(crate) fn gpui_encode_uri_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'!'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    encoded
}

pub(crate) fn gpui_random_uuid_string() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    #[cfg(target_os = "windows")]
    {
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status != 0 {
            return Err("Could not generate a project identity.".to_string());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut file = fs::File::open("/dev/urandom")
            .map_err(|_| "Could not generate a project identity.".to_string())?;
        file.read_exact(&mut bytes)
            .map_err(|_| "Could not generate a project identity.".to_string())?;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

pub(crate) fn gpui_project_settings_projects_from_gxserver() -> Vec<serde_json::Value> {
    /*
    CDXC:GPUISettingsProjectMetadata 2026-06-24-11:59:
    Settings project rows in GPUI must come from real local gxserver project domain data, falling back only to the presentation snapshot when `/api/listProjects` is unavailable or lacks usable rows. Do not synthesize paths, names, worktree metadata, or Beads settings from UI labels, session titles, terminal cwd, or local filesystem guesses.
    */
    let domain_projects = gpui_gxserver_domain_projects(Duration::from_secs(2));
    gpui_project_settings_projects_from_domain_projects_or_presentation(&domain_projects)
}

pub(crate) fn gpui_gxserver_domain_projects(timeout: Duration) -> Vec<serde_json::Value> {
    gpui_gxserver_domain_projects_result(timeout).unwrap_or_default()
}

pub(crate) fn gpui_gxserver_domain_projects_result(
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, String> {
    let result = gpui_gxserver_rpc_result("/api/listProjects", &serde_json::json!({}), timeout)?;
    result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
}

pub(crate) fn gpui_gxserver_recent_projects(timeout: Duration) -> Vec<serde_json::Value> {
    gpui_gxserver_rpc_result("/api/listRecentProjects", &serde_json::json!({}), timeout)
        .ok()
        .and_then(|result| {
            result
                .get("recentProjects")
                .and_then(serde_json::Value::as_array)
                .map(|projects| {
                    projects
                        .iter()
                        .filter_map(gpui_recent_project_from_gxserver)
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default()
}

pub(crate) fn gpui_recent_project_from_gxserver(project: &serde_json::Value) -> Option<serde_json::Value> {
    let project = project.as_object()?;
    let project_id = gpui_trimmed_json_string_field(project, "projectId")?;
    let title = gpui_trimmed_json_string_field(project, "title")?;
    let path = gpui_trimmed_json_string_field(project, "path")?;
    let session_count = project
        .get("sessionCount")
        .and_then(json_u64_value)
        .unwrap_or(0);

    let mut item = serde_json::Map::new();
    item.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    item.insert(
        "sessionCount".to_string(),
        serde_json::Value::Number(serde_json::Number::from(session_count)),
    );
    item.insert(
        "title".to_string(),
        serde_json::Value::String(title.to_string()),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "recentClosedAt",
        gpui_trimmed_json_string_field(project, "recentClosedAt"),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "iconDataUrl",
        gpui_trimmed_json_string_field(project, "iconDataUrl"),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "theme",
        gpui_trimmed_json_string_field(project, "theme"),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "themeColor",
        gpui_trimmed_json_string_field(project, "themeColor"),
    );
    if let Some(icon) = project.get("icon").filter(|value| value.is_object()) {
        item.insert("icon".to_string(), icon.clone());
    }
    Some(serde_json::Value::Object(item))
}

#[derive(Clone)]
pub(crate) struct GpuiRecentProjectsRequest {
    pub(crate) machine_id: Option<String>,
    pub(crate) machine_name: Option<String>,
    pub(crate) remote_target: Option<GpuiRemoteGxserverRequestTarget>,
}

pub(crate) fn gpui_find_gxserver_project_by_id(
    project_id: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listProjects",
        &serde_json::json!({}),
        Duration::from_secs(10),
    )?;
    let projects = result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver returned invalid project metadata.".to_string())?;
    gpui_normal_gxserver_project_row_by_id(projects, project_id)
        .cloned()
        .ok_or_else(|| "gxserver project metadata was not found.".to_string())
}

pub(crate) fn gpui_normal_gxserver_project_row_by_id<'a>(
    projects: &'a [serde_json::Value],
    project_id: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    /*
    CDXC:GPUIRecentProjects 2026-06-25-21:36:
    Project Settings metadata writes must reject stale or direct ids that resolve only to explicit parked `/api/listProjects` rows. Skip only boolean `isRecentProject: true`; false, missing, and non-boolean flags remain normal Settings metadata targets, while Recent Project actions keep using `/api/listRecentProjects`.
    */
    let project_id = gpui_trimmed_nonempty_str(Some(project_id))?;
    projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|project| {
            !gpui_gxserver_project_row_is_explicit_recent_project(project)
                && gpui_trimmed_json_string_field(project, "projectId") == Some(project_id)
        })
}

pub(crate) fn gpui_clone_json_object_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> serde_json::Map<String, serde_json::Value> {
    object
        .get(key)
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn gpui_settings_metadata_string_or_null(value: &str) -> serde_json::Value {
    match gpui_trimmed_nonempty_str(Some(value)) {
        Some(value) => serde_json::Value::String(value.to_string()),
        None => serde_json::Value::Null,
    }
}

pub(crate) fn gpui_settings_beads_display_key_or_null(value: &str) -> serde_json::Value {
    let display_key = value
        .trim()
        .chars()
        .flat_map(|ch| ch.to_uppercase())
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(3)
        .collect::<String>();
    if display_key.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(display_key)
    }
}

pub(crate) fn gpui_insert_optional_nonempty_string(
    item: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = gpui_trimmed_nonempty_str(value) {
        item.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

pub(crate) fn gpui_trimmed_json_string_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    gpui_trimmed_nonempty_str(json_string_field(object, key))
}

pub(crate) fn gpui_trimmed_nonempty_str(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiPreviousSessionsRequest {
    pub(crate) cursor: Option<String>,
    pub(crate) limit: usize,
    pub(crate) query: Option<String>,
    pub(crate) request_id: String,
    pub(crate) session_tags: Option<Vec<String>>,
}

#[derive(Default)]
pub(crate) struct GpuiPreviousSessionsPage {
    pub(crate) cursor: Option<String>,
    pub(crate) items: Vec<serde_json::Value>,
}

pub(crate) fn gpui_list_previous_sessions_from_gxserver(
    request: &GpuiPreviousSessionsRequest,
) -> Result<GpuiPreviousSessionsPage, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listPreviousSessions",
        &gpui_previous_sessions_list_params(request),
        Duration::from_secs(10),
    )?;
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver returned invalid previous-session results.".to_string())?;
    Ok(GpuiPreviousSessionsPage {
        cursor: result
            .get("cursor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        items: results
            .iter()
            .filter_map(gpui_gxserver_search_result_to_previous_session_item)
            .collect(),
    })
}

pub(crate) fn gpui_gxserver_search_result_to_previous_session_item(
    result: &serde_json::Value,
) -> Option<serde_json::Value> {
    gpui_gxserver_search_result_to_previous_session_item_with_options(result, "gxserver", None)
}

pub(crate) fn gpui_gxserver_search_result_to_previous_session_item_with_options(
    result: &serde_json::Value,
    history_id_prefix: &str,
    project_name_prefix: Option<&str>,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
    Previous-session rows returned to React must stay contract-shaped and metadata-only. Mirror the existing GPUI TypeScript projection from gxserver search results, including project/session restore identity and title/provider fields, but do not forward raw command text, stdout/stderr, workspace paths, URLs, tokens, gxserver responses, or archived session records.
    */
    let result = result.as_object()?;
    let title = json_string_field(result, "displayTitle")
        .or_else(|| json_string_field(result, "primaryTitle"))
        .or_else(|| json_string_field(result, "title"))
        .unwrap_or("Previous Session");
    let closed_at = json_string_field(result, "closedAt")
        .or_else(|| json_string_field(result, "updatedAt"))
        .or_else(|| json_string_field(result, "createdAt"))?;
    let project_id = json_string_field(result, "projectId")?;
    let session_id = json_string_field(result, "sessionId")?;
    let project_title = json_string_field(result, "projectTitle")?;
    let agent_name =
        json_string_field(result, "agentName").or_else(|| json_string_field(result, "agentId"));
    let agent_icon = json_string_field(result, "agentIcon").or(agent_name);
    let session_persistence_provider =
        json_string_field(result, "sessionPersistenceProvider").unwrap_or("zmx");
    let session_persistence_name = json_string_field(result, "sessionPersistenceName")
        .or_else(|| json_string_field(result, "zmxName"));

    let mut item = serde_json::Map::new();
    item.insert(
        "activity".to_string(),
        serde_json::Value::String("idle".to_string()),
    );
    if let Some(agent_icon) = gpui_sidebar_agent_icon(agent_icon) {
        item.insert(
            "agentIcon".to_string(),
            serde_json::Value::String(agent_icon.to_string()),
        );
    }
    gpui_insert_optional_string(&mut item, "agentId", json_string_field(result, "agentId"));
    gpui_insert_optional_string(
        &mut item,
        "agentSessionId",
        json_string_field(result, "agentSessionId"),
    );
    item.insert(
        "alias".to_string(),
        serde_json::Value::String(title.to_string()),
    );
    item.insert(
        "closedAt".to_string(),
        serde_json::Value::String(closed_at.to_string()),
    );
    item.insert("column".to_string(), serde_json::Value::Number(0.into()));
    gpui_insert_optional_string(
        &mut item,
        "displayTitle",
        json_string_field(result, "displayTitle"),
    );
    gpui_insert_optional_string(
        &mut item,
        "displayTitleTooltip",
        json_string_field(result, "displayTitleTooltip"),
    );
    item.insert(
        "historyId".to_string(),
        serde_json::Value::String(format!("{history_id_prefix}:{project_id}:{session_id}")),
    );
    item.insert(
        "isFavorite".to_string(),
        serde_json::Value::Bool(json_bool_field(result, "isFavorite").unwrap_or(false)),
    );
    item.insert("isFocused".to_string(), serde_json::Value::Bool(false));
    item.insert(
        "isGeneratedName".to_string(),
        serde_json::Value::Bool(false),
    );
    item.insert(
        "isPinned".to_string(),
        serde_json::Value::Bool(json_bool_field(result, "isPinned").unwrap_or(false)),
    );
    item.insert(
        "isPrimaryTitleTerminalTitle".to_string(),
        serde_json::Value::Bool(
            json_bool_field(result, "isPrimaryTitleTerminalTitle").unwrap_or(false),
        ),
    );
    item.insert("isRestorable".to_string(), serde_json::Value::Bool(true));
    item.insert("isRunning".to_string(), serde_json::Value::Bool(false));
    item.insert("isVisible".to_string(), serde_json::Value::Bool(false));
    gpui_insert_optional_string(
        &mut item,
        "lastInteractionAt",
        json_string_field(result, "lastActiveAt"),
    );
    item.insert(
        "lifecycleState".to_string(),
        serde_json::Value::String("done".to_string()),
    );
    item.insert(
        "primaryTitle".to_string(),
        serde_json::Value::String(
            json_string_field(result, "primaryTitle")
                .unwrap_or(title)
                .to_string(),
        ),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    item.insert(
        "projectName".to_string(),
        serde_json::Value::String(
            project_name_prefix
                .map(str::trim)
                .filter(|prefix| !prefix.is_empty())
                .map(|prefix| format!("{prefix} / {project_title}"))
                .unwrap_or_else(|| project_title.to_string()),
        ),
    );
    item.insert("row".to_string(), serde_json::Value::Number(0.into()));
    item.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    item.insert(
        "sessionKind".to_string(),
        serde_json::Value::String("terminal".to_string()),
    );
    gpui_insert_optional_string(
        &mut item,
        "sessionPersistenceName",
        session_persistence_name,
    );
    item.insert(
        "sessionPersistenceProvider".to_string(),
        serde_json::Value::String(session_persistence_provider.to_string()),
    );
    gpui_insert_optional_string(
        &mut item,
        "sessionTag",
        json_string_field(result, "sessionTag"),
    );
    if let Some(sidebar_order) = result
        .get("sidebarOrder")
        .and_then(serde_json::Value::as_number)
        .cloned()
    {
        item.insert(
            "sidebarOrder".to_string(),
            serde_json::Value::Number(sidebar_order),
        );
    }
    item.insert(
        "shortcutLabel".to_string(),
        serde_json::Value::String(String::new()),
    );
    gpui_insert_optional_string(
        &mut item,
        "terminalTitle",
        json_string_field(result, "terminalTitle"),
    );
    Some(serde_json::Value::Object(item))
}

pub(crate) fn gpui_insert_optional_string(
    item: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        item.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

pub(crate) fn gpui_daemon_info_from_gxserver_health(
    health: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if health.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || health.get("product").and_then(serde_json::Value::as_str) != Some(GPUI_GXSERVER_PRODUCT)
    {
        return Err("gxserver health was invalid.".to_string());
    }
    let pid = health
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gxserver health omitted pid.".to_string())?;
    let port = health
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gxserver health omitted port.".to_string())?;
    let protocol_version = health
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gxserver health omitted protocol version.".to_string())?;
    let started_at = health
        .get("startedAt")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "gxserver health omitted start time.".to_string())?;
    Ok(serde_json::json!({
        "pid": pid,
        "port": port,
        "protocolVersion": protocol_version,
        "startedAt": started_at,
    }))
}

pub(crate) fn gpui_read_gxserver_presentation_snapshot() -> Result<serde_json::Value, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/readPresentationSnapshot",
        &serde_json::json!({}),
        Duration::from_secs(10),
    )?;
    result
        .get("snapshot")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .map(serde_json::Value::Object)
        .ok_or_else(|| "gxserver returned an invalid presentation snapshot.".to_string())
}

/*
CDXC:GPUIResourcesDevServers 2026-07-26:
Project the daemon presentation snapshot into the shared titlebar
`TitlebarResourceGroup` contract so Resources sees every project that owns live
sessions, not only the mounted panes of the active project. Session membership
mirrors the shared sidebar projection (group membership, no hidden or
command-surface sessions, agent sessions are terminal resources) and only ids,
titles, project paths, and lifecycle enums cross the bridge.
*/
pub(crate) fn gpui_daemon_session_status_from_gxserver(
    lifecycle_state: &str,
    provider_state: &str,
) -> &'static str {
    match lifecycle_state {
        "running" if provider_state == "exists" || provider_state == "unknown" => "running",
        "running" => "disconnected",
        "stopped" => "exited",
        "sleeping" | "missing" | "unknown" => "disconnected",
        _ => "disconnected",
    }
}

pub(crate) const GPUI_OS_INTEGRATION_EDITOR_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "json", "jsonc", "yaml", "yml", "toml", "ini", "env", "xml", "csv",
    "html", "css", "scss", "js", "jsx", "ts", "tsx", "sh", "bash", "zsh", "fish", "py", "rb", "go",
    "rs", "swift", "java", "kt", "c", "h", "cpp", "hpp", "cs", "php", "lua", "sql",
];
pub(crate) const GPUI_OS_INTEGRATION_STATUS_EDITOR_EXTENSIONS: &[&str] =
    &["txt", "md", "json", "js", "ts", "sh"];
pub(crate) const GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS: &[&str] = &["command", "tool", "sh"];

pub(crate) fn gpui_set_os_integration_defaults_status_message(target: Option<&str>) -> serde_json::Value {
    let status_items = gpui_set_os_integration_defaults(target);
    let mut payload = gpui_os_integration_status_message();
    gpui_merge_os_integration_status_items(&mut payload, status_items);
    payload
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_set_os_integration_defaults(target: Option<&str>) -> Vec<serde_json::Value> {
    /*
    CDXC:GPUIOSIntegration 2026-06-24-15:02:
    GPUI Settings may mutate Launch Services defaults only from explicit OS Integration button clicks. Status refreshes and startup must stay read-only, while this path targets only the requested editor, ghostex:// terminal-link, script-runner, or all roles.

    CDXC:GPUIOSIntegration 2026-06-24-15:10:
    Default mutations must return privacy-safe status items for the reused Settings UI. Capture per-extension and per-scheme Launch Services failures as enum reasons only; never expose bundle paths, file paths, URLs, command text, environment values, stdout/stderr, daemon bodies, or raw OSStatus values.
    */
    let mut status_items = Vec::new();
    let Some(target) = target else {
        status_items.push(gpui_os_integration_status_item(
            "platform",
            "setDefault",
            "skipped",
            "invalidTarget",
            None,
            None,
        ));
        return status_items;
    };
    if !matches!(target, "editor" | "terminalLinks" | "scriptRunner" | "all") {
        status_items.push(gpui_os_integration_status_item(
            "platform",
            "setDefault",
            "skipped",
            "invalidTarget",
            None,
            None,
        ));
        return status_items;
    }
    let Some(bundle) = gpui_macos_os_integration_bundle_info() else {
        status_items.push(gpui_os_integration_status_item(
            "bundleRegistration",
            "setDefault",
            "failed",
            "bundleIdentifierMissing",
            None,
            None,
        ));
        return status_items;
    };

    match gpui_macos_register_os_integration_bundle(&bundle.bundle_root) {
        Some(status) if status == 0 => {}
        _ => status_items.push(gpui_os_integration_status_item(
            "bundleRegistration",
            "registerBundle",
            "failed",
            "bundleRegistrationFailed",
            None,
            None,
        )),
    }
    if target == "editor" || target == "all" {
        for file_extension in GPUI_OS_INTEGRATION_EDITOR_EXTENSIONS {
            let Some(content_type) = gpui_macos_content_type_for_extension(file_extension) else {
                status_items.push(gpui_os_integration_status_item(
                    "editor",
                    "setDefault",
                    "skipped",
                    "contentTypeUnavailable",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let Some(bundle_identifier) = GpuiCfString::new(&bundle.bundle_identifier) else {
                status_items.push(gpui_os_integration_status_item(
                    "editor",
                    "setDefault",
                    "failed",
                    "bundleIdentifierMissing",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let status = unsafe {
                LSSetDefaultRoleHandlerForContentType(
                    content_type.as_ref(),
                    K_LS_ROLES_EDITOR,
                    bundle_identifier.as_ref(),
                )
            };
            if status != 0 {
                status_items.push(gpui_os_integration_status_item(
                    "editor",
                    "setDefault",
                    "failed",
                    "launchServicesRejected",
                    Some(file_extension),
                    None,
                ));
            }
        }
    }
    if target == "terminalLinks" || target == "all" {
        if let (Some(scheme), Some(bundle_identifier)) = (
            GpuiCfString::new("ghostex"),
            GpuiCfString::new(&bundle.bundle_identifier),
        ) {
            let status = unsafe {
                LSSetDefaultHandlerForURLScheme(scheme.as_ref(), bundle_identifier.as_ref())
            };
            if status != 0 {
                status_items.push(gpui_os_integration_status_item(
                    "terminalLinks",
                    "setDefault",
                    "failed",
                    "launchServicesRejected",
                    None,
                    Some("ghostex"),
                ));
            }
        } else {
            status_items.push(gpui_os_integration_status_item(
                "terminalLinks",
                "setDefault",
                "failed",
                "bundleIdentifierMissing",
                None,
                Some("ghostex"),
            ));
        }
    }
    if target == "scriptRunner" || target == "all" {
        for file_extension in GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS {
            let Some(content_type) = gpui_macos_content_type_for_extension(file_extension) else {
                status_items.push(gpui_os_integration_status_item(
                    "scriptRunner",
                    "setDefault",
                    "skipped",
                    "contentTypeUnavailable",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let Some(bundle_identifier) = GpuiCfString::new(&bundle.bundle_identifier) else {
                status_items.push(gpui_os_integration_status_item(
                    "scriptRunner",
                    "setDefault",
                    "failed",
                    "bundleIdentifierMissing",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let status = unsafe {
                LSSetDefaultRoleHandlerForContentType(
                    content_type.as_ref(),
                    K_LS_ROLES_SHELL,
                    bundle_identifier.as_ref(),
                )
            };
            if status != 0 {
                status_items.push(gpui_os_integration_status_item(
                    "scriptRunner",
                    "setDefault",
                    "failed",
                    "launchServicesRejected",
                    Some(file_extension),
                    None,
                ));
            }
        }
    }
    status_items
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_set_os_integration_defaults(target: Option<&str>) -> Vec<serde_json::Value> {
    if matches!(
        target,
        Some("editor") | Some("terminalLinks") | Some("scriptRunner") | Some("all")
    ) {
        return Vec::new();
    }
    vec![gpui_os_integration_status_item(
        "platform",
        "setDefault",
        "skipped",
        "invalidTarget",
        None,
        None,
    )]
}

pub(crate) fn gpui_os_integration_status_message() -> serde_json::Value {
    gpui_os_integration_status_payload()
}

pub(crate) fn gpui_merge_os_integration_status_items(
    payload: &mut serde_json::Value,
    status_items: Vec<serde_json::Value>,
) {
    if status_items.is_empty() {
        return;
    }
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    match object.get_mut("statusItems") {
        Some(serde_json::Value::Array(existing_items)) => {
            existing_items.extend(status_items);
        }
        _ => {
            object.insert(
                "statusItems".to_string(),
                serde_json::Value::Array(status_items),
            );
        }
    }
}

pub(crate) fn gpui_os_integration_status_item(
    target: &str,
    operation: &str,
    status: &str,
    reason: &str,
    file_extension: Option<&str>,
    scheme: Option<&str>,
) -> serde_json::Value {
    let mut item = serde_json::Map::new();
    item.insert(
        "operation".to_string(),
        serde_json::Value::String(operation.to_string()),
    );
    item.insert(
        "reason".to_string(),
        serde_json::Value::String(reason.to_string()),
    );
    item.insert(
        "status".to_string(),
        serde_json::Value::String(status.to_string()),
    );
    item.insert(
        "target".to_string(),
        serde_json::Value::String(target.to_string()),
    );
    if let Some(file_extension) = file_extension {
        item.insert(
            "extension".to_string(),
            serde_json::Value::String(file_extension.to_string()),
        );
    }
    if let Some(scheme) = scheme {
        item.insert(
            "scheme".to_string(),
            serde_json::Value::String(scheme.to_string()),
        );
    }
    serde_json::Value::Object(item)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_os_integration_status_payload() -> serde_json::Value {
    /*
    CDXC:GPUIOSIntegration 2026-06-24-15:02:
    Non-macOS builds cannot inspect or mutate macOS Launch Services. Keep the shared status payload honest with unavailable registrations and no default handlers instead of inventing platform parity.
    */
    serde_json::json!({
        "bundleIdentifier": "com.madda.ghostex.gpui-unavailable",
        "editorDefaults": {},
        "generatedAt": gpui_status_generated_at(),
        "registeredEditableFiles": false,
        "registeredGhostexURLScheme": false,
        "registeredScriptRunner": false,
        "scriptDefaults": {},
        "statusItems": [
            gpui_os_integration_status_item(
                "platform",
                "readStatus",
                "unsupported",
                "unsupportedPlatform",
                None,
                None,
            )
        ],
        "terminalLinkDefaultBundleId": "GPUI Launch Services bridge unavailable",
        "type": "osIntegrationStatus",
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_status_payload() -> serde_json::Value {
    /*
    CDXC:GPUIOSIntegration 2026-06-24-15:02:
    Settings OS integration status should mirror the Swift host payload: app bundle id, Launch Services defaults for representative editor/script extensions, ghostex:// default handler, and Info.plist registration booleans. This function is read-only and must not set defaults or register the app on status requests.
    */
    let bundle = gpui_macos_os_integration_bundle_info();
    let bundle_identifier = bundle
        .as_ref()
        .map(|info| info.bundle_identifier.as_str())
        .unwrap_or("com.madda.ghostex.gpui-unavailable");
    let info_plist = bundle
        .as_ref()
        .map(|info| info.info_plist.as_str())
        .unwrap_or("");
    let mut payload = serde_json::json!({
        "bundleIdentifier": bundle_identifier,
        "editorDefaults": gpui_macos_default_role_handlers(
            GPUI_OS_INTEGRATION_STATUS_EDITOR_EXTENSIONS,
            K_LS_ROLES_EDITOR,
        ),
        "generatedAt": gpui_status_generated_at(),
        "registeredEditableFiles": gpui_os_integration_has_editable_registration(info_plist),
        "registeredGhostexURLScheme": gpui_os_integration_has_ghostex_url_registration(info_plist),
        "registeredScriptRunner": gpui_os_integration_has_script_registration(info_plist),
        "scriptDefaults": gpui_macos_default_role_handlers(
            GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS,
            K_LS_ROLES_SHELL,
        ),
        "type": "osIntegrationStatus",
    });
    if bundle.is_none() {
        gpui_merge_os_integration_status_items(
            &mut payload,
            vec![gpui_os_integration_status_item(
                "bundleRegistration",
                "readStatus",
                "failed",
                "bundleIdentifierMissing",
                None,
                None,
            )],
        );
    }
    if let Some(handler) = gpui_macos_default_url_scheme_handler("ghostex") {
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "terminalLinkDefaultBundleId".to_string(),
                serde_json::Value::String(handler),
            );
        }
    }
    payload
}

#[cfg(target_os = "macos")]
pub(crate) struct GpuiOSIntegrationBundleInfo {
    pub(crate) bundle_identifier: String,
    pub(crate) bundle_root: PathBuf,
    pub(crate) info_plist: String,
}

pub(crate) fn gpui_gxserver_rpc_result(
    endpoint: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:48:
    Settings status/actions share the typed-operation transport and may expose only the validated gxserver `result` object to the modal host. Transport, status, envelope, and parse failures stay as local errors so callers can clear loading with explicit empty status payloads without logging private daemon data.
    */
    let (status_code, body) = gxserver_post_typed_operation(endpoint, params, timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver request failed with HTTP {status_code}."));
    }
    parse_gpui_gxserver_rpc_result(&body)
}

pub(crate) fn gpui_update_portless_gxserver_state(
    update: GpuiPortlessStateUpdate,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:GPUISettingsPortlessBridge 2026-06-24-11:48:
    Successful Portless state updates return canonical gxserver status and presentation metadata. Parse just enough of that response to refresh the shared app-modal `hud.portless` payload immediately, while transport/parser failures remain silent local `Result` values so unavailable gxserver cannot create fake success or roll back saved Settings.
    */
    let result = gpui_gxserver_rpc_result(
        "/api/updatePortlessState",
        &update.to_rpc_params(),
        Duration::from_secs(10),
    )?;
    gpui_sidebar_portless_state_from_update_result(&result)
        .ok_or_else(|| "gxserver returned invalid Portless state.".to_string())
}

pub(crate) fn gpui_gxserver_server_health(timeout: Duration) -> Result<serde_json::Value, String> {
    let (status_code, body) = gxserver_get_typed_operation("/api/health/server", timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver health failed with HTTP {status_code}."));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|_| "gxserver health returned invalid JSON.".to_string())
}

pub(crate) const GPUI_GXSERVER_DAEMON_TOAST_ID: &str = "toast-gxserver-daemon";
pub(crate) const GPUI_MISSING_MONACO_PROMPT_EDITOR_TOAST_ID: &str = "toast-monaco-prompt-editor-missing";
pub(crate) const GPUI_GXSERVER_EXPECTED_PRODUCT: &str = "gxserver";

pub(crate) enum GpuiLocalGxserverHealthState {
    Healthy { tools_available: bool },
    BuildMismatch,
    ProtocolMismatch { reported: Option<u64> },
    Unreachable,
}

/// Mirrors the macOS GxserverClient handshake: authenticated health, product
/// check, hard protocol-version and build-identity matches, and availability
/// of the tools shipped beside this GPUI build's gxserver binary.
pub(crate) fn gpui_probe_local_gxserver_health() -> GpuiLocalGxserverHealthState {
    let Ok(health) = gpui_gxserver_server_health(Duration::from_millis(1000)) else {
        return GpuiLocalGxserverHealthState::Unreachable;
    };
    if health.get("product").and_then(serde_json::Value::as_str)
        != Some(GPUI_GXSERVER_EXPECTED_PRODUCT)
    {
        return GpuiLocalGxserverHealthState::Unreachable;
    }
    let reported_protocol = health
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64);
    if reported_protocol != Some(GPUI_GXSERVER_PROTOCOL_VERSION) {
        return GpuiLocalGxserverHealthState::ProtocolMismatch {
            reported: reported_protocol,
        };
    }
    #[cfg(not(target_os = "windows"))]
    if let Some(expected_build_identity) = gpui_expected_local_gxserver_build_identity() {
        let reported_build_identity = health
            .get("buildIdentity")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if reported_build_identity != Some(expected_build_identity.as_str()) {
            return GpuiLocalGxserverHealthState::BuildMismatch;
        }
    }
    GpuiLocalGxserverHealthState::Healthy {
        tools_available: gpui_gxserver_required_tools_available(&health),
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_expected_local_gxserver_build_identity() -> Option<String> {
    let binary = gpui_resolve_local_gxserver_binary()?;
    let package_root = binary.parent()?.parent()?;
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(package_root.join("build-identity.json")).ok()?)
            .ok()?;
    value
        .get("buildIdentity")?
        .as_str()
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .map(str::to_string)
}

/// A running app can outlive replacement of its app bundle during an update.
/// Reconcile that version skew at the durable Delayed Send command boundary so
/// the user's submission is persisted by the daemon that implements the
/// bundled contract instead of being accepted by the modal and rejected by an
/// obsolete control plane.
pub(crate) fn gpui_schedule_agents_delayed_send_with_current_gxserver_build(
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if matches!(
        gpui_probe_local_gxserver_health(),
        GpuiLocalGxserverHealthState::BuildMismatch
    ) {
        gpui_restart_local_gxserver_for_current_build()?;
    }
    gpui_gxserver_rpc_result("/api/scheduleDelayedSend", params, Duration::from_secs(5))
}

pub(crate) fn gpui_restart_local_gxserver_for_current_build() -> Result<(), String> {
    let (status_code, _) = gxserver_post_typed_operation(
        "/api/control/stop",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver stop failed with HTTP {status_code}."));
    }

    let mut stopped = false;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        if matches!(
            gpui_probe_local_gxserver_health(),
            GpuiLocalGxserverHealthState::Unreachable
        ) {
            stopped = true;
            break;
        }
    }
    if !stopped {
        return Err("gxserver did not stop before its build update.".to_string());
    }

    let binary = gpui_resolve_local_gxserver_binary()
        .ok_or_else(|| "Bundled gxserver binary is missing.".to_string())?;
    gpui_spawn_local_gxserver_daemon(&binary)?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(500));
        match gpui_probe_local_gxserver_health() {
            GpuiLocalGxserverHealthState::Healthy {
                tools_available: true,
            } => return Ok(()),
            GpuiLocalGxserverHealthState::Healthy {
                tools_available: false,
            } => {
                return Err(
                    "The current gxserver build is missing its required toolchain.".to_string(),
                );
            }
            GpuiLocalGxserverHealthState::ProtocolMismatch { .. } => {
                return Err("The current gxserver build has an incompatible protocol.".to_string());
            }
            GpuiLocalGxserverHealthState::BuildMismatch
            | GpuiLocalGxserverHealthState::Unreachable => {}
        }
    }
    Err("The current gxserver build did not become healthy in time.".to_string())
}

pub(crate) fn gpui_gxserver_required_tools_available(health: &serde_json::Value) -> bool {
    let Some(tools) = health.get("tools").and_then(serde_json::Value::as_array) else {
        return true;
    };
    // zmx is the one mandatory GPUI daemon companion.
    //
    // CDXC:AgentHistorySearch 2026-08-20: Zehn used to be gated here too,
    // because it was a separate bundled binary a build might not carry. It is
    // now a Rust crate compiled into gxserver, so every daemon that exists can
    // serve prompt-history search and there is nothing left to probe for.
    //
    // Beads is deliberately excluded. gxserver resolves `bd` from the user's
    // machine-installed Beads release (see `system_bd_tool_candidates`) and
    // never from the app bundle, so a missing `bd` reports the operator's
    // environment, not a stale daemon. Gating on it made a first launch on a
    // machine without Beads report "gxserver toolchain unavailable" and then
    // restart a perfectly healthy daemon on every later launch. Project board
    // surfaces already carry their own install guidance for that case.
    let required_tools = vec!["zmx"];
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        return required_tools.iter().all(|required| {
            tools.iter().any(|tool| {
                tool.get("tool").and_then(serde_json::Value::as_str) == Some(*required)
                    && tool.get("availability").and_then(serde_json::Value::as_str)
                        == Some("available")
            })
        });
    }
    required_tools.iter().all(|required| {
        tools.iter().any(|tool| {
            tool.get("tool").and_then(serde_json::Value::as_str) == Some(*required)
                && tool.get("availability").and_then(serde_json::Value::as_str) == Some("available")
        })
    })
}

/// Resolution order is explicit env selection, this app bundle's Resources,
/// then the GPUI-owned development runtime next to this crate. Only native
/// gxserver executables are launched.
pub(crate) fn gpui_resolve_local_gxserver_binary() -> Option<PathBuf> {
    for key in ["GHOSTEX_GXSERVER_CLI", "GHOSTEX_GXSERVER_BIN"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                let path = PathBuf::from(trimmed);
                if path.is_absolute() && gpui_is_executable_file(&path) {
                    return Some(path);
                }
                return None;
            }
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(contents_dir) = current_exe.parent().and_then(Path::parent) {
            candidates.push(contents_dir.join("Resources/Web/gxserver/bin/gxserver"));
        }
        // CDXC:GPUILinuxX11Backend 2026-07-05: the Linux app is a flat
        // CEF-conventional directory (scripts/build-linux-app.sh), so the
        // bundled gxserver package sits beside the executable instead of
        // under a macOS Contents/Resources tree.
        #[cfg(target_os = "linux")]
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join("gxserver/bin/gxserver"));
        }
    }
    if let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
    {
        candidates.push(repo_root.join("apps/desktop/runtime/macos/Web/gxserver/bin/gxserver"));
        // Linux dev runs resolve the package produced by
        // `bun gxserver-rs/package-remote-linux.mjs` before any packaging step.
        #[cfg(target_os = "linux")]
        {
            #[cfg(target_arch = "x86_64")]
            const GPUI_LINUX_GXSERVER_PACKAGE_ARCH: &str = "x64";
            #[cfg(target_arch = "aarch64")]
            const GPUI_LINUX_GXSERVER_PACKAGE_ARCH: &str = "arm64";
            candidates.push(repo_root.join(format!(
                "build/remote-gxserver-linux/{GPUI_LINUX_GXSERVER_PACKAGE_ARCH}/package/bin/gxserver"
            )));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| gpui_is_executable_file(candidate))
}

/// The bundled zmx binary ships beside the bundled gxserver binary in every
/// GPUI package layout (`Resources/Web/gxserver/bin`, the Linux flat app
/// directory, and the development tree), so it resolves as a sibling of the
/// resolved gxserver executable. Mirrors macOS `nativeBundledZmxExecutablePath`.
pub(crate) fn gpui_resolve_local_zmx_binary() -> Option<PathBuf> {
    let gxserver = gpui_resolve_local_gxserver_binary()?;
    let candidate = gxserver.parent()?.join("zmx");
    gpui_is_executable_file(&candidate).then_some(candidate)
}

/*
CDXC:GPUIZmxPersistenceRefresh 2026-07-06:
Terminal-content clicks should repair a zmx session that another client
resized, but a click inside an already-correct pane must not repaint the
terminal because a repaint scrolls the view to the visible bottom. Mirror
macOS `nativeRunZmxRefreshIfStaleProcess`: run bundled
`zmx refresh-if-stale <name> <rows> <cols>` through zmx IPC outside the
terminal input/output path, on a background thread, with a one-second
deadline, discarding output.
*/
pub(crate) fn gpui_gxserver_launch_log_path() -> PathBuf {
    shared_settings::ghostex_storage_paths()
        .logs_dir
        .join("gxserver")
        .join("macos-launch.log")
}

pub(crate) fn gpui_normalized_user_tool_path(current_path: Option<&str>) -> String {
    /*
    CDXC:GPUIUserToolPathEntries 2026-07-24:
    Keep the normal user tool locations used by the GPUI process and its local
    daemon bootstrap in one place. Packaged macOS apps otherwise inherit a
    sparse LaunchServices PATH that cannot see Homebrew-installed Ghostex tools.
    */
    let home = env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mut entries = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
    ];
    if let Some(home) = home.as_deref() {
        entries.extend([
            format!("{home}/.volta/bin"),
            format!("{home}/.local/share/mise/shims"),
            format!("{home}/.local/bin"),
            format!("{home}/.asdf/shims"),
            format!("{home}/.nodenv/shims"),
        ]);
    }
    entries.extend([
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ]);
    if let Some(current_path) = current_path {
        entries.extend(current_path.split(':').map(str::to_string));
    }

    let mut seen = HashSet::new();
    entries
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty() && seen.insert(entry.clone()))
        .collect::<Vec<_>>()
        .join(":")
}

/// The gxserver launchd agent label is client-agnostic on purpose: one
/// per-user gxserver serves the Swift app, GPUI, and the CLI, so whichever
/// client bootstraps it owns the same job definition.
#[cfg(target_os = "macos")]
pub(crate) const GPUI_GXSERVER_LAUNCH_AGENT_LABEL: &str = "com.madda.ghostex.gxserver";

#[cfg(target_os = "macos")]
pub(crate) fn gpui_launchd_plist_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/*
CDXC:GPUIGxserverLaunchdJob 2026-07-10:
macOS 27 attributes app-spawned processes that outlive the app to the app's
Background Task Management identity, and the Dock then renders the dim
"Running in Background" indicator instead of the normal running dot even
while the app is open. Registered launchd jobs carry their own BTM identity
(the TeamViewer_Service and sh.portless.proxy precedents), so the persistent
gxserver daemon -- and every zmx/node process it goes on to spawn -- must
enter the user session as a launchd agent instead of a detached nohup child
of the GPUI process. launchd starts the job with a clean environment, which
also covers the color-blocker and session-identity stripping the nohup path
performed; CLICOLOR mirrors terminal_environment's color opt-in.
*/
#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<(), String> {
    const LAUNCH_FAILURE: &str = "gxserver failed to launch.";
    let Some(binary_path) = binary.to_str() else {
        return Err(LAUNCH_FAILURE.to_string());
    };
    let launch_log = gpui_gxserver_launch_log_path();
    if let Some(parent) = launch_log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Some(launch_log_path) = launch_log.to_str() else {
        return Err(LAUNCH_FAILURE.to_string());
    };
    let home = env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LAUNCH_FAILURE.to_string())?;
    let agents_dir = PathBuf::from(&home).join("Library/LaunchAgents");
    std::fs::create_dir_all(&agents_dir).map_err(|_| LAUNCH_FAILURE.to_string())?;
    let plist_path = agents_dir.join(format!("{GPUI_GXSERVER_LAUNCH_AGENT_LABEL}.plist"));

    let current_path = env::var("PATH").ok();
    let mut environment_entries = vec![
        (
            "PATH".to_string(),
            gpui_normalized_user_tool_path(current_path.as_deref()),
        ),
        ("CLICOLOR".to_string(), "1".to_string()),
    ];
    for variable in [
        "GHOSTEX_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        if let Some(value) = env::var(variable)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| Path::new(value).is_absolute())
        {
            environment_entries.push((variable.to_string(), value));
        }
    }
    let environment_xml = environment_entries
        .iter()
        .map(|(key, value)| {
            format!(
                "\t\t<key>{}</key>\n\t\t<string>{}</string>\n",
                gpui_launchd_plist_xml_escape(key),
                gpui_launchd_plist_xml_escape(value),
            )
        })
        .collect::<String>();
    // RunAtLoad stays false so the daemon keeps its on-demand lifecycle:
    // loading the job at login must not start gxserver; only an explicit
    // kickstart from a client that found it unreachable does.
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{binary}</string>
		<string>--foreground</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
{environment_xml}	</dict>
	<key>RunAtLoad</key>
	<false/>
	<key>KeepAlive</key>
	<false/>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>{launch_log}</string>
	<key>StandardErrorPath</key>
	<string>{launch_log}</string>
</dict>
</plist>
"#,
        label = gpui_launchd_plist_xml_escape(GPUI_GXSERVER_LAUNCH_AGENT_LABEL),
        binary = gpui_launchd_plist_xml_escape(binary_path),
        environment_xml = environment_xml,
        launch_log = gpui_launchd_plist_xml_escape(launch_log_path),
    );
    std::fs::write(&plist_path, plist).map_err(|_| LAUNCH_FAILURE.to_string())?;

    let uid_output = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| LAUNCH_FAILURE.to_string())?;
    let uid = String::from_utf8_lossy(&uid_output.stdout)
        .trim()
        .to_string();
    if uid.is_empty() {
        return Err(LAUNCH_FAILURE.to_string());
    }
    let job_target = format!("gui/{uid}/{GPUI_GXSERVER_LAUNCH_AGENT_LABEL}");
    let domain_target = format!("gui/{uid}");

    let run_launchctl = |arguments: &[&str]| -> bool {
        std::process::Command::new("/bin/launchctl")
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    };
    // This path only runs when the daemon health probe reported unreachable,
    // so removing a stale or hung job definition before loading the fresh
    // plist (whose binary path changes across app versions) is safe.
    let _ = run_launchctl(&["bootout", &job_target]);
    let bootstrapped = run_launchctl(&["bootstrap", &domain_target, &plist_path.to_string_lossy()]);
    // A concurrent client may have loaded the same label between the bootout
    // and bootstrap (making bootstrap fail) or already started the job
    // (making kickstart report already-active), so either success signal
    // means the job is loaded; the caller's health-probe loop owns the
    // final running/not-running verdict.
    let kickstarted = run_launchctl(&["kickstart", &job_target]);
    support_logs::append(
        support_logs::GpuiSupportLog::HostLifecycle,
        "gpui.gxserverBootstrap.launchdAgent",
        serde_json::json!({
            "bootstrapped": bootstrapped,
            "kickstarted": kickstarted,
        }),
    );
    if !bootstrapped && !kickstarted {
        return Err(LAUNCH_FAILURE.to_string());
    }
    Ok(())
}

/// Launches the daemon exactly like the macOS client used to: a
/// shell-detached `nohup <gxserver> --foreground` so the process is
/// app-independent and survives quitting Ghostex. The app never retains the
/// child as ownership. (Linux has no Dock background-attribution concern, so
/// the detached-child launch remains correct there.)
#[cfg(target_os = "linux")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<(), String> {
    let Some(binary_path) = binary.to_str() else {
        return Err("gxserver failed to launch.".to_string());
    };
    let launch_log = gpui_gxserver_launch_log_path();
    if let Some(parent) = launch_log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Some(launch_log_path) = launch_log.to_str() else {
        return Err("gxserver failed to launch.".to_string());
    };
    let command = format!(
        "nohup {} --foreground >>{} 2>&1 </dev/null &",
        gpui_shell_single_quote(binary_path),
        gpui_shell_single_quote(launch_log_path),
    );
    let mut shell = std::process::Command::new("/bin/sh");
    terminal_environment::apply_color_capable_process_command(&mut shell);
    terminal_environment::remove_session_identity_from_process_command(&mut shell);
    let current_path = env::var("PATH").ok();
    shell.env(
        "PATH",
        gpui_normalized_user_tool_path(current_path.as_deref()),
    );
    let status = shell
        .arg("-c")
        .arg(&command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    if !status.success() {
        return Err("gxserver failed to launch.".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let launch_log = gpui_gxserver_launch_log_path();
    if let Some(parent) = launch_log.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "gxserver failed to launch.".to_string())?;
    }
    let stdout = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&launch_log)
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    let stderr = stdout
        .try_clone()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    let mut command = std::process::Command::new(binary);
    terminal_environment::apply_color_capable_process_command(&mut command);
    terminal_environment::remove_session_identity_from_process_command(&mut command);
    command
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr))
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    Ok(())
}

/// Last few launch-log lines so a startup-timeout toast can say why, matching
/// the macOS client's recentGxserverLaunchOutput.
pub(crate) fn gpui_recent_gxserver_launch_output() -> Option<String> {
    let text = std::fs::read_to_string(gpui_gxserver_launch_log_path()).ok()?;
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(6);
    Some(lines[start..].join(" "))
}

pub(crate) fn gpui_gxserver_protocol_mismatch_message(reported: Option<u64>) -> String {
    format!(
        "gxserver protocol mismatch. Expected protocol {GPUI_GXSERVER_PROTOCOL_VERSION}, got {}. Update Ghostex and gxserver so their protocol versions match.",
        reported.map_or_else(|| "none".to_string(), |version| version.to_string()),
    )
}

pub(crate) fn gpui_sidebar_hud_from_gxserver(
    timeout: Duration,
    active_project_id: Option<&str>,
) -> Result<GpuiSidebarHudButtons, String> {
    /*
    CDXC:SidebarHudContract 2026-06-24-20:34:
    GPUI Settings/app-modal and titlebar reads must use gxserver's normalized sidebar HUD contract instead of recreating custom agent/action projection in host Rust. If the endpoint is unavailable, callers leave those read rows empty rather than falling back to a second custom metadata normalizer.
    */
    let mut params = serde_json::Map::new();
    if let Some(active_project_id) =
        active_project_id.and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
    {
        params.insert(
            "activeProjectId".to_string(),
            serde_json::Value::String(active_project_id.to_string()),
        );
    }
    let result = gpui_gxserver_rpc_result(
        "/api/readSidebarHud",
        &serde_json::Value::Object(params),
        timeout,
    )?;
    Ok(GpuiSidebarHudButtons {
        agents: gpui_sidebar_hud_array_field(&result, "agents")?,
        commands: gpui_sidebar_hud_array_field(&result, "commands")?,
        /*
        A gxserver older than this app omits globalCommands entirely, so treat a
        missing list as empty rather than failing the whole HUD read and blanking
        Actions that do exist.
        */
        global_commands: gpui_sidebar_hud_array_field(&result, "globalCommands")
            .unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
    })
}

pub(crate) fn gpui_persist_sidebar_agents_to_gxserver_projects(
    domain_projects: &[serde_json::Value],
    agents: &[GpuiStoredSidebarAgent],
    agent_order: &[String],
) -> Result<(), String> {
    let mut updated_any = false;
    for project in domain_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
    {
        let Some(project_id) = gpui_trimmed_json_string_field(project, "projectId") else {
            continue;
        };
        let mut params = serde_json::Map::new();
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
        params.insert(
            "customAgents".to_string(),
            gpui_stored_sidebar_agents_value(agents),
        );
        params.insert(
            "customAgentOrder".to_string(),
            gpui_string_array_value(agent_order),
        );
        let result = gpui_gxserver_rpc_result(
            "/api/updateProject",
            &serde_json::Value::Object(params),
            Duration::from_secs(10),
        )?;
        if result
            .get("project")
            .and_then(serde_json::Value::as_object)
            .is_none()
        {
            return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
        }
        updated_any = true;
    }
    if updated_any {
        Ok(())
    } else {
        Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
    }
}

pub(crate) fn gpui_read_gxserver_app_user_data(timeout: Duration) -> GpuiAppModalProductState {
    /*
    CDXC:GxserverAppUserData 2026-06-24-13:30:
    GPUI app-modal hydrate reads Scratch Pad and Pinned Prompts from gxserver's
    shared app-user-data snapshot instead of the old GPUI product-state file.
    Parse only the shared React contract fields and silently drop malformed
    prompt rows without logging note or prompt content.
    */
    gpui_gxserver_rpc_result("/api/readAppUserData", &serde_json::json!({}), timeout)
        .ok()
        .and_then(|value| gpui_app_modal_product_state_from_value(&value))
        .unwrap_or_default()
}

pub(crate) fn gpui_save_gxserver_scratch_pad(content: &str) -> Result<(), String> {
    gpui_gxserver_rpc_result(
        "/api/saveScratchPad",
        &serde_json::json!({ "content": content }),
        Duration::from_secs(5),
    )
    .map(|_| ())
}

pub(crate) fn gpui_save_gxserver_pinned_prompt(
    content: &str,
    title: &str,
    prompt_id: Option<&str>,
) -> Result<(), String> {
    let mut params = serde_json::Map::new();
    params.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    params.insert(
        "title".to_string(),
        serde_json::Value::String(title.to_string()),
    );
    if let Some(prompt_id) = prompt_id {
        params.insert(
            "promptId".to_string(),
            serde_json::Value::String(prompt_id.to_string()),
        );
    }
    gpui_gxserver_rpc_result(
        "/api/savePinnedPrompt",
        &serde_json::Value::Object(params),
        Duration::from_secs(5),
    )
    .map(|_| ())
}

pub(crate) fn gpui_app_modal_product_state_from_value(
    value: &serde_json::Value,
) -> Option<GpuiAppModalProductState> {
    let object = value.as_object()?;
    Some(GpuiAppModalProductState {
        pinned_prompts: gpui_pinned_prompts_from_value(object.get("pinnedPrompts")),
        scratch_pad_content: object
            .get("scratchPadContent")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

pub(crate) fn gpui_pinned_prompts_from_value(value: Option<&serde_json::Value>) -> Vec<GpuiPinnedPrompt> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    gpui_normalize_pinned_prompts(
        items
            .iter()
            .filter_map(gpui_pinned_prompt_from_value)
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn gpui_pinned_prompt_from_value(value: &serde_json::Value) -> Option<GpuiPinnedPrompt> {
    let object = value.as_object()?;
    let prompt_id = object.get("promptId")?.as_str()?.trim().to_string();
    let content = object.get("content")?.as_str()?.to_string();
    let created_at = object.get("createdAt")?.as_str()?.to_string();
    let title = object
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let updated_at = object.get("updatedAt")?.as_str()?.to_string();
    if prompt_id.is_empty() || content.is_empty() || created_at.is_empty() || updated_at.is_empty()
    {
        return None;
    }
    Some(GpuiPinnedPrompt {
        title: gpui_normalize_pinned_prompt_title(title, &content),
        content,
        created_at,
        prompt_id,
        updated_at,
    })
}

pub(crate) fn gpui_normalize_pinned_prompts(mut prompts: Vec<GpuiPinnedPrompt>) -> Vec<GpuiPinnedPrompt> {
    prompts.retain(|prompt| {
        !prompt.prompt_id.is_empty()
            && !prompt.content.is_empty()
            && !prompt.created_at.is_empty()
            && !prompt.updated_at.is_empty()
    });
    prompts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    prompts
}

pub(crate) fn gpui_pinned_prompt_value(prompt: &GpuiPinnedPrompt) -> serde_json::Value {
    serde_json::json!({
        "content": prompt.content.clone(),
        "createdAt": prompt.created_at.clone(),
        "promptId": prompt.prompt_id.clone(),
        "title": prompt.title.clone(),
        "updatedAt": prompt.updated_at.clone(),
    })
}

pub(crate) fn gpui_normalize_pinned_prompt_title(title: &str, content: &str) -> String {
    let trimmed_title = title.trim();
    if !trimmed_title.is_empty() {
        return trimmed_title.to_string();
    }
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(80).collect::<String>())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Untitled Prompt".to_string())
}

pub(crate) fn kanban_workarea_runtime_url_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    /*
    CDXC:GPUIProjectWorkareaRuntimeCefBundles 2026-06-24-11:03:
    Kanban runtime URL authority is the bundled first-party CEF page plus the explicit live sidebar project identity. The URL is passed directly to CefSurface creation and is not stored in shell state, logged, derived from .git/folders, or backed by WKWebView/WebKit.
    */
    if !snapshot.feature_availability.kanban || snapshot.is_quick_projectless {
        return None;
    }
    let active_project_id = snapshot.active_project_id.as_ref()?.0.clone();
    let remote_reference = gpui_remote_project_reference_from_project_id(&active_project_id);
    let request_project_id = remote_reference
        .as_ref()
        .map(|reference| reference.project_id.clone())
        .unwrap_or(active_project_id);
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .to_string();
    let surface_id = snapshot.surface_ids.kanban_board_id.as_ref()?.clone();
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_KANBAN_URL", "kanban.html").ok()?;
    let mut params = vec![
        ("projectName", snapshot.display_name.clone()),
        ("projectPath", project_path),
        ("projectId", request_project_id),
        ("projectEditorId", surface_id),
        ("beadsDisplayKey", snapshot.display_name.clone()),
    ];
    if let Some(reference) = remote_reference {
        params.push(("remoteMachineId", reference.remote_machine_id));
    }
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
        base_url, &params,
    ))
}

#[derive(Clone)]
pub(crate) struct ProjectBoardBridgeRuntimeContext {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: String,
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) remote_target: Option<GpuiRemoteGxserverRequestTarget>,
}

pub(crate) fn project_board_bridge_runtime_context_from_snapshot(
    snapshot: Option<&GpuiProjectSnapshot>,
) -> Option<ProjectBoardBridgeRuntimeContext> {
    /*
    CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
    Kanban and Automate CEF bridge execution may carry only explicit active-project identity from the live sidebar snapshot. The bridge sends gxserver the current project id/path pair for scoped Beads and automation operations, never derives projects from .git/folders/URLs, never launches bd directly, and never logs or persists the private path.
    */
    let snapshot = snapshot?;
    if snapshot.is_quick_projectless
        || (!snapshot.feature_availability.kanban && !snapshot.feature_availability.automate)
    {
        return None;
    }
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .trim()
        .to_string();
    if project_path.is_empty() {
        return None;
    }
    let active_project_id = snapshot
        .active_project_id
        .as_ref()
        .map(|project_id| project_id.0.clone());
    let remote_reference = active_project_id
        .as_deref()
        .and_then(gpui_remote_project_reference_from_project_id);
    Some(ProjectBoardBridgeRuntimeContext {
        project_id: remote_reference
            .as_ref()
            .map(|reference| reference.project_id.clone())
            .or(active_project_id),
        project_path,
        remote_machine_id: remote_reference.map(|reference| reference.remote_machine_id),
        remote_target: None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiProjectBoardCommandIntent {
    InitializeBeads,
    InstallOrUpdateBeads,
    MigrateBeads,
    AdoptBeads,
    AdoptBeadsFastForward,
    ReconcileBeadsFork,
}

impl GpuiProjectBoardCommandIntent {
    pub(crate) fn command_id(self) -> &'static str {
        match self {
            Self::InitializeBeads => GPUI_PROJECT_BOARD_INITIALIZE_BEADS_COMMAND_ID,
            Self::InstallOrUpdateBeads => GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_COMMAND_ID,
            Self::MigrateBeads => GPUI_PROJECT_BOARD_MIGRATE_BEADS_COMMAND_ID,
            Self::AdoptBeads => GPUI_PROJECT_BOARD_ADOPT_BEADS_COMMAND_ID,
            Self::AdoptBeadsFastForward => GPUI_PROJECT_BOARD_ADOPT_BEADS_FAST_FORWARD_COMMAND_ID,
            Self::ReconcileBeadsFork => GPUI_PROJECT_BOARD_RECONCILE_BEADS_FORK_COMMAND_ID,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::InitializeBeads => "Initialize Beads",
            Self::InstallOrUpdateBeads => "Install or Update Beads",
            Self::MigrateBeads => "Migrate Beads",
            Self::AdoptBeads | Self::AdoptBeadsFastForward => "Adopt Beads Remote",
            Self::ReconcileBeadsFork => "Back Up and Adopt Beads Remote",
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::InitializeBeads => "bd init",
            Self::InstallOrUpdateBeads => {
                "curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash"
            }
            Self::MigrateBeads => "bd migrate --force && bd dolt push",
            Self::AdoptBeads | Self::AdoptBeadsFastForward => "bd bootstrap",
            Self::ReconcileBeadsFork => "bd export --all -o backup.jsonl && bd bootstrap",
        }
    }
}

pub(crate) fn gpui_project_board_action_for_command_id(command_id: &str) -> Option<&'static str> {
    match command_id {
        GPUI_PROJECT_BOARD_INITIALIZE_BEADS_COMMAND_ID => {
            Some(GPUI_PROJECT_BOARD_INITIALIZE_BEADS_ACTION)
        }
        GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_COMMAND_ID => {
            Some(GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_ACTION)
        }
        GPUI_PROJECT_BOARD_MIGRATE_BEADS_COMMAND_ID
        | GPUI_PROJECT_BOARD_ADOPT_BEADS_COMMAND_ID
        | GPUI_PROJECT_BOARD_ADOPT_BEADS_FAST_FORWARD_COMMAND_ID
        | GPUI_PROJECT_BOARD_RECONCILE_BEADS_FORK_COMMAND_ID => {
            Some(GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION)
        }
        _ => None,
    }
}

pub(crate) fn gpui_project_board_command_request(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> Result<GpuiProjectBoardCommandIntent, String> {
    let object = request
        .as_object()
        .ok_or_else(|| "The Beads command request is invalid.".to_string())?;
    if object
        .keys()
        .any(|key| !["action", "migrationOption", "projectId", "requestId"].contains(&key.as_str()))
    {
        return Err("The Beads command request is invalid.".to_string());
    }
    let _request_id = object
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "The Beads command request is invalid.".to_string())?;
    let action = object
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "The Beads command request is invalid.".to_string())?;
    let migration_option = object
        .get("migrationOption")
        .and_then(serde_json::Value::as_str);
    let intent = match (action, migration_option) {
        (GPUI_PROJECT_BOARD_INITIALIZE_BEADS_ACTION, None) => {
            GpuiProjectBoardCommandIntent::InitializeBeads
        }
        (GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_ACTION, None) => {
            GpuiProjectBoardCommandIntent::InstallOrUpdateBeads
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("migrate")) => {
            GpuiProjectBoardCommandIntent::MigrateBeads
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("adopt")) => {
            GpuiProjectBoardCommandIntent::AdoptBeads
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("adopt-fast-forward")) => {
            GpuiProjectBoardCommandIntent::AdoptBeadsFastForward
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("reconcile-fork")) => {
            GpuiProjectBoardCommandIntent::ReconcileBeadsFork
        }
        _ => return Err("The Beads command request is invalid.".to_string()),
    };
    let context = context.ok_or_else(|| "No active Kanban project is available.".to_string())?;
    if context.remote_machine_id.is_some() {
        return Err(
            "Run this Beads command from a terminal on the remote machine, then refresh the board."
                .to_string(),
        );
    }
    let request_project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if request_project_id.is_some_and(|project_id| {
        project_id.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            || project_id.chars().any(char::is_control)
            || context.project_id.as_deref() != Some(project_id)
    }) {
        return Err("The Beads command request is not for this active project.".to_string());
    }
    Ok(intent)
}

pub(crate) fn project_board_bridge_response_for_request_payload(
    payload: &str,
    snapshot: Option<&GpuiProjectSnapshot>,
) -> serde_json::Value {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    let action = manage_request_string(&request, "action").unwrap_or_default();
    if action == "getState" || action == "projectEditorFocusOwnerChanged" {
        let project_id = snapshot
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.clone());
        return serde_json::json!({
            "ok": true,
            "payload": {
                "agents": [],
                "links": [],
                "projectId": project_id,
                "sessions": [],
            },
            "requestId": request_id,
        });
    }
    serde_json::json!({
        "error": "Project board conversation action is not handled by this GPUI runtime surface.",
        "ok": false,
        "requestId": request_id,
    })
}

pub(crate) fn project_beads_gxserver_action_for_board_action(action: &str) -> Result<&'static str, String> {
    match action {
        "addComment" => Ok("comment"),
        "addLabel" => Ok("addLabel"),
        "configGet" => Ok("configGet"),
        "configGetIssuePrefix" => Ok("configGetIssuePrefix"),
        "configSet" => Ok("configSet"),
        "create" => Ok("create"),
        "delete" => Ok("delete"),
        "depAdd" => Ok("depAdd"),
        "depRemove" => Ok("depRemove"),
        "list" => Ok("list"),
        "listIssues" => Ok("board"),
        "listAllLabels" => Ok("listAllLabels"),
        "renamePrefix" => Ok("renamePrefix"),
        "removeLabel" => Ok("removeLabel"),
        "search" => Ok("search"),
        "setLabels" => Ok("setLabels"),
        "show" => Ok("show"),
        "updateDescription" => Ok("updateDescription"),
        "updateEstimate" => Ok("updateEstimate"),
        "updatePriority" => Ok("updatePriority"),
        "updateStatus" => Ok("updateStatus"),
        "updateTitle" => Ok("updateTitle"),
        _ => Err(format!("Unsupported Beads action: {action}")),
    }
}

pub(crate) fn project_beads_bridge_response_from_gxserver(
    status_code: u16,
    body: &str,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    if !(200..300).contains(&status_code) {
        let error = project_beads_gxserver_error_message(body, Some(status_code));
        return Ok(project_beads_bridge_error_response(request_id, &error));
    }
    let response = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid Beads response.".to_string())?;
    let result = response
        .get("result")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not return a Beads result.".to_string())?;
    Ok(project_beads_bridge_response_from_result(
        &serde_json::Value::Object(result.clone()),
        request_id,
    ))
}

pub(crate) fn project_beads_gxserver_error_message(body: &str, status_code: Option<u16>) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
        {
            return message.to_string();
        }
    }
    match status_code {
        Some(status_code) => format!("gxserver Beads request failed with HTTP {status_code}."),
        None => "gxserver Beads request failed.".to_string(),
    }
}

pub(crate) enum GpuiAutomationBoardNavigation {
    FocusSession(String),
    FocusProject(String),
    RevealWorktreePath(String),
}

pub(crate) const GPUI_QUICK_AUTOMATIONS_PROJECT_ID: &str = "quick-automations";
pub(crate) const GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE: &str = "Automations Overview";

pub(crate) fn gpui_automation_gxserver_endpoint_for_board_action(action: &str) -> Option<&'static str> {
    // macOS `gxserverAutomationEndpointForProjectBoardAction` parity.
    match action {
        "automationGetState" => Some("/api/readAutomationState"),
        "automationSave" => Some("/api/saveAutomation"),
        "automationDelete" => Some("/api/deleteAutomation"),
        "automationRunNow" => Some("/api/runAutomationNow"),
        "automationSetEnabled" => Some("/api/setAutomationEnabled"),
        "automationArchiveRun" => Some("/api/archiveAutomationRun"),
        "automationMarkRunRead" => Some("/api/markAutomationRunRead"),
        _ => None,
    }
}

pub(crate) fn gpui_automation_board_ok_response(
    request_id: &str,
    automation_state: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "payload": automation_state,
        "requestId": request_id,
    })
}

pub(crate) fn gpui_project_board_error_response(request_id: &str, error: &str) -> serde_json::Value {
    serde_json::json!({
        "error": error,
        "ok": false,
        "requestId": request_id,
    })
}

pub(crate) fn gpui_project_board_conversation_action_forwarded(action: &str) -> bool {
    // The board conversation surface owned by the sidebar runtime — the same
    // action set macOS `handleProjectBoardRequest` serves in native-sidebar.tsx
    // (minus the automation family, which Rust forwards to gxserver directly,
    // and `projectEditorFocusOwnerChanged`, which stays Rust-answered).
    matches!(
        action,
        "getState"
            | "startWork"
            | "associateFocusedSession"
            | "jumpToConversation"
            | "unlinkConversation"
            | "showToast"
            | "appendDebugLog"
    )
}

pub(crate) fn gpui_automation_rpc_automation_state(
    endpoint: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (status_code, body) =
        gxserver_post_typed_operation(endpoint, params, Duration::from_secs(60))?;
    if !(200..300).contains(&status_code) {
        return Err(gpui_automation_gxserver_error_message(&body, status_code));
    }
    parse_gpui_gxserver_rpc_result(&body)?
        .get("automationState")
        .cloned()
        .ok_or_else(|| "gxserver did not return an automation state.".to_string())
}

pub(crate) fn gpui_automation_gxserver_error_message(body: &str, status_code: u16) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
        {
            return message.to_string();
        }
    }
    format!("gxserver automation request failed with HTTP {status_code}.")
}

pub(crate) fn gpui_list_gxserver_domain_projects() -> Result<Vec<serde_json::Value>, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listProjects",
        &serde_json::json!({}),
        Duration::from_secs(30),
    )?;
    Ok(result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub(crate) fn gpui_find_gxserver_project_id_by_path(path: &str) -> Result<Option<String>, String> {
    let normalized = gpui_normalized_project_path_for_comparison(path);
    Ok(gpui_list_gxserver_domain_projects()?
        .iter()
        .find(|project| {
            project
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(gpui_normalized_project_path_for_comparison)
                .is_some_and(|candidate| candidate == normalized)
        })
        .and_then(|project| project.get("projectId").and_then(serde_json::Value::as_str))
        .map(str::to_string))
}

pub(crate) struct GpuiAutomationBoardScope {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: Option<String>,
}

pub(crate) fn gpui_automation_board_scope(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> GpuiAutomationBoardScope {
    // macOS `createGxserverAutomationParams` scope resolution: the request's
    // ids win (the overview page targets other projects), the active board
    // project is the fallback.
    let request_project_id = manage_request_string(request, "projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let request_project_path = manage_request_string(request, "projectPath")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    GpuiAutomationBoardScope {
        project_id: request_project_id
            .or_else(|| context.and_then(|context| context.project_id.clone())),
        project_path: request_project_path
            .or_else(|| context.map(|context| context.project_path.clone())),
    }
}

pub(crate) fn gpui_automation_scope_params(
    scope: &GpuiAutomationBoardScope,
) -> serde_json::Map<String, serde_json::Value> {
    let mut params = serde_json::Map::new();
    if let Some(project_id) = &scope.project_id {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.clone()),
        );
    }
    if let Some(project_path) = &scope.project_path {
        params.insert(
            "projectPath".to_string(),
            serde_json::Value::String(project_path.clone()),
        );
    }
    params
}

pub(crate) fn gpui_automation_payload_json(request: &serde_json::Value) -> Result<serde_json::Value, String> {
    let payload_json = manage_request_string(request, "payloadJson")
        .ok_or_else(|| "No automation payload was supplied.".to_string())?;
    serde_json::from_str::<serde_json::Value>(&payload_json)
        .map_err(|_| "Automation payload is not valid JSON.".to_string())
}

pub(crate) fn gpui_automation_enabled_from_payload(request: &serde_json::Value) -> Result<bool, String> {
    let payload = gpui_automation_payload_json(request)
        .map_err(|_| "No automation enabled payload was supplied.".to_string())?;
    payload
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Automation enabled payload is incomplete.".to_string())
}

pub(crate) fn gpui_automation_remove_worktree_from_payload(request: &serde_json::Value) -> bool {
    gpui_automation_payload_json(request)
        .ok()
        .and_then(|payload| {
            payload
                .get("removeWorktree")
                .and_then(serde_json::Value::as_bool)
        })
        == Some(true)
}

pub(crate) fn run_gpui_project_board_automation_request(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> (serde_json::Value, Option<GpuiAutomationBoardNavigation>) {
    let request_id = manage_request_string(request, "requestId").unwrap_or_default();
    match gpui_project_board_automation_result(request, context) {
        Ok((automation_state, navigation)) => (
            gpui_automation_board_ok_response(&request_id, automation_state),
            navigation,
        ),
        Err(error) => (gpui_project_board_error_response(&request_id, &error), None),
    }
}

pub(crate) fn gpui_project_board_automation_result(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> Result<(serde_json::Value, Option<GpuiAutomationBoardNavigation>), String> {
    let action = manage_request_string(request, "action").unwrap_or_default();
    let scope = gpui_automation_board_scope(request, context);
    if action == "automationGetAllState" {
        return Ok((gpui_automation_all_projects_state(&scope)?, None));
    }
    if action == "automationOpenRunSession" || action == "automationOpenWorktree" {
        return gpui_automation_open_run_target(&action, request, &scope);
    }
    let endpoint = gpui_automation_gxserver_endpoint_for_board_action(&action)
        .ok_or_else(|| format!("Unsupported automation action: {action}"))?;
    let mut params = gpui_automation_scope_params(&scope);
    match action.as_str() {
        "automationSave" => {
            params.insert(
                "definition".to_string(),
                gpui_automation_payload_json(request)?,
            );
        }
        "automationDelete" | "automationRunNow" | "automationSetEnabled" => {
            let automation_id = manage_request_string(request, "sessionId")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "No automation id was supplied.".to_string())?;
            params.insert(
                "automationId".to_string(),
                serde_json::Value::String(automation_id),
            );
        }
        "automationArchiveRun" | "automationMarkRunRead" => {
            let run_id = manage_request_string(request, "sessionId")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "No automation run id was supplied.".to_string())?;
            params.insert("runId".to_string(), serde_json::Value::String(run_id));
        }
        _ => {}
    }
    if action == "automationSetEnabled" {
        params.insert(
            "enabled".to_string(),
            serde_json::Value::Bool(gpui_automation_enabled_from_payload(request)?),
        );
    }
    if action == "automationArchiveRun" {
        params.insert(
            "removeWorktree".to_string(),
            serde_json::Value::Bool(gpui_automation_remove_worktree_from_payload(request)),
        );
    }
    let state = gpui_automation_rpc_automation_state(endpoint, &serde_json::Value::Object(params))?;
    Ok((state, None))
}

pub(crate) fn gpui_automation_all_projects_state(
    scope: &GpuiAutomationBoardScope,
) -> Result<serde_json::Value, String> {
    /*
    macOS `createAllProjectGxserverAutomationsBridgeState` parity for a client
    with no native project registry: aggregate per-project gxserver automation
    states across the daemon's registered target projects. Agents come from the
    per-project states (the daemon builds them from the same HUD source macOS
    seeds from), and per-project read failures skip that project like the
    macOS `Promise.allSettled` walk.
    */
    let target_projects: Vec<(String, String)> = gpui_list_gxserver_domain_projects()?
        .iter()
        .filter(|project| {
            let project_id = project
                .get("projectId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let path = project
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            project_id != GPUI_QUICK_AUTOMATIONS_PROJECT_ID
                && !path.trim().is_empty()
                && project
                    .get("isRecentProject")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                && project
                    .get("visibility")
                    .and_then(serde_json::Value::as_str)
                    != Some("hidden")
                && project
                    .get("systemKind")
                    .and_then(serde_json::Value::as_str)
                    != Some("remoteAttachCarrier")
        })
        .filter_map(|project| {
            let project_id = project
                .get("projectId")
                .and_then(serde_json::Value::as_str)?;
            let path = project.get("path").and_then(serde_json::Value::as_str)?;
            Some((project_id.to_string(), path.to_string()))
        })
        .collect();
    let states: Vec<serde_json::Value> = target_projects
        .iter()
        .filter_map(|(project_id, path)| {
            gpui_automation_rpc_automation_state(
                "/api/readAutomationState",
                &serde_json::json!({ "projectId": project_id, "projectPath": path }),
            )
            .ok()
        })
        .collect();
    let mut agents: Vec<serde_json::Value> = Vec::new();
    let mut seen_agent_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut automations: Vec<serde_json::Value> = Vec::new();
    let mut runs: Vec<serde_json::Value> = Vec::new();
    for state in &states {
        for agent in state
            .get("agents")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let agent_id = agent
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !agent_id.is_empty() && seen_agent_ids.insert(agent_id) {
                agents.push(agent.clone());
            }
        }
        automations.extend(
            state
                .get("automations")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        runs.extend(
            state
                .get("runs")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
    }
    let first_state = states.first();
    let projects = first_state
        .and_then(|state| state.get("projects").cloned())
        .unwrap_or_else(|| {
            serde_json::Value::Array(
                target_projects
                    .iter()
                    .map(|(project_id, path)| {
                        serde_json::json!({
                            "canUseWorktrees": false,
                            "label": path.rsplit('/').next().unwrap_or(project_id),
                            "path": path,
                            "projectId": project_id,
                            "worktreeUnavailableReason":
                                "Open the project Automate view to use worktree mode.",
                        })
                    })
                    .collect(),
            )
        });
    Ok(serde_json::json!({
        "agents": agents,
        "automations": automations,
        "defaultAgentId": first_state
            .and_then(|state| state.get("defaultAgentId").cloned())
            .unwrap_or(serde_json::Value::String("codex".to_string())),
        "projectCanUseWorktrees": false,
        "projectId": scope
            .project_id
            .clone()
            .unwrap_or_else(|| GPUI_QUICK_AUTOMATIONS_PROJECT_ID.to_string()),
        "projectName": GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
        "projectPath": "",
        "projects": projects,
        "runs": runs,
        "worktreeUnavailableReason": "Choose a project before using worktree mode.",
    }))
}

pub(crate) fn gpui_automation_open_run_target(
    action: &str,
    request: &serde_json::Value,
    scope: &GpuiAutomationBoardScope,
) -> Result<(serde_json::Value, Option<GpuiAutomationBoardNavigation>), String> {
    let run_id = manage_request_string(request, "sessionId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "No automation run id was supplied.".to_string())?;
    let state = gpui_automation_rpc_automation_state(
        "/api/readAutomationState",
        &serde_json::Value::Object(gpui_automation_scope_params(scope)),
    )?;
    let run = state
        .get("runs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|run| run.get("id").and_then(serde_json::Value::as_str) == Some(run_id.as_str()))
        .cloned()
        .ok_or_else(|| "Automation run not found.".to_string())?;
    let run_project_id = run
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| scope.project_id.clone())
        .ok_or_else(|| "Automation run has no project.".to_string())?;
    let worktree_path = run
        .get("worktree")
        .and_then(|worktree| worktree.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    let navigation = if action == "automationOpenWorktree" {
        let worktree_path =
            worktree_path.ok_or_else(|| "Automation run has no linked worktree.".to_string())?;
        match gpui_find_gxserver_project_id_by_path(&worktree_path)? {
            // macOS parity: focus the registered worktree project when it
            // exists, otherwise reveal the checkout in Finder.
            Some(project_id) => Some(GpuiAutomationBoardNavigation::FocusProject(project_id)),
            None => Some(GpuiAutomationBoardNavigation::RevealWorktreePath(
                worktree_path,
            )),
        }
    } else {
        let session_id = run
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "Automation run has no linked session.".to_string())?;
        // Worktree/thread runs live in another project than the automation;
        // resolve the session's owning project like gxserver's own
        // `find_run_session_project_id` (worktree path match, else run project).
        let session_project_id = match &worktree_path {
            Some(path) => {
                gpui_find_gxserver_project_id_by_path(path)?.unwrap_or(run_project_id.clone())
            }
            None => run_project_id.clone(),
        };
        gpui_combined_presentation_session_focus_id(&session_project_id, &session_id)
            .map(GpuiAutomationBoardNavigation::FocusSession)
    };
    let mut mark_read_params = gpui_automation_scope_params(scope);
    mark_read_params.insert("runId".to_string(), serde_json::Value::String(run_id));
    let refreshed = gpui_automation_rpc_automation_state(
        "/api/markAutomationRunRead",
        &serde_json::Value::Object(mark_read_params),
    )?;
    Ok((refreshed, navigation))
}

pub(crate) fn parse_gpui_gxserver_rpc_result(body: &str) -> Result<serde_json::Value, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid RPC response.".to_string())?;
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || value.get("product").and_then(serde_json::Value::as_str) != Some(GPUI_GXSERVER_PRODUCT)
        || value
            .get("protocolVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(GPUI_GXSERVER_PROTOCOL_VERSION)
    {
        return Err("gxserver returned an invalid RPC response.".to_string());
    }
    value
        .get("result")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .map(serde_json::Value::Object)
        .ok_or_else(|| "gxserver returned an invalid RPC result.".to_string())
}

pub(crate) fn gxserver_post_typed_operation(
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<(u16, String), String> {
    /*
    CDXC:GPUIProjectBoardGxserverBridge 2026-06-24-11:03:
    GPUI Kanban CEF parity must use the existing gxserver typed-operation boundary for Beads work. Send the same protocol-version envelope and bearer token as the native bridge to localhost only, with no bd subprocess execution, remote fallback, raw request logging, response logging, URL/title inspection, or persisted board payloads.

    CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-11:39:
    Settings fan-out also uses this narrow local gxserver RPC helper so agent-policy saves share the existing bearer-token path and response-body parser. Keep callers endpoint-specific and privacy-safe; this helper must not log tokens, URLs, settings payloads, project paths, command text, stdout/stderr, or daemon responses.

    CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-12:14:
    Startup/open-time agent-policy hydration also uses this helper for `/api/readAgentSettings` so GPUI does not invent a second daemon client or bypass the same token, localhost, timeout, protocol-header, and response-sanitization boundary.

    CDXC:GPUISettingsStatusBridge 2026-06-24-11:40:
    Settings status/action parity reuses this helper for hook status/install/uninstall and Portless state updates. Keep the helper transport-only: callers own endpoint allowlists, result validation, explicit UI error messages, and no persistent logging.
    */
    if !path.starts_with("/api/") {
        return Err("Invalid gxserver API path.".to_string());
    }
    let token = read_gpui_gxserver_auth_token()?;
    let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}");
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "gxserver is not reachable on 127.0.0.1:58744.".to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver write timeout.".to_string())?;

    let body = serde_json::json!({
        "protocolVersion": GPUI_GXSERVER_PROTOCOL_VERSION,
        "params": params,
    })
    .to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send gxserver request.".to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| "Could not read gxserver response.".to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "gxserver returned an invalid HTTP response.".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "gxserver returned an invalid HTTP status.".to_string())?;
    Ok((status_code, gxserver_http_response_body(headers, body)?))
}

pub(crate) fn gxserver_get_typed_operation(path: &str, timeout: Duration) -> Result<(u16, String), String> {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:40:
    GPUI Settings reads gxserver health through the same localhost/token/protocol boundary as typed POST operations. Health reads must stay GET-only, short-timeout, response-unlogged, and limited to `/api/` paths so Portless HUD hydration does not introduce a second daemon client.
    */
    if !path.starts_with("/api/") {
        return Err("Invalid gxserver API path.".to_string());
    }
    let token = read_gpui_gxserver_auth_token()?;
    let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}");
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "gxserver is not reachable on 127.0.0.1:58744.".to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver write timeout.".to_string())?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\n\r\n",
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send gxserver health request.".to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| "Could not read gxserver health response.".to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "gxserver returned an invalid HTTP response.".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "gxserver returned an invalid HTTP status.".to_string())?;
    Ok((status_code, gxserver_http_response_body(headers, body)?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiGxserverReadAgentSettingsResult {
    pub(crate) is_persisted: bool,
    pub(crate) settings: shared_settings::SharedGxserverAgentSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiGxserverAgentSettingsReconciliationAction {
    SeedLocal(shared_settings::SharedGxserverAgentSettings),
    ApplyCanonical(shared_settings::SharedGxserverAgentSettings),
    Noop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiGxserverAgentSettingsHydrationResult {
    pub(crate) expected_local_settings: shared_settings::SharedGxserverAgentSettings,
    pub(crate) canonical_settings: shared_settings::SharedGxserverAgentSettings,
}

pub(crate) fn gpui_gxserver_agent_settings_reconciliation_action(
    local_settings: &shared_settings::SharedGxserverAgentSettings,
    daemon_result: &GpuiGxserverReadAgentSettingsResult,
) -> GpuiGxserverAgentSettingsReconciliationAction {
    if !daemon_result.is_persisted {
        return GpuiGxserverAgentSettingsReconciliationAction::SeedLocal(local_settings.clone());
    }
    if daemon_result.settings != *local_settings {
        return GpuiGxserverAgentSettingsReconciliationAction::ApplyCanonical(
            daemon_result.settings.clone(),
        );
    }
    GpuiGxserverAgentSettingsReconciliationAction::Noop
}

pub(crate) fn reconcile_gpui_gxserver_agent_settings_with_daemon()
-> Result<Option<GpuiGxserverAgentSettingsHydrationResult>, String> {
    let daemon_agent_settings = read_gpui_gxserver_agent_settings()?;
    let local_agent_settings =
        shared_settings::shared_sidebar_settings_snapshot().gxserver_agent_settings();
    match gpui_gxserver_agent_settings_reconciliation_action(
        &local_agent_settings,
        &daemon_agent_settings,
    ) {
        GpuiGxserverAgentSettingsReconciliationAction::SeedLocal(settings) => {
            let expected_local_settings = settings.clone();
            update_gpui_gxserver_agent_settings(&settings).map(|canonical_settings| {
                Some(GpuiGxserverAgentSettingsHydrationResult {
                    expected_local_settings,
                    canonical_settings,
                })
            })
        }
        GpuiGxserverAgentSettingsReconciliationAction::ApplyCanonical(settings) => {
            Ok(Some(GpuiGxserverAgentSettingsHydrationResult {
                expected_local_settings: local_agent_settings,
                canonical_settings: settings,
            }))
        }
        GpuiGxserverAgentSettingsReconciliationAction::Noop => Ok(None),
    }
}

pub(crate) fn read_gpui_gxserver_agent_settings() -> Result<GpuiGxserverReadAgentSettingsResult, String> {
    /*
    CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-12:14:
    Startup/open-time reconciliation reads the daemon's canonical agent-settings row through the same typed localhost RPC envelope as save-time sync. Parse only `isPersisted`, `agentAcceptAllEnabled`, and `defaultPromptAgentId`; failures remain local `Result` values and never create fallback daemon state or leak raw gxserver responses.
    */
    let (status_code, body) = gxserver_post_typed_operation(
        "/api/readAgentSettings",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    if !(200..300).contains(&status_code) {
        return Err("gxserver agent settings read failed.".to_string());
    }
    parse_gpui_gxserver_read_agent_settings_response(&body)
}

pub(crate) fn update_gpui_gxserver_agent_settings(
    settings: &shared_settings::SharedGxserverAgentSettings,
) -> Result<shared_settings::SharedGxserverAgentSettings, String> {
    /*
    CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-11:39:
    GPUI Settings saves use the same local gxserver HTTP/token path as other GPUI gxserver calls. The request is the macOS-compatible `/api/updateAgentSettings` RPC envelope with only Accept All and Default Prompt Agent values; failures stay as local `Result` values so callers can silently preserve the render cache without private logs or fake daemon state.
    */
    let mut params = serde_json::Map::new();
    settings.write_to_settings_object(&mut params);
    let (status_code, body) = gxserver_post_typed_operation(
        "/api/updateAgentSettings",
        &serde_json::Value::Object(params),
        Duration::from_secs(5),
    )?;
    if !(200..300).contains(&status_code) {
        return Err("gxserver agent settings request failed.".to_string());
    }
    parse_gpui_gxserver_agent_settings_response(&body)
}

pub(crate) fn parse_gpui_gxserver_agent_settings_response(
    body: &str,
) -> Result<shared_settings::SharedGxserverAgentSettings, String> {
    let result = parse_gpui_gxserver_agent_settings_rpc_result(body)?;
    parse_gpui_gxserver_agent_settings_from_result(&result)
}

pub(crate) fn parse_gpui_gxserver_read_agent_settings_response(
    body: &str,
) -> Result<GpuiGxserverReadAgentSettingsResult, String> {
    let result = parse_gpui_gxserver_agent_settings_rpc_result(body)?;
    let is_persisted = result
        .get("isPersisted")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    let settings = parse_gpui_gxserver_agent_settings_from_result(&result)?;
    Ok(GpuiGxserverReadAgentSettingsResult {
        is_persisted,
        settings,
    })
}

pub(crate) fn parse_gpui_gxserver_agent_settings_rpc_result(
    body: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid agent settings response.".to_string())?;
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || value.get("product").and_then(serde_json::Value::as_str) != Some(GPUI_GXSERVER_PRODUCT)
        || value
            .get("protocolVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(GPUI_GXSERVER_PROTOCOL_VERSION)
    {
        return Err("gxserver returned an invalid agent settings response.".to_string());
    }
    value
        .get("result")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())
}

pub(crate) fn parse_gpui_gxserver_agent_settings_from_result(
    result: &serde_json::Map<String, serde_json::Value>,
) -> Result<shared_settings::SharedGxserverAgentSettings, String> {
    let settings = result
        .get("settings")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    let agent_accept_all_enabled = settings
        .get("agentAcceptAllEnabled")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    let default_prompt_agent_id = settings
        .get("defaultPromptAgentId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    Ok(shared_settings::SharedGxserverAgentSettings::new(
        agent_accept_all_enabled,
        default_prompt_agent_id,
    ))
}

pub(crate) fn gpui_sidebar_gxserver_bootstrap(
    latest_snapshot: Option<&GpuiProjectSnapshot>,
    focus_state: &GpuiGxserverPresentationFocusState,
    local_focus_key: Option<&GpuiLocalWorkspaceSessionKey>,
) -> Option<cef::SidebarGxserverBootstrap> {
    /*
    CDXC:GPUISidebarGxserverBootstrap 2026-06-24-11:17:
    Build the GPUI sidebar bootstrap only from real local gxserver facts: the selected loopback API port, the existing auth-token helper, protocol version 1, a stable GPUI sidebar client id, and the explicit active project id already stored from the live sidebar snapshot. Do not infer optional session ids from project paths, titles, shell terminal ids, Browser tabs, fixtures, or fallback state.

    CDXC:GPUISidebarGxserverBootstrap 2026-06-24-13:34 (revised 2026-08-07):
    `initialActiveProjectId` may be supplied only from the validated latest sidebar active-project snapshot, the exact local workspace-focus key whose raw session matches `focusedSessionId`, or the contract-validated persisted focus state's own `activeProjectId` (cold-start replay of the last active workspace project). This helper must not query gxserver project lists or log project identity.

    CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
    `focusedSessionId` and `visibleSessionIds` may be supplied only from the separate GPUI focus state that has already accepted real gxserver presentation ids. Local ids remain raw daemon session ids; remote ids remain machine-scoped sidebar ids so bootstrap replay is collision-safe.

    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
    A GPUI sidebar local-session click carries the authoritative project/session pair through the fixed workspace-focus bridge. If that project matches the stored focused session exactly, prefer it over the latest active-project snapshot so a second click cannot bootstrap the current session under a stale project and render an empty selection.
    */
    let initial_active_project_id = local_focus_key
        .filter(|key| focus_state.focused_session_id.as_deref() == Some(key.session_id.as_str()))
        .map(|key| key.project_id.as_str())
        .or_else(|| gpui_active_project_id_from_snapshot(latest_snapshot))
        .or(focus_state.active_project_id.as_deref())
        .map(str::to_string);
    Some(cef::SidebarGxserverBootstrap {
        base_url: format!("http://{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}"),
        auth_token: read_gpui_gxserver_auth_token().ok()?,
        protocol_version: GPUI_GXSERVER_PROTOCOL_VERSION as i32,
        client_id: GPUI_SIDEBAR_GXSERVER_CLIENT_ID.to_string(),
        initial_active_project_id,
        focused_session_id: focus_state.focused_session_id.clone(),
        visible_session_ids: focus_state.visible_session_ids.clone(),
    })
}

pub(crate) fn read_gpui_gxserver_auth_token() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return windows_terminal_backend::auth_token()
            .ok_or_else(|| "gxserver auth token is unavailable.".to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let token_path = shared_settings::ghostex_storage_paths()
            .gxserver_state_dir()
            .join("auth")
            .join("token");
        let token = fs::read_to_string(token_path)
            .map_err(|_| "gxserver auth token is unavailable.".to_string())?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("gxserver auth token is empty.".to_string());
        }
        Ok(token)
    }
}

pub(crate) fn gxserver_http_response_body(headers: &str, body: &str) -> Result<String, String> {
    if !headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    }) {
        return Ok(body.to_string());
    }
    gxserver_decode_chunked_http_body(body)
}

pub(crate) fn gxserver_decode_chunked_http_body(body: &str) -> Result<String, String> {
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut output = Vec::new();
    loop {
        let line_end = gxserver_find_crlf(bytes, index)
            .ok_or_else(|| "Invalid chunked gxserver response.".to_string())?;
        let size_text = std::str::from_utf8(&bytes[index..line_end])
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| "Invalid chunked gxserver response size.".to_string())?;
        index = line_end + 2;
        if size == 0 {
            return String::from_utf8(output)
                .map_err(|_| "Invalid UTF-8 in gxserver response.".to_string());
        }
        let chunk_end = index
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "Invalid chunked gxserver response.".to_string())?;
        output.extend_from_slice(&bytes[index..chunk_end]);
        index = chunk_end;
        if bytes.get(index..index + 2) != Some(b"\r\n") {
            return Err("Invalid chunked gxserver response.".to_string());
        }
        index += 2;
    }
}

pub(crate) fn gxserver_find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}

pub(crate) fn project_board_image_request_needs_clipboard(payload: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|request| manage_request_string(&request, "action"))
        .as_deref()
        == Some("pasteImage")
}

pub(crate) fn project_board_image_bridge_response_for_payload(
    payload: &str,
    clipboard_item: Option<ClipboardItem>,
) -> serde_json::Value {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    let action = manage_request_string(&request, "action").unwrap_or_default();
    let result = match action.as_str() {
        "pasteImage" => {
            project_board_clipboard_image_path(clipboard_item.as_ref()).map(|image_path| {
                let request_id = request_id.clone();
                serde_json::json!({
                    "dataUrl": null,
                    "error": null,
                    "imagePath": image_path,
                    "path": null,
                    "requestId": request_id,
                })
            })
        }
        "loadPreview" => {
            let path = manage_request_string(&request, "path").unwrap_or_default();
            project_board_image_preview_data_url(&path).map(|data_url| {
                let request_id = request_id.clone();
                serde_json::json!({
                    "dataUrl": data_url,
                    "error": null,
                    "imagePath": null,
                    "path": path,
                    "requestId": request_id,
                })
            })
        }
        _ => Err(format!("Unsupported Project Board image action: {action}")),
    };
    result.unwrap_or_else(|error| {
        serde_json::json!({
            "dataUrl": null,
            "error": error,
            "imagePath": null,
            "path": request.get("path").and_then(serde_json::Value::as_str),
            "requestId": request_id,
        })
    })
}

pub(crate) fn project_board_clipboard_image_path(
    clipboard_item: Option<&ClipboardItem>,
) -> Result<String, String> {
    /*
    CDXC:GPUIProjectBoardImagePaste 2026-06-24-11:03:
    Kanban image paste through CEF should preserve the native Project Board contract: return a durable path reference, not base64 Markdown. Existing image file references stay as paths; GPUI image clipboard bytes are saved under the resolved Ghostex image directory with their declared image format and returned as the same display path convention.
    */
    let item = clipboard_item
        .ok_or_else(|| "Clipboard does not contain an image path or image data.".to_string())?;
    for entry in &item.entries {
        if let ClipboardEntry::ExternalPaths(paths) = entry {
            for path in paths.paths() {
                if is_project_board_image_file_path(path) {
                    return Ok(project_board_display_image_path_for_existing_path(path));
                }
            }
        }
    }
    for entry in &item.entries {
        if let ClipboardEntry::String(clipboard_string) = entry {
            if let Some(path) = project_board_image_path_from_reference(clipboard_string.text()) {
                if is_project_board_image_file_path(&path) {
                    return Ok(project_board_display_image_path_for_existing_path(&path));
                }
            }
        }
    }
    for entry in &item.entries {
        if let ClipboardEntry::Image(image) = entry {
            let bytes = image.bytes();
            if bytes.is_empty() || bytes.len() > PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES {
                return Err("Clipboard image is too large to save.".to_string());
            }
            let path =
                unique_project_board_image_path(project_board_image_extension(image.format()))?;
            fs::write(&path, bytes).map_err(|_| "Could not save clipboard image.".to_string())?;
            return Ok(project_board_display_image_path_for_saved_path(&path));
        }
    }
    Err("Clipboard does not contain an image path or image data.".to_string())
}

pub(crate) fn project_board_image_preview_data_url(path: &str) -> Result<String, String> {
    let path = project_board_image_path_from_reference(path)
        .ok_or_else(|| "Image preview path does not point to a local image.".to_string())?;
    if !is_project_board_image_file_path(&path) {
        return Err("Image preview path does not point to a local image.".to_string());
    }
    let metadata = fs::metadata(&path)
        .map_err(|_| "Image preview path does not point to a local image.".to_string())?;
    if !metadata.is_file() || metadata.len() as usize > PROJECT_BOARD_IMAGE_PREVIEW_MAX_BYTES {
        return Err("Image preview data could not be decoded.".to_string());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime_type = project_board_image_mime_type_for_extension(&extension)
        .ok_or_else(|| "Image preview format is not supported by this CEF runtime.".to_string())?;
    let data =
        fs::read(&path).map_err(|_| "Image preview data could not be decoded.".to_string())?;
    Ok(format!(
        "data:{mime_type};base64,{}",
        project_board_base64_encode(&data)
    ))
}

pub(crate) fn project_board_image_path_from_reference(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    if trimmed.starts_with("file://") {
        let parsed = gpui::http_client::Url::parse(trimmed).ok()?;
        if parsed.scheme() == "file" {
            return parsed.to_file_path().ok();
        }
        return None;
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/.ghostex/") {
        return Some(
            shared_settings::ghostex_storage_paths()
                .data_dir
                .join(relative_path),
        );
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/") {
        return Some(home_dir().join(relative_path));
    }
    if trimmed.starts_with('/') {
        return Some(PathBuf::from(trimmed));
    }
    None
}

pub(crate) fn is_project_board_image_file_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    project_board_image_extension_is_allowed(&extension)
        && fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

pub(crate) fn project_board_image_extension_is_allowed(extension: &str) -> bool {
    matches!(
        extension,
        "avif"
            | "bmp"
            | "gif"
            | "heic"
            | "heif"
            | "ico"
            | "jpg"
            | "jpeg"
            | "png"
            | "svg"
            | "tif"
            | "tiff"
            | "webp"
    )
}

pub(crate) fn project_board_image_mime_type_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "avif" => Some("image/avif"),
        "bmp" => Some("image/bmp"),
        "gif" => Some("image/gif"),
        "ico" => Some("image/x-icon"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub(crate) fn project_board_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tif",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

pub(crate) fn unique_project_board_image_path(extension: &str) -> Result<PathBuf, String> {
    let directory = shared_settings::ghostex_storage_paths().images_dir();
    fs::create_dir_all(&directory).map_err(|_| "Could not create image directory.".to_string())?;
    let base_name = system_time_epoch_millis_string(std::time::SystemTime::now());
    let first = directory.join(format!("{base_name}.{extension}"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 2..100 {
        let candidate = directory.join(format!("{base_name}-{index}.{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(directory.join(format!(
        "{}-{}.{}",
        base_name,
        std::process::id(),
        extension
    )))
}

pub(crate) fn project_board_display_image_path_for_saved_path(path: &Path) -> String {
    project_board_display_image_path_for_existing_path(path)
}

pub(crate) fn project_board_display_image_path_for_existing_path(path: &Path) -> String {
    let user_home = home_dir();
    if let Ok(relative_path) = path.strip_prefix(&user_home) {
        return format!("~/{}", relative_path.to_string_lossy());
    }
    path.to_string_lossy().to_string()
}

pub(crate) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn project_board_base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

pub(crate) fn gpui_project_board_conversation_request_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onProjectBoardConversationRequest==='function'){{bridge.onProjectBoardConversationRequest(payload);}}else{{const pending=Array.isArray(bridge.pendingProjectBoardConversationRequests)?bridge.pendingProjectBoardConversationRequests:[];pending.push(payload);bridge.pendingProjectBoardConversationRequests=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_worktree_modal_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorktreeModalCommand==='function'){{bridge.onWorktreeModalCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingWorktreeModalCommands)?bridge.pendingWorktreeModalCommands:[];pending.push(payload);bridge.pendingWorktreeModalCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_git_commit_modal_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onGitCommitModalCommand==='function'){{bridge.onGitCommitModalCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingGitCommitModalCommands)?bridge.pendingGitCommitModalCommands:[];pending.push(payload);bridge.pendingGitCommitModalCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_export_transcript_modal_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onExportTranscriptModalCommand==='function'){{bridge.onExportTranscriptModalCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingExportTranscriptModalCommands)?bridge.pendingExportTranscriptModalCommands:[];pending.push(payload);bridge.pendingExportTranscriptModalCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_gxserver_git_action_result(
    project_id: &str,
    action: &str,
) -> Result<serde_json::Value, String> {
    gpui_gxserver_rpc_result(
        "/api/runGitAction",
        &serde_json::json!({
            "action": action,
            "projectId": project_id,
        }),
        Duration::from_secs(10),
    )
}

pub(crate) fn gpui_typed_operation_exit_code(result: &serde_json::Value) -> Option<i64> {
    result.get("exitCode").and_then(serde_json::Value::as_i64)
}

pub(crate) fn gpui_typed_operation_stdout(result: &serde_json::Value) -> &str {
    result
        .get("stdout")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(crate) fn gpui_collect_git_numstat_paths(stdout: &str, files: &mut HashSet<String>) {
    for line in stdout.trim().lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split_whitespace();
        let _additions = parts.next();
        let _deletions = parts.next();
        let path = parts.collect::<Vec<_>>().join(" ");
        if let Some(path) = gpui_normalized_relative_git_file_path(&path) {
            files.insert(path);
        }
    }
}

pub(crate) fn gpui_collect_git_status_porcelain_paths(stdout: &str, files: &mut HashSet<String>) {
    /*
    CDXC:GPUISidebarGit 2026-06-24-18:19:
    Git porcelain rename parsing must stay compatible with Rust toolchains where `split(&str)` is forward-only. Select the rename destination path without requiring DoubleEndedIterator, then pass it through the shared relative-path sanitizer before the IDE-open allowlist uses it.
    */
    for line in stdout.lines().filter(|line| line.chars().count() >= 4) {
        let raw_path = line.chars().skip(3).collect::<String>();
        let candidate = raw_path.trim().split(" -> ").last().unwrap_or("").trim();
        if let Some(path) = gpui_normalized_relative_git_file_path(candidate) {
            files.insert(path);
        }
    }
}

pub(crate) fn gpui_collect_git_zero_delimited_paths(stdout: &str, files: &mut HashSet<String>) {
    for path in stdout.split('\0') {
        if let Some(path) = gpui_normalized_relative_git_file_path(path) {
            files.insert(path);
        }
    }
}

pub(crate) fn gpui_normalized_relative_git_file_path(value: &str) -> Option<String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains('\0')
        || normalized.chars().count() > GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS
    {
        return None;
    }
    let segments = normalized.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return None;
    }
    Some(segments.join("/"))
}

pub(crate) fn gpui_gxserver_recent_project_path_by_id(project_id: &str) -> Result<PathBuf, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listRecentProjects",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let recent_projects = result
        .get("recentProjects")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "GPUI could not resolve that recent project.".to_string())?;
    gpui_gxserver_recent_project_path_from_rows_by_id(recent_projects, project_id)
        .ok_or_else(|| "GPUI could not resolve that recent project.".to_string())
}

pub(crate) fn gpui_gxserver_recent_project_path_from_rows_by_id(
    recent_projects: &[serde_json::Value],
    project_id: &str,
) -> Option<PathBuf> {
    recent_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find_map(|project| {
            (json_string_field(project, "projectId") == Some(project_id))
                .then(|| gpui_project_path_from_gxserver_row(project))
                .flatten()
        })
}

pub(crate) fn gpui_gxserver_workspace_project_path_by_id(project_id: &str) -> Result<PathBuf, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listProjects",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let projects = result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "GPUI could not resolve that workspace project.".to_string())?;
    gpui_gxserver_workspace_project_path_from_rows_by_id(projects, project_id)
        .ok_or_else(|| "GPUI could not resolve that workspace project.".to_string())
}

pub(crate) fn gpui_gxserver_workspace_project_path_from_rows_by_id(
    projects: &[serde_json::Value],
    project_id: &str,
) -> Option<PathBuf> {
    /*
    CDXC:GPUIRecentProjects 2026-06-25-19:54:
    Normal workspace project actions must not resolve explicit parked Recent Project rows even if the Rust action entry point is called directly. Skip only boolean `isRecentProject: true` rows here; recent actions continue to resolve trusted recent ids through `/api/listRecentProjects`, and false or non-boolean flags remain normal workspace rows.
    */
    projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find_map(|project| {
            if gpui_gxserver_project_row_is_explicit_recent_project(project) {
                return None;
            }
            (json_string_field(project, "projectId") == Some(project_id))
                .then(|| gpui_project_path_from_gxserver_row(project))
                .flatten()
        })
}

pub(crate) fn gpui_gxserver_project_row_is_explicit_recent_project(
    project: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    json_bool_field(project, "isRecentProject") == Some(true)
}

pub(crate) fn gpui_project_path_from_gxserver_row(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<PathBuf> {
    let path = json_string_field(project, "path")?.trim();
    if path.is_empty() || path.chars().count() > GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS {
        return None;
    }
    let path = PathBuf::from(path);
    path.is_absolute().then_some(path)
}

pub(crate) fn gpui_gxserver_presentation_focus_state_from_sidebar_contract_json(
    text: &str,
) -> Result<GpuiGxserverPresentationFocusState, GpuiGxserverPresentationFocusStateContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_gxserver_presentation_focus_state_from_sidebar_contract_value(&value)
}

pub(crate) fn gpui_gxserver_presentation_focus_state_from_sidebar_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiGxserverPresentationFocusState, GpuiGxserverPresentationFocusStateContractError> {
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "activeProjectId",
            "tabSessions",
            "focusedSessionId",
            "visibleSessionIds",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let focused_session_id = optional_gxserver_focus_session_id_field(object, "focusedSessionId")?;
    let visible_session_ids = required_gxserver_visible_session_ids_field(object)?;
    let active_project_id = optional_gxserver_focus_project_id_field(object, "activeProjectId")?;
    let active_project_tab_sessions = optional_gxserver_workspace_tab_sessions_field(object)?;
    Ok(GpuiGxserverPresentationFocusState {
        active_project_id,
        active_project_tab_sessions,
        focused_session_id,
        visible_session_ids,
    })
}

pub(crate) fn gpui_gxserver_focus_contract_object(
    value: &serde_json::Value,
) -> Result<
    &serde_json::Map<String, serde_json::Value>,
    GpuiGxserverPresentationFocusStateContractError,
> {
    value
        .as_object()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::ExpectedObject)
}

pub(crate) fn reject_unexpected_gxserver_focus_contract_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_keys: &[&str],
) -> Result<(), GpuiGxserverPresentationFocusStateContractError> {
    if object
        .keys()
        .any(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedKey);
    }
    Ok(())
}

pub(crate) fn optional_gxserver_focus_session_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, GpuiGxserverPresentationFocusStateContractError> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => gxserver_focus_session_id_string(value).map(Some),
    }
}

pub(crate) fn optional_gxserver_focus_project_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, GpuiGxserverPresentationFocusStateContractError> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => {
            let value = value
                .as_str()
                .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
                .trim();
            if !gpui_workspace_project_key_allowed(value) {
                return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
            }
            Ok(Some(value.to_string()))
        }
    }
}

pub(crate) fn optional_gxserver_workspace_tab_sessions_field(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<
    Option<Vec<GpuiSidebarWorkspaceTabSession>>,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let Some(value) = object.get("tabSessions") else {
        return Ok(None);
    };
    let array = value
        .as_array()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    if array.len() > GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    let mut seen = HashSet::new();
    let mut sessions = Vec::with_capacity(array.len());
    for value in array {
        let session = gxserver_workspace_tab_session_from_value(value)?;
        if seen.insert(session.key.clone()) {
            sessions.push(session);
        }
    }
    Ok(Some(sessions))
}

pub(crate) fn gxserver_workspace_tab_session_from_value(
    value: &serde_json::Value,
) -> Result<GpuiSidebarWorkspaceTabSession, GpuiGxserverPresentationFocusStateContractError> {
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "activity",
            "agentIcon",
            "agentSessionId",
            "isGeneratingFirstPromptTitle",
            "isSleeping",
            "kind",
            "lifecycleState",
            "projectId",
            // CDXC:SessionChatPromptQueue 2026-08-21: gxserver publishes a
            // session's queued-prompt count on the presentation snapshot the
            // sidebar runtime already reads. Accepted here (unused for now, the
            // pane chip reads the count itself) so that the day the runtime
            // forwards it, one added key cannot invalidate the whole
            // focus-state message and blank the Agents tab strip.
            "queuedPromptCount",
            "sessionId",
            "title",
        ],
    )?;
    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    let title = gxserver_workspace_tab_session_title_field(object, "title")?;
    let kind = json_string_field(object, "kind")
        .and_then(AgentsWorkspaceSessionKind::from_sidebar_kind)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let activity = match json_string_field(object, "activity") {
        Some("working") => AgentTerminalActivity::Working,
        Some("attention") => AgentTerminalActivity::Attention,
        Some("idle") | None => AgentTerminalActivity::Idle,
        Some(_) => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };
    let is_sleeping = json_bool_field(object, "isSleeping").unwrap_or(false);
    let is_generating_first_prompt_title =
        json_bool_field(object, "isGeneratingFirstPromptTitle").unwrap_or(false);
    let agent_session_id = match object.get("agentSessionId") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let value = value
                .as_str()
                .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
                .trim();
            if value.is_empty() || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
                return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
            }
            Some(value.to_string())
        }
    };
    let presentation_state = if is_sleeping {
        TerminalSessionPresentationState::Sleeping
    } else {
        match json_string_field(object, "lifecycleState") {
            Some("running") | None => TerminalSessionPresentationState::Running,
            Some("sleeping") => TerminalSessionPresentationState::Sleeping,
            Some("error") => TerminalSessionPresentationState::StartupFailed,
            Some("done") => TerminalSessionPresentationState::RestoredUnmounted,
            Some(_) => {
                return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
            }
        }
    };
    let key = if let Some(remote_project) =
        gpui_remote_project_reference_from_project_id(project_id.as_str())
    {
        GpuiWorkspaceTerminalSessionKey::Remote(GpuiRemoteAttachSessionKey {
            remote_machine_id: remote_project.remote_machine_id,
            project_id: remote_project.project_id,
            session_id,
        })
    } else {
        GpuiWorkspaceTerminalSessionKey::Local(GpuiLocalWorkspaceSessionKey {
            project_id,
            session_id,
        })
    };
    Ok(GpuiSidebarWorkspaceTabSession {
        activity,
        agent_icon: gpui_sidebar_agent_icon(json_string_field(object, "agentIcon")),
        agent_session_id,
        key,
        kind,
        is_generating_first_prompt_title,
        presentation_state,
        title,
    })
}

pub(crate) fn required_gxserver_visible_session_ids_field(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<String>, GpuiGxserverPresentationFocusStateContractError> {
    let value = object
        .get("visibleSessionIds")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?;
    let array = value
        .as_array()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    if array.len() > GPUI_SIDEBAR_VISIBLE_SESSION_IDS_MAX {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    let mut seen = HashSet::new();
    let mut session_ids = Vec::with_capacity(array.len());
    for value in array {
        let session_id = gxserver_focus_session_id_string(value)?;
        if seen.insert(session_id.clone()) {
            session_ids.push(session_id);
        }
    }
    Ok(session_ids)
}

pub(crate) fn gxserver_workspace_focus_project_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = object
        .get(key)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .trim();
    if !gpui_workspace_project_key_allowed(value) {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn gxserver_workspace_focus_session_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = object
        .get(key)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .trim();
    if !gpui_sidebar_local_gxserver_session_id_allowed(value) {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn gxserver_workspace_terminal_rename_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = object
        .get(key)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    if value.trim() != value
        || value.is_empty()
        || value.chars().count() > GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_TITLE_MAX_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn gxserver_workspace_tab_session_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = object
        .get(key)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .trim();
    if value.is_empty()
        || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || value.contains('\0')
        || value.chars().any(char::is_control)
    {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn gxserver_focus_session_id_string(
    value: &serde_json::Value,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = value
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .trim();
    if !gpui_sidebar_gxserver_presentation_session_id_allowed(value) {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn gpui_sidebar_gxserver_presentation_session_id_allowed(value: &str) -> bool {
    if gpui_remote_attach_session_reference_from_project_id(value).is_some() {
        return true;
    }
    !value.is_empty()
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_sidebar_local_gxserver_session_id_allowed(value: &str) -> bool {
    gpui_remote_attach_session_reference_from_project_id(value).is_none()
        && gpui_sidebar_gxserver_presentation_session_id_allowed(value)
}

pub(crate) fn gpui_gxserver_presentation_focus_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-gxserver-presentation-focus-state.json")
}

/// Persists the sidebar-owned presentation focus state (focused + visible
/// gxserver session ids) so relaunch bootstraps can replay it and eagerly
/// re-materialize the previously focused session (Decision #3). The file
/// carries only the fixed focus-state contract shape — no titles, paths,
/// commands, or terminal content.
pub(crate) fn persist_gpui_gxserver_presentation_focus_state(state: &GpuiGxserverPresentationFocusState) {
    let path = gpui_gxserver_presentation_focus_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    /*
    CDXC:GPUIWorkspaceSessionReattach 2026-08-07:
    `activeProjectId` persists alongside the focus ids so a cold start can
    replay the last active workspace project (local or machine-scoped remote)
    through the sidebar bootstrap instead of re-deriving it from the focused
    session alone — a derivation that fails whenever the focused session was
    remote or the focused id is stale. Tab sessions stay unpersisted: the
    sidebar's first hydrate is their only authority.
    */
    let payload = serde_json::json!({
        "version": GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION,
        "type": GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE,
        "activeProjectId": state.active_project_id,
        "focusedSessionId": state.focused_session_id,
        "visibleSessionIds": state.visible_session_ids,
    });
    let _ = fs::write(path, payload.to_string());
}

pub(crate) fn load_gpui_gxserver_presentation_focus_state() -> GpuiGxserverPresentationFocusState {
    fs::read_to_string(gpui_gxserver_presentation_focus_state_path())
        .ok()
        .and_then(|text| {
            gpui_gxserver_presentation_focus_state_from_sidebar_contract_json(&text).ok()
        })
        .unwrap_or_default()
}

// Revision markers mirror shared/first-launch-setup-settings.ts
// (FIRST_LAUNCH_SETUP_CURRENT_REVISION / HIGHLIGHTED_FEATURES_CURRENT_REVISION);
// keep them in sync when the shared revisions bump so both apps replay the
// refreshed onboarding exactly once.
pub(crate) const GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION: &str = "2026-06-18-short-first-launch";
pub(crate) const GPUI_HIGHLIGHTED_FEATURES_SEEN_REVISION: &str = "2026-06-16-highlighted-features-launch";

