pub(crate) mod terminal_content_layout;
pub(crate) mod window_drag_region;
/// The measurements and the icon of the desktop's terminal action bar; the bar itself is drawn here until the desktop's file can be compiled here.
#[allow(dead_code)]
pub(crate) mod terminal_agent_action_bar {
    // The desktop keeps these constants private to its file, so what draws with them lives in this module too.
    use gpui::{AnyElement, Context, div, prelude::*, px};
    use gpui_component::h_flex;
    use gpui_component::tooltip::Tooltip;

    use crate::GhostexGpuiApp;
    use crate::app::helpers::*;

    include!(concat!(env!("OUT_DIR"), "/terminal_agent_action_bar.rs"));

    impl GhostexGpuiApp {
        /// The terminal's action bar, with the one action this build has: back to the chat. The desktop mirrors the two surfaces' toggles (the chat composer shows a terminal glyph for "Terminal View", the terminal shows the message bubble for "Chat View"), and so does this.
        pub(crate) fn render_terminal_action_bar(&self, cx: &mut Context<Self>) -> AnyElement {
            let text = titlebar_active_text_color();
            h_flex()
                .h(px(TERMINAL_AGENT_BAR_HEIGHT))
                .w_full()
                .flex_none()
                .px(px(TERMINAL_AGENT_BAR_OUTER_PADDING))
                .justify_center()
                .border_t_1()
                .border_color(titlebar_popup_menu_border_color())
                .child(
                    h_flex()
                        .w_full()
                        .max_w(px(TERMINAL_AGENT_BAR_MAX_CONTENT_WIDTH))
                        .justify_end()
                        .child(
                            div()
                                .id("web-terminal-chat-view")
                                .size(px(TERMINAL_AGENT_BAR_BUTTON_SIZE))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
                                .child(titlebar_svg_icon(
                                    TERMINAL_AGENT_BAR_CHAT_VIEW_ICON,
                                    TERMINAL_AGENT_BAR_ICON_SIZE,
                                    text,
                                ))
                                .tooltip(|window, cx| Tooltip::new("Chat View").build(window, cx))
                                .on_click(
                                    cx.listener(|app, _, _, cx| app.web_show_terminal(false, cx)),
                                ),
                        ),
                )
                .into_any_element()
        }
    }
}
