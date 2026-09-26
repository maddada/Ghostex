use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div, px, relative,
    rgb, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::json;
use std::time::Duration;

/// CDXC:SessionChat 2026-09-22 WHY: The controller echoes every asyncQuestionText edit back in a later snapshot, so the draft a snapshot carries lags what the user has typed by a round trip. Writing that echo into the input truncated the answer to the older text and, because multi-line set_value resets the selection to 0..0, threw the caret to the start every few keystrokes. The input owns what it typed; only a draft this input never reported (an option choice clearing it, the saved answer restored after load) is written back.
#[derive(Default)]
pub(super) struct AsyncAnswerEcho {
    /// Texts this input reported whose snapshot has not come back yet, oldest first.
    pub(super) sent: Vec<String>,
    /// The draft text the last snapshot carried.
    seen: String,
    pub(super) references: Vec<super::composer_references::ComposerReference>,
    pub(super) reference_draft: Option<String>,
    pub(super) pending: usize,
    pub(super) error: Option<String>,
    pub(super) hovered_image: Option<String>,
    pub(super) reference_retry: Option<gpui::Task<()>>,
    caret: Option<usize>,
    _observer: Option<gpui::Subscription>,
}

impl NativeChatView {
    pub(super) fn render_async_questions(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.snapshot["asyncQuestions"].clone();
        let question = &state["question"];
        let key = question["key"].as_str()?.to_owned();
        let s = p.scale;
        let collapsed = state["collapsed"] == true;
        let count = state["count"].as_u64().unwrap_or(1);
        let index = state["index"].as_u64().unwrap_or(0);
        let busy = state["disabled"] == true || self.async_answer_echo.pending > 0;
        let mut indicator = div().relative().size(px(16.0 * s)).flex_shrink_0();
        if state["working"] == true {
            indicator =
                indicator.child(if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
                    question_spinner(s, 0.0)
                } else {
                    div()
                        .size_full()
                        .with_throttled_animation(
                            "async-question-working",
                            Duration::from_millis(820),
                            move |ring, progress| ring.child(question_spinner(s, progress)),
                        )
                        .into_any_element()
                });
        }
        indicator = indicator.child(
            div()
                .absolute()
                .left(px(5.0 * s))
                .top(px(5.0 * s))
                .size(px(6.0 * s))
                .rounded_full()
                .bg(rgb(0xf472b6)),
        );
        let header =
            div()
                .id("async-question-header")
                .role(gpui::Role::Button)
                .text_color(p.primary)
                .aria_label(if collapsed {
                    "Expand questions from Codex"
                } else {
                    "Collapse questions from Codex"
                })
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .px(px(16.0 * s))
                .py(px(12.0 * s))
                .text_size(px(12.0 * s))
                .line_height(relative(1.4))
                .chat_cursor_pointer()
                .child(indicator)
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(if count > 1 {
                            "Questions from Codex"
                        } else {
                            "Question from Codex"
                        }),
                )
                // CDXC:SessionChat 2026-09-14 DECISION: User: remove the idle "Reply when ready" label from the Codex questions card.
                .child(div().min_w_0().flex_1().text_color(p.muted).child(
                    if state["working"] == true {
                        "Still working"
                    } else {
                        ""
                    },
                ))
                .child(
                    div()
                        .text_color(p.muted)
                        .child(format!("{}/{count}", index + 1)),
                )
                .child(
                    svg()
                        .path(if collapsed {
                            "titlebar/chevron-right.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        })
                        .size(px(16.0 * s))
                        .text_color(p.foreground),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invoke(json!({"type":"asyncQuestionToggle"}), cx)
                }));
        let mut card = div()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(16.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_background)
            .text_color(p.foreground)
            .text_size(px(13.0 * s))
            .line_height(relative(1.4))
            .child(header);
        let motion = self.disclosure_frame("async-questions", !collapsed, cx);
        if collapsed && motion.is_none() {
            return Some(card.into_any_element());
        }
        let pane_height = f32::from(self.bounds.get().size.height);
        let mut body = div()
            .id("async-question-body")
            .flex()
            .flex_col()
            .gap(px(12.0 * s))
            .max_h(px((pane_height * 0.42 - 44.0 * s)
                .max(64.0 * s)
                .min(360.0 * s)))
            .overflow_y_scroll()
            .px(px(16.0 * s))
            .pb(px(12.0 * s))
            .child(div().flex_shrink_0().child(text(question, "title")));
        let options = question["options"].as_array();
        if let Some(options) = options.filter(|options| !options.is_empty()) {
            let mut choices = div().flex().flex_col().flex_shrink_0().gap(px(6.0 * s));
            for (option_index, option) in options.iter().enumerate() {
                let selected = state["selected"].as_array().is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_u64() == Some(option_index as u64))
                });
                choices = choices.child(self.question_choice(
                    format!("async-option:{key}:{option_index}"),
                    option.as_str().unwrap_or_default().into(),
                    String::new(),
                    selected,
                    None,
                    busy,
                    json!({"type":"asyncQuestionOption","key":key,"index":option_index}),
                    p,
                    cx,
                ));
            }
            body = body.child(choices);
        }
        let answer = text(&state["draft"], "other");
        if self
            .async_answer_input
            .as_ref()
            .is_none_or(|(previous, _)| previous != &key)
        {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .submit_on_enter(true)
                    .auto_grow(1, 5)
                    .placeholder(if options.is_some_and(|options| !options.is_empty()) {
                        "Or write your own answer…"
                    } else {
                        "Write your answer…"
                    })
                    .default_value(answer.clone())
            });
            let question_key = key.clone();
            self.async_answer_subscription = Some(cx.subscribe_in(
                &input,
                window,
                move |this, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let text = input.read(cx).value().to_string();
                        if this.async_answer_echo.sent.last() == Some(&text) {
                            return;
                        }
                        this.async_answer_echo.sent.push(text.clone());
                        this.invoke(
                            json!({"type":"asyncQuestionText","key":question_key,"text":text}),
                            cx,
                        )
                    }
                    // CDXC:SessionChat 2026-09-12 DECISION: User: Enter sends a question answer; Shift+Enter inserts a newline.
                    InputEvent::PressEnter { shift: false, .. } => {
                        if this.async_answer_echo.pending == 0 {
                            this.invoke(json!({"type":"asyncQuestionSend"}), cx)
                        }
                    }
                    InputEvent::Focus => {
                        super::focus::reclaim_keyboard_focus(window);
                        cx.notify();
                    }
                    InputEvent::Blur => cx.notify(),
                    _ => {}
                },
            ));
            let observer = cx.observe_in(&input, window, |this, input, _, cx| {
                let caret = input.read(cx).cursor();
                if this.async_answer_echo.caret != Some(caret) {
                    this.async_answer_echo.caret = Some(caret);
                    cx.notify();
                }
            });
            self.async_answer_input = Some((key, input));
            self.async_answer_echo = AsyncAnswerEcho {
                sent: Vec::new(),
                seen: answer.clone(),
                pending: self.async_answer_echo.pending,
                _observer: Some(observer),
                ..Default::default()
            };
        }
        let input = self.async_answer_input.as_ref().unwrap().1.clone();
        if self.async_answer_echo.seen != answer {
            self.async_answer_echo.seen = answer.clone();
            let echo = &mut self.async_answer_echo.sent;
            match echo.iter().position(|sent| sent == &answer) {
                // The controller caught up with an edit this input made; anything typed since is still on its way there.
                Some(settled) => {
                    echo.drain(..=settled);
                }
                None => {
                    echo.clear();
                    if input.read(cx).value().as_str() != answer {
                        input.update(cx, |input, cx| input.set_value(answer, window, cx));
                    }
                }
            }
        }
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        self.sync_answer_references(p, cx);
        let caret = input.read(cx).cursor();
        let active = self.async_answer_echo.hovered_image.clone().or_else(|| {
            self.async_answer_echo
                .references
                .iter()
                .find(|reference| {
                    reference.kind == "image"
                        && reference.range.start <= caret
                        && caret <= reference.range.end
                })
                .map(|reference| reference.path.clone())
        });
        if let Some(previews) = self.render_reference_previews(
            self.async_answer_echo.references.clone(),
            self.async_answer_echo.pending as u64,
            active,
            self.async_answer_input.as_ref().map(|(key, _)| key.clone()),
            state["submitting"] == true || state["loading"] == true,
            p,
            cx,
        ) {
            body = body.child(div().flex_shrink_0().child(previews));
        }
        body = body.child(
            div()
                .id("async-answer-editor")
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(Self::click_answer_reference),
                )
                .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    this.hover_answer_reference(Some(event.position), cx)
                }))
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    if !*hovered {
                        this.hover_answer_reference(None, cx);
                    }
                }))
                .child(
                    Input::new(&input)
                        .aria_label("Your answer")
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .disabled(state["submitting"] == true || state["loading"] == true)
                        .placeholder_color(p.muted.opacity(0.6))
                        .w_full()
                        .min_w_0()
                        .min_h(px(60.0 * s))
                        .max_h(px(118.0 * s))
                        .flex_shrink_0()
                        .border_1()
                        .border_color(p.input_border)
                        .when(focused, |input| {
                            input.border_color(p.ring).shadow(vec![
                                gpui::BoxShadow::new(px(0.0), px(0.0), p.ring.opacity(0.2))
                                    .spread_radius(px(3.0 * s))
                                    .inset(),
                            ])
                        })
                        // CDXC:SessionChat 2026-09-12 DECISION: User: the async question answer textarea must be rounded too.
                        .rounded(px(12.0 * s))
                        .bg(p.background)
                        .px(px(10.0 * s))
                        .py(px(8.0 * s))
                        .text_size(px(13.0 * s))
                        .line_height(px(24.0 * s))
                        .text_color(p.foreground),
                ),
        );
        if let Some(error) = self.async_answer_echo.error.clone() {
            body = body.child(div().flex_shrink_0().text_color(rgb(0xef4444)).child(error));
        }
        if let Some(error) = state["error"].as_str().filter(|value| !value.is_empty()) {
            body = body.child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(0xef4444))
                    .child(error.to_owned()),
            );
        }
        if state["canSend"] == false {
            body =
                body.child(div().flex_shrink_0().text_color(p.muted).child(
                    "Answers are unavailable while this chat is read-only or disconnected.",
                ));
        }
        let mut actions = div()
            .flex()
            .flex_wrap()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.0 * s))
            .text_size(px(14.0 * s));
        if count > 1 {
            for (direction, label, icon, disabled) in [
                (
                    "previous",
                    "Previous question",
                    "titlebar/chevron-left.svg",
                    state["previousDisabled"] == true || self.async_answer_echo.pending > 0,
                ),
                (
                    "next",
                    "Next question",
                    "titlebar/chevron-right.svg",
                    state["nextDisabled"] == true || self.async_answer_echo.pending > 0,
                ),
            ] {
                actions = actions.child(
                    div()
                        .id(format!("async-{direction}"))
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .size(px(28.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.0 * s))
                        .when(disabled, |button| button.opacity(0.5))
                        .when(!disabled, |button| {
                            button
                                .chat_cursor_pointer()
                                .hover(|style| style.bg(p.input))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.invoke(
                                    json!({"type":"asyncQuestionNavigate","direction":direction}),
                                    cx,
                                )
                                }))
                        })
                        .child(svg().path(icon).size(px(16.0 * s)).text_color(p.primary)),
                );
            }
        }
        actions = actions
            .child(div().flex_1())
            .child(
                self.question_button(
                    "async-skip",
                    "Skip",
                    json!({"type":"asyncQuestionSkip"}),
                    busy,
                    true,
                    false,
                    p,
                    cx,
                )
                .text_color(p.primary)
                .font_weight(gpui::FontWeight::MEDIUM)
                .line_height(px(20.0 * s)),
            )
            .child(
                self.question_button(
                    "async-send",
                    if state["submitting"] == true {
                        "Sending…"
                    } else {
                        "Send answer"
                    },
                    json!({"type":"asyncQuestionSend"}),
                    busy || text(&state, "answer").trim().is_empty(),
                    false,
                    false,
                    p,
                    cx,
                )
                .font_weight(gpui::FontWeight::MEDIUM)
                .line_height(px(20.0 * s)),
            );
        body = body.child(actions);
        // The header carries its own padding, so the body eases from nothing with no gap to carry.
        card = card.child(self.disclosure_body_motion(
            "async-questions",
            motion,
            0.0,
            body.into_any_element(),
        ));
        Some(card.into_any_element())
    }
}

fn question_spinner(scale: f32, phase: f32) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let radius = 7.25 * scale;
            let at = |angle: f32| {
                bounds.center() + gpui::point(px(radius * angle.cos()), px(radius * angle.sin()))
            };
            let start = std::f32::consts::FRAC_PI_4 + std::f32::consts::TAU * phase;
            let mut path = gpui::PathBuilder::stroke(px(1.5 * scale));
            path.move_to(at(start));
            path.arc_to(
                gpui::point(px(radius), px(radius)),
                px(0.0),
                true,
                true,
                at(start + std::f32::consts::PI * 1.5),
            );
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(0xd99a62));
            }
        },
    )
    .size_full()
    .into_any_element()
}
