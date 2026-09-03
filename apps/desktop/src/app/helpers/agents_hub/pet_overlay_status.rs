// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the pet-overlay status/activity state
// types and animation-frame constants/statics, plus the sidebar
// status-indicator JSON parsing, validation, and activation-target helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::sync::{Arc, OnceLock};

use gpui::Image;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiPetOverlayStatusItemState {
    pub(crate) count: u64,
    pub(crate) status: GpuiStatusIndicatorStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiPetOverlayActivityState {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) state: GpuiStatusIndicatorStatus,
    pub(crate) title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiPetOverlayAnimationState {
    Idle,
    Jumping,
    Review,
    Running,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiPetOverlayAnimationFrame {
    pub(crate) column_index: u8,
    pub(crate) duration_ms: u64,
    pub(crate) row_index: u8,
}

pub(crate) const GPUI_PET_OVERLAY_IDLE_FRAMES: [GpuiPetOverlayAnimationFrame; 6] = [
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 0,
        duration_ms: 280,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 1,
        duration_ms: 110,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 2,
        duration_ms: 110,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 3,
        duration_ms: 140,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 4,
        duration_ms: 140,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 5,
        duration_ms: 320,
    },
];
pub(crate) const GPUI_PET_OVERLAY_JUMPING_FRAMES: [GpuiPetOverlayAnimationFrame; 5] =
    gpui_pet_overlay_row_frames::<5>(4, 140, 280);
pub(crate) const GPUI_PET_OVERLAY_REVIEW_FRAMES: [GpuiPetOverlayAnimationFrame; 6] =
    gpui_pet_overlay_row_frames::<6>(8, 150, 280);
pub(crate) const GPUI_PET_OVERLAY_RUNNING_FRAMES: [GpuiPetOverlayAnimationFrame; 6] =
    gpui_pet_overlay_row_frames::<6>(7, 120, 220);

pub(crate) static GPUI_PET_OVERLAY_BOO_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_BSOD_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_CODEX_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_DEWEY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_FIREBALL_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_NULL_SIGNAL_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_ROCKY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_SEEDY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_STACKY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();

pub(crate) fn gpui_pet_overlay_status_item_from_value(
    value: &serde_json::Value,
) -> Result<GpuiPetOverlayStatusItemState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["status", "count"])?;
    Ok(GpuiPetOverlayStatusItemState {
        count: gpui_status_count_field(object, "count")?,
        status: gpui_status_field(object, "status")?,
    })
}

pub(crate) fn gpui_pet_overlay_activity_from_value(
    value: &serde_json::Value,
) -> Result<GpuiPetOverlayActivityState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["id", "projectId", "state", "title"])?;
    Ok(GpuiPetOverlayActivityState {
        project_id: gpui_status_id_field(object, "projectId")?,
        session_id: gpui_status_id_field(object, "id")?,
        state: gpui_status_field(object, "state")?,
        title: gpui_status_title_field(object, "title")?,
    })
}

pub(crate) fn gpui_pet_overlay_selected_pet_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    let pet_id = gpui_status_id_field(object, key)?;
    if gpui_pet_overlay_pet_id_known(pet_id.as_str()) {
        Ok(pet_id)
    } else {
        Err(())
    }
}

pub(crate) fn reject_unexpected_gpui_status_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_keys: &[&str],
) -> Result<(), ()> {
    if object
        .keys()
        .any(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(());
    }
    Ok(())
}

pub(crate) fn gpui_status_bool_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<bool, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or(())
}

pub(crate) fn gpui_status_count_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<u64, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value <= u64::from(u16::MAX))
        .ok_or(())
}

pub(crate) fn gpui_status_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<GpuiStatusIndicatorStatus, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiStatusIndicatorStatus::from_str)
        .ok_or(())
}

pub(crate) fn gpui_status_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_status_bridge_id_allowed(value))
        .map(str::to_string)
        .ok_or(())
}

pub(crate) fn gpui_status_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_status_title_allowed(value))
        .map(str::to_string)
        .ok_or(())
}

pub(crate) fn gpui_status_optional_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ()> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(_) => gpui_status_title_field(object, key).map(Some),
    }
}

pub(crate) fn gpui_status_optional_icon_data_url_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ()> {
    /*
    CDXC:Notifications 2026-06-26-07:22:
    GPUI session-attention icons may enter Rust only as bounded image data URLs on sanitized status project rows. Reject paths, http(s) URLs, control characters, oversized strings, malformed headers, and unexpected keys so native notification attachment creation never receives a fallback file/URL source or raw renderer payload.
    */
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_status_icon_data_url_allowed(value))
            .map(str::to_string)
            .map(Some)
            .ok_or(()),
    }
}

