use super::{
    appearance::ChatAppearance, cards::SINGLE_LINE_CARD, state::NativeChatView,
    thinking::lane_marker, transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
struct StatusTone {
    color: Option<String>,
}

/// The tone table React's status rows read (`STATUS_TONE_ICON` in
/// session-chat-message-list/rows.tsx), kept as `status-tone.json` in the core; this renderer
/// takes each tone's hex color.
static TONES: LazyLock<HashMap<String, StatusTone>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/status-tone.json"
    ))
    .expect("shared status tone table")
});

fn hex(color: &str) -> Option<Hsla> {
    u32::from_str_radix(color.trim_start_matches('#'), 16)
        .ok()
        .map(|value| rgb(value).into())
}

/// An unnamed tone is the completed-action tone, the same default React's row took.
fn tone(name: &str) -> &'static StatusTone {
    TONES.get(name).unwrap_or(&TONES["ok"])
}

fn fenced_verbatim_block(body: &str) -> String {
    let mut max_run = 0usize;
    let mut run = 0usize;
    for byte in body.bytes() {
        if byte == b'`' {
            run += 1;
            max_run = max_run.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat(max_run.max(2) + 1);
    format!("{fence}\n{body}\n{fence}")
}

/// The tint a tone paints its glyph with, muted when the tone has no color of its own.
pub(super) fn tone_color(name: &str, p: &ChatAppearance) -> Hsla {
    tone(name)
        .color
        .as_deref()
        .and_then(hex)
        .unwrap_or(p.card_muted)
}

impl NativeChatView {
    /// A harness-injected turn, in the shape its classification asks for:
    /// completed-action status rows, one muted line of prose, or a collapsed
    /// marker that opens onto the verbatim text (rows.tsx StatusRows,
    /// InlineSuppressedTurn, SuppressedTurn).
    pub(super) fn suppressed_row(
        &self,
        id: &str,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let suppressed = &message["suppressed"];
        match text(suppressed, "kind").as_str() {
            "status" => self.status_rows(id, suppressed, p, cx),
            "inline" => self.inline_suppressed_row(suppressed, p),
            _ => self.collapsed_suppressed_row(id, suppressed, p, cx),
        }
    }

    /// One card per status: a model change with a pin warning reports both. A background task's end
    /// is a tool row instead (gx-chat-core `transcript/background_rows.rs`).
    ///
    /// The column carries no padding of its own. Every transcript row already
    /// ends with the same trailing space (`transcript_item_row`).
    fn status_rows(
        &self,
        id: &str,
        suppressed: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let sole = json!([{"label": suppressed["label"], "tone": suppressed["tone"]}]);
        let statuses = suppressed["statuses"]
            .as_array()
            .filter(|statuses| !statuses.is_empty())
            .unwrap_or_else(|| sole.as_array().expect("status row"));
        let subject = text(suppressed, "subject");
        let delivery_retry = (subject == "delivery").then(|| {
            let dn = &suppressed["deliveryNotice"];
            let project_id = text(dn, "projectId");
            let session_id = text(dn, "sessionId");
            let prompt_ids: Vec<String> = dn["promptIds"]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let preview = text(dn, "preview");
            (!project_id.is_empty() && !session_id.is_empty() && !prompt_ids.is_empty())
                .then_some((project_id, session_id, prompt_ids))
                .map(|target| (target, preview))
        });
        let delivery_preview = (subject == "delivery")
            .then(|| text(&suppressed["deliveryNotice"], "preview"))
            .filter(|preview| !preview.is_empty());
        let rows = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(SINGLE_LINE_CARD.stack_gap(p) * s))
            .children(statuses.iter().enumerate().map(|(index, status)| {
                let retry = if index == 0 {
                    delivery_retry
                        .as_ref()
                        .and_then(|opt| opt.as_ref().map(|(target, _)| target.clone()))
                } else {
                    None
                };
                self.status_row(
                    format!("status:{id}:{index}"),
                    id,
                    &subject,
                    text(status, "label"),
                    text(status, "detail"),
                    &text(status, "tone"),
                    delivery_preview.as_deref(),
                    retry,
                    p,
                    cx,
                )
            }));
        if suppressed["expandable"] != true {
            return rows.into_any_element();
        }
        self.expandable_status(id, &subject, rows, text(suppressed, "text"), p, cx)
    }

    /// A slash command's output with more than one line (gx-chat-core `command_output_note`): the
    /// note shows the first line and a click opens the whole output under it, in the box a
    /// collapsed harness turn opens onto.
    fn expandable_status(
        &self,
        id: &str,
        subject: &str,
        rows: gpui::Div,
        body: String,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("suppressed:{id}");
        let expanded = self.expanded.contains(&key);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let toggle_key = key.clone();
        let is_delivery = subject == "delivery";
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id(SharedString::from(format!("status-toggle:{id}")))
                    .role(gpui::Role::Button)
                    .aria_expanded(expanded)
                    .w_full()
                    .min_w_0()
                    .chat_cursor_pointer()
                    .child(rows)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.anchor_disclosure_toggle(&toggle_key, !expanded);
                        if expanded {
                            this.expanded.remove(&toggle_key);
                            this.collapsed.insert(toggle_key.clone());
                        } else {
                            this.collapsed.remove(&toggle_key);
                            this.expanded.insert(toggle_key.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(expanded || motion.is_some(), |column| {
                let detail_view: AnyElement = if is_delivery {
                    div()
                        .min_w_0()
                        .px(px(12.0 * s))
                        .py(px(8.0 * s))
                        .rounded(px(8.0 * s))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.input)
                        .text_size(px(12.5 * s))
                        .text_color(p.card_muted)
                        .child(format!("First line: {body}"))
                        .into_any_element()
                } else {
                    self.nested_scroll(
                        format!("suppressed-detail:{id}"),
                        div()
                            .min_w_0()
                            .max_h(px(400.0 * s))
                            .p(px(10.0 * s))
                            .rounded(px(8.0 * s))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.input)
                            .child(self.markdown(
                                format!("suppressed-body:{id}"),
                                fenced_verbatim_block(&body),
                                &Value::Null,
                                p,
                                cx,
                            )),
                    )
                    .into_any_element()
                };
                column.child(self.disclosure_body_motion(&key, motion, 6.0 * s, detail_view))
            })
            .into_any_element()
    }

    fn retry_delivery_notice(
        &mut self,
        notice_id: String,
        project_id: String,
        session_id: String,
        prompt_ids: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.retried_delivery_notices.insert(notice_id) {
            return;
        }
        cx.notify();
        let remote = self.config.remote.clone();
        cx.background_executor()
            .spawn(async move {
                for prompt_id in prompt_ids {
                    let params = json!({
                        "projectId": project_id,
                        "sessionId": session_id,
                        "promptId": prompt_id,
                        "retry": true,
                    });
                    let _ = super::rpc::request(
                        remote.clone(),
                        "/api/updateSessionChatQueuedPrompt",
                        &params,
                    )
                    .await;
                }
            })
            .detach();
    }

    /// A status as a one-line card (`SINGLE_LINE_CARD`, cards.rs): a tile for what it is about
    /// with its tone as the badge (a green check, a red cross, none when neutral), the sentence on
    /// one line with the whole of it on hover, and a trailing fact such as "exit code 1" as a pill
    /// (or a Retry button for a delivery-failure notice whose failed queue rows can be retried).
    #[allow(clippy::too_many_arguments)]
    fn status_row(
        &self,
        id: String,
        message_id: &str,
        subject: &str,
        label: String,
        detail: String,
        tone_name: &str,
        preview: Option<&str>,
        retry_target: Option<(String, String, Vec<String>)>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let card = &*SINGLE_LINE_CARD;
        let glyph = match subject {
            "compaction" => "titlebar/arrows-diagonal-minimize.svg",
            "command" => "titlebar/terminal-2.svg",
            // A prompt the agent's own hook refused (gx-chat-core `transcript/hook_blocked.rs`).
            "hook" => "titlebar/barrier-block.svg",
            // A compact delivery-failure notice (gx-chat-core `transcript/delivery_notice.rs`).
            "delivery" => "titlebar/alert-circle.svg",
            _ => "titlebar/adjustments-horizontal.svg",
        };
        let tile = match tone_name {
            "error" => card.tile(glyph, p).child(card.error_badge(p)),
            "neutral" => card.tile(glyph, p),
            _ => card.tile(glyph, p).child(card.check_badge(p)),
        };
        // The tile's badge carries the tone; the pill stays muted (CDXC:SessionChat 2026-11-13 on
        // `tone_tile`, tone_card.rs).
        let detail_tone = p.card_muted;
        let hint = SharedString::from(match preview {
            Some(first_line) if !first_line.is_empty() => {
                format!("{label}\nFirst line: {first_line}")
            }
            _ => label.clone(),
        });
        let retried = self.retried_delivery_notices.contains(message_id);
        let trailing: Option<AnyElement> =
            if let Some((target_project, target_session, prompt_ids)) = retry_target {
                if retried {
                    Some(
                        card.pill(None, Some("Retried".to_string()), detail_tone, p)
                            .into_any_element(),
                    )
                } else {
                    let notice_id = message_id.to_string();
                    Some(
                        div()
                            .id(SharedString::from(format!("delivery-retry:{message_id}")))
                            .role(gpui::Role::Button)
                            .aria_label("Retry")
                            .flex_shrink_0()
                            .px(px(8.0 * s))
                            .py(px(3.0 * s))
                            .rounded(px(6.0 * s))
                            .text_size(px(12.0 * s))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.primary)
                            .bg(p.primary.opacity(if p.light { 0.12 } else { 0.16 }))
                            .hover(|style| {
                                style.bg(p.primary.opacity(if p.light { 0.2 } else { 0.26 }))
                            })
                            .chat_cursor_pointer()
                            .child("Retry")
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.retry_delivery_notice(
                                    notice_id.clone(),
                                    target_project.clone(),
                                    target_session.clone(),
                                    prompt_ids.clone(),
                                    cx,
                                );
                            }))
                            .into_any_element(),
                    )
                }
            } else if !detail.is_empty() {
                Some(
                    card.pill(None, Some(detail), detail_tone, p)
                        .into_any_element(),
                )
            } else {
                None
            };
        card.shell(div(), p)
            .id(SharedString::from(id))
            .role(gpui::Role::Status)
            .aria_label(hint.clone())
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .h(px(card.height * s))
            .flex_shrink_0()
            .gap(px(10.0 * s))
            .px(px(card.padding_x * s))
            .child(tile)
            .child(
                card.title(div().flex_1().min_w_0().truncate(), p)
                    .text_color(p.foreground)
                    .child(label),
            )
            .children(trailing)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
            })
            .into_any_element()
    }

    /// A harness turn short enough to read in place, in the quiet lane's voice
    /// with its marker label as the lead-in.
    fn inline_suppressed_row(&self, suppressed: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let label = text(suppressed, "label");
        let body = text(suppressed, "text");
        div()
            .flex()
            .items_start()
            .w_full()
            .min_w_0()
            .gap(px(6.0 * s))
            .pb(px(p.inner_gap(13.0) * s))
            .text_size(px(14.0 * s))
            .text_color(p.muted)
            .child(lane_marker(p.muted, p))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0 * s))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.muted.opacity(0.78))
                            .child(if body.is_empty() {
                                label
                            } else {
                                format!("{label} ·")
                            }),
                    )
                    .when(!body.is_empty(), |row| {
                        row.child(div().flex_1().min_w_0().child(body))
                    }),
            )
            .into_any_element()
    }

    /// Long harness output: the muted marker line, expanding to the verbatim text.
    fn collapsed_suppressed_row(
        &self,
        id: &str,
        suppressed: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("suppressed:{id}");
        let expanded = self.expanded.contains(&key);
        let body = text(suppressed, "text");
        let motion = self.disclosure_frame(&key, expanded, cx);
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(6.0 * s))
            .pb(px(8.0 * s))
            .text_color(p.muted)
            .child(self.disclosure(
                key.clone(),
                text(suppressed, "label"),
                expanded,
                None,
                p,
                cx,
            ))
            .when((expanded || motion.is_some()) && !body.is_empty(), |row| {
                row.child(
                    self.disclosure_body_motion(
                        &key,
                        motion,
                        6.0 * s,
                        self.nested_scroll(
                            format!("suppressed-detail:{id}"),
                            div()
                                .min_w_0()
                                .max_h(px(400.0 * s))
                                .p(px(10.0 * s))
                                .rounded(px(8.0 * s))
                                .border_1()
                                .border_color(p.border)
                                .bg(p.input)
                                .child(self.markdown(
                                    format!("suppressed-body:{id}"),
                                    fenced_verbatim_block(&body),
                                    &Value::Null,
                                    p,
                                    cx,
                                )),
                        ),
                    ),
                )
            })
            .into_any_element()
    }
}
