//! What the "Previous question" icon pill on the working row (chat_pills.rs) does: the jump to a
//! question it (or a Q-tag link) asks for, and the brief highlight on the form or page card it lands
//! on. Which questions are
//! unanswered, which one a click goes to and the row it sits in are the chat core's
//! (`openQuestions`, packages/gx-chat-core/src/questions/open_questions.rs); this scrolls and draws.
//!
//! CDXC:SessionChat 2026-10-14 SEE-ALSO:
//! The phone draws the same pill and highlight in apps/mobile/app/src/chat/native/transcript/Transcript.tsx and OpenQuestions.tsx; the rules are packages/gx-chat-core/src/transcript/open_questions.rs. Change them together.

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{
    AnyElement, Context, Hsla, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div,
    px, rgb,
};
use gpui_component::text::{InlineLink, InlineLinkFrame};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::Cell, collections::HashMap, rc::Rc, sync::LazyLock, time::Duration};
use web_time::Instant;

/// The question a jump landed on, highlighted for a moment, and the row the transcript still has
/// to settle once it has measured it (`settle_question_row`).
pub(super) struct QuestionFlash {
    target: String,
    /// The tag of the form question a Q-tag link named (`jump.question`).
    question: Option<String>,
    started: Instant,
    settle: Option<Settle>,
}

/// A jump's row waiting to be drawn, and where its parts were drawn. The tops come from the same
/// paint, so their differences are offsets inside the row whatever the list did to its origin.
struct Settle {
    row: usize,
    frames: u8,
    tops: Rc<SettleTops>,
}

#[derive(Default)]
struct SettleTops {
    row: Cell<Option<Pixels>>,
    card: Cell<Option<Pixels>>,
    question: Cell<Option<Pixels>>,
}

/// Room kept below a tall reply's end once it is brought into view.
const END_MARGIN: f32 = 16.0;

/// Frames a jump waits for its row and card to be drawn before it settles on the row alone.
const SETTLE_FRAMES: u8 = 6;

impl NativeChatView {
    /// The pill's text while the chat has unanswered questions.
    pub(super) fn open_question_pill_text(&self) -> Option<String> {
        let open = &self.snapshot["openQuestions"];
        (open["count"].as_u64().unwrap_or(0) > 0)
            .then(|| open["text"].as_str().map(str::to_string))
            .flatten()
    }

    /// The pill's click: the core picks the question.
    pub(super) fn previous_question(&mut self, cx: &mut Context<Self>) {
        self.invoke(json!({"type": "previousQuestion"}), cx);
    }

    /// Scrolls to the reply holding the question once per jump (the core bumps `revision`), then
    /// highlights the question for a moment.
    pub(super) fn sync_open_question_jump(&mut self, window: &mut Window) {
        let jump = &self.snapshot["openQuestions"]["jump"];
        let revision = jump["revision"].as_u64();
        match (self.open_question_jump_revision, revision) {
            // The first document this pane reads: a jump made before it opened is not replayed.
            (None, _) => self.open_question_jump_revision = Some(revision.unwrap_or(0)),
            (Some(seen), Some(revision)) if revision != seen => {
                self.open_question_jump_revision = Some(revision);
                if let Some(item) = jump["item"].as_u64() {
                    // `scroll_to` (not `scroll_to_reveal_item`) also stops a chat at the bottom
                    // from following its tail straight back (search.rs, `sync_search_scroll`).
                    self.list.scroll_to(gpui::ListOffset {
                        item_ix: item as usize,
                        offset_in_item: px(0.0),
                    });
                    self.question_flash = Some(QuestionFlash {
                        target: jump["target"].as_str().unwrap_or_default().to_string(),
                        question: jump["question"].as_str().map(str::to_string),
                        started: Instant::now(),
                        settle: Some(Settle {
                            row: item as usize,
                            frames: 0,
                            tops: Rc::default(),
                        }),
                    });
                    return window.request_animation_frame();
                }
            }
            _ => {}
        }
        self.settle_question_row();
        if let Some(flash) = &self.question_flash {
            if flash.started.elapsed() >= flash_duration() {
                self.question_flash = None;
            } else {
                window.request_animation_frame();
            }
        }
    }

