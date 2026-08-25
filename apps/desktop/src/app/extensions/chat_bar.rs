use std::rc::Rc;

use crate::*;

use super::{GpuiExtensionBridgeResponder, GpuiExtensionPlacement, GpuiExtensionSurfaceContext};

impl GhostexGpuiApp {
    pub(crate) fn handle_chat_bar_extension_bridge_request(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(extension_id) = message
            .get("extensionId")
            .and_then(serde_json::Value::as_str)
            .and_then(ExtensionId::new)
        else {
            return;
        };
        let Some(request) = message
            .get("request")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        let Ok(payload) = serde_json::to_string(request) else {
            return;
        };
        let start_session =
            self.local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (*mapped_session_id == session_id)
                        .then(|| self.extension_session_details.get(&key.session_id).cloned())
                        .flatten()
                });
        let surface_context = GpuiExtensionSurfaceContext {
            placement: GpuiExtensionPlacement::ChatBar,
            start_session,
        };
        let response_app = cx.entity().downgrade();
        let response_async_cx = cx.to_async();
        let response_foreground = cx.foreground_executor().clone();
        let responder: GpuiExtensionBridgeResponder = Rc::new(move |payload| {
            let response_app = response_app.clone();
            let mut response_async_cx = response_async_cx.clone();
            response_foreground
                .spawn(async move {
                    let _ = response_app.update_in(&mut response_async_cx, |this, _window, cx| {
                        this.dispatch_chat_bar_extension_bridge_message(
                            session_id,
                            "onSessionChatExtensionBridgeMessage",
                            &payload,
                            cx,
                        );
                    });
                })
                .detach();
        });
        self.handle_extension_bridge_event(
            cef::ExtensionBridgeEvent {
                extension_id: extension_id.as_str().to_string(),
                payload,
            },
            surface_context,
            responder,
            None,
            cx,
        );
    }

    pub(crate) fn broadcast_chat_bar_extension_context_changes(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let session_ids = self
            .agents_chat_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for session_id in session_ids {
            let start_session = self.local_workspace_session_mappings.iter().find_map(
                |(key, mapped_session_id)| {
                    (*mapped_session_id == session_id)
                        .then(|| self.extension_session_details.get(&key.session_id).cloned())
                        .flatten()
                },
            );
            let context = self.extension_context_payload(&GpuiExtensionSurfaceContext {
                placement: GpuiExtensionPlacement::ChatBar,
                start_session,
            });
            self.dispatch_chat_bar_extension_bridge_message(
                session_id,
                "onSessionChatExtensionContextChanged",
                &context,
                cx,
            );
        }
    }

    fn dispatch_chat_bar_extension_bridge_message(
        &mut self,
        session_id: TerminalSessionId,
        callback: &str,
        payload: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        let script = format!("window.ghostexGpui?.{callback}?.({payload}); undefined;");
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }
}
