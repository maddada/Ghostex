//! The page's side of the Beads view (the desktop's `app/native_beads/`, compiled here unchanged):
//! a panel beside the chat that the header's Beads button shows, like Changes. The page has no
//! view panel, so this panel is where the desktop's Beads tab lives here.

use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// The Beads panel, when the header's button has it open.
    pub(crate) fn render_web_beads_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.beads_open {
            self.native_kanban_shown = false;
            return None;
        }
        let view = self.render_beads_view(window, cx).unwrap_or_else(|| {
            div()
                .p(px(16.0))
                .text_color(gpui::rgb(0x8a8a8a))
                .child("Pick a project to see its beads.")
                .into_any_element()
        });
        // The Board's own poll runs only while the Board is what the panel shows.
        self.native_kanban_shown = self.native_beads.as_ref().is_some_and(|view| {
            view.read(cx).layout == crate::app::native_beads::BeadsLayout::Board
        });
        Some(
            div()
                .h_full()
                .w(px(self.beads_width))
                .flex_none()
                .border_l_1()
                .border_color(crate::app::helpers::titlebar_popup_menu_border_color())
                .child(view)
                .into_any_element(),
        )
    }

    pub(crate) fn web_toggle_beads(&mut self, cx: &mut Context<Self>) {
        self.beads_open = !self.beads_open;
        cx.notify();
    }
}
