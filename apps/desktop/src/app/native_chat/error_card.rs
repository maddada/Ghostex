//! An error or wait the chat shows on its own, as a one-line card in the Message to / Message
//! from family (gx-chat-core `session/error_card.rs`): a tinted warning tile, the title, a status
//! chip, and on opening the whole message and the actions the line used to carry.

use super::{
    appearance::ChatAppearance,
    cards::{SINGLE_LINE_CARD, card_hover_fill},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::Value;

/// CDXC:SessionChat 2026-11-08 SEE-ALSO: the tints are the sidebar's failed red (`FAILED_COLOR`, native_sidebar/status.rs) and the Waiting pill's amber (`tone_color`, inter_agent_message.rs), so an error reads like the sidebar's failed session; the phone draws the same (apps/mobile/app/src/chat/native/cards/ErrorCard.tsx).
fn tint(tone: &str, p: &ChatAppearance) -> Hsla {
    match (tone, p.light) {
        ("wait", false) => rgb(0xfbbf24).into(),
        ("wait", true) => rgb(0xd97706).into(),
        (_, false) => rgb(0xef4444).into(),
        (_, true) => rgb(0xdc2626).into(),
    }
}

impl NativeChatView {
    /// `card` is the core's `{tone, title, chip, detail}`; `actions` are the buttons the old error
    /// line carried, shown under the message once the card is opened. Collapsed by default.
    pub(super) fn error_card(
        &self,
        key: &str,
        card: &Value,
        actions: Vec<AnyElement>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*SINGLE_LINE_CARD;
        let tone = text(card, "tone");
        let tint = tint(&tone, p);
        let title = text(card, "title");
        let detail = text(card, "detail");
        let chip = text(card, "chip");
        let expandable = !detail.is_empty() || !actions.is_empty();
        let expanded = expandable && self.expanded.contains(key);
        let motion = self.disclosure_frame(key, expanded, cx);
        let hairline = p.foreground.opacity(if p.light { 0.08 } else { 0.05 });
        let glyph = if tone == "wait" {
            "titlebar/alert-triangle.svg"
        } else {
            "titlebar/alert-circle.svg"
        };
        // Only the icon carries the colour (CDXC:SessionChat 2026-11-13 on `tone_tile`,
        // tone_card.rs): no tinted tile, and the chip is muted.
        let tile = shape.tile_box(p).child(
            gpui::svg()
                .path(glyph)
                .size(px(shape.glyph_size() * s))
                .text_color(tint),
        );
        let mut row = div()
            .id(SharedString::from(format!("{key}:row")))
            .h(px(shape.header_height(s)))
            .flex_shrink_0()
            .px(px(shape.padding_x * s))
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .min_w_0()
            .child(tile)
            .child(
                shape
                    .title(div().flex_1().min_w_0().truncate(), p)
                    .text_color(p.foreground)
                    .child(title.clone()),
            )
            .when(!chip.is_empty(), |row| {
                row.child(shape.pill(
                    Some(if tone == "wait" {
                        "titlebar/clock.svg"
                    } else {
                        "titlebar/alert-triangle.svg"
                    }),
                    Some(chip),
                    p.muted,
                    p,
                ))
            });
        if expandable {
            let toggle_key = key.to_string();
            let hover = card_hover_fill(p);
            row = row
                .role(gpui::Role::Button)
                .aria_label(title)
                .aria_expanded(expanded)
                .chat_cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.expanded.remove(&toggle_key) {
                        this.expanded.insert(toggle_key.clone());
                    }
                    cx.notify();
                }));
        }
        let opened = (expanded || motion.is_some()).then(|| {
            let body = div()
                .border_t_1()
                .border_color(hairline)
                .px(px(12.0 * s))
                .pt(px(8.0 * s))
                .pb(px(10.0 * s))
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(8.0 * s))
                .text_size(px(13.0 * s))
                .line_height(px(19.6 * s))
                .text_color(p.card_muted)
                .when(!detail.is_empty(), |body| body.child(detail))
                .when(!actions.is_empty(), |body| {
                    body.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0 * s))
                            .children(actions),
                    )
                })
                .into_any_element();
            self.disclosure_body_motion(key, motion, 0.0, body)
        });
        shape
            .shell(div(), p)
            .id(SharedString::from(key.to_string()))
            .role(gpui::Role::Alert)
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(row)
            .children(opened)
            .into_any_element()
    }
}
