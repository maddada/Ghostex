//! The `/resume` picker card (Claude Code and Codex) and the pass-through card (every other agent):
//! an agent screen that owns the keyboard, drawn in the chat and driven from it.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO: the user's mirror and pass-through decision in packages/gx-chat-core/src/questions/terminal_picker.rs; server/src/session_chat_resume_picker.rs, apps/mobile/app/src/chat/native/cards/TerminalPickerCard.tsx.

use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{Value, json};
use std::time::Duration;
use web_time::Instant;

/// The picker's search field, kept while the same picker stays open.
pub(super) struct TerminalPickerInput {
    identity: String,
    pub(super) input: Entity<InputState>,
    /// When the user last typed, so a search the agent has not repainted yet is not overwritten.
    edited: Option<Instant>,
    /// Set while the field is being given the agent's search text, which is not the user typing.
    applying: bool,
    _subscription: Subscription,
}

/// How long the field trusts its own typing over the agent's repaint.
const TYPING_GRACE: Duration = Duration::from_millis(1500);

impl NativeChatView {
    fn picker_presentation(&self) -> Option<&Value> {
        (self.snapshot["noticeVisible"] == true)
            .then(|| &self.snapshot["terminalNotice"]["dialog"]["presentation"]["picker"])
            .filter(|picker| picker.is_object())
    }

    fn passthrough_presentation(&self) -> Option<&Value> {
        (self.snapshot["noticeVisible"] == true)
            .then(|| &self.snapshot["terminalNotice"]["passthrough"])
            .filter(|passthrough| passthrough.is_object())
    }

    /// The picker's search field, which keeps the keyboard like the chat's other fields.
    pub(super) fn terminal_picker_focus_handle(&self, cx: &gpui::App) -> Option<gpui::FocusHandle> {
        self.terminal_picker_input
            .as_ref()
            .map(|state| state.input.read(cx).focus_handle(cx))
    }

    /// A letter typed while the picker is open and the composer is empty searches the picker:
    /// the composer is blocked while the agent's list owns its input anyway.
    fn redirect_typing_to_picker(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(typed) = event
            .keystroke
            .key_char
            .clone()
            .filter(|typed| !typed.is_empty() && !typed.chars().any(char::is_control))
        else {
            return false;
        };
        if self.picker_presentation().is_none() || self.terminal_picker_input.is_none() {
            return false;
        }
        // The field takes the letter and the keyboard once this key handler is done.
        cx.defer_in(window, move |this, window, cx| {
            this.type_into_picker(&typed, window, cx)
        });
        true
    }

