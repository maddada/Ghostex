//! The merged-PR popup in the browser: the desktop opens it as a child window beside the sidebar
//! (apps/desktop/src/app/window/merged_pr_popup.rs), which a page does not have, so the page draws
//! the same card in its own canvas, beside the sidebar and level with the row, and a press outside
//! it closes it.
//!
//! SEE-ALSO: CDXC:WorkMode 2026-11-25 in native_sidebar/work_chips.rs.

use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, anchored,
    deferred, point, px,
};

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::work_chips::{
    MERGED_PR_POPUP_GAP, MergedAnswerHandler, MergedPopupOpen, merged_pr_popup_card,
};

impl GhostexGpuiApp {
    pub(crate) fn open_merged_pr_popup(&mut self, open: MergedPopupOpen, cx: &mut Context<Self>) {
        self.native_sidebar.merged_popup = Some(open);
        cx.notify();
    }

    /// The page has no popup window to close.
    pub(crate) fn close_merged_pr_popup_window(&mut self, _cx: &mut Context<Self>) {}

    /// The card beside the sidebar, over the work area.
    pub(crate) fn render_merged_pr_page_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.native_sidebar.merged_popup.as_ref()?;
        let sidebar = self.native_sidebar.bounds;
        let origin = point(sidebar.right() + px(MERGED_PR_POPUP_GAP), px(open.top));
        let app = cx.weak_entity();
        let offer = open.offer.clone();
        let on_answer: MergedAnswerHandler = Rc::new(move |answer, _window, cx| {
            let offer = offer.clone();
            let _ = app.update(cx, |app, cx| app.answer_merged_pr_popup(&offer, answer, cx));
        });
        let card = merged_pr_popup_card(&open.offer, false, false, on_answer)
            .occlude()
            .on_mouse_down_out(cx.listener(|app, _, _, cx| app.close_merged_pr_popup(cx)));
        Some(
            deferred(
                anchored()
                    .position(origin)
                    .snap_to_window_with_margin(px(8.0))
                    .child(card),
            )
            .with_priority(30)
            .into_any_element(),
        )
    }
}
