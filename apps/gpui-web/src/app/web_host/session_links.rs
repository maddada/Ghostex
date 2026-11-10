//! The floating links card in the browser: the desktop opens it as a native child window beside
//! the sidebar (apps/desktop/src/app/window/session_links_card.rs), which a page does not have, so
//! the page draws the same list (native_sidebar/links_list.rs) in its own canvas, beside the row.
//! From the chat's links pill it opens at the pill instead, upward from the chat box, as the
//! desktop's dropdown does (native_chat/links_pill.rs).

use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Point, Styled as _, Window, anchored, deferred, div, point, px,
};

use gpui::prelude::FluentBuilder as _;

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::links_list::{
    SESSION_LINKS_CARD_WIDTH, SESSION_LINKS_LIST_WIDTH, SessionLinksList, SessionLinksListHost,
};
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    pub(crate) fn open_session_links_card(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.web_host.links_card_anchor = None;
        self.open_session_links_card_list(session_id, None, cx);
        self.watch_session_links_card(cx);
    }

    /// The chat's links pill: the card with its bottom left at `anchor` (just above the chat box,
    /// lined up with the pill), until a press outside it or the pill again closes it. It is not
    /// the row's hover card, so it does not close when the pointer leaves.
    pub(crate) fn toggle_session_links_card_at(
        &mut self,
        session_id: String,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.web_host.links_card_anchor.is_some()
            && self.native_sidebar.links_card_session.as_deref() == Some(session_id.as_str())
        {
            self.close_session_links_card(cx);
            return;
        }
        // The press on the pill that closed the card a moment ago is the pill's own toggle.
        if self
            .web_host
            .links_card_closed_at
            .take()
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(400))
        {
            return;
        }
        // Up to half the page, and never taller than the room above the pill.
        let room = (f32::from(anchor.y) - 16.0).max(120.0);
        self.open_session_links_card_list(session_id, Some(room), cx);
        if self.native_sidebar.links_card_list.is_some() {
            self.web_host.links_card_anchor = Some(anchor);
        }
    }

    fn open_session_links_card_list(
        &mut self,
        session_id: String,
        room: Option<f32>,
        cx: &mut Context<Self>,
    ) {
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
        let max_height = (self.native_sidebar.bounds.size.height.as_f32() / 2.0)
            .max(80.0)
            .min(room.unwrap_or(f32::INFINITY));
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
        self.web_host.links_card_anchor = None;
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
        let height = list.read(cx).fitted_height();
        let pill = self.web_host.links_card_anchor;
        // From the pill it is the desktop dropdown's width, which the pill lined it up for.
        let width = if pill.is_some() {
            SESSION_LINKS_LIST_WIDTH
        } else {
            SESSION_LINKS_CARD_WIDTH
        };
        let origin = match pill {
            Some(anchor) => point(anchor.x, (anchor.y - px(height)).max(px(8.0))),
            None => {
                let row = self.native_sidebar.session_card_bounds.get(session_id)?;
                point(self.native_sidebar.bounds.right() + px(6.0), row.top())
            }
        };
        self.native_sidebar
            .links_card_bounds
            .set(Some(gpui::Bounds::new(
                origin,
                gpui::size(px(width), px(height)),
            )));
        Some(
            deferred(
                anchored()
                    .position(origin)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .id("web-session-links-card")
                            .w(px(width))
                            .h(px(height))
                            .when(pill.is_some(), |card| {
                                card.on_mouse_down_out(cx.listener(|app, _, _, cx| {
                                    app.web_host.links_card_closed_at =
                                        Some(web_time::Instant::now());
                                    app.close_session_links_card(cx);
                                }))
                            })
                            .child(list),
                    ),
            )
            .with_priority(30)
            .into_any_element(),
        )
    }
}
