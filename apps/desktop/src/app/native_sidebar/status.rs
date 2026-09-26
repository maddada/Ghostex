use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use gpui_component::h_flex;

use crate::app::helpers::chrome_palette::chrome_color;
use crate::app::helpers::titlebar_background;

/// CDXC:SessionStatus 2026-09-21 DECISION:
/// User: "I feel we have multiple degrees for the orange color status \"working\" in the sidebar pls unify all of them on this new one you picked", so every working dot, count, and badge in the sidebar uses this one orange (the Spaces badge orange, 20% darker than the old 0xf8ad07 so a white digit stays readable on it).
pub(crate) const WORKING_COLOR: u32 = 0xc68a06;

/// CDXC:SessionStatus 2026-09-19 DECISION:
/// User: "make the working indicator just the orange dot without animation. i dont mind. like the one we have in the SESSIONS header", so a working session shows the same static 8px orange dot the section headers draw.
/// This supersedes the 2026-09-17 decision that brought the animated working spinner back.
/// CDXC:SessionStatus 2026-09-22 DECISION:
/// User: "make shell running just show a #B4B8BF color dot in the sidebar in dark mode and a darker dot in light mode", so a row whose agent still has a background shell or monitor running after its turn draws a grey dot in place of the time; working and attention keep precedence over it. Headers (sections, projects, collections, Spaces, machines) draw the same grey when they have no working session but have a grey-dot session.
pub(crate) fn background_work_color() -> gpui::Rgba {
    chrome_color(0xb4b8bf, 0x6b7078)
}

pub(crate) fn activity_indicator(
    activity: &str,
    has_background_work: bool,
    scale: f32,
) -> Option<AnyElement> {
    let indicator = match activity {
        "working" => div()
            .size(px(8.0 * scale))
            .rounded_full()
            .bg(rgb(WORKING_COLOR))
            .into_any_element(),
        "attention" => div()
            .size(px(7.0 * scale))
            .rounded_full()
            .bg(rgb(0x95d7f6))
            .into_any_element(),
        _ if has_background_work => div()
            .size(px(8.0 * scale))
            .rounded_full()
            .bg(background_work_color())
            .into_any_element(),
        _ => return None,
    };
    Some(
        div()
            .w(px(19.0 * scale))
            .h(px(16.0 * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(indicator)
            .into_any_element(),
    )
}

/// CDXC:Spaces 2026-09-25 DECISION:
/// User: the status dots under a Space, and on a remote machine tab, overlap by half instead of sitting side by side: blue most right and on top, orange in the middle, the leftmost slot (the grey shell-running dot, which never shows with orange) behind them. Each dot keeps its session-card size (8px working, 7px attention) and gets a thin ring in the sidebar colour so the covered edge stays readable. This supersedes the 2026-09-22 side-by-side dots.
pub(crate) fn status_dot_stack(
    working_count: usize,
    attention_count: usize,
    background_work_count: usize,
    scale: f32,
) -> gpui::Div {
    let ring = 1.5 * scale;
    let ring_color = titlebar_background();
    let background = working_count == 0 && background_work_count > 0;
    let dot = |size: f32, color: gpui::Hsla, first: bool| {
        div()
            .size(px(size * scale + 2.0 * ring))
            .flex_shrink_0()
            .rounded_full()
            .border(px(ring))
            .border_color(ring_color)
            .bg(color)
            .when(!first, |dot| dot.ml(px(-(4.0 * scale + 2.0 * ring))))
    };
    h_flex()
        .flex_shrink_0()
        .when(background, |row| {
            row.child(dot(8.0, background_work_color().into(), true))
        })
        .when(working_count > 0, |row| {
            row.child(dot(8.0, rgb(WORKING_COLOR).into(), true))
        })
        .when(attention_count > 0, |row| {
            row.child(dot(
                7.0,
                rgb(0x95d7f6).into(),
                !background && working_count == 0,
            ))
        })
}

pub(crate) fn question_indicator(working: bool, scale: f32) -> AnyElement {
    div()
        .h(px(16.0 * scale))
        .min_w(px(16.0 * scale))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.0 * scale))
        .when(working, |indicator| {
            indicator.child(
                div()
                    .size(px(8.0 * scale))
                    .rounded_full()
                    .bg(rgb(WORKING_COLOR)),
            )
        })
        .child(div().size(px(6.0 * scale)).rounded_full().bg(rgb(0xf472b6)))
        .into_any_element()
}

pub(crate) fn completion_opacity(start: web_time::Instant) -> f32 {
    let progress = (start.elapsed().as_secs_f32() / 3.0).min(1.0);
    let stops = [
        (0.0, 1.0),
        (0.08, 0.9),
        (0.16, 0.58),
        (0.24, 1.0),
        (0.36, 0.9),
        (0.44, 0.58),
        (0.52, 1.0),
        (0.64, 0.9),
        (0.72, 0.58),
        (0.80, 1.0),
        (1.0, 1.0),
    ];
    for pair in stops.windows(2) {
        if progress <= pair[1].0 {
            let t = (progress - pair[0].0) / (pair[1].0 - pair[0].0);
            return pair[0].1 + (pair[1].1 - pair[0].1) * t;
        }
    }
    1.0
}
