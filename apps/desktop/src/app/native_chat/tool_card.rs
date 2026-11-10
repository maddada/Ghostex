//! The tool-call card (Settings > Chat > Tool call cards): one rounded card per burst of tool
//! calls, its header stacking the tools' icons with "Using N tools" / "Used N tools", the failures
//! and the burst's time, and a row per call in the call's own tense with its result and time on the
//! right, a shimmer on the running row and a spinner, check or mark beside it. What every row says,
//! which rows the card holds and when it folds are decided in
//! `packages/gx-chat-core/src/transcript/tool_card.rs` (the user's decision is there); this file
//! only lays the card out and moves it.
//!
//! CDXC:SessionChat 2026-11-08 DECISION: User, of a folded "Used 1 tool" card above a "Message to <thread>" card: "make these 2 please have same height, they don't right now", then "keep the 'Message to …' card's height and make the tool cards match it". The tool card's header is the one-line cards' header height and its border their one scaled pixel, so folded it is 44px tall like every one-line card (the 2026-10-18 DECISION in cards.rs); its look is otherwise unchanged.

use super::file_change_card::card_colors;
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::helpers::indicator_animation::INDICATOR_MAX_FPS;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, StyledText,
    div, px, rgb,
};
use serde_json::Value;
use std::{cell::Cell, rc::Rc, time::Duration};

/// How long a row takes to grow in, and the header's text to settle into its next value.
const ENTER: Duration = Duration::from_millis(320);
/// One sweep of the running row's shimmer.
pub(super) const SHIMMER: Duration = Duration::from_millis(1400);
/// One turn of the spinner.
pub(super) const SPIN: Duration = Duration::from_millis(800);
/// The header's icon circles and how far each overlaps the one before it.
const STACK_ICON: f32 = 22.0;
const STACK_OVERLAP: f32 = 7.0;
/// The ring cut out of the card's colour around each circle, so an overlapping circle reads as
/// sitting on top of the one before it.
const STACK_RING: f32 = 2.0;
/// A row's icon slot.
const ROW_ICON: f32 = 18.0;
/// Tool call lines: the line's height, and where the rail its rows hang from sits.
const LINE_HEIGHT: f32 = 34.0;
const LINE_RAIL_INSET: f32 = 18.0;
const LINE_RAIL_GAP: f32 = 8.0;

/// The bundled icon for each glyph the core classifies a tool into (`tool_rows::tool_glyph`).
fn glyph_icon(glyph: &str) -> &'static str {
    match glyph {
        "edit" => "titlebar/pencil.svg",
        "file" => "titlebar/file-text.svg",
        "terminal" => "titlebar/terminal-2.svg",
        "web" => "titlebar/world.svg",
        _ => "titlebar/tool.svg",
    }
}

fn hex(color: &Value) -> Option<Hsla> {
    let hex = color.as_str()?.trim_start_matches('#');
    u32::from_str_radix(hex, 16)
        .ok()
        .map(|value| rgb(value).into())
}

/// A tool's icon: its app's logo in white on the app's colour, or its glyph.
fn tool_icon(icon: &Value, size: f32, round: bool, p: &ChatAppearance) -> gpui::Div {
    let s = p.scale;
    let app = icon["app"].as_str().filter(|app| !app.is_empty());
    let tile = div()
        .size(px(size * s))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .when(round, |tile| tile.rounded_full())
        .when(!round, |tile| tile.rounded(px(5.0 * s)));
    match (app, hex(&icon["appColor"])) {
        (Some(app), Some(color)) => tile.bg(color).child(
            gpui::svg()
                .path(format!("chat-tool-apps/{app}.svg"))
                .size(px(size * if round { 0.5 } else { 0.58 } * s))
                .text_color(gpui::white()),
        ),
        _ => tile.child(
            gpui::svg()
                .path(glyph_icon(icon["glyph"].as_str().unwrap_or("tool")))
                .size(px(size * if round { 0.5 } else { 0.8 } * s))
                .text_color(if round {
                    p.foreground.opacity(0.75)
                } else {
                    p.muted
                }),
        ),
    }
}

/// `base` moved `amount` of the way to `toward`, opaque.
fn mix(base: Hsla, toward: Hsla, amount: f32) -> Hsla {
    let (from, to) = (base.to_rgb(), toward.to_rgb());
    gpui::Rgba {
        r: from.r + (to.r - from.r) * amount,
        g: from.g + (to.g - from.g) * amount,
        b: from.b + (to.b - from.b) * amount,
        a: 1.0,
    }
    .into()
}

