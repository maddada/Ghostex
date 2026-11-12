use super::tone_card::ToneButton;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use serde_json::{Value, json};

/// The width, at scale 1, of the footer button named after a menu's highlighted row.
const HIGHLIGHTED_ROW_SLOT_WIDTH: f32 = 220.0;

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
    /// One numbered row of a live terminal dialog (option D, `dialog_rows.rs` in the chat core):
    /// the question card's row sizes (`choice_row`) with its number at the start, filled on the
    /// row the terminal highlights, the name on one line (whole on hover), a muted suffix and
    /// detail, and the row's stats and shortcut at the end. `answer` is `None` for the "More" row,
    /// which `on_more` opens instead.
    pub(super) fn dialog_choice(
        &self,
        id: String,
        choice: &Value,
        shortcut: Option<String>,
        disabled: bool,
        regular_label: bool,
        answer: Option<Value>,
        on_more: Option<Box<dyn Fn(&mut Self, &mut Context<Self>)>>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let highlighted = choice["highlighted"] == true;
        let title = text(choice, "title");
        let number = choice["number"]
            .as_u64()
            .map(|number| number.to_string())
            .unwrap_or_default();
        let badge = div()
            .size(px(20.0 * s))
            .flex_shrink_0()
            .rounded(px(6.0 * s))
            .border_1()
            .flex()
            .justify_center()
            .items_center()
            .text_size(px(11.5 * s))
            .font_weight(gpui::FontWeight::BOLD)
            .map(|badge| {
                if highlighted {
                    badge
                        .bg(p.control_primary)
                        .border_color(p.control_primary)
                        .text_color(p.background)
                } else {
                    badge
                        .bg(p.background.opacity(0.4))
                        .border_color(p.control_border.opacity(0.6))
                        .text_color(p.card_muted)
                }
            })
            .map(|badge| {
                if number.is_empty() {
                    badge.child(
                        gpui::svg()
                            .path("titlebar/dots.svg")
                            .size(px(12.0 * s))
                            .text_color(p.card_muted),
                    )
                } else {
                    badge.child(number.clone())
                }
            });
        let suffix = text(choice, "suffix");
        let detail = text(choice, "detail");
        let meta = text(choice, "meta");
        let tooltip = title.clone();
        div()
            .id(gpui::SharedString::from(id))
            .when(!disabled, |row| row.tab_index(0))
            .role(gpui::Role::Button)
            .aria_label(title.clone())
            .flex()
            .items_start()
            .gap(px(11.0 * s))
            .w_full()
            .px(px(12.0 * s))
            .py(px(8.0 * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(if highlighted {
                p.control_primary.opacity(0.45)
            } else {
                p.control_border
            })
            .text_color(p.foreground)
            .when(p.light, |row| row.bg(p.background))
            .when(highlighted, |row| row.bg(p.control_primary.opacity(0.06)))
            .when(disabled, |row| row.opacity(0.6))
            .when(!disabled, |row| {
                row.chat_cursor_pointer().hover(|style| {
                    style.bg(if p.light {
                        p.input
                    } else {
                        p.input_border.opacity(0.3)
                    })
                })
            })
            .child(badge)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(1.0 * s))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(6.0 * s))
                            .min_w_0()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(14.0 * s))
                                    .line_height(px(19.25 * s))
                                    // CDXC:SessionChat 2026-11-01 DECISION: User wants the usage limit choices at normal chat weight while the card title stays bold. Other numbered dialog choices keep their weight.
                                    .font_weight(if regular_label {
                                        gpui::FontWeight::NORMAL
                                    } else {
                                        gpui::FontWeight::SEMIBOLD
                                    })
                                    .child(title.clone()),
                            )
                            .when(!suffix.is_empty(), |line| {
                                line.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(12.0 * s))
                                        .text_color(p.card_muted)
                                        .child(suffix),
                                )
                            })
                            .when(choice["ticked"] == true, |line| {
                                line.child(
                                    gpui::svg()
                                        .path("titlebar/check.svg")
                                        .size(px(14.0 * s))
                                        .flex_shrink_0()
                                        .text_color(p.control_primary),
                                )
                            }),
                    )
                    .when(!detail.is_empty(), |column| {
                        column.child(
                            div()
                                .text_size(px(13.0 * s))
                                .line_height(px(18.0 * s))
                                .text_color(p.card_muted)
                                .child(detail),
                        )
                    }),
            )
            .when(!meta.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(180.0 * s))
                        .truncate()
                        .text_size(px(12.0 * s))
                        .line_height(px(19.25 * s))
                        .text_color(p.muted)
                        .child(meta),
                )
            })
            .when_some(shortcut, |row, shortcut| {
                row.child(
                    div()
                        .h(px(20.0 * s))
                        .min_w(px(20.0 * s))
                        .px(px(4.0 * s))
                        .flex_shrink_0()
                        .border_1()
                        .border_color(p.control_border.opacity(0.6))
                        .bg(p.background.opacity(0.4))
                        .rounded(px(4.0 * s))
                        .flex()
                        .justify_center()
                        .items_center()
                        .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                        .text_size(px(13.0 * s))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(p.muted)
                        .child(shortcut),
                )
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .when(!disabled, |row| {
                row.on_click(
                    cx.listener(move |this, _, _, cx| match (&answer, &on_more) {
                        (Some(answer), _) => this.invoke(answer.clone(), cx),
                        (None, Some(on_more)) => on_more(this, cx),
                        (None, None) => {}
                    }),
                )
            })
            .into_any_element()
    }

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
            let restore = json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":dialog_id,"dialogAction":"cancel"}});
            actions.push(
                self.tone_button(
                    "restore-chat",
                    if busy {
                        "Restoring chat…"
                    } else {
                        "Restore chat"
                    },
                    None,
                    ToneButton::Primary,
                    None,
                    false,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(restore.clone(), cx)))
                .into_any_element(),
            );
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
        } else if let Some(blocks) = dialog["presentation"]["blocks"]
            .as_array()
            .filter(|blocks| !blocks.is_empty())
        {
            // A long panel (the Usage tab) scrolls inside the card rather than pushing the composer
            // off screen; beside option rows it shares the height with them.
            let share = if has_rows { 0.25 } else { 0.5 };
            body.push(
                div()
                    .id("terminal-dialog-blocks")
                    .max_h(px(window.viewport_size().height.as_f32() * share))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(12.0 * p.scale))
                    // A scrolling column shrinks its children to fit unless they refuse to.
                    .children(
                        self.panel_blocks(dialog, blocks, p, cx)
                            .into_iter()
                            .map(|block| div().flex_shrink_0().child(block)),
                    )
                    .into_any_element(),
            );
        } else if !has_rows && !text(dialog, "body").is_empty() {
            body.push(
                div()
                    .id("terminal-dialog-body")
                    .max_h(px(256.0 * p.scale))
                    .overflow_y_scroll()
                    .rounded(px(8.0 * p.scale))
                    .p(px(12.0 * p.scale))
                    .bg(p.border.opacity(0.3))
                    .font_family(crate::app::native_chat::fonts::CHAT_MONO)
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
                            .auto_text_direction(true)
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
                    .caret_color(p.foreground)
                    .w_full()
                    .text_size(px(14.0 * p.scale))
                    .into_any_element(),
                TerminalDialogField::Lines(input) => Textarea::new(input)
                    .disabled(busy)
                    .caret_color(p.foreground)
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
                self.tone_button(
                    "terminal-dialog-submit",
                    &submit_label,
                    None,
                    // Beside numbered rows the labelled Confirm is the one filled button.
                    if has_rows && dialog["input"] == "search" {
                        ToneButton::Secondary
                    } else {
                        ToneButton::Primary
                    },
                    None,
                    false,
                    p,
                )
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
        // Written copy and blocks come with buttons for everything the key-hint footer says, and
        // numbered rows with their own hint in the card's footer.
        if !copy.is_object()
            && !has_rows
            && !dialog["presentation"]["blocks"].is_array()
            && !text(dialog, "footer").is_empty()
        {
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
            let answer = json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":dialog_id,"dialogAction":action["action"]}});
            actions.push(
                self.tone_button_in_slot(
                    gpui::SharedString::from(format!("dialog-action:{}", text(action, "action"))),
                    &text(action, "label"),
                    None,
                    // The labelled Confirm says what Enter picks, so it is the card's filled button.
                    if has_rows && action["action"] == "confirm" {
                        ToneButton::Primary
                    } else {
                        ToneButton::Secondary
                    },
                    None,
                    false,
                    // The button named after the highlighted row keeps one width as the highlight moves.
                    (action["highlightedRow"] == true).then_some(HIGHLIGHTED_ROW_SLOT_WIDTH),
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(answer.clone(), cx)))
                .into_any_element(),
            );
        }
        (body, actions)
    }
}
