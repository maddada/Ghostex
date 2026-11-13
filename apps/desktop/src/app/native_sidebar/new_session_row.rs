//! The New session row under the sidebar's top row, in the Project and the Unified sidebar alike
//! (and so in the floating panel of a hidden sidebar, and with bots showing): one wide button in
//! two halves, the row's full width.
//!
//! CDXC:Sidebar 2026-11-10 DECISION:
//! User (Q2340 C, Q2341 C, Q2342 D, Q2343 A, Q2344 "beside", Q2345 A): every sidebar layout has a wide New session button in a row of its own under the top row, with an icon, its words and its shortcut on it (see the 2026-11-11 decision below for which and when). A click starts the agent you used last at once, in the project of the session you have open, as Cmd+Shift+O does (`start_new_agent_session`); the arrow at its end opens the New Thread picker, as Cmd+N does (`open_unified_new_session`). The button replaces the Unified header's pencil. It rests on a faint light fill so it reads as the sidebar's main action, never a dark one on glass.
//! CDXC:Sidebar 2026-11-11 DECISION:
//! User: "i dont like new project shown like button without bg there fam add it to the top row instead please to the right of search". Add project left this row for the top row, right of Search (native_sidebar/navigation.rs), so the row holds only the New session button and its arrow, the row's full width, 28 high like every row at the top. This supersedes Add project sitting beside the button.
//! CDXC:Sidebar 2026-11-11 DECISION:
//! User: "dont show chevron down icon here, instead should be another icon that indicates we will show a menu of agents ... also please dont show the hotkey for new session unless i hover over it", then "also make icon for new session just + icon". The button leads with a plain plus, the arrow half shows a chevron pointing right (the user's pick, "chevron right", over a robot, sparkles or people icon) for the agent picker it opens, and the shortcut shows only while the pointer is over the button or it has the keyboard, keeping its room at rest so nothing moves. This supersedes the pencil-on-a-square, the chevron pointing down and the always-shown shortcut of 2026-11-10.
//! SEE-ALSO: native_sidebar/render.rs and selectors.rs (where the row goes), native_sidebar/unified/header.rs (the header it took the pencil and Add project from), apps/gpui-web/src/app/web_host/new_thread_picker.rs (the web build's `start_new_agent_session`).
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The row's side padding and its height, before the zoom: the top rows' shared edge and button
/// height (navigation.rs).
const ROW_PADDING: f32 = super::navigation::SIDEBAR_TOP_EDGE_INSET;
const ROW_HEIGHT: f32 = super::navigation::SIDEBAR_TOP_ROW_BUTTON_HEIGHT;
/// The button's corner radius, the arrow half's width and the inset of the label half.
const BUTTON_RADIUS: f32 = super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS;
const ARROW_WIDTH: f32 = 29.0;
const LABEL_PADDING: f32 = 10.0;
/// The label half's least width that still fits its icon, "New session" and the shortcut; below
/// it the shortcut is left off (the tooltip still names it).
const LABEL_WITH_KEY_MIN_WIDTH: f32 = 150.0;

/// The words on the button.
const LABEL: &str = "New session";
/// The start half's focus stop and its hover group, which shows the shortcut.
const START_STOP: &str = "new-session:start";
const START_GROUP: &str = "native-sidebar-new-session-start";
/// The arrow half's icon: the sidebar's plain chevron, pointing to the list it opens.
const CHOOSE_ICON: &str = "titlebar/chevron-right.svg";

/// The width the label half gets in a sidebar `sidebar_width` wide, at `scale`.
fn label_half_width(sidebar_width: f32, scale: f32) -> f32 {
    sidebar_width / scale - ROW_PADDING * 2.0 - ARROW_WIDTH
}

/// The shortcut the button shows: the configured New Agent Session key. The browser build has no
/// such key, so it shows none there.
fn start_shortcut() -> Option<String> {
    if cfg!(ghostex_web) {
        return None;
    }
    crate::gpui_configured_hotkey_label("createAgentSession")
}

