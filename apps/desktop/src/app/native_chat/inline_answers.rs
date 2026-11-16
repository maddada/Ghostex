//! One-click answers under an agent's question: the marked line `transcript/inline_answers.rs`
//! in gx-chat-core puts under the question, drawn as one compact segmented pill, or stacked one
//! answer per row when the pill does not fit the line. The state is the
//! question's one-question form (`questionForms`): open, sending, queued, answered, and whether the
//! reader is changing the answer; this file only lays it out and sends the clicks.
//!
//! CDXC:SessionChat 2026-11-05 SEE-ALSO: The decision (Q1245 C, the segmented pill on the line under the question) is in packages/gx-chat-core/src/transcript/inline_answers.rs; the phone draws the same pill in apps/mobile/app/src/chat/native/transcript/markdown/InlineAnswers.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::{Value, json};

impl NativeChatView {
    /// The answers of one question (`mark`, the marked line's JSON), at the list depth the
    /// question sits at.
    pub(super) fn inline_answers(
        &self,
        key: &str,
        mark: &Value,
        depth: usize,
        p: &ChatAppearance,
        cx: &gpui::Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let form_id = mark["form"].as_str().unwrap_or_default().to_string();
        let form = &self.snapshot["questionForms"][&form_id];
        // Asked by a work item too: drawn once, as its ask card (gx-chat-core work_cards.rs
        // `card_tags`). By its tag, since a pill's form id can differ from the projected form's.
        let tag_asked = mark["tag"].as_str().is_some_and(|tag| {
            self.snapshot["workAskTags"].as_array().is_some_and(|tags| {
                tags.iter().any(|asked| {
                    asked
                        .as_str()
                        .is_some_and(|asked| asked.eq_ignore_ascii_case(tag))
                })
            })
        });
        if form["coveredByCard"] == true || tag_asked {
            return gpui::div().into_any_element();
        }
        let phase = form["state"].as_str().unwrap_or("open");
        let changing = form["changing"] == true;
        let withdrawn = form["tagWithdrawn"] == true || matches!(phase, "withdrawn" | "canceled");
        let picked = form["drafts"][0]["indices"][0].as_u64();
        let sent = matches!(phase, "sending" | "queued" | "held" | "answered");
        let clickable = !withdrawn && (phase == "open" || (changing && phase != "sending"));
        // A newer copy of this question sits further down; every copy answers it there.
        let asked_again = form["askedAgain"] == true && phase == "open" && !changing && !withdrawn;
        let own_answer = form["ownAnswer"]
            .as_str()
            .filter(|own| sent && !own.is_empty());
        let tag = mark["tag"].as_str().unwrap_or_default().to_string();
        let edge = p.foreground.opacity(0.16);
        let ink = p.foreground;
        let ink_text: gpui::Hsla = if p.light {
            rgb(0xfafafa).into()
        } else {
            rgb(0x111113).into()
        };
        let choices = mark["choices"].as_array().cloned().unwrap_or_default();
        let count = choices.len();
        let gutter = super::markdown_style::LIST_GUTTER * s * depth as f32;
        let stacked =
            self.inline_answers_overflow(&choices, sent, picked, clickable, gutter, p, cx);
        let segments = choices.iter().enumerate().map(|(index, choice)| {
            let label = choice["label"].as_str().unwrap_or_default().to_string();
            let chosen = sent && picked == Some(index as u64);
            let recommended = choice["recommended"] == true && !sent;
            let faded = (sent && !chosen && !changing) || withdrawn;
            let action = json!({"type":"inlineAnswer","form":form_id,"choice":index});
            div()
                .id(gpui::SharedString::from(format!(
                    "inline-answer:{key}:{index}"
                )))
                .flex()
                .items_center()
                .gap(px(5.0 * s))
                .px(px(11.0 * s))
                .when(!stacked, |segment| segment.h(px(24.0 * s)).flex_none())
                .when(stacked, |segment| {
                    segment.w_full().min_h(px(24.0 * s)).py(px(4.0 * s))
                })
                .text_size(px(12.5 * s))
                .font_weight(FontWeight::SEMIBOLD)
                .when(index > 0 && !stacked, |segment| {
                    segment.border_l_1().border_color(edge)
                })
                .when(index > 0 && stacked, |segment| {
                    segment.border_t_1().border_color(edge)
                })
                .when(chosen, |segment| segment.bg(ink).text_color(ink_text))
                .when(!chosen, |segment| {
                    segment.text_color(ink).bg(p.foreground.opacity(0.035))
                })
                .when(faded, |segment| segment.opacity(0.4))
                .when(recommended, |segment| {
                    segment.child(
                        div()
                            .size(px(5.0 * s))
                            .rounded_full()
                            .flex_none()
                            .bg(p.foreground.opacity(0.55)),
                    )
                })
                .when(!stacked, |segment| segment.child(label.clone()))
                .when(stacked, |segment| {
                    segment.child(div().flex_1().min_w_0().child(label.clone()))
                })
                .when(chosen, |segment| segment.child("✓"))
                .when(clickable && !(chosen && !changing), |segment| {
                    segment
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.09)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.invoke(action.clone(), cx);
                        }))
                })
                .when(recommended, |segment| {
                    segment.tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new("Recommended").build(window, cx)
                    })
                })
                .into_any_element()
        });
        // The last segment opens the Reply popover for the reader's own words (reply_popover.rs).
        let own = clickable.then(|| {
            let form = form_id.clone();
            let tag = tag.clone();
            div()
                .id(gpui::SharedString::from(format!("inline-answer-own:{key}")))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .when(!stacked, |segment| segment.h(px(24.0 * s)))
                .when(stacked, |segment| {
                    segment.min_h(px(24.0 * s)).py(px(4.0 * s))
                })
                .px(px(9.0 * s))
                .when(!stacked, |segment| segment.flex_none())
                .when(stacked, |segment| segment.w_full())
                .text_size(px(12.5 * s))
                .text_color(p.card_muted)
                .bg(p.foreground.opacity(0.035))
                .when(count > 0 && !stacked, |segment| {
                    segment.border_l_1().border_color(edge)
                })
                .when(count > 0 && stacked, |segment| {
                    segment.border_t_1().border_color(edge)
                })
                .role(gpui::Role::Button)
                .aria_label("Write your own answer")
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.09)).text_color(ink))
                // The app's pencil icon, not "✎": the web build's fonts have no such glyph and drew
                // a missing-character box there.
                .child(
                    gpui::svg()
                        .path("titlebar/pencil.svg")
                        .size(px(12.0 * s))
                        .flex_none()
                        .text_color(p.card_muted),
                )
                // Stacked, the pencil's row has the room to say what it does.
                .when(stacked, |segment| {
                    segment.child(div().flex_1().min_w_0().child("Write your own answer"))
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new("Write your own answer").build(window, cx)
                })
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_question_reply(Some(&form), &tag, event.position(), window, cx);
                    }),
                )
                .into_any_element()
        });
        let pill = div()
            .id(gpui::SharedString::from(format!("inline-answers:{key}")))
            .flex()
            .when(!stacked, |pill| pill.flex_none())
            .when(stacked, |pill| pill.flex_col().w_full())
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(edge)
            .overflow_hidden()
            .children(segments)
            .children(own);
        // The reader's own words, which no segment shows.
        let words = own_answer.map_or_else(String::new, |own| format!(" “{}”", short(own)));
        let status = if withdrawn {
            Some(("Withdrawn".to_string(), None))
        } else if changing {
            Some(("Pick another answer ·".to_string(), Some("Cancel")))
        } else if asked_again {
            Some(("Asked again below".to_string(), None))
        } else {
            match phase {
                "sending" => Some((format!("Sending{words}…"), None)),
                "queued" => Some((format!("Queued{words} ·"), Some("Change"))),
                // A Ghostex Collab guest's answer the chat's owner has not approved yet.
                "held" => Some((
                    format!("Waiting for {}{words}", text(form, "heldFor")),
                    None,
                )),
                "answered" => Some((format!("Sent{words} ·"), Some("Change"))),
                _ => None,
            }
        };
        let status = status.map(|(text, link)| {
            let toggle = json!({"type":"inlineAnswerChange","form":form_id});
            div()
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .text_size(px(11.5 * s))
                .text_color(p.card_muted)
                .child(text)
                .when_some(link, |row, link| {
                    row.child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "inline-answer-change:{key}"
                            )))
                            .underline()
                            .chat_cursor_pointer()
                            .hover(|style| style.text_color(p.foreground))
                            .child(link)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invoke(toggle.clone(), cx);
                            })),
                    )
                })
        });
        let row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .pl(px(gutter))
            .when(count > 0, |row| row.child(pill))
            .children(status)
            .into_any_element();
        self.open_question_frame(Some(&form_id), row, p)
    }
}