    /// Once the row a jump scrolled to (top first) has been drawn, lands on the question by the
    /// rule in gx-chat-core's `OpenQuestionJump`: a row that fits stays whole, a taller one shows
    /// its end unless that hides the question's top, which then sits a margin below the top. The
    /// phone (Transcript.tsx) lands the same way.
    ///
    /// CDXC:SessionChat 2026-10-20 WHY:
    /// Inside the transcript's list a row's descendants report bounds shifted by the rows above it (the nudge from a card's own bounds overshot by a whole row, 2026-10-14), so the card's place in its row is its top minus the row's own top, both read from canvases in the same paint, never a descendant's bounds against `bounds_for_item`.
    fn settle_question_row(&mut self) {
        let Some(flash) = self.question_flash.as_mut() else {
            return;
        };
        let Some(settle) = flash.settle.as_mut() else {
            return;
        };
        let item = settle.row;
        let Some(row) = self.list.bounds_for_item(item) else {
            settle.frames += 1;
            if settle.frames > SETTLE_FRAMES {
                flash.settle = None;
            }
            return;
        };
        let tops = settle.tops.clone();
        let anchor = tops.question.get().or(tops.card.get());
        let in_row = tops.row.get().zip(anchor).map(|(row, anchor)| anchor - row);
        if in_row.is_none() && settle.frames < SETTLE_FRAMES {
            settle.frames += 1;
            return;
        }
        flash.settle = None;
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let viewport = self.list.viewport_bounds().size.height;
        let end = row.size.height - viewport + px(END_MARGIN * scale);
        if end <= px(0.0) {
            return;
        }
        let margin = px(super::scroll_bottom::question_jump_margin() * scale);
        let offset = match in_row {
            Some(in_row) => end.min((in_row - margin).max(px(0.0))),
            None => end,
        };
        self.list.scroll_to(gpui::ListOffset {
            item_ix: item,
            offset_in_item: offset,
        });
    }

    /// The jump's settle measures, if it is still measuring.
    fn settle_tops(&self) -> Option<Rc<SettleTops>> {
        let flash = self.question_flash.as_ref()?;
        Some(flash.settle.as_ref()?.tops.clone())
    }

    /// A zero-size marker at the top of the row a jump is settling, so its card's place in it can
    /// be read (`settle_question_row`).
    pub(super) fn question_row_marker(&self, index: usize) -> Option<AnyElement> {
        let flash = self.question_flash.as_ref()?;
        if flash.settle.as_ref()?.row != index {
            return None;
        }
        let tops = self.settle_tops()?;
        Some(top_marker(move |top| tops.row.set(Some(top))))
    }

    /// The same marker at the top of the form question a Q-tag jump named.
    pub(super) fn question_marker(&self, form: &str, tag: &str) -> Option<AnyElement> {
        let flash = self.question_flash.as_ref()?;
        if flash.target != form
            || tag.is_empty()
            || !flash
                .question
                .as_deref()
                .is_some_and(|question| question.eq_ignore_ascii_case(tag))
        {
            return None;
        }
        let tops = self.settle_tops()?;
        Some(top_marker(move |top| tops.question.set(Some(top))))
    }

    /// A question form or page card, with the jump's highlight around it while it lasts.
    pub(super) fn open_question_frame(
        &self,
        target: Option<&str>,
        card: AnyElement,
        p: &ChatAppearance,
    ) -> AnyElement {
        let Some(flash) = self
            .question_flash
            .as_ref()
            .filter(|flash| target == Some(flash.target.as_str()))
        else {
            return card;
        };
        let marker = self
            .settle_tops()
            .map(|tops| top_marker(move |top| tops.card.set(Some(top))));
        let s = p.scale;
        let (_, border, _) = question_tone(p.light);
        // Full strength, then fading out over the last third.
        let progress =
            flash.started.elapsed().as_secs_f32() / flash_duration().as_secs_f32().max(0.001);
        let strength = ((1.0 - progress) * 3.0).clamp(0.0, 1.0);
        div()
            .relative()
            .w_full()
            .child(card)
            .children(marker)
            // Painted, not hit-tested: a plain div with no id takes no input.
            .child(
                div()
                    .absolute()
                    .top(px(-4.0 * s))
                    .left(px(-4.0 * s))
                    .right(px(-4.0 * s))
                    .bottom(px(-4.0 * s))
                    .rounded(px(18.0 * s))
                    .border_2()
                    .border_color(border.opacity(strength))
                    .bg(border.opacity(0.08 * strength)),
            )
            .into_any_element()
    }
}

