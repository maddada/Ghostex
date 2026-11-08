//! The Main Hub card in the browser: the desktop opens it as a child window beside the sidebar
//! (apps/desktop/src/app/window/main_hub_card.rs), which a page does not have, so the page draws
//! the same card in its own canvas, beside the sidebar and level with the row.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, Styled as _, anchored, deferred, div,
    point, px,
};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn open_main_hub_card(&mut self, key: String, top: f32, cx: &mut Context<Self>) {
        self.native_sidebar.main_hub_card_open = Some((key, top));
        cx.notify();
    }

    pub(crate) fn close_main_hub_card(&mut self, cx: &mut Context<Self>) {
        self.native_sidebar.main_hub_card_open = None;
        cx.notify();
    }

    /// The card beside the sidebar, over the work area.
    pub(crate) fn render_main_hub_page_card(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, top) = self.native_sidebar.main_hub_card_open.as_ref()?;
        let sidebar = self.native_sidebar.bounds;
        let origin = point(sidebar.right() + px(6.0), px(*top));
        Some(
            deferred(
                anchored()
                    .position(origin)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div().child(crate::app::native_sidebar::tooltips::main_hub_card(
                            1.0, false, false,
                        )),
                    ),
            )
            .with_priority(30)
            .into_any_element(),
        )
    }
}
