//! `/api/gbots {op, …}`: what clients and the `ghostex gbots` CLI ask of GBots. Until native
//! GBots land, `list` answers with no bots and every other op is refused.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

pub(crate) async fn gbots_action(
    _state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let op = params
        .get("op")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    match op {
        "list" => Ok(json!({ "bots": [] })),
        _ => Err(DomainStateError::bad_request("op must be list.")),
    }
}
