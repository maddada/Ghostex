//! The Automations row pinned at the top of the Bots list, which opens and closes the Bot
//! automations feed.
//!
//! CDXC:Bots 2026-09-26 DECISION:
//! User: an "Automations" row is pinned at the top of the Bots sidebar while Bot automations is on; clicking it opens the feed in the work area and clicking it again closes the feed. It shows how many runs landed today.
//! SEE-ALSO: apps/desktop/src/app/native_bot_feed/host.rs (`toggle_bot_feed_view`, `bot_feed_showing`) and its browser stand-in apps/gpui-web/src/app/web_host/bot_feed.rs; server/src/bot_feed.rs (`bot_runs_today`, the count).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::h_flex;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::consts::TITLEBAR_ICON_MESSAGES;
use crate::app::helpers::*;

impl GhostexGpuiApp {
    /// Laid out like a bot row (project_header.rs) so the two line up, and selected while the feed shows.
    pub(crate) fn render_native_sidebar_automations_row(
        &self,
        runs_today: u64,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let showing = self.bot_feed_showing();
        // A group's margins (render.rs) plus the header row's own inset (project_header.rs).
        h_flex()
            .id("native-sidebar-automations-row")
            .role(gpui::Role::Button)
            .aria_label("Automations")
            .aria_selected(showing)
            .relative()
            .h(px(30.0 * scale))
            .ml(px(21.0 * scale))
            .mr(px(8.0 * scale))
            .mb(px(10.0 * scale))
            .pl(px(5.0 * scale))
            .pr(px(6.0 * scale))
            .gap(px(10.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_default()
            .hover(|row| row.bg(appearance.session_hover))
            .when(showing, |row| {
                row.bg(appearance.selected)
                    .child(super::decorations::selected_outline(appearance))
            })
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_MESSAGES,
                16.0 * scale,
                appearance.muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::LIGHT)
                    .child("Automations"),
            )
            .when(runs_today > 0, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0 * scale))
                        .text_color(appearance.muted)
                        .child(format!("{runs_today} today")),
                )
            })
            .on_click(cx.listener(|app, _, window, cx| {
                cx.stop_propagation();
                app.toggle_bot_feed_view(window, cx);
            }))
            .into_any_element()
    }
}
