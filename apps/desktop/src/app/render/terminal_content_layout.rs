use gpui::AnyElement;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::container_query;
use gpui::div;
use gpui::px;

use crate::shared_settings::TerminalContentWidth;

const TERMINAL_NARROW_VIEW_MINIMUM_PANE_WIDTH: f32 = 1070.0;
const CUSTOM_CONTENT_MINIMUM_HORIZONTAL_GUTTER: f32 = 30.0;

/// The terminal body inside its padding and, when a Terminal width mode applies, centred at that
/// width.
///
/// CDXC:Terminal 2026-11-09 DECISION:
/// Match chat width with a custom Transcript width uses the chat's own cap, at most that percentage of the window, so switching between Chat and Terminal keeps the same width while side panels are open. The user agreed to change it together with the transcript width fix (Q8).
pub(crate) fn terminal_content_frame(
    content: impl IntoElement + 'static,
    horizontal_padding: f32,
    vertical_padding: f32,
    width: Option<TerminalContentWidth>,
) -> AnyElement {
    let content = content.into_any_element();
    container_query(move |size, window, _cx| {
        let body_width = size.width.as_f32();
        let width_gutter = match width {
            Some(TerminalContentWidth::MaxWidth(max_width)) => {
                ((body_width - max_width) / 2.0).max(0.0)
            }
            Some(TerminalContentWidth::WindowPercent(percent)) => {
                let max_width = window.viewport_size().width.as_f32() * percent / 100.0;
                ((body_width - max_width) / 2.0).max(0.0)
            }
            Some(TerminalContentWidth::Percent(percent))
                if body_width > TERMINAL_NARROW_VIEW_MINIMUM_PANE_WIDTH =>
            {
                (body_width * (1.0 - percent / 100.0) / 2.0)
                    .max(CUSTOM_CONTENT_MINIMUM_HORIZONTAL_GUTTER)
            }
            Some(TerminalContentWidth::Percent(_)) | None => 0.0,
        };

        div().relative().size_full().child(
            div()
                .absolute()
                .left(px(width_gutter + horizontal_padding))
                .right(px(width_gutter + horizontal_padding))
                .top(px(vertical_padding))
                .bottom(px(vertical_padding))
                .child(content),
        )
    })
    .into_any_element()
}
