//! A Cloud Box session's ☁ badge on its row: the cloud glyph, a dot in the box's state colour and
//! the box's name, with provider and state in its tooltip. The GPUI web build compiles this file
//! too.
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1015 "in project with badge"): a cloud session sits in its project with a ☁ badge whose
//! dot shows the box state. The dot follows the box chip's colours (D2): blue while it starts,
//! green running, amber idle, grey paused or stopped, red failed.
//!
//! SEE-ALSO: packages/gx-core/src/cloud_boxes.rs (`SessionCloudBoxBadge`),
//! apps/desktop/src/app/gx_store/sidebar_snapshot.rs (`cloudBox` in a row's details).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;

use super::tooltips::SidebarTooltipSpan;
use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::app::helpers::*;

/// The dot's colour for a box state, as the snapshot spells it.
fn state_color(state: &str, appearance: &SidebarAppearance) -> Hsla {
    match state {
        "pending" | "creating" | "booting" => rgb(0x60a5fa).into(),
        "running" => rgb(0x4ade80).into(),
        "idle" => rgb(0xfbbf24).into(),
        "failed" => rgb(0xf87171).into(),
        _ => appearance.muted.opacity(0.7),
    }
}

/// The badge after a Cloud Box session's title.
pub(super) fn cloud_box_badge(
    session: &NativeSidebarSession,
    appearance: &SidebarAppearance,
    tooltip_span: SidebarTooltipSpan,
    tooltips: bool,
) -> Option<AnyElement> {
    let cloud_box = session.details.get("cloudBox")?;
    let state = cloud_box["state"].as_str().unwrap_or_default();
    let name = cloud_box["boxName"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Cloud")
        .to_owned();
    let tooltip = cloud_box["tooltip"].as_str().unwrap_or_default().to_owned();
    let scale = appearance.scale;
    Some(
        div()
            .id(format!("native-session-cloud-box-{}", session.session_id))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(3.0 * scale))
            .max_w(px(96.0 * scale))
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .child(titlebar_svg_icon(
                        "titlebar/cloud.svg",
                        12.0 * scale,
                        appearance.muted,
                    ))
                    .child(
                        div()
                            .absolute()
                            .right(px(-1.5 * scale))
                            .bottom(px(-0.5 * scale))
                            .size(px(5.0 * scale))
                            .rounded_full()
                            .bg(state_color(state, appearance)),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.5 * scale))
                    .text_color(appearance.muted)
                    .child(name),
            )
            .when(tooltips && !tooltip.is_empty(), |badge| {
                badge.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_tooltip(
                            tooltip.clone(),
                            tooltip_span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
            })
            .into_any_element(),
    )
}