/// The header stack's circle fill, its ring colour and the ring's width.
///
/// CDXC:SessionChat 2026-11-06 FIX:
/// Brewed Bytes (Discord #support-and-bugs, "Blacked icons for the tool calls in the new Tool call cards"): "I tried changing the theme a few times but it seems the tool call icons are still blacked. The phone side is okay." On window glass the card's surface is a translucent wash (4% black in a light theme, 6% white in a dark one, `ChatAppearance::on_window_glass`), and making it opaque for the circles turned them solid black in light themes and solid white under white icons in dark ones. On glass the circles now take an opaque frosted tone of the theme's own background (near-white in light, a lifted grey in dark, never black or near-black on glass per AGENTS.md) with a hairline ring; without glass they keep the phone's stack: the card tone stepped darker (light) or lighter (dark), cut out of the card by a ring of its colour.
fn stack_colors(surface: Hsla, p: &ChatAppearance) -> (Hsla, Hsla, f32) {
    if p.window_glass {
        let disc = if p.light {
            mix(p.background, gpui::white(), 0.7)
        } else {
            mix(p.background, gpui::white(), 0.2)
        };
        let ring = p.foreground.opacity(if p.light { 0.12 } else { 0.1 });
        return (disc, ring, 1.0);
    }
    let disc = Hsla {
        l: (surface.l + if p.light { -0.1 } else { 0.13 }).clamp(0.0, 1.0),
        a: 1.0,
        ..surface
    };
    (disc, Hsla { a: 1.0, ..surface }, STACK_RING)
}

