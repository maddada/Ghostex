//! App toasts in the page. The desktop draws them in a bottom-centre popup window because native terminal and CEF views cover its work area; the page is one canvas, so the same toast stack (`window/toast.rs`, the desktop's file) is drawn as an overlay at the bottom centre of the shell.
use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _,
    div, px,
};

use crate::GhostexGpuiApp;
use crate::app::window::notice_modal::{
    GpuiNoticeModalWindow, NOTICE_MODAL_INITIAL_HEIGHT, NOTICE_MODAL_WIDTH, NoticeModalHost,
    NoticeModalItem,
};
use crate::app::window::toast::{
    GPUI_APP_TOAST_BOTTOM_MARGIN, GPUI_APP_TOAST_DEFAULT_DURATION_MS, GPUI_APP_TOAST_MAX_VISIBLE,
    GPUI_APP_TOAST_WINDOW_WIDTH, GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel,
    GpuiAppToastWindow, gpui_app_toast_from_bridge_message, gpui_app_toast_stack_height,
    gpui_app_toast_without_repeats, gpui_normalized_app_toast_description,
};

#[derive(Default)]
pub(crate) struct WebToasts {
    toasts: Vec<GpuiAppToast>,
    view: Option<Entity<GpuiAppToastWindow>>,
    id_counter: u64,
    epoch: u64,
}

