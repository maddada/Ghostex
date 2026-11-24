//! A thought drawn as one slim line shaped like a tool row, instead of the open thinking lane
//! (`thinking.rs`): a brain tile, "Thinking…" while the thought is being written or "Thought" once
//! the agent moved on, and the thought's line (its newest finished line while it streams, its first
//! line after). One click opens or closes the full thought below it. A thought outside a turn fold
//! that owns the calls it made ends its line with "N tool calls" and a chevron that folds them, as
//! a reply's sentence folds its calls (`thinkingCard.calls`). gx-chat-core marks the rows
//! (`thinkingCard` for the running turn and for settled thoughts outside a turn fold that own tool
//! calls, `thinkingRow` for every thought; the user's decision is in its
//! `transcript/thinking_card.rs`). The phone's `ThinkingLine` in `MessageRow.tsx` is the same row.

use super::cards::{SINGLE_LINE_CARD, TITLE_ROW_GAP, card_hover_fill};
use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::tool_run::{DESCRIBED_LABEL_OPACITY, ROW_PAD_X, TILE, boxed_header_corners};
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::transcript::thinking_card::{
    is_working_thought_card, working_card_key, working_card_open_default,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

/// React's secondary-block cap (`thinking.rs`): an opened thought scrolls past this.
const OPEN_CAP: f32 = 288.0;

impl NativeChatView {
    /// The thought as one closed line, or inside an opened Quiet View Working line as the shared
    /// single-line card matching plain tool-call groups (`boxed_thinking_card`).
    /// `message["thinkingCard"]` (present on the running turn and on settled thoughts outside a
    /// turn fold that own tool calls) supplies the live flag, the preview line and the label of
    /// the calls it folds; any other thought reads its `thinkingRow`.
    pub(super) fn thinking_card(
        &mut self,
        id: &str,
        message: &Value,
        body: String,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if is_working_thought_card(message, self.quiet_working_here()) {
            return self.boxed_thinking_card(id, message, body, p, cx);
        }
        let s = p.scale;
        let card = &message["thinkingCard"];
        let live = card["live"] == true;
        let key = format!("thinking-card:{id}");
        let expanded = self.is_expanded(&key, false);
        let row = &message["thinkingRow"];
        let preview = match text(card, "preview") {
            preview if preview.is_empty() => text(row, "preview"),
            preview => preview,
        };
        // A Codex reasoning summary is often its headline alone: there is nothing to open once it
        // has finished writing.
        let expandable = live || row["expandable"] != false;
        let lane = p.thinking_lane();
        let title = if live { "Thinking…" } else { "Thought" };
        // The calls it folds open on a sentence's default: in detailed or verbose mode, or while
        // one of them waits on an approval.
        let calls = card["calls"].as_str().map(str::to_string);
        let calls_key = format!("reasoning:{id}");
        let calls_open = calls.is_some()
            && self.is_expanded(&calls_key, p.work_open || message["toolsOpen"] == true);

        let header = div()
            .id(SharedString::from(format!("{key}:header")))
            .role(gpui::Role::Button)
            .aria_label(if preview.is_empty() {
                title.to_string()
            } else {
                format!("{title} {preview}")
            })
            .when(expandable, |this| this.aria_expanded(expanded))
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(8.0 * s))
            .px(px(ROW_PAD_X * s))
            .py(px(5.0 * s))
            .rounded(px(8.0 * s))
            .text_size(px(13.5 * s))
            .child(
                div()
                    .size(px(TILE * s))
                    .rounded(px(6.0 * s))
                    .bg(p.foreground.opacity(0.05))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        gpui::svg()
                            .path("titlebar/brain.svg")
                            .size(px(13.0 * s))
                            .text_color(p.muted),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(14.0 * s))
                    .text_color(p.prose.opacity(DESCRIBED_LABEL_OPACITY))
                    .child(if expanded { String::new() } else { preview }),
            )
            .when_some(calls.clone(), |this, calls| {
                this.child(self.folded_calls_toggle(&calls_key, calls, calls_open, p, cx))
            })
            .when(expandable, |this| {
                let key = key.clone();
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.03)))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_marker_disclosure(key.clone(), expanded, cx)
                    }))
            });

        let mut column = div().w_full().min_w_0().flex().flex_col().gap(px(4.0 * s));
        column = column.child(header);
        // The thought opens and shuts with the chat's fold motion (disclosure_motion.rs).
        let body_motion = self.disclosure_frame(&key, expanded && expandable, cx);
        if (expanded || body_motion.is_some()) && expandable {
            let body_key = SharedString::from(format!("{key}:body"));
            // While it is being written the open thought keeps the newest lines in view, the way a
            // terminal does, until the reader scrolls up; scrolling back to the end follows again.
            if live {
                let handle = self
                    .nested_scrolls
                    .borrow_mut()
                    .entry(body_key.clone())
                    .or_default()
                    .clone();
                if handle.offset().y <= -handle.max_offset().y + px(2.0) {
                    handle.scroll_to_bottom();
                }
            }
            let thought = self
                .nested_scroll(
                    body_key,
                    div().min_w_0().max_h(px(OPEN_CAP * s)).child(self.markdown(
                        format!("thinking-card-body:{id}"),
                        body,
                        &message["markdownReferences"],
                        &lane,
                        cx,
                    )),
                )
                .into_any_element();
            let body = div()
                .pl(px(ROW_PAD_X * s))
                .child(disclosure_body(
                    p,
                    DisclosureRail::Marker,
                    4.0,
                    key.clone(),
                    "Collapse thinking",
                    [thought],
                    cx,
                ))
                .into_any_element();
            column = column.child(self.disclosure_body_motion(&key, body_motion, 4.0 * s, body));
        }
        if calls.is_some() {
            let motion = self.disclosure_frame(&calls_key, calls_open, cx);
            if calls_open || motion.is_some() {
                let rows = self.tool_rows(message, p, cx);
                if !rows.is_empty() {
                    let body = disclosure_body(
                        p,
                        DisclosureRail::Marker,
                        8.0,
                        calls_key.clone(),
                        "Collapse tool calls",
                        rows,
                        cx,
                    );
                    column = column.child(
                        div()
                            .pl(px(ROW_PAD_X * s))
                            .child(self.disclosure_body_motion(&calls_key, motion, 4.0 * s, body)),
                    );
                }
            } else if let Some(pictures) = self.read_picture_strip(
                &format!("{calls_key}:read"),
                &message["readImages"],
                super::read_pictures::FOLDED_CALLS_INSET,
                p.background,
                p,
                cx,
            ) {
                // Folded calls keep the pictures they read in view, as a closed turn fold does
                // (gx-chat-core `read_images.rs`).
                column = column.child(pictures);
            }
        }
        div()
            .w_full()
            .pb(px(6.0 * s))
            .child(column)
            .into_any_element()
    }

    /// A running thought inside an opened Quiet View Working line, styled as the same
    /// `SINGLE_LINE_CARD` as a plain "N tool calls" group (`tool_run.rs` `boxed_card_heading`).
    ///
    /// CDXC:SessionChat 2026-11-24 DECISION: User (Q4839 A, Q4840 "working only", Q4841 "build"):
    /// "when thought is collapsed just show single line truncated. when i expand fade out that line
    /// that's truncated and show full thoughts inside the card below the header". Inside an opened
    /// Working line, collapsed shows the 44px card header (brain tile, title, one truncated preview
    /// line, and "N tool calls" pill on the right); expanding fades out that truncated preview line
    /// with the disclosure motion and reveals the full wrapped thought inside the card below the
    /// header hairline, followed by its tool calls when present. Phone matches (`BoxedThinkingCard`
    /// in `MessageRow.tsx`).
    fn boxed_thinking_card(
        &mut self,
        id: &str,
        message: &Value,
        body: String,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let card = &message["thinkingCard"];
        let row = &message["thinkingRow"];
        let live = card["live"] == true;
        let preview = match text(card, "preview") {
            preview if preview.is_empty() => text(row, "preview"),
            preview => preview,
        };
        let title = if live { "Thinking…" } else { "Thought" };
        let calls = card["calls"].as_str().map(str::to_string);
        let key = working_card_key(id);
        let expanded = self.is_expanded(&key, working_card_open_default(message));
        let motion = self.disclosure_frame(&key, expanded, cx);
        let open_progress = match motion {
            Some(frame) => frame.log.clamp(0.0, 1.0),
            None if expanded => 1.0,
            None => 0.0,
        };
        let preview_opacity = (1.0 - open_progress).clamp(0.0, 1.0);
        let body_shown = expanded || motion.is_some();
        let shape = &*SINGLE_LINE_CARD;
        let hover = card_hover_fill(p);
        let toggle_key = key.clone();

        let header = boxed_header_corners(
            div()
                .id(SharedString::from(format!("{key}:header")))
                .role(gpui::Role::Button)
                .aria_label(if preview.is_empty() {
                    title.to_string()
                } else {
                    format!("{title} {preview}")
                })
                .aria_expanded(expanded)
                .flex()
                .items_center()
                .w_full()
                .min_w_0()
                .gap(px(TITLE_ROW_GAP * s))
                .px(px(shape.padding_x * s))
                .h(px(shape.header_height(s)))
                .text_color(p.foreground)
                .chat_cursor_pointer()
                .hover(move |style| style.bg(hover)),
            body_shown,
            p,
        )
        .child(shape.tile("titlebar/brain.svg", p))
        .child(
            div()
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .gap(px(6.0 * s))
                .child(
                    shape
                        .title(div().flex_shrink_0().text_color(p.foreground), p)
                        .child(title),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(super::chat_text_look::PROSE_SIZE * s))
                        .line_height(px(20.0 * s))
                        .text_color(p.prose.opacity(DESCRIBED_LABEL_OPACITY * preview_opacity))
                        .when(preview_opacity > 0.01 && !preview.is_empty(), |this| {
                            this.child(preview)
                        }),
                ),
        )
        .when_some(calls.clone(), |this, label| {
            this.child(shape.pill(None, Some(label), p.muted, p))
        })
        .on_click(cx.listener(move |view, _, _, cx| {
            view.toggle_marker_disclosure(toggle_key.clone(), expanded, cx)
        }))
        .into_any_element();

        let mut parts = vec![header];
        if body_shown {
            let body_key = SharedString::from(format!("{key}:body"));
            if live {
                let handle = self
                    .nested_scrolls
                    .borrow_mut()
                    .entry(body_key.clone())
                    .or_default()
                    .clone();
                if handle.offset().y <= -handle.max_offset().y + px(2.0) {
                    handle.scroll_to_bottom();
                }
            }
            let lane = p.thinking_lane();
            let thought = div()
                .w_full()
                .min_w_0()
                .px(px(shape.padding_x * s))
                .pt(px(10.0 * s))
                .pb(px(11.0 * s))
                .child(self.nested_scroll(
                    body_key,
                    div().min_w_0().max_h(px(OPEN_CAP * s)).child(self.markdown(
                        format!("thinking-card-body:{id}"),
                        body,
                        &message["markdownReferences"],
                        &lane,
                        cx,
                    )),
                ));
            let tool_rows = if calls.is_some() {
                self.boxed_card_tool_rows(message, p, cx)
            } else {
                None
            };
            let card_body = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(shape.hairline(p))
                .child(thought)
                .children(tool_rows)
                .into_any_element();
            parts.push(self.disclosure_body_motion(&key, motion, 0.0, card_body));
        } else if calls.is_some()
            && let Some(pictures) = self.read_picture_strip(
                &format!("{key}:read"),
                &message["readImages"],
                super::read_pictures::FOLDED_CALLS_INSET,
                p.background,
                p,
                cx,
            )
        {
            parts.push(
                div()
                    .px(px(shape.padding_x * s))
                    .pb(px(8.0 * s))
                    .child(pictures)
                    .into_any_element(),
            );
        }

        let column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .children(parts);
        shape.shell(column, p).into_any_element()
    }

    /// "N tool calls" and the chevron at the end of a running thought's line, which open or close
    /// the calls it made without opening the thought's text.
    fn folded_calls_toggle(
        &self,
        key: &str,
        label: String,
        open: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = key.to_string();
        div()
            .id(SharedString::from(format!("{key}:toggle")))
            .role(gpui::Role::Button)
            .aria_label(if open {
                format!("Hide {label}")
            } else {
                format!("Show {label}")
            })
            .aria_expanded(open)
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(2.0 * s))
            .px(px(4.0 * s))
            .rounded(px(4.0 * s))
            .text_size(px(12.5 * s))
            .text_color(p.muted)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.05)))
            .child(label)
            .child(
                super::disclosure_motion::turned_chevron(self.fold_turn(&key, open, cx), true)
                    .size(px(13.0 * s))
                    .text_color(p.muted),
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(key.clone(), open, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}
