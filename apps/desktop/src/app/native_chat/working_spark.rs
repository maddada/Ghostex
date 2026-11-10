use super::appearance::ChatAppearance;
use crate::app::helpers::ThrottledAnimationExt;
use crate::assets::chat_working::{VISUAL, spark_svg_size};
use gpui::{
    AnyElement, IntoElement, ParentElement, Styled, Transformation, div, percentage,
    prelude::FluentBuilder as _, px, rgb, size, svg,
};
use std::time::Duration;

/// CSS ease-in-out is cubic-bezier(.42, 0, .58, 1), not GPUI's quadratic easing.
pub(super) fn css_ease_in_out(progress: f32) -> f32 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..14 {
        let t = (lo + hi) / 2.0;
        let x = 3.0 * (1.0 - t) * (1.0 - t) * t * 0.42 + 3.0 * (1.0 - t) * t * t * 0.58 + t * t * t;
        if x < progress {
            lo = t;
        } else {
            hi = t;
        }
    }
    let t = (lo + hi) / 2.0;
    3.0 * (1.0 - t) * t * t + t * t * t
}

/// The working spark; `glow` adds the soft blue and white layers behind it, drawn up to 96px wide.
pub(super) fn spark(p: &ChatAppearance, reduced_motion: bool, glow: bool) -> AnyElement {
    let container = div()
        .relative()
        .size(px(VISUAL.spark_box * p.scale))
        .flex_shrink_0();
    if reduced_motion {
        return container
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("chat-working/spark")
                    .size(px(spark_svg_size(VISUAL.spark_size) * p.scale))
                    .text_color(p.foreground),
            )
            .into_any_element();
    }
    let p = p.clone();
    container
        .with_throttled_animation(
            "chat-working-spark",
            Duration::from_secs(72),
            move |container, frame| {
                let elapsed = frame * 72_000.0;
                let phase = (elapsed / VISUAL.pulse_ms).fract();
                let pulse = css_ease_in_out(if phase < 0.5 {
                    phase * 2.0
                } else {
                    (1.0 - phase) * 2.0
                });
                let scale = 0.88 + 0.24 * pulse;
                let opacity = 0.6 + 0.4 * pulse;
                let transform =
                    Transformation::rotate(percentage((elapsed / VISUAL.spin_ms).fract()))
                        .with_scaling(size(scale, scale));
                let glow_step = (pulse * 32.0).round() as u8;
                let glow_inset = (VISUAL.spark_box - 96.0) / 2.0 * p.scale;
                let glyph_svg = spark_svg_size(VISUAL.spark_size);
                let glyph_inset = (VISUAL.spark_box - glyph_svg) / 2.0 * p.scale;
                container
                    .when(glow, |container| {
                        container
                            .child(
                                svg()
                                    .absolute()
                                    .left(px(glow_inset))
                                    .top(px(glow_inset))
                                    .size(px(96.0 * p.scale))
                                    .path(format!("chat-working/blue-{glow_step}"))
                                    .text_color(
                                        gpui::Hsla::from(rgb(0xa0beff))
                                            .opacity(0.22 * pulse * opacity),
                                    )
                                    .with_transformation(transform),
                            )
                            .child(
                                svg()
                                    .absolute()
                                    .left(px(glow_inset))
                                    .top(px(glow_inset))
                                    .size(px(96.0 * p.scale))
                                    .path(format!("chat-working/white-{glow_step}"))
                                    .text_color(
                                        gpui::Hsla::from(rgb(0xffffff))
                                            .opacity((0.15 + 0.3 * pulse) * opacity),
                                    )
                                    .with_transformation(transform),
                            )
                    })
                    .child(
                        svg()
                            .absolute()
                            .left(px(glyph_inset))
                            .top(px(glyph_inset))
                            .size(px(glyph_svg * p.scale))
                            .path("chat-working/spark")
                            .text_color(p.foreground.opacity(opacity))
                            .with_transformation(transform),
                    )
            },
        )
        // The pane may draw it over the chat one frame and the chat the next
        // (indicator_overlay.rs).
        .keep_phase()
        .into_any_element()
}