    fn type_into_picker(&mut self, typed: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(query_answer) = self
            .picker_presentation()
            .map(|picker| picker["queryAnswer"].clone())
        else {
            return;
        };
        let Some(state) = self.terminal_picker_input.as_mut() else {
            return;
        };
        let value = format!("{}{typed}", state.input.read(cx).value());
        state.applying = true;
        state.edited = Some(Instant::now());
        let input = state.input.clone();
        input.update(cx, |input, cx| input.set_value(value.clone(), window, cx));
        if let Some(state) = self.terminal_picker_input.as_mut() {
            state.applying = false;
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        let mut answer = query_answer;
        answer["pickerQuery"] = json!(value);
        self.invoke(json!({"type":"answer","answer":answer}), cx);
    }

    /// Up, Down, Enter and Escape for an open picker or pass-through screen. They are taken from
    /// the picker's search field, and from the composer while it is empty so a draft stays editable.
    /// Returns true when the key went to the agent.
    pub(super) fn terminal_picker_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }
        if let Some(passthrough) = self.passthrough_presentation().cloned() {
            return self.passthrough_key_down(event, &passthrough, cx);
        }
        let composer_focused_empty = self.input.as_ref().is_some_and(|input| {
            input.read(cx).focus_handle(cx).is_focused(window)
                && input.read(cx).value().trim().is_empty()
        });
        if composer_focused_empty && self.redirect_typing_to_picker(event, window, cx) {
            return true;
        }
        if modifiers.shift {
            return false;
        }
        let key = match keystroke.key.as_str() {
            "up" => "up",
            "down" => "down",
            "enter" => "enter",
            "escape" => "escape",
            "pageup" => "pageUp",
            "pagedown" => "pageDown",
            _ => return false,
        };
        if event.is_held && matches!(key, "enter" | "escape") {
            return false;
        }
        let keys = match (self.picker_presentation(), self.passthrough_presentation()) {
            (Some(picker), _) => picker["keys"].clone(),
            (None, Some(passthrough)) => passthrough["keys"].clone(),
            _ => return false,
        };
        let in_search = self
            .terminal_picker_input
            .as_ref()
            .is_some_and(|state| state.input.read(cx).focus_handle(cx).is_focused(window));
        let composer_empty = self
            .input
            .as_ref()
            .is_none_or(|input| input.read(cx).value().trim().is_empty());
        if !in_search && !composer_empty {
            return false;
        }
        let answer = &keys[key];
        if !answer.is_object() {
            return false;
        }
        self.invoke(json!({"type":"answer","answer":answer}), cx);
        true
    }

    /// While a pass-through card is up it has the chat's keyboard, composer text or not: letters,
    /// digits and other characters, the four arrows, Enter and Esc go to the agent's screen.
    /// Backspace, Tab and shortcuts stay with the chat.
    fn passthrough_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        passthrough: &Value,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let named = match keystroke.key.as_str() {
            "up" => Some("up"),
            "down" => Some("down"),
            "left" => Some("left"),
            "right" => Some("right"),
            "enter" => Some("enter"),
            "escape" => Some("escape"),
            _ => None,
        };
        let answer = match named {
            Some(_) if keystroke.modifiers.shift => return false,
            // A held Enter or Esc would answer the agent's next screen too.
            Some("enter" | "escape") if event.is_held => return true,
            Some(key) => passthrough["keys"][key].clone(),
            None => {
                let Some(typed) = keystroke
                    .key_char
                    .clone()
                    .filter(|typed| !typed.is_empty() && !typed.chars().any(char::is_control))
                else {
                    return false;
                };
                let mut answer = passthrough["textAnswer"].clone();
                answer["text"] = json!(typed);
                answer
            }
        };
        if answer.is_object() {
            self.invoke(json!({"type":"answer","answer":answer}), cx);
        }
        true
    }

    fn picker_input_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let Some(state) = self.terminal_picker_input.as_mut() else {
            return;
        };
        if state.applying {
            return;
        }
        state.edited = Some(Instant::now());
        let Some(picker) = self.picker_presentation() else {
            return;
        };
        let mut answer = picker["queryAnswer"].clone();
        answer["pickerQuery"] = json!(input.read(cx).value().to_string());
        self.invoke(json!({"type":"answer","answer":answer}), cx);
    }

    /// The search field, made once per open picker and given the keyboard when it opens.
    fn picker_search_field(
        &mut self,
        picker: &Value,
        identity: String,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = text(picker, "query");
        if self
            .terminal_picker_input
            .as_ref()
            .is_none_or(|state| state.identity != identity)
        {
            let placeholder = text(picker, "placeholder");
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .validate(|value, _| value.chars().count() <= 512)
                    .placeholder(placeholder)
                    .default_value(query.clone())
            });
            let subscription = cx.subscribe_in(&input, window, Self::picker_input_event);
            input.update(cx, |input, cx| input.focus(window, cx));
            self.terminal_picker_input = Some(TerminalPickerInput {
                identity,
                input,
                edited: None,
                applying: false,
                _subscription: subscription,
            });
        }
        let state = self.terminal_picker_input.as_mut().unwrap();
        // The agent's own search is the truth (Esc clears it there), except while the user is
        // still typing ahead of the agent's repaint.
        if state.input.read(cx).value() != query
            && state.edited.is_none_or(|at| at.elapsed() > TYPING_GRACE)
        {
            state.applying = true;
            let input = state.input.clone();
            input.update(cx, |input, cx| input.set_value(query, window, cx));
            if let Some(state) = self.terminal_picker_input.as_mut() {
                state.applying = false;
            }
        }
        let input = self.terminal_picker_input.as_ref().unwrap().input.clone();
        div()
            .w_full()
            .px(px(10.0 * p.scale))
            .py(px(6.0 * p.scale))
            .rounded(px(8.0 * p.scale))
            .border_1()
            .border_color(p.input_border)
            .bg(p.composer_background)
            .child(
                Input::new(&input)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .p_0()
                    .text_size(px(14.0 * p.scale))
                    .text_color(p.foreground)
                    .placeholder_color(p.muted.opacity(0.6))
                    .caret_color(p.foreground),
            )
            .into_any_element()
    }

    /// The mirrored `/resume` list.
    pub(super) fn terminal_picker_card(
        &mut self,
        notice: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picker = notice["dialog"]["presentation"]["picker"].clone();
        let s = p.scale;
        // Every key repaints the picker and re-stamps the notice, so the field lives as long as the
        // list stays open (render_notice drops it when the list closes).
        let identity = format!("{}:{}", text(notice, "kind"), text(&picker, "agent"));
        let mut body = vec![self.picker_search_field(&picker, identity, p, window, cx)];
        let mut list = div()
            .id("terminal-picker-rows")
            .role(gpui::Role::List)
            .flex()
            .flex_col()
            .gap(px(2.0 * s))
            .max_h(px(f32::from(window.viewport_size().height) * 0.45))
            .overflow_y_scroll();
        for (index, row) in picker["rows"].as_array().into_iter().flatten().enumerate() {
            if let Some(heading) = row["heading"].as_str() {
                list = list.child(
                    div()
                        .pt(px(if index == 0 { 0.0 } else { 6.0 } * s))
                        .px(px(10.0 * s))
                        .text_size(px(12.0 * s))
                        .text_color(p.card_muted)
                        .child(heading.to_string()),
                );
            }
            let selected = row["selected"] == true;
            let answer = row["answer"].clone();
            let title = text(row, "title");
            list = list.child(
                div()
                    .id(format!("terminal-picker-row:{index}"))
                    .role(gpui::Role::ListItem)
                    .aria_label(title.clone())
                    .chat_cursor_pointer()
                    .flex()
                    .flex_col()
                    .gap(px(1.0 * s))
                    .px(px(10.0 * s))
                    .py(px(6.0 * s))
                    .rounded(px(6.0 * s))
                    .when(selected, |row| row.bg(p.border))
                    .hover(|style| style.bg(p.border.opacity(0.6)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .text_color(p.foreground)
                            .child(
                                div()
                                    .w(px(10.0 * s))
                                    .flex_shrink_0()
                                    .text_color(p.primary)
                                    .child(if selected { "›" } else { "" }),
                            )
                            .child(div().min_w_0().truncate().child(title)),
                    )
                    .when(row["detail"].is_string(), |item| {
                        item.child(
                            div()
                                .pl(px(16.0 * s))
                                .text_size(px(12.0 * s))
                                .text_color(p.card_muted)
                                .truncate()
                                .child(text(row, "detail")),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.invoke(json!({"type":"answer","answer":answer}), cx)
                    })),
            );
        }
        body.push(list.into_any_element());
        if let Some(message) = picker["message"].as_str() {
            body.push(
                div()
                    .text_color(p.card_muted)
                    .child(message.to_string())
                    .into_any_element(),
            );
        }
        let hint = match picker["position"].as_str() {
            Some(position) => format!("{position} · {}", text(&picker, "hint")),
            None => text(&picker, "hint"),
        };
        body.push(
            div()
                .text_size(px(12.0 * s))
                .text_color(p.card_muted)
                .child(hint)
                .into_any_element(),
        );
        if let Some(error) = self.snapshot["noticeError"].as_str() {
            body.push(
                div()
                    .id("terminal-picker-error")
                    .role(gpui::Role::Alert)
                    .text_size(px(14.0 * s))
                    .text_color(p.card_muted)
                    .child(error.to_owned())
                    .into_any_element(),
            );
        }
        let actions = vec![
            self.chat_button(
                "terminal-picker-cancel".into(),
                "Cancel".into(),
                json!({"type":"answer","answer":picker["keys"]["escape"]}),
                p,
                cx,
            ),
            self.chat_button(
                "terminal-picker-pick".into(),
                if text(&picker, "title").starts_with("Fork") {
                    "Fork"
                } else {
                    "Resume"
                }
                .into(),
                json!({"type":"answer","answer":picker["keys"]["enter"]}),
                p,
                cx,
            ),
        ];
        self.status_card(
            text(&picker, "title"),
            "titlebar/terminal-2.svg",
            body,
            actions,
            p,
        )
    }

    /// An agent screen the chat cannot read: its live region, and the keys that drive it.
    pub(super) fn passthrough_card(
        &mut self,
        notice: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let passthrough = &notice["passthrough"];
        let s = p.scale;
        let mut body = Vec::new();
        if notice["detail"].is_string() {
            body.push(
                div()
                    .line_height(px(19.6 * s))
                    .child(text(notice, "detail"))
                    .into_any_element(),
            );
        }
        let screen = text(passthrough, "screen");
        if self.passthrough_screen_shown.as_deref() != Some(screen.as_str()) {
            self.passthrough_screen_scroll.scroll_to_bottom();
            self.passthrough_screen_shown = Some(screen.clone());
        }
        body.push(
            div()
                .id("terminal-passthrough-screen")
                /*
                CDXC:SessionChat 2026-11-02 WHY:
                The region showed the whole tail unbounded and wrapped, so OMP's 200-column tool approval rows broke into several lines each, the card grew past the chat's height and the transcript above it had no room left to scroll (live test, 2026-11-02). Like the phone's card (TerminalPickerCard.tsx in the phone app: at most 320 points, scrolled to its end, rows scroll sideways), the region is bounded, keeps one terminal row per line, scrolls both ways and keeps its end in view as the agent repaints, because the agent's picker is at the bottom of the tail.
                */
                .max_h(px(
                    (320.0 * s).min(f32::from(window.viewport_size().height) * 0.25)
                ))
                .overflow_scroll()
                .track_scroll(&self.passthrough_screen_scroll)
                .flex()
                .flex_col()
                .items_start()
                .rounded(px(8.0 * s))
                .border_1()
                .border_color(p.control_border.opacity(0.65))
                .bg(p.background.opacity(0.7))
                .p(px(12.0 * s))
                .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                .text_size(px(12.0 * s))
                .line_height(px(16.0 * s))
                .whitespace_nowrap()
                .child(screen)
                .into_any_element(),
        );
        body.push(
            div()
                .id("terminal-passthrough-keyboard")
                .role(gpui::Role::Status)
                .aria_label(text(passthrough, "hint"))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .text_size(px(13.0 * s))
                .text_color(p.foreground)
                .child(
                    gpui::svg()
                        .path("titlebar/keyboard.svg")
                        .size(px(14.0 * s))
                        .text_color(p.primary)
                        .flex_shrink_0(),
                )
                // The hint is longer than one row in a narrow chat, and a flex row's text does
                // not wrap on its own.
                .child(div().flex_1().min_w_0().child(text(passthrough, "hint")))
                .into_any_element(),
        );
        if let Some(error) = self.snapshot["noticeError"].as_str() {
            body.push(
                div()
                    .id("terminal-passthrough-error")
                    .role(gpui::Role::Alert)
                    .text_color(p.card_muted)
                    .child(error.to_owned())
                    .into_any_element(),
            );
        }
        // Giving the keyboard back hides the card until the agent shows another screen.
        let release = self.chat_button(
            "terminal-passthrough-release".into(),
            text(passthrough, "releaseLabel"),
            json!({"type":"dismissNotice"}),
            p,
            cx,
        );
        let mut actions: Vec<AnyElement> = [
            ("up", "↑"),
            ("down", "↓"),
            ("left", "←"),
            ("right", "→"),
            ("enter", "Enter"),
            ("escape", "Esc"),
        ]
        .into_iter()
        .map(|(key, label)| {
            self.chat_button(
                format!("terminal-passthrough-key:{key}"),
                label.into(),
                json!({"type":"answer","answer":passthrough["keys"][key]}),
                p,
                cx,
            )
        })
        .collect();
        actions.push(
            div()
                .id("terminal-passthrough-terminal")
                .role(gpui::Role::Button)
                .aria_label("Terminal View")
                .chat_cursor_pointer()
                .px(px(8.0 * s))
                .py(px(4.0 * s))
                .rounded(px(6.0 * s))
                .border_1()
                .chat_filled(p.neutral_button_fills(), true)
                .text_color(p.primary)
                .child("Terminal View")
                .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                    this.perform_composer_action("terminalView", event.position(), window, cx)
                }))
                .into_any_element(),
        );
        actions.push(release);
        self.status_card(
            text(notice, "title"),
            "titlebar/terminal-2.svg",
            body,
            actions,
            p,
        )
    }
}
