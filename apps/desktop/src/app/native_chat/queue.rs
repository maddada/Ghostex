use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use serde_json::{Value, json};

#[derive(Clone)]
struct QueuedPromptDrag {
    session: gpui::EntityId,
    prompt: String,
    preview: String,
    appearance: ChatAppearance,
}

impl Render for QueuedPromptDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        crate::app::helpers::drag_ghost(self.preview.clone(), None, None, self.appearance.scale)
    }
}

/// CDXC:SessionChat 2026-09-26 DECISION:
/// User: an undelivered row shows a small info icon whose tooltip holds the full reason, not a wrapped line of text. The tooltip is the stock chat tooltip (frosted under window glass) with a sensible maximum width, held inside the composer card so it never leaves the chat.
fn undelivered_info(
    id: &str,
    reason: String,
    composer: gpui::Bounds<gpui::Pixels>,
    color: gpui::Hsla,
    s: f32,
) -> AnyElement {
    let (left, right, max_width) = row_tooltip_bounds(composer);
    div()
        .id(format!("queue-undelivered:{id}"))
        .role(gpui::Role::Button)
        .aria_label(reason.clone())
        .size(px(20.0 * s))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .managed_tooltip_with_placement(
            ManagedTooltipPlacement::BelowWithin { left, right },
            move |window, cx| {
                gpui_component::tooltip::Tooltip::new(reason.clone())
                    .max_w(max_width)
                    .build(window, cx)
            },
        )
        .child(
            gpui::svg()
                .path("titlebar/info-circle.svg")
                .size(px(14.0 * s))
                .text_color(color),
        )
        .into_any_element()
}

/// The edges a row's tooltip stays inside (the composer card) and its width.
fn row_tooltip_bounds(
    composer: gpui::Bounds<gpui::Pixels>,
) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    // The tooltip's own 12px side margins sit inside these edges.
    let (left, right) = (composer.left(), composer.right());
    (
        left,
        right,
        (right - left - px(24.0)).min(px(320.0)).max(px(160.0)),
    )
}

/// CDXC:SessionChat 2026-11-25 DECISION:
/// User (Q4865, "chip"): the line under a queued message ("Waiting for the agent to finish
/// compacting the conversation.") used the message's own grey and read as more of the user's
/// text. The status is now a small neutral chip at the row's right, a clock and gxserver's short
/// label (`waitLabel`, server/src/session_chat_queue_runtime/wait_reason.rs), and "the full text
/// on hovering on that status pill" is its tooltip, the same stock chat tooltip as the Not sent
/// info icon. Never accent-coloured. The phone draws the same chip (QueueList.tsx).
fn wait_chip(
    id: &str,
    label: String,
    reason: String,
    composer: gpui::Bounds<gpui::Pixels>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let (left, right, max_width) = row_tooltip_bounds(composer);
    div()
        .id(format!("queue-wait:{id}"))
        .aria_label(reason.clone())
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(4.0 * s))
        .h(px(18.0 * s))
        .px(px(7.0 * s))
        .mr(px(3.0 * s))
        .rounded(px(9.0 * s))
        .bg(p.foreground.opacity(0.06))
        .text_size(px(10.5 * s))
        .line_height(px(18.0 * s))
        .text_color(p.muted)
        .whitespace_nowrap()
        .managed_tooltip_with_placement(
            ManagedTooltipPlacement::BelowWithin { left, right },
            move |window, cx| {
                gpui_component::tooltip::Tooltip::new(reason.clone())
                    .max_w(max_width)
                    .build(window, cx)
            },
        )
        .child(
            gpui::svg()
                .path("titlebar/clock.svg")
                .size(px(10.0 * s))
                .text_color(p.muted),
        )
        .child(label)
        .into_any_element()
}

