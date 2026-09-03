// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the add-project dialog RPC result
// helpers, status indicator project/session state conversion, menu bar and
// command palette bridge scripts, and project git changed-file lookup. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{collections::HashSet, time::Duration};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_add_project_dialog_rpc_result(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let (status_code, body) = match target {
        Some(target) => gpui_remote_gxserver_post_typed_operation(target, path, params, timeout)
            .map_err(|_| "The remote machine did not answer.".to_string())?,
        None => gxserver_post_typed_operation(path, params, timeout)
            .map_err(|_| "gxserver is not reachable.".to_string())?,
    };
    if !(200..300).contains(&status_code) {
        return Err(gpui_add_project_dialog_error_message(&body));
    }
    parse_gpui_gxserver_rpc_result(&body)
        .map_err(|_| "gxserver returned an unexpected response.".to_string())
}

pub(crate) fn gpui_add_project_dialog_restore_recent_project(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    add_result: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:AddProject 2026-08-12:
    `/api/addProjectPath` is intentionally idempotent and returns an existing
    path registration unchanged. When that registration is parked in Recent
    Projects, Add Project must perform the same authoritative restore mutation
    as clicking its Recent Projects row before reporting success. Otherwise the
    dialog closes around a still-hidden `isRecentProject: true` project and the
    user sees a silent no-op.
    */
    let Some(project) = add_result
        .get("project")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(add_result);
    };
    if !gpui_gxserver_project_row_is_explicit_recent_project(project) {
        return Ok(add_result);
    }
    let project_id = gpui_trimmed_json_string_field(project, "projectId")
        .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
        .ok_or_else(|| "gxserver returned an unexpected response.".to_string())?;
    gpui_add_project_dialog_rpc_result(
        target,
        "/api/restoreRecentProject",
        &serde_json::json!({ "projectId": project_id }),
        timeout,
    )
}

pub(crate) fn gpui_add_project_dialog_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(|message| {
            message
                .chars()
                .filter(|character| !character.is_control())
                .take(GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
                .collect::<String>()
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "gxserver rejected the request.".to_string())
}

pub(crate) fn gpui_repository_clone_rpc_result(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    match target {
        Some(target) => gpui_remote_gxserver_rpc_result(target, path, params, timeout),
        None => gpui_gxserver_rpc_result(path, params, timeout),
    }
}

pub(crate) fn gpui_command_pane_side_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> GpuiCommandPaneSide {
    match settings.command_pane_side() {
        shared_settings::SharedCommandPaneSide::Bottom => GpuiCommandPaneSide::Bottom,
        shared_settings::SharedCommandPaneSide::Right => GpuiCommandPaneSide::Right,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiStatusIndicatorProjectState {
    pub(crate) icon_data_url: Option<String>,
    pub(crate) project_id: String,
    pub(crate) sessions: Vec<GpuiStatusIndicatorSessionState>,
    pub(crate) title: String,
}

/*
CDXC:AgentLauncher 2026-08-01-16:00:
What the tab strip needs to draw one Global Action button and to ask the sidebar
runtime to run it: a bounded id, a display name for the tooltip, and an optional
icon slug. Deliberately no command text, URL, cwd, or run state — the click
sends the id back through the existing Action selector bridge, which resolves the
trusted definition on the sidebar side, so a compromised renderer payload cannot
put an executable string in front of the user.
*/
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct GpuiMenuBarStatusNativeProjectEntry {
    pub(crate) project_id: *const std::ffi::c_char,
    pub(crate) title: *const std::ffi::c_char,
    pub(crate) sessions: *const GpuiMenuBarStatusNativeSessionEntry,
    pub(crate) session_count: usize,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) struct GpuiMenuBarStatusNativeSessionOwner {
    pub(crate) session_id: std::ffi::CString,
    pub(crate) title: std::ffi::CString,
    pub(crate) last_active_at: Option<std::ffi::CString>,
    pub(crate) entry: GpuiMenuBarStatusNativeSessionEntry,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) struct GpuiMenuBarStatusNativeProjectOwner {
    pub(crate) project_id: std::ffi::CString,
    pub(crate) title: std::ffi::CString,
    pub(crate) sessions: Vec<GpuiMenuBarStatusNativeSessionOwner>,
    pub(crate) entries: Vec<GpuiMenuBarStatusNativeSessionEntry>,
}

pub(crate) fn gpui_status_indicator_project_from_value(
    value: &serde_json::Value,
) -> Result<GpuiStatusIndicatorProjectState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["projectId", "title", "sessions", "iconDataUrl"])?;
    let sessions = object
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .filter(|sessions| sessions.len() <= GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT)
        .ok_or(())?
        .iter()
        .map(gpui_status_indicator_session_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GpuiStatusIndicatorProjectState {
        icon_data_url: gpui_status_optional_icon_data_url_field(object, "iconDataUrl")?,
        project_id: gpui_status_id_field(object, "projectId")?,
        sessions,
        title: gpui_status_title_field(object, "title")?,
    })
}

pub(crate) fn gpui_status_indicator_session_from_value(
    value: &serde_json::Value,
) -> Result<GpuiStatusIndicatorSessionState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(
        object,
        &[
            "sessionId",
            "status",
            "title",
            "sidebarOrder",
            "lastActiveAt",
        ],
    )?;
    Ok(GpuiStatusIndicatorSessionState {
        last_active_at: gpui_status_optional_title_field(object, "lastActiveAt")?,
        order: object
            .get("sidebarOrder")
            .and_then(serde_json::Value::as_u64)
            .ok_or(())?,
        session_id: gpui_status_id_field(object, "sessionId")?,
        status: gpui_status_field(object, "status")?,
        title: gpui_status_title_field(object, "title")?,
    })
}

