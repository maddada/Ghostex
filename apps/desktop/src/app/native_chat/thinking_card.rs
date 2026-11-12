//! A thought drawn as one slim line shaped like a tool row, instead of the open thinking lane
//! (`thinking.rs`): a brain tile, "Thinking…" while the thought is being written or "Thought" once
//! the agent moved on, and the thought's line (its newest finished line while it streams, its first
//! line after). One click opens or closes the full thought below it. A running thought that owns
//! the calls it made ends its line with "N tool calls" and a chevron that folds them, as a reply's
//! sentence folds its calls (`thinkingCard.calls`). gx-chat-core marks the rows
//! (`thinkingCard` for the running turn, `thinkingRow` for every thought; the user's decision is in
//! its `transcript/thinking_card.rs`). The phone's `ThinkingLine` in `MessageRow.tsx` is the same
//! row.

use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::tool_run::{DESCRIBED_LABEL_OPACITY, ROW_PAD_X, TILE};
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

/// React's secondary-block cap (`thinking.rs`): an opened thought scrolls past this.
const OPEN_CAP: f32 = 288.0;

impl NativeChatView {
    /// The thought as one closed line. `message["thinkingCard"]` (the running turn) supplies the
    /// live flag, the newest finished line and the label of the calls it folds; any other thought
    /// reads its `thinkingRow`.
    pub(super) fn thinking_card(
        &mut self,
        id: &str,
        message: &Value,
        body: String,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let card = &message["thinkingCard"];
        let running = card.is_object();
        let live = card["live"] == true;
        let key = format!("thinking-card:{id}");
        let expanded = self.is_expanded(&key, false);
        let row = &message["thinkingRow"];
        let preview = match text(card, "preview") {
            preview if preview.is_empty() => text(row, "preview"),
            preview => preview,
        };
        // A Codex reasoning summary is often its headline alone: there is nothing to open.
        let expandable = running || row["expandable"] != false;
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
