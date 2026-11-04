use super::super::resources_style::{
    RESOURCE_CARD_RADIUS, RESOURCE_CONTROL_RADIUS, resource_card_border, resource_card_fill,
    resource_glass_washes,
};
pub(super) use super::text::tracked;
use crate::*;
use gpui::{BoxShadow, SharedString};
use std::sync::Arc;

/// CDXC:AgentProviders 2026-10-30 DECISION:
/// User: the usage panel gets "rounded corners and looks closer to" the Notifications panel; they
/// picked mockup A (Q811, the Notifications layout) and Q812 A (provider colour only on the bars,
/// the Live dot and the icon). Every neutral here is the Notifications / Resources ink, card and
/// glass wash (`resources_style.rs`), so the panel follows the theme and window glass the way
/// Notifications does; `accent` (Claude #d97757, Codex #8cbbe8, picked 2026-09-12) is only for the
/// limit bars, the history bars, the Live dot and text, and the logo. This supersedes the
/// 2026-09-12 tinted neutrals, accent glow, gradient bars and decorative background.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub accent: Hsla,
    /// The accent as text: the darker shade on a light panel, where the bar colour is too pale.
    pub accent_text: Hsla,
    /// Section labels and quiet text: the Notifications heading ink.
    pub muted: Hsla,
    /// Fine print and the chart's axis.
    pub dim: Hsla,
    /// Values and labels inside cards.
    pub ink: Hsla,
    /// A card's edge and the hairlines inside it.
    pub line: Hsla,
    /// A bar's empty track.
    pub track: Hsla,
    /// Rows and buttons raised on a card.
    pub raised: Hsla,
    pub codex: bool,
}

impl Palette {
    pub fn new(codex: bool) -> Self {
        let ink = chrome_ink();
        Self {
            accent: rgb(if codex { 0x8cbbe8 } else { 0xd97757 }).into(),
            accent_text: chrome_color(
                if codex { 0x8cbbe8 } else { 0xd97757 },
                if codex { 0x285b8c } else { 0x9c4328 },
            )
            .into(),
            muted: ink.opacity(0.62).into(),
            dim: ink.opacity(0.50).into(),
            ink: ink.opacity(0.94).into(),
            line: resource_card_border().into(),
            track: ink.opacity(0.09).into(),
            raised: if window_glass_active() {
                resource_glass_washes().raised.into()
            } else {
                ink.opacity(0.055).into()
            },
            codex,
        }
    }

    /// The Notifications card: 6px corners, the resource card's thin edge and fill.
    pub fn card(self) -> gpui::Div {
        v_flex()
            .w_full()
            .flex_shrink_0()
            .px(px(CARD_PADDING_X))
            .py(px(CARD_PADDING_Y))
            .rounded(px(RESOURCE_CARD_RADIUS))
            .border_1()
            .border_color(self.line)
            .bg(resource_card_fill())
    }

    /// A small neutral control: the Resources metric chip's edge and fill.
    pub fn control(self, id: impl Into<gpui::ElementId>) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(CONTROL_HEIGHT))
            .px(px(7.0))
            .items_center()
            .gap(px(4.0))
            .rounded(px(RESOURCE_CONTROL_RADIUS))
            .border_1()
            .border_color(chrome_ink().opacity(0.105))
            .bg(chrome_ink().opacity(0.055))
            .text_color(chrome_ink().opacity(0.88))
            .text_size(px(12.0))
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(|this| this.bg(chrome_ink().opacity(0.12)))
    }

    /// A full-width button inside the reset confirmation.
    pub fn outline_button(self, id: &'static str) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id)
            .flex_1()
            .h(px(30.0))
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .rounded(px(RESOURCE_CONTROL_RADIUS))
            .border_1()
            .border_color(chrome_ink().opacity(0.16))
            .text_color(self.ink)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|this| this.bg(chrome_ink().opacity(0.12)))
    }
}

/// Inside a card.
pub(super) const CARD_PADDING_X: f32 = 12.0;
pub(super) const CARD_PADDING_Y: f32 = 10.0;
/// The Rate limit resets control and the reset rows' Use button.
pub(super) const CONTROL_HEIGHT: f32 = 22.0;

pub(super) fn shadow(x: f32, y: f32, blur: f32, color: Hsla) -> BoxShadow {
    BoxShadow::new(px(x), px(y), color).blur_radius(px(blur))
}

/// GPUI's discrete macOS font matching rounds intermediate weights up to the next face.
/// Chromium's system-font headings in the reference use the lower face (650/620 semibold, 550 medium).
pub(super) fn usage_font_weight(css_weight: f32) -> FontWeight {
    FontWeight(if cfg!(target_os = "macos") {
        (css_weight / 100.0).floor() * 100.0
    } else {
        css_weight
    })
}

pub(super) fn label(text: impl Into<SharedString>, size: f32, color: Hsla) -> gpui::Div {
    div()
        .text_size(px(size))
        .line_height(px(size * 1.45))
        .text_color(color)
        .child(text.into())
}

pub(super) fn tabular_numbers() -> gpui::FontFeatures {
    gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

/// The keyboard focus ring, in ink rather than the provider colour.
pub(super) fn focus_ring() -> Vec<BoxShadow> {
    vec![shadow(0.0, 0.0, 0.0, chrome_ink().opacity(0.45).into()).spread_radius(px(2.0))]
}

pub(super) fn chevron(down: bool, up: bool, color: Hsla) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let points = if up {
                [(1.5, 6.0), (5.0, 2.5), (8.5, 6.0)]
            } else if down {
                [(1.5, 3.5), (5.0, 7.0), (8.5, 3.5)]
            } else {
                [(3.5, 1.5), (7.0, 5.0), (3.5, 8.5)]
            };
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (index, (x, y)) in points.into_iter().enumerate() {
                let position = bounds.origin + point(px(x), px(y));
                if index == 0 {
                    path.move_to(position);
                } else {
                    path.line_to(position);
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(10.0))
    .flex_shrink_0()
    .into_any_element()
}

/// The amber of a stale reading, a pace warning and the panel's notices.
pub(super) fn amber() -> Hsla {
    chrome_color(0xf0a94f, 0x986009).into()
}
