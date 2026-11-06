//! `settingsSyncExchange`: one request trades both computers' entries. The computer that starts it
//! sends all of its synced entries; the other merges them, writes what changed on its side and
//! answers with all of its own, which the first then merges.
//!
//! CDXC:SettingsSync 2026-11-06 WHY:
//! The whole set goes both ways every time (settings are a few hundred small keys) so a computer never has to remember what another has seen, a third computer's changes pass through whoever meets, and the phone can take part with plain JSON. Only one side of a connection needs to reach the other: an Easy Connect link has no way back, and one exchange covers both directions.

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use ghostex_gx_core::settings_sync::SyncEntry;
use serde_json::{json, Value};

use crate::connected_computers::ConnectedComputer;
use crate::protocol::{rpc_error, rpc_success};
use crate::server::{routed_json, AppState, RoutedResponse};

use super::engine::{now_ms, with_engine};
use super::store::SyncNote;

pub(crate) const EXCHANGE_ENDPOINT: &str = "/api/settingsSyncExchange";

/// Answers another computer's exchange. `peer` is the server id its peer token belongs to; this
/// computer's own clients (`None`) may exchange for testing and for the phone later.
pub(crate) async fn handle_settings_sync_exchange(
    state: Arc<AppState>,
    peer: Option<String>,
    body: Value,
    request_id: String,
) -> RoutedResponse {
    let params = body.get("params").unwrap_or(&body);
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let refuse = |status: StatusCode, code: &str, message: String| {
        routed_json(
            Some(EXCHANGE_ENDPOINT.to_string()),
            status,
            rpc_error(code, message, Some(request_id.clone())),
        )
    };
    // A paired phone asks with its own client token and its own id (CDXC:SettingsSync 2026-11-06
    // in gx-core settings_sync `phone_member_id`); a computer's token never speaks for a phone.
    let phone = text("phoneId")
        .filter(|_| peer.is_none())
        .and_then(|id| ghostex_gx_core::settings_sync::phone_member_id(&id));
    let to_phone = phone.is_some();
    let Some(caller) = phone
        .or_else(|| text("serverId").filter(|id| crate::connected_computers::is_server_id(id)))
    else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "serverId (or a phone's phoneId) is required.".to_string(),
        );
    };
    if peer.as_deref().is_some_and(|peer| peer != caller) || caller == state.metadata.server_id {
        return refuse(
            StatusCode::FORBIDDEN,
            "forbidden",
            "That serverId isn't the computer this token was given to.".to_string(),
        );
    }
    let name = text("name").unwrap_or_else(|| caller.clone());
    // A computer asks first with the fingerprint of what it holds (`probe`); the same fingerprint
    // here means there is nothing to trade, and only a different one is followed by the entries.
    let probe = !to_phone && params.get("probe").and_then(Value::as_bool) == Some(true);
    let their_digest = text("digest");
    let their_categories: Vec<String> = params
        .get("categories")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|prefix| prefix.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let entries: Vec<SyncEntry> = params
        .get("entries")
        .cloned()
        .and_then(|entries| serde_json::from_value(entries).ok())
        .unwrap_or_default();
    let step_state = state.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        with_engine(&step_state, |engine| {
            if engine.removed(&caller) {
                return Ok(Err(()));
            }
            if probe {
                engine.export()?;
                let same = their_digest.as_deref() == Some(engine.digest_with(&their_categories).as_str());
                let record = engine.store.peers.entry(caller.clone()).or_default();
                record.name = name.clone();
                if same {
                    record.last_sync_ms = now_ms();
                }
                return Ok(Ok((
                    json!({
                        "serverId": engine.server_id,
                        "name": engine.name,
                        "same": same,
                        "categories": engine.enabled_categories(),
                    }),
                    Vec::new(),
                    false,
                )));
            }
            engine.export()?;
            let first = !engine.store.peers.get(&caller).is_some_and(|peer| peer.met);
            let notes = engine.merge(entries, first);
            engine.welcome(&caller, &name);
            engine.import()?;
            let record = engine.store.peers.entry(caller.clone()).or_default();
            record.name = name.clone();
            record.last_sync_ms = now_ms();
            record.met = true;
            record.refused = None;
            Ok(Ok((
                json!({
                    "serverId": engine.server_id,
                    "name": engine.name,
                    "digest": engine.digest(),
                    "entries": if to_phone { engine.outgoing_to_phone() } else { engine.outgoing() },
                    // What this computer took and sends; a phone keeps its other entries as they are.
                    "categories": engine
                        .enabled_categories()
                        .into_iter()
                        .filter(|prefix| !to_phone || *prefix != super::engine::PHONE_LEFT_OUT)
                        .collect::<Vec<_>>(),
                }),
                notes,
                engine.spaces_named_apart,
            )))
        })
    })
    .await;
    match outcome {
        Ok(Ok(Ok((answer, notes, named_apart)))) => {
            broadcast_notes(&state, &notes);
            if named_apart {
                broadcast_spaces_named_apart(&state);
            }
            routed_json(
                Some(EXCHANGE_ENDPOINT.to_string()),
                StatusCode::OK,
                rpc_success(request_id, answer),
            )
        }
        Ok(Ok(Err(()))) => refuse(
            StatusCode::FORBIDDEN,
            "syncRemoved",
            format!(
                "This computer was removed from sync on {}. Add it back there in Settings > Sync.",
                crate::connected_computers::this_computer_name()
            ),
        ),
        Ok(Err(error)) => refuse(StatusCode::INTERNAL_SERVER_ERROR, error.code, error.message),
        Err(_) => refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internalError",
            "Settings sync stopped unexpectedly.".to_string(),
        ),
    }
}

