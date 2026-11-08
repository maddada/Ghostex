//! "Start a new chat with Main Hub?": what New Chat and Change Agent ask first, with a
//! "Don't ask again" box that turns the question off (`chiefConfirmNewChat`).
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/chief.rs (opens it and runs the change),
//! server/src/chief/session.rs (CDXC:Chief 2026-11-08 DECISION on New Chat, Q1910).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::h_flex;
use std::rc::Rc;

pub(crate) const CHIEF_NEW_CHAT_MODAL_WIDTH: f32 = 440.0;
/// First-frame height only; the window is resized to the measured layout on the first prepaint.
pub(crate) const CHIEF_NEW_CHAT_MODAL_INITIAL_HEIGHT: f32 = 200.0;

/// What the dialog asks its host to do. The dialog removes its own window before sending either.
pub(crate) enum ChiefNewChatModalCommand {
    /// "New Chat" or Enter; `dont_ask_again` is the box's state.
    Start { dont_ask_again: bool },
    /// "Cancel", Escape or the corner close.
    Cancel,
}

pub(crate) type ChiefNewChatModalHost = Rc<dyn Fn(ChiefNewChatModalCommand, &mut App)>;

pub(crate) struct GpuiChiefNewChatModalWindow {
    host: ChiefNewChatModalHost,
    palette: ModalPalette,
    title: SharedString,
    message: SharedString,
    confirm_label: SharedString,
    dont_ask_again: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiChiefNewChatModalWindow {
    /// `title`, `message` and `confirm_label` differ for New Chat and Change Agent.
    pub(crate) fn new(
        palette: ModalPalette,
        host: ChiefNewChatModalHost,
        title: String,
        message: String,
        confirm_label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette,
            title: title.into(),
            message: message.into(),
            confirm_label: confirm_label.into(),
            dont_ask_again: false,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: ChiefNewChatModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dont_ask_again = self.dont_ask_again;
        self.close_window_and_send(
            ChiefNewChatModalCommand::Start { dont_ask_again },
            window,
            cx,
        );
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => self.start(window, cx),
            "escape" => self.close_window_and_send(ChiefNewChatModalCommand::Cancel, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// The "Don't ask again" box: a square in the switch's colours, the whole line clickable.
    fn dont_ask_again_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let checked = self.dont_ask_again;
        h_flex()
            .id("chief-new-chat-dont-ask")
            .role(gpui::Role::CheckBox)
            .aria_label("Don't ask again")
            .aria_toggled(a11y_toggled(checked))
            .gap(px(8.0))
            .items_center()
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(p.foreground)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.dont_ask_again = !this.dont_ask_again;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(if checked { p.primary } else { p.hairline })
                    .bg(if checked { p.primary } else { p.switch_off })
                    .when(checked, |this| {
                        this.child(modal_icon(
                            "modals/kit/legacy-check.svg",
                            16.0,
                            p.primary_foreground,
                        ))
                    }),
            )
            .child("Don't ask again")
    }
}

impl Render for GpuiChiefNewChatModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let footer = modal_footer(vec![
            modal_action_button(
                &p,
                "chief-new-chat-cancel",
                "Cancel",
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| {
                    this.close_window_and_send(ChiefNewChatModalCommand::Cancel, window, cx)
                },
                cx,
            ),
            modal_action_button(
                &p,
                "chief-new-chat-start",
                self.confirm_label.clone(),
                None,
                ModalButtonTone::Primary,
                false,
                |this, window, cx| this.start(window, cx),
                cx,
            ),
        ]);
        let dont_ask = self.dont_ask_again_row(cx).into_any_element();
        modal_shell(
            &p,
            "ghostex-gpui-chief-new-chat-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            vec![
                modal_header(&p, self.title.clone(), Some(self.message.clone())),
                dont_ask,
            ],
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiChiefNewChatModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(ChiefNewChatModalCommand::Cancel, window, cx);
    }
}
