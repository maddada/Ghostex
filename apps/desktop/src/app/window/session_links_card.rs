//! The floating links card: a native child window beside the sidebar, next to the row the pointer
//! rests on, listing the session's links (native_sidebar/links_list.rs). It opens after the
//! tooltip delay (native_sidebar/session_hover.rs), stays while the pointer is on the row or on
//! the card, and closes once it has left both.
//!
//! CDXC:SessionLinks 2026-10-24 WHY:
//! A card drawn inside the main window would sit over the work area, where CEF views can cover it, so it is its own non-activating child window like the header's dropdowns (AGENTS.md, native layout discipline). The web build, which has no child windows, draws the same list in its page (apps/gpui-web/src/app/web_host/session_links.rs).

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    point, px, size,
};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::native_sidebar::links_list::{
    SESSION_LINKS_CARD_WIDTH, SessionLinksList, SessionLinksListHost, folded_session_link_groups,
    session_links_list_height, session_links_max_height,
};
use crate::app::window::GpuiTitlebarPopupKind;
use ghostex_gx_core::SessionKey;

/// The gap between the sidebar's edge and the card.
const CARD_GAP: f32 = 6.0;

pub(crate) struct SessionLinksCardWindow {
    list: Entity<SessionLinksList>,
}

impl Render for SessionLinksCardWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.list.clone())
    }
}

impl GhostexGpuiApp {
    /// Opens the card for a sidebar row (its hover, or its line's "+N").
    pub(crate) fn open_session_links_card(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.defer_in_main_window(cx, move |app, window, cx| {
            app.open_session_links_card_in(session_id, window, cx);
        });
    }

    fn open_session_links_card_in(
        &mut self,
        session_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = SessionKey::parse_sidebar_session_id(&session_id) else {
            return;
        };
        let Some(links) = self.session_links_of(&key) else {
            return;
        };
        self.want_session_links_now(&key, cx);
        let Some(row) = self
            .native_sidebar
            .session_card_bounds
            .get(&session_id)
            .copied()
        else {
            return;
        };
        self.close_session_links_card(cx);

        let main = window.bounds();
        let sidebar = self.native_sidebar.bounds;
        let width = SESSION_LINKS_CARD_WIDTH;
        // Past half the window's height the list scrolls inside the card.
        let half_window = session_links_max_height(main.size.height.as_f32());
        let height = session_links_list_height(
            Some(&links),
            &links.folded_groups_on_open(&folded_session_link_groups()),
        )
        .min(half_window)
        .max(80.0);
        // Beside the sidebar, on the work area's side.
        let sidebar_on_right = sidebar.left().as_f32() > main.size.width.as_f32() / 2.0;
        let left = if sidebar_on_right {
            sidebar.left().as_f32() - CARD_GAP - width
        } else {
            sidebar.right().as_f32() + CARD_GAP
        };
        let top = row
            .top()
            .as_f32()
            .min(main.size.height.as_f32() - 8.0 - height)
            .max(8.0);
        let frame = Bounds {
            origin: point(main.origin.x + px(left), main.origin.y + px(top)),
            size: size(px(width), px(height)),
        };
        let display_id =
            crate::app::window::popup_frame::PopupOwner::of(window, cx).display_for(frame, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(frame)),
            display_id,
            focus: !cfg!(any(target_os = "macos", target_os = "windows")),
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: if window_glass_active() {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Transparent
            },
            ..Default::default()
        };
        // The card grows downwards when a group opens, so it stops at the window's bottom too.
        let max_height = half_window.min(main.size.height.as_f32() - 8.0 - top);
        let main_app = cx.weak_entity();
        let opened = cx.open_window(options, move |card_window, cx| {
            card_window.set_background_corner_radius(px(8.0));
            apply_frosted_menu_blur(card_window);
            crate::app::window::titlebar_popup_chrome::prepare_gpui_titlebar_popup_window_chrome(
                card_window,
            );
            let list = cx.new(|cx| {
                SessionLinksList::new(
                    main_app,
                    key,
                    SessionLinksListHost::Card,
                    Some(links),
                    max_height,
                    cx,
                )
            });
            cx.new(|_| SessionLinksCardWindow { list })
        });
        if let Ok(handle) = opened {
            self.native_sidebar.links_card_session = Some(session_id);
            self.native_sidebar.links_card_hovered = false;
            self.native_sidebar.links_card_window = Some(handle.into());
            self.watch_session_links_card(cx);
        }
    }

    /// The card window reports its own hover (`links_card_hovered`), so nothing else is needed.
    pub(crate) fn session_links_card_holds_pointer(&self, _cx: &mut gpui::App) -> bool {
        false
    }

    /// Closes the card, from anywhere but the card's own window (the hover timers).
    pub(crate) fn close_session_links_card(&mut self, cx: &mut Context<Self>) {
        self.native_sidebar.links_card_session = None;
        self.native_sidebar.links_card_hovered = false;
        if let Some(handle) = self.native_sidebar.links_card_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// A list closing itself: an entry was opened from the header's dropdown or from the card.
    pub(crate) fn close_session_links_host(
        &mut self,
        host: SessionLinksListHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match host {
            SessionLinksListHost::Dropdown => {
                self.clear_gpui_titlebar_popup_from_window(GpuiTitlebarPopupKind::SessionLinks, cx);
            }
            SessionLinksListHost::Card => {
                self.native_sidebar.links_card_session = None;
                self.native_sidebar.links_card_hovered = false;
                self.native_sidebar.links_card_window = None;
            }
        }
        window.remove_window();
    }

    /// The desktop draws the card in its own window, never in the sidebar.
    pub(crate) fn render_session_links_page_card(
        &self,
        _cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        None
    }
}
