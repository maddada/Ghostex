//! The delivery status line above an accepted send that is still waiting for
//! the agent's terminal: "Waiting for agent…" while it is queued or sending,
//! the daemon's own sentence once it failed, with Retry and Remove beside it.
//! Ported from React's `session-chat-startup-send-status.tsx`; it acts on the queue
//! row the send became, so Retry and Remove are the queue's own operations.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::{Value, json};

impl NativeChatView {
    pub(super) fn render_startup_delivery(
        &self,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let delivery = &message["startupDelivery"];
        if !delivery.is_object() {
            return None;
        }
        let s = p.scale;
        let failed = delivery["state"] == "failed";
        let prompt_id = delivery["promptId"].clone();
        let status = if failed {
            let message = text(delivery, "errorMessage");
            if message.is_empty() {
                "Message could not be delivered.".to_string()
            } else {
                message
            }
        } else {
            "Waiting for agent…".to_string()
        };
        let action = |id: &'static str, label: &'static str, command: Value| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .chat_cursor_pointer()
                .px(px(6.0 * s))
                .py(px(2.0 * s))
                .rounded(px(5.0 * s))
                .text_color(p.primary)
                .hover(|style| style.bg(p.border))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
        };
        Some(
            div()
                .id("startup-delivery")
                .role(gpui::Role::Status)
                .flex()
                .flex_wrap()
                .items_center()
                .justify_end()
                .gap(px(4.0 * s))
                .w_full()
                .text_size(px(12.0 * s))
                .text_color(p.muted)
                .child(status)
                .when(failed, |this| {
                    this.child(action(
                        "startup-delivery-retry",
                        "Retry",
                        json!({"type":"retryQueue","promptId":prompt_id.clone()}),
                    ))
                    .child(action(
                        "startup-delivery-remove",
                        "Remove",
                        json!({"type":"removeQueue","promptId":prompt_id}),
                    ))
                })
                .into_any_element(),
        )
    }
}