/// What asking another computer came to.
pub(super) enum Asked {
    Answered(Value),
    /// It removed this computer from sync (its message).
    Removed(String),
    /// Its gxserver has no settings sync.
    Unsupported,
    Failed,
}

/// Sends this computer's entries to `computer` over the peer link and returns its answer.
pub(super) fn ask(computer: &ConnectedComputer, body: &Value) -> Asked {
    let url = format!("{}{EXCHANGE_ENDPOINT}", computer.base_url);
    // The token goes only to a listener of this account (ghostex_paths::loopback_owner).
    if ghostex_paths::loopback_owner::check_url(&url).is_err() {
        return Asked::Failed;
    }
    let response = crate::http_agent::answering_agent()
        .post(&url)
        .header("authorization", &format!("Bearer {}", computer.token()))
        .header("content-type", "application/json")
        .header(
            crate::constants::GXSERVER_PROTOCOL_HEADER,
            &crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .send(body.to_string().as_bytes());
    let Ok(response) = response else {
        return Asked::Failed;
    };
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    if std::io::Read::read_to_end(&mut response.into_body().into_reader(), &mut bytes).is_err() {
        return Asked::Failed;
    }
    let answer: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    match status {
        200 => answer
            .get("result")
            .cloned()
            .map_or(Asked::Failed, Asked::Answered),
        403 if answer.get("error").and_then(Value::as_str) == Some("syncRemoved") => {
            Asked::Removed(
                answer
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("This computer was removed from sync there.")
                    .to_string(),
            )
        }
        403 | 404 => Asked::Unsupported,
        _ => Asked::Failed,
    }
}

/// Tells this computer's clients that the first sync with a computer merged only the Spaces named
/// alike, for the toast asking to name Spaces the same on each computer (Q1354).
pub(super) fn broadcast_spaces_named_apart(state: &AppState) {
    state.event_hub.broadcast(json!({
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": "settingsSyncNote",
        "note": { "kind": "spacesNamedApart" },
    }));
}

/// Tells this computer's clients about new conflict notes (the "changed on X · Undo" toast).
pub(super) fn broadcast_notes(state: &AppState, notes: &[SyncNote]) {
    for note in notes {
        state.event_hub.broadcast(json!({
            "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
            "serverId": state.metadata.server_id.clone(),
            "type": "settingsSyncNote",
            "note": super::http::note_view(note, &state.metadata.server_id),
        }));
    }
}
