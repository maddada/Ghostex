//! A question form an agent wrote as a ```questions block, drawn where the block sits in its reply:
//! every question at once, choices picked with a click, a note or text box per question, and one
//! Send button. The rules (picks, Other, what is sent, the answered state) are the chat core's
//! (`questionForms`, packages/gx-chat-core/src/questions/forms.rs); this file only lays them out.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The phone draws the same form in apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx; the grammar is packages/gx-chat-core/src/transcript/question_forms.rs. Change them together.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use super::ask_card::{
    AskBodyMotion, AskOption, CHECKLIST_GLYPH, QuestionBadge, SKIPPED_HINT, ask_color,
    ask_info_hint, question_ask_head, single_line_ask_head, single_line_press_head,
};
use super::cards::SINGLE_LINE_CARD;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px, rgb,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::{Value, json};

/// The line that opens a marked form; the form's id follows it (`NATIVE_QUESTIONS_OPEN` in
/// packages/gx-chat-core/src/transcript/native_markdown.rs).
pub(super) const OPEN: &str = "\u{E000}questions:";
pub(super) const CLOSE: &str = "\u{E000}/questions";

/// One open form's note or text box.
pub(super) struct FormInput {
    pub(super) input: Entity<TextareaState>,
    _subscription: Subscription,
    /// The placeholder it shows, which follows the picks.
    placeholder: String,
    /// It holds the reader's text: typed into, or filled from the saved draft once.
    settled: bool,
    /// Its height when it was last painted, for `form_input_reveal_sync`.
    painted_height: Rc<Cell<f32>>,
}

/// The line under a broken form once Ghostex has asked the agent for it again; the phone
/// (QuestionForm.tsx) says the same.
const RESENT_NOTE: &str = "Ghostex asked the agent to send these questions again.";

fn error_color(p: &ChatAppearance) -> Hsla {
    rgb(if p.light { 0xb91c1c } else { 0xfb7185 }).into()
}

/// The form's own notes box's hint; the phone (QuestionForm.tsx) says the same.
const NOTES_PLACEHOLDER: &str = "Anything else? (optional)";

fn placeholder(question: &Value, draft: &Value) -> String {
    if question["text"] == true {
        let own = text(question, "placeholder");
        return if own.is_empty() {
            "Your answer (optional)".to_string()
        } else {
            own
        };
    }
    if draft["otherPicked"] == true {
        "What would you prefer instead? (optional)".to_string()
    } else {
        "Note (optional)".to_string()
    }
}

