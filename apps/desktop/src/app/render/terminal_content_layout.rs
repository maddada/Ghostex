use gpui::AnyElement;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::container_query;
use gpui::div;
use gpui::px;

const TERMINAL_NARROW_VIEW_MINIMUM_PANE_WIDTH: f32 = 1070.0;

pub(crate) fn terminal_content_frame(
    content: impl IntoElement + 'static,
    horizontal_padding: f32,
    vertical_padding: f32,
    width_percent: Option<f32>,
) -> AnyElement {
    let content = content.into_any_element();
    container_query(move |size, _window, _cx| {
        let body_width = size.width.as_f32();
        let width_gutter = width_percent
            .filter(|_| body_width > TERMINAL_NARROW_VIEW_MINIMUM_PANE_WIDTH)
            .map(|percent| body_width * (1.0 - percent / 100.0) / 2.0)
            .unwrap_or(0.0);

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
