/*
CDXC:NavigationHistory 2026-08-19:
The titlebar's Back/Forward pair, sitting between the sidebar toggle and the
active project name. It walks ONE chronological trail of everything the user has
had active — sessions and projects, across machines — not a per-project stack.

The pair is deliberately LEFT of the project name: anchored to the fixed-width
sidebar toggle it never moves, whereas right of the name it slid horizontally
every time the active project's title changed length.

Ownership is deliberately split:
- gxserver owns the trail and the cursor (`server/src/navigation_history`).
- The CEF sidebar runtime owns the conversation with it and the activation of a
  target, because it already owns project/session selection — the web app runs
  the exact same controller, so the two apps cannot drift.
- This module owns pixels and nothing else. It renders from a cached state the
  sidebar pushes over the native-host bridge, and a click sends one intent back.

That split is what keeps the buttons off the frame-rate path: `render_titlebar`
runs on every frame, so it may only read `navigation_history_state`. There is no
RPC, no blocking read, and no allocation-heavy work in the render path here —
the same discipline the Actions snapshot and the git menu state follow after a
per-frame `readSidebarHud` call once cost this titlebar its frame rate.
*/

use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, Styled as _, div, prelude::FluentBuilder as _, px,
};
use gpui_component::h_flex;

use crate::{
    GhostexGpuiApp, TITLEBAR_CONTROL_HEIGHT, titlebar_button_hover_color,
    titlebar_disabled_text_color, titlebar_icon_color, titlebar_svg_icon,
};

/// Page-side event the sidebar runtime listens for. Must stay identical to
/// GPUI_SIDEBAR_NAVIGATION_HISTORY_COMMAND_EVENT_NAME in gxserver-runtime.ts.
const NAVIGATION_HISTORY_COMMAND_EVENT_NAME: &str =
    "ghostex-gpui-sidebar-navigation-history-command";
/// Bridge message the sidebar runtime posts whenever the buttons should change.
pub(crate) const NAVIGATION_HISTORY_STATE_MESSAGE_TYPE: &str = "navigationHistoryState";

const NAVIGATION_BUTTON_WIDTH: f32 = 24.0;
const NAVIGATION_ICON_SIZE: f32 = 15.0;
const NAVIGATION_ICON_BACK: &str = "titlebar/chevron-left.svg";
const NAVIGATION_ICON_FORWARD: &str = "titlebar/chevron-right.svg";

/// Availability only. The arrows carry no hover tooltip, so the destination
/// labels the sidebar also sends are deliberately not read here — two arrows
/// beside the project name do not need a hover card to explain themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiNavigationHistoryState {
    pub(crate) can_go_back: bool,
    pub(crate) can_go_forward: bool,
}

impl GpuiNavigationHistoryState {
    /// Strictly parse the sidebar's bridge payload. A malformed message leaves
    /// the previous state alone rather than blanking the buttons: this is the
    /// only source of truth the titlebar has, and it cannot re-query it.
    pub(crate) fn from_bridge_message(message: &serde_json::Value) -> Option<Self> {
        Some(Self {
            can_go_back: message.get("canGoBack")?.as_bool()?,
            can_go_forward: message.get("canGoForward")?.as_bool()?,
        })
    }
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
    /// `{ "type": "navigationHistoryState", … }` from the sidebar's native-host
    /// bridge. Repaints only when the buttons would actually look different.
    pub(crate) fn receive_navigation_history_state_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = GpuiNavigationHistoryState::from_bridge_message(message) else {
            return;
        };
        if self.navigation_history_state == state {
            return;
        }
        self.navigation_history_state = state;
        cx.notify();
    }

    /// Ask the sidebar runtime to walk the trail. Rust deliberately does not
    /// call gxserver itself: the runtime owns both the daemon conversation and
    /// the project/session activation that has to follow it.
    pub(crate) fn request_navigation_history_navigation(
        &mut self,
        direction: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let script = format!(
            "window.dispatchEvent(new CustomEvent('{NAVIGATION_HISTORY_COMMAND_EVENT_NAME}', {{ detail: {{ direction: '{direction}' }} }})); undefined;"
        );
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn render_titlebar_navigation_history_buttons(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .ml(px(4.0))
            .mr(px(2.0))
            .mt(px(2.0))
            .flex_shrink_0()
            .items_center()
            .child(self.render_titlebar_navigation_history_button(true, cx))
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

        div()
            .id(if back {
                "ghostex-gpui-titlebar-navigate-back"
            } else {
                "ghostex-gpui-titlebar-navigate-forward"
            })
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(NAVIGATION_BUTTON_WIDTH))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(5.0))
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
            .child(titlebar_svg_icon(icon, NAVIGATION_ICON_SIZE, icon_color))
            .into_any_element()
    }
}