pub(crate) fn gpui_menu_bar_project_activation_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onMenuBarProjectActivation==='function'){{bridge.onMenuBarProjectActivation(payload);}}else{{const pending=Array.isArray(bridge.pendingMenuBarProjectActivations)?bridge.pendingMenuBarProjectActivations:[];pending.push(payload);bridge.pendingMenuBarProjectActivations=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_menu_bar_session_activation_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onMenuBarSessionActivation==='function'){{bridge.onMenuBarSessionActivation(payload);}}else{{const pending=Array.isArray(bridge.pendingMenuBarSessionActivations)?bridge.pendingMenuBarSessionActivations:[];pending.push(payload);bridge.pendingMenuBarSessionActivations=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_command_palette_session_focus_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onCommandPaletteSessionFocus==='function'){{bridge.onCommandPaletteSessionFocus(payload);}}else{{const pending=Array.isArray(bridge.pendingCommandPaletteSessionFocusRequests)?bridge.pendingCommandPaletteSessionFocusRequests:[];pending.push(payload);bridge.pendingCommandPaletteSessionFocusRequests=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_workspace_tab_session_selected_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTabSessionSelected==='function'){{bridge.onWorkspaceTabSessionSelected(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTabSessionSelections)?bridge.pendingWorkspaceTabSessionSelections:[];pending.push(payload);bridge.pendingWorkspaceTabSessionSelections=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_workspace_terminal_rename_command_input(
    command: GpuiWorkspaceTerminalRenameCommandKind,
    title: &str,
) -> String {
    /*
    CDXC:SessionTitles 2026-06-27-02:27:
    This is the only rename path that turns the validated title into terminal input. It must remain a fixed `/rename <title>`, Pi `/name <title>`, or Hermes Agent `/title <title>` command chosen by the validated enum selector for the already-resolved Agents surface and must not add shell escaping, logging, persistence, fallback commands, or renderer-selected text.
    */
    match command {
        GpuiWorkspaceTerminalRenameCommandKind::Name => format!("/name {title}"),
        GpuiWorkspaceTerminalRenameCommandKind::Rename => format!("/rename {title}"),
        GpuiWorkspaceTerminalRenameCommandKind::Title => format!("/title {title}"),
    }
}

pub(crate) fn gpui_project_git_changed_file_paths(
    project_id: &str,
) -> Result<HashSet<String>, String> {
    let status = gpui_gxserver_git_action_result(project_id, "statusPorcelain")?;
    if gpui_typed_operation_exit_code(&status) != Some(0) {
        return Err("GPUI could not refresh changed files.".to_string());
    }
    let mut files = HashSet::new();
    gpui_collect_git_status_porcelain_paths(gpui_typed_operation_stdout(&status), &mut files);

    let diff = gpui_gxserver_git_action_result(project_id, "diffNumstat")?;
    if gpui_typed_operation_exit_code(&diff) == Some(0) {
        gpui_collect_git_numstat_paths(gpui_typed_operation_stdout(&diff), &mut files);
    }

    let untracked = gpui_gxserver_git_action_result(project_id, "listUntracked")?;
    if gpui_typed_operation_exit_code(&untracked) == Some(0) {
        gpui_collect_git_zero_delimited_paths(gpui_typed_operation_stdout(&untracked), &mut files);
    }
    Ok(files)
}
