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

use serde_json::Value;

use crate::composer::reference_pills::{composer_references, ReferenceKind};
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};

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

/// What hovering the pill says: the question's title, then how its questions stand when `states`
/// (the document's `questionStates`) knows them. Never its tags (CDXC:Coordinators 2026-11-16
/// above).
pub fn tooltip(label: &str, href: &str, states: &Value) -> String {
    let Some(pill) = parse_href(href) else {
        return format!("Your answer to: {}", label.trim());
    };
    let title = format!(
        "{}: {}",
        if pill.settles {
            "Your answer to"
        } else {
            "About"
        },
        label.trim()
    );
    let known: Vec<&str> = pill
        .questions()
        .iter()
        .filter_map(|tag| states.get(tag.as_str())?.as_str())
        .collect();
    let Some(first) = known.first() else {
        return title;
    };
    if known.iter().all(|state| state == first) {
        return format!("{title}\n{}", state_words(first));
    }
    // Questions that stand differently: how many stand each way ("1 answered · 2 open").
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for state in known {
        match counts.iter_mut().find(|(seen, _)| *seen == state) {
            Some((_, count)) => *count += 1,
            None => counts.push((state, 1)),
        }
    }
    let line = counts
        .iter()
        .map(|(state, count)| format!("{count} {state}"))
        .collect::<Vec<_>>()
        .join(" · ");
    format!("{title}\n{line}")
}

/// A question state's words in the hover.
fn state_words(state: &str) -> &str {
    match state {
        "open" => "Open, waiting for your answer",
        "answered" => "Answered",
        "withdrawn" => "Withdrawn",
        "skipped" => "Skipped",
        other => other,
    }
}

/// How each question this chat asked stands, by tag uppercased (`Q12`): `open`, `answered`,
/// `withdrawn` or `skipped`. A work item's ask says it for its tag, over a form or page asking the
/// same tag. The pills' hover reads it ([`tooltip`]).
pub fn question_states(state: &ChatState) -> Value {
    let mut states = serde_json::Map::new();
    let askers = &*state.transcript_view.askers;
    for tag in askers.iter().flat_map(|asker| asker.tags.iter()) {
        let tag = tag.to_ascii_uppercase();
        if states.contains_key(&tag) {
            continue;
        }
        if let Some(found) = crate::transcript::open_questions::tag_state(askers, &tag) {
            states.insert(tag, found.as_str().into());
        }
    }
    let asks = crate::extras::work_source::work_of(state)
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| item["asks"].as_array().into_iter().flatten());
    for ask in asks {
        let Some(tag) = ask["tag"].as_str().filter(|tag| !tag.is_empty()) else {
            continue;
        };
        let how = if ask["open"] == true {
            "open"
        } else if ask["withdrawn"] == true {
            "withdrawn"
        } else {
            "answered"
        };
        states.insert(tag.to_ascii_uppercase(), how.into());
    }
    Value::Object(states)
}

/// A sent pill was clicked: the question opens where it lives. A work item's ask opens on the Work
/// panel with its row picked; a form's or page's question scrolls to its card in the chat (or opens
/// its page), as a Q-tag link does (`open_questions::jump_to_tag`); a pill naming only a work item
/// opens that item on the Work panel. One this chat no longer has says so in a toast, without tags.
pub fn open(state: &mut ChatState, context: &ChatContext, href: &str) -> Vec<Effect> {
    let Some(pill) = parse_href(href) else {
        return Vec::new();
    };
    use crate::extras::coordinator_board::{focus_row, work_panel_group};
    let questions = pill.questions();
    for tag in &questions {
        if let Some(key) = crate::extras::work_cards::ask_key_of_tag(state, tag) {
            // A settled ask leaves the panel's Needs you rows: its item's row opens instead.
            if work_panel_group(state, &key).is_none() {
                if let Some(item) = item_key_of_ask(state, tag) {
                    if work_panel_group(state, &item).is_some() {
                        return focus_row(state, &item);
                    }
                }
            }
            return focus_row(state, &key);
        }
        let asked =
            crate::transcript::open_questions::tag_target(&state.transcript_view.askers, tag)
                .is_some()
                || crate::questions::open_questions::newest_one_click_ask(state, tag).is_some();
        if asked {
            return crate::questions::open_questions::jump_to_tag(state, context, tag);
        }
    }
    if questions.is_empty() {
        let item = pill
            .item()
            .and_then(|number| work_item_key(state, |item| item["ref"] == format!("W{number}")));
        if let Some(key) = item {
            return focus_row(state, &key);
        }
    }
    vec![Effect::Toast {
        level: "info".to_string(),
        message: "That question isn't in this chat any more.".to_string(),
    }]
}

/// The Work panel row key of the work item holding the ask `tag`.
fn item_key_of_ask(state: &ChatState, tag: &str) -> Option<String> {
    work_item_key(state, |item| {
        item["asks"].as_array().is_some_and(|asks| {
            asks.iter().any(|ask| {
                ask["tag"]
                    .as_str()
                    .is_some_and(|known| known.eq_ignore_ascii_case(tag))
            })
        })
    })
}

/// The Work panel row key of the first work item `pick` takes.
fn work_item_key(state: &ChatState, pick: impl Fn(&Value) -> bool) -> Option<String> {
    crate::extras::work_source::work_of(state)?
        .get("items")?
        .as_array()?
        .iter()
        .find(|item| pick(item))
        .and_then(|item| item["key"].as_str())
        .filter(|key| !key.is_empty())
        .map(str::to_string)
}

/// The chat box after a Reply in chat box: `draft` with the pill and the typed `reply` (its
/// pictures numbered after the ones already there) appended at its end, as Add to Chat appends.
/// `None` when there are no tags to name.
pub fn in_chat_box(
    draft: &str,
    name: &str,
    tags: &str,
    settles: bool,
    reply: &str,
) -> Option<String> {
    let added = format!("{}{}", pill(name, tags, settles)?, reply.trim());
    let added = crate::composer::references::renumber_image_references(draft, &added);
    Some(crate::composer::transcript_menu::append_draft_text(
        draft, &added,
    ))
}
