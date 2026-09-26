/*
CDXC:Navigation 2026-08-19:
The titlebar's Back/Forward pair, sitting between the sidebar toggle and the
active project name. It walks ONE chronological trail of everything the user has
had active — sessions and projects, across machines — not a per-project stack.

The pair is deliberately LEFT of the project name: anchored to the fixed-width
sidebar toggle it never moves, whereas right of the name it slid horizontally
every time the active project's title changed length.

Ownership is deliberately split:
- gxserver owns the trail and the cursor (`server/src/navigation_history`).
- `controller.rs` owns the conversation with it and the activation of a target
  (since 2026-09-25; before that the CEF sidebar runtime did).
- This file owns pixels and nothing else. It renders from the cached state the
  controller keeps, and a click sends one intent to it.

That split is what keeps the buttons off the frame-rate path: `render_titlebar`
runs on every frame, so it may only read `navigation_history_state`. There is no
RPC, no blocking read, and no allocation-heavy work in the render path here —
the same discipline the Actions snapshot and the git menu state follow after a
per-frame `readSidebarHud` call once cost this titlebar its frame rate.
*/

#[cfg(target_os = "macos")]
mod native_gestures;
mod controller;

pub(crate) use controller::NavigationHistoryHost;

use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, Styled as _, div, prelude::FluentBuilder as _, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use crate::{
    GhostexGpuiApp, TITLEBAR_BUTTON_HORIZONTAL_PADDING, TITLEBAR_BUTTON_RADIUS,
    TITLEBAR_CONTROL_HEIGHT, TITLEBAR_LEADING_TALL_BUTTON_HEIGHT, titlebar_button_hover_color,
    titlebar_disabled_text_color, titlebar_icon_color, titlebar_svg_icon, titlebar_tooltip,
    titlebar_tooltip_label,
};

const NAVIGATION_ICON_SIZE: f32 = 15.0;
const NAVIGATION_ICON_BACK: &str = "titlebar/chevron-left.svg";
const NAVIGATION_ICON_FORWARD: &str = "titlebar/chevron-right.svg";

/// Availability only. The arrows carry no hover tooltip, so the destination
/// labels the daemon also answers with are deliberately not read here: two
/// arrows beside the project name do not need a hover card to explain themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiNavigationHistoryState {
    pub(crate) can_go_back: bool,
    pub(crate) can_go_forward: bool,
}

/// Map the shared hotkey action ids (`packages/shared/ghostex-hotkeys.ts`) onto a trail
/// direction, so a keypress and a titlebar click enter the exact same route.
pub(crate) fn navigation_history_hotkey_direction(action_id: &str) -> Option<&'static str> {
    match action_id {
        "navigateHistoryBack" => Some("back"),
        "navigateHistoryForward" => Some("forward"),
        _ => None,
    }
}

impl GhostexGpuiApp {
    /// Walk the trail one stop (`controller.rs`).
    pub(crate) fn request_navigation_history_navigation(
        &mut self,
        direction: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.navigate_history(direction, cx);
    }

    pub(crate) fn render_titlebar_navigation_history_buttons(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .mt(px(2.0))
            .ml(px(2.0))
            .gap(px(2.0))
            .flex_shrink_0()
            .items_center()
            .child(self.render_titlebar_navigation_history_button(true, cx))
            .child(self.render_titlebar_session_reveal_button(cx))
            .child(self.render_titlebar_navigation_history_button(false, cx))
    }

    /// One arrow. Unavailable directions render dimmed and install no click
    /// handler at all rather than swallowing the press later.
    fn render_titlebar_navigation_history_button(
        &self,
        back: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let state = &self.navigation_history_state;
        let enabled = if back {
            state.can_go_back
        } else {
            state.can_go_forward
        };
        let icon = if back {
            NAVIGATION_ICON_BACK
        } else {
            NAVIGATION_ICON_FORWARD
        };
        let icon_color = if enabled {
            titlebar_icon_color()
        } else {
            titlebar_disabled_text_color()
        };
        let tooltip = if back {
            titlebar_tooltip_label("Back", "navigateHistoryBack")
        } else {
            titlebar_tooltip_label("Forward", "navigateHistoryForward")
        };

        div()
            .id(if back {
                "ghostex-gpui-titlebar-navigate-back"
            } else {
                "ghostex-gpui-titlebar-navigate-forward"
            })
            .flex()
            /*
            CDXC:Navigation 2026-09-20 DECISION:
            User: Back/Forward carry the same corner rounding as the Quick Actions
            split button, like every other header button. This supersedes the
            2026-09-06 rule that they are square; the rest of that decision still
            holds, so their hit and hover area keeps reaching 1px past the titlebar
            control height at the top and the bottom and the arrows still sit in a
            taller strip than the sidebar toggle next to them.
            */
            .h(px(TITLEBAR_LEADING_TALL_BUTTON_HEIGHT))
            .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .cursor_default()
            .when(enabled, |this| {
                this.hover(|this| this.bg(titlebar_button_hover_color()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.request_navigation_history_navigation(
                                if back { "back" } else { "forward" },
                                cx,
                            );
                        }),
                    )
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                titlebar_tooltip(tooltip.clone(), window, cx)
            })
            .child(titlebar_svg_icon(icon, NAVIGATION_ICON_SIZE, icon_color))
            .into_any_element()
    }
}
