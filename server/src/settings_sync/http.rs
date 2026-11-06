//! Settings > Sync's calls: what syncs and with whom, Undo on a note, removing or adding back a
//! computer, and deleting a project group everywhere.

use axum::http::StatusCode;
use ghostex_gx_core::settings_sync::{
    category_enabled, group_deleted_edit, removed_key, SYNC_CATEGORIES,
};
use serde_json::{json, Value};

use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::{rpc_error, rpc_success};
use crate::server::{routed_json, AppState, RoutedResponse};

use super::engine::{now_ms, with_engine, Engine};
use super::store::SyncNote;

pub(crate) const SETTINGS_SYNC_ENDPOINTS: &[&str] = &[
    "/api/readSettingsSync",
    "/api/undoSettingsSyncChange",
    "/api/removeSettingsSyncComputer",
    "/api/restoreSettingsSyncComputer",
    "/api/deleteSyncedProjectGroup",
];

pub(crate) fn handle_settings_sync_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => Value::Object(params),
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", error.message, Some(request_id)),
            );
        }
    };
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let result = with_engine(state, |engine| match endpoint_path.as_str() {
        "/api/undoSettingsSyncChange" => {
            let id = text("id").ok_or_else(|| DomainStateError::bad_request("id is required."))?;
            let note = engine
                .store
                .notes
                .iter()
                .find(|note| note.conflict.id == id)
                .cloned()
                .ok_or_else(|| DomainStateError::bad_request("That change is no longer listed."))?;
            engine.export()?;
            engine.edit(&note.conflict.lost.key, note.conflict.lost.value.clone());
            if let Some(listed) = engine
                .store
                .notes
                .iter_mut()
                .find(|known| known.conflict.id == id)
            {
                listed.undone = true;
            }
            engine.import()?;
            Ok(json!({ "undone": true }))
        }
        "/api/removeSettingsSyncComputer" | "/api/restoreSettingsSyncComputer" => {
            let server_id = text("serverId")
                .filter(|id| {
                    (crate::connected_computers::is_server_id(id)
                        || ghostex_gx_core::settings_sync::is_phone_member(id))
                        && *id != engine.server_id
                })
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "serverId must be another computer's server id or a phone's id.",
                    )
                })?;
            let value = (endpoint_path == "/api/removeSettingsSyncComputer")
                .then(|| json!({ "by": engine.name }));
            engine.edit(&removed_key(&server_id), value);
            Ok(status(engine))
        }
        "/api/deleteSyncedProjectGroup" => {
            let group = text("collectionId")
                .ok_or_else(|| DomainStateError::bad_request("collectionId is required."))?;
            engine.export()?;
            let (key, value) = group_deleted_edit(&group);
            engine.edit(&key, value);
            engine.import()?;
            Ok(json!({ "deleted": true }))
        }
        _ => Ok(status(engine)),
    });
    super::nudge_settings_sync();
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            if error.code == "badRequest" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}

/// What Settings > Sync shows.
fn status(engine: &Engine<'_>) -> Value {
    let settings = engine.settings();
    let connected: Vec<String> = crate::connected_computers::list()
        .into_iter()
        .filter(|computer| computer.agent_access)
        .map(|computer| computer.server_id)
        .collect();
    let categories: Vec<Value> = SYNC_CATEGORIES
        .iter()
        .map(|category| {
            json!({
                "id": category.prefix,
                "settingKey": category.setting_key,
                "label": category.label,
                "available": category.available,
                "enabled": category_enabled(category, settings),
            })
        })
        .collect();
    let mut computers: Vec<Value> = Vec::new();
    for (key, entry) in engine.store.entries.range("group/member/".to_string()..) {
        let Some(server_id) = key.strip_prefix("group/member/") else {
            break;
        };
        let Some(member) = &entry.value else {
            continue;
        };
        let peer = engine.store.peers.get(server_id);
        computers.push(json!({
            "serverId": server_id,
            "name": member.get("name").and_then(Value::as_str).unwrap_or(server_id),
            "thisComputer": server_id == engine.server_id,
            "phone": ghostex_gx_core::settings_sync::is_phone_member(server_id),
            "connected": connected.iter().any(|id| id == server_id),
            "lastSyncMs": peer.map(|peer| peer.last_sync_ms).filter(|ms| *ms > 0),
            "removed": engine.removed(server_id),
            "refusedHere": peer.and_then(|peer| peer.refused.clone()),
        }));
    }
    let notes: Vec<Value> = engine
        .store
        .notes
        .iter()
        .rev()
        .map(|note| note_view(note, &engine.server_id))
        .collect();
    json!({
        "serverId": engine.server_id,
        "name": engine.name,
        "categories": categories,
        "computers": computers,
        "notes": notes,
        "nowMs": now_ms(),
    })
}

/// A note as clients show it.
pub(super) fn note_view(note: &SyncNote, server_id: &str) -> Value {
    json!({
        "id": note.conflict.id,
        "text": note.text,
        "keptOn": note.conflict.kept.origin_name,
        "lostOn": note.conflict.lost.origin_name,
        "lostHere": note.conflict.lost.origin == server_id,
        "atMs": note.at_ms,
        "undone": note.undone,
    })
}
