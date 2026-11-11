//! The working card (`workingCard`, decided in packages/gx-chat-core/src/extras/working_card.rs):
//! the spark and the working or worked line, one row at the top of the composer's stack
//! (composer.rs). The pills it used to carry sit in the chat box's strip (pills_strip.rs).
//!
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! The phone draws the same card in apps/mobile/app/src/chat/native/cards/WorkingCard.tsx.

use super::{
    appearance::ChatAppearance,
    cursor::ChatCursor as _,
    quiet_work,
    state::NativeChatView,
    transcript::text,
    working_spark::{css_ease_in_out, spark},
};
use crate::app::helpers::ThrottledAnimationExt;
use crate::assets::chat_working::VISUAL;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, canvas, div, px,
    relative, svg,
};
use serde_json::Value;
use std::{cell::Cell, rc::Rc, time::Duration};

/// The line's padding above and below (unscaled).
const HEADER_PAD_Y: f32 = 6.0;

/// The gap (unscaled) between the card's bottom edge and the chat box's top edge, when nothing else
/// stacks between them (composer.rs).
///
/// CDXC:SessionChat 2026-10-23 DECISION:
/// User, pointing at the gap between the working card and the chat box: "reduce the margin here to 9px" (2026-10-18; it was 16px: the stack's 8px gap plus the box's own 8px lead-in). User (2026-10-23): "please make the gap at B match the gap at A (for all similar cases also for cards that appear above the chat view composer": every gap in the stack above the composer is the stack's 8px gap (working card to a card, card to card, last card to the box); the box adds no lead-in under a card. Supersedes "a card or notice stacked between them keeps the usual spacing". The phone matches in apps/mobile/app/src/chat/native/cards/WorkingCard.tsx.
pub(super) const WORKING_CARD_GAP: f32 = 9.0;

/// The line's height (unscaled), the pills' 22px it was sized for, so the bar keeps its height.
const LINE_HEIGHT: f32 = 22.0;

/// Where the working line is drawn: the capsule above the chat box, or the transcript's last row
/// in the quiet style's type (Status line in the chat, status_tail.rs).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkingLineLook {
    Capsule,
    Quiet,
}

/// How long a finished turn's "Worked for" line stays, in ms (`worked_line_spent`).
const WORKED_LINE_MS: i64 = 60_000;

/// Where an element was last painted, with the clip it was painted in.
pub(super) type PaintedSlot = Rc<Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>>;

/// What the card measured at its last paint.
#[derive(Default)]
pub(super) struct WorkingCardCells {
    /// The label did not fit its line, so hovering it shows the whole text.
    label_truncated: Rc<Cell<bool>>,
    /// The status row's empty lead slot as last painted, with the clip it was painted in
    /// (status_tail.rs draws the animated lead there).
    pub(super) tail_lead: PaintedSlot,
    /// The status row's line as last painted, which clips the lead.
    pub(super) tail_line: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Set when the status row painted this frame, so the lead measures where its slot and line
    /// sit in the row again (status_tail.rs).
    pub(super) tail_fresh: Rc<Cell<bool>>,
    /// The finished turn whose "Worked for" line has a wake-up set for when its minute is up.
    pub(super) worked_wake: Rc<Cell<Option<i64>>>,
    /// The slot and the line, offset from the status row's origin, with the clip the slot was
    /// painted in, as last measured.
    pub(super) tail_in_row: Rc<Cell<Option<super::status_tail::TailInRow>>>,
}

impl NativeChatView {
    /// The card shows: the transcript has rows and the core has a working (or worked) line, and a
    /// worked line has not had its minute yet (`worked_line_spent`).
    fn working_card_shown(&self) -> bool {
        self.list.item_count() > 0
            && self.snapshot["workingCard"]["visible"] == true
            && !self.worked_line_spent()
    }

    /// The status the dock's header bar carries instead of the card (work_cards.rs
    /// `work_dock_header`): the card's value when it would show above the chat box, `None` when
    /// it would not or the transcript draws it as its last row.
    pub(super) fn dock_status_card(&self) -> Option<Value> {
        let card = &self.snapshot["workingCard"];
        (card["visible"] == true && card["inTranscript"] != true && !self.worked_line_spent())
            .then(|| card.clone())
    }

