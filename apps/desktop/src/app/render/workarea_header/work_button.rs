//! The header's Work button: the focused chat's Work panel (a coordinator's work items, or an
//! ordinary session's own while the Work items extension is on).
//! It sits beside Session details; the two share the spot beside the chat, one at a time.
//!
//! CDXC:Coordinators 2026-11-18 SEE-ALSO: native_chat/work_topic.rs (the user's decision: layout B,
//! buttons in the header, for coordinators too), session_chat/session_details.rs
//! (`toggle_work_panel`), apps/gpui-web/src/shell.rs (the web header's copy).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::ElementExt as _;

use crate::app::helpers::*;
use crate::*;

/// The Work button's look, shared by the main header and a popup window's header: the board
/// icon and "Work". No count badge: what waits on the user shows in the dock (CDXC:Coordinators
/// 2026-11-19, the user: "I don't want to see 6 here at the top. I think it's useless because I
/// can already see it in the dock").
pub(crate) fn work_button(id: &'static str, open: bool) -> gpui::Stateful<gpui::Div> {
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
        .child(super::header_pins::header_icon(
            "titlebar/layout-board-split.svg",
            text,
        ))
        .child(div().text_size(px(12.0)).text_color(text).child("Work"))
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
        let open = {
            let chat = chat.read(cx);
            if !chat.work_panel_available() {
                return None;
            }
            chat.work_panel_shown()
        };
        let bounds = self.session_details.work_button_bounds.clone();
        Some(
            work_button("ghostex-gpui-workarea-header-work", open)
                .on_mouse_down(
                    MouseButton::Right,
                    super::header_pins::header_slot_right_click(
                        super::header_pins::HeaderSlot::Work,
                        cx,
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_work_panel(window, cx);
                    }),
                )
                // A press from accessibility (VoiceOver, cua-driver): `on_mouse_down` alone
                // offers none, and the button keeps acting on mouse-down for the pointer.
                .on_a11y_action(gpui::AccessibleAction::Click, {
                    let app = cx.entity().downgrade();
                    move |_, window, cx| {
                        let _ = app.update(cx, |this, cx| this.toggle_work_panel(window, cx));
                    }
                })
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
        let open = {
            let chat = chat.read(cx);
            if !chat.work_panel_available() {
                return None;
            }
            chat.work_panel_shown()
        };
        let bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>> = Default::default();
        let anchor = bounds.clone();
        Some(
            work_button("session-popup-work", open)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let anchor = anchor.clone();
                        move |this, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.toggle_work_panel_for(session_id, anchor.get(), window, cx);
                        }
                    }),
                )
                .on_a11y_action(gpui::AccessibleAction::Click, {
                    let app = cx.entity().downgrade();
                    move |_, window, cx| {
                        let _ = app.update(cx, |this, cx| {
                            this.toggle_work_panel_for(session_id, anchor.get(), window, cx)
                        });
                    }
                })
                .on_prepaint(move |painted, _, _| bounds.set(painted))
                .into_any_element(),
        )
    }
}
