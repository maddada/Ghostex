//! Shared geometry and typography for the Resources and remote sites dropdowns.
//! CDXC:Browser 2026-09-06 DECISION:
//! User: remote dev servers must use the exact Resources dropdown width, sizes, fonts, and style; the Resources implementation supersedes the HTML mockup's proportions.
use crate::*;

/// CDXC:Titlebar 2026-09-22 DECISION:
/// User: the Tips, Resources, Dev servers and Notifications dropdowns get the Ask Ghostex menu's roundness instead of a hard square look, with the layout unchanged, and they take the background colour the sidebar shows instead of the near-black menu fill.
/// The panel matches the menu's 8px corners and cards its 6px rows; small buttons, chips and icon tiles take a slightly smaller radius so they stay proportionate.
/// SEE-ALSO: titlebar/popup_menu_builders.rs `titlebar_popup_menu_with_scroll_behavior`, helpers/titlebar/chrome_colors.rs `titlebar_background`.
pub(crate) const RESOURCE_PANEL_RADIUS: f32 = 8.0;
pub(super) const RESOURCE_CARD_RADIUS: f32 = 6.0;
pub(crate) const RESOURCE_CONTROL_RADIUS: f32 = 5.0;

/// The frame alone paints the panel fill: a square child fill would cover its rounded corners.
/// `titlebar_background` is the sidebar's own flat base, the solid counterpart of the gradient the
/// sidebar paints across its height.
pub(super) fn resource_panel_frame() -> gpui::Div {
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .rounded(px(RESOURCE_PANEL_RADIUS))
        .border_1()
        .border_color(titlebar_popup_menu_border_color())
        .bg(popup_window_panel_surface(titlebar_background()))
        .text_color(chrome_ink())
}

pub(crate) fn resource_header() -> gpui::Div {
    h_flex()
        .relative()
        .h(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
        .flex_shrink_0()
        .items_stretch()
        .border_b_1()
        .border_color(if window_glass_active() {
            resource_glass_washes().hairline
        } else {
            chrome_ink().opacity(0.12)
        })
}

/// Under window glass these panels take the dialogs' washes on the darker panel frost
/// (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`): the cards at the panel wash, their edges and
/// the header's line at the hairline.
pub(crate) fn resource_glass_washes() -> super::native_modal_kit::FrostedDialogWashes {
    super::native_modal_kit::frosted_dialog_washes(chrome_uses_light_appearance())
}

/// A card's edge on these pages: ink at 10%, the dialogs' hairline under glass.
pub(super) fn resource_card_border() -> gpui::Rgba {
    if window_glass_active() {
        resource_glass_washes().hairline
    } else {
        chrome_ink().opacity(0.10)
    }
}

/// Hosted in the view panel these pages can be as narrow as `WORKAREA_VIEW_PANEL_MIN_WIDTH`, where
/// the header's fixed-width buttons leave the title a few dozen pixels; it clips rather than
/// pushing them off the row.
pub(super) fn resource_heading() -> gpui::Div {
    h_flex()
        .min_w_0()
        .flex_1()
        .overflow_hidden()
        .whitespace_nowrap()
        .items_center()
        .gap(px(8.0))
        .pl(px(12.0))
        .text_size(px(14.0))
        .font_weight(FontWeight::BOLD)
        .text_color(chrome_ink().opacity(0.96))
}

pub(super) fn resource_section_heading() -> gpui::Div {
    h_flex()
        .h(px(24.0))
        .items_center()
        .gap(px(6.0))
        .px(px(2.0))
        .text_size(px(11.0))
        .text_color(chrome_ink().opacity(0.62))
}

/// The fill of a card on one of these pages. Ink at 2.5% is a clear step down from the dark
/// surface, but on the white light surface it resolves to #f9f9f9 and the card all but disappears,
/// so light mode carries a stronger alpha for the same amount of separation.
pub(super) fn resource_card_fill() -> gpui::Hsla {
    if window_glass_active() {
        return resource_glass_washes().panel.into();
    }
    chrome_ink()
        .opacity(if chrome_uses_light_appearance() {
            0.05
        } else {
            0.025
        })
        .into()
}

pub(super) fn resource_row_frame() -> gpui::Div {
    v_flex()
        .w_full()
        .overflow_hidden()
        .rounded(px(RESOURCE_CARD_RADIUS))
        .border_1()
        .border_color(resource_card_border())
        .bg(resource_card_fill())
}

pub(super) fn resource_row_content() -> gpui::Div {
    h_flex()
        .min_h(px(44.0))
        .items_center()
        .gap(px(8.0))
        .p(px(8.0))
        .py(px(7.0))
}

pub(super) fn resource_avatar_tile() -> gpui::Div {
    div()
        .flex_shrink_0()
        .flex()
        .size(px(28.0))
        .items_center()
        .justify_center()
        .rounded(px(RESOURCE_CARD_RADIUS))
        .bg(chrome_ink().opacity(0.10))
}

pub(super) fn resource_name_text() -> gpui::Div {
    div()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(13.0))
        .text_color(chrome_ink().opacity(0.94))
}

pub(super) fn resource_detail_text() -> gpui::Div {
    div()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(12.0))
        .text_color(chrome_ink().opacity(0.58))
}

pub(super) fn resource_metric(width: f32) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .w(px(width))
        .h(px(24.0))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(RESOURCE_CONTROL_RADIUS))
        .border_1()
        .border_color(chrome_ink().opacity(0.105))
        .bg(chrome_ink().opacity(0.055))
        .text_size(px(12.0))
        .text_color(chrome_ink().opacity(0.88))
}

pub(super) fn resource_square_button(id: String) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .flex_shrink_0()
        .size(px(22.0))
        .items_center()
        .justify_center()
        .rounded(px(RESOURCE_CONTROL_RADIUS))
        .border_1()
        .border_color(chrome_ink().opacity(0.16))
        .bg(chrome_ink().opacity(0.14))
        .cursor_pointer()
        .hover(|this| this.bg(chrome_ink().opacity(0.20)))
}
