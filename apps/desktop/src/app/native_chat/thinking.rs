use super::disclosure_motion::measured;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

/// The transcript column fits roughly this many characters at the prose size.
const WRAP_COLUMNS: usize = 90;
/// React capped a secondary block at 18rem (`.ghostex-chat-scroll-cap`).
const CAP_HEIGHT: f32 = 288.0;
const LINE_HEIGHT: f32 = 22.75;

/// About how many lines a body takes in the transcript column. A GPUI row cannot
/// measure its own text before layout, so the rows that cap or clamp a body
/// decide from the text itself.
pub(super) fn estimated_lines(text: &str) -> usize {
    text.lines()
        .map(|line| 1 + line.chars().count() / WRAP_COLUMNS)
        .sum()
}

/// The transcript's marker column: the bullet every prose lane hangs its first
/// line from.
pub(super) fn lane_marker(color: Hsla, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .w(px(16.0 * s))
        .h(px(LINE_HEIGHT * s))
        .ml(px(2.0 * s))
        .flex()
        .justify_center()
        .flex_shrink_0()
        .child(
            div()
                .mt(px(9.5 * s))
                .size(px(4.0 * s))
                .rounded_full()
                .bg(color),
        )
        .into_any_element()
}

impl NativeChatView {
    /// A reasoning turn that carries no tool calls: the quiet lane React drew
    /// with `.ghostex-chat-thinking-row`, the same bullet and prose size as the
    /// answer in a muted voice, with a long thought capped so it cannot own the
    /// transcript's scrollbar.
    pub(super) fn thinking_row(
        &self,
        id: &str,
        body: String,
        references: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut lane = p.clone();
        lane.prose = if p.light { p.muted } else { p.primary };
        let key = format!("thinking:{id}");
        let expanded = self.expanded.contains(&key);
        let capped = estimated_lines(&body) > (CAP_HEIGHT / LINE_HEIGHT) as usize;
        let toggle_key = key.clone();
        let motion = capped
            .then(|| self.disclosure_frame(&key, expanded, cx))
            .flatten();
        let prose = self.markdown(format!("body:{id}"), body, references, &lane, cx);
        let text_body = match motion {
            // "Show more" and "Show less" ease between the capped height and the full one.
            Some(frame) => self.capped_body_motion(
                &key,
                frame,
                0.0,
                div()
                    .id(format!("thinking-body:{id}"))
                    .min_w_0()
                    .child(prose)
                    .into_any_element(),
            ),
            None => {
                let block = div()
                    .id(format!("thinking-body:{id}"))
                    .min_w_0()
                    .when(capped && !expanded, |body| {
                        body.max_h(px(CAP_HEIGHT * s)).overflow_y_scroll()
                    })
                    .child(prose)
                    .into_any_element();
                if capped && !expanded {
                    measured(self.disclosure_floor(&key), block)
                } else {
                    block
                }
            }
        };
        div()
            .flex()
            .items_start()
            .w_full()
            .min_w_0()
            .gap(px(6.0 * s))
            .pb(px(13.0 * s))
            .child(lane_marker(lane.prose, p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0 * s))
                    .child(text_body)
                    .when(capped, |column| {
                        column.child(
                            div()
                                .id(format!("thinking-toggle:{id}"))
                                .flex()
                                .items_center()
                                .gap(px(4.0 * s))
                                .text_size(px(14.0 * s))
                                .text_color(p.muted)
                                .chat_cursor_pointer()
                                .hover(|style| style.text_color(p.foreground))
                                .child(
                                    gpui::svg()
                                        .path(if expanded {
                                            "titlebar/chevron-up.svg"
                                        } else {
                                            "titlebar/chevron-down.svg"
                                        })
                                        .size(px(14.0 * s))
                                        .text_color(p.muted),
                                )
                                .child(if expanded { "Show less" } else { "Show more" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if expanded {
                                        this.expanded.remove(&toggle_key);
                                    } else {
                                        this.expanded.insert(toggle_key.clone());
                                    }
                                    this.list.remeasure();
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .into_any_element()
    }
}
