//! The New session row under the sidebar's top row, in the Project and the Unified sidebar alike
//! (and so in the floating panel of a hidden sidebar, and with bots showing): one wide single
//! button across the row's full width.
//!
//! CDXC:Sidebar 2026-11-24 DECISION:
//! User: remove the ">" arrow segment from the sidebar's New session button. Clicking it (and
//! Cmd+N) creates a Native draft directly in the open session's project (or Main Hub when none is
//! open), selects it under that project's Drafts, and opens its empty chat, with no picker.
//! The button keeps the usage cards' neutral fills (`usage_card_fills`, native_sidebar/usage.rs)
//! and shows its shortcut on hover or keyboard focus so resting width never shifts.
//! SEE-ALSO: native_sidebar/render.rs and selectors.rs (where the row goes), native_sidebar/unified/header.rs, apps/gpui-web/src/app/web_host/new_thread_picker.rs (the web build's `start_new_agent_session`).
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The row's height before the zoom: the top rows' shared button height (navigation.rs). Its side
/// padding is the sidebar's side insets (`native_sidebar_side_insets`).
const ROW_HEIGHT: f32 = super::navigation::SIDEBAR_TOP_ROW_BUTTON_HEIGHT;
/// The button's corner radius and side padding.
const BUTTON_RADIUS: f32 = super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS;
const LABEL_PADDING: f32 = 10.0;
/// The button's least width that still fits its icon, "New session" and the shortcut; below
/// it the shortcut is left off (the tooltip still names it).
const LABEL_WITH_KEY_MIN_WIDTH: f32 = 150.0;

/// The words on the button.
const LABEL: &str = "New session";
/// The button's focus stop and its hover group, which shows the shortcut.
const START_STOP: &str = "new-session:start";
const START_GROUP: &str = "native-sidebar-new-session-start";

/// The width the button gets in a sidebar `sidebar_width` wide with these side insets, at
/// `scale`.
fn button_width(
    sidebar_width: f32,
    insets: super::navigation::SidebarSideInsets,
    scale: f32,
) -> f32 {
    (sidebar_width - insets.left - insets.right) / scale
}

/// The shortcut the button shows: Cmd+N (`openNewThreadPalette` until the picker hotkey is
/// retired, falling back to `createAgentSession`). The browser build owns Cmd+N, so it shows none.
fn start_shortcut() -> Option<String> {
    if cfg!(ghostex_web) {
        return None;
    }
    crate::gpui_configured_hotkey_label("openNewThreadPalette")
        .or_else(|| crate::gpui_configured_hotkey_label("createAgentSession"))
}

impl GhostexGpuiApp {
    /// The New session row: one full-width button.
    pub(in crate::app::native_sidebar) fn render_native_sidebar_new_session_row(
        &self,
        appearance: &SidebarAppearance,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let tooltip_delay = appearance.tooltip_delay;
        let tooltips = self.native_sidebar.pointer_inside;
        let sidebar_width = match f32::from(self.native_sidebar.bounds.size.width) {
            0.0 => self.sidebar_width,
            width => width,
        };
        let insets = self.native_sidebar_side_insets(scale);
        let configured_key = start_shortcut();
        let shortcut = configured_key
            .clone()
            .filter(|_| button_width(sidebar_width, insets, scale) >= LABEL_WITH_KEY_MIN_WIDTH);
        let start_tooltip = match configured_key {
            Some(key) => format!("New session ({key})"),
            None => "New session".to_string(),
        };
        // The usage section's own card fills (usage.rs), so the button is the same neutral surface
        // as those cards in light and dark, with glass on or off.
        let fills = super::usage::usage_card_fills(appearance.light);
        let tooltip = |text: String| {
            move |window: &mut gpui::Window, cx: &mut gpui::App| {
                super::tooltips::sidebar_style_tooltip(text.clone(), window, cx)
            }
        };
        // The shortcut shows only while the pointer is over the button or it has the keyboard; at
        // rest it keeps its room, drawn clear, so the label never moves.
        let stops = &self.native_sidebar.focus_stops;
        let focused = stops.recording()
            && stops
                .handle(&gpui::SharedString::new_static(START_STOP), cx)
                .is_focused(window);
        let start = h_flex()
            .id("native-sidebar-new-session")
            .group(START_GROUP)
            .role(gpui::Role::Button)
            .aria_label(start_tooltip.clone())
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(LABEL_PADDING * scale))
            .gap(px(8.0 * scale))
            .rounded(px(BUTTON_RADIUS * scale))
            .bg(fills.rest)
            .hover(|button| button.bg(fills.hover))
            .active(|button| button.bg(fills.open))
            .child(titlebar_svg_icon(
                "titlebar/plus.svg",
                15.0 * scale,
                appearance.chrome_icon,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(titlebar_project_text_color())
                    .child(LABEL),
            )
            .children(shortcut.map(|key| {
                /*
                CDXC:Sidebar 2026-11-11 WHY:
                The hint shows through its opacity, not its text colour: GPUI shapes text, colour included, while it lays the frame out, before this frame's group hover is known, so a group-hover text colour never reached it and the hint stayed hidden on hover. Opacity is applied when the frame is painted, where the group hover is known, as it is for the button's hover fill.
                */
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * scale))
                    .text_color(appearance.muted)
                    .opacity(if focused { 1.0 } else { 0.0 })
                    .group_hover(START_GROUP, |key| key.opacity(1.0))
                    .child(key)
            }))
            .when(tooltips, |button| {
                button.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowLeft,
                    tooltip_delay,
                    tooltip(start_tooltip),
                )
            })
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.start_new_agent_session(cx);
            }));
        // Focus stop of the sidebar's focus order; the row sits above the list in both sidebars.
        let start = self.sidebar_focus_stop(
            start,
            START_STOP,
            super::focus_order::SidebarStopPlace::Fixed,
            appearance,
            cx,
        );
        h_flex()
            .w_full()
            .flex_shrink_0()
            .pl(px(insets.left))
            .pr(px(insets.right))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h(px(ROW_HEIGHT * scale))
                    .rounded(px(BUTTON_RADIUS * scale))
                    .text_size(px(13.0 * scale))
                    .font_weight(FontWeight::NORMAL)
                    .child(start),
            )
            .into_any_element()
    }
}
