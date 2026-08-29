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

pub(crate) fn terminal_content_frame(
    content: impl IntoElement + 'static,
    horizontal_padding: f32,
    vertical_padding: f32,
    width: Option<TerminalContentWidth>,
) -> AnyElement {
    let content = content.into_any_element();
    container_query(move |size, _window, _cx| {
        let body_width = size.width.as_f32();
        let width_gutter = match width {
            Some(TerminalContentWidth::MaxWidth(max_width)) => {
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
