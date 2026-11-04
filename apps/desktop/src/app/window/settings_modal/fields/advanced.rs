//! The Advanced fold at the bottom of a page: one clickable card row that opens the page's
//! advanced sections under it (CDXC:Settings 2026-10-29 DECISION in search.rs: the rail's Show
//! Advanced switch is gone; user: "a new section that's collapsed by default at the bottom ...
//! called Advanced"). The sections themselves are ordinary `settings_section`s the page appends
//! after this header while it is open.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::icon;
use super::row::{ROW_MIN_HEIGHT, ROW_PADDING_X, settings_icon};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;

pub(crate) fn advanced_fold_header<V: SettingsPage>(
    p: &SettingsPalette,
    open: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let hover = p.raised_hover;
    h_flex()
        .id("settings-advanced-fold")
        .role(gpui::Role::Button)
        .aria_label("Advanced")
        .aria_expanded(open)
        .accessibility_id("settings-advanced-fold")
        .w_full()
        .min_h(px(ROW_MIN_HEIGHT))
        .px(px(ROW_PADDING_X))
        .gap(px(10.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.raised))
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(hover)))
        .on_press(cx, move |page, window, cx| on_toggle(page, window, cx))
        .child(settings_icon(
            if open {
                icon::CHEVRON_DOWN
            } else {
                icon::CHEVRON_RIGHT
            },
            16.0,
            p.muted,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(14.0))
                .line_height(px(18.9))
                .text_color(hsla(p.foreground))
                .child("Advanced"),
        )
        .into_any_element()
}
