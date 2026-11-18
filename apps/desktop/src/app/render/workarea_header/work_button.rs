//! The header's Work button: the focused chat's Work panel (a coordinator's work items, or an
//! ordinary session's own while the Work items extension is on), with how many wait on the user.
//! It sits beside Session details; the two share the spot beside the chat, one at a time.
//!
//! CDXC:Coordinators 2026-11-18 SEE-ALSO: native_chat/work_topic.rs (the user's decision: layout B,
//! buttons in the header, for coordinators too), session_chat/session_details.rs
//! (`toggle_work_panel`), apps/gpui-web/src/shell.rs (the web header's copy).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use gpui_component::ElementExt as _;

use crate::app::helpers::*;
use crate::*;

/// The Work button's look, shared by the main header and a popup window's header: the board
/// icon, "Work", and the pink count of what waits on the user.
pub(crate) fn work_button(
    id: &'static str,
    open: bool,
    waiting: usize,
) -> gpui::Stateful<gpui::Div> {
    let text = titlebar_active_text_color();
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label("Work")
        .aria_expanded(open)
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
        .child(
            svg()
                .path("titlebar/layout-board-split.svg")
                .size(px(14.0))
                .flex_shrink_0()
                .text_color(text),
        )
        .child(div().text_size(px(12.0)).text_color(text).child("Work"))
        .when(waiting > 0, |this| {
            this.child(
                div()
                    .px(px(5.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .bg(gpui::rgb(0xf472b6))
                    .text_size(px(10.5))
                    .text_color(gpui::rgb(0x1a1a1a))
                    .child(waiting.to_string()),
            )
        })
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(if open {
                "Hide work items"
            } else {
                "Show work items"
            })
            .build(window, cx)
        })
}

impl GhostexGpuiApp {
    pub(crate) fn render_workarea_header_work_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let chat = self.session_details_target_chat()?;
        let (open, waiting) = {
            let chat = chat.read(cx);
            if !chat.work_panel_available() {
                return None;
            }
            (chat.work_panel_shown(), chat.work_panel_waiting())
        };
        let bounds = self.session_details.work_button_bounds.clone();
        Some(
            work_button("ghostex-gpui-workarea-header-work", open, waiting)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_work_panel(window, cx);
                    }),
                )
                .on_prepaint(move |painted, _, _| bounds.set(painted))
                .into_any_element(),
        )
    }

    /// A popup window's Work button, for the session its header names: the popup's own opener of
    /// that chat's Work panel (decided after review: one opener per window).
    pub(crate) fn render_session_popup_work_button(
        &self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let chat = self.native_chat_views.get(&session_id)?;
        let (open, waiting) = {
            let chat = chat.read(cx);
            if !chat.work_panel_available() {
                return None;
            }
            (chat.work_panel_shown(), chat.work_panel_waiting())
        };
        let bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>> = Default::default();
        let anchor = bounds.clone();
        Some(
            work_button("session-popup-work", open, waiting)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_work_panel_for(session_id, anchor.get(), window, cx);
                    }),
                )
                .on_prepaint(move |painted, _, _| bounds.set(painted))
                .into_any_element(),
        )
    }
}