pub(crate) fn gpui_status_bridge_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_status_icon_data_url_allowed(value: &str) -> bool {
    if value.is_empty()
        || value.len() > GPUI_STATUS_INDICATOR_ICON_DATA_URL_MAX_CHARS
        || !value.is_ascii()
        || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return false;
    }
    let Some(comma_index) = value.find(',') else {
        return false;
    };
    let header = value[..comma_index].to_ascii_lowercase();
    let payload = &value[comma_index + 1..];
    header.starts_with("data:image/")
        && header.contains(";base64")
        && gpui_status_icon_base64_payload_allowed(payload)
}

pub(crate) fn gpui_status_icon_base64_payload_allowed(payload: &str) -> bool {
    if payload.is_empty() || payload.len() % 4 != 0 {
        return false;
    }
    let padding_index = payload.find('=').unwrap_or(payload.len());
    let padding_len = payload.len().saturating_sub(padding_index);
    padding_len <= 2
        && payload[padding_index..].bytes().all(|byte| byte == b'=')
        && payload[..padding_index]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
}

pub(crate) fn gpui_status_title_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_pet_overlay_activity_activation_session_id(
    activity: &GpuiPetOverlayActivityState,
) -> &str {
    /*
    CDXC:StatusPet 2026-06-26-05:30:
    Pet activity cards activate the exact sanitized activity `id` supplied by the sidebar bridge. The GPUI click payload must not include project ids, titles, paths, URLs, commands, logs, or terminal content, and it must not derive a different target from the visible card text.
    */
    activity.session_id.as_str()
}

pub(crate) fn gpui_status_indicator_aggregate_activation_session_id(
    state: &GpuiSidebarSessionStatusIndicatorsState,
    status: GpuiStatusIndicatorStatus,
    focused_session_id: Option<&str>,
) -> Option<String> {
    /*
    CDXC:StatusPet 2026-06-26-05:07:
    Aggregate GPUI status clicks must mirror macOS ordering from the sanitized sidebar payload: sort same-status candidates by lastActiveAt descending, then sidebarOrder. If the focused session is already the top candidate and another candidate exists, cycle to the next candidate instead of reselecting the same row.
    */
    let mut candidates = state
        .projects
        .iter()
        .flat_map(|project| project.sessions.iter())
        .filter(|session| session.status == status)
        .collect::<Vec<_>>();
    candidates.sort_by(gpui_status_indicator_activation_candidate_order);
    if candidates.is_empty() {
        return None;
    }
    if let Some(focused_session_id) = focused_session_id {
        if candidates.len() > 1 {
            if let Some(index) = candidates.iter().position(|session| {
                gpui_status_indicator_session_matches_focus(
                    session.session_id.as_str(),
                    focused_session_id,
                )
            }) {
                return Some(
                    candidates[(index + 1) % candidates.len()]
                        .session_id
                        .clone(),
                );
            }
        }
    }
    Some(candidates[0].session_id.clone())
}

pub(crate) fn gpui_status_indicator_activation_candidate_order(
    left: &&GpuiStatusIndicatorSessionState,
    right: &&GpuiStatusIndicatorSessionState,
) -> std::cmp::Ordering {
    let left_timestamp = left.last_active_at.as_deref().unwrap_or("");
    let right_timestamp = right.last_active_at.as_deref().unwrap_or("");
    right_timestamp
        .cmp(left_timestamp)
        .then_with(|| left.order.cmp(&right.order))
}

pub(crate) fn gpui_status_indicator_session_matches_focus(
    session_id: &str,
    focused_session_id: &str,
) -> bool {
    if session_id == focused_session_id {
        return true;
    }
    if gpui_local_combined_session_original_id(session_id)
        .as_deref()
        .is_some_and(|original_session_id| original_session_id == focused_session_id)
    {
        return true;
    }
    gpui_local_combined_session_original_id(focused_session_id)
        .as_deref()
        .is_some_and(|original_session_id| original_session_id == session_id)
}

pub(crate) fn gpui_local_combined_session_original_id(session_id: &str) -> Option<String> {
    const PREFIX: &str = "combined-session:";
    let payload = session_id.strip_prefix(PREFIX)?;
    let (_project_id, encoded_session_id) = payload.split_once(':')?;
    let decoded =
        browser_favicon_percent_decode(encoded_session_id, GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
            .and_then(|bytes| String::from_utf8(bytes).ok())?;
    (!decoded.is_empty()
        && decoded.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !decoded.chars().any(char::is_control))
    .then_some(decoded)
}

pub(crate) fn gpui_status_pet_activation_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onStatusPetActivation==='function'){{bridge.onStatusPetActivation(payload);}}else{{const pending=Array.isArray(bridge.pendingStatusPetActivations)?bridge.pendingStatusPetActivations:[];pending.push(payload);bridge.pendingStatusPetActivations=pending;}}}})(); undefined;"
    )
}
