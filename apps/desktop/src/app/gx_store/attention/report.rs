//! `/api/updateAgentActivity` for an acknowledgement or an Escape, on the session's own machine.
//! Uses only `gx_rpc` and gx-core, so the GPUI web build can compile this file unchanged.

use ghostex_gx_core::{AgentActivityReport, SessionKey};
use serde_json::{Map, Value};

use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// Best effort, as the old runtime's: a failure is dropped, and the daemon's next row settles the
/// local clear either way (gx-core `attention.rs`).
pub(super) async fn report_agent_activity(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    session: SessionKey,
    report: AgentActivityReport,
    agent_name: Option<String>,
) {
    let mut params = Map::new();
    if let Some(agent_name) = agent_name {
        params.insert("agentName".into(), Value::from(agent_name));
    }
    params.insert("event".into(), Value::from(report.event()));
    params.insert("projectId".into(), Value::from(session.project_id));
    params.insert("sessionId".into(), Value::from(session.session_id));
    let _ = gx_rpc(remote, "/api/updateAgentActivity", Value::Object(params)).await;
}