impl NativeChatView {
    /// Gives each open form's text boxes an input before the transcript is drawn (the segment
    /// renderer has no window to make one), keeps their placeholders in step with the picks, and
    /// drops the inputs of forms that were answered or left the transcript.
    pub(super) fn sync_form_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Holding the document's `Arc` frees `self` for the inputs without copying the forms on
        // every frame the chat draws.
        let snapshot = self.snapshot.clone();
        let forms = &snapshot["questionForms"];
        let mut wanted = HashSet::new();
        for (id, form) in forms.as_object().into_iter().flatten() {
            if form["state"] != "open" {
                continue;
            }
            for (index, question) in form["questions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let key = format!("{id}:{index}");
                let draft = &form["drafts"][index];
                wanted.insert(key.clone());
                let form_id = id.clone();
                self.sync_form_input(
                    key,
                    placeholder(question, draft),
                    text(draft, "other"),
                    id,
                    move |text| json!({"type":"formText","form":form_id,"question":index,"text":text}),
                    window,
                    cx,
                );
            }
            // The form's own notes box under the last question (see the SessionChat 2026-10-14
            // DECISION in packages/gx-chat-core/src/questions/forms.rs).
            if form["takesNotes"] == true {
                let key = format!("{id}:notes");
                wanted.insert(key.clone());
                let form_id = id.clone();
                self.sync_form_input(
                    key,
                    NOTES_PLACEHOLDER.to_string(),
                    text(form, "notes"),
                    id,
                    move |text| json!({"type":"formNotes","form":form_id,"text":text}),
                    window,
                    cx,
                );
            }
        }
        self.form_inputs.retain(|key, _| wanted.contains(key));
    }

    /// One text box of an open form: made once, its placeholder kept in step, and `change` the
    /// action each edit sends.
    #[allow(clippy::too_many_arguments)]
    fn sync_form_input(
        &mut self,
        key: String,
        hint: String,
        value: String,
        form_id: &str,
        change: impl Fn(String) -> Value + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_form_input_in(false, key, hint, value, form_id, change, window, cx)
    }

    /// The text box `key` names, from the transcript's own boxes or, while the Resources modal is
    /// drawn, from the modal's (`modal_form_inputs`: made in its window, so they never share a
    /// window with the card's).
    pub(super) fn form_input_entry(&self, key: &str) -> Option<&FormInput> {
        if self.form_in_modal.get() {
            self.modal_form_inputs.get(key)
        } else {
            self.form_inputs.get(key)
        }
    }

    /// Gives the Resources modal's form its text boxes (made with the modal's `window`), and drops
    /// the ones of a form that is not open.
    pub(super) fn sync_modal_form_inputs(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot.clone();
        let form = &snapshot["questionForms"][id];
        let mut wanted = HashSet::new();
        if form["state"] == "open" {
            for (index, question) in form["questions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let key = format!("{id}:{index}");
                let draft = &form["drafts"][index];
                wanted.insert(key.clone());
                let form_id = id.to_string();
                self.sync_form_input_in(
                    true,
                    key,
                    placeholder(question, draft),
                    text(draft, "other"),
                    id,
                    move |text| json!({"type":"formText","form":form_id,"question":index,"text":text}),
                    window,
                    cx,
                );
            }
            if form["takesNotes"] == true {
                let key = format!("{id}:notes");
                wanted.insert(key.clone());
                let form_id = id.to_string();
                self.sync_form_input_in(
                    true,
                    key,
                    NOTES_PLACEHOLDER.to_string(),
                    text(form, "notes"),
                    id,
                    move |text| json!({"type":"formNotes","form":form_id,"text":text}),
                    window,
                    cx,
                );
            }
        }
        self.modal_form_inputs.retain(|key, _| wanted.contains(key));
    }

    fn inputs_mut(&mut self, modal: bool) -> &mut std::collections::HashMap<String, FormInput> {
        if modal {
            &mut self.modal_form_inputs
        } else {
            &mut self.form_inputs
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_form_input_in(
        &mut self,
        modal: bool,
        key: String,
        hint: String,
        value: String,
        form_id: &str,
        change: impl Fn(String) -> Value + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.inputs_mut(modal).get_mut(&key) {
            if entry.placeholder != hint {
                entry.placeholder = hint.clone();
                entry
                    .input
                    .update(cx, |state, cx| state.set_placeholder(hint, window, cx));
            }
            // The saved draft is read as the form appears, so its text can land after the
            // box was made; it fills the box once, never over what the reader typed.
            if !entry.settled && !value.is_empty() {
                entry.settled = true;
                entry
                    .input
                    .update(cx, |state, cx| state.set_value(value, window, cx));
            }
            return;
        }
        let settled = !value.is_empty();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 6)
                .auto_text_direction(true)
                .placeholder(hint.clone())
                .default_value(value)
        });
        let typed_key = key.clone();
        let form_id = form_id.to_string();
        let subscription = cx.subscribe_in(
            &input,
            window,
            move |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if let Some(entry) = this.inputs_mut(modal).get_mut(&typed_key) {
                        entry.settled = true;
                    }
                    this.invoke(change(input.read(cx).value().to_string()), cx)
                }
                // Ctrl+Shift+Enter (Cmd+Shift+Enter on macOS) skips the rest and sends; Ctrl+Enter
                // (Cmd+Enter) sends the form from its note or text box.
                InputEvent::PressEnter {
                    secondary: true,
                    shift: true,
                } => this.invoke(json!({"type":"formSkipAll","form":form_id}), cx),
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.invoke(json!({"type":"formSend","form":form_id}), cx),
                // The modal's boxes keep the keyboard in the modal's own window.
                InputEvent::Focus if !modal => super::focus::reclaim_keyboard_focus(window),
                _ => {}
            },
        );
        self.inputs_mut(modal).insert(
            key,
            FormInput {
                input,
                _subscription: subscription,
                placeholder: hint,
                settled,
                painted_height: Rc::new(Cell::new(0.0)),
            },
        );
    }

    /// Paints nothing; placed after the transcript list so its paint reads the bounds the form's
    /// boxes were just painted with. When the focused box grew (a new line while typing) and its
    /// bottom, where the cursor is, went past the transcript's visible bottom while part of it was
    /// still on screen, the list scrolls by exactly that much so the line being typed stays in view.
    ///
    /// CDXC:SessionChat 2026-10-16 WHY: the list keeps its scroll position while a row grows, so a box at the transcript's bottom edge (the reader scrolled up) slid its newest line under the composer. Only a box that grew moves the list: typing in a box already in view, or one the reader scrolled away from, leaves the transcript alone. A list following its tail already keeps the bottom in view. The phone does the same (apps/mobile/app/src/chat/native/transcript/fieldReveal.ts).
    pub(super) fn form_input_reveal_sync(&self, p: &ChatAppearance) -> AnyElement {
        let list = self.list.clone();
        let boxes: Vec<_> = self
            .form_inputs
            .values()
            .map(|entry| (entry.input.clone(), entry.painted_height.clone()))
            .collect();
        // Above the fade band painted over the list's last rows (composer_scroll.rs), plus a gap.
        let fade = if crate::app::helpers::window_glass_active_for(self.main_window) {
            0.0
        } else {
            24.0
        };
        let margin = px((fade + 8.0) * p.scale);
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let viewport = list.viewport_bounds();
                for (input, painted_height) in &boxes {
                    let state = input.read(cx);
                    let bounds = state.input_bounds();
                    let height = bounds.size.height.as_f32();
                    let grew = height > painted_height.get() + 0.5 && painted_height.get() > 0.0;
                    painted_height.set(height);
                    if !grew || !state.focus_handle(cx).is_focused(window) {
                        continue;
                    }
                    let visible_bottom = viewport.bottom() - margin;
                    let overflow = bounds.bottom() - visible_bottom;
                    if overflow > px(0.0) && bounds.top() < viewport.bottom() {
                        list.scroll_by(overflow);
                        window.request_animation_frame();
                    }
                }
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    /// The form `id` names, or its source as code when the document does not carry it (a form in
    /// a subagent's transcript).
    ///
    /// CDXC:SessionChat 2026-10-11 DECISION: User: the form takes the "Clear ask, clear answer" card (ask_card.rs) like the agent's own question card: an amber tile while it waits, a green check once it is answered (that tile replaces the old Answered chip), the agent's title over a one-line detail, and a filled send button.
    pub(super) fn question_form(
        &self,
        id: &str,
        fence: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let form = &self.snapshot["questionForms"][id];
        if !form.is_object() {
            return self.text_view(
                format!("question-form-source:{id}"),
                fence.to_string(),
                &Value::Null,
                p,
                cx,
            );
        }
        let s = p.scale;
        let phase = text(form, "state");
        let enabled = phase == "open";
        let mut title = text(form, "title");
        if title.is_empty() {
            title = "Questions".to_string();
        }
        let error = text(form, "error");
        if !error.is_empty() {
            let not_shown = SINGLE_LINE_CARD
                .pill(None, Some("Not shown".to_string()), error_color(p), p)
                .into_any_element();
            let head = question_ask_head(
                CHECKLIST_GLYPH,
                QuestionBadge::None,
                title,
                "These questions could not be shown".to_string(),
                vec![not_shown],
                p,
            )
            .into_any_element();
            let body = div()
                .flex()
                .flex_col()
                .gap(px(12.0 * s))
                .child(div().text_color(error_color(p)).child(error))
                // gxserver asks the agent to send the questions again (session_chat_question_resend.rs).
                .when(form["resent"].as_bool() == Some(true), |body| {
                    body.child(div().text_color(p.card_muted).child(RESENT_NOTE))
                })
                .child(self.text_view(
                    format!("question-form-source:{id}"),
                    fence.to_string(),
                    &Value::Null,
                    p,
                    cx,
                ))
                .into_any_element();
            return self.ask_card(head, Some(body), Vec::new(), None, p);
        }
        let questions = form["questions"].as_array().cloned().unwrap_or_default();
        let other_label = text(form, "otherLabel");
        let body = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .children(questions.iter().enumerate().map(|(index, question)| {
                self.form_question(
                    id,
                    index,
                    question,
                    &form["drafts"][index],
                    &other_label,
                    enabled,
                    p,
                    cx,
                )
            }))
            .when_some(self.form_notes(id, form, enabled, p), |body, notes| {
                body.child(notes)
            })
            .into_any_element();
        if phase == "answered" {
            return self.answered_form(id, title, body, form["collapsed"] != true, p, cx);
        }
        // The form's state is a pill on the right, as "Answered" is once it folds.
        let detail = if questions.len() == 1 {
            "1 question".to_string()
        } else {
            format!("{} questions", questions.len())
        };
        // Put off For later (gx-chat-core questions/later.rs): it says so in grey instead of the
        // amber Waiting, and stays answerable.
        let later = form["later"] == true;
        let (glyph, state) = match phase.as_str() {
            "sending" => (None, "Sending…"),
            "queued" => (Some("titlebar/clock.svg"), "Queued"),
            "canceled" => (Some("titlebar/x.svg"), "Canceled"),
            // The agent withdrew it (shown in the Resources modal): it takes no answer.
            "withdrawn" => (Some("titlebar/x.svg"), "Withdrawn"),
            _ if later => (Some("titlebar/clock.svg"), "For later"),
            _ => (Some("titlebar/clock.svg"), "Waiting"),
        };
        let canceled = phase == "canceled" || phase == "withdrawn";
        let state = SINGLE_LINE_CARD
            .pill(
                glyph,
                Some(state.to_string()),
                if later || canceled {
                    p.muted
                } else {
                    ask_color(p)
                },
                p,
            )
            .into_any_element();
        let head = question_ask_head(
            CHECKLIST_GLYPH,
            QuestionBadge::None,
            title,
            detail,
            vec![state],
            p,
        )
        .into_any_element();
        let send_error = text(form, "sendError");
        let can_send = form["canSend"] == true;
        let status = match phase.as_str() {
            "answered" => "Sent to the agent.".to_string(),
            "queued" => "Queued: it goes to the agent once it's ready.".to_string(),
            // gx-chat-core `questions/unreachable_retry.rs`: the answers wait for gxserver to come back.
            "sending" if form["reconnecting"] == true => {
                "Reconnecting to Ghostex… your answers go out as soon as it's back.".to_string()
            }
            "sending" => "Sending…".to_string(),
            // Removed from Chat Resources without an answer (gx-chat-core questions/later.rs).
            "canceled" => "Canceled: it takes no answer.".to_string(),
            "withdrawn" => "Withdrawn: no answer needed.".to_string(),
            _ if !send_error.is_empty() => send_error.clone(),
            // Nothing to say: the (i) icon holds the sentence about blank questions (`ask_info_hint`).
            _ => String::new(),
        };
        let status_is_hint = status.is_empty();
        let status_color = if enabled && !send_error.is_empty() {
            error_color(p)
        } else {
            p.muted
        };
        let send_label = match phase.as_str() {
            "answered" => "Sent".to_string(),
            "queued" => "Queued".to_string(),
            _ => text(form, "submit"),
        };
        let status_slot = div()
            .flex_1()
            .min_w_0()
            .text_size(px(12.5 * s))
            .line_height(px(18.0 * s))
            .text_color(status_color)
            .when(!status.is_empty(), |slot| slot.child(status))
            .when(status_is_hint, |slot| {
                slot.child(ask_info_hint(SKIPPED_HINT, p))
            })
            .into_any_element();
        // A canceled form has no buttons: it opens to be read and takes no answer.
        let footer = if canceled {
            vec![status_slot]
        } else {
            vec![
                status_slot,
                self.later_button(id, enabled, later, p, cx),
                self.skip_all_button(id, enabled, p, cx),
                self.ask_primary_button(
                    SharedString::from(format!("question-form-send:{id}")),
                    send_label,
                    None,
                    enabled.then(|| ("Send answers".to_string(), send_chord())),
                    json!({"type":"formSend","form":id}),
                    !can_send,
                    p,
                    cx,
                ),
            ]
        };
        // CDXC:SessionChat 2026-10-11 DECISION: User: Ctrl+Enter (Cmd+Enter on macOS) presses the form's send button from anywhere in it, a note box included; the page kit (server/src/visual_pages/kit.rs) does the same. Adding Shift presses Skip all (2026-10-16, the SessionChat DECISION on `SKIPPED` in packages/gx-visual/src/questions.rs).
        let send_form = id.to_string();
        div()
            .w_full()
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter"
                    && super::keyboard::primary_held(&event.keystroke.modifiers)
                {
                    let action = if event.keystroke.modifiers.shift {
                        "formSkipAll"
                    } else {
                        "formSend"
                    };
                    this.invoke(json!({"type":action,"form":send_form}), cx);
                    cx.stop_propagation();
                }
            }))
            .child(self.ask_card(head, Some(body), footer, None, p))
            .into_any_element()
    }

    /// An answered form: one 44px line (its question tile, title and a green "Answered" pill, the shared
    /// one-line card of cards.rs, with no chevron) that opens the read-only form below it and closes it again.
    /// Whether it is open is the core's (`collapsed`, see the SessionChat 2026-10-12 DECISION in
    /// packages/gx-chat-core/src/questions/forms.rs).
    fn answered_form(
        &self,
        id: &str,
        title: String,
        body: AnyElement,
        open: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let fold_key = format!("question-form:{id}");
        let motion = self.disclosure_frame(&fold_key, open, cx);
        let answered = SINGLE_LINE_CARD
            .pill(
                Some("titlebar/check.svg"),
                Some("Answered".to_string()),
                p.success(),
                p,
            )
            .into_any_element();
        let form_id = id.to_string();
        let head = single_line_press_head(
            single_line_ask_head(title, String::new(), vec![answered], p),
            SharedString::from(format!("question-form-head:{id}")),
            if open {
                "Hide the questions"
            } else {
                "Show the questions"
            },
            open || motion.is_some(),
            p,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.invoke(json!({"type":"formToggle","form":form_id}), cx);
        }))
        .into_any_element();
        self.single_line_ask_card(
            head,
            (open || motion.is_some()).then_some(body),
            Some(AskBodyMotion {
                key: &fold_key,
                frame: motion,
            }),
            p,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn form_question(
        &self,
        id: &str,
        index: usize,
        question: &Value,
        draft: &Value,
        other_label: &str,
        enabled: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let answered = self.snapshot["questionForms"][id]["state"] == "answered";
        let skipped = draft["skipped"] == true;
        let tag = text(question, "tag");
        let detail = text(question, "detail");
        let multiple = question["multiple"] == true;
        let picked: Vec<u64> = draft["indices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect();
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .children(self.question_marker(id, &tag))
            .when(index > 0, |column| {
                column
                    .mt(px(16.0 * s))
                    .pt(px(16.0 * s))
                    .border_t_1()
                    .border_color(p.foreground.opacity(0.06))
            })
            .child(
                // CDXC:SessionChat 2026-10-11 DECISION: User picked from the looks page: a question's tag is a filled chip (no border), "b, but pls make it show like eyebrow"; and choices keep the radio or checkbox with the letter as a muted keycap on the right (look A). User (2026-10-19): "please move the Q294 to be right aligned same row as the question", so the chip sits at the right end of the title's first line and a long title wraps beside it, superseding the chip above the title. The phone (QuestionForm.tsx) and the page kit (server/src/visual_pages/kit.rs) match.
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.0 * s))
                    .w_full()
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(15.0 * s))
                            .line_height(px(22.0 * s))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .child(text(question, "title")),
                    )
                    .when(!tag.is_empty(), |header| {
                        header.child(
                            div()
                                .flex_shrink_0()
                                .mt(px(2.0 * s))
                                .px(px(7.0 * s))
                                .rounded_full()
                                .bg(p.muted.opacity(0.18))
                                .font_family(super::fonts::CHAT_MONO)
                                .text_size(px(10.5 * s))
                                .line_height(px(18.0 * s))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.muted)
                                .child(tag),
                        )
                    }),
            )
            .when(!detail.is_empty(), |column| {
                column.child(
                    div()
                        .text_size(px(13.0 * s))
                        .line_height(px(19.0 * s))
                        .text_color(p.card_muted)
                        .child(detail),
                )
            });
        if question["text"] != true {
            if multiple {
                column = column.child(
                    div()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child("Pick any that apply."),
                );
            }
            let choices = question["choices"].as_array().cloned().unwrap_or_default();
            let row = |number: usize,
                       label: String,
                       detail: String,
                       recommended: bool,
                       picked: bool,
                       other: bool| {
                self.ask_option(
                    AskOption {
                        id: format!("question-form-choice:{id}:{index}:{number}").into(),
                        label,
                        detail,
                        picked,
                        multiple,
                        // The letter the reader can refer to ("do b"), as a muted keycap at the row's end.
                        key: Some(choice_letter(number)),
                        recommended,
                        other,
                        enabled,
                        answered,
                        action: json!({"type":"formChoice","form":id,"question":index,"choice":number}),
                    },
                    p,
                    cx,
                )
            };
            for (choice_index, choice) in choices.iter().enumerate() {
                column = column.child(row(
                    choice_index,
                    text(choice, "label"),
                    text(choice, "detail"),
                    choice["recommended"] == true,
                    picked.contains(&(choice_index as u64)),
                    false,
                ));
            }
            column = column.child(row(
                choices.len(),
                other_label.to_string(),
                String::new(),
                false,
                draft["otherPicked"] == true,
                true,
            ));
        }
        let written = text(draft, "other");
        match self.form_input_entry(&format!("{id}:{index}")) {
            Some(FormInput { input, .. }) if enabled => {
                column = column.child(note_box(input, "Note", p));
            }
            _ if !written.trim().is_empty() => {
                let shown = if question["text"] == true {
                    written
                } else {
                    format!("Note: {written}")
                };
                column = column.child(sent_note(shown, p));
            }
            _ if skipped && !enabled => {
                column = column.child(
                    div()
                        .text_size(px(13.5 * s))
                        .text_color(p.muted)
                        .child("Skipped"),
                );
            }
            _ if question["text"] == true && !enabled => {
                column = column.child(
                    div()
                        .text_size(px(13.5 * s))
                        .text_color(p.muted)
                        .child("Not answered"),
                );
            }
            _ => {}
        }
        column.into_any_element()
    }
}

