//! Opening a session's links list from the chat's Links pill (native_chat/chat_pills.rs).
//!
//! CDXC:SessionLinks 2026-10-28 SEE-ALSO:
//! The pill replaced the work area header's link chips after the session's title (the user's words are on `SessionLinks::chat_pill` in packages/gx-core/src/sidebar_view/links.rs). The list itself is apps/desktop/src/app/native_sidebar/links_list.rs.

use gpui::{Bounds, Pixels};

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
}