    /// The turn ended more than a minute ago, so its "Worked for" line has gone (the time stays on
    /// the turn's own row in the chat). The view draws again when the minute is up.
    ///
    /// CDXC:Coordinators 2026-11-11 DECISION:
    /// User, picking on the chat box bars page: "Worked for 28s" and what changed stay for one
    /// minute after the turn ends, then go; an idle chat with a dock shows only the count, and a
    /// chat with no dock loses its status bar the same minute later.
    pub(super) fn worked_line_spent(&self) -> bool {
        let card = &self.snapshot["workingCard"];
        if card["phase"] != "worked" {
            return false;
        }
        let Some(at) = card["workedAtMs"].as_i64() else {
            return false;
        };
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as i64);
        now - at >= WORKED_LINE_MS
    }

    /// Draws the chat again when a showing "Worked for" line's minute is up, so it goes on time;
    /// once per finished turn.
    pub(super) fn wake_when_worked_line_ends(&self, cx: &mut Context<Self>) {
        let card = &self.snapshot["workingCard"];
        let Some(at) = card["workedAtMs"]
            .as_i64()
            .filter(|_| card["phase"] == "worked")
        else {
            return;
        };
        if self.working_card.worked_wake.get() == Some(at) || self.worked_line_spent() {
            return;
        }
        self.working_card.worked_wake.set(Some(at));
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as i64);
        let left = (at + WORKED_LINE_MS - now).max(0) as u64 + 50;
        cx.spawn(async move |chat, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(left))
                .await;
            let _ = chat.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// The card for the top of the composer's stack. While the scroll-to-bottom pill shows, a click
    /// anywhere on the card jumps to the bottom like the pill does.
    ///
    /// CDXC:SessionChat 2026-10-20 DECISION:
    /// User: "i want clicking anywhere on the bar that's above the composer to scroll to the bottom (when the scroll to bottom button is shown) so i dont need to click specifically on that button". With the pill hidden (already at the bottom) the card does nothing on click. The phone taps the same card (apps/mobile/app/src/chat/native/cards/WorkingCard.tsx).
    ///
    /// CDXC:SessionChat 2026-10-18 WHY:
    /// The card is a row of the composer's stack, not a transcript row: a row lives in the cached transcript view (transcript_host.rs), and the card's spark animates for as long as the agent works, so every frame of it would lay out every visible row again, the cost that view exists to avoid. Outside the list, the transcript's visible area ends above it and nothing ever scrolls under it. It stays in that row in a short conversation too; lifting it under the last message left the gap above the composer the user asked about (the removed working_lane.rs, which must not come back).
    pub(super) fn render_working_card(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.working_card_shown() {
            return None;
        }
        let card = self.snapshot["workingCard"].clone();
        let left_bottom = super::scroll_bottom::transcript_left_bottom(&self.list);
        let header = self.working_card_header(&card, WorkingLineLook::Capsule, p, cx);
        let card = div()
            .id("chat-working-card")
            .relative()
            .role(gpui::Role::Status)
            .aria_label(text(&card, "ariaLabel"))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.input_border)
            // A capsule (the 2026-10-18 DECISION in the core's extras/working_card.rs).
            .rounded_full()
            // The composer's own fill, so the card and the chat box under it always match.
            .bg(p.composer_background)
            .overflow_hidden()
            .text_color(p.card_muted)
            .when(left_bottom, |card| {
                card.chat_cursor_pointer()
                    .on_click(cx.listener(|chat, _, _, cx| chat.jump_to_bottom(cx)))
            })
            .child(header)
            .into_any_element();
        Some(card)
    }

    /// Scroll-to-bottom on its own over the transcript's bottom edge, while the reader is away from
    /// the bottom and the chat box has no pill strip to carry it (pills_strip.rs): a strip opened
    /// only for the ↓ would resize the box every time the reader scrolled up.
    pub(super) fn render_lone_scroll_bottom(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.list.item_count() == 0 || self.pills_strip_shown() {
            return None;
        }
        let pill = self.scroll_bottom_icon_pill(p, cx)?;
        let s = p.scale;
        // The column the transcript's rows use (transcript.rs), the pill at its right end.
        let column = self
            .with_transcript_width(div().w_full().max_w(px(768.0 * s)).px(px(16.0 * s)), p)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .px(px(VISUAL.padding_x * s))
                    .block_mouse_except_scroll()
                    .child(pill),
            );
        Some(
            div()
                .absolute()
                .bottom(px(super::transcript_layout::LAYOUT.end_padding * s))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(column)
                .into_any_element(),
        )
    }

    /// The card's line: the lead glyph and the label, cut with an ellipsis at its end, then a live
    /// activity's numbers.
    pub(super) fn working_card_header(
        &self,
        card: &Value,
        look: WorkingLineLook,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.working_line(card, look, true, p, cx)
    }

    /// The status line as the dock's header bar carries it (work_cards.rs `work_dock_header`): the
    /// capsule's spark and words without the capsule's own padding, cut with an ellipsis before
    /// anything beside it would be overlapped.
    pub(super) fn working_line_in_bar(
        &self,
        card: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.working_line(card, WorkingLineLook::Capsule, false, p, cx)
    }

    fn working_line(
        &self,
        card: &Value,
        look: WorkingLineLook,
        padded: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let reduced_motion = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        let kind = card["labelKind"].as_str().unwrap_or_default();
        let activity = &card["activity"];
        let lead = match look {
            WorkingLineLook::Capsule => working_line_lead(card, look, p, reduced_motion),
            // The row keeps the lead's place empty; the lead itself is drawn over it outside the
            // cached transcript (status_tail.rs).
            WorkingLineLook::Quiet => working_line_lead(card, look, p, true).map(|_| {
                let slot = self.working_card.tail_lead.clone();
                div()
                    .relative()
                    .size(px(VISUAL.spark_box * s))
                    .flex_shrink_0()
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                slot.set(Some((bounds, window.content_mask().bounds)))
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .into_any_element()
            }),
        };
        let chat = cx.entity_id();
        // A tool's label in runs, its file paths shortened (the core's `labelParts`).
        let parts = card["labelParts"]
            .as_array()
            .filter(|parts| !parts.is_empty())
            .cloned();
        let label = card["label"]
            .as_str()
            .map(|label| self.working_card_label(label.to_owned(), kind, chat, parts, look, p));
        let mut trailing = Vec::new();
        if kind == "activity" {
            if let Some(hint) = activity["hint"].as_str() {
                let hint = hint.to_owned();
                trailing.push(
                    div()
                        .id("compaction-messaging-hint")
                        .role(gpui::Role::Button)
                        .aria_label("Messaging during compaction")
                        .size(px(20.0 * s))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
                        })
                        .child(
                            svg()
                                .path("titlebar/info-circle.svg")
                                .size(px(14.0 * s))
                                .text_color(p.muted),
                        )
                        .into_any_element(),
                );
            }
            // In the transcript the progress track stays still: an animation there would draw
            // the rows again on every frame (status_tail.rs).
            trailing.extend(activity_trailing(
                activity,
                p,
                reduced_motion || look == WorkingLineLook::Quiet,
            ));
        }
        let gap = match look {
            WorkingLineLook::Capsule => VISUAL.gap,
            WorkingLineLook::Quiet => quiet_work::ICON_GAP,
        };
        div()
            .id("chat-working-card-header")
            .w_full()
            .min_w_0()
            .when(look == WorkingLineLook::Capsule && padded, |line| {
                line.min_h(px((LINE_HEIGHT + 2.0 * HEADER_PAD_Y) * s))
                    .pl(px(12.0 * s))
                    .pr(px(12.0 * s))
                    .pt(px(HEADER_PAD_Y * s))
                    .pb(px(HEADER_PAD_Y * s))
            })
            .when(look == WorkingLineLook::Quiet, |line| {
                line.line_height(px(quiet_work::LINE_HEIGHT * s))
            })
            .flex()
            .items_center()
            .gap(px(gap * s))
            .children(lead)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(gap * s))
                    .flex_1()
                    .min_w_0()
                    .children(label)
                    .children(trailing),
            )
            .into_any_element()
    }

    /// The label on one line, cut with an ellipsis at its end; when it is cut, hovering it shows the
    /// whole text in the app's tooltip. In the quiet look a tool's label takes the muted colour and
    /// its folders the quiet style's dim one, and the word is dim like a quiet line.
    fn working_card_label(
        &self,
        label: String,
        kind: &str,
        chat: gpui::EntityId,
        parts: Option<Vec<Value>>,
        look: WorkingLineLook,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let truncated = self.working_card.label_truncated.clone();
        let cut = truncated.get();
        let measured = label.clone();
        let full = label.clone();
        let named = matches!(kind, "tool" | "activity");
        let (size, color, dim) = match look {
            WorkingLineLook::Capsule if named => (13.0, p.foreground, p.muted),
            WorkingLineLook::Capsule => (VISUAL.font_size, p.muted, p.muted),
            WorkingLineLook::Quiet if named => (quiet_work::TEXT_SIZE, p.muted, p.quiet_dim()),
            WorkingLineLook::Quiet => (quiet_work::TEXT_SIZE, p.quiet_dim(), p.quiet_dim()),
        };
        let line_height = match look {
            WorkingLineLook::Capsule => relative(1.5),
            WorkingLineLook::Quiet => px(quiet_work::LINE_HEIGHT * s).into(),
        };
        div()
            .id("chat-working-card-label")
            .relative()
            .min_w_0()
            .flex_shrink(1.0)
            .text_size(px(size * s))
            .line_height(line_height)
            .text_color(color)
            .truncate()
            .when(cut, |this| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(full.clone()).build(window, cx)
                })
            })
            .child(match parts {
                // A path's folders dimmed and shortened, its file name whole.
                Some(parts) => {
                    let mut text = String::new();
                    let mut dimmed = Vec::new();
                    for part in &parts {
                        let run = part["text"].as_str().unwrap_or_default();
                        if part["dim"] == true {
                            dimmed.push((
                                text.len()..text.len() + run.len(),
                                gpui::HighlightStyle {
                                    color: Some(dim),
                                    ..Default::default()
                                },
                            ));
                        }
                        text.push_str(run);
                    }
                    gpui::StyledText::new(text)
                        .with_highlights(dimmed)
                        .into_any_element()
                }
                None => label.into_any_element(),
            })
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let style = window.text_style();
                        let font_size = style.font_size.to_pixels(window.rem_size());
                        let run = style.to_run(measured.len());
                        let width = window
                            .text_system()
                            .shape_line(measured.clone().into(), font_size, &[run], None)
                            .width;
                        let cut = width > bounds.size.width + px(0.5);
                        if truncated.replace(cut) != cut {
                            cx.notify(chat);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    /// CDXC:DelayedSend 2026-09-21 DECISION:
    /// User: clicking an armed Delayed Send or Close After Done indicator on the chat working row opens the Delayed Actions modal so they can be managed there. (2026-10-17: they ride on the working card, Q259; since 2026-10-29 in the chat box's pill strip, pills_strip.rs.)
    pub(super) fn armed_action_items(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        self.armed_actions
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| {
                let color = VISUAL.armed_color(action["id"].as_str()?)?;
                let label = action["label"].as_str()?.to_owned();
                let id = action["id"].as_str()?.to_owned();
                Some(
                    div()
                        .id(format!("chat-armed-{id}"))
                        .role(gpui::Role::Button)
                        .aria_label(format!("{label}. Manage delayed actions"))
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.8))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.host(
                                "delayedActions",
                                serde_json::json!({"type": "host", "action": "delayedActions"}),
                                cx,
                            );
                        }))
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .gap(px(VISUAL.armed_icon_gap * s))
                        .child(
                            div()
                                .size(px(VISUAL.spark_box * s))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg()
                                        .path("titlebar/clock.svg")
                                        .size(px(VISUAL.spark_size * s))
                                        .text_color(color),
                                ),
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_size(px(VISUAL.font_size * s))
                                .line_height(relative(1.5))
                                .text_color(p.foreground)
                                .child(label),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}