impl NativeChatView {
    /// "Skip all", beside the send button while the form is open: it skips every question, whatever
    /// is picked, and sends at once (the core's `skip_all`). It is a soft-fill button (see
    /// `ask_soft_button`), and its shortcut shows in a quick tooltip rather than on the button.
    fn skip_all_button(
        &self,
        id: &str,
        enabled: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        if !enabled {
            return div().into_any_element();
        }
        self.ask_soft_button(
            SharedString::from(format!("question-form-skip-all:{id}")),
            "Skip all".to_string(),
            Some(("Skip all".to_string(), skip_all_chord())),
            json!({"type":"formSkipAll","form":id}),
            p,
            cx,
        )
    }

    /// "For later" while the form waits, "Back to unanswered" once it is put off
    /// (gx-chat-core questions/later.rs, where the user's decision lives).
    fn later_button(
        &self,
        id: &str,
        enabled: bool,
        later: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        if !enabled {
            return div().into_any_element();
        }
        self.ask_soft_button(
            SharedString::from(format!("question-form-later:{id}")),
            if later {
                "Back to unanswered"
            } else {
                "For later"
            }
            .to_string(),
            None,
            json!({"type": "questionForLater", "id": id, "later": !later}),
            p,
            cx,
        )
    }

    /// The form's own notes box under the last question, set off like a further question: the
    /// box while the form is open, the sent notes once it is not, nothing for a one-question form.
    fn form_notes(
        &self,
        id: &str,
        form: &Value,
        enabled: bool,
        p: &ChatAppearance,
    ) -> Option<AnyElement> {
        if form["takesNotes"] != true {
            return None;
        }
        let s = p.scale;
        let content = match self.form_input_entry(&format!("{id}:notes")) {
            Some(FormInput { input, .. }) if enabled => note_box(input, "Notes", p),
            _ => {
                let notes = text(form, "notes");
                if notes.trim().is_empty() {
                    return None;
                }
                sent_note(format!("Notes: {notes}"), p)
            }
        };
        Some(
            div()
                .w_full()
                .min_w_0()
                .mt(px(16.0 * s))
                .pt(px(16.0 * s))
                .border_t_1()
                .border_color(p.foreground.opacity(0.06))
                .child(content)
                .into_any_element(),
        )
    }
}