impl NativeChatView {
    /// Whether the pill on one line is wider than the transcript gives the answers at this depth, so
    /// they stack one per row instead.
    ///
    /// CDXC:SessionChat 2026-11-15 WHY:
    /// A question's three long answers ("Rebuild my desktop from the fix branch now", ...) drew as one segmented pill that ran past the chat's edge on a phone, the second cut to "Push the" and the third out of reach; a narrow desktop pane or the web build clipped it the same way. When the pill's measured width does not fit, the answers stack one per row at the full width and a long answer wraps inside its row. The phone's InlineAnswers.tsx stacks the same way, measured from its own layout.
    #[allow(clippy::too_many_arguments)]
    fn inline_answers_overflow(
        &self,
        choices: &[Value],
        sent: bool,
        picked: Option<u64>,
        clickable: bool,
        gutter: f32,
        p: &ChatAppearance,
        cx: &gpui::Context<Self>,
    ) -> bool {
        // Before the chat's first paint there is no width to compare with.
        if self.bounds.get().size.width.as_f32() <= 0.0 {
            return false;
        }
        let s = p.scale;
        let system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let font = gpui::Font {
            weight: FontWeight::SEMIBOLD,
            ..gpui::font(p.font.clone())
        };
        let measure = |text: &str| {
            let run = gpui::TextRun {
                len: text.len(),
                font: font.clone(),
                ..Default::default()
            };
            system
                .shape_line(text.to_owned().into(), px(12.5 * s), &[run], None)
                .width
                .as_f32()
        };
        let segments: f32 = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let label = choice["label"].as_str().unwrap_or_default();
                let chosen = sent && picked == Some(index as u64);
                let recommended = choice["recommended"] == true && !sent;
                let dot = if recommended { 10.0 * s } else { 0.0 };
                let check = if chosen {
                    5.0 * s + measure("✓")
                } else {
                    0.0
                };
                22.0 * s + dot + measure(label) + check + 1.0
            })
            .sum();
        let own = if clickable { 30.0 * s + 1.0 } else { 0.0 };
        // The row's 16px sides in the transcript column, the list depth, and the pill's border.
        // CDXC:SessionChat 2026-11-16 WHY: GitHub #638, #640 and #642: this ran
        // `transcript_text_width`, whose minimap gutter asks the transcript list how many rows it
        // holds, from inside that list's own layout (which holds the list's state mutably), so any
        // chat showing one-click answers with no ask card over them panicked ("RefCell already
        // mutably borrowed", gpui list.rs) and took the app down, on opening the chat or on every
        // launch that restored it. The width comes from this frame's render instead, measured
        // before the list lays out.
        let room = self.frame_text_width - 32.0 * s - gutter;
        segments + own + 2.0 > room
    }
}

/// The reader's own answer as the pill's status quotes it: its first line, at most 48 characters.
fn short(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let mut out: String = line.chars().take(48).collect();
    if out.len() < line.len() || text.trim().lines().count() > 1 {
        out.push('…');
    }
    out
}
