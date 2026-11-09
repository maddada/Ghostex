//! The New Thread picker in the page: the desktop's picker view (`window/new_thread_picker.rs` and
//! its project and Run on rows) in one of the platform's overlay windows, with the page's answers
//! behind its commands. The desktop's lifecycle (preload, recycle, blocking reads) is native-only;
//! the rows it builds from the HUD and the accounts list are lifted from it byte for byte
//! (`extracted-items.txt`), so both pickers list the same agents, accounts and projects.
//!
//! CDXC:WebGpui 2026-11-03 DECISION:
//! User (Q1188 "all"): starting a session in any project on any machine ships to the desktop, the phone and the browser in one change. The page's New session, a project header's "+" and a filter row's "+" open this picker instead of starting a terminal at once. What a page cannot do answers with a toast: a browser tab, Settings, and Cloud Boxes (this computer's boxes are read by the desktop app). The New Thread hotkey (Cmd+N) belongs to the browser, so the project list opens from the picker's top row.
use gpui::{AppContext as _, Bounds, Context, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

use crate::GhostexGpuiApp;
use crate::app::window::*;

#[allow(dead_code, unused_imports)]
mod rows {
    use crate::app::helpers::*;
    use crate::app::titlebar::account_usage::*;
    use crate::app::window::*;
    use crate::*;
    use serde_json::Value;
    include!(concat!(env!("OUT_DIR"), "/new_thread_picker_rows.rs"));
}

/// The picker open over the page.
pub(crate) struct WebNewThreadPicker {
    window: gpui::WindowHandle<Root>,
    view: gpui::Entity<GpuiNewThreadPickerWindow>,
}

impl GhostexGpuiApp {
    /// The page's New session: the picker on the project `gx_store_new_thread_picker_place` says.
    pub(crate) fn open_gpui_new_thread_picker(&mut self, cx: &mut Context<Self>) {
        self.close_web_new_thread_picker(cx);
        let hud_agents = self.new_thread_picker_agents.clone().unwrap_or_default();
        // The last agent picked anywhere leads, as the desktop's picker orders it.
        let primary = crate::app::gx_store::read_primary_agent_launcher_id();
        let agents = rows::order_new_thread_picker_agents(&hud_agents, primary.as_deref());
        // The browser keeps the New Thread hotkey for itself, so the row shows no key cap.
        let place = self.gx_store_new_thread_picker_place().map(|mut place| {
            place.hotkey = None;
            place
        });
        let config = NewThreadPickerConfig {
            palette: self.gpui_native_panel_palette(),
            agents_loaded: self.new_thread_picker_agents.is_some(),
            agents,
            accounts: None,
            boxes: NewThreadPickerBoxes::Unknown,
            default_run_location: "local".to_string(),
            cloud_box: false,
            close_when_inactive: true,
            cloud_default_account: None,
            place,
        };
        let page = web_sys::window()
            .map(|window| {
                let dimension = |value: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
                    value.ok().and_then(|value| value.as_f64()).unwrap_or(800.0) as f32
                };
                (
                    dimension(window.inner_width()),
                    dimension(window.inner_height()),
                )
            })
            .unwrap_or((1280.0, 800.0));
        let height =
            new_thread_picker_window_height(config.agents.len(), config.place.is_some(), false);
        let bounds = Bounds::centered_at(
            gpui::point(px(page.0 / 2.0), px(page.1 / 2.0)),
            size(px(NEW_THREAD_PICKER_WIDTH), px(height)),
        );
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: true,
            show: true,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            ..Default::default()
        };
        let host = self.native_app_modal_host(cx, Self::handle_web_new_thread_picker_command);
        let view_slot: Rc<RefCell<Option<gpui::Entity<GpuiNewThreadPickerWindow>>>> =
            Rc::new(RefCell::new(None));
        let view_out = view_slot.clone();
        let window = cx
            .open_window(options, move |window, cx| {
                window.activate_window();
                let view = cx.new(|cx| GpuiNewThreadPickerWindow::new(config, host, window, cx));
                *view_out.borrow_mut() = Some(view.clone());
                cx.new(|cx| {
                    gpui::Styled::bg(Root::new(view, window, cx), gpui::transparent_black())
                })
            })
            .ok();
        let view = view_slot.borrow_mut().take();
        self.web_host.new_thread_picker = window
            .zip(view)
            .map(|(window, view)| WebNewThreadPicker { window, view });
        self.refresh_web_new_thread_picker_accounts(cx);
    }

    fn close_web_new_thread_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = self.web_host.new_thread_picker.take() {
            let _ = picker
                .window
                .update(cx, |_root, window, _cx| window.remove_window());
        }
    }

    /// The accounts the picker's Claude and Codex rows count and list.
    fn refresh_web_new_thread_picker_accounts(&mut self, cx: &mut Context<Self>) {
        let mut params = json!({ "operation": "list" });
        if let Some(project_id) = self.gpui_app_modal_active_project_id() {
            params["projectId"] = json!(project_id);
        }
        cx.spawn(async move |this, cx| {
            let result = crate::app::gx_store::gx_rpc(None, "/api/agentAccounts", params).await;
            let _ = this.update(cx, |this, cx| {
                let Some(picker) = this.web_host.new_thread_picker.as_ref() else {
                    return;
                };
                let accounts = result
                    .as_ref()
                    .map(rows::new_thread_picker_accounts)
                    .map_err(|error| error.to_string());
                let view = picker.view.clone();
                let _ = picker.window.update(cx, |_root, _window, cx| {
                    view.update(cx, |view, cx| view.set_accounts(accounts, cx));
                });
            });
        })
        .detach();
    }

    fn handle_web_new_thread_picker_command(
        &mut self,
        command: NewThreadPickerCommand,
        cx: &mut Context<Self>,
    ) {
        let unavailable = |app: &mut Self, title: &str, cx: &mut Context<Self>| {
            app.dispatch_gpui_workspace_action_toast(
                "info",
                title,
                "Open this from the Ghostex app.",
                cx,
            );
        };
        match command {
            NewThreadPickerCommand::LaunchAgent {
                agent_id,
                account_id,
                target,
                ..
            } => {
                self.web_host.new_thread_picker = None;
                self.gx_store_take_new_thread_picker_target(target, cx);
                let mut message = json!({ "type": "runSidebarAgent", "agentId": agent_id });
                if let Some(account_id) = account_id {
                    message["accountId"] = json!(account_id);
                }
                if let Some(group_id) = self.gx_store.unified_sidebar.picker_target.take() {
                    message["groupId"] = json!(group_id);
                }
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
            NewThreadPickerCommand::NewCoordinator { target } => {
                self.web_host.new_thread_picker = None;
                self.gx_store_take_new_thread_picker_target(target, cx);
                let group_id = self
                    .gx_store
                    .unified_sidebar
                    .picker_target
                    .take()
                    .or_else(|| self.gx_store_active_group_id());
                // The sidebar's own route, as on the desktop (new_thread_picker_lifecycle.rs).
                if let Some(group_id) = group_id {
                    self.dispatch_native_sidebar_ui(
                        json!({ "type": "projectAction", "action": "coordinator", "groupId": group_id }),
                        cx,
                    );
                }
            }
            NewThreadPickerCommand::CreateTerminal { target } => {
                self.web_host.new_thread_picker = None;
                self.gx_store_take_new_thread_picker_target(target, cx);
                let command = match self.gx_store.unified_sidebar.picker_target.take() {
                    Some(group_id) => json!({"type": "createSessionInGroup", "groupId": group_id}),
                    None => json!({"type": "createSession"}),
                };
                self.dispatch_native_sidebar_command(command, cx);
            }
            NewThreadPickerCommand::OpenBrowser { .. } => {
                self.web_host.new_thread_picker = None;
                unavailable(self, "A browser tab is not available in the browser", cx);
            }
            NewThreadPickerCommand::AddAccount | NewThreadPickerCommand::OpenCloudBoxesSettings => {
                self.web_host.new_thread_picker = None;
                unavailable(self, "Settings is not available in the browser", cx);
            }
            NewThreadPickerCommand::RetryAccounts => {
                self.refresh_web_new_thread_picker_accounts(cx)
            }
            NewThreadPickerCommand::Closed => self.web_host.new_thread_picker = None,
        }
    }
}
