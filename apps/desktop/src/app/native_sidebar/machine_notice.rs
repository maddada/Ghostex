//! What a remote machine that is not connected says over its list
//! (`ghostex_gx_core::MachineNotice`): a card docked at the top of the list when it failed or is
//! not connected, a one-line pill near the top while it connects. Both float over the top of the
//! list and never move its rows (the decision in machine_connection.rs).
//!
//! CDXC:RemoteMachines 2026-10-25 WHY: The notice is painted deferred above the sticky project
//! header (priority 5), which otherwise pins over it once the list scrolls. It occludes, so a press
//! on the card or the pill never reaches the row under it; it is a visible element with its own
//! frame and buttons, not a shield.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/machine_connection.rs (the decision and the copy),
//! machines.rs (the tab's spinner and the red cloud).

use super::appearance::SidebarAppearance;
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

const PILL_HEIGHT: f32 = 26.0;

/// The raised fill the card and the pill share, a step off the sidebar's own.
fn notice_fill(appearance: &SidebarAppearance) -> Hsla {
    if appearance.light {
        rgb(0xffffff).into()
    } else {
        titlebar_background().blend(appearance.foreground.opacity(0.07))
    }
}

fn spinning_loader(id: String, size: f32, color: Hsla) -> AnyElement {
    gpui::svg()
        .path("titlebar/loader2.svg")
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
        .with_throttled_animation(
            id,
            std::time::Duration::from_millis(900),
            |icon, progress| {
                icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(progress)))
            },
        )
        .into_any_element()
}

fn notice_button(
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    primary: bool,
    appearance: &SidebarAppearance,
) -> gpui::Stateful<gpui::Div> {
    let scale = appearance.scale;
    let color = if primary {
        appearance.foreground
    } else {
        appearance.muted
    };
    h_flex()
        .id(id)
        .h(px(24.0 * scale))
        .pl(px(7.0 * scale))
        .pr(px(9.0 * scale))
        .gap(px(5.0 * scale))
        .rounded(px(6.0 * scale))
        .text_size(px(12.0 * scale))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .cursor_pointer()
        .when(primary, |button| {
            button
                .bg(appearance.foreground.opacity(0.08))
                .border_1()
                .border_color(appearance.foreground.opacity(0.12))
        })
        .hover(|button| button.bg(appearance.hover))
        .child(titlebar_svg_icon(icon, 12.0 * scale, color))
        .child(label)
}

impl GhostexGpuiApp {
    /// The notice floating over the top of the selected machine's list: the pill while it
    /// connects, the card otherwise.
    pub(crate) fn render_native_sidebar_machine_notice(
        &self,
        notice: &ghostex_gx_core::MachineNotice,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let content = if notice.busy || notice.collapsed {
            self.render_native_sidebar_machine_pill(notice, appearance, cx)
        } else {
            self.render_native_sidebar_machine_card(notice, appearance, cx)
        };
        gpui::deferred(div().absolute().top_0().left_0().right_0().child(content))
            .with_priority(6)
            .into_any_element()
    }

