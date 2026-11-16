//! The pill a Reply in chat box puts in the chat box: `[<question's title>](answering:W7.Q12)`,
//! drawn as one pill inside the text, with the words after it as the answer. Several pills in one
//! message answer several questions, each with the words up to the next pill.
//!
//! The reader sees only the question's title (in the chat box, in the sent message, and in the
//! pill's tooltip); the agent reads the tags in the link, as it reads "W7 Q12: words". A pill that
//! answers a work item ask or a question settles it once the message reaches the agent (gxserver
//! `coordinators::settle_chat_replies`, gx-chat-core `tags_answered_in_words`). A whole form's or
//! page's pill (`regarding:`) only says what the words are about: those questions stay open, as
//! that row's Reply leaves them.
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! User, on issue #627 (Reply on an ask had no voice or pictures, and Add to prompt dropped
//! "W7 Q12:" into the chat box: "We're humans, not AIs. We don't know what W7 Q12 is!"), picked
//! both: voice and pictures in the Reply card, and "Reply in chat box". His note: what you are
//! replying to "must be a pill INSIDE the text, like the image pill the chat box has today (not a
//! chip attached above the box), so he can reply to several questions in one message; hovering the
//! pill shows a tooltip saying what it's about (the question's title). Never show tags like W7 Q12
//! to the user; what reaches the agent can still carry the tags."

use crate::composer::reference_pills::{composer_references, ReferenceKind};

/// A pill whose words answer the questions it names.
pub const ANSWERING: &str = "answering:";
/// A pill whose words are about a whole form or page, whose questions stay open.
pub const REGARDING: &str = "regarding:";

/// What a pill's link names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnswerPill {
    /// The work item first when there is one (`W7`), then the question tags (`Q12`).
    pub tags: Vec<String>,
    /// `answering:`: the words answer the questions it names.
    pub settles: bool,
}

impl AnswerPill {
    /// The work item it names (`7` of `W7`).
    pub fn item(&self) -> Option<i64> {
        self.tags.first()?.strip_prefix(['W', 'w'])?.parse().ok()
    }

    /// Its question tags, uppercased (`Q12`).
    pub fn questions(&self) -> Vec<String> {
        self.tags
            .iter()
            .filter(|tag| tag.starts_with(['Q', 'q']))
            .map(|tag| tag.to_ascii_uppercase())
            .collect()
    }
}

/// A tag the link may carry: `W7` or `Q12`.
fn tag_ok(tag: &str) -> bool {
    let mut chars = tag.chars();
    matches!(chars.next(), Some('W' | 'w' | 'Q' | 'q'))
        && tag.len() > 1
        && chars.all(|c| c.is_ascii_digit())
}

/// The pill a link names, `None` for any other link.
pub fn parse_href(href: &str) -> Option<AnswerPill> {
    let href = href.trim();
    let (rest, settles) = match href.strip_prefix(ANSWERING) {
        Some(rest) => (rest, true),
        None => (href.strip_prefix(REGARDING)?, false),
    };
    let tags: Vec<String> = rest.split('.').map(str::to_owned).collect();
    (!tags.is_empty() && tags.iter().all(|tag| tag_ok(tag))).then_some(AnswerPill { tags, settles })
}

/// The pill's Markdown for `name` (the question's title) and `tags` ("W7 Q12", "Q12 Q13"), with a
/// space after it for the words; `None` when there are no tags to name.
pub fn pill(name: &str, tags: &str, settles: bool) -> Option<String> {
    let tags: Vec<&str> = tags.split_whitespace().filter(|tag| tag_ok(tag)).collect();
    if tags.is_empty() {
        return None;
    }
    let name: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['[', ']'], "");
    let name = if name.is_empty() {
        tags.join(" ")
    } else {
        name
    };
    Some(format!(
        "[{name}]({}{}) ",
        if settles { ANSWERING } else { REGARDING },
        tags.join(".")
    ))
}

/// The pills in `text` with the words each one carries: from the end of the pill to the next pill
/// or the end of the text, trimmed.
pub fn answers_in(text: &str) -> Vec<(AnswerPill, String)> {
    // UTF-16 offset to byte offset.
    let mut bytes = Vec::with_capacity(text.len() + 1);
    for (byte, character) in text.char_indices() {
        for _ in 0..character.len_utf16() {
            bytes.push(byte);
        }
    }
    bytes.push(text.len());
    let at = |units: usize| bytes.get(units).copied().unwrap_or(text.len());
    let pills: Vec<(usize, usize, AnswerPill)> = composer_references(text)
        .into_iter()
        .filter(|reference| reference.kind == ReferenceKind::Answer)
        .filter_map(|reference| {
            Some((
                at(reference.start),
                at(reference.end),
                parse_href(&reference.path)?,
            ))
        })
        .collect();
    pills
        .iter()
        .enumerate()
        .map(|(index, (_, end, pill))| {
            let next = pills
                .get(index + 1)
                .map_or(text.len(), |(start, _, _)| *start);
            (pill.clone(), text[*end..next.max(*end)].trim().to_string())
        })
        .collect()
}

/// The question tags the pills in `text` answer (`answering:` ones only), uppercased.
pub fn answered_tags(text: &str) -> Vec<String> {
    answers_in(text)
        .into_iter()
        .filter(|(pill, _)| pill.settles)
        .flat_map(|(pill, _)| pill.questions())
        .collect()
}

/// What hovering the pill says: the question's title.
pub fn tooltip(label: &str) -> String {
    format!("Your answer to: {}", label.trim())
}

/// The chat box after a Reply in chat box: `draft` with the pill and the typed `reply` (its
/// pictures numbered after the ones already there) appended at its end, as Add to Chat appends.
/// `None` when there are no tags to name.
pub fn in_chat_box(draft: &str, name: &str, tags: &str, settles: bool, reply: &str) -> Option<String> {
    let added = format!("{}{}", pill(name, tags, settles)?, reply.trim());
    let added = crate::composer::references::renumber_image_references(draft, &added);
    Some(crate::composer::transcript_menu::append_draft_text(
        draft, &added,
    ))
}
