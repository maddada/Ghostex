use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use serde_json::{Value, json};

pub(super) struct TerminalDialogInput {
    identity: String,
    server_value: String,
    pub(super) field: TerminalDialogField,
    _subscription: Subscription,
}

/// The dialog's text field: one line, or a growing box when the dialog asks for multiline input.
pub(super) enum TerminalDialogField {
    Line(Entity<InputState>),
    Lines(Entity<TextareaState>),
}

impl TerminalDialogField {
    fn value(&self, cx: &App) -> SharedString {
        match self {
            Self::Line(input) => input.read(cx).value(),
            Self::Lines(input) => input.read(cx).value(),
        }
    }

    pub(super) fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Line(input) => input.focus_handle(cx),
            Self::Lines(input) => input.focus_handle(cx),
        }
    }

    fn set_value(&self, value: String, window: &mut Window, cx: &mut App) {
        match self {
            Self::Line(input) => input.update(cx, |input, cx| input.set_value(value, window, cx)),
            Self::Lines(input) => input.update(cx, |input, cx| input.set_value(value, window, cx)),
        }
    }
}

impl NativeChatView {
    fn terminal_dialog_input_event<T>(
        &mut self,
        _: &Entity<T>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
            self.submit_terminal_dialog(cx);
        }
    }

    pub(super) fn submit_terminal_dialog(&mut self, cx: &mut Context<Self>) {
        let dialog = &self.snapshot["terminalNotice"]["dialog"];
        let Some(input) = &self.terminal_dialog_input else {
            return;
        };
        self.invoke(
            json!({"type":"answer","answer":{
                "kind":"terminalDialog", "dialogId":dialog["id"],
                "dialogAction":if dialog["input"] == "search" { "text" } else { "submit" },
                "text":input.field.value(cx).to_string(),
            }}),
            cx,
        );
    }

    pub(super) fn render_terminal_dialog(
        &mut self,
        dialog: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, Vec<AnyElement>) {
        let mut body = Vec::new();
        let mut actions = Vec::new();
        let dialog_id = text(dialog, "id");
        let busy = self.snapshot["questionCard"]["busy"] == true;
        if dialog_id == "codex-transcript-pager" {
            body.push(div().child(text(dialog, "body")).into_any_element());
            actions.push(self.chat_button("restore-chat".into(), if busy { "Restoring chat…" } else { "Restore chat" }.into(),
                json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":dialog_id,"dialogAction":"cancel"}}), p, cx));
            return (body, actions);
        }
        let has_rows = dialog["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty());
        let copy = &dialog["presentation"]["copy"];
        if copy.is_object() {
            // Written copy reads as the card's prose, and its buttons already say what the
            // panel's keyboard hint footer would.
            for (index, paragraph) in copy["paragraphs"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                body.push(self.markdown(
                    format!("terminal-dialog-copy:{dialog_id}:{index}"),
                    paragraph.as_str().unwrap_or_default().to_string(),
                    &Value::Null,
                    p,
                    cx,
                ));
            }
        } else if !has_rows && !text(dialog, "body").is_empty() {
            body.push(
                div()
                    .id("terminal-dialog-body")
                    .max_h(px(256.0 * p.scale))
                    .overflow_y_scroll()
                    .rounded(px(8.0 * p.scale))
                    .p(px(12.0 * p.scale))
                    .bg(p.border.opacity(0.3))
                    .font_family("Menlo")
                    .text_size(px(12.0 * p.scale))
                    .child(text(dialog, "body"))
                    .into_any_element(),
            );
        }
        if dialog["input"] == "text" || dialog["input"] == "search" {
            let identity = format!(
                "{}:{}:{}",
                dialog_id,
                text(dialog, "title"),
                text(dialog, "input")
            );
            let server_value = text(dialog, "inputValue");
            let multiline = dialog["presentation"]["multilineInput"] == true;
            if self
                .terminal_dialog_input
                .as_ref()
                .is_none_or(|previous| previous.identity != identity)
            {
                let limit = if dialog["input"] == "search" {
                    512
                } else {
                    8192
                };
                let placeholder = if dialog["input"] == "search" {
                    "Search options…"
                } else {
                    "Enter text…"
                };
                let (field, subscription) = if multiline {
                    let input = cx.new(|cx| {
                        TextareaState::new(window, cx)
                            .submit_on_enter(true)
                            .auto_grow(2, 6)
                            .validate(move |value, _| value.encode_utf16().count() <= limit)
                            .placeholder(placeholder)
                            .default_value(server_value.clone())
                    });
                    let subscription =
                        cx.subscribe_in(&input, window, Self::terminal_dialog_input_event);
                    (TerminalDialogField::Lines(input), subscription)
                } else {
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .validate(move |value, _| value.encode_utf16().count() <= limit)
                            .placeholder(placeholder)
                            .default_value(server_value.clone())
                    });
                    let subscription =
                        cx.subscribe_in(&input, window, Self::terminal_dialog_input_event);
                    (TerminalDialogField::Line(input), subscription)
                };
                self.terminal_dialog_input = Some(TerminalDialogInput {
                    identity,
                    server_value: server_value.clone(),
                    field,
                    _subscription: subscription,
                });
            }
            let state = self.terminal_dialog_input.as_mut().unwrap();
            if state.server_value != server_value {
                state.server_value = server_value.clone();
                state.field.set_value(server_value, window, cx);
            }
            body.push(match &state.field {
                TerminalDialogField::Line(input) => Input::new(input)
                    .disabled(busy)
                    .w_full()
                    .text_size(px(14.0 * p.scale))
                    .into_any_element(),
                TerminalDialogField::Lines(input) => Textarea::new(input)
                    .disabled(busy)
                    .w_full()
                    .text_size(px(14.0 * p.scale))
                    .into_any_element(),
            });
            let submit_label = if dialog["input"] == "search" {
                "Search".to_string()
            } else {
                text(&dialog["presentation"], "submitLabel")
            };
            actions.push(
                div()
                    .id("terminal-dialog-submit")
                    .role(gpui::Role::Button)
                    .aria_label(submit_label.clone())
                    .chat_cursor_pointer()
                    .px(px(8.0 * p.scale))
                    .py(px(4.0 * p.scale))
                    .rounded(px(6.0 * p.scale))
                    .border_1()
                    .border_color(p.border)
                    .child(submit_label)
                    .on_click(cx.listener(|this, _, _, cx| this.submit_terminal_dialog(cx)))
                    .into_any_element(),
            );
        } else {
            self.terminal_dialog_input = None;
        }
        if dialog["input"] == "key" {
            let id = dialog_id.clone();
            body.push(div().id("terminal-dialog-key").track_focus(&self.terminal_dialog_key_focus)
                .border_1().border_color(p.border).rounded(px(6.0*p.scale)).p(px(8.0*p.scale)).chat_cursor_pointer()
                .child("Focus here, then press the new shortcut")
                .on_click(cx.listener(|this,_,window,cx| this.terminal_dialog_key_focus.focus(window,cx)))
                .on_key_down(cx.listener(move |this,event:&gpui::KeyDownEvent,window,cx| {
                    if !this.terminal_dialog_key_focus.is_focused(window) { return; }
                    let key = &event.keystroke;
                    let text = match key.key.as_str() { "enter"=>"Enter", "escape"=>"Escape", "space"=>" ", "backspace"=>"Backspace", "tab"=>"Tab", "up"=>"ArrowUp", "down"=>"ArrowDown", "left"=>"ArrowLeft", "right"=>"ArrowRight", _=>key.key.as_str() };
                    this.invoke(json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":id,"dialogAction":"key","text":text,
                        "keyModifiers":u8::from(key.modifiers.shift)+2*u8::from(key.modifiers.alt)+4*u8::from(key.modifiers.control)+8*u8::from(key.modifiers.platform)}}),cx);
                    window.prevent_default(); cx.stop_propagation();
                })).into_any_element());
        }
        if !copy.is_object() && !text(dialog, "footer").is_empty() {
            body.push(
                div()
                    .text_size(px(14.0 * p.scale))
                    .line_height(px(18.666667 * p.scale))
                    .child(text(dialog, "footer"))
                    .into_any_element(),
            );
        }
        for action in dialog["presentation"]["actions"]
            .as_array()
            .into_iter()
            .flatten()
        {
            actions.push(self.chat_button(format!("dialog-action:{}",text(action,"action")),text(action,"label"),
                json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":dialog_id,"dialogAction":action["action"]}}),p,cx));
        }
        (body, actions)
    }
}
