use super::cards::{SINGLE_LINE_CARD, card_hover_fill};
use super::disclosure_motion::measured;
use super::{
    appearance::ChatAppearance, state::NativeChatView, status_rows::tone_color,
    thinking::estimated_lines, transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

/// An advisor severity's glyph, named by the core (`ADVISOR_RANKS` in gx-chat-core's
/// `transcript/system_cards.rs`).
fn advisor_icon(name: &str) -> &'static str {
    match name {
        "alert-octagon" => "titlebar/alert-octagon.svg",
        "alert-triangle" => "titlebar/alert-triangle.svg",
        "bulb" => "titlebar/bulb.svg",
        _ => "titlebar/info-circle.svg",
    }
}

/// An advisor severity's colour: the message cards' red and amber, otherwise the card's muted text.
fn advisor_tone(tone: &str, p: &ChatAppearance) -> Hsla {
    match tone {
        "failed" | "waiting" => super::inter_agent_message::tone_color(tone, p),
        _ => p.card_muted,
    }
}

/// Collapsed, a goal shows this many lines of its objective.
const GOAL_PREVIEW_LINES: usize = 3;

impl NativeChatView {
    /// A system turn that is a card or a rule rather than a sentence. Which card
    /// it is comes from the core's classifier
    /// (packages/gx-chat-core/src/transcript/system_cards.rs); this file
    /// owns only the GPUI layout of each one.
    pub(super) fn system_card(
        &self,
        id: &str,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let card = &message["systemCard"];
        match text(card, "kind").as_str() {
            "auto-named" => self.auto_named_card(card, p),
            "fork-boundary" => self.fork_boundary_row(card, p),
            "goal" => self.goal_card(id, card, p, cx),
            "command-output" => self.command_output_card(id, card, p, cx),
            "agent-message" => {
                self.subagent_message_card(id, card, &message["markdownReferences"], p, cx)
            }
            "advisor" => self.advisor_card(id, card, p, cx),
            _ => self.system_marker_row(card, p),
        }
    }

