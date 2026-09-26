//! App modals in the page. The desktop opens every app modal through one door (`open_app_modal_from_bridge`): the native GPUI dialogs (and Quick Access) as borderless child windows, the rest in its reusable CEF window. The page has no CEF, so only the native dialogs open here, as the platform's overlay windows, through the desktop's own dialog and lifecycle files; a modal that exists only as a CEF page is answered with a toast that says so.
use gpui::{
    AppContext as _, Bounds, Context, Render, Styled as _, Window, WindowBounds, WindowOptions, px,
    size,
};
use gpui_component::Root;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::GhostexGpuiApp;
use crate::app::model::GpuiAppModalKind;
use crate::app::native_app_modal_lifecycle::NativeAppModal;

/// The dialogs that open in the page: the ones whose actions reach gxserver through the executor files this build compiles. The others need the file system, a folder picker or a CEF page.
fn opens_in_browser(kind: GpuiAppModalKind) -> bool {
    matches!(
        kind,
        GpuiAppModalKind::RenameSession
            | GpuiAppModalKind::SessionNote
            | GpuiAppModalKind::DelayedSend
            | GpuiAppModalKind::AgentHooksRequired
            | GpuiAppModalKind::Worktree
            | GpuiAppModalKind::DeleteWorktree
            | GpuiAppModalKind::RenameWorktree
            | GpuiAppModalKind::CommandPalette
            | GpuiAppModalKind::RecentProjects
            | GpuiAppModalKind::PreviousSessions
            | GpuiAppModalKind::StashedPrompts
    )
}

impl GhostexGpuiApp {
    /// The desktop's door into every app modal, for the dialogs the page can show.
    pub(crate) fn open_app_modal_from_bridge(&mut self, message: Value, cx: &mut Context<Self>) {
        let Some(kind) = message
            .get("modal")
            .and_then(Value::as_str)
            .and_then(GpuiAppModalKind::from_modal_id)
        else {
            return;
        };
        if !opens_in_browser(kind) {
            self.dispatch_gpui_workspace_action_toast(
                "info",
                &format!(
                    "{} is not available in the browser",
                    kind.window_title().trim_start_matches("Ghostex ")
                ),
                "Open this from the Ghostex app.",
                cx,
            );
            return;
        }
        let mut message = message;
        if kind.requires_sidebar_state() {
            message["latestSidebarStateMessage"] =
                serde_json::json!({ "hud": *self.gx_store_sidebar_hud_value() });
        }
        self.web_open_native_app_modal(kind, &message, cx);
    }

    fn web_open_native_app_modal(
        &mut self,
        kind: GpuiAppModalKind,
        message: &Value,
        cx: &mut Context<Self>,
    ) {
        match kind {
            GpuiAppModalKind::RenameSession => self.open_gpui_rename_session_modal(message, cx),
            GpuiAppModalKind::SessionNote => self.open_gpui_session_note_modal(message, cx),
            GpuiAppModalKind::DelayedSend => self.open_gpui_delayed_send_modal(message, cx),
            GpuiAppModalKind::AgentHooksRequired => {
                self.open_gpui_agent_hooks_required_modal(message, cx)
            }
            GpuiAppModalKind::Worktree => self.open_gpui_create_worktree_modal(message, cx),
            GpuiAppModalKind::DeleteWorktree => self.open_gpui_delete_worktree_modal(message, cx),
            GpuiAppModalKind::RenameWorktree => self.open_gpui_rename_worktree_modal(message, cx),
            GpuiAppModalKind::CommandPalette
            | GpuiAppModalKind::RecentProjects
            | GpuiAppModalKind::PreviousSessions
            | GpuiAppModalKind::StashedPrompts => {
                self.open_gpui_quick_access_modal(kind, message, cx)
            }
            _ => {}
        }
    }

    pub(crate) fn close_app_modal_from_bridge(&mut self, cx: &mut Context<Self>) {
        self.remove_native_app_modal_window(cx);
    }

    /// Opens `kind` as an overlay window centred on the page, replacing any open dialog.
    pub(crate) fn open_native_app_modal<V: Render>(
        &mut self,
        kind: GpuiAppModalKind,
        width: f32,
        initial_height: f32,
        build: impl FnOnce(&mut Window, &mut gpui::App) -> gpui::Entity<V>,
        cx: &mut Context<Self>,
    ) {
        self.remove_native_app_modal_window(cx);
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
        let window_size = size(px(width), px(initial_height));
        let bounds =
            Bounds::centered_at(gpui::point(px(page.0 / 2.0), px(page.1 / 2.0)), window_size);
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
        let view_slot: Rc<RefCell<Option<gpui::AnyEntity>>> = Rc::new(RefCell::new(None));
        let view_out = view_slot.clone();
        let window_border = self.gpui_native_modal_palette().window_border();
        let window = cx
            .open_window(options, move |window, cx| {
                crate::app::window::popup_frame::frame_app_modal_window(window, window_border);
                window.activate_window();
                let view = build(window, cx);
                *view_out.borrow_mut() = Some(view.clone().into_any());
                cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
            })
            .ok();
        let view = view_slot.borrow_mut().take();
        self.native_app_modal = match (window, view) {
            (Some(window), Some(view)) => Some(NativeAppModal { kind, window, view }),
            _ => None,
        };
    }

    /// Drops the handle after a dialog removed its own window.
    pub(crate) fn release_native_app_modal_window(
        &mut self,
        kind: GpuiAppModalKind,
        _cx: &mut Context<Self>,
    ) {
        if self.native_app_modal_kind() == Some(kind) {
            self.native_app_modal = None;
        }
    }

    pub(crate) fn remove_native_app_modal_window(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(modal) = self.native_app_modal.take() else {
            return false;
        };
        // Quick Access's controller stops publishing when its window goes, as on the desktop.
        if crate::app::window::quick_access::QuickAccessTabId::from_modal_kind(modal.kind).is_some()
        {
            self.dispatch_gpui_quick_access_command(serde_json::json!({ "type": "closed" }), cx);
        }
        modal
            .window
            .update(cx, |_root, window, _cx| window.remove_window())
            .is_ok()
    }

    /// A host message meant for the open dialog (the Delayed Send agent list, a worktree result).
    pub(crate) fn receive_native_app_modal_message(
        &mut self,
        message: &Value,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.native_app_modal_kind() {
            Some(GpuiAppModalKind::DelayedSend) => {
                self.receive_gpui_delayed_send_modal_message(message, cx)
            }
            Some(GpuiAppModalKind::Worktree) => {
                self.receive_gpui_create_worktree_modal_message(message, cx)
            }
            _ => false,
        }
    }
}
