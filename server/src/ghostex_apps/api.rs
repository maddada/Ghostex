//! The authenticated Ghostex Apps routes. Every one is refused while Ghostex Apps is off.
//!
//! - `/api/publishGhostexApp` (publish.rs): agents publish through `ghostex app show`.
//! - `/api/listGhostexApps`, `/api/readGhostexApp {id}`: what is there, with each power's state.
//! - `/api/openGhostexApp {id}`: a launch token and the address to open the app with.
//! - `/api/setGhostexAppGrant {id, permission, decision}`: the user's decision on one power.
//! - `/api/deleteGhostexApp {id}`: removes the app, its versions, icon and data.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! The brief: "never grant anything without the user's click". Opening an app with its powers, changing a grant and deleting an app are for the user's own Ghostex (the desktop app, the web build and the `ghostex` CLI in the user's own terminal on the token file, and paired phones: `Principal::is_owner_device`); an agent's session token, another computer, a Collab guest and an extension are refused, so an agent can build and publish an app but never allow it anything or call its bridge.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    security::Principal,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    feature::{apps_off_message, apps_on},
    manifest::{Power, ALL_POWERS},
    publish::{publish_app, PUBLISH_GHOSTEX_APP_ENDPOINT},
    serve::APP_ROUTE_PREFIX,
    store::{delete, list_records, power_state, read_record, set_grant},
    tokens,
};

pub(crate) const LIST_GHOSTEX_APPS_ENDPOINT: &str = "/api/listGhostexApps";
pub(crate) const READ_GHOSTEX_APP_ENDPOINT: &str = "/api/readGhostexApp";
pub(crate) const OPEN_GHOSTEX_APP_ENDPOINT: &str = "/api/openGhostexApp";
pub(crate) const SET_GHOSTEX_APP_GRANT_ENDPOINT: &str = "/api/setGhostexAppGrant";
pub(crate) const DELETE_GHOSTEX_APP_ENDPOINT: &str = "/api/deleteGhostexApp";

pub(crate) fn is_ghostex_apps_endpoint(path: &str) -> bool {
    matches!(
        path,
        PUBLISH_GHOSTEX_APP_ENDPOINT
            | LIST_GHOSTEX_APPS_ENDPOINT
            | READ_GHOSTEX_APP_ENDPOINT
            | OPEN_GHOSTEX_APP_ENDPOINT
            | SET_GHOSTEX_APP_GRANT_ENDPOINT
            | DELETE_GHOSTEX_APP_ENDPOINT
    )
}

/// Whether `endpoint` is only for the user's own Ghostex.
fn users_only(endpoint: &str) -> bool {
    matches!(
        endpoint,
        OPEN_GHOSTEX_APP_ENDPOINT | SET_GHOSTEX_APP_GRANT_ENDPOINT | DELETE_GHOSTEX_APP_ENDPOINT
    )
}

pub(crate) async fn handle_ghostex_apps_http(
    state: Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: Value,
    caller: Principal,
) -> RoutedResponse {
    if !apps_on(&state.paths) {
        tokens::revoke_all();
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request(apps_off_message()),
        );
    }
    if users_only(&endpoint_path) && !caller.is_owner_device() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "forbidden",
                message: "Only you can do this, from Ghostex itself; an agent can't.".to_string(),
            },
        );
    }
    let params = match read_domain_rpc_params(&body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let state_dir = state.paths.root_dir.clone();
    let endpoint = endpoint_path.clone();
    let result = tokio::task::spawn_blocking(move || run(&state_dir, &endpoint, &params))
        .await
        .unwrap_or_else(|error| {
            Err(DomainStateError::corrupt_state(format!(
                "Ghostex Apps stopped unexpectedly: {error}"
            )))
        });
    match result {
        Ok(value) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn run(
    state_dir: &std::path::Path,
    endpoint: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if endpoint == PUBLISH_GHOSTEX_APP_ENDPOINT {
        return publish_app(state_dir, params);
    }
    if endpoint == LIST_GHOSTEX_APPS_ENDPOINT {
        let apps: Vec<Value> = list_records(state_dir).iter().map(summary).collect();
        return Ok(json!({"apps": apps}));
    }
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let record =
        read_record(state_dir, &id).ok_or_else(|| DomainStateError::not_found("No such app."))?;
    match endpoint {
        READ_GHOSTEX_APP_ENDPOINT => Ok(summary(&record)),
        OPEN_GHOSTEX_APP_ENDPOINT => {
            let token = tokens::mint(&id);
            Ok(json!({
                "id": id,
                "path": format!("{APP_ROUTE_PREFIX}{id}"),
                "token": token,
                "fragment": format!("gx-app-token={token}"),
            }))
        }
        SET_GHOSTEX_APP_GRANT_ENDPOINT => {
            let power = params
                .get("permission")
                .and_then(Value::as_str)
                .and_then(Power::from_name)
                .ok_or_else(|| DomainStateError::bad_request("Name a permission such as send."))?;
            let decision = params
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let record = set_grant(state_dir, &id, power, decision)
                .map_err(DomainStateError::bad_request)?;
            Ok(summary(&record))
        }
        DELETE_GHOSTEX_APP_ENDPOINT => {
            tokens::revoke_app(&id);
            let deleted = delete(state_dir, &id).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Could not delete the app: {error}"),
            })?;
            Ok(json!({"id": id, "deleted": deleted}))
        }
        _ => Err(DomainStateError::not_found("Unknown Ghostex Apps call.")),
    }
}

/// What a client is told about an app: no HTML and no data, only its card, its versions and each
/// declared power's state.
fn summary(record: &Value) -> Value {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let permissions: Map<String, Value> = ALL_POWERS
        .into_iter()
        .filter_map(|power| {
            let state = power_state(record, power);
            (state != "undeclared").then(|| (power.name().to_string(), json!(state)))
        })
        .collect();
    let pending: Vec<Value> = record
        .get("pendingAsks")
        .and_then(Value::as_object)
        .map(|asks| asks.keys().map(|name| json!(name)).collect())
        .unwrap_or_default();
    json!({
        "id": id,
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "version": record.get("version").cloned().unwrap_or(Value::Null),
        "createdAt": record.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": record.get("updatedAt").cloned().unwrap_or(Value::Null),
        "sessionRef": record.get("sessionRef").cloned().unwrap_or(Value::Null),
        "sourcePath": record.get("sourcePath").cloned().unwrap_or(Value::Null),
        "path": format!("{APP_ROUTE_PREFIX}{id}"),
        "iconPath": format!("{APP_ROUTE_PREFIX}{id}/icon"),
        "permissions": permissions,
        "pendingAsks": pending,
    })
}
