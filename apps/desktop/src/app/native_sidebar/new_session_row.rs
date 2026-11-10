//! The New session row under the sidebar's top row, in the Project and the Unified sidebar alike
//! (and so in the floating panel of a hidden sidebar, and with bots showing): one wide button in
//! two halves, then Add project.
//!
//! CDXC:Sidebar 2026-11-10 DECISION:
//! User (Q2340 C, Q2341 C, Q2342 D, Q2343 A, Q2344 "beside", Q2345 A): every sidebar layout has a wide New session button in a row of its own under the top row, with the pencil-on-a-square icon, its words and its shortcut on it. A click starts the agent you used last at once, in the project of the session you have open, as Cmd+Shift+O does (`start_new_agent_session`); the arrow at its end opens the New Thread picker, as Cmd+N does (`open_unified_new_session`). Add project sits beside it in both layouts and stays in the top-right menu too. The button replaces the Unified header's pencil, and Add project left that header for this row. It rests on a faint light fill so it reads as the sidebar's main action, never a dark one on glass.
//! SEE-ALSO: native_sidebar/render.rs and selectors.rs (where the row goes), native_sidebar/unified/header.rs (the header it took the pencil and Add project from), apps/gpui-web/src/app/web_host/new_thread_picker.rs (the web build's `start_new_agent_session`).
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use serde_json::json;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The row's side padding, its height and the gap before Add project, before the zoom.
const ROW_PADDING: f32 = 8.0;
const ROW_HEIGHT: f32 = 30.0;
const ROW_GAP: f32 = 4.0;
/// The button's corner radius, the arrow half's width and the inset of the label half.
const BUTTON_RADIUS: f32 = 7.0;
const ARROW_WIDTH: f32 = 29.0;
const LABEL_PADDING: f32 = 10.0;
/// The label half's least width that still fits its icon, "New session" and the shortcut; below
/// it the shortcut is left off (the tooltip still names it).
const LABEL_WITH_KEY_MIN_WIDTH: f32 = 150.0;

/// The words on the button.
const LABEL: &str = "New session";

/// The width the label half gets in a sidebar `sidebar_width` wide, at `scale`.
fn label_half_width(sidebar_width: f32, scale: f32) -> f32 {
    sidebar_width / scale - ROW_PADDING * 2.0 - ROW_HEIGHT - ROW_GAP - ARROW_WIDTH
}

/// The shortcut the button shows: the configured New Agent Session key. The browser build has no
/// such key, so it shows none there.
fn start_shortcut() -> Option<String> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    crate::gpui_configured_hotkey_label("createAgentSession")
}

/// What a hover over the arrow half says, with the New Thread key when one is set.
fn choose_tooltip() -> String {
    let shortcut = if cfg!(target_arch = "wasm32") {
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
    /// The New session row: the two-half button, then Add project.
    pub(in crate::app::native_sidebar) fn render_native_sidebar_new_session_row(
        &self,
        appearance: &SidebarAppearance,
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
        let start = h_flex()
            .id("native-sidebar-new-session")
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
                "titlebar/edit.svg",
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
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * scale))
                    .text_color(appearance.muted)
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
                "titlebar/chevron-down.svg",
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
        let add_project = div()
            .id("native-sidebar-new-row-add-project")
            .role(gpui::Role::Button)
            .aria_label("Add project")
            .size(px(ROW_HEIGHT * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(BUTTON_RADIUS * scale))
            .hover(|button| button.bg(appearance.hover))
            .child(titlebar_svg_icon(
                "titlebar/folder-plus.svg",
                15.0 * scale,
                appearance.chrome_icon,
            ))
            .when(tooltips, |button| {
                button.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowRight,
                    tooltip_delay,
                    tooltip("Add project".to_string()),
                )
            })
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "sidebarAction", "action": "addProject"}),
                    cx,
                );
            }));
        h_flex()
            .w_full()
            .flex_shrink_0()
            .pt(px(1.0 * scale))
            .px(px(ROW_PADDING * scale))
            .gap(px(ROW_GAP * scale))
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
            .child(add_project)
            .into_any_element()
    }
}