/// A zero-size canvas at its parent's top that reports where it was drawn.
fn top_marker(record: impl Fn(Pixels) + 'static) -> AnyElement {
    gpui::canvas(move |bounds, _, _| record(bounds.top()), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_0()
        .into_any_element()
}

fn flash_duration() -> Duration {
    Duration::from_millis(super::scroll_bottom::question_flash_ms())
}

/// The jump highlight's fill, border and text (`questionTone` in `scroll-bottom.json`).
pub(super) fn question_tone(light: bool) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    super::scroll_bottom::question_tone(light)
}

/// How a Q-tag pill is drawn, read from `question-tags.json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagVisual {
    radius: f32,
    padding_x: f32,
    border_width: f32,
    icon_size: f32,
    gap: f32,
    states: HashMap<String, Themed<TagTone>>,
    hover: Themed<Wash>,
}

#[derive(Deserialize)]
struct Themed<T> {
    dark: T,
    light: T,
}

impl<T> Themed<T> {
    fn get(&self, light: bool) -> &T {
        if light { &self.light } else { &self.dark }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagTone {
    text: String,
    border: String,
    border_alpha: f32,
}

#[derive(Deserialize)]
struct Wash {
    color: String,
    alpha: f32,
}

/// CDXC:SessionChat 2026-10-15 SEE-ALSO: The Q-tag pills' colours and shape come from packages/gx-chat-core/visual/question-tags.json, which the phone's Markdown.tsx and ChatPills.tsx read too; the state is gx-chat-core's `TagState` (transcript/open_questions.rs).
static TAG_VISUAL: LazyLock<TagVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/question-tags.json"
    ))
    .expect("shared question tag appearance")
});

fn hex(value: &str) -> Hsla {
    rgb(u32::from_str_radix(value.trim_start_matches('#'), 16).unwrap_or(0)).into()
}

/// A tag state's text and border colours.
fn tag_tone(state: &str, light: bool) -> Option<(Hsla, Hsla)> {
    let tone = TAG_VISUAL.states.get(state)?.get(light);
    Some((
        hex(&tone.text),
        hex(&tone.border).opacity(tone.border_alpha),
    ))
}

/// A `question` reference (a Q-tag link, gx-chat-core `question_reference`) as an outlined pill in
/// its state's colour: a ? while open, a check once answered, grey once skipped, and grey and
/// struck through once withdrawn.
pub(super) fn question_tag_link(reference: &Value, p: &ChatAppearance) -> Option<InlineLink> {
    let state = reference["state"].as_str()?;
    let (color, border) = tag_tone(state, p.light)?;
    let visual = &*TAG_VISUAL;
    let s = p.scale;
    let icon = match state {
        "open" => "titlebar/question-mark.svg",
        "answered" => "titlebar/check-bold.svg",
        _ => "",
    };
    let wash = visual.hover.get(p.light);
    Some(InlineLink {
        label: reference["label"].as_str()?.to_owned().into(),
        title: reference["title"].as_str()?.to_owned().into(),
        icon: icon.into(),
        icon_size: px(if icon.is_empty() {
            0.0
        } else {
            visual.icon_size * s
        }),
        gap: px(if icon.is_empty() { 0.0 } else { visual.gap * s }),
        color,
        truncate_start: false,
        frame: Some(InlineLinkFrame {
            padding_x: px(visual.padding_x * s),
            border_width: px(visual.border_width),
            border_color: border,
            radius: px(visual.radius * s),
            hover_background: Some(hex(&wash.color).opacity(wash.alpha)),
            background: None,
        }),
        font_weight: Some(gpui::FontWeight::SEMIBOLD),
        strikethrough: state == "withdrawn",
        ..Default::default()
    })
}
