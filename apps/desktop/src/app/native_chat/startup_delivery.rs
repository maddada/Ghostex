//! A prompt's delivery state. A user bubble still waiting for the agent's terminal shows a
//! spinner, and one the agent queued behind its running turn shows a play button, both left of the
//! bubble at its bottom. A failed send gets the daemon's own sentence above the bubble,
//! with Retry and Remove beside it, and a send gxserver has held a minute says why there. An inter-agent card keeps the older "Waiting for agent…" line.
//! Ported from React's `session-chat-startup-send-status.tsx`; it acts on the queue
//! row the send became, so Retry and Remove are the queue's own operations.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

impl NativeChatView {
    /// The status line for a send the terminal has not taken yet. `waiting_line` keeps the
    /// "Waiting for agent…" sentence for cards that have no delivery indicator; without it only a
    /// failed send gets a line.
    pub(super) fn render_startup_delivery(
        &self,
        message: &Value,
        waiting_line: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let delivery = &message["startupDelivery"];
        if !delivery.is_object() {
            return None;
        }
        let s = p.scale;
        let failed = delivery["state"] == "failed";
        // Why gxserver still holds it, once it has waited about a minute; shown on every waiting
        // send, beside the spinner a user bubble already has.
        let reason = text(delivery, "waitReason");
        if !failed && !waiting_line && reason.is_empty() {
            return None;
        }
        let prompt_id = delivery["promptId"].clone();
        // A held row that says why can go from here too, and a send that left the queue
        // unconfirmed has only this line (CDXC:SessionChat 2026-10-31 in
        // packages/gx-chat-core/src/session/startup_sends.rs).
        let removable = !failed
            && (delivery["state"] == "unconfirmed"
                || (delivery["state"] == "queued"
                    && !reason.is_empty()
                    && self.snapshot["queue"]["capabilities"]["canRemove"] == true));
        let status = if failed {
            let message = text(delivery, "errorMessage");
            if message.is_empty() {
                "Message could not be delivered.".to_string()
            } else {
                message
            }
        } else if !reason.is_empty() {
            reason
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
                        json!({"type":"removeQueue","promptId":prompt_id.clone()}),
                    ))
                })
                .when(removable, |this| {
                    this.child(action(
                        "startup-delivery-remove",
                        "Remove",
                        json!({"type":"removeQueue","promptId":prompt_id}),
                    ))
                })
                .into_any_element(),
        )
    }

    /// CDXC:SessionChat 2026-09-28 DECISION:
    /// User: instead of "Waiting for agent…" above a prompt or a "Queued" label, show an icon left of the bubble at its bottom (an earlier placement under the bubble in the time and Copy row drifted away from short prompts and was rejected as ugly). A spinner means "Waiting for agent"; a play button means "Queued, click to interrupt and send", and a click sends one Escape so the agent takes the queued message right away.
    ///
    /// CDXC:SessionChat 2026-10-22 DECISION:
    /// User: "add button to cancel sending and bring it back to chat". A queued message the agent can give back (`queueTakeBackIds`, Claude Code and Grok Build: packages/gx-chat-core/src/composer/queue_take_back.rs) gets Back to composer left of its play button, the same take-back as the hover row's button and the composer's Up key; it spins while gxserver pulls the message out of the agent's queue.
    ///
    /// Answers the indicator and how many 24px slots it is wide, so the caller hangs it off the
    /// bubble's left edge at the right distance.
    pub(super) fn delivery_indicator(
        &self,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<(AnyElement, usize)> {
        let id = text(message, "id");
        let s = p.scale;
        let delivery = &message["startupDelivery"];
        // An unconfirmed send is no longer waiting on anything: its line says so, with no spinner.
        let waiting = delivery.is_object()
            && delivery["state"] != "failed"
            && delivery["state"] != "unconfirmed";
        let queued = message["queued"] == true && delivery.is_null();
        if !waiting && !queued {
            return None;
        }
        let label = if waiting {
            "Waiting for agent"
        } else {
            "Queued, click to interrupt and send"
        };
        let slot = div()
            .id(gpui::SharedString::from(format!("delivery-indicator:{id}")))
            .group("native-chat-delivery-indicator")
            .aria_label(label)
            .size(px(24.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            });
        if waiting {
            let glyph = svg()
                .path("titlebar/loader2.svg")
                .size(px(12.0 * s))
                .text_color(p.muted);
            let glyph = if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
                glyph.into_any_element()
            } else {
                glyph
                    .with_throttled_animation(
                        gpui::SharedString::from(format!("delivery-spinner:{id}")),
                        std::time::Duration::from_millis(900),
                        |svg, delta| {
                            svg.with_transformation(gpui::Transformation::rotate(gpui::radians(
                                delta * std::f32::consts::TAU,
                            )))
                        },
                    )
                    .into_any_element()
            };
            return Some((
                slot.role(gpui::Role::Status)
                    .child(glyph)
                    .into_any_element(),
                1,
            ));
        }
        let take_back = (!self.in_subagent
            && self.snapshot["queueTakeBackIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|queued| queued == &message["id"])))
        .then(|| self.take_back_indicator(message, p, cx));
        let play = slot
            .role(gpui::Role::Button)
            .tab_index(0)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
            .child(
                svg()
                    .path("titlebar/player-play.svg")
                    .size(px(12.0 * s))
                    .text_color(p.muted)
                    .group_hover("native-chat-delivery-indicator", |style| {
                        style.text_color(p.foreground)
                    }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.invoke(json!({"type":"sendKey","key":"escape"}), cx)
            }));
        let slots = 1 + usize::from(take_back.is_some());
        Some((
            div()
                .flex()
                .items_center()
                .gap(px(2.0 * s))
                .children(take_back)
                .child(play)
                .into_any_element(),
            slots,
        ))
    }

    /// Back to composer beside a queued message's play button, or its spinner while the take-back
    /// runs.
    fn take_back_indicator(
        &self,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = text(message, "id");
        let s = p.scale;
        let busy = self.snapshot["queueTakeBackBusy"] == message["id"];
        let label: gpui::SharedString = if busy {
            "Taking the message back…".into()
        } else {
            format!(
                "Back to composer ({})",
                text(&self.snapshot, "queueTakeBackHotkey")
            )
            .into()
        };
        let tooltip = label.clone();
        let slot = div()
            .id(gpui::SharedString::from(format!("delivery-take-back:{id}")))
            .group("native-chat-delivery-take-back")
            .aria_label(label)
            .size(px(24.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            });
        if busy {
            return slot
                .role(gpui::Role::Status)
                .child(
                    svg()
                        .path("titlebar/loader2.svg")
                        .size(px(12.0 * s))
                        .text_color(p.muted),
                )
                .into_any_element();
        }
        slot.role(gpui::Role::Button)
            .tab_index(0)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
            .child(
                svg()
                    .path("titlebar/arrow-back-up.svg")
                    .size(px(12.0 * s))
                    .text_color(p.muted)
                    .group_hover("native-chat-delivery-take-back", |style| {
                        style.text_color(p.foreground)
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.invoke(json!({"type":"takeBackQueued","messageId":id.clone()}), cx)
            }))
            .into_any_element()
    }
}
