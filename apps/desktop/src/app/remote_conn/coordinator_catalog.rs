//! Another computer's agent model catalog, read once for the New Coordinator dialog.
//!
//! CDXC:AgentModelCatalog 2026-11-09 WHY:
//! A computer's catalog reaches this one only over a chat socket to it, which is open only while one of its chats is followed, so the New Coordinator dialog would offer another computer's Model and Effort only after a chat there was opened. Every gxserver sends the lineup it serves (`agentModelCatalogChanged`) to each event socket as it connects (server/src/server/ws.rs), so the dialog opens one through the computer's tunnel, takes that first event and closes it. A gxserver that holds no catalog yet, or predates the push, sends none: the dialog then offers no model choice there rather than this computer's lineup.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crate::app::helpers::*;
use crate::app::window::RemoteModelCatalog;
use crate::*;

/// How long the read waits for the catalog before treating the computer as having none.
const CATALOG_READ_TIMEOUT: Duration = Duration::from_secs(5);

impl GhostexGpuiApp {
    /// The model catalog the connected computer `machine_id` serves: parsed, none served, or not
    /// reached (not connected, or no answer in time).
    pub(crate) fn gpui_remote_model_catalog(
        &self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<RemoteModelCatalog> {
        let Some(target) = self.gpui_remote_gxserver_request_target(machine_id) else {
            return gpui::Task::ready(RemoteModelCatalog::Unreachable);
        };
        cx.background_executor().spawn(async move {
            match read_remote_model_catalog(&target) {
                Some(Some(catalog)) => {
                    ghostex_gx_chat_core::menus::catalog::parse_agent_model_catalog(&catalog)
                        .map_or(RemoteModelCatalog::NoneServed, RemoteModelCatalog::Served)
                }
                Some(None) => RemoteModelCatalog::NoneServed,
                None => RemoteModelCatalog::Unreachable,
            }
        })
    }
}

/// Opens the computer's event socket, waits for the catalog it sends as the socket connects, and
/// closes it: `Some(Some(_))` the catalog, `Some(None)` it answered without one, `None` it was
/// not reached in time.
fn read_remote_model_catalog(
    target: &GpuiRemoteGxserverRequestTarget,
) -> Option<Option<serde_json::Value>> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let cancel = cancel.clone();
        thread::spawn(move || {
            thread::sleep(CATALOG_READ_TIMEOUT);
            cancel.store(true, Ordering::SeqCst);
        });
    }
    let mut stream = gpui_remote_gxserver_open_presentation_websocket(target, &cancel).ok()?;
    let mut liveness = GpuiRemoteGxserverPresentationLiveness::new();
    let mut ready = false;
    while !cancel.load(Ordering::SeqCst) {
        match gpui_websocket_read_frame(&mut stream, target, &mut liveness, &cancel).ok()?? {
            GpuiWebSocketFrame::Text(text) => {
                let event = serde_json::from_str::<serde_json::Value>(text.trim()).ok()?;
                match event.get("type").and_then(serde_json::Value::as_str) {
                    Some("eventStreamReady") => ready = true,
                    Some("agentModelCatalogChanged") => return Some(event.get("catalog").cloned()),
                    // The catalog comes right after the ready event or not at all.
                    _ if ready => return Some(None),
                    _ => {}
                }
            }
            GpuiWebSocketFrame::Ping(payload) => {
                gpui_websocket_write_control_frame(&mut stream, 0xA, &payload).ok()?;
            }
            GpuiWebSocketFrame::Close => return ready.then_some(None),
            GpuiWebSocketFrame::Pong | GpuiWebSocketFrame::Other => {}
        }
    }
    None
}