/// `text` in `font` with a band of light sweeping across it from left to right, `phase` running
/// 0 to 1.
pub(super) fn shimmer_text(
    text: &str,
    font: &str,
    phase: f32,
    dim: Hsla,
    bright: Hsla,
) -> StyledText {
    let count = text.chars().count().max(1) as f32;
    // The band is about a quarter of the text wide and starts and ends off it.
    let band = (count * 0.25).max(4.0);
    let center = -band + phase * (count + band * 2.0);
    let runs = text
        .chars()
        .enumerate()
        .map(|(index, character)| {
            let light = (1.0 - ((index as f32 - center).abs() / band)).clamp(0.0, 1.0);
            gpui::TextRun {
                len: character.len_utf8(),
                font: gpui::font(font.to_string()),
                color: Hsla {
                    h: bright.h,
                    s: dim.s + (bright.s - dim.s) * light,
                    l: dim.l + (bright.l - dim.l) * light,
                    a: dim.a + (bright.a - dim.a) * light,
                },
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect::<Vec<_>>();
    StyledText::new(text.to_string()).with_runs(runs)
}

impl NativeChatView {
    /// The card for `message["toolCard"]`, or `None` when the message has none.
    pub(super) fn tool_card(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = message.get("toolCard").filter(|card| card.is_object())?;
        let s = p.scale;
        let id = text(message, "id");
        let key = format!("tool-card:{id}");
        let live = message["toolCardLive"] == true;
        let running = card["running"] == true;
        // One line until the reader opens it; a call waiting on an approval opens it (`waiting`).
        // While calls run the header carries the newest one's words (the core's `liveLabel`).
        // Tool call lines open in Detailed view and stay shut in Simple (Q1964).
        let line = p.tool_lines;
        let open = self.is_expanded(&key, card["waiting"] == true || (line && p.work_open));
        let motion = self.disclosure_frame(&key, open, cx);
        let animate = (running || live) && !cx.reduce_motion();
        let (surface, border, hairline, _, _, _) = card_colors(p);
        let entries = card["entries"].as_array().map_or(&[][..], Vec::as_slice);

        // Settings > Extensions > Experimental > Tool call lines: the card's header without its
        // box, and its calls as rows on the rail under it.
        if line {
            let mut column = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .child(self.tool_card_header(&key, card, open, animate, true, p, cx));
            if open || motion.is_some() {
                let rows: Vec<AnyElement> = entries
                    .iter()
                    .enumerate()
                    .map(|(position, entry)| {
                        self.tool_card_row(&id, &key, position, entry, message, animate, p, cx)
                    })
                    .collect();
                let body = div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.0 * s))
                    .mt(px(2.0 * s))
                    .ml(px(LINE_RAIL_INSET * s))
                    .pl(px(LINE_RAIL_GAP * s))
                    .border_l_1()
                    .border_color(border)
                    .children(rows)
                    .into_any_element();
                column = column.child(self.disclosure_body_motion(&key, motion, 0.0, body));
            }
            return Some(column.into_any_element());
        }

        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(12.0 * s))
            // One scaled pixel, as the one-line cards' shell has, so the folded card is exactly as
            // tall as they are (`SINGLE_LINE_CARD.header_height`).
            .border(px(s))
            .border_color(border)
            .bg(surface)
            .overflow_hidden()
            .child(self.tool_card_header(&key, card, open, animate, false, p, cx));
        if open || motion.is_some() {
            let rows: Vec<AnyElement> = entries
                .iter()
                .enumerate()
                .map(|(position, entry)| {
                    self.tool_card_row(&id, &key, position, entry, message, animate, p, cx)
                })
                .collect();
            let body = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.0 * s))
                .p(px(4.0 * s))
                .border_t_1()
                .border_color(hairline)
                .children(rows)
                .into_any_element();
            column = column.child(self.disclosure_body_motion(&key, motion, 0.0, body));
        }
        Some(column.into_any_element())
    }

    #[allow(clippy::too_many_arguments)]
    fn tool_card_header(
        &self,
        key: &str,
        card: &Value,
        open: bool,
        animate: bool,
        line: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let live_label = text(card, "liveLabel");
        let live_more = text(card, "liveMore");
        let title = if !live_label.is_empty() {
            live_label.clone()
        } else if line {
            text(card, "lineTitle")
        } else {
            text(card, "title")
        };
        let failed = text(card, "failedLabel");
        let time = text(card, "time");
        let more = card["moreIcons"].as_u64().unwrap_or(0);
        let (card_surface, _, _, hover, _, _) = card_colors(p);
        // A line has no box: its circles are cut out of the chat's own background.
        let surface = if line { p.background } else { card_surface };
        let icons = card["icons"].as_array().map_or(&[][..], Vec::as_slice);
        let mut stack = div().flex().items_center().flex_shrink_0();
        let (disc, ring_color, ring_width) = stack_colors(surface, p);
        let ring = |tile: gpui::Div, index: usize| {
            tile.border(px(ring_width * s))
                .border_color(ring_color)
                .when(index > 0, |tile| tile.ml(px(-STACK_OVERLAP * s)))
        };
        let mut tiles: Vec<gpui::Div> = icons
            .iter()
            .enumerate()
            .map(|(index, icon)| {
                ring(
                    tool_icon(icon, STACK_ICON, true, p)
                        .when(icon["app"].is_null(), |circle| circle.bg(disc)),
                    index,
                )
            })
            .collect();
        if more > 0 {
            tiles.push(ring(
                div()
                    .size(px(STACK_ICON * s))
                    .rounded_full()
                    .bg(disc)
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .text_size(px(9.5 * s))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(p.foreground.opacity(0.75))
                    .child(format!("+{more}")),
                icons.len(),
            ));
        }
        for (index, tile) in tiles.into_iter().enumerate() {
            // A circle slides out from behind the one before it as its tool starts.
            stack = stack.child(if animate {
                let (hidden, shown) = (-STACK_ICON * s, -STACK_OVERLAP * s);
                tile.with_animation(
                    SharedString::from(format!("{key}:icon:{index}")),
                    enter_animation(),
                    move |tile, delta| {
                        let tile = tile.opacity(delta);
                        if index == 0 {
                            tile
                        } else {
                            tile.ml(px(hidden + (shown - hidden) * delta))
                        }
                    },
                )
                .into_any_element()
            } else {
                tile.into_any_element()
            });
        }
        // The running call's words sweep with the row's shimmer; otherwise the header's words
        // settle into their next value ("Using 3 tools" into "Used 3 tools") while the card is
        // live, the old text giving way to the new one fading up.
        let title_element = if !live_label.is_empty() && animate {
            // Drawn over the row, not in it (row_motion.rs).
            self.running_shimmer(
                SharedString::from(format!("{key}:live")),
                div().min_w_0().truncate().text_size(px(13.5 * s)),
                live_label.clone(),
                p,
            )
        } else {
            let plain = div()
                .min_w_0()
                .truncate()
                .text_size(px(13.5 * s))
                .text_color(p.muted)
                .child(title.clone());
            if animate {
                plain
                    .with_animation(
                        SharedString::from(format!("{key}:title:{title}")),
                        enter_animation(),
                        |title, delta| title.opacity(0.2 + 0.8 * delta),
                    )
                    .into_any_element()
            } else {
                plain.into_any_element()
            }
        };
        // A spinner beside the live line while a call runs, as its row has.
        let spinner: Option<AnyElement> = (!live_label.is_empty()).then(|| {
            if animate {
                return self.running_spinner(
                    SharedString::from(format!("{key}:spin")),
                    13.0 * s,
                    p.muted,
                );
            }
            gpui::svg()
                .path("titlebar/loader2.svg")
                .size(px(13.0 * s))
                .flex_shrink_0()
                .text_color(p.muted)
                .into_any_element()
        });
        let toggle_key = key.to_string();
        let aria = [
            title.as_str(),
            live_more.as_str(),
            failed.as_str(),
            time.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        div()
            .id(SharedString::from(format!("{key}:header")))
            .role(gpui::Role::Button)
            .aria_label(aria)
            .aria_expanded(open)
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(if line { 8.0 } else { 10.0 } * s))
            // The one-line cards' header height ("Message to …", "Worked for 12s"), so a tool card
            // folded beside one of them is the same height; a line is a tool row's height.
            .h(px(if line {
                LINE_HEIGHT * s
            } else {
                super::cards::SINGLE_LINE_CARD.header_height(s)
            }))
            .when(line, |this| this.rounded(px(8.0 * s)))
            .min_w_0()
            .chat_cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(toggle_key.clone(), open, cx)
            }))
            .child(stack)
            .child(title_element)
            .child(div().flex_1())
            .when(!failed.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.error())
                        .child(failed.clone()),
                )
            })
            .when(!failed.is_empty() && !time.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child("\u{b7}"),
                )
            })
            .when(!time.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(time),
                )
            })
            // The card's other calls beside the live line ("+2 more").
            .when(!live_more.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(live_more.clone()),
                )
            })
            .children(spinner)
            .child(
                gpui::svg()
                    .path(if open {
                        "titlebar/chevron-up.svg"
                    } else {
                        "titlebar/chevron-down.svg"
                    })
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn tool_card_row(
        &mut self,
        message_id: &str,
        card_key: &str,
        position: usize,
        entry: &Value,
        message: &Value,
        animate: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let kind = text(entry, "kind");
        let index = entry["index"].as_u64().unwrap_or(0) as usize;
        let source = match kind.as_str() {
            "file" => message["files"][index].clone(),
            _ => message["tools"][index].clone(),
        };
        let key = match kind.as_str() {
            "file" => format!("file:{message_id}:{index}"),
            _ => format!("tool:{message_id}:{index}"),
        };
        let status = text(entry, "status");
        let label = text(entry, "label");
        let reason = text(entry, "reason");
        let result = text(entry, "result");
        let time = text(entry, "time");
        let (_, _, _, hover, added_color, removed_color) = card_colors(p);
        let has_detail = match kind.as_str() {
            "file" => true,
            _ => source["hasDetail"] == true,
        };
        let expanded = has_detail && self.is_expanded(&key, false);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let running = status == "running";
        let failed = status == "failed";
        let label_color = match status.as_str() {
            "failed" => p.error(),
            "stopped" => p.muted,
            _ => p.foreground,
        };
        let label_element: AnyElement = if running && animate {
            // Drawn over the row, not in it (row_motion.rs).
            self.running_shimmer(
                SharedString::from(format!("{key}:shimmer")),
                div().min_w_0().truncate().text_size(px(13.5 * s)),
                label.clone(),
                p,
            )
        } else {
            div()
                .min_w_0()
                .truncate()
                .text_size(px(13.5 * s))
                .text_color(label_color)
                .child(label.clone())
                .into_any_element()
        };
        let mut meta = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(5.0 * s))
            .text_size(px(12.0 * s))
            .text_color(p.muted);
        let mut parts = 0;
        if let (Some(added), Some(removed)) = (entry["added"].as_u64(), entry["removed"].as_u64()) {
            meta = meta
                .child(div().text_color(added_color).child(format!("+{added}")))
                .child(
                    div()
                        .text_color(removed_color)
                        .child(format!("\u{2212}{removed}")),
                );
            parts += 1;
        } else if !result.is_empty() {
            meta = meta.child(
                div()
                    .when(failed, |this| this.text_color(p.error()))
                    .child(result),
            );
            parts += 1;
        }
        if !time.is_empty() {
            if parts > 0 {
                meta = meta.child("\u{b7}");
            }
            meta = meta.child(time);
        }
        let status_icon: AnyElement = match status.as_str() {
            "running" if animate => {
                self.running_spinner(SharedString::from(format!("{key}:spin")), 13.0 * s, p.muted)
            }
            "running" => gpui::svg()
                .path("titlebar/loader2.svg")
                .size(px(13.0 * s))
                .text_color(p.muted)
                .into_any_element(),
            "waiting" => gpui::svg()
                .path("titlebar/clock.svg")
                .size(px(13.0 * s))
                .text_color(p.control_primary)
                .into_any_element(),
            "failed" => gpui::svg()
                .path("titlebar/alert-circle.svg")
                .size(px(14.0 * s))
                .text_color(p.error())
                .into_any_element(),
            "stopped" => gpui::svg()
                .path("titlebar/circle-dashed.svg")
                .size(px(13.0 * s))
                .text_color(p.muted)
                .into_any_element(),
            _ => gpui::svg()
                .path("titlebar/check.svg")
                .size(px(14.0 * s))
                .text_color(p.success())
                .into_any_element(),
        };
        let toggle_key = key.clone();
        let a11y = [label.as_str(), reason.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(": ");
        let pictures = self.read_pictures(&key, &entry["images"], p, cx);
        let trigger = div()
            .id(SharedString::from(format!("{card_key}:row:{position}")))
            .role(gpui::Role::Button)
            .aria_label(a11y)
            .when(has_detail, |this| this.aria_expanded(expanded))
            .when(failed, |this| this.aria_description("failed"))
            .flex()
            .items_center()
            .gap(px(9.0 * s))
            .px(px(8.0 * s))
            .py(px(6.0 * s))
            .min_h(px(34.0 * s))
            .min_w_0()
            .rounded(px(8.0 * s))
            .when(has_detail, |this| {
                this.chat_cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_marker_disclosure(toggle_key.clone(), expanded, cx)
                    }))
            })
            .child(tool_icon(entry, ROW_ICON, false, p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(label_element)
                    .when(!reason.is_empty(), |this| {
                        this.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.0 * s))
                                .text_color(p.muted)
                                .child(reason.clone()),
                        )
                    }),
            )
            // The picture the call opened from disk, at the row's right before its counts
            // (gx-chat-core `read_images.rs`, `READ_TILE`).
            .children(pictures)
            .child(meta)
            .child(
                div()
                    .size(px(16.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(status_icon),
            );
        let mut row = div().w_full().min_w_0().flex().flex_col().child(trigger);
        if expanded || motion.is_some() {
            let body = match kind.as_str() {
                "file" => self.file_entry_diff(&key, &source, p, cx),
                _ => self.tool_detail_body(&key, message_id, index, &source, p, cx),
            };
            if let Some(body) = body {
                let body = div()
                    .pl(px(4.0 * s))
                    .pb(px(4.0 * s))
                    .min_w_0()
                    .child(body)
                    .into_any_element();
                row = row.child(self.disclosure_body_motion(&key, motion, 0.0, body));
            }
        }
        // A row grows in as its call starts, while the card is live. Its fade reaches the
        // shimmer and spinner drawn over it (row_motion.rs).
        if animate {
            let faded = Rc::new(Cell::new(1.0));
            let fading = faded.clone();
            return super::row_motion::opacity_scope(
                faded,
                row.with_animation(
                    SharedString::from(format!("{card_key}:enter:{position}")),
                    enter_animation(),
                    move |row, delta| {
                        fading.set(delta);
                        row.opacity(delta).mt(px(-6.0 * (1.0 - delta)))
                    },
                )
                .into_any_element(),
            );
        }
        row.into_any_element()
    }
}

/// A row growing in, a circle sliding out or the header's words settling.
///
/// CDXC:SessionChat 2026-11-10 WHY:
/// These run inside the cached transcript, so each of their frames lays out and paints every visible row again; uncapped they asked for every display frame, 120 a second on ProMotion, for their 320ms. Thirty a second is the chat's indicator rate (`INDICATOR_MAX_FPS`).
fn enter_animation() -> Animation {
    Animation::new(ENTER)
        .with_easing(gpui::ease_out_quint())
        .with_max_fps(INDICATOR_MAX_FPS)
}
