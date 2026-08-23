// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds sidebar open-browser-url/session-completion-sound JSON parsing, workspace-terminal lifecycle-result parsing, and runtime settings snapshot helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_sidebar_open_browser_url_from_json(
    text: &str,
) -> Result<GpuiSidebarOpenBrowserUrlMessage, GpuiGxserverPresentationFocusStateContractError> {
    /*
    The renderer-command browser open carries only one URL-or-search string and
    a fixed reuse selector. Rust re-normalizes the address through the same
    toolbar path as typed input, so renderer payloads cannot smuggle project
    ids, paths, commands, tokens, or raw renderer envelopes into Browser state.

    CDXC:GPUIRemoteBrowserTabs 2026-07-12:
    The one exception is the optional first-party sidebar `projectId`, which
    must be a known browser project key shape — a local `P…` workspace id or a
    machine-scoped `remote:<machine>:project:<id>` reference — so project
    headers can target their own browser tab model without racing the async
    active-project context round-trip. Any other string still rejects the
    whole payload.
    */
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &["version", "type", "url", "reuse", "origin", "projectId"],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let url = object
        .get("url")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .trim();
    if url.len() > GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS
        || url.chars().any(|character| character.is_control())
    {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }

    let reuse = match object
        .get("reuse")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
    {
        "exact" => GpuiBrowserRendererOpenReuse::Exact,
        "none" => GpuiBrowserRendererOpenReuse::None,
        "similar" => GpuiBrowserRendererOpenReuse::Similar,
        _ => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };

    let from_quick_header = match object.get("origin") {
        None => false,
        Some(serde_json::Value::String(origin)) if origin == "quickHeader" => true,
        Some(_) => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };

    let project_id = match object.get("projectId") {
        None => None,
        Some(serde_json::Value::String(project_id))
            if gpui_browser_tabs_project_key_allowed(project_id.trim()) =>
        {
            Some(project_id.trim().to_string())
        }
        Some(_) => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };

    Ok(GpuiSidebarOpenBrowserUrlMessage {
        url: url.to_string(),
        reuse,
        from_quick_header,
        project_id,
    })
}

pub(crate) fn gpui_sidebar_workspace_terminal_lifecycle_result_from_json(
    text: &str,
) -> Result<
    GpuiSidebarWorkspaceTerminalLifecycleResultMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_lifecycle_result_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_lifecycle_result_from_value(
    value: &serde_json::Value,
) -> Result<
    GpuiSidebarWorkspaceTerminalLifecycleResultMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &["version", "type", "requestId", "ok"],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let request_id = object
        .get("requestId")
        .and_then(serde_json::Value::as_u64)
        .filter(|request_id| {
            (1..=GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_ID_MAX).contains(request_id)
        })
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let ok = object
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    Ok(GpuiSidebarWorkspaceTerminalLifecycleResultMessage { ok, request_id })
}

pub(crate) fn sidebar_runtime_settings_snapshot_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> cef::SidebarRuntimeSettingsSnapshot {
    /*
    CDXC:GPUIProjectSidebarBridge 2026-06-23-06:36:
    The sidebar CEF runtime settings handoff must use the same shared sidebar settings file and strict boolean interpretation as SidebarApp. These booleans seed TS-side payload and workarea behavior only; Docs titlebar visibility stays governed by project context, not debuggingMode/showBetaFeatures.

    CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
    The GPUI sidebar runtime snapshot now also carries the saved shared Settings object as serialized first-party payload so the mounted SidebarApp can normalize real user preferences immediately on initial CEF install and after Settings saves. This is not a generic settings bus and must not write logs, persist another copy, or expose settings to Browser/workarea/modal CEF clients.
    */
    cef::SidebarRuntimeSettingsSnapshot {
        debugging_mode: settings.debugging_mode(),
        show_beta_features: settings.show_beta_features(),
        saved_settings_json: sidebar_runtime_saved_settings_json(settings),
    }
}

pub(crate) fn sidebar_runtime_saved_settings_json(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> String {
    serde_json::to_string(&serde_json::Value::Object(settings.object().clone()))
        .unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn changed_sidebar_runtime_settings_snapshot(
    current: &cef::SidebarRuntimeSettingsSnapshot,
    next: cef::SidebarRuntimeSettingsSnapshot,
) -> Option<cef::SidebarRuntimeSettingsSnapshot> {
    (current != &next).then_some(next)
}

