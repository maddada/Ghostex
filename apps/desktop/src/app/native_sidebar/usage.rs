// The account usage strip at the bottom of the sidebar, directly above the
// Commands row. The meters themselves are the shared renderer in
// app/titlebar/account_usage.rs; this module owns only the strip around them.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::{h_flex, v_flex};

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::titlebar::account_usage::{GpuiAccountUsageMeter, GpuiAccountUsageMeterHost};

/// The narrowest a meter reads at: a provider glyph plus two monospace numbers.
/// The strip fits as many of these per row as the current sidebar width allows.
const SIDEBAR_USAGE_METER_MIN_WIDTH: f32 = 58.0;

/// Four meters per row, whatever the sidebar's width allows below that.
const SIDEBAR_USAGE_MAX_COLUMNS: usize = 4;

/// The space between two cards, and between two rows of them.
const SIDEBAR_USAGE_GAP: f32 = 4.0;

/// The strip's own inset. The cards fill the width left between these two edges,
/// so this is also the gap the user sees before the first card and after the last.
const SIDEBAR_USAGE_INSET: f32 = 8.0;

/// An account this close to its limit lights the Commands row's toggle while the
/// strip is hidden, so putting the strip away never puts the warning away.
const SIDEBAR_USAGE_ALERT_PRESSURE: f64 = 0.9;

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-20 DECISION:
    /// User: the account usage meters sit at the bottom of the sidebar, above the Commands row, and are hidden by default. The chart button in the Commands row, immediately left of the Settings gear, shows every account at once, four per row, and clicking it again hides them; that button is the only toggle. This supersedes the earlier rule that the strip started as a single collapsed row of the accounts closest to their limit and was its own toggle: with the strip hidden by default there is nothing to collapse, and a strip that is asked for shows everything it has.
    ///
    /// CDXC:Sidebar 2026-09-20 DECISION:
    /// User: each meter is a card with its own background, the cards fill their column so the rows line up, and their content is centred, which is the `space-around` look the user asked for: the gap from the sidebar's edge to the first card's content matches the gap from the last card's content to the other edge. The strip also sits lower, with more room between the session list and the first row of cards.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// The strip itself takes no click any more. A press on a meter still opens that account's usage popup, and the strip's padding and its leftover columns are inert, so the only way in and out is the Commands row button.
    pub(crate) fn render_native_sidebar_usage(
        &self,
        appearance: &SidebarAppearance,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sidebar_usage_visible {
            return None;
        }
        let meters = self.account_usage_meters();
        if meters.is_empty() {
            return None;
        }
        let scale = appearance.scale;
        let gap = SIDEBAR_USAGE_GAP * scale;
        // Every column but the last carries a gap, so the row fits one more card than
        // the plain division would allow.
        let usable_width = (self.sidebar_width - 2.0 * SIDEBAR_USAGE_INSET * scale + gap).max(0.0);
        let columns = ((usable_width / (SIDEBAR_USAGE_METER_MIN_WIDTH * scale + gap)).floor()
            as usize)
            .clamp(1, SIDEBAR_USAGE_MAX_COLUMNS);

        /*
        CDXC:Sidebar 2026-09-20 WHY:
        Resting, hovered and popup-open have to stay three separable fills now that resting is a
        filled card. The old neutral hover (grey at 0.12) landed within a point of the card on the
        sidebar's dark fill and simply vanished, so each state is a step of the same ink instead.
        */
        let (card, card_hover, card_open) = if appearance.light {
            (
                gpui::rgb(0x000000).opacity(0.05),
                gpui::rgb(0x000000).opacity(0.10),
                gpui::rgb(0x000000).opacity(0.17),
            )
        } else {
            (
                gpui::rgb(0xffffff).opacity(0.05),
                gpui::rgb(0xffffff).opacity(0.11),
                gpui::rgb(0xffffff).opacity(0.20),
            )
        };
        let host = GpuiAccountUsageMeterHost {
            element_id_prefix: "native-sidebar-usage-meter",
            anchor_key_prefix: "native-sidebar-usage-meter-anchor",
            height: 26.0 * scale,
            padding_x: 4.0 * scale,
            corner_radius: 7.0 * scale,
            background: card.into(),
            fill_width: true,
            hover_background: card_hover.into(),
            // Brighter than the hovered card, so the account whose popup is open
            // stays distinct from the one merely under the pointer.
            open_background: card_open.into(),
            tooltip_placement: ManagedTooltipPlacement::Right,
            tooltip_delay: appearance.tooltip_delay,
            scale,
        };

        let rows = meters
            .chunks(columns)
            .map(|row| self.render_native_sidebar_usage_row(row, columns, &host, scale, window, cx))
            .collect::<Vec<_>>();

        Some(
            div()
                .w_full()
                .flex_shrink_0()
                .px(px(SIDEBAR_USAGE_INSET * scale))
                .pt(px(10.0 * scale))
                .pb(px(4.0 * scale))
                .child(
                    v_flex()
                        .w_full()
                        .gap(px(SIDEBAR_USAGE_GAP * scale))
                        .cursor_default()
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    fn render_native_sidebar_usage_row(
        &self,
        row: &[GpuiAccountUsageMeter],
        columns: usize,
        host: &GpuiAccountUsageMeterHost,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .gap(px(SIDEBAR_USAGE_GAP * scale))
            .children(row.iter().map(|meter| {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .child(self.render_account_usage_meter(meter, host, window, cx))
            }))
            // The last row keeps its empty columns so the cards above them stay aligned.
            .children((row.len()..columns).map(|_| div().flex_1().min_w_0().h(px(host.height))))
            .into_any_element()
    }

    /// The Commands row's account-usage button: the only way the strip is shown
    /// or hidden. It carries a dot while the strip is hidden and an account is
    /// close to a limit, so hiding the meters never hides the warning.
    pub(crate) fn render_native_sidebar_usage_toggle(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        // One pass over the accounts per frame: the button exists only once an
        // account is starred, and the same list decides whether it is alerting.
        let meters = self.account_usage_meters();
        if meters.is_empty() {
            return None;
        }
        let scale = appearance.scale;
        let visible = self.sidebar_usage_visible;
        let alerting = !visible
            && meters
                .iter()
                .any(|meter| meter.pressure >= SIDEBAR_USAGE_ALERT_PRESSURE);
        let tooltip_delay = appearance.tooltip_delay;
        Some(
            div()
                .id("native-sidebar-usage-toggle")
                .relative()
                .h(px(28.0 * scale))
                .w(px(34.0 * scale))
                .mr(px(2.0 * scale))
                .rounded(px(5.0 * scale))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .cursor_default()
                .when(visible, |button| button.bg(appearance.hover))
                .hover(|button| button.bg(appearance.hover))
                .child(titlebar_svg_icon(
                    "titlebar/chart-bar.svg",
                    15.0 * scale,
                    if visible {
                        titlebar_active_text_color()
                    } else {
                        appearance.muted
                    },
                ))
                .when(alerting, |button| {
                    button.child(
                        div()
                            .absolute()
                            .top(px(5.0 * scale))
                            .right(px(6.0 * scale))
                            .size(px(5.0 * scale))
                            .rounded_full()
                            .bg(chrome_color(0xe2a06a, 0xb4642a)),
                    )
                })
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.toggle_native_sidebar_usage_visible(cx);
                }))
                .managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    tooltip_delay,
                    move |window, cx| {
                        titlebar_tooltip(
                            if visible {
                                "Hide account usage"
                            } else {
                                "Account usage"
                            },
                            window,
                            cx,
                        )
                    },
                )
                .into_any_element(),
        )
    }

    pub(crate) fn toggle_native_sidebar_usage_visible(&mut self, cx: &mut gpui::Context<Self>) {
        self.sidebar_usage_visible = !self.sidebar_usage_visible;
        self.persist_shell_layout_state();
        cx.notify();
    }
}