/// The line's lead glyph: a live activity's, the worked check, or the spark while the agent works.
///
/// CDXC:SessionChat 2026-11-08 WHY:
/// The spark's glow shows only in the capsule, whose own fill and border bound it. In the status row (Status line in the chat) there is nothing around the spark: clipped to the 20px line, the 96px glow read as a pale square behind it, and unclipped it would wash over the text around it. The phone's spark has no glow either (apps/mobile/app/src/chat/native/cards/WorkingStrip.tsx).
pub(super) fn working_line_lead(
    card: &Value,
    look: WorkingLineLook,
    p: &ChatAppearance,
    reduced_motion: bool,
) -> Option<AnyElement> {
    let s = p.scale;
    match card["labelKind"].as_str().unwrap_or_default() {
        "" => None,
        "activity" => Some(activity_lead(&card["activity"], p, reduced_motion)),
        "worked" => Some(
            div()
                .size(px(VISUAL.spark_box * s))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path("titlebar/check.svg")
                        .size(px(VISUAL.spark_size * s))
                        .text_color(p.muted),
                )
                .into_any_element(),
        ),
        _ if card["working"] == true => {
            Some(spark(p, reduced_motion, look == WorkingLineLook::Capsule))
        }
        _ => None,
    }
}

/// A live activity's lead: the loader while shells run, else the pulsing dot.
fn activity_lead(activity: &Value, p: &ChatAppearance, reduced_motion: bool) -> AnyElement {
    let s = p.scale;
    let glyph = if activity["shellsRunning"] == true {
        let glyph = svg()
            .path("titlebar/loader2.svg")
            .size(px(14.0 * s))
            .text_color(p.control_primary);
        if reduced_motion {
            glyph.into_any_element()
        } else {
            glyph
                .with_throttled_animation(
                    "chat-shells-running",
                    Duration::from_secs(1),
                    |glyph, progress| {
                        glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                            progress,
                        )))
                    },
                )
                .into_any_element()
        }
    } else {
        let dot = div().size(px(6.0 * s)).rounded_full().bg(p.control_primary);
        if reduced_motion {
            dot.into_any_element()
        } else {
            dot.with_throttled_animation(
                "chat-activity-dot",
                Duration::from_millis(1600),
                |dot, phase| {
                    let progress = css_ease_in_out(if phase < 0.5 {
                        phase * 2.0
                    } else {
                        (1.0 - phase) * 2.0
                    });
                    dot.opacity(1.0 - 0.65 * progress)
                },
            )
            .into_any_element()
        }
    };
    div()
        .size(px(VISUAL.spark_box * s))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
        .into_any_element()
}

