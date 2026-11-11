//! Opening a session's links list from the chat's Links pill (native_chat/chat_pills.rs).
//!
//! CDXC:SessionLinks 2026-10-28 SEE-ALSO:
//! The pill replaced the work area header's link chips after the session's title (the user's words are on `SessionLinks::chat_pill` in packages/gx-core/src/sidebar_view/links.rs). The list itself is apps/desktop/src/app/native_sidebar/links_list.rs.

use gpui::{Bounds, Context, Pixels};

use crate::*;
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    /// Opens the session's links list under `anchor`.
    pub(crate) fn open_session_links_list(
        &mut self,
        key: SessionKey,
        anchor: Bounds<Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let reopen = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::SessionLinks)
            && self.native_sidebar.links_list_session.as_ref() == Some(&key);
        if reopen {
            self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::SessionLinks,
                false,
                None,
                window,
                cx,
            );
            return;
        }
        if self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::SessionLinks) {
            self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::SessionLinks,
                false,
                None,
                window,
                cx,
            );
        }
        self.want_session_links_now(&key, cx);
        self.native_sidebar.links_list_session = Some(key);
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::SessionLinks,
            true,
            Some(anchor),
            window,
            cx,
        );
    }

    /// Re-places the open links dropdown for the list's measured `height`, keeping its bottom edge
    /// where it is: the list hangs above the Links pill, so when a suggestion goes or a group
    /// folds its top comes down, and when one arrives its top goes up (the room above the pill
    /// caps it, `titlebar_popup_window_bounds_for_trigger_bounds`). False when the window cannot
    /// be moved here, and the caller resizes it where it stands instead.
    ///
    /// CDXC:SessionLinks 2026-11-11 DECISION:
    /// User: "when i remove all suggested links or collapse etc then we should bring the window
    /// down not up towards the top (should be anchored to the bottom of it)". The list used to
    /// `Window::resize` itself, which AppKit does around the window's top-left corner, so a list
    /// that shrank drifted up and away from its pill and the chat box. It now takes the same
    /// frame it opened at, for its new height, the way the usage popup does
    /// (`fit_account_usage_popup`).
    pub(crate) fn fit_session_links_popup(&mut self, height: f32, cx: &mut Context<Self>) -> bool {
        let kind = GpuiTitlebarPopupKind::SessionLinks;
        let (Some(state), Some(popup), Some(main)) = (
            self.titlebar_popup_menu
                .as_ref()
                .filter(|state| state.kind == kind),
            self.titlebar_popup_window,
            self.main_window_handle,
        ) else {
            return false;
        };
        let trigger = state.trigger_bounds;
        let Ok((frame, parent)) = cx.update_window(main, |_, window, _| {
            let screen = titlebar_popup_window_bounds_for_trigger_bounds(
                kind,
                trigger,
                height,
                titlebar_popup_menu_width(kind),
                window,
            );
            let origin = window.bounds().origin;
            (
                Bounds::new(screen.origin - origin, screen.size),
                crate::app::native_chat::child_window::window_native_view(window),
            )
        }) else {
            return false;
        };
        let current = popup.update(cx, |_, window, _| window.bounds()).ok();
        if current.is_some_and(|current| current.size == frame.size) {
            return true;
        }
        parent.is_some_and(|parent| {
            crate::app::native_chat::child_window::move_child_window(
                popup.into(),
                parent,
                frame,
                cx,
            )
        })
    }
}