/// The chords the footer's tooltips name, written the way the app's tooltips write shortcuts.
fn send_chord() -> String {
    if super::keyboard::chat_mac_keys() {
        "⌘Enter".to_string()
    } else {
        "Ctrl+Enter".to_string()
    }
}

fn skip_all_chord() -> String {
    if super::keyboard::chat_mac_keys() {
        "⌘⇧Enter".to_string()
    } else {
        "Ctrl+Shift+Enter".to_string()
    }
}

/// A question's note box, and the form's notes box: one look.
fn note_box(input: &Entity<TextareaState>, label: &'static str, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .w_full()
        .px(px(10.0 * s))
        .py(px(6.0 * s))
        .rounded(px(9.0 * s))
        .border_1()
        .border_color(p.input_border)
        .bg(if p.light { p.background } else { p.card_row })
        .child(
            Textarea::new(input)
                .aria_label(label)
                .placeholder_color(p.muted.opacity(0.75))
                .caret_color(p.foreground)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .p_0()
                .line_height(px(20.0 * s))
                .text_color(p.foreground)
                .text_size(px(13.5 * s)),
        )
        .into_any_element()
}

/// A note as a sent form shows it.
fn sent_note(shown: String, p: &ChatAppearance) -> AnyElement {
    div()
        .text_size(px(13.5 * p.scale))
        .line_height(px(19.0 * p.scale))
        .text_color(p.card_muted)
        .child(shown)
        .into_any_element()
}

/// A choice's letter: a, b, c… and numbers past z.
fn choice_letter(number: usize) -> String {
    if number < 26 {
        ((b'a' + number as u8) as char).to_string()
    } else {
        (number + 1).to_string()
    }
}
