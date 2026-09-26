//! The pending tool row gxserver reads off the agent's terminal, rendered as
//! the working strip's card shape at the end of the transcript (React:
//! `session-chat-terminal-tool-row.tsx`). The activity itself, its label and the
//! painted tool block under it, is projected by the core.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

/// One key for every card, so the next pending tool comes up the way the last
/// one was left, which is what React persisted in client storage.
const EXPANDED_KEY: &str = "terminal-tool";

/// The transcript's prose leading, React's `line-height: 1.625` on the header row.
const LINE_HEIGHT: f32 = 22.75;

impl NativeChatView {
    pub(super) fn terminal_tool_row(
        &mut self,
        activity: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let detail = text(activity, "detail").trim().to_string();
        let expandable = !detail.is_empty();
        let open = expandable && self.expanded.contains(EXPANDED_KEY);
        let motion = self.disclosure_frame(EXPANDED_KEY, open, cx);
        let has_body = open || motion.is_some();
        // React's header row was `align-items: flex-start` and put each glyph in a
        // one-line-tall box that centres it (`.ghostex-chat-status-card-lead`), so a label that
        // wraps keeps the dot and the chevron on its first line instead of drifting to the middle
        // of the card.
        let lead = |glyph: AnyElement| {
            div()
                .h(px(LINE_HEIGHT * s))
                .flex()
                .items_center()
                .flex_shrink_0()
                .child(glyph)
        };
        let header = div()
            .id("terminal-tool-header")
            .flex()
            .items_start()
            .min_w_0()
            .gap(px(8.0 * s))
            .when(expandable, |this| {
                this.chat_cursor_pointer().on_click(
                    // Above the composer, not in the transcript: no header anchor, the transcript keeps
                    // following its tail while the card grows.
                    cx.listener(|view, _, _, cx| {
                        if !view.expanded.remove(EXPANDED_KEY) {
                            view.expanded.insert(EXPANDED_KEY.to_string());
                        }
                        cx.notify();
                    }),
                )
            })
            .child(lead(
                div()
                    .size(px(7.0 * s))
                    .rounded_full()
                    .bg(p.card_muted)
                    .into_any_element(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(p.foreground)
                    .child(text(activity, "label")),
            )
            .when(expandable, |this| {
                this.child(lead(
                    gpui::svg()
                        .path(if open {
                            "titlebar/chevron-down.svg"
                        } else {
                            "titlebar/chevron-right.svg"
                        })
                        .size(px(14.0 * s))
                        .text_color(p.muted)
                        .into_any_element(),
                ))
            });
        let header = if expandable {
            super::cards::status_card_press_header(header, has_body, false, p)
        } else {
            header
        }
        .into_any_element();
        let body = if has_body {
            vec![
                div()
                    .id("terminal-tool-detail")
                    .max_h(px(300.0 * s))
                    .overflow_y_scroll()
                    .p(px(10.0 * s))
                    .bg(p.input)
                    .rounded(px(6.0 * s))
                    .child(self.markdown(
                        "terminal-tool-body".to_string(),
                        format!("```\n{detail}\n```"),
                        &Value::Null,
                        p,
                        cx,
                    ))
                    .into_any_element(),
            ]
        } else {
            Vec::new()
        };
        self.status_card_with_header_motion(
            super::cards::CardBodyMotion {
                key: EXPANDED_KEY,
                frame: motion,
                shut_body: false,
                shut: !open,
            },
            header,
            body,
            Vec::new(),
            p,
        )
    }
}