/// CDXC:SessionChat 2026-11-25 DECISION:
/// User (Q4866, "line count"): a queued message of several lines showed only its first line. The
/// row keeps that one line and adds the core's "+N lines" (`moreLines`, gx-chat-core
/// `queue_row_more`) in the chip's dimmer grey, and hovering the message shows the whole of it
/// (`fullText`) in the same stock chat tooltip as the status chip. The phone shows it on a press
/// (QueueList.tsx).
fn queued_text(
    id: &str,
    preview: String,
    more_lines: String,
    full_text: String,
    busy: bool,
    composer: gpui::Bounds<gpui::Pixels>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let line = div()
        .id(format!("queue-text:{id}"))
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.0 * s))
        .when(busy, |this| this.opacity(0.7))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_size(px(12.0 * s))
                .line_height(px(20.0 * s))
                .child(preview),
        );
    if more_lines.is_empty() || full_text.is_empty() {
        return line.into_any_element();
    }
    let (left, right, max_width) = row_tooltip_bounds(composer);
    line.managed_tooltip_with_placement(
        ManagedTooltipPlacement::BelowWithin { left, right },
        move |window, cx| {
            gpui_component::tooltip::Tooltip::new(full_text.clone())
                .max_w(max_width)
                .build(window, cx)
        },
    )
    .child(
        div()
            .flex_shrink_0()
            .whitespace_nowrap()
            .text_size(px(10.5 * s))
            .line_height(px(18.0 * s))
            .text_color(p.muted.opacity(0.75))
            .child(more_lines),
    )
    .into_any_element()
}

/// The six-dot drag grip, also drawn by the Bot automations feed's channels in Manual order.
pub(crate) fn grip(p: &ChatAppearance) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0 * p.scale))
        .children((0..3).map(|_| {
            div()
                .flex()
                .gap(px(2.0 * p.scale))
                .children((0..2).map(|_| div().size(px(1.5 * p.scale)).rounded_full().bg(p.muted)))
        }))
        .into_any_element()
}

