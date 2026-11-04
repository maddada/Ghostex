//! The working card (`workingCard`, decided in packages/gx-chat-core/src/extras/working_card.rs):
//! the spark and the working or worked line, one row at the top of the composer's stack
//! (composer.rs). The pills it used to carry sit in the chat box's strip (pills_strip.rs).
//!
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! The phone draws the same card in apps/mobile/app/src/chat/native/cards/WorkingCard.tsx.

use super::{
    appearance::ChatAppearance,
    cursor::ChatCursor as _,
    state::NativeChatView,
    transcript::text,
    working_spark::{css_ease_in_out, spark},
};
use crate::app::helpers::ThrottledAnimationExt;
use crate::assets::chat_working::VISUAL;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, relative, svg,
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

/// What the card measured at its last paint.
#[derive(Default)]
pub(super) struct WorkingCardCells {
    /// The label did not fit its line, so hovering it shows the whole text.
    label_truncated: Rc<Cell<bool>>,
}

impl NativeChatView {
    /// The card shows: the transcript has rows and the core has a working (or worked) line.
    fn working_card_shown(&self) -> bool {
        self.list.item_count() > 0 && self.snapshot["workingCard"]["visible"] == true
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
        let header = self.working_card_header(&card, p, cx);
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
        let column = div()
            .w_full()
            .max_w(px(768.0 * s))
            .px(px(16.0 * s))
            .when_some(p.transcript_width, |this, width| {
                this.max_w(relative(1.0)).w(relative(width))
            })
            .when_some(self.held_text_width, |this, width| {
                this.flex_shrink_0().w(px(width)).max_w(px(width))
            })
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
    fn working_card_header(
        &self,
        card: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let reduced_motion = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        let kind = card["labelKind"].as_str().unwrap_or_default();
        let activity = &card["activity"];
        let lead = match kind {
            "" => None,
            "activity" => Some(activity_lead(activity, p, reduced_motion)),
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
            _ if card["working"] == true => Some(spark(p, reduced_motion)),
            _ => None,
        };
        let chat = cx.entity_id();
        // A tool's label in runs, its file paths shortened (the core's `labelParts`).
        let parts = card["labelParts"]
            .as_array()
            .filter(|parts| !parts.is_empty())
            .cloned();
        let label = card["label"]
            .as_str()
            .map(|label| self.working_card_label(label.to_owned(), kind, chat, parts, p));
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
            trailing.extend(activity_trailing(activity, p, reduced_motion));
        }
        div()
            .id("chat-working-card-header")
            .w_full()
            .min_w_0()
            .min_h(px((LINE_HEIGHT + 2.0 * HEADER_PAD_Y) * s))
            .pl(px(12.0 * s))
            .pr(px(12.0 * s))
            .pt(px(HEADER_PAD_Y * s))
            .pb(px(HEADER_PAD_Y * s))
            .flex()
            .items_center()
            .gap(px(VISUAL.gap * s))
            .children(lead)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(VISUAL.gap * s))
                    .flex_1()
                    .min_w_0()
                    .children(label)
                    .children(trailing),
            )
            .into_any_element()
    }

    /// The label on one line, cut with an ellipsis at its end; when it is cut, hovering it shows the
    /// whole text in the app's tooltip.
    fn working_card_label(
        &self,
        label: String,
        kind: &str,
        chat: gpui::EntityId,
        parts: Option<Vec<Value>>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let truncated = self.working_card.label_truncated.clone();
        let cut = truncated.get();
        let measured = label.clone();
        let full = label.clone();
        let (size, color) = match kind {
            "tool" | "activity" => (13.0, p.foreground),
            _ => (VISUAL.font_size, p.muted),
        };
        div()
            .id("chat-working-card-label")
            .relative()
            .min_w_0()
            .flex_shrink(1.0)
            .text_size(px(size * s))
            .line_height(relative(1.5))
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
                                    color: Some(p.muted),
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
