//! The clickable header the two composer panels share: lead icon, title, a
//! muted meta line, an optional trailing ornament and the fold chevron. Both
//! the Tasks panel and the Subagents strip are the shared status card with this
//! header, the way React's `SessionChatStatusCard` rendered them.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

pub(super) struct PanelHeader {
    pub(super) id: &'static str,
    pub(super) icon: &'static str,
    pub(super) title: &'static str,
    pub(super) meta: String,
    pub(super) open: bool,
    /// Whether a body is drawn under the header now, which outlasts `open` while it eases away.
    pub(super) has_body: bool,
    pub(super) toggle_label: &'static str,
    pub(super) trailing: Option<AnyElement>,
    pub(super) command: Value,
}

impl NativeChatView {
    pub(super) fn panel_header(
        &self,
        header: PanelHeader,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let command = header.command;
        let row = div()
            .id(header.id)
            .role(gpui::Role::Button)
            .aria_label(header.toggle_label)
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            // No `w_full`: the press header's negative side margins need the card's
            // stretch to widen the row to the border; a 100% width stops it short.
            .min_w_0()
            .chat_cursor_pointer()
            .child(
                gpui::svg()
                    .path(header.icon)
                    .size(px(14.0 * s))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.foreground)
                    .child(header.title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(header.meta),
            )
            .when_some(header.trailing, |this, trailing| this.child(trailing))
            .child(
                gpui::svg()
                    .path(if header.open {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(14.0 * s))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)));
        super::cards::status_card_press_header(row, header.has_body, false, p).into_any_element()
    }
}
