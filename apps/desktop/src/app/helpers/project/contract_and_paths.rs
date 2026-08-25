// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds project snapshot contract field
// parsing helpers and workspace/settings state path resolution. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{env, path::PathBuf, time::Duration};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_project_context_from_env_bridge_value(
    value: Option<&str>,
) -> GpuiProjectContext {
    GpuiProjectContext::from_project_is_quick_bridge(value == Some("true"))
}

#[allow(dead_code)]
pub(crate) fn gpui_project_snapshot_from_contract_project_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectSnapshot, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(
        object,
        &[
            "activeProjectId",
            "displayName",
            "projectIconDataUrl",
            "projectPath",
            "selectionOwnerProjectId",
            "isQuickProjectless",
            "workareaAvailability",
            "surfaceIds",
        ],
    )?;

    let active_project_id = required_nullable_contract_string_field(
        object,
        "activeProjectId",
        GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
    )?
    .map(GpuiProjectId);
    let selection_owner_project_id = required_nullable_contract_string_field(
        object,
        "selectionOwnerProjectId",
        GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
    )?
    .map(GpuiProjectId);
    let display_name = required_contract_string_field(
        object,
        "displayName",
        GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
    )?;
    let project_icon_data_url =
        optional_contract_icon_data_url_field(object, "projectIconDataUrl")?;
    let in_memory_project_path = required_nullable_contract_string_field(
        object,
        "projectPath",
        GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS,
    )?
    .map(PathBuf::from);
    let is_quick_projectless = required_contract_bool_field(object, "isQuickProjectless")?;
    let feature_availability = object
        .get("workareaAvailability")
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(gpui_project_feature_availability_from_contract_value)?;
    let surface_ids = object
        .get("surfaceIds")
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(gpui_project_surface_ids_from_contract_value)?;
    let is_quick_automations_overview = active_project_id
        .as_ref()
        .is_some_and(|project_id| project_id.0 == GPUI_QUICK_AUTOMATIONS_PROJECT_ID);

    if !feature_availability.source && !is_quick_automations_overview {
        return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
    }
    if surface_ids.has_ids_for_unavailable_features(feature_availability) {
        return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
    }

    if is_quick_projectless {
        if active_project_id.is_some()
            || selection_owner_project_id.is_some()
            || project_icon_data_url.is_some()
            || in_memory_project_path.is_some()
            || surface_ids.has_any()
            || !feature_availability.is_quick_projectless_compatible()
        {
            return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
        }
    } else if active_project_id.is_none() || selection_owner_project_id.is_none() {
        return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
    }

    Ok(GpuiProjectSnapshot {
        active_project_id,
        selection_owner_project_id,
        display_name,
        project_icon_data_url,
        in_memory_project_path,
        is_quick_projectless,
        feature_availability,
        surface_ids,
    })
}

#[allow(dead_code)]
pub(crate) fn gpui_project_feature_availability_from_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectScopedFeatureAvailability, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(
        object,
        &["source", "browser", "kanban", "automate", "manage"],
    )?;
    Ok(GpuiProjectScopedFeatureAvailability {
        source: required_contract_bool_field(object, "source")?,
        browser: required_contract_bool_field(object, "browser")?,
        kanban: required_contract_bool_field(object, "kanban")?,
        automate: required_contract_bool_field(object, "automate")?,
        manage: required_contract_bool_field(object, "manage")?,
    })
}