/// What a hover over the arrow half says, with the New Thread key when one is set.
fn choose_tooltip() -> String {
    let shortcut = if cfg!(ghostex_web) {
        None
    } else {
        crate::gpui_configured_hotkey_label("openNewThreadPalette")
    };
    match shortcut {
        Some(key) => format!("Choose an agent, Coordinator, Browser or Terminal ({key})"),
        None => "Choose an agent, Coordinator, Browser or Terminal".to_string(),
    }
}

impl GhostexGpuiApp {
    /// The New session row: the two-half button.
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
        let shortcut = start_shortcut()
            .filter(|_| label_half_width(sidebar_width, scale) >= LABEL_WITH_KEY_MIN_WIDTH);
        let start_tooltip = match start_shortcut() {
            Some(key) => format!("Start your last agent in the open project ({key})"),
            None => "Start your last agent in the open project".to_string(),
        };
        // A faint light wash on dark glass; on light chrome the raised white tile the sidebar's
        // selected rows use, with its hairline.
        let (fill, outline) = if appearance.light {
            (appearance.selected, Some(appearance.selected_outline))
        } else {
            (rgb(0xffffff).opacity(0.11).into(), None)
        };
        let divider = if appearance.light {
            appearance.selected_outline
        } else {
            rgb(0xffffff).opacity(0.10).into()
        };
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
            .rounded_l(px(BUTTON_RADIUS * scale))
            .hover(|half| half.bg(appearance.hover))
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
                The hint shows through its opacity, not its text colour: GPUI shapes text, colour included, while it lays the frame out, before this frame's group hover is known, so a group-hover text colour never reached it and the hint stayed hidden on hover. Opacity is applied when the frame is painted, where the group hover is known, as it is for the half's hover fill.
                */
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * scale))
                    .text_color(appearance.muted)
                    .opacity(if focused { 1.0 } else { 0.0 })
                    .group_hover(START_GROUP, |key| key.opacity(1.0))
                    .child(key)
            }))
            .when(tooltips, |half| {
                half.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowLeft,
                    tooltip_delay,
                    tooltip(start_tooltip),
                )
            })
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.start_new_agent_session(cx);
            }));
        // Stops of the sidebar's focus order; the row sits above the list in both sidebars.
        let start = self.sidebar_focus_stop(
            start,
            START_STOP,
            super::focus_order::SidebarStopPlace::Fixed,
            appearance,
            cx,
        );
        let choose_label = choose_tooltip();
        let choose = div()
            .id("native-sidebar-new-session-choose")
            .role(gpui::Role::Button)
            .aria_label(choose_label.clone())
            .flex_shrink_0()
            .w(px(ARROW_WIDTH * scale))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(divider)
            .rounded_r(px(BUTTON_RADIUS * scale))
            .hover(|half| half.bg(appearance.hover))
            .child(titlebar_svg_icon(
                CHOOSE_ICON,
                13.0 * scale,
                appearance.chrome_icon,
            ))
            .when(tooltips, |half| {
                half.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowRight,
                    tooltip_delay,
                    tooltip(choose_label),
                )
            })
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.open_unified_new_session(cx);
            }));
        let choose = self.sidebar_focus_stop(
            choose,
            "new-session:choose",
            super::focus_order::SidebarStopPlace::Fixed,
            appearance,
            cx,
        );
        h_flex()
            .w_full()
            .flex_shrink_0()
            .px(px(ROW_PADDING * scale))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h(px(ROW_HEIGHT * scale))
                    .rounded(px(BUTTON_RADIUS * scale))
                    .bg(fill)
                    .when_some(outline, |button, outline| {
                        button.border_1().border_color(outline)
                    })
                    .text_size(px(13.0 * scale))
                    .font_weight(FontWeight::NORMAL)
                    .child(start)
                    .child(choose),
            )
            .into_any_element()
    }
}
