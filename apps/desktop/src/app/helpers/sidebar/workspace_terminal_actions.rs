// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds project-snapshot and workspace-terminal focus/rename/enter JSON parsing helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn store_latest_gpui_project_snapshot_from_sidebar_contract_json(
    latest_snapshot: &mut Option<GpuiProjectSnapshot>,
    text: &str,
) -> Result<GpuiProjectSnapshotStoreResult, GpuiProjectSnapshotContractError> {
    /*
    CDXC:GPUIProjectSidebarBridge 2026-06-22-19:32:
    The live CEF sidebar bridge may update only the in-memory latest active-project snapshot after the strict contract parser succeeds. Malformed payloads leave the prior snapshot untouched, and the snapshot remains non-persistent.

    CDXC:GPUIProjectSidebarBridge 2026-06-22-19:44:
    Once stored, the latest valid sidebar snapshot becomes the App runtime availability source; the env bridge is only the fallback before any valid sidebar payload arrives. The store helper itself still does not log raw JSON or project details and does not coerce active mode without the App context.

    CDXC:GPUIProjectSidebarBridge 2026-06-23-06:53:
    The store helper returns an explicit change result so bridge callers can no-op duplicate valid payloads. Parse and validate exactly as before, preserve the previous snapshot on errors, and replace the in-memory snapshot only after the parsed snapshot differs; do not add project/path/name heuristics, fallbacks, persistence, or logging of raw contract data.
    */
    let snapshot = gpui_project_snapshot_from_sidebar_contract_json(text)?;
    if latest_snapshot
        .as_ref()
        .is_some_and(|latest_snapshot| latest_snapshot == &snapshot)
    {
        return Ok(GpuiProjectSnapshotStoreResult::Unchanged);
    }

    *latest_snapshot = Some(snapshot);
    Ok(GpuiProjectSnapshotStoreResult::Changed)
}

#[allow(dead_code)]
pub(crate) fn gpui_project_snapshot_from_sidebar_contract_json(
    text: &str,
) -> Result<GpuiProjectSnapshot, GpuiProjectSnapshotContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiProjectSnapshotContractError::MalformedJson)?;
    gpui_project_snapshot_from_sidebar_contract_value(&value)
}

#[allow(dead_code)]
pub(crate) fn gpui_project_snapshot_from_sidebar_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectSnapshot, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(object, &["version", "type", "activeProject"])?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiProjectSnapshotContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_PROJECT_CONTEXT_MESSAGE_VERSION {
        return Err(GpuiProjectSnapshotContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiProjectSnapshotContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_PROJECT_CONTEXT_MESSAGE_TYPE {
        return Err(GpuiProjectSnapshotContractError::UnexpectedMessageType);
    }

    let active_project = object
        .get("activeProject")
        .ok_or(GpuiProjectSnapshotContractError::MissingField)?;
    gpui_project_snapshot_from_contract_project_value(active_project)
}

pub(crate) fn gpui_sidebar_workspace_terminal_focus_from_json(
    text: &str,
) -> Result<GpuiSidebarWorkspaceTerminalFocusMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_focus_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_focus_from_value(
    value: &serde_json::Value,
) -> Result<GpuiSidebarWorkspaceTerminalFocusMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "forceRemount",
            "placementTargetSessionId",
            "preferredInterface",
            "projectId",
            "sessionId",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    let placement_target_session_id = object
        .get("placementTargetSessionId")
        .map(|_| gxserver_workspace_focus_session_id_field(object, "placementTargetSessionId"))
        .transpose()?;
    let force_remount = match object.get("forceRemount") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedJson)?,
    };
    let preferred_interface = match object.get("preferredInterface") {
        None => GpuiPreferredAgentInterface::Terminal,
        Some(value) => value
            .as_str()
            .and_then(GpuiPreferredAgentInterface::from_str)
            .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedJson)?,
    };
    Ok(GpuiSidebarWorkspaceTerminalFocusMessage {
        force_remount,
        placement_target_session_id,
        preferred_interface,
        project_id,
        session_id,
    })
}

pub(crate) fn gpui_sidebar_create_project_agent_from_json(
    text: &str,
) -> Result<GpuiSidebarCreateProjectAgentMessage, GpuiGxserverPresentationFocusStateContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "projectId",
            "agentId",
            "preferredInterface",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_CREATE_PROJECT_AGENT_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_CREATE_PROJECT_AGENT_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let preferred_interface = object
        .get("preferredInterface")
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiPreferredAgentInterface::from_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let agent_id = object
        .get("agentId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .to_string();
    Ok(GpuiSidebarCreateProjectAgentMessage {
        agent_id,
        preferred_interface,
        project_id,
    })
}

pub(crate) fn gpui_sidebar_create_project_terminal_from_json(
    text: &str,
) -> Result<GpuiSidebarCreateProjectTerminalMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(object, &["version", "type", "projectId"])?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_CREATE_PROJECT_TERMINAL_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_CREATE_PROJECT_TERMINAL_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    Ok(GpuiSidebarCreateProjectTerminalMessage { project_id })
}

pub(crate) fn gpui_sidebar_workspace_terminal_rename_command_from_json(
    text: &str,
) -> Result<
    GpuiSidebarWorkspaceTerminalRenameCommandMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_rename_command_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_rename_command_from_value(
    value: &serde_json::Value,
) -> Result<
    GpuiSidebarWorkspaceTerminalRenameCommandMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    /*
    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    The fixed renderer payload must contain only version/type, raw local gxserver project/session ids, one already-trimmed bounded title, and an optional literal command selector. Reject extra keys, remote or combined ids, missing ids, untrimmed/empty/oversized/control-character titles, paths, free-text command fields, stdout/stderr, terminal content, tokens, and raw renderer envelopes before any terminal surface is consulted.
    */
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "projectId",
            "sessionId",
            "title",
            "command",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    let title = gxserver_workspace_terminal_rename_title_field(object, "title")?;
    let command = match object.get("command") {
        None => GpuiWorkspaceTerminalRenameCommandKind::Rename,
        Some(serde_json::Value::String(command)) if command == "rename" => {
            GpuiWorkspaceTerminalRenameCommandKind::Rename
        }
        Some(serde_json::Value::String(command)) if command == "name" => {
            GpuiWorkspaceTerminalRenameCommandKind::Name
        }
        Some(serde_json::Value::String(command)) if command == "title" => {
            GpuiWorkspaceTerminalRenameCommandKind::Title
        }
        Some(_) => {
            return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
        }
    };
    Ok(GpuiSidebarWorkspaceTerminalRenameCommandMessage {
        command,
        project_id,
        session_id,
        title,
    })
}

pub(crate) fn gpui_sidebar_workspace_terminal_enter_from_json(
    text: &str,
) -> Result<GpuiSidebarWorkspaceTerminalEnterMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_enter_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_enter_from_value(
    value: &serde_json::Value,
) -> Result<GpuiSidebarWorkspaceTerminalEnterMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &["version", "type", "projectId", "sessionId"],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_ENTER_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_ENTER_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    Ok(GpuiSidebarWorkspaceTerminalEnterMessage {
        project_id,
        session_id,
    })
}

pub(crate) fn gpui_sidebar_session_completion_sound_from_json(
    text: &str,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(object, &["version", "type", "sound"])?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let sound = object
        .get("sound")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let trimmed = sound.trim();
    if trimmed.is_empty() || trimmed.len() > GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MAX_CHARS {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(trimmed.to_string())
}

