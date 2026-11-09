//! A prompt's delivery state. A user bubble still waiting for the agent's terminal shows a
//! spinner, and one the agent queued behind its running turn shows a play button, both left of the
//! bubble at its bottom. A failed send gets the daemon's own sentence under the bubble,
//! with Retry and Remove under it, and a send gxserver has held a minute says why there. An inter-agent card keeps the older "Waiting for agent…" line.
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

/// One button under a delivery note.
pub(super) struct NoteAction {
    pub id: gpui::SharedString,
    pub label: &'static str,
    pub command: Value,
}

/// What `delivery_note` draws: the sentence (after an optional glyph such as a spinner), its colour,
/// the buttons under it, and whether it sits at the right edge like the message it belongs to.
pub(super) struct DeliveryNote {
    pub id: gpui::SharedString,
    pub role: gpui::Role,
    pub lead: Option<AnyElement>,
    pub status: gpui::SharedString,
    pub color: gpui::Hsla,
    pub actions: Vec<NoteAction>,
    pub end: bool,
}

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
        let failed = delivery["state"] == "failed";
        // Why gxserver still holds it, once it has waited about a minute; shown on every waiting
        // send, beside the spinner a user bubble already has.
        let reason = text(delivery, "waitReason");
        if !failed && !waiting_line && reason.is_empty() {
            return None;
        }
        let prompt_id = delivery["promptId"].clone();
        // A message held on this computer while the session's computer is offline
        // (packages/gx-chat-core/src/session/computer.rs): "Will send when Windows PC is back" and
        // Cancel, which puts it back in the chat box.
        if delivery["state"] == "held" {
            let sending = reason.starts_with("Sending");
            return Some(
                self.delivery_note(
                    DeliveryNote {
                        id: "startup-delivery".into(),
                        role: gpui::Role::Status,
                        lead: None,
                        status: reason.into(),
                        color: p.muted,
                        actions: if sending {
                            Vec::new()
                        } else {
                            vec![NoteAction {
                                id: "held-send-cancel".into(),
                                label: "Cancel",
                                command: json!({"type":"removeQueue","promptId":prompt_id}),
                            }]
                        },
                        // A left-aligned message keeps its note on its own side.
                        end: !p.left_aligned(),
                    },
                    p,
                    cx,
                )
                .max_w(gpui::relative(0.8))
                .into_any_element(),
            );
        }
        // A held row that says why can go from here too, and a send that left the queue
        // unconfirmed has only this line (CDXC:SessionChat 2026-10-31 in
        // packages/gx-chat-core/src/session/startup_sends.rs).
        let removable = !failed
            && (delivery["state"] == "unconfirmed"
                || (delivery["state"] == "queued"
                    && !reason.is_empty()
                    && self.snapshot["queue"]["capabilities"]["canRemove"] == true));
        // The card that stands for this row has the reason and Retry (chat core's
        // `sendFailureCardPromptId`), so the row only says what happened to it.
        let on_card = failed && self.snapshot["sendFailureCardPromptId"] == delivery["promptId"];
        let status = if on_card {
            "Not sent".to_string()
        } else if failed {
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
        let mut actions = Vec::new();
        if failed {
            if !on_card {
                actions.push(NoteAction {
                    id: "startup-delivery-retry".into(),
                    label: "Retry",
                    command: json!({"type":"retryQueue","promptId":prompt_id.clone()}),
                });
            }
            actions.push(NoteAction {
                id: "startup-delivery-remove".into(),
                label: "Remove",
                command: json!({"type":"removeQueue","promptId":prompt_id.clone()}),
            });
        }
        if removable {
            actions.push(NoteAction {
                id: "startup-delivery-remove".into(),
                label: "Remove",
                command: json!({"type":"removeQueue","promptId":prompt_id}),
            });
        }
        Some(
            self.delivery_note(
                DeliveryNote {
                    id: "startup-delivery".into(),
                    role: gpui::Role::Status,
                    lead: None,
                    status: status.into(),
                    color: p.muted,
                    actions,
                    // A left-aligned message keeps its note on its own side.
                    end: !p.left_aligned(),
                },
                p,
                cx,
            )
            .max_w(gpui::relative(0.8))
            .into_any_element(),
        )
    }

    /// CDXC:SessionChat 2026-11-05 DECISION:
    /// User, on a failed send's note drawn as one long line across the chat, clipped at its start: "it looks so bad". Every note about a message that did not go out (a user bubble's, a Message from/to card's, a page-answers card's) is this one shape: the sentence wraps inside the message's own width, never wider than the column and never clipped, and Retry / Remove sit on their own line under it, on the message's side. The sentence used to be a bare text child of a `justify_end` wrapping row, so its one-line width overflowed the row's left edge.
    pub(super) fn delivery_note(
        &self,
        note: DeliveryNote,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        let DeliveryNote {
            id,
            role,
            lead,
            status,
            color,
            actions,
            end,
        } = note;
        let sentence = div()
            .min_w_0()
            .max_w_full()
            .when(end, |this| this.text_right())
            .child(status.clone());
        let line = match lead {
            Some(lead) => div()
                .flex()
                .items_start()
                .gap(px(6.0 * s))
                .min_w_0()
                .max_w_full()
                .child(div().flex_shrink_0().pt(px(3.0 * s)).child(lead))
                .child(sentence)
                .into_any_element(),
            None => sentence.into_any_element(),
        };
        let buttons = (!actions.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(4.0 * s))
                .when(end, |this| this.justify_end())
                // Level with the sentence's edge: each label has 6px of padding inside its hover fill.
                .mx(px(-6.0 * s))
                .children(actions.into_iter().map(|action| {
                    let command = action.command;
                    div()
                        .id(action.id)
                        .role(gpui::Role::Button)
                        .aria_label(action.label)
                        .chat_cursor_pointer()
                        .px(px(6.0 * s))
                        .py(px(2.0 * s))
                        .rounded(px(5.0 * s))
                        .text_color(p.primary)
                        .hover(|style| style.bg(p.border))
                        .child(action.label)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)),
                        )
                }))
        });
        div()
            .id(id)
            .role(role)
            .aria_label(status)
            .flex()
            .flex_col()
            .gap(px(2.0 * s))
            .min_w_0()
            .max_w_full()
            .when(end, |this| this.items_end())
            .text_size(px(12.0 * s))
            .line_height(px(18.0 * s))
            .text_color(color)
            .child(line)
            .children(buttons)
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
        // CDXC:Clipboard 2026-11-09 DECISION: a message sent while its picture is still being
        // saved gets the same spinner, and the same Back to composer arrow as a message the agent
        // has not taken yet, once the wait has lasted a moment (the chat core's
        // `attachingMessages`, packages/gx-chat-core/src/composer/attaching.rs). A quick save
        // shows neither.
        let attaching = self.attaching_status(&id);
        if !waiting && !queued && !attaching {
            return None;
        }
        // A held message waits for its computer, not for the agent: a clock, not a spinner.
        let held = delivery["state"] == "held";
        let label = if attaching {
            "Attaching the picture"
        } else if held {
            "Waiting for the computer"
        } else if waiting {
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
        if held {
            return Some((
                slot.role(gpui::Role::Status)
                    .child(
                        svg()
                            .path("titlebar/clock.svg")
                            .size(px(12.0 * s))
                            .text_color(p.muted),
                    )
                    .into_any_element(),
                1,
            ));
        }
        if waiting || attaching {
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
            let spinner = slot.role(gpui::Role::Status).child(glyph);
            if attaching && !self.in_subagent {
                return Some((
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0 * s))
                        .child(self.take_back_indicator(message, p, cx))
                        .child(spinner)
                        .into_any_element(),
                    2,
                ));
            }
            return Some((spinner.into_any_element(), 1));
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
