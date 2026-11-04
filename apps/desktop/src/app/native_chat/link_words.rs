//! A question's words with their links (the core's runs, packages/gx-chat-core/src/questions/form_links.rs):
//! each link in the text's own colour with the faint dotted underline a session name has on a
//! "Message to" card (`session_name_underline`), opening on a click the way a message's link does.
//! The question form (question_form.rs) and the "You answered" card (page_answers.rs) draw them.
//!
//! CDXC:SessionChat 2026-11-04 WHY: GPUI's text has no dotted underline (`UnderlineStyle` is solid or wavy) and the chat's Markdown view paints links in the link colour, so the words are laid out one by one in a wrapping row, a word space apart, and each link word gets the painted dots; a link's dots carry on under the space to its next word. The phone draws the same in apps/mobile/app/src/chat/native/transcript/LinkWords.tsx.

use super::inter_agent_message::session_name_underline;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::{Value, json};

/// The width of a word space in the chat font, in ems (inter_agent_message.rs uses the same).
const WORD_SPACE_EM: f32 = 0.27;

/// Words as `link_words` draws them.
pub(super) struct LinkWords<'a> {
    /// The words as plain text, drawn as they are when `runs` is not an array.
    pub plain: String,
    /// `[{text, href?, issue?}]` from the core, or null when there is nothing to link.
    pub runs: &'a Value,
    pub size: f32,
    pub line_height: f32,
    pub weight: FontWeight,
    pub color: Hsla,
}

impl NativeChatView {
    /// Plain text, or the words with their links when the core found any. A bare `#407` links to
    /// the project's GitHub issues when it has a GitHub `origin` (`issue_link_base`), else stays
    /// text. A click on a link opens it only, never the row it sits in.
    pub(super) fn link_words(
        &self,
        key: &str,
        words: LinkWords,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let line = div()
            .w_full()
            .min_w_0()
            .text_size(px(words.size * s))
            .line_height(px(words.line_height * s))
            .font_weight(words.weight)
            .text_color(words.color);
        let Some(runs) = words.runs.as_array() else {
            return line.child(words.plain).into_any_element();
        };
        // Each word as its pieces, a piece being some words and where they go.
        let mut list: Vec<Vec<(String, Option<String>)>> = vec![Vec::new()];
        for run in runs {
            let href = run["href"].as_str().map(str::to_string).or_else(|| {
                let issue = run["issue"].as_str()?;
                Some(format!("{}{issue}", self.issue_link_base.as_ref()?))
            });
            for (index, part) in run["text"]
                .as_str()
                .unwrap_or_default()
                .split(char::is_whitespace)
                .enumerate()
            {
                if index > 0 {
                    list.push(Vec::new());
                }
                if let (Some(word), false) = (list.last_mut(), part.is_empty()) {
                    word.push((part.to_string(), href.clone()));
                }
            }
        }
        list.retain(|word| !word.is_empty());
        let space = WORD_SPACE_EM * words.size * s;
        let color = words.color;
        let mut row = line.flex().flex_wrap().gap_x(px(space));
        for (word_index, word) in list.iter().enumerate() {
            let next_href = list
                .get(word_index + 1)
                .and_then(|next| next.first())
                .and_then(|(_, href)| href.as_ref());
            let mut pieces = div().flex().min_w_0();
            for (piece_index, (text, href)) in word.iter().enumerate() {
                let Some(href) = href.clone() else {
                    pieces = pieces.child(div().min_w_0().child(text.clone()));
                    continue;
                };
                // The dots run on under the space when the next word is the same link.
                let carries = piece_index + 1 == word.len() && next_href == Some(&href);
                let id = format!("{key}:{word_index}:{piece_index}");
                let tip = href.clone();
                pieces = pieces.child(
                    div()
                        .id(SharedString::from(id.clone()))
                        .relative()
                        .min_w_0()
                        .role(gpui::Role::Link)
                        .aria_label(SharedString::from(text.clone()))
                        .chat_cursor_pointer()
                        // Redraws the word as the pointer crosses it, which moves the dots' fade.
                        .hover(|style| style)
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.invoke(
                                json!({"type":"openMarkdownLink","href":href,"external":event.modifiers().shift}),
                                cx,
                            );
                        }))
                        .child(text.clone())
                        .child(session_name_underline(
                            format!("{id}:underline"),
                            0.0,
                            if carries { -space } else { 0.0 },
                            color,
                            s,
                        )),
                );
            }
            row = row.child(pieces);
        }
        row.into_any_element()
    }
}
