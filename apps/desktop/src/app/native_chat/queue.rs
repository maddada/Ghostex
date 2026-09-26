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
        let p = &self.appearance;
        div()
            .px(px(8.0 * p.scale))
            .h(px(28.0 * p.scale))
            .max_w(px(500.0 * p.scale))
            .flex()
            .items_center()
            .rounded(px(8.0 * p.scale))
            .bg(p.input)
            .border_1()
            .border_color(p.border)
            .text_color(p.muted)
            .font_family(p.font.clone())
            .text_size(px(12.0 * p.scale))
            .text_ellipsis()
            .child(self.preview.clone())
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
    // The tooltip's own 12px side margins sit inside these edges.
    let (left, right) = (composer.left(), composer.right());
    let max_width = (right - left - px(24.0)).min(px(320.0)).max(px(160.0));
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

fn grip(p: &ChatAppearance) -> AnyElement {
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
                session: session.clone(),
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
                if capabilities[capability] == true && (capability != "canRetry" || failed) {
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
            let source_session = session.clone();
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
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_size(px(12.0 * s))
                        .line_height(px(20.0 * s))
                        .when(busy, |this| this.opacity(0.7))
                        .child(preview),
                )
                .when(failed, |this| {
                    let reason = format!(
                        "Not delivered: {}",
                        prompt["errorMessage"]
                            .as_str()
                            .unwrap_or("the send failed.")
                    );
                    this.child(undelivered_info(&id, reason, composer, failed_color, s))
                })
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
                .border_color(if failed {
                    failed_color.opacity(0.35)
                } else {
                    p.border.opacity(0.58)
                })
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
