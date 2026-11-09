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
        let rows = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(SINGLE_LINE_CARD.stack_gap(p) * s))
            .children(statuses.iter().enumerate().map(|(index, status)| {
                self.status_row(
                    format!("status:{id}:{index}"),
                    &subject,
                    text(status, "label"),
                    text(status, "detail"),
                    &text(status, "tone"),
                    p,
                )
            }));
        if suppressed["expandable"] != true {
            return rows.into_any_element();
        }
        self.expandable_status(id, rows, text(suppressed, "text"), p, cx)
    }

    /// A slash command's output with more than one line (gx-chat-core `command_output_note`): the
    /// note shows the first line and a click opens the whole output under it, in the box a
    /// collapsed harness turn opens onto.
    fn expandable_status(
        &self,
        id: &str,
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
                column.child(
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
                                    format!("```\n{body}\n```"),
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

    /// A status as a one-line card (`SINGLE_LINE_CARD`, cards.rs): a tile for what it is about
    /// with its tone as the badge (a green check, a red cross, none when neutral), the sentence on
    /// one line with the whole of it on hover, and a trailing fact such as "exit code 1" as a pill.
    fn status_row(
        &self,
        id: String,
        subject: &str,
        label: String,
        detail: String,
        tone_name: &str,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let card = &*SINGLE_LINE_CARD;
        let glyph = match subject {
            "compaction" => "titlebar/arrows-diagonal-minimize.svg",
            "command" => "titlebar/terminal-2.svg",
            _ => "titlebar/adjustments-horizontal.svg",
        };
        let tile = match tone_name {
            "error" => card.tile(glyph, p).child(card.error_badge(p)),
            "neutral" => card.tile(glyph, p),
            _ => card.tile(glyph, p).child(card.check_badge(p)),
        };
        let detail_tone = if tone_name == "error" {
            tone_color(tone_name, p)
        } else {
            p.card_muted
        };
        let hint = SharedString::from(label.clone());
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
            .when(!detail.is_empty(), |row| {
                row.child(card.pill(None, Some(detail), detail_tone, p))
            })
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
                                    format!("```\n{body}\n```"),
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
