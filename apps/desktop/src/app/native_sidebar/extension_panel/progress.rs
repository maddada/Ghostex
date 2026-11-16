//! The `progress` block: a label, a bar and how far along it is.

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, relative};
use gpui_component::{h_flex, v_flex};

use crate::app::native_sidebar::appearance::SidebarAppearance;

pub(super) fn render_panel_progress(
    value: f64,
    max: f64,
    label: Option<&str>,
    appearance: &SidebarAppearance,
) -> AnyElement {
    let scale = appearance.scale;
    let fraction = if max > 0.0 {
        (value / max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let amount = if max == 100.0 {
        format!("{}%", (fraction * 100.0).round())
    } else {
        format!("{} of {}", trim_number(value), trim_number(max))
    };
    v_flex()
        .w_full()
        .min_w_0()
        .px(px(6.0 * scale))
        .gap(px(5.0 * scale))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap(px(8.0 * scale))
                .text_size(px(12.0 * scale))
                // The panel's label, or how far along it is when it gives none (never both:
                // a label usually says the same).
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .when(label.is_none(), |text| text.text_color(appearance.muted))
                        .child(label.map(str::to_string).unwrap_or(amount)),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(4.0 * scale))
                .rounded_full()
                .bg(appearance.foreground.opacity(0.1))
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction as f32))
                        .rounded_full()
                        .bg(appearance.foreground.opacity(0.55)),
                ),
        )
        .into_any_element()
}

/// `3` rather than `3.0`, and at most two decimals.
fn trim_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{:.2}", value).trim_end_matches('0').to_string()
    }
}
