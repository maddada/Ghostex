// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the gxserver presentation focus-state contract parsing,
// validation, and on-disk persistence helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{collections::HashSet, fs, path::PathBuf};

use crate::app::helpers::*;
use crate::*;

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
            "agentName",
            "agentSessionId",
            // CDXC:DraftAgentSwitch 2026-08-28: present-only draft marker; a
            // daemon/sidebar that predates drafts simply omits it.
            "isDraft",
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
            "hasSessionNote",
            "sessionId",
            "stashedPromptCount",
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
    let is_draft = json_bool_field(object, "isDraft").unwrap_or(false);
    let is_generating_first_prompt_title =
        json_bool_field(object, "isGeneratingFirstPromptTitle").unwrap_or(false);
    let agent_name = match object.get("agentName") {
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
    let has_session_note = match object.get("hasSessionNote") {
        None | Some(serde_json::Value::Null) => false,
        Some(value) => value
            .as_bool()
            .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?,
    };
    let stashed_prompt_count = match object.get("stashedPromptCount") {
        None | Some(serde_json::Value::Null) => 0,
        Some(value) => value
            .as_u64()
            .filter(|count| *count <= 200)
            .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?,
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
        agent_name,
        agent_session_id,
        key,
        kind,
        is_draft,
        is_generating_first_prompt_title,
        presentation_state,
        has_session_note,
        stashed_prompt_count,
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
pub(crate) fn persist_gpui_gxserver_presentation_focus_state(
    state: &GpuiGxserverPresentationFocusState,
) {
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

// Revision markers mirror packages/shared/first-launch-setup-settings.ts
// (FIRST_LAUNCH_SETUP_CURRENT_REVISION / HIGHLIGHTED_FEATURES_CURRENT_REVISION);
// keep them in sync when the shared revisions bump so both apps replay the
// refreshed onboarding exactly once.
/*
CDXC:FirstLaunchSetup 2026-08-24:
The onboarding flow was rebuilt (Welcome/Plugins/Agents/Connect/Skills/Get
started). Bumping the revision shows the new tour once to installs that saw an
older first-launch setup.
*/
pub(crate) const GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION: &str = "2026-08-24-onboarding-redesign";
pub(crate) const GPUI_HIGHLIGHTED_FEATURES_SEEN_REVISION: &str =
    "2026-06-16-highlighted-features-launch";
