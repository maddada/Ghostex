use std::rc::Rc;
use std::time::Duration;

use gpui::Action;
use gpui::{Bounds, Pixels, Window};

use crate::*;

#[derive(Clone, Debug, Eq, PartialEq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct LaunchGpuiExtension {
    pub(crate) extension_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct ToggleGpuiExtensionPin {
    pub(crate) extension_id: String,
    pub(crate) pinned: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct BrowseGpuiExtensions;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiExtensionLaunch {
    View(ExtensionId),
    ChatBar(ExtensionId),
    Popup(ExtensionId),
    Modal(ExtensionId),
    TerminalPane(ExtensionId),
}

impl GhostexGpuiApp {
    pub(crate) fn extension_launch_for_id(&self, id: ExtensionId) -> Option<GpuiExtensionLaunch> {
        let extension = self
            .extensions_snapshot
            .installed
            .get(id.as_str())
            .filter(|extension| extension.enabled)?;
        if extension.terminal_pane {
            return Some(GpuiExtensionLaunch::TerminalPane(id));
        }
        match extension.placement? {
            GpuiExtensionPlacement::View => Some(GpuiExtensionLaunch::View(id)),
            GpuiExtensionPlacement::ChatBar => Some(GpuiExtensionLaunch::ChatBar(id)),
            GpuiExtensionPlacement::Popup => Some(GpuiExtensionLaunch::Popup(id)),
            GpuiExtensionPlacement::Modal => Some(GpuiExtensionLaunch::Modal(id)),
        }
    }

    pub(crate) fn launch_extension_from_titlebar(
        &mut self,
        extension_id: &str,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(id) = ExtensionId::new(extension_id) else {
            return false;
        };
        let Some(launch) = self.extension_launch_for_id(id) else {
            return false;
        };
        match launch {
            GpuiExtensionLaunch::View(id) => {
                self.close_titlebar_extension_popup(window, cx);
                self.set_active_mode(TitlebarMode::Extension(id), window, cx)
            }
            GpuiExtensionLaunch::ChatBar(id) => {
                self.close_titlebar_extension_popup(window, cx);
                self.launch_chat_bar_extension(id, window, cx)
            }
            GpuiExtensionLaunch::Popup(id) => {
                self.set_titlebar_extension_popup_open(id, trigger_bounds, window, cx)
            }
            GpuiExtensionLaunch::Modal(id) => {
                self.close_titlebar_extension_popup(window, cx);
                self.open_gpui_extension_modal(id, Some(window), cx)
            }
            GpuiExtensionLaunch::TerminalPane(_) => false,
        }
    }

    fn launch_chat_bar_extension(
        &mut self,
        id: ExtensionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        let _ = self.set_active_mode(TitlebarMode::Agents, window, cx);
        if !self.show_agents_session_chat_mode(session_id, cx) {
            return false;
        }
        if !self.deliver_chat_bar_extension_launch(session_id, id, cx) {
            self.schedule_chat_bar_extension_launch(session_id, id, cx);
        }
        true
    }

    fn deliver_chat_bar_extension_launch(
        &mut self,
        session_id: TerminalSessionId,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return false;
        };
        let payload = serde_json::json!({
            "type": "ghostexChatBarPanelShow",
            "extensionId": id.as_str(),
        });
        let script = format!(
            "(function(){{const payload={payload};let attempts=0;const send=()=>{{const ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatExtensionRequested==='function'){{ns.onSessionChatExtensionRequested(payload);return;}}if(++attempts<250){{setTimeout(send,20);}}}};send();}})(); undefined;"
        );
        surface.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    fn schedule_chat_bar_extension_launch(
        &mut self,
        session_id: TerminalSessionId,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            for _ in 0..100 {
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
                match this.update(cx, |this, cx| {
                    this.deliver_chat_bar_extension_launch(session_id, id, cx)
                }) {
                    Ok(false) => {}
                    _ => return,
                }
            }
        })
        .detach();
    }

    pub(crate) fn update_extension_pin(
        &mut self,
        extension_id: String,
        pinned: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = serde_json::json!({
            "id": extension_id,
            "patch": { "pinned": pinned },
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { extension_rpc_result("/api/updateExtensionState", &params) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if result.is_ok() {
                    this.refresh_extensions_in_background(cx);
                } else {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Extensions",
                        "The extension pin state could not be saved.",
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn set_titlebar_extension_popup_open(
        &mut self,
        id: ExtensionId,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self
            .titlebar_extension_popup
            .as_ref()
            .is_some_and(|state| state.id == id)
        {
            self.close_titlebar_extension_popup(window, cx);
            return true;
        }
        let Some(extension) = self
            .extensions_snapshot
            .installed
            .get(id.as_str())
            .filter(|extension| {
                extension.enabled && extension.placement == Some(GpuiExtensionPlacement::Popup)
            })
            .cloned()
        else {
            return false;
        };
        if self.titlebar_tips_panel_open {
            self.set_gpui_titlebar_tips_panel_open(false, window, cx);
        }
        if self.titlebar_resources_panel_open {
            self.set_gpui_titlebar_resources_panel_open(false, window, cx);
        }
        self.close_titlebar_extension_popup(window, cx);
        self.titlebar_extension_popup_generation =
            self.titlebar_extension_popup_generation.wrapping_add(1);
        let generation = self.titlebar_extension_popup_generation;
        let size = extension.popup_size.unwrap_or(GpuiExtensionPopupSize {
            width: TITLEBAR_EXTENSION_POPUP_DEFAULT_WIDTH,
            height: TITLEBAR_EXTENSION_POPUP_DEFAULT_HEIGHT,
        });
        self.titlebar_dropdown_previous_focus_handle = window.focused(cx);
        self.titlebar_dropdown_focus_handle.focus(window, cx);
        self.titlebar_extension_popup = Some(GpuiTitlebarExtensionPopupState {
            id,
            trigger_bounds,
            size,
            generation,
            panel: None,
            error: None,
        });
        cx.notify();

        if let Some(url) = extension.runtime_url.clone() {
            self.schedule_titlebar_extension_panel_creation(generation, id, url, cx);
        } else {
            self.start_titlebar_extension_runtime(generation, id, cx);
        }
        true
    }

    pub(crate) fn close_titlebar_extension_popup(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.titlebar_extension_popup_generation =
            self.titlebar_extension_popup_generation.wrapping_add(1);
        if let Some(mut state) = self.titlebar_extension_popup.take()
            && let Some(panel) = state.panel.take()
        {
            panel.update(cx, |panel, cx| panel.set_visible(false, cx));
        }
        if self
            .titlebar_dropdown_focus_handle
            .contains_focused(window, cx)
            && let Some(previous) = self.titlebar_dropdown_previous_focus_handle.take()
        {
            previous.focus(window, cx);
        } else {
            self.titlebar_dropdown_previous_focus_handle = None;
        }
        cx.notify();
    }

    fn start_titlebar_extension_runtime(
        &mut self,
        generation: u64,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = serde_json::json!({
            "id": id.as_str(),
            "context": self.extension_launch_context_value(),
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/startExtension",
                        &params,
                        Duration::from_secs(65),
                    )
                })
                .await;
            let mut ready_url = None;
            let error = match result {
                Ok(result) => {
                    let status = result.get("status").and_then(serde_json::Value::as_object);
                    match status
                        .and_then(|status| status.get("state"))
                        .and_then(serde_json::Value::as_str)
                    {
                        Some("ready") => {
                            ready_url = status
                                .and_then(|status| status.get("url"))
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string);
                            ready_url
                                .is_none()
                                .then(|| "The extension did not provide a runtime URL.".to_string())
                        }
                        Some("failed") => Some(
                            status
                                .and_then(|status| status.get("error"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("The extension failed to launch.")
                                .to_string(),
                        ),
                        _ => Some("The extension did not reach its ready state.".to_string()),
                    }
                }
                Err(error) => Some(error),
            };
            let _ = this.update(cx, |this, cx| {
                let current = this
                    .titlebar_extension_popup
                    .as_ref()
                    .is_some_and(|state| state.generation == generation && state.id == id);
                if !current {
                    return;
                }
                if let Some(error) = error {
                    if let Some(state) = this.titlebar_extension_popup.as_mut() {
                        state.error = Some(error);
                    }
                    cx.notify();
                    return;
                }
                if let Some(url) = ready_url {
                    this.schedule_titlebar_extension_panel_creation(generation, id, url, cx);
                }
                this.refresh_extensions_in_background(cx);
            });
        })
        .detach();
    }

    fn titlebar_extension_bridge_event_handler(
        &self,
        generation: u64,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) -> cef::ExtensionBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |event: cef::ExtensionBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            let foreground = foreground.clone();
            foreground
                .clone()
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                        let response_app = cx.entity().downgrade();
                        let response_async_cx = cx.to_async();
                        let response_foreground = cx.foreground_executor().clone();
                        let responder: GpuiExtensionBridgeResponder = Rc::new(move |payload| {
                            let response_app = response_app.clone();
                            let mut response_async_cx = response_async_cx.clone();
                            response_foreground
                                .spawn(async move {
                                    let _ = response_app.update_in(
                                        &mut response_async_cx,
                                        |this, _window, cx| {
                                            let Some(panel) = this
                                                .titlebar_extension_popup
                                                .as_ref()
                                                .filter(|state| {
                                                    state.generation == generation && state.id == id
                                                })
                                                .and_then(|state| state.panel.clone())
                                            else {
                                                return;
                                            };
                                            panel.update(cx, |panel, cx| {
                                                panel.dispatch_bridge_message(&payload, cx);
                                            });
                                        },
                                    );
                                })
                                .detach();
                        });
                        let close_app = cx.entity().downgrade();
                        let close_async_cx = cx.to_async();
                        let close_foreground = cx.foreground_executor().clone();
                        let close_handler: GpuiExtensionCloseHandler = Rc::new(move || {
                            let close_app = close_app.clone();
                            let mut close_async_cx = close_async_cx.clone();
                            close_foreground
                                .spawn(async move {
                                    let _ = close_app.update_in(
                                        &mut close_async_cx,
                                        |this, window, cx| {
                                            let current = this
                                                .titlebar_extension_popup
                                                .as_ref()
                                                .is_some_and(|state| {
                                                    state.generation == generation && state.id == id
                                                });
                                            if current {
                                                this.close_titlebar_extension_popup(window, cx);
                                            }
                                        },
                                    );
                                })
                                .detach();
                        });
                        this.handle_extension_bridge_event(
                            event,
                            this.extension_surface_context(GpuiExtensionPlacement::Popup),
                            responder,
                            Some(close_handler),
                            cx,
                        );
                    });
                })
                .detach();
        })
    }

    fn schedule_titlebar_extension_panel_creation(
        &mut self,
        generation: u64,
        id: ExtensionId,
        url: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(extension) = self.extensions_snapshot.installed.get(id.as_str()) else {
            return;
        };
        let Some(bridge_surface) = extension.bridge_surface_spec_for_url(&url) else {
            if let Some(state) = self
                .titlebar_extension_popup
                .as_mut()
                .filter(|state| state.generation == generation && state.id == id)
            {
                state.error = Some("The extension returned an invalid runtime URL.".to_string());
                cx.notify();
            }
            return;
        };
        let parent_ns_view = self.parent_ns_view;
        let event_handler = self.titlebar_extension_bridge_event_handler(generation, id, cx);
        let app = cx.entity().downgrade();
        let foreground = cx.foreground_executor().clone();
        let mut async_cx = cx.to_async();
        foreground
            .spawn(async move {
                let result = GpuiTitlebarExtensionPanel::create_browser(
                    parent_ns_view,
                    id,
                    &url,
                    bridge_surface,
                    event_handler,
                );
                let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                    this.attach_titlebar_extension_panel(generation, id, result, cx);
                });
            })
            .detach();
    }

    fn attach_titlebar_extension_panel(
        &mut self,
        generation: u64,
        id: ExtensionId,
        result: Result<Rc<CefBrowser>, String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = self
            .titlebar_extension_popup
            .as_mut()
            .filter(|state| state.generation == generation && state.id == id)
        else {
            if let Ok(browser) = result {
                browser.set_visible(false);
            }
            return;
        };
        match result {
            Ok(browser) => {
                let panel = GpuiTitlebarExtensionPanel::from_browser(id, browser, cx);
                panel.update(cx, |panel, cx| panel.set_visible(true, cx));
                state.panel = Some(panel);
            }
            Err(error) => {
                state.error = Some(format!("The extension panel could not be created: {error}"));
            }
        }
        cx.notify();
    }
}
