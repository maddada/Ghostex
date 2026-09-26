use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, AppContext as _, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::json;

impl NativeChatView {
    pub(super) fn render_note(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.snapshot["note"]["open"] != true {
            self.note_input = None;
            self.note_subscription = None;
            return None;
        }
        let value = self.snapshot["note"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if self.note_input.is_none() {
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(3, 8)
                    .placeholder("What’s next in this thread…")
                    .default_value(value.clone())
            });
            self.note_subscription = Some(cx.subscribe_in(
                &input,
                window,
                |this, input, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.invoke(
                        json!({"type":"editNote","text":input.read(cx).value().to_string()}),
                        cx,
                    ),
                    InputEvent::Blur => this.invoke(json!({"type":"saveNote"}), cx),
                    _ => {}
                },
            ));
            input.read(cx).focus_handle(cx).focus(window, cx);
            self.note_input = Some(input);
        }
        let input = self.note_input.as_ref().unwrap().clone();
        if input.read(cx).value().as_ref() != value.as_str() {
            input.update(cx, |input, cx| input.set_value(value.clone(), window, cx));
        }
        let s = p.scale;
        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .px(px(12.0 * s))
                .pt(px(8.0 * s))
                .pb(px(10.0 * s))
                .border_1()
                .border_color(p.border)
                .rounded(px(16.0 * s))
                .bg(p.input)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child("Session note")
                        .child(
                            div()
                                .flex()
                                .gap(px(4.0 * s))
                                .child(
                                    div()
                                        .id("copy-note")
                                        .chat_cursor_pointer()
                                        .size(px(24.0 * s))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            gpui::svg()
                                                .path("titlebar/copy.svg")
                                                .size(px(14.0 * s)),
                                        )
                                        .on_click(move |_, _, cx| {
                                            crate::app::helpers::gpui_copy_to_clipboard(
                                                gpui::ClipboardItem::new_string(value.clone()),
                                                cx,
                                            );
                                        }),
                                )
                                .child(self.icon_command(
                                    "clear-note",
                                    "Clear note",
                                    "titlebar/eraser.svg",
                                    json!({"type":"clearNote"}),
                                    p,
                                    cx,
                                ))
                                .child(self.icon_command(
                                    "close-note",
                                    "Close session note",
                                    "titlebar/x.svg",
                                    json!({"type":"toggleNote"}),
                                    p,
                                    cx,
                                )),
                        ),
                )
                .child(
                    Textarea::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .text_size(px(13.0 * s)),
                )
                .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    let key = &event.keystroke;
                    if key.key == "escape"
                        || key.key == "enter" && (key.modifiers.platform || key.modifiers.control)
                    {
                        this.invoke(
                            json!({"type":if key.key == "escape" {"toggleNote"}else{"saveNote"}}),
                            cx,
                        );
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                }))
                .into_any_element(),
        )
    }
}
