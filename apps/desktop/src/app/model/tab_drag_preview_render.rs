// C1 wave-3 re-cluster: the Render impls for the command/browser dragged-tab preview widgets, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

impl Render for CommandTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let show_status_indicator =
            command_terminal_tab_status_indicator_visible(self.tab_status, false);
        let title_trailing_reserved_width =
            command_terminal_tab_status_title_trailing_reserved_width(self.tab_status);

        drag_ghost_surface(div(), 1.0)
            .relative()
            .flex()
            .h(px(COMMAND_PANE_TAB_BAR_HEIGHT))
            .w(px(COMMAND_PANE_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .pl(px(8.0))
            .pr(px(0.0))
            .text_size(px(COMMAND_PANE_TAB_TITLE_FONT_SIZE))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(command_pane_tab_title_text_color(true, false))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .pr(px(title_trailing_reserved_width))
                    .child(self.title.clone()),
            )
            .when(show_status_indicator, |this| {
                this.child(command_pane_tab_status_indicator_element(
                    "ghostex-gpui-command-tab-drag-preview-status",
                    self.tab_status,
                ))
            })
    }
}

impl Render for BrowserTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        drag_ghost_surface(div(), 1.0)
            .flex()
            .h(px(BROWSER_TAB_BAR_HEIGHT))
            .w(px(BROWSER_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .pl(px(8.0))
            .pr(px(10.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::NORMAL)
            .text_color(browser_tab_text_color(self.state, true))
            .child(browser_tab_icon_element(
                self.profile_id,
                self.chrome_status,
                self.runtime_favicon_image.as_ref(),
                self.runtime_favicon_fetch.as_ref(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .ml(px(5.0))
                    .child(self.title.clone()),
            )
    }
}
