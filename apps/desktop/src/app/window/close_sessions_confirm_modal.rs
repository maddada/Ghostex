//! "Close Main Coordinator and its 3 open threads?": the question a close asks when it would stop
//! a coordinator or a busy agent. The wording is gx-core's `close_confirmation`.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_close_confirm.rs (opens it and runs the close),
//! packages/gx-core/src/sidebar_actions/close_confirm.rs (the DECISION and the rule).
use super::native_modal_kit::*;
use gpui::{
    App, Context, FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, px, svg,
};
use std::rc::Rc;

pub(crate) const CLOSE_SESSIONS_CONFIRM_MODAL_WIDTH: f32 = 460.0;
/// First-frame height only; the window is resized to the measured layout on the first prepaint.
pub(crate) const CLOSE_SESSIONS_CONFIRM_MODAL_INITIAL_HEIGHT: f32 = 190.0;

/// What the dialog asks its host to do. The dialog removes its own window before sending either.
pub(crate) enum CloseSessionsConfirmModalCommand {
    /// Confirm button or Enter, carrying the optional checkbox's final state.
    Close { checkbox_checked: bool },
    /// "Cancel", Escape or the corner close.
    Cancel,
}

pub(crate) type CloseSessionsConfirmModalHost =
    Rc<dyn Fn(CloseSessionsConfirmModalCommand, &mut App)>;

pub(crate) struct GpuiCloseSessionsConfirmModalWindow {
    host: CloseSessionsConfirmModalHost,
    palette: ModalPalette,
    title: SharedString,
    message: SharedString,
    confirm_label: SharedString,
    checkbox_label: Option<SharedString>,
    checkbox_checked: bool,
    primary_tone: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiCloseSessionsConfirmModalWindow {
    pub(crate) fn new(
        palette: ModalPalette,
        host: CloseSessionsConfirmModalHost,
        confirmation: ghostex_gx_core::CloseConfirmation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_options(palette, host, confirmation, None, false, window, cx)
    }

    pub(crate) fn with_options(
        palette: ModalPalette,
        host: CloseSessionsConfirmModalHost,
        confirmation: ghostex_gx_core::CloseConfirmation,
        checkbox: Option<(String, bool)>,
        primary_tone: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let (checkbox_label, checkbox_checked) = match checkbox {
            Some((label, checked)) => (Some(label.into()), checked),
            None => (None, false),
        };
        Self {
            host,
            palette,
            title: confirmation.title.into(),
            message: confirmation.message.into(),
            confirm_label: confirmation.confirm_label.into(),
            checkbox_label,
            checkbox_checked,
            primary_tone,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: CloseSessionsConfirmModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let command = match event.keystroke.key.as_str() {
            "enter" => CloseSessionsConfirmModalCommand::Close {
                checkbox_checked: self.checkbox_checked,
            },
            "escape" => CloseSessionsConfirmModalCommand::Cancel,
            _ => return,
        };
        self.close_window_and_send(command, window, cx);
        cx.stop_propagation();
    }
}

impl Render for GpuiCloseSessionsConfirmModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let checked = self.checkbox_checked;
        let confirm_tone = if self.primary_tone {
            ModalButtonTone::Primary
        } else {
            ModalButtonTone::Danger
        };
        let mut body = vec![modal_header(
            &p,
            self.title.clone(),
            Some(self.message.clone()),
        )];
        if let Some(label) = self.checkbox_label.clone() {
            let box_el = div()
                .size(px(16.0))
                .flex_shrink_0()
                .rounded(px(4.0))
                .border_1()
                .border_color(if checked {
                    hsla(p.primary)
                } else {
                    hsla(p.muted)
                })
                .bg(if checked {
                    hsla(p.primary)
                } else {
                    gpui::transparent_black()
                })
                .flex()
                .items_center()
                .justify_center()
                .children(checked.then(|| {
                    svg()
                        .path("titlebar/check.svg")
                        .size(px(12.0))
                        .text_color(hsla(p.primary_foreground))
                }));
            let row = div()
                .id("close-sessions-confirm-checkbox")
                .role(gpui::Role::CheckBox)
                .aria_label(label.clone())
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.panel))
                .cursor_pointer()
                .hover(|style| style.bg(hsla(p.raised)))
                .child(box_el)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(hsla(p.foreground))
                        .child(label),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.checkbox_checked = !this.checkbox_checked;
                    cx.notify();
                }))
                .into_any_element();
            body.push(row);
        }
        let footer = modal_footer(vec![
            modal_action_button(
                &p,
                "close-sessions-confirm-cancel",
                "Cancel",
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| {
                    this.close_window_and_send(CloseSessionsConfirmModalCommand::Cancel, window, cx)
                },
                cx,
            ),
            modal_action_button(
                &p,
                "close-sessions-confirm-close",
                self.confirm_label.clone(),
                None,
                confirm_tone,
                false,
                |this, window, cx| {
                    let checkbox_checked = this.checkbox_checked;
                    this.close_window_and_send(
                        CloseSessionsConfirmModalCommand::Close { checkbox_checked },
                        window,
                        cx,
                    )
                },
                cx,
            ),
        ]);
        modal_shell(
            &p,
            "ghostex-gpui-close-sessions-confirm-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            body,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiCloseSessionsConfirmModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(CloseSessionsConfirmModalCommand::Cancel, window, cx);
    }
}