impl GhostexGpuiApp {
    /// The desktop sends this to the open app modal's toast layer, or to the app toast window when no modal is open. The page has one toast stack for both.
    pub(crate) fn dispatch_gpui_app_modal_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(level, title, description, cx);
    }

    pub(crate) fn dispatch_gpui_workspace_action_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut Context<Self>,
    ) {
        let toasts = &mut self.web_host.toasts;
        toasts.id_counter = toasts.id_counter.wrapping_add(1);
        let id = format!("gpui-app-toast-{}", toasts.id_counter);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                copy_text: None,
                level: GpuiAppToastLevel::from_raw(Some(level)),
                title: title.to_string(),
                description: (!description.is_empty()).then(|| description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn receive_gpui_app_toast_bridge_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let toasts = &mut self.web_host.toasts;
        toasts.id_counter = toasts.id_counter.wrapping_add(1);
        let generated_id = format!("gpui-app-toast-{}", toasts.id_counter);
        if let Some(toast) = gpui_app_toast_from_bridge_message(message, generated_id) {
            self.upsert_gpui_app_toast(toast, cx);
        }
    }

    /// The desktop's `upsert_gpui_app_toast`: same id replaces in place, at most four shown, auto-dismiss guarded by the epoch.
    pub(crate) fn upsert_gpui_app_toast(
        &mut self,
        mut toast: GpuiAppToast,
        cx: &mut Context<Self>,
    ) {
        toast.description =
            gpui_normalized_app_toast_description(&toast.title, toast.description.as_deref());
        // A toast with a button waits in the centred notice modal instead, as on the desktop
        // (notice_modal_host.rs).
        let Some(toast) = self.app_notice_take(toast, cx) else {
            return;
        };
        // The desktop's repeat rule (CDXC:AppModal 2026-10-24 in `window/toast.rs`).
        let Some(mut toast) = gpui_app_toast_without_repeats(&self.web_host.toasts.toasts, toast)
        else {
            return;
        };
        let toasts = &mut self.web_host.toasts;
        toasts.epoch = toasts.epoch.wrapping_add(1);
        toast.epoch = toasts.epoch;
        let auto_dismiss =
            (!toast.persistent).then(|| (toast.id.clone(), toast.epoch, toast.duration_ms));
        if let Some(existing) = toasts
            .toasts
            .iter_mut()
            .find(|existing| existing.id == toast.id)
        {
            *existing = toast;
        } else {
            toasts.toasts.push(toast);
            while toasts.toasts.len() > GPUI_APP_TOAST_MAX_VISIBLE {
                toasts.toasts.remove(0);
            }
        }
        if let Some((toast_id, epoch, duration_ms)) = auto_dismiss {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(duration_ms))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this
                        .web_host
                        .toasts
                        .toasts
                        .iter()
                        .any(|toast| toast.id == toast_id && toast.epoch == epoch)
                    {
                        this.remove_gpui_app_toast(&toast_id, cx);
                    }
                });
            })
            .detach();
        }
        self.sync_web_toasts(cx);
    }

    /// A notice's button. The page has no Settings, so it says where to go.
    pub(crate) fn run_gpui_app_toast_action(
        &mut self,
        action: &GpuiAppToastAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            GpuiAppToastAction::OpenAccounts { .. } => self.dispatch_gpui_workspace_action_toast(
                "info",
                "Not in the browser",
                "Accounts are in the Ghostex app's Settings > Accounts.",
                cx,
            ),
            GpuiAppToastAction::OpenViewSettings => self.dispatch_gpui_workspace_action_toast(
                "info",
                "Not in the browser",
                "Views are turned on in the Ghostex app's Settings > Extensions.",
                cx,
            ),
            GpuiAppToastAction::NativeProjectPathAction { payload, .. } => {
                self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
            }
            // Raised only by the app's terminal extensions, which a page does not open.
            GpuiAppToastAction::RunInNewTerminal { .. } => {}
            // Raised only by the Linux AppImage updater, which a page does not run.
            GpuiAppToastAction::OpenUpdateDownloadPage => {}
            // Raised only by the app's settings sync notes (gx_store/settings_sync.rs), which a page
            // does not get.
            GpuiAppToastAction::UndoSyncedChange { .. } => {}
            // Raised only by Chief's Disable Extension, which a page does not offer
            // (native_sidebar/chief.rs).
            GpuiAppToastAction::UndoChiefDisable => {}
            // Raised only by the Linux app at launch (display_scale_offer.rs); a page's size
            // follows the browser.
            GpuiAppToastAction::ResetInterfaceScale => {}
            // Raised only by the app's approval cards (app/owner_security/ui.rs); approving needs
            // the computer's own key store and presence prompt, which a page has not.
            GpuiAppToastAction::SecurityApproval { .. } => {}
        }
    }

    pub(crate) fn remove_gpui_app_toast(&mut self, toast_id: &str, cx: &mut Context<Self>) {
        self.app_notice_remove(toast_id, cx);
        let before = self.web_host.toasts.toasts.len();
        self.web_host
            .toasts
            .toasts
            .retain(|toast| toast.id != toast_id);
        if self.web_host.toasts.toasts.len() != before {
            self.sync_web_toasts(cx);
        }
    }

    fn sync_web_toasts(&mut self, cx: &mut Context<Self>) {
        // A toast given a button after it was raised moves to the notice modal (the desktop's
        // `sync_gpui_app_toast_window`).
        if self
            .web_host
            .toasts
            .toasts
            .iter()
            .any(GpuiAppToast::has_buttons)
        {
            let (notices, toasts) = std::mem::take(&mut self.web_host.toasts.toasts)
                .into_iter()
                .partition(GpuiAppToast::has_buttons);
            self.web_host.toasts.toasts = toasts;
            for notice in notices {
                self.app_notice_insert(notice, cx);
            }
        }
        let toasts = self.web_host.toasts.toasts.clone();
        match &self.web_host.toasts.view {
            Some(view) => view.update(cx, |view, cx| view.set_toasts(toasts, cx)),
            None if !toasts.is_empty() => {
                let app = cx.weak_entity();
                self.web_host.toasts.view = Some(cx.new(|_| GpuiAppToastWindow {
                    app,
                    toasts,
                    hovered_toast_id: None,
                    blur_region: Vec::new(),
                }));
            }
            None => {}
        }
        cx.notify();
    }

    /// The toast stack, bottom-centre over the shell; nothing while there is no toast.
    pub(crate) fn render_web_toasts(&self) -> Option<AnyElement> {
        let toasts = &self.web_host.toasts;
        if toasts.toasts.is_empty() {
            return None;
        }
        let view = toasts.view.clone()?;
        Some(
            div()
                .absolute()
                .bottom(px(GPUI_APP_TOAST_BOTTOM_MARGIN))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w(px(GPUI_APP_TOAST_WINDOW_WIDTH))
                        .h(px(gpui_app_toast_stack_height(&toasts.toasts)))
                        .child(view),
                )
                .into_any_element(),
        )
    }

    /// The notice modal as an overlay window centred on the page (the desktop's
    /// app/notice_modal_window.rs), separate from the page's one dialog slot so it never closes an
    /// open dialog.
    pub(crate) fn open_app_notice_window(
        &mut self,
        palette: crate::app::window::ModalPalette,
        host: NoticeModalHost,
        items: Vec<NoticeModalItem>,
        cx: &mut Context<Self>,
    ) -> Option<(
        gpui::WindowHandle<gpui_component::Root>,
        Entity<GpuiNoticeModalWindow>,
    )> {
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
        let bounds = gpui::Bounds::centered_at(
            gpui::point(px(page.0 / 2.0), px(page.1 / 2.0)),
            gpui::size(px(NOTICE_MODAL_WIDTH), px(NOTICE_MODAL_INITIAL_HEIGHT)),
        );
        let options = gpui::WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
            focus: true,
            show: true,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            ..Default::default()
        };
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<GpuiNoticeModalWindow>>>> =
            Default::default();
        let out = slot.clone();
        let window_border = palette.window_border();
        let window = cx
            .open_window(options, move |window, cx| {
                crate::app::window::popup_frame::frame_app_modal_window(window, window_border);
                window.activate_window();
                let view =
                    cx.new(|cx| GpuiNoticeModalWindow::new(palette, host, items, window, cx));
                *out.borrow_mut() = Some(view.clone());
                let frame = cx.new(|_| crate::app::window::ModalWindowFrame::new(view, palette));
                cx.new(|cx| {
                    gpui_component::Root::new(frame, window, cx).bg(gpui::transparent_black())
                })
            })
            .ok()?;
        let view = slot.borrow_mut().take()?;
        Some((window, view))
    }

    /// The page gives focus back by itself when the notice window goes.
    pub(crate) fn app_notice_window_closed(&mut self, _cx: &mut Context<Self>) {}
}
