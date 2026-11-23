//! Soft count circle badge: a perfect circle for a single digit, widening into a rounded
//! rectangle for two or more digits, matching Ghostex's dark and light themes.
use gpui::{Div, FontWeight, Hsla, ParentElement as _, SharedString, Styled as _, div, px};

/// Standard soft count circle badge (18px tall at 1x scale).
/// Perfect circle for 1 digit (`0`..`9`); widens into a rounded pill for 2+ digits.
pub(crate) fn count_badge(count: impl Into<SharedString>, ink: Hsla, scale: f32) -> Div {
    count_badge_sized(count, ink, 18.0 * scale, 11.0 * scale)
}

/// Soft count circle badge at an explicit diameter and font size.
pub(crate) fn count_badge_sized(
    count: impl Into<SharedString>,
    ink: Hsla,
    size: f32,
    font_size: f32,
) -> Div {
    let text: SharedString = count.into();
    let single_char = text.chars().count() <= 1;
    let light = ink.l < 0.5;
    let bg = Hsla {
        a: if light { 0.08 } else { 0.10 },
        ..ink
    };
    let fg = Hsla {
        a: if light { 0.74 } else { 0.80 },
        ..ink
    };
    let badge = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .h(px(size))
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px(font_size))
        .line_height(px(font_size))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap();
    let badge = if single_char {
        badge.w(px(size))
    } else {
        badge.min_w(px(size)).px(px((size * 0.30).max(4.5)))
    };
    badge.child(text)
}
