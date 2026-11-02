//! "Close Main Coordinator and its 3 open threads?": the question a close asks when it would stop
//! a coordinator or a busy agent. The wording is gx-core's `close_confirmation`.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_close_confirm.rs (opens it and runs the close),
//! packages/gx-core/src/sidebar_actions/close_confirm.rs (the DECISION and the rule).
use super::native_modal_kit::*;
use gpui::{App, Context, FocusHandle, IntoElement, KeyDownEvent, Render, SharedString, Window};
use std::rc::Rc;

pub(crate) const CLOSE_SESSIONS_CONFIRM_MODAL_WIDTH: f32 = 460.0;
/// First-frame height only; the window is resized to the measured layout on the first prepaint.
pub(crate) const CLOSE_SESSIONS_CONFIRM_MODAL_INITIAL_HEIGHT: f32 = 190.0;

/// What the dialog asks its host to do. The dialog removes its own window before sending either.
pub(crate) enum CloseSessionsConfirmModalCommand {
    /// "Close" or Enter.
    Close,
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
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette,
            title: confirmation.title.into(),
            message: confirmation.message.into(),
            confirm_label: confirmation.confirm_label.into(),
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
            "enter" => CloseSessionsConfirmModalCommand::Close,
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
                ModalButtonTone::Danger,
                false,
                |this, window, cx| {
                    this.close_window_and_send(CloseSessionsConfirmModalCommand::Close, window, cx)
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
            vec![modal_header(
                &p,
                self.title.clone(),
                Some(self.message.clone()),
            )],
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
