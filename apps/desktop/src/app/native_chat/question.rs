use super::ask_card::{
    AskBodyMotion, AskOption, CHECKLIST_GLYPH, QuestionBadge, chevron, press_head, progress_dots,
    question_ask_head,
};
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use crate::app::window::native_modal_kit::app_vertical_scrollbar;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
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
        if prompt["kind"] == "approval" {
            return Some(self.render_approval(&prompt, p, cx));
        }
        let index = self.snapshot["questionCard"]["questionIndex"]
            .as_u64()
            .unwrap_or(0) as usize;
        let count = prompt["questions"].as_array().map(Vec::len).unwrap_or(0);
        let question = &prompt["questions"][index];
        let draft = self.snapshot["questionCard"]["drafts"][index].clone();
        let header = text(question, "header");
        let collapse_key = format!("question:{}", prompt);
        let collapsed = self.collapsed.contains(&collapse_key);
        let motion = self.disclosure_frame(&collapse_key, !collapsed, cx);
        let multiple = question["multiSelect"] == true;
        let max_h = question_card_max_height(self.bounds.get().size.height, window);
        let has_previews = question["options"].as_array().is_some_and(|options| {
            options
                .iter()
                .any(|option| !text(option, "preview").is_empty())
        });
        let options_scroll = self
            .nested_scrolls
            .borrow_mut()
            .entry(format!("question-options:{}", prompt_hash(&collapse_key, index)).into())
            .or_default()
            .clone();
        let mut options = div()
            .id("question-options")
            .track_scroll(&options_scroll)
            .flex()
            .flex_col()
            .gap(px(8.0 * s))
            .max_h(max_h * 0.45)
            .overflow_y_scroll()
            // Clear of the thumb, which sits over the list's right edge.
            .when(has_previews, |options| options.pr(px(12.0 * s)));
        for (option_index, option) in question["options"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            // A multi-select note is sent beside the picks, so they stay visible while it is typed.
            let selected = (multiple || text(&draft, "other").trim().is_empty())
                && draft["indices"].as_array().is_some_and(|indices| {
                    indices
                        .iter()
                        .any(|i| i.as_u64() == Some(option_index as u64))
                });
            options = options.child(self.ask_option(
                AskOption {
                    id: format!("question-option:{index}:{option_index}").into(),
                    label: text(option, "label"),
                    detail: text(option, "description"),
                    detail_view: None,
                    picked: selected,
                    multiple,
                    key: (option_index < 9).then(|| (option_index + 1).to_string()),
                    recommended: false,
                    other: false,
                    enabled: !busy,
                    answered: false,
                    action: json!({"type":"questionOption","index":option_index}),
                },
                p,
                cx,
            ));
            // The mockup Claude shows beside the option, under the row that picks it.
            let preview = text(option, "preview");
            if !preview.is_empty() {
                options = options.child(self.question_option_preview(
                    format!(
                        "question-option-preview:{}:{option_index}",
                        prompt_hash(&collapse_key, index)
                    ),
                    &preview,
                    p,
                ));
            }
        }
        let question_scroll = self
            .nested_scrolls
            .borrow_mut()
            .entry(format!("question-text:{}", prompt_hash(&collapse_key, index)).into())
            .or_default()
            .clone();
        let body = div()
            .flex()
            .flex_col()
            .min_h_0()
            .gap(px(12.0 * s))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .min_w_0()
                    .child(
                        div()
                            .id("question-text")
                            .track_scroll(&question_scroll)
                            .overflow_y_scroll()
                            .min_h_0()
                            .min_w_0()
                            // Clear of the thumb, which sits over the text's right edge.
                            .pr(px(10.0 * s))
                            .text_size(px(15.0 * s))
                            .line_height(px(22.0 * s))
                            .text_color(p.foreground)
                            .child(text(question, "question")),
                    )
                    .child(app_vertical_scrollbar(&question_scroll).id("question-text-scrollbar")),
            )
            .when(multiple, |body| {
                body.child(
                    div()
                        .flex_shrink_0()
                        .mt(px(-6.0 * s))
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child("Select one or more options."),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(options)
                    .child(
                        app_vertical_scrollbar(&options_scroll).id("question-options-scrollbar"),
                    ),
            )
            .when(
                self.snapshot["questionCard"]["reconnecting"] == true,
                |body| body.child(reconnecting_note(p)),
            )
            .into_any_element();
        let mut footer = Vec::new();
        if index > 0 {
            footer.push(
                self.question_button(
                    "question-back",
                    "←",
                    json!({"type":"questionBack"}),
                    busy,
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
                    TextareaState::new(window, cx)
                        .submit_on_enter(true)
                        .auto_grow(1, 4)
                        .auto_text_direction(true)
                        .placeholder("Or write your own answer…")
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
            footer.push(
                div()
                    .flex_1()
                    .min_w(px(160.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .px(px(10.0 * s))
                    .py(px(4.0 * s))
                    .rounded(px(9.0 * s))
                    .border_1()
                    .border_color(p.input_border)
                    .bg(if p.light { p.background } else { p.card_row })
                    .child(
                        gpui::svg()
                            .path("titlebar/pencil.svg")
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        Textarea::new(&self.answer_input.as_ref().unwrap().1)
                            .aria_label("Your answer")
                            .disabled(busy)
                            .placeholder_color(p.muted.opacity(0.75))
                            .caret_color(p.foreground)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .p_0()
                            .line_height(px(22.0 * s))
                            .text_color(p.foreground)
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.5 * s)),
                    )
                    .into_any_element(),
            );
        } else {
            footer.push(div().min_w_0().flex_1().into_any_element());
        }
        // CDXC:SessionChat 2026-09-16 DECISION: User: the question's X was the interrupt, an action rather than a dismiss, and sat beside the collapse chevron; it is a Cancel button in the footer instead. The approval keeps its circled X since it has no chevron (approval.rs).
        footer.push(
            self.question_button(
                "question-cancel",
                "Cancel",
                json!({"type":"questionCancel"}),
                busy,
                false,
                p,
                cx,
            )
            .into_any_element(),
        );
        footer.push(self.ask_primary_button(
            "question-next",
            text(&self.snapshot["questionCard"]["controls"], "label"),
            Some("↵".to_string()),
            None,
            json!({"type":"questionNext"}),
            busy || self.snapshot["questionCard"]["controls"]["disabled"] == true,
            p,
            cx,
        ));
        // CDXC:SessionChat 2026-10-11 DECISION: User picked "Clear ask, clear answer": the header names the ask ("Needs your answer") over the question's own header and its place ("Auth method · 1 of 2"), and short progress bars sit by the chevron while the card is open (they replace the 2026-09-21 "question N of M" counter there). Folded, the detail line shows the question instead.
        let detail = if collapsed {
            text(question, "question")
        } else if count > 1 {
            if header.is_empty() {
                format!("{} of {count}", index + 1)
            } else {
                format!("{header} · {} of {count}", index + 1)
            }
        } else {
            header
        };
        let mut trailing = Vec::new();
        if count > 1 && !collapsed {
            trailing.push(progress_dots(index, count, p));
        }
        trailing.push(chevron(!collapsed, p));
        let shows_body = !collapsed || motion.is_some();
        let toggle_key = collapse_key.clone();
        let head = press_head(
            question_ask_head(
                CHECKLIST_GLYPH,
                QuestionBadge::Asking,
                "Needs your answer".to_string(),
                detail,
                trailing,
                p,
            ),
            "question-header",
            if collapsed {
                "Show the question and its options"
            } else {
                "Hide the question and its options"
            },
            true,
            p,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if !this.collapsed.remove(&toggle_key) {
                this.collapsed.insert(toggle_key.clone());
            }
            cx.notify();
        }))
        .into_any_element();
        Some(self.capped_ask_card(
            head,
            shows_body.then_some(body),
            footer,
            Some(AskBodyMotion {
                key: &collapse_key,
                frame: motion,
            }),
            max_h,
            p,
        ))
    }

    /// CDXC:SessionChat 2026-10-14 DECISION:
    /// User: "please make the buttons look the same" (2026-10-09, no borderless text buttons), then "always use filled buttons everywhere" (CDXC:AppModal 2026-10-14 on `neutral_button_fills`). Every question, async-question and approval footer button (Back, Cancel, Skip included) takes the chat's neutral fill instead of the 2026-10-09 control border; the ghost variant stays gone here and in the phone's `CardButton` (apps/mobile/app/src/chat/native/cards/primitives.tsx).
    pub(super) fn question_button(
        &self,
        id: &'static str,
        label: &str,
        action: serde_json::Value,
        disabled: bool,
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
            .chat_filled(p.neutral_button_fills(), !disabled)
            .text_color(p.foreground)
            .font_weight(gpui::FontWeight::NORMAL)
            .when(wide, |button| button.min_w(px(96.0 * s)))
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| button.chat_cursor_pointer())
            .child(label.to_owned())
            .when(!disabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
            })
    }
}