impl NativeChatView {
    fn queue_action(
        &self,
        id: String,
        label: &'static str,
        icon: &'static str,
        disabled: bool,
        command: Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .size(px(24.0 * p.scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0 * p.scale))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .group("native-chat-queue-action")
            .when(disabled, |this| this.opacity(0.4))
            .when(!disabled, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
            })
            // CDXC:SessionChat 2026-09-24 WHY: a GPUI svg paints only with its OWN text color; it does not inherit the parent's, so without one these icons took up space and drew nothing.
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(14.0 * p.scale))
                    .text_color(p.muted)
                    .when(!disabled, |this| {
                        this.group_hover("native-chat-queue-action", |style| {
                            style.text_color(p.foreground)
                        })
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_queue(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let prompts = self.snapshot["queue"]["prompts"].as_array()?;
        if prompts.is_empty() {
            return None;
        }
        let capabilities = &self.snapshot["queue"]["capabilities"];
        let blocked = self.snapshot["sendBlockedReason"].is_string();
        let can_drag = !blocked && prompts.len() > 1 && capabilities["canReorder"] == true;
        let s = p.scale;
        let session = cx.entity_id();
        let composer = self.composer_bounds.get();
        let rows = prompts.iter().map(|prompt| {
            let id = text(prompt, "id");
            let busy = prompt["busy"] == true;
            let failed = prompt["state"] == "failed";
            let preview = text(prompt, "preview");
            let drag = QueuedPromptDrag {
                session,
                prompt: id.clone(),
                preview: preview.clone(),
                appearance: p.clone(),
            };
            // CDXC:SessionChat 2026-09-22 WHY: the id is load-bearing. GPUI applies a group-hover refinement at LAYOUT time only from per-element hover state, which exists only for a div with an id; without one the `w_auto` never reaches layout and the buttons stay clipped at width 0 while the row is hovered.
            // CDXC:SessionChat 2026-09-24 DECISION: a row that could not be delivered keeps its controls on screen.
            let mut actions = div()
                .id(format!("queue-actions:{id}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .when(!failed, |this| {
                    this.w_0()
                        .overflow_hidden()
                        .opacity(0.0)
                        .group_hover("native-chat-queue-row", |style| style.w_auto().opacity(1.0))
                });
            for (capability, label, icon, command) in [
                (
                    "canRetry",
                    "Retry",
                    "titlebar/refresh.svg",
                    json!({"type":"retryQueue","promptId":id}),
                ),
                (
                    "canEdit",
                    "Edit",
                    "titlebar/pencil.svg",
                    json!({"type":"removeQueue","promptId":id,"edit":true}),
                ),
                (
                    "canSendNow",
                    "Send now",
                    "titlebar/arrow-up.svg",
                    json!({"type":"sendQueue","promptId":id}),
                ),
                (
                    "canRemove",
                    "Delete",
                    "titlebar/trash.svg",
                    json!({"type":"removeQueue","promptId":id}),
                ),
            ] {
                let on_card =
                    self.snapshot["sendFailureCardPromptId"].as_str() == Some(id.as_str());
                if capabilities[capability] == true
                    && (capability != "canRetry" || (failed && !on_card))
                {
                    actions = actions.child(self.queue_action(
                        format!("queue-{capability}:{id}"),
                        label,
                        icon,
                        busy || blocked,
                        command,
                        p,
                        cx,
                    ));
                }
            }
            let source_session = session;
            let failed_color = gpui::Hsla::from(gpui::rgb(0xef9999));
            let line = div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(4.0 * s))
                .min_h(px(26.0 * s))
                .child(
                    div()
                        .id(format!("queue-grip:{}", text(prompt, "id")))
                        .w(px(18.0 * s))
                        .h(px(20.0 * s))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(can_drag && !busy, |this| {
                            this.cursor_grab()
                                .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                        })
                        .when(!can_drag && !busy, |this| this.opacity(0.35))
                        .child(if busy {
                            gpui::svg()
                                .path("titlebar/loader2.svg")
                                .size(px(13.0 * s))
                                .text_color(p.muted)
                                .into_any_element()
                        } else {
                            grip(p)
                        }),
                )
                .child(queued_text(
                    &id,
                    preview,
                    text(prompt, "moreLines"),
                    text(prompt, "fullText"),
                    busy,
                    composer,
                    p,
                ))
                .when(failed, |this| {
                    // One wording for a message that did not go out (packages/gx-chat-core/src/composer/delivery_card.rs).
                    let reason = match ghostex_gx_chat_core::composer::delivery_card::plain_reason(
                        prompt["errorMessage"].as_str().unwrap_or(""),
                    ) {
                        reason if reason.is_empty() => "Not sent.".to_string(),
                        reason => format!("Not sent: {reason}"),
                    };
                    this.child(undelivered_info(&id, reason, composer, failed_color, s))
                })
                // Why gxserver still holds the row, once it has waited about a minute.
                .when_some(
                    prompt["waitReason"]
                        .as_str()
                        .filter(|reason| !failed && !reason.trim().is_empty())
                        .map(str::to_string),
                    |this, reason| {
                        let label = text(prompt, "waitLabel");
                        this.child(wait_chip(&id, label, reason, composer, p))
                    },
                )
                .child(actions);
            div()
                .id(format!("queued-prompt:{id}"))
                .group("native-chat-queue-row")
                .flex()
                .flex_col()
                .min_w_0()
                .pr(px(2.0 * s))
                .rounded(px(8.0 * s))
                .border_1()
                // Only the row's icon carries the failure's colour (CDXC:SessionChat 2026-11-13 on
                // `tone_tile`, tone_card.rs).
                .border_color(p.border.opacity(0.58))
                .text_color(p.muted)
                .hover(|style| {
                    let style = style.bg(p.foreground.opacity(0.05));
                    if failed {
                        style
                    } else {
                        style.border_color(gpui::transparent_black())
                    }
                })
                .when(can_drag, |this| {
                    this.on_drop(cx.listener(move |this, drag: &QueuedPromptDrag, _, cx| {
                        if drag.session == source_session {
                            this.invoke(
                                json!({"type":"moveQueue","promptId":drag.prompt,"targetId":id}),
                                cx,
                            );
                        }
                    }))
                })
                .child(line)
        });
        Some(
            div()
                .id("native-chat-queue")
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .max_h(px(156.0 * s))
                .pb(px(6.0 * s))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element(),
        )
    }
}
