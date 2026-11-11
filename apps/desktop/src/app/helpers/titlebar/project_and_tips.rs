use anyhow::Result;

use crate::*;

pub(crate) fn gpui_titlebar_project_selection_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
    settings_key: &str,
    project_id: &str,
) -> Option<String> {
    settings
        .get(settings_key)
        .and_then(serde_json::Value::as_object)
        .and_then(|selections| selections.get(project_id))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        })
        .map(str::to_string)
}

pub(crate) fn gpui_persist_titlebar_project_selection(
    settings_key: &str,
    project_id: &str,
    value: &str,
) -> Result<(), shared_settings::SharedSidebarSettingsWriteError> {
    let project_id = project_id.trim();
    let value = value.trim();
    if project_id.is_empty()
        || value.is_empty()
        || project_id.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || !matches!(
            settings_key,
            GPUI_TITLEBAR_OPEN_TARGET_SELECTIONS_SETTINGS_KEY
                | GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY
        )
    {
        return Ok(());
    }

    let mut settings = shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .clone();
    let selections = settings
        .entry(settings_key.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !selections.is_object() {
        *selections = serde_json::Value::Object(serde_json::Map::new());
    }
    let selections = selections
        .as_object_mut()
        .expect("titlebar selection map must be an object");
    if !selections.contains_key(project_id)
        && selections.len() >= GPUI_TITLEBAR_SELECTION_PROJECT_LIMIT
        && let Some(oldest_key) = selections.keys().next().cloned()
    {
        selections.remove(&oldest_key);
    }
    selections.insert(
        project_id.to_string(),
        serde_json::Value::String(value.to_string()),
    );
    shared_settings::write_shared_sidebar_settings_object(settings).map(|_| ())
}

/// Saves the new chat tips card's read tips (native_chat/new_chat_tips.rs) to the settings file:
/// the ids of tips the data file still has, so a retired tip's id leaves on the next save. An empty
/// list is Settings > Chat > Show tips again.
pub(crate) fn gpui_write_new_chat_tips_read_ids(read_ids: Vec<String>) {
    let mut read_ids: Vec<serde_json::Value> = read_ids
        .into_iter()
        .filter(|id| ghostex_gx_chat_core::extras::new_chat_tips::is_new_chat_tip_id(id))
        .map(serde_json::Value::String)
        .collect();
    read_ids.dedup();
    let mut settings = shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .clone();
    settings.insert(
        crate::app::native_chat::new_chat_tips::NEW_CHAT_TIPS_READ_SETTINGS_KEY.to_string(),
        serde_json::Value::Array(read_ids),
    );
    let _ = shared_settings::write_shared_sidebar_settings_object(settings);
}

pub(crate) fn titlebar_project_label_from_latest_sidebar_snapshot(
    latest_snapshot: Option<&GpuiProjectSnapshot>,
) -> String {
    /*
    CDXC:Titlebar 2026-06-22-19:57:
    The titlebar label is runtime-only sidebar state: use the latest valid snapshot display name and show the static Ghostex label before any valid sidebar payload arrives. Do not read env vars, repo folders, .git metadata, workspace names, fixture names, paths, URLs, sidebar titles, persisted state, or logs to infer the label.
    */
    latest_snapshot
        .map(|snapshot| snapshot.display_name.clone())
        .unwrap_or_else(|| TITLEBAR_PROJECT_LABEL_FALLBACK.to_string())
}