/// The tallest an option's preview grows before it scrolls: 10 lines of its 18px line height, so an
/// option and its whole preview box fit in the options' share of the card.
const PREVIEW_MAX_LINES: f32 = 10.0;

impl NativeChatView {
    /// An option's preview (Claude's `AskUserQuestion` mockup): monospace lines kept exactly as
    /// written, up to `PREVIEW_MAX_LINES` tall, then its own scroll with a scrollbar that stays
    /// shown; long lines never wrap and scroll sideways, so a mockup's columns stay lined up.
    ///
    /// CDXC:SessionChat 2026-11-09 WHY: the options list is a height-capped flex column, and a
    /// box that scrolls has no minimum height in it, so a preview was squeezed to a row or two
    /// (GitHub #524). It must keep `flex_shrink_0`, and the list scrolls instead.
    fn question_option_preview(
        &self,
        key: String,
        preview: &str,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let handle = self
            .nested_scrolls
            .borrow_mut()
            .entry(key.clone().into())
            .or_default()
            .clone();
        let preview = preview.trim_end_matches('\n');
        // Lines never wrap, so their count says whether the box scrolls down; a box that fits
        // would otherwise show a vertical bar over a fraction of a pixel.
        let tall = preview.lines().count() as f32 > PREVIEW_MAX_LINES;
        let lines = preview.lines().map(|line| {
            // An empty line keeps its height.
            div().child(if line.is_empty() {
                " ".to_string()
            } else {
                line.replace('\t', "    ")
            })
        });
        let wheel = handle.clone();
        div()
            .relative()
            .flex_shrink_0()
            .ml(px(28.0 * s))
            .rounded(px(8.0 * s))
            .bg(p.card_row)
            .overflow_hidden()
            .child(
                div()
                    .id(gpui::SharedString::from(key))
                    .track_scroll(&handle)
                    .overflow_scroll()
                    // A row that doesn't stretch it, so the lines' column keeps its widest line's
                    // width and all its lines' height: that is what the box measures to scroll.
                    .flex()
                    .items_start()
                    .max_h(px((PREVIEW_MAX_LINES * 18.0 + 16.0) * s))
                    .font_family(super::fonts::CHAT_MONO)
                    .text_size(px(12.6 * s))
                    .line_height(px(18.0 * s))
                    .text_color(p.muted)
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .px(px(10.0 * s))
                            .py(px(8.0 * s))
                            .children(lines),
                    ),
            )
            .child(
                if tall {
                    gpui_component::scroll::Scrollbar::new(&handle)
                } else {
                    gpui_component::scroll::Scrollbar::horizontal(&handle)
                }
                .mode(gpui_component::scroll::ScrollbarMode::Always),
            )
            .child(
                // The wheel moves the preview before the options list around it (GPUI hands a
                // wheel to every scroller under the pointer), and goes on to the list once the
                // preview is at its end, as `nested_scroll` does for the transcript's boxes.
                gpui::canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
                        let handle = wheel.clone();
                        window.on_mouse_event(
                            move |event: &gpui::ScrollEvent, phase, window, cx| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !hitbox.should_handle_scroll(window)
                                {
                                    return;
                                }
                                let delta = event.delta.pixel_delta(window.line_height());
                                let offset = handle.offset();
                                let max = handle.max_offset();
                                let next = gpui::point(
                                    (offset.x + delta.x).clamp(-max.x, px(0.0)),
                                    (offset.y + delta.y).clamp(-max.y, px(0.0)),
                                );
                                if next == offset {
                                    return;
                                }
                                handle.set_offset(next);
                                cx.stop_propagation();
                                window.refresh();
                            },
                        );
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// CDXC:SessionChat 2026-10-23 DECISION: User: "please make the height of this limited and the text in the middle scrollable. right now it sometimes becomes super tall and i can't click on the buttons at the bottom." The card stands in for the composer, so it may take at most four fifths of the chat pane's height, leaving the rest to the transcript above it; its head, options and footer keep their height and only the question text scrolls in what is left. The options keep their own scroll at under half the card. The phone caps its card the same way (apps/mobile/app/src/chat/native/cards/QuestionCard.tsx).
fn question_card_max_height(pane: gpui::Pixels, window: &Window) -> gpui::Pixels {
    // The pane is measured after the first frame; until then the window stands in for it.
    let room = if pane > px(0.0) {
        pane
    } else {
        window.viewport_size().height
    };
    room * 0.8
}

/// The question text's scroll offset is kept per question, so the next one starts at its top.
fn prompt_hash(prompt: &str, index: usize) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    prompt.hash(&mut hasher);
    index.hash(&mut hasher);
    hasher.finish()
}

/// The line a question or approval card shows while its answer waits for gxserver to come back
/// (gx-chat-core `questions/unreachable_retry.rs`).
pub(super) fn reconnecting_note(p: &ChatAppearance) -> AnyElement {
    div()
        .flex_shrink_0()
        .text_size(px(12.0 * p.scale))
        .line_height(px(17.0 * p.scale))
        .text_color(p.muted)
        .child("Reconnecting to Ghostex… your answer goes out as soon as it's back.")
        .into_any_element()
}
