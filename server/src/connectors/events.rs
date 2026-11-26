//! `connectorsChanged`: tells clients a workspace's connectors changed (signed in or out, apps
//! connected, tools listed, the Executor CLI installed or signed in or out), so Settings refetches
//! `/api/readConnectors`.

use std::sync::{Arc, Weak};

use serde_json::json;

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::AppState;

pub(crate) const CONNECTORS_CHANGED_EVENT_TYPE: &str = "connectorsChanged";

pub(crate) fn broadcast_connectors_changed(state: &AppState, workspace: &str) {
    super::bump_tools_revision();
    state.event_hub.broadcast(json!({
        "type": CONNECTORS_CHANGED_EVENT_TYPE,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "workspaceId": workspace,
    }));
}

/// Wires the Composio, CLI and Executor change listeners to the event and stops the Executor CLI's
/// MCP relay when gxserver shuts down, for this gxserver's lifetime.
pub(crate) fn start_connectors(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    let weak: Weak<AppState> = Arc::downgrade(&state);
    let listener: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |workspace: &str| {
        if let Some(state) = weak.upgrade() {
            broadcast_connectors_changed(&state, workspace);
        }
    });
    super::composio::set_changed_listener(listener.clone());
    super::cli::set_changed_listener(listener.clone());
    super::executor::set_changed_listener(listener);
    tokio::spawn(async move {
        let _ = shutdown.recv().await;
        let _ = tokio::task::spawn_blocking(super::executor::stop_relay).await;
    });
}