    /// The card of a failed or unconnected machine: why, and the way out.
    fn render_native_sidebar_machine_card(
        &self,
        notice: &ghostex_gx_core::MachineNotice,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let reconnect_id = notice.machine_id.clone();
        let configure_id = notice.machine_id.clone();
        let collapse = json!({"type": "machineNotice", "machineId": notice.machine_id, "collapsedState": notice.state});
        let indent = px(20.0 * scale);
        v_flex()
            .id("native-sidebar-machine-notice")
            .flex_shrink_0()
            .mx(px(8.0 * scale))
            .mt(px(6.0 * scale))
            .mb(px(6.0 * scale))
            .px(px(10.0 * scale))
            .pt(px(8.0 * scale))
            .pb(px(9.0 * scale))
            .rounded(px(9.0 * scale))
            .border_1()
            .border_color(appearance.foreground.opacity(0.12))
            .bg(notice_fill(appearance))
            .shadow_sm()
            .occlude()
            .text_size(px(12.0 * scale))
            .child(
                h_flex()
                    .gap(px(7.0 * scale))
                    .child(titlebar_svg_icon(
                        if notice.failed {
                            "titlebar/alert-triangle.svg"
                        } else {
                            "titlebar/cloud.svg"
                        },
                        13.0 * scale,
                        if notice.failed {
                            rgb(0xff9494).into()
                        } else {
                            appearance.muted
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.5 * scale))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(appearance.foreground)
                            .child(notice.title.clone()),
                    )
                    .child(
                        h_flex()
                            .id("native-sidebar-machine-notice-close")
                            .flex_shrink_0()
                            .size(px(20.0 * scale))
                            .mr(px(-4.0 * scale))
                            .justify_center()
                            .rounded(px(5.0 * scale))
                            .cursor_pointer()
                            .hover(|button| button.bg(appearance.hover))
                            .child(titlebar_svg_icon(
                                "titlebar/x.svg",
                                12.0 * scale,
                                appearance.muted,
                            ))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.dispatch_native_sidebar_ui(collapse.clone(), cx);
                            })),
                    ),
            )
            .when_some(notice.detail.clone(), |card, detail| {
                card.child(
                    div()
                        .pl(indent)
                        .mt(px(1.0 * scale))
                        .text_color(appearance.muted)
                        .child(detail),
                )
            })
            .child(
                h_flex()
                    .mt(px(7.0 * scale))
                    .pl(indent)
                    .gap(px(6.0 * scale))
                    .flex_wrap()
                    // A machine that is this computer is already connected: only Remote Settings.
                    .when(
                        notice.state != ghostex_gx_core::THIS_COMPUTER_NOTICE_STATE,
                        |row| {
                            row.child(
                        notice_button(
                            "native-sidebar-machine-notice-connect",
                            if notice.failed { "Reconnect" } else { "Connect" },
                            "titlebar/refresh.svg",
                            true,
                            appearance,
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.remote_reconnect_from_sidebar(&reconnect_id, cx);
                        })),
                            )
                        },
                    )
                    .child(
                        notice_button(
                            "native-sidebar-machine-notice-settings",
                            "Remote Settings",
                            "titlebar/settings.svg",
                            false,
                            appearance,
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(
                                json!({"type": "machineAction", "action": "configure", "machineId": configure_id}),
                                cx,
                            );
                        })),
                    ),
            )
            .into_any_element()
    }

    /// The one-line pill near the top of the list: a spinner while the machine connects, or the
    /// shrunk failure's red mark and title, which opens the card again on a press.
    fn render_native_sidebar_machine_pill(
        &self,
        notice: &ghostex_gx_core::MachineNotice,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let expand = json!({"type": "machineNotice", "machineId": notice.machine_id});
        let mark = if notice.busy {
            spinning_loader(
                format!("native-machine-notice-busy-{}", notice.machine_id),
                12.0 * scale,
                appearance.muted,
            )
        } else {
            titlebar_svg_icon(
                if notice.failed {
                    "titlebar/alert-triangle.svg"
                } else {
                    "titlebar/cloud.svg"
                },
                12.0 * scale,
                if notice.failed {
                    rgb(0xff9494).into()
                } else {
                    appearance.muted
                },
            )
            .into_any_element()
        };
        h_flex()
            .flex_shrink_0()
            .pt(px(6.0 * scale))
            .pb(px(4.0 * scale))
            .px(px(10.0 * scale))
            .justify_center()
            .child(
                h_flex()
                    .id("native-sidebar-machine-notice-pill")
                    .min_w_0()
                    .h(px(PILL_HEIGHT * scale))
                    .px(px(12.0 * scale))
                    .gap(px(7.0 * scale))
                    .rounded_full()
                    .border_1()
                    .border_color(appearance.foreground.opacity(0.12))
                    .bg(notice_fill(appearance))
                    .shadow_sm()
                    .occlude()
                    .text_size(px(12.0 * scale))
                    .text_color(if notice.busy {
                        appearance.muted
                    } else {
                        appearance.foreground
                    })
                    .when(!notice.busy, |pill| {
                        pill.font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|pill| pill.bg(appearance.hover))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.dispatch_native_sidebar_ui(expand.clone(), cx);
                            }))
                    })
                    .child(mark)
                    .child(div().min_w_0().truncate().child(notice.title.clone())),
            )
            .into_any_element()
    }
}
