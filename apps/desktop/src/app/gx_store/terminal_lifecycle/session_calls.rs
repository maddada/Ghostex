//! Single gxserver calls a session's own controls make. Web-ready: `gx_rpc` only.

use serde_json::json;

use crate::app::gx_store::{GxRpcError, gx_rpc};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// `/api/switchSessionAgent` on the local daemon.
pub(crate) async fn switch_session_agent(
    project_id: String,
    session_id: String,
    agent_id: String,
) -> Result<(), GxRpcError> {
    gx_rpc(
        None,
        "/api/switchSessionAgent",
        json!({ "agentId": agent_id, "projectId": project_id, "sessionId": session_id }),
    )
    .await
    .map(|_| ())
}

/// `/api/toggleCloseAfterDone` on the machine that owns the session: flips it, or sets it when
/// `armed` is given. Answers whether it is armed now.
pub(crate) async fn toggle_close_after_done(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    project_id: String,
    session_id: String,
    armed: Option<bool>,
) -> Result<bool, GxRpcError> {
    let mut params = json!({ "projectId": project_id, "sessionId": session_id });
    if let Some(armed) = armed {
        params["armed"] = serde_json::Value::Bool(armed);
    }
    gx_rpc(remote, "/api/toggleCloseAfterDone", params)
        .await
        .map(|result| result.get("armed").and_then(serde_json::Value::as_bool) == Some(true))
}

/// `/api/openConversation` on the local daemon: `{ outcome, projectId, sessionId }` where the
/// outcome is `focus` (a live session), `restored` or `resumed` (a new one).
pub(crate) async fn open_conversation(
    params: serde_json::Value,
) -> Result<serde_json::Value, GxRpcError> {
    gx_rpc(None, "/api/openConversation", params).await
}