/// A live activity's clock, tokens, percent and a small progress track after its label.
fn activity_trailing(
    activity: &Value,
    p: &ChatAppearance,
    reduced_motion: bool,
) -> Vec<AnyElement> {
    let s = p.scale;
    let numbers = |value: String| {
        div()
            .flex_shrink_0()
            .text_size(px(VISUAL.font_size * s))
            .text_color(p.muted)
            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
            .child(value)
            .into_any_element()
    };
    let mut parts = Vec::new();
    let percent = activity["percent"].as_f64();
    if percent.is_some() || activity["indeterminate"] == true {
        let primary = p.control_primary;
        let track = div()
            .relative()
            .w(px(48.0 * s))
            .h(px(4.0 * s))
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
            .bg(p.foreground.opacity(0.1));
        let fill = div().h_full().rounded_full().bg(primary);
        parts.push(if let Some(percent) = percent {
            track
                .child(fill.w(relative(percent as f32 / 100.0)))
                .into_any_element()
        } else if reduced_motion {
            track
                .flex()
                .justify_center()
                .child(fill.w(relative(0.35)))
                .into_any_element()
        } else {
            track
                .with_throttled_animation(
                    "chat-compaction-progress",
                    Duration::from_millis(1800),
                    move |track, phase| {
                        track.child(
                            div()
                                .absolute()
                                .h_full()
                                .rounded_full()
                                .bg(primary)
                                .w(relative(0.35))
                                .left(relative(-0.35 + 1.351 * css_ease_in_out(phase))),
                        )
                    },
                )
                .into_any_element()
        });
    }
    if let Some(elapsed) = activity["elapsedLabel"].as_str() {
        parts.push(numbers(elapsed.to_owned()));
    }
    if let Some(tokens) = activity["tokens"].as_str() {
        parts.push(numbers(tokens.to_owned()));
    }
    if let Some(percent) = percent {
        parts.push(numbers(format!("{percent}%")));
    }
    parts
}
