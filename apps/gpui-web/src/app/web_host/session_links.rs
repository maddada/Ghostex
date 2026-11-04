//! The floating links card in the browser: the desktop opens it as a native child window beside
//! the sidebar (apps/desktop/src/app/window/session_links_card.rs), which a page does not have, so
//! the page draws the same list (native_sidebar/links_list.rs) in its own canvas, beside the row.

use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, Styled as _, Window,
    anchored, deferred, div, point, px,
};

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::links_list::{
    SESSION_LINKS_CARD_WIDTH, SessionLinksList, SessionLinksListHost,
};
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    pub(crate) fn open_session_links_card(&mut self, session_id: String, cx: &mut Context<Self>) {
        let Some(key) = SessionKey::parse_sidebar_session_id(&session_id) else {
            return;
        };
        let Some(links) = self.session_links_of(&key) else {
            return;
        };
        self.want_session_links_now(&key, cx);
        let main_app = cx.weak_entity();
        // Past half the page's height the list scrolls inside the card; the sidebar runs the
        // page's full height.
        let max_height = (self.native_sidebar.bounds.size.height.as_f32() / 2.0).max(80.0);
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
        self.native_sidebar.links_card_session = Some(session_id);
        self.native_sidebar.links_card_hovered = false;
        self.native_sidebar.links_card_list = Some(list);
        self.watch_session_links_card(cx);
        cx.notify();
    }

    /// Whether the pointer is on the card the page drew last.
    pub(crate) fn session_links_card_holds_pointer(&self, cx: &mut gpui::App) -> bool {
        let (Some(handle), Some(bounds)) = (
            self.web_host.main_window,
            self.native_sidebar.links_card_bounds.get(),
        ) else {
            return false;
        };
        handle
            .update(cx, |_, window, _| bounds.contains(&window.mouse_position()))
            .unwrap_or(false)
    }

    pub(crate) fn close_session_links_card(&mut self, cx: &mut Context<Self>) {
        self.native_sidebar.links_card_session = None;
        self.native_sidebar.links_card_hovered = false;
        self.native_sidebar.links_card_list = None;
        self.native_sidebar.links_card_bounds.set(None);
        cx.notify();
    }

    /// The page has no header dropdown, so only the card ever closes itself here.
    pub(crate) fn close_session_links_host(
        &mut self,
        _host: SessionLinksListHost,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_session_links_card(cx);
    }

    /// The card beside the row it belongs to, over the work area.
    pub(crate) fn render_session_links_page_card(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let list = self.native_sidebar.links_card_list.clone()?;
        let session_id = self.native_sidebar.links_card_session.as_ref()?;
        let row = self.native_sidebar.session_card_bounds.get(session_id)?;
        let height = list.read(cx).fitted_height();
        let sidebar = self.native_sidebar.bounds;
        let origin = point(sidebar.right() + px(6.0), row.top());
        self.native_sidebar
            .links_card_bounds
            .set(Some(gpui::Bounds::new(
                origin,
                gpui::size(px(SESSION_LINKS_CARD_WIDTH), px(height)),
            )));
        Some(
            deferred(
                anchored()
                    .position(origin)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .w(px(SESSION_LINKS_CARD_WIDTH))
                            .h(px(height))
                            .child(list),
                    ),
            )
            .with_priority(30)
            .into_any_element(),
        )
    }
}
