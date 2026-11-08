//! The notice modal's window on the desktop: a borderless child of the main window, centred on
//! it, framed and frosted like an app modal. It is not the app modal slot (`native_app_modal`):
//! an approval that arrives while Settings is open (adding a remote computer) shows over Settings
//! and leaves it open, and Settings' click-away close does not count a click in it
//! (`popup_dismissal::set_notice_window`).
//!
//! SEE-ALSO: apps/desktop/src/app/notice_modal_host.rs (the queue), apps/desktop/src/app/window/notice_modal.rs
//! (the dialog and its CDXC:AppModal 2026-11-08 DECISION), apps/gpui-web/src/app/web_host/toasts.rs (the page's twin).
use crate::app::helpers::*;
use crate::app::window::notice_modal::{
    GpuiNoticeModalWindow, NOTICE_MODAL_INITIAL_HEIGHT, NOTICE_MODAL_WIDTH, NoticeModalHost,
    NoticeModalItem,
};
use crate::app::window::*;
use crate::*;
use gpui::{Entity, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;

impl GhostexGpuiApp {
    pub(crate) fn open_app_notice_window(
        &mut self,
        palette: ModalPalette,
        host: NoticeModalHost,
        items: Vec<NoticeModalItem>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(WindowHandle<Root>, Entity<GpuiNoticeModalWindow>)> {
        let window_size = size(px(NOTICE_MODAL_WIDTH), px(NOTICE_MODAL_INITIAL_HEIGHT));
        let window_bounds =
            gpui::Bounds::centered_at(self.main_window_bounds.center(), window_size);
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            window_decorations: crate::app::window::popup_frame::child_window_decorations(),
            #[cfg(target_os = "linux")]
            x11_parent: self.main_window_handle,
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: false,
            is_minimizable: false,
            display_id: self
                .main_window_popup_owner()
                .display_for(window_bounds, cx),
            titlebar: None,
            window_background: window_glass_background_appearance(),
            ..Default::default()
        };
        let view_slot: Rc<RefCell<Option<Entity<GpuiNoticeModalWindow>>>> =
            Rc::new(RefCell::new(None));
        let view_out = view_slot.clone();
        let window_border = palette.window_border();
        let main_window_native_view = self.parent_ns_view;
        let window = cx
            .open_window(options, move |window, cx| {
                crate::app::window::popup_frame::frame_app_modal_window(window, window_border);
                apply_frosted_menu_blur(window);
                window.set_window_title(if cfg!(any(target_os = "windows", target_os = "linux")) {
                    "Ghostex"
                } else {
                    ""
                });
                window.activate_window();
                attach_gpui_app_modal_window_to_main_window(window, main_window_native_view);
                // Owned by the main window, never by the modal that was active (CDXC:AppModal
                // 2026-10-06 in native_app_modal_lifecycle.rs).
                #[cfg(target_os = "windows")]
                crate::app::window::own_gpui_popup_window(window, main_window_native_view);
                let view =
                    cx.new(|cx| GpuiNoticeModalWindow::new(palette, host, items, window, cx));
                *view_out.borrow_mut() = Some(view.clone());
                let frame = cx.new(|_| ModalWindowFrame::new(view, palette));
                cx.new(|cx| {
                    Root::new(frame, window, cx)
                        .bordered(false)
                        .bg(gpui::transparent_black())
                })
            })
            .ok()?;
        let view = view_slot.borrow_mut().take()?;
        crate::app::window::popup_dismissal::set_notice_window(Some(window.into()));
        Some((window, view))
    }

    /// The last notice was answered: the app modal it opened over (Settings) gets the keyboard
    /// back.
    pub(crate) fn app_notice_window_closed(&mut self, cx: &mut gpui::Context<Self>) {
        crate::app::window::popup_dismissal::set_notice_window(None);
        if let Some(modal) = self.native_app_modal.as_ref() {
            let _ = modal
                .window
                .update(cx, |_, window, _| window.activate_window());
        }
    }
}
