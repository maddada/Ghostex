//! The header's Session details button: the focused chat's context ring with its percentage, which
//! opens and closes that chat's Session details panel.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q4): the button is "a small context ring with the %", so the context use shows even with the panel closed. It shows only while the Session details panel extension is on and the focused session is a chat, of any agent since Q555 (it was Claude's alone before); it sits before the ⋯ menu so a narrow column never scrolls it away.
//! SEE-ALSO: apps/desktop/src/app/session_chat/session_details.rs (the switch), apps/gpui-web/src/shell.rs (the web header's copy).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::ElementExt as _;

use crate::app::helpers::*;
use crate::app::native_chat::session_details::{session_details_ring, session_details_ring_label};
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_workarea_header_session_details_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.built_in_extension_enabled(
            ghostex_settings_catalog::built_in_extensions::SESSION_DETAILS,
        ) {
            return None;
        }
        let chat = self.session_details_target_chat()?;
        let (percent, label) = {
            let chat = chat.read(cx);
            if !chat.session_details_available() {
                return None;
            }
            chat.session_details_ring()
        };
        let open = self.session_details_target_open();
        let bounds = self.session_details.button_bounds.clone();
        let text = titlebar_active_text_color();
        Some(
            div()
                .id("ghostex-gpui-workarea-header-session-details")
                .role(gpui::Role::Button)
                .aria_label("Session details")
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .cursor_default()
                .when(cfg!(target_os = "windows"), |this| this.occlude())
                .when(open, |this| this.bg(titlebar_button_hover_color()))
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .child(session_details_ring(
                    percent.unwrap_or(0.0) as f32,
                    titlebar_button_border_color(),
                    text,
                    14.0,
                ))
                .child(session_details_ring_label(label, text))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(if open {
                        "Hide session details"
                    } else {
                        "Show session details"
                    })
                    .build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_session_details(cx);
                    }),
                )
                .on_prepaint(move |painted, _, _| bounds.set(painted))
                .into_any_element(),
        )
    }
}
