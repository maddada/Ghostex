use gpui::{
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Styled as _, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn reveal_sidebar_session(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:Sidebar 2026-09-20 DECISION:
        // User (ruling 7B): the floating reveal is on all three platforms, so Reveal Active Session
        // floats the sidebar everywhere and never changes its saved collapsed state. This
        // supersedes the Windows and Linux behaviour of expanding the docked sidebar instead.
        if self.sidebar_collapsed {
            self.update_sidebar_reveal(true, false, cx);
        }
        self.dispatch_gpui_sidebar_host_message(
            serde_json::json!({
                "type": "revealSidebarSession",
                "sessionId": session_id,
                "requestId": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("current time is after the Unix epoch")
                    .as_micros() as u64,
            }),
            cx,
        );
    }

    fn active_sidebar_session_reveal_id(&self) -> Option<String> {
        let state = &self.sidebar_gxserver_presentation_focus_state;
        let session_id = state.focused_session_id.as_deref()?;
        if gpui_remote_attach_session_reference_from_project_id(session_id).is_some() {
            return Some(session_id.to_string());
        }
        Some(gpui_combined_presentation_session_id(
            state.active_project_id.as_deref()?,
            session_id,
        ))
    }

    /// CDXC:Navigation 2026-09-16 DECISION:
    /// User: place a circle icon between Previous and Next that reveals the currently focused/active session in the sidebar.
    /// User: match the sidebar's unfilled active-session circle; this replaces the filled dot.
    pub(crate) fn render_titlebar_session_reveal_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let focus = &self.sidebar_gxserver_presentation_focus_state;
        let enabled = self.sidebar.is_some()
            && focus.focused_session_id.is_some()
            && focus.active_project_id.is_some();
        div()
            .id("ghostex-gpui-titlebar-reveal-active-session")
            .flex()
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
                        cx.listener(|this, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            let Some(session_id) = this.active_sidebar_session_reveal_id() else {
                                return;
                            };
                            this.reveal_sidebar_session(&session_id, cx);
                        }),
                    )
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                titlebar_tooltip("Reveal Active Session in Sidebar", window, cx)
            })
            .child(titlebar_svg_icon(
                "titlebar/circle.svg",
                15.0,
                if enabled {
                    titlebar_icon_color()
                } else {
                    titlebar_disabled_text_color()
                },
            ))
    }
}