    /// React drew this one as an `inline-flex` card, not a full-width panel: it announces a name,
    /// so it hugs the name and the two lines stack beside the icon (rows.tsx, `auto-named`).
    fn auto_named_card(&self, card: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        div()
            .flex()
            .w_full()
            .min_w_0()
            .pb(px(8.0 * s))
            .child(
                div()
                    .flex()
                    .items_start()
                    .min_w_0()
                    .flex_shrink(1.0)
                    .gap(px(10.0 * s))
                    .px(px(14.0 * s))
                    .py(px(10.0 * s))
                    .rounded(px(16.0 * s))
                    .border(px(s))
                    .border_color(p.border.opacity(0.7))
                    .bg(p.input.opacity(0.4))
                    .child(
                        gpui::svg()
                            .path(if card["userRenamed"].as_bool() == Some(true) {
                                "titlebar/pencil.svg"
                            } else {
                                "titlebar/sparkles.svg"
                            })
                            .size(px(16.0 * s))
                            .mt(px(2.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap(px(2.0 * s))
                            .child(
                                div()
                                    .text_size(px(14.0 * s))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.foreground)
                                    .child(text(card, "lead")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .min_w_0()
                                    .gap_x(px(4.0 * s))
                                    .text_size(px(12.0 * s))
                                    .text_color(p.card_muted)
                                    .child("New name:")
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_color(p.foreground.opacity(0.85))
                                            .child(text(card, "title")),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// CDXC:SessionFork 2026-08-28 WHY:
    /// Stitched scroll-back crossing from one fork ancestor into the next is the boundary between two threads, so it reads as a labeled rule instead of another sentence, with the daemon's text unchanged.
    fn fork_boundary_row(&self, card: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let rule = || div().h(px(1.0)).flex_1().min_w_0().bg(p.border);
        div()
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .gap(px(6.0 * s))
            .pt(px(4.0 * s))
            .pb(px(12.0 * s))
            .text_size(px(14.0 * s))
            .text_color(p.muted)
            .child(rule())
            .child(
                gpui::svg()
                    .path("titlebar/git-branch.svg")
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(div().flex_shrink_0().child(text(card, "text")))
            .child(rule())
            .into_any_element()
    }

    fn goal_card(
        &self,
        id: &str,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("goal:{id}");
        let expanded = self.expanded.contains(&key);
        let status = text(card, "status");
        let motion = self.disclosure_frame(&key, expanded, cx);
        let usage = text(card, "usage");
        let objective = text(card, "objective");
        let expandable = estimated_lines(&objective) > GOAL_PREVIEW_LINES;
        let pill = tone_color(
            match status.as_str() {
                "stalled" | "usage limited" | "limited by budget" => "error",
                _ => "",
            },
            p,
        );
        let header = div()
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .child(
                gpui::svg()
                    .path("titlebar/focus-2.svg")
                    .size(px(14.0 * s))
                    .mt(px(4.0 * s))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(8.0 * s))
                    .child(div().text_color(p.foreground).child("Goal"))
                    .when(!status.is_empty(), |heading| {
                        heading.child(
                            div()
                                .rounded_full()
                                .px(px(6.0 * s))
                                .bg(pill.opacity(0.15))
                                .text_size(px(11.0 * s))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(pill)
                                .child(status),
                        )
                    }),
            )
            .when(!usage.is_empty(), |header| {
                header.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(usage),
                )
            })
            .when(expandable, |header| {
                header.child(self.card_chevron(key.clone(), expanded, p, cx))
            })
            .into_any_element();
        let body = if objective.is_empty() {
            Vec::new()
        } else if let Some(frame) = motion {
            // The whole text, clipped between the clamped height and its own.
            let full = div().min_w_0().child(objective).into_any_element();
            vec![self.capped_body_motion(&key, frame, 0.0, full)]
        } else {
            let text = div()
                .min_w_0()
                // GPUI only clamps when the row also asks for an ellipsis: `line_clamp` alone
                // sets the line budget the truncating measure needs and is otherwise ignored.
                .when(!expanded, |body| {
                    body.line_clamp(GOAL_PREVIEW_LINES).text_ellipsis()
                })
                .child(objective)
                .into_any_element();
            vec![if expanded || !expandable {
                text
            } else {
                measured(self.disclosure_floor(&key), text)
            }]
        };
        self.status_card_with_header(header, body, Vec::new(), p)
    }

    /// The output Ghostex captured from a command it ran for the session, open
    /// by default: the reader asked for the command, so its output is the row.
    fn command_output_card(
        &self,
        id: &str,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("command-output:{id}");
        let expanded = self.expanded.contains(&key) || !self.collapsed.contains(&key);
        let output = text(card, "output");
        let motion = self.disclosure_frame(&key, expanded, cx);
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(p.border)
            .bg(p.input)
            .overflow_hidden()
            .text_color(p.card_muted)
            .child(
                div()
                    .px(px(12.0 * s))
                    .py(px(8.0 * s))
                    .text_size(px(12.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.disclosure(
                        key.clone(),
                        text(card, "command"),
                        expanded,
                        None,
                        p,
                        cx,
                    )),
            )
            .when(
                (expanded || motion.is_some()) && !output.is_empty(),
                |row| {
                    row.child(
                        self.disclosure_body_motion(
                            &key,
                            motion,
                            0.0,
                            self.nested_scroll(
                                format!("command-output-body:{id}"),
                                div()
                                    .min_w_0()
                                    .max_h(px(384.0 * s))
                                    .px(px(12.0 * s))
                                    .py(px(8.0 * s))
                                    .border_t_1()
                                    .border_color(p.border)
                                    // React's `<pre>`: the captured output as it came, in the
                                    // transcript's monospace and nothing else. A fenced block
                                    // would give it a language header and a copy button the
                                    // card already is the frame for.
                                    .font_family(super::fonts::CHAT_MONO)
                                    .text_size(px(12.0 * s))
                                    .line_height(px(19.5 * s))
                                    .child(output.clone()),
                            ),
                        ),
                    )
                },
            )
            .into_any_element()
    }

    /// OMP's advisor notes: one 44px row (the info tile, "Advisor shared notes", the time and one pill
    /// per severity) that opens to the notes, each with its severity word and icon. The core
    /// decides the notes, their order, their words and tones; this lays them out in the one-line
    /// card every transcript card shares (`SINGLE_LINE_CARD`, cards.rs), body like an opened
    /// message card's.
    fn advisor_card(
        &self,
        id: &str,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*SINGLE_LINE_CARD;
        let key = format!("advisor:{id}");
        let expanded = self.expanded.contains(&key);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let hairline = p.foreground.opacity(if p.light { 0.08 } else { 0.05 });
        let time = text(card, "time");
        let pills = div()
            .flex_1()
            .min_w_0()
            .flex()
            .justify_end()
            .items_center()
            .gap(px(6.0 * s))
            .when(!time.is_empty(), |pills| {
                pills.child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(time),
                )
            })
            .children(
                card["counts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|count| {
                        shape.pill(
                            Some(advisor_icon(&text(count, "icon"))),
                            Some(text(count, "label")),
                            advisor_tone(&text(count, "tone"), p),
                            p,
                        )
                    }),
            );
        let hover = card_hover_fill(p);
        let toggle = key.clone();
        let row = div()
            .id(SharedString::from(format!("{key}:row")))
            .h(px(shape.header_height(s)))
            .flex_shrink_0()
            .px(px(shape.padding_x * s))
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .min_w_0()
            .child(shape.tile("titlebar/info-circle.svg", p))
            .child(
                shape
                    .title(div().flex_shrink_0().whitespace_nowrap(), p)
                    .text_color(p.foreground)
                    .child(text(card, "name")),
            )
            .child(pills)
            .role(gpui::Role::Button)
            .aria_label(text(card, "title"))
            .aria_expanded(expanded)
            .chat_cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_marker_disclosure(toggle.clone(), expanded, cx)
            }));
        let opened = (expanded || motion.is_some()).then(|| {
            let notes = div()
                .border_t_1()
                .border_color(hairline)
                .px(px(12.0 * s))
                .pt(px(8.0 * s))
                .pb(px(10.0 * s))
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(10.0 * s))
                .text_color(p.card_muted)
                .children(
                    card["notes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(|(index, note)| {
                            let tone = advisor_tone(&text(note, "tone"), p);
                            div()
                                .flex()
                                .items_start()
                                .gap(px(10.0 * s))
                                .min_w_0()
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .min_w(px(76.0 * s))
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0 * s))
                                        .text_size(px(12.0 * s))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(tone)
                                        .child(
                                            gpui::svg()
                                                .path(advisor_icon(&text(note, "icon")))
                                                .size(px(13.0 * s))
                                                .flex_shrink_0()
                                                .text_color(tone),
                                        )
                                        .child(text(note, "label")),
                                )
                                .child(div().flex_1().min_w_0().child(self.markdown(
                                    format!("{key}:note:{index}"),
                                    text(note, "markdown"),
                                    &note["references"],
                                    p,
                                    cx,
                                )))
                        }),
                )
                .into_any_element();
            self.disclosure_body_motion(&key, motion, 0.0, notes)
        });
        shape
            .shell(div(), p)
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(row)
            .children(opened)
            .into_any_element()
    }

    /// Every other system turn: one muted line of what the daemon wrote.
    fn system_marker_row(&self, card: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        div()
            .w_full()
            .min_w_0()
            .pb(px(8.0 * s))
            .text_size(px(14.0 * s))
            .text_color(p.muted)
            .child(text(card, "text"))
            .into_any_element()
    }

    /// The expand control a card wears at the end of its header row.
    fn card_chevron(
        &self,
        key: String,
        expanded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(format!("card-chevron:{key}"))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .size(px(20.0 * s))
            .rounded(px(4.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
            .child(
                gpui::svg()
                    .path(if expanded {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(14.0 * s))
                    .text_color(p.muted),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.anchor_disclosure_toggle(&key, !expanded);
                if expanded {
                    this.expanded.remove(&key);
                    this.collapsed.insert(key.clone());
                } else {
                    this.collapsed.remove(&key);
                    this.expanded.insert(key.clone());
                }
                cx.notify();
            }))
            .into_any_element()
    }
}
