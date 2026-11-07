//! `/api/updates`: `action` is `list` (`fresh` reads every source again), `install` (`ids`, or
//! `all` for every item with an update Ghostex can run), `setAuto` (`id`, `enabled`) or `dismiss`
//! (`id`, `error`: hides that failure until the item's next update runs). Every
//! answer is `{ items, running, auto }`, `auto` being the ids whose switch is on.
use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::{install, items, store};
use crate::domain::read_domain_rpc_params;
use crate::protocol::{rpc_error, rpc_success};
use crate::server::{routed_json, AppState, RoutedResponse};

pub(crate) async fn handle_updates_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = match read_domain_rpc_params(body) {
        Ok(params) => dispatch(state, &params).await,
        Err(error) => Err(error.message),
    };
    match result {
        Ok(answer) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, answer),
        ),
        Err(message) => routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error("badRequest", message, Some(request_id)),
        ),
    }
}

async fn dispatch(state: &AppState, params: &Map<String, Value>) -> Result<Value, String> {
    let fresh = params.get("fresh").and_then(Value::as_bool) == Some(true);
    match params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("list")
    {
        "list" => {}
        "install" => {
            let ids: Vec<String> = if params.get("all").and_then(Value::as_bool) == Some(true) {
                items::list(state, false)
                    .await
                    .into_iter()
                    .filter(|item| {
                        matches!(item["state"].as_str(), Some("available" | "failed"))
                            && item["canUpdate"] == true
                    })
                    .filter_map(|item| item["id"].as_str().map(str::to_string))
                    .collect()
            } else {
                params
                    .get("ids")
                    .and_then(Value::as_array)
                    .ok_or("ids is required.")?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            };
            install::start(state, ids, "request");
        }
        "setAuto" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or("id is required.")?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("enabled must be true or false.")?;
            if id == "app" {
                crate::app_update::set_app_auto_update(state, enabled).await?;
                items::invalidate();
            } else {
                store::set_auto(&state.paths, id, enabled)?;
            }
        }
        "dismiss" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or("id is required.")?;
            let message = params
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_default();
            store::dismiss(id, message);
        }
        _ => return Err("Unknown updates action.".to_string()),
    }
    let items = items::list(state, fresh).await;
    // A row its own Settings page is updating (an agent job queued behind another install) runs
    // too, so the desktop keeps reading the list until it ends.
    let running = store::any_running() || items.iter().any(|item| item["state"] == "updating");
    Ok(json!({
        "items": items,
        "running": running,
        // Every switch that is on, also for the items only the desktop lists (updates/ there).
        "auto": store::auto_ids(&state.paths),
    }))
}
