use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::json;

impl NativeChatView {
    pub(crate) fn render_prompt(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.snapshot["questionCard"]["visible"] != true {
            self.collapsed.retain(|key| !key.starts_with("question:"));
            self.answer_input = None;
            self.answer_subscription = None;
            return None;
        }
        if self.snapshot["questionCard"]["loading"] == true {
            return Some(self.status_card(
                "Question".into(),
                "titlebar/help-circle.svg",
                vec![div().child("Restoring your answer…").into_any_element()],
                vec![],
                p,
            ));
        }
        let prompt = self.snapshot["prompt"].clone();
        let s = p.scale;
        let busy = self.snapshot["questionCard"]["busy"] == true;
        let mut body = Vec::new();
        let mut actions = Vec::new();
        if prompt["kind"] == "approval" {
            return Some(self.render_approval(&prompt, p, cx));
        }
        let index = self.snapshot["questionCard"]["questionIndex"]
            .as_u64()
            .unwrap_or(0) as usize;
        let count = prompt["questions"].as_array().map(Vec::len).unwrap_or(0);
        let question = &prompt["questions"][index];
        let draft = self.snapshot["questionCard"]["drafts"][index].clone();
        let title = question["header"]
            .as_str()
            .unwrap_or(if count > 1 { "Questions" } else { "Question" })
            .to_string();
        body.push(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(8.0 * s))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_color(p.card_muted)
                        .line_height(px(19.6 * s))
                        .child(text(question, "question")),
                )
                .when(count > 1, |this| {
                    this.child(div().flex_shrink_0().text_size(px(12.0 * s)).child(format!(
                        "question {} of {}",
                        index + 1,
                        count
                    )))
                })
                .into_any_element(),
        );
        if question["multiSelect"] == true {
            body.push(
                div()
                    .text_size(px(12.0 * s))
                    .child("Select one or more options.")
                    .into_any_element(),
            );
        }
        let mut choices = div()
            .id("question-options")
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .max_h(window.viewport_size().height * 0.45)
            .overflow_y_scroll();
        for (option_index, option) in question["options"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let selected = text(&draft, "other").trim().is_empty()
                && draft["indices"].as_array().is_some_and(|indices| {
                    indices
                        .iter()
                        .any(|i| i.as_u64() == Some(option_index as u64))
                });
            let label = text(option, "label");
            let description = text(option, "description");
            choices = choices.child(self.question_choice(
                format!("question-option:{index}:{option_index}"),
                label,
                description,
                selected,
                (option_index < 9).then_some(option_index + 1),
                busy,
                json!({"type":"questionOption","index":option_index}),
                p,
                cx,
            ));
        }
        body.push(choices.into_any_element());
        if index > 0 {
            actions.push(
                self.question_button(
                    "question-back",
                    "←",
                    json!({"type":"questionBack"}),
                    busy,
                    true,
                    false,
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        if question["allowCustom"] != false {
            let key = format!("{}:{index}", prompt);
            if self
                .answer_input
                .as_ref()
                .is_none_or(|(previous, _)| previous != &key)
            {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .multi_line(true)
                        .submit_on_enter(true)
                        .auto_grow(1, 4)
                        .placeholder("Write a custom answer…")
                        .default_value(text(&draft, "other"))
                });
                self.answer_subscription = Some(cx.subscribe_in(&input,window,|this,input,event:&InputEvent,window,cx| match event {
                    InputEvent::Change => this.invoke(json!({"type":"questionText","text":input.read(cx).value().to_string()}),cx),
                    InputEvent::PressEnter { shift:false,.. } => this.invoke(json!({"type":"questionNext"}),cx),
                    InputEvent::Focus => super::focus::reclaim_keyboard_focus(window),
                    _=>{},
                }));
                self.answer_input = Some((key, input));
            }
            actions.push(
                Input::new(&self.answer_input.as_ref().unwrap().1)
                    .aria_label("Your answer")
                    .disabled(self.snapshot["questionCard"]["busy"] == true)
                    .placeholder_color(p.muted.opacity(0.6))
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .p_0()
                    .line_height(px(24.0 * s))
                    .text_color(p.foreground)
                    .flex_1()
                    .min_w_0()
                    .text_size(px(14.0 * s))
                    .into_any_element(),
            );
        } else {
            actions.push(div().min_w_0().flex_1().into_any_element());
        }
        actions.push(
            self.question_button(
                "question-cancel",
                "Cancel",
                json!({"type":"questionCancel"}),
                busy,
                true,
                false,
                p,
                cx,
            )
            .into_any_element(),
        );
        actions.push(
            self.question_button(
                "question-next",
                &text(&self.snapshot["questionCard"]["controls"], "label"),
                json!({"type":"questionNext"}),
                busy || self.snapshot["questionCard"]["controls"]["disabled"] == true,
                false,
                true,
                p,
                cx,
            )
            .into_any_element(),
        );
        let collapse_key = format!("question:{}", prompt);
        let collapsed = self.collapsed.contains(&collapse_key);
        let header = div()
            .id("question-header")
            .role(gpui::Role::Button)
            .aria_label(if collapsed {
                "Show the question and its options"
            } else {
                "Hide the question and its options"
            })
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .chat_cursor_pointer()
            .mx(px(-16.0 * s))
            .mt(px(-12.0 * s))
            .mb(px(if collapsed { -12.0 } else { -2.0 } * s))
            .px(px(16.0 * s))
            .pt(px(12.0 * s))
            .pb(px(if collapsed { 12.0 } else { 6.0 } * s))
            .hover(|style| style.bg(p.foreground.opacity(0.04)))
            .child(
                gpui::svg()
                    .path("titlebar/help-circle.svg")
                    .size(px(14.0 * s))
                    .mt(px(4.375 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .gap(px(8.0 * s))
                    .text_color(p.foreground)
                    .child(div().flex_shrink_0().child(title))
                    .when(collapsed, |heading| {
                        heading.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(p.muted)
                                .child(text(question, "question")),
                        )
                    }),
            )
            .child(
                gpui::svg()
                    .path(if collapsed {
                        "titlebar/chevron-right.svg"
                    } else {
                        "titlebar/chevron-down.svg"
                    })
                    .size(px(14.0 * s))
                    .mt(px(4.375 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.collapsed.remove(&collapse_key) {
                    this.collapsed.insert(collapse_key.clone());
                }
                cx.notify();
            }))
            .into_any_element();
        Some(self.status_card_with_header(
            header,
            if collapsed { vec![] } else { body },
            actions,
            p,
        ))
    }

    pub(super) fn question_button(
        &self,
        id: &'static str,
        label: &str,
        action: serde_json::Value,
        disabled: bool,
        ghost: bool,
        wide: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.to_owned())
            .h(px(28.0 * s))
            .px(px(12.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(if ghost {
                gpui::transparent_black()
            } else {
                p.control_border
            })
            .text_color(p.foreground)
            .font_weight(gpui::FontWeight::NORMAL)
            .when(wide, |button| button.min_w(px(96.0 * s)))
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.input))
            })
            .when(!ghost && p.light, |button| button.bg(p.background))
            .child(label.to_owned())
            .when(!disabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
            })
    }
}