#[allow(dead_code)]
pub(crate) fn gpui_project_surface_ids_from_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectSurfaceIds, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(
        object,
        &[
            "sourceWorkareaId",
            "kanbanBoardId",
            "automateBoardId",
            "manageWorkspaceId",
        ],
    )?;
    Ok(GpuiProjectSurfaceIds {
        source_workarea_id: optional_contract_string_field(
            object,
            "sourceWorkareaId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
        kanban_board_id: optional_contract_string_field(
            object,
            "kanbanBoardId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
        automate_board_id: optional_contract_string_field(
            object,
            "automateBoardId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
        manage_workspace_id: optional_contract_string_field(
            object,
            "manageWorkspaceId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
    })
}

#[allow(dead_code)]
pub(crate) fn gpui_contract_object(
    value: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, GpuiProjectSnapshotContractError> {
    value
        .as_object()
        .ok_or(GpuiProjectSnapshotContractError::ExpectedObject)
}

#[allow(dead_code)]
pub(crate) fn reject_unexpected_contract_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_keys: &[&str],
) -> Result<(), GpuiProjectSnapshotContractError> {
    if object
        .keys()
        .any(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(GpuiProjectSnapshotContractError::UnexpectedKey);
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn required_contract_bool_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<bool, GpuiProjectSnapshotContractError> {
    object
        .get(key)
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(|value| {
            value
                .as_bool()
                .ok_or(GpuiProjectSnapshotContractError::MalformedField)
        })
}

#[allow(dead_code)]
pub(crate) fn required_contract_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Result<String, GpuiProjectSnapshotContractError> {
    object
        .get(key)
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(|value| contract_string(value, max_chars))
}

#[allow(dead_code)]
pub(crate) fn required_nullable_contract_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Result<Option<String>, GpuiProjectSnapshotContractError> {
    let value = object
        .get(key)
        .ok_or(GpuiProjectSnapshotContractError::MissingField)?;
    if value.is_null() {
        Ok(None)
    } else {
        contract_string(value, max_chars).map(Some)
    }
}

#[allow(dead_code)]
pub(crate) fn optional_contract_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Result<Option<String>, GpuiProjectSnapshotContractError> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => contract_string(value, max_chars).map(Some),
    }
}

#[allow(dead_code)]
pub(crate) fn optional_contract_icon_data_url_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, GpuiProjectSnapshotContractError> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_status_icon_data_url_allowed(value))
            .map(str::to_string)
            .map(Some)
            .ok_or(GpuiProjectSnapshotContractError::MalformedField),
    }
}

#[allow(dead_code)]
pub(crate) fn contract_string(
    value: &serde_json::Value,
    max_chars: usize,
) -> Result<String, GpuiProjectSnapshotContractError> {
    let value = value
        .as_str()
        .ok_or(GpuiProjectSnapshotContractError::MalformedField)?;
    if value.trim().is_empty() || value.chars().count() > max_chars {
        return Err(GpuiProjectSnapshotContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn next_available_gpui_local_workspace_lifecycle_request_id(
    next_request_id: u64,
    mut is_pending: impl FnMut(u64) -> bool,
) -> Option<(u64, u64)> {
    next_available_gpui_local_workspace_lifecycle_request_id_in_range(
        next_request_id,
        GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_ID_MAX,
        &mut is_pending,
    )
}

pub(crate) fn next_available_gpui_local_workspace_lifecycle_request_id_in_range(
    next_request_id: u64,
    request_id_max: u64,
    mut is_pending: impl FnMut(u64) -> bool,
) -> Option<(u64, u64)> {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:44:
    Rust-to-sidebar Sleep/Wake acks are matched only by request id, so id allocation must never overwrite a still-pending request after wraparound. Local-first Close also receives an id for its ignored cleanup acknowledgement. Skip live pending ids and fail dispatch only if the bounded id space is exhausted.
    */
    let request_id_max = request_id_max.max(1);
    let first_request_id = next_request_id.clamp(1, request_id_max);
    let mut request_id = first_request_id;
    loop {
        let next_candidate = if request_id >= request_id_max {
            1
        } else {
            request_id + 1
        };
        if !is_pending(request_id) {
            return Some((request_id, next_candidate));
        }
        request_id = next_candidate;
        if request_id == first_request_id {
            return None;
        }
    }
}

pub(crate) fn project_editor_auto_sleep_duration(
    mode: TitlebarMode,
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> Option<Duration> {
    let target = match mode {
        TitlebarMode::Source => shared_settings::SharedSettingsAutoSleepTarget::CodeEditor,
        TitlebarMode::Browser => shared_settings::SharedSettingsAutoSleepTarget::Browser,
        TitlebarMode::Kanban | TitlebarMode::Automate | TitlebarMode::Manage => {
            shared_settings::SharedSettingsAutoSleepTarget::ProjectEditor
        }
        TitlebarMode::Agents | TitlebarMode::Extension(_) => return None,
    };
    settings.auto_sleep_duration(target)
}

pub(crate) fn native_chrome_settings_path() -> PathBuf {
    if let Ok(override_path) = env::var("ghostex_SETTINGS_PATH") {
        if !override_path.trim().is_empty() {
            return PathBuf::from(override_path);
        }
    }

    let Some(home) = env::var_os("HOME") else {
        return PathBuf::from("settings.json");
    };
    let app_support = PathBuf::from(home).join("Library/Application Support");
    let candidates = [
        app_support.join("com.madda.ghostex.host/state/settings.json"),
        app_support.join("dev.maddada.ghostex/dev/state/settings.json"),
        app_support.join("com.ghostex.host/state/settings.json"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

pub(crate) fn ghostex_state_root() -> PathBuf {
    shared_settings::ghostex_storage_paths().state_dir.clone()
}

pub(crate) fn gpui_workspace_shell_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-workspace-shell-state.json")
}
