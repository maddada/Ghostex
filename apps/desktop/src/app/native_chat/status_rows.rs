use super::{
    appearance::ChatAppearance, state::NativeChatView, thinking::lane_marker, transcript::text,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, IntoElement, ParentElement as _, Styled as _, div, px,
    rgb,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
struct StatusTone {
    icon: String,
    color: Option<String>,
}

/// The tone table React's status rows read (`STATUS_TONE_ICON` in
/// session-chat-message-list/rows.tsx), kept as `status-tone.json` in the core.
/// One row names each tone's glyph; the class names in the file are React's, the
/// hex colors are this renderer's.
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
            "status" => self.status_rows(suppressed, p),
            "inline" => self.inline_suppressed_row(suppressed, p),
            _ => self.collapsed_suppressed_row(id, suppressed, p, cx),
        }
    }

    /// One row per status: a turn reporting several background tasks reports each of them.
    ///
    /// The column carries no padding of its own. Every transcript row already
    /// ends with the same trailing space (`transcript_item_row`), and a second
    /// block of it here is what pushed consecutive completed-action pills far
    /// further apart than React's, which were only `pb-3` from each other.
    fn status_rows(&self, suppressed: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let sole = json!([{"label": suppressed["label"], "tone": suppressed["tone"]}]);
        let statuses = suppressed["statuses"]
            .as_array()
            .filter(|statuses| !statuses.is_empty())
            .unwrap_or_else(|| sole.as_array().expect("status row"));
        div()
            .flex()
            .flex_col()
            .items_start()
            .w_full()
            .min_w_0()
            .gap(px(6.0 * s))
            .children(statuses.iter().map(|status| {
                self.status_row(
                    text(status, "label"),
                    text(status, "detail"),
                    &text(status, "tone"),
                    p,
                )
            }))
            .into_any_element()
    }

    fn status_row(
        &self,
        label: String,
        detail: String,
        tone_name: &str,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let glyph = tone(tone_name);
        let color = tone_color(tone_name, p);
        let badge = if glyph.color.is_some() {
            color.opacity(0.15)
        } else {
            p.border
        };
        div()
            .flex()
            .items_start()
            .max_w_full()
            .min_w_0()
            .gap(px(8.0 * s))
            // CDXC:SessionChat 2026-09-04 DECISION: User: the row is less rounded than a pill (0.75rem, same as the terminal activity card) so a wrapped two-line row does not read as a lozenge.
            .rounded(px(12.0 * s))
            .border_1()
            .border_color(p.border)
            .bg(p.input)
            .px(px(12.0 * s))
            .py(px(6.0 * s))
            .text_size(px(12.0 * s))
            // The pill is its own type scale, so it takes the line height that goes with its size
            // instead of the transcript's prose leading; on that leading the pill stood half a line
            // taller than React's and the tone badge had to be nudged down to look centred.
            .line_height(px(16.0 * s))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.muted)
            .child(
                div()
                    .size(px(16.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(badge)
                    .child(
                        gpui::svg()
                            .path(format!("titlebar/{}.svg", glyph.icon))
                            .size(px(10.0 * s))
                            .text_color(color),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(6.0 * s))
                    .child(div().min_w_0().child(label))
                    // The harness writes the exit code inside the sentence, where a
                    // wrap can strand the number on its own line; it reads as its own chip.
                    .when(!detail.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .rounded(px(6.0 * s))
                                .border_1()
                                .border_color(p.border)
                                .px(px(6.0 * s))
                                .font_family(super::fonts::CHAT_MONO)
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(11.0 * s))
                                .child(detail),
                        )
                    }),
            )
            .into_any_element()
    }

    /// A harness turn short enough to read in place, in the quiet lane's voice
    /// with its marker label as the lead-in.
    fn inline_suppressed_row(&self, suppressed: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        div()
            .flex()
            .items_start()
            .w_full()
            .min_w_0()
            .gap(px(6.0 * s))
            .pb(px(13.0 * s))
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
                            .child(format!("{} ·", text(suppressed, "label"))),
                    )
                    .child(div().flex_1().min_w_0().child(text(suppressed, "text"))),
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
