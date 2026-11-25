//! Message IDs: every message the user writes in a chat gets a number (M1, M2, …) that they can
//! point back to later instead of pasting the message again.
//!
//! The one place for what an ID looks like, which rows get one, how a message is pointed at
//! (a `[M12](message:M12)` pill, a typed `#M12`, a pasted `G9jt7.M12`) and what the agent gets for
//! it (the ID, and the start of the message quoted at the bottom of the prompt). gxserver hands out
//! the numbers (server/src/chat_message_numbers.rs) and adds the quote when it sends; every chat
//! (desktop, web and the phone's WebAssembly core) reads the same rules here.
//!
//! CDXC:SessionChat 2026-11-20 DECISION:
//! User, on the message IDs page: "i want all my messages i send u to have ID counting up so i can
//! reference an earlier message i sent u without needing to actually attach the message", with
//! "copy message id in the 3 dot under the message". Then picked: M12 counting per chat, written
//! G9jt7.M12 when pointing at another chat, Copy message ID copying that full form (Q4237); the
//! agent gets the ID and a quote of a few lines of the message at the bottom of the prompt, not the
//! whole message (Q4238); only the user's own messages get a number (Q4239); only new messages,
//! numbering from M1 once this shipped, no backfill of older chats (Q4240). And: "don't show the
//! ID under the message next to the timestamp": the ID is reached only through the message's ⋯
//! menu (Copy message ID, Reference in chat), the `#M` picker and the pill's hover. Numbers are
//! never given out twice after a rewind, a fork keeps the numbers up to where it was cut, and a
//! queued message gets its number once it is sent to the agent.
//! SEE-ALSO: server/src/chat_message_numbers.rs (the numbers, the quote on send, `ghostex
//! message`), composer/suggestions.rs (the `#M` picker), composer/reference_pills.rs and
//! transcript/markdown_links.rs (the pill), apps/desktop/src/app/native_chat/message_actions.rs and
//! apps/mobile/app/src/chat/native/transcript/MessageActions.tsx (the menu rows).

use ghostex_gx_protocol::{ChatMessage, ChatRole, ChatSource};
use serde_json::{Map, Value};

use crate::state::ChatRow;
use crate::transcript::jsstr::js_trim;

/// The scheme of a message pill's link: `[M12](message:M12)`, `[G9jt7.M12](message:G9jt7.M12)`.
pub const MESSAGE_LINK_SCHEME: &str = "message:";

/// The `kind` of a message pill's reference entry.
pub const MESSAGE_REFERENCE_KIND: &str = "message";

/// The most lines of a referenced message the agent is sent.
pub const QUOTE_MAX_LINES: usize = 6;
/// The most characters of a referenced message the agent is sent.
pub const QUOTE_MAX_CHARS: usize = 600;
/// The most referenced messages one prompt quotes; the rest are only named, on one line of the
/// block (`quotes_block`), and are not looked up.
pub const MAX_QUOTED_MESSAGES: usize = 5;

/// A message ID as written: the number, and the session when it points into another chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRef {
    /// `G9jt7` in `G9jt7.M12`; `None` for `M12`, a message of the chat it is written in.
    pub session_id: Option<String>,
    pub number: u32,
}

impl MessageRef {
    /// `M12` or `G9jt7.M12`, as it is written.
    pub fn written(&self) -> String {
        match &self.session_id {
            Some(session) => format!("{session}.M{}", self.number),
            None => format!("M{}", self.number),
        }
    }

    /// Whether this names a message of the chat `session_id`.
    pub fn is_in(&self, session_id: &str) -> bool {
        self.session_id
            .as_deref()
            .is_none_or(|own| own == session_id)
    }
}

/// The full ID of message `number` in session `session_id`: `G9jt7.M12`.
pub fn message_ref(session_id: &str, number: u32) -> String {
    format!("{session_id}.M{number}")
}

/// A session id as Ghostex writes it: `G`, a digit, then lowercase letters and digits.
fn is_session_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next() == Some('G')
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// `M12`, `m12`, `#M12`, `G9jt7.M12`, `#G9jt7.M12` or `message:…` as a [`MessageRef`]; `None` for
/// anything else.
pub fn parse_message_ref(value: &str) -> Option<MessageRef> {
    let value = js_trim(value);
    let value = value.strip_prefix(MESSAGE_LINK_SCHEME).unwrap_or(value);
    let value = value.strip_prefix('#').unwrap_or(value);
    let (session_id, tag) = match value.rsplit_once('.') {
        Some((session, tag)) if is_session_id(session) => (Some(session.to_string()), tag),
        Some(_) => return None,
        None => (None, value),
    };
    let digits = tag.strip_prefix(['M', 'm'])?;
    if digits.is_empty() || digits.len() > 9 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let number: u32 = digits.parse().ok()?;
    (number > 0).then_some(MessageRef { session_id, number })
}

/// The pill a message reference is in the chat box and in the sent message: `[M12](message:M12)`.
pub fn message_link(reference: &MessageRef) -> String {
    let written = reference.written();
    format!("[{written}]({MESSAGE_LINK_SCHEME}{written})")
}

/// What a message's Copy message ID copies, from its full ID (`G9jt7.M12`): `[M12](message:G9jt7.M12)`,
/// a link that pastes as the pill. Its words are the short ID, as Reference in chat writes them;
/// its link keeps the session, so the pill still points at this message pasted in another chat.
/// `None` for anything that is not a message ID.
///
/// CDXC:SessionChat 2026-11-25 DECISION:
/// User: "when i copy id of message make it actually copy the reference instead of just the id of
/// it pls (same for all similar cases pls, we copy the reference like this: [M573](message:M573)
/// not just message:M573 please so when i paste it then it looks better)". Supersedes Copy message
/// ID copying the bare full form (Q4237): the full form now lives in the link.
pub fn copied_message_link(full: &str) -> Option<String> {
    let reference = parse_message_ref(full)?;
    Some(format!(
        "[M{}]({MESSAGE_LINK_SCHEME}{})",
        reference.number,
        reference.written()
    ))
}

/// The chat box after a message's Reference in chat: `current` with `pill` at its end, a space
/// from the words before it and a space after it, so the user types on.
pub fn append_inline(current: &str, pill: &str) -> String {
    let separator = if current.is_empty() || current.ends_with(char::is_whitespace) {
        ""
    } else {
        " "
    };
    format!("{current}{separator}{pill} ")
}

/// Whether `message` is one the user wrote, which is what gets a number: a prompt row the agent
/// recorded, not a queued copy, an optimistic echo, Ghostex's own notes, another agent's message or
/// report, a background agent's report, or a row the chat hides or folds (local command output,
/// interruptions, hook context).
pub fn is_own_prompt(message: &ChatMessage) -> bool {
    message.role == ChatRole::User
        && message.source == ChatSource::Transcript
        && !message.queued
        && message.startup_delivery.is_none()
        && !crate::transcript::foreign::is_pending_message_id(&message.id)
        && crate::transcript::noise::classify_suppressed_turn(message).is_none()
        && !crate::session::appended_notes::is_system_only(message)
        && !crate::transcript::turns::is_agent_conversation(message)
        && !crate::transcript::turns::is_subagent_report(message)
        && !shown_text(message).trim().is_empty()
}

/// What the user wrote in `message`, as the chat shows it: without Ghostex's notes and quotes.
pub fn shown_text(message: &ChatMessage) -> String {
    let prose = crate::transcript::prose::prose_markdown(&message.blocks);
    neutralised(&crate::session::appended_notes::without_notes(&prose))
}

/// `text` with every `<ghostex-system>` and `</ghostex-system>` it holds broken by a zero-width
/// space, so words that talk about the tag (or quote a message that did) can never open or close
/// the block a quote travels in. They read the same.
///
/// CDXC:SessionChat 2026-11-20 WHY:
/// The review of the first build found a quoted `</ghostex-system>` ended the quote block early:
/// the rest of the quote showed in the user's bubble, and the next message's stored words carried
/// the leftover too. Users often write about Ghostex's own tags.
pub fn neutralised(text: &str) -> String {
    use crate::session::appended_notes::{SYSTEM_CLOSE, SYSTEM_OPEN};
    if !text.contains("ghostex-system>") {
        return text.to_string();
    }
    text.replace(SYSTEM_CLOSE, "<\u{200b}/ghostex-system>")
        .replace(SYSTEM_OPEN, "<\u{200b}ghostex-system>")
}

/// Every message ID `text` points at, in the order it first names them: the pills
/// (`[…](message:M12)`), and the IDs typed as `#M12`, `#G9jt7.M12` or `G9jt7.M12`. A bare `M12`
/// is plain text ("my M1 laptop"), and `#12` is a GitHub issue.
pub fn referenced_messages(text: &str) -> Vec<MessageRef> {
    let mut found: Vec<MessageRef> = Vec::new();
    let push = |reference: MessageRef, found: &mut Vec<MessageRef>| {
        if !found.contains(&reference) {
            found.push(reference);
        }
    };
    // The pills' links.
    let link_open = format!("]({MESSAGE_LINK_SCHEME}");
    let mut rest = text;
    while let Some(at) = rest.find(&link_open) {
        let after = &rest[at + link_open.len()..];
        let Some(close) = after.find(')') else {
            break;
        };
        if let Some(reference) = parse_message_ref(&after[..close]) {
            push(reference, &mut found);
        }
        rest = &after[close..];
    }
    // The typed ones: a token that starts a word with `#` or a session id.
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (index, &(at, character)) in chars.iter().enumerate() {
        if character != '#' && character != 'G' {
            continue;
        }
        let starts_word = index == 0
            || chars[index - 1].1.is_whitespace()
            || matches!(chars[index - 1].1, '(' | '"' | '\'' | ',' | ';');
        if !starts_word {
            continue;
        }
        let end = chars[index..]
            .iter()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '.' || *c == '#'))
            .map_or(text.len(), |(end, _)| *end);
        let token = text[at..end].trim_end_matches('.');
        let Some(reference) = parse_message_ref(token) else {
            continue;
        };
        // `G9jt7.M12` needs no `#`; a number alone does, or `M1` in prose would be one.
        if character == 'G' && reference.session_id.is_none() {
            continue;
        }
        push(reference, &mut found);
    }
    found
}

/// One referenced message, as the quote under a prompt names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotedMessage {
    /// The ID as the user wrote it: `M12`, or `G9jt7.M12` for another chat's message.
    pub reference: String,
    /// When it was sent, already written for the agent (`2026-11-19 16:59`).
    pub sent_at: Option<String>,
    /// Its words, or `None` when no message has that ID. May be only their start
    /// ([`quote_prefix`]) when `totals` gives the whole message's size.
    pub text: Option<String>,
    /// The whole message's lines and characters ([`quote_totals`]) when `text` is only its start.
    pub totals: Option<(usize, usize)>,
}

/// How much of a message gxserver keeps for quoting: comfortably more than one quote takes
/// ([`QUOTE_MAX_CHARS`]), so the quote never runs out of words; `ghostex message show` reads the
/// whole message from the transcript.
pub const QUOTE_PREFIX_CHARS: usize = 1_000;

/// `text` without blank lines at either end, as the quote counts it.
fn quoted_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let first = lines.iter().position(|line| !line.trim().is_empty());
    let last = lines.iter().rposition(|line| !line.trim().is_empty());
    match (first, last) {
        (Some(first), Some(last)) => lines[first..=last].to_vec(),
        _ => Vec::new(),
    }
}

/// The start of a message gxserver keeps for quoting it: its first [`QUOTE_PREFIX_CHARS`]
/// characters, blank lines at either end dropped.
pub fn quote_prefix(text: &str) -> String {
    quoted_lines(text)
        .join("\n")
        .chars()
        .take(QUOTE_PREFIX_CHARS)
        .collect()
}

/// The whole message's lines and characters as a quote counts them, kept with its
/// [`quote_prefix`] so the note under a quote says how much was left out.
pub fn quote_totals(text: &str) -> (usize, usize) {
    let lines = quoted_lines(text);
    let chars = lines.iter().map(|line| line.chars().count()).sum();
    (lines.len(), chars)
}

/// The start of `text` the agent is sent, as quoted lines, and the note on what was left out.
///
/// The rule: blank lines at either end are dropped; then at most [`QUOTE_MAX_LINES`] lines and
/// [`QUOTE_MAX_CHARS`] characters are kept, whole lines first. A line that would go past the
/// character budget is cut at the last space before it (at the budget itself when it has none) and
/// ends in "…". When anything was left out, the note says how much was kept and names the
/// command that prints the whole message.
pub fn quote_excerpt(text: &str, reference: &str) -> (Vec<String>, Option<String>) {
    excerpt(text, reference, None)
}

/// [`quote_excerpt`] of a message's start (`text`), with the whole message's `totals` for the note.
fn excerpt(
    text: &str,
    reference: &str,
    totals: Option<(usize, usize)>,
) -> (Vec<String>, Option<String>) {
    let lines = quoted_lines(text);
    let (total_lines, total_chars) = totals.unwrap_or_else(|| quote_totals(text));
    let mut kept: Vec<String> = Vec::new();
    let mut budget = QUOTE_MAX_CHARS;
    let mut cut_inside = false;
    for line in lines.iter().take(QUOTE_MAX_LINES) {
        let count = line.chars().count();
        if count <= budget {
            kept.push((*line).to_string());
            budget -= count;
            continue;
        }
        let head: String = line.chars().take(budget).collect();
        let head = match head.rfind(' ') {
            Some(space) if space > 0 => head[..space].trim_end().to_string(),
            _ => head,
        };
        if !head.is_empty() {
            kept.push(format!("{head}\u{2026}"));
        }
        cut_inside = true;
        break;
    }
    let all_lines = kept.len() == total_lines && !cut_inside;
    let command = format!("`ghostex message show {reference}` prints all of it");
    let note = if all_lines {
        None
    } else if cut_inside && kept.len() <= 1 && total_lines == 1 {
        Some(format!(
            "(the first {} of {total_chars} characters; {command})",
            QUOTE_MAX_CHARS.min(total_chars)
        ))
    } else {
        Some(format!(
            "(the first {} of {total_lines} lines; {command})",
            kept.len(),
        ))
    };
    let quoted = kept
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {}", neutralised(&line))
            }
        })
        .collect();
    (quoted, note)
}

/// The block under a prompt that quotes the messages it points at, in one `<ghostex-system>`
/// block: every chat already leaves those out of the bubble it draws, the phone apps already out
/// included (`session::appended_notes::without_notes`). `quotes` are the first
/// [`MAX_QUOTED_MESSAGES`] references, looked up; `unquoted` names the ones past them, which the
/// agent can read with `ghostex message show`. `None` for neither.
pub fn quotes_block(quotes: &[QuotedMessage], unquoted: &[String]) -> Option<String> {
    if quotes.is_empty() && unquoted.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for quote in quotes {
        match &quote.text {
            Some(text) => {
                let (lines, note) = excerpt(text, &quote.reference, quote.totals);
                let mut part = match &quote.sent_at {
                    Some(at) => format!("{} (sent {at}):", quote.reference),
                    None => format!("{}:", quote.reference),
                };
                for line in lines {
                    part.push('\n');
                    part.push_str(&line);
                }
                if let Some(note) = note {
                    part.push('\n');
                    part.push_str(&note);
                }
                parts.push(part);
            }
            None => parts.push(format!(
                "{}: Ghostex has no message with this ID.",
                quote.reference
            )),
        }
    }
    if !unquoted.is_empty() {
        parts.push(format!(
            "Also pointed at, not quoted (only the first {MAX_QUOTED_MESSAGES} are): {}. `ghostex message show <ID>` prints each.",
            unquoted.join(", ")
        ));
    }
    use crate::session::appended_notes::{SYSTEM_CLOSE, SYSTEM_OPEN};
    Some(format!(
        "{SYSTEM_OPEN}\nThe user pointed at their earlier messages above; Ghostex quotes the start of each:\n{}\n{SYSTEM_CLOSE}",
        parts.join("\n\n")
    ))
}

/// `text` with the quotes of the messages it points at under it, a blank line apart
/// ([`quotes_block`]).
pub fn with_quotes(text: &str, quotes: &[QuotedMessage], unquoted: &[String]) -> String {
    match quotes_block(quotes, unquoted) {
        Some(block) => format!("{}\n\n{block}", text.trim_end()),
        None => text.to_string(),
    }
}

/// A sent message's pill for a `message:` link, named by its link text until the transcript fills
/// in the message's words (`dress_message_references`).
pub fn message_reference(href: &str, label: &str) -> Option<Value> {
    let reference = parse_message_ref(href.strip_prefix(MESSAGE_LINK_SCHEME)?)?;
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    let label = js_trim(label);
    entry.insert(
        "label".to_string(),
        if label.is_empty() {
            reference.written()
        } else {
            label.to_string()
        }
        .into(),
    );
    entry.insert(
        "title".to_string(),
        format!("Message {}", reference.written()).into(),
    );
    entry.insert("kind".to_string(), MESSAGE_REFERENCE_KIND.into());
    entry.insert("number".to_string(), reference.number.into());
    Some(Value::Object(entry))
}

/// The first line of a message's words, cut to `max` characters, for a pill's tooltip and the
/// `#M` picker.
pub fn preview(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let line = link_labels(line);
    let line = line.as_str();
    let characters: Vec<char> = line.chars().collect();
    if characters.len() <= max {
        return line.to_string();
    }
    let mut out: String = characters[..max.saturating_sub(1)].iter().collect();
    out.push('\u{2026}');
    out
}

/// `line` with each Markdown link `[label](destination)` written as its label, as the chat shows
/// it: "same as M2", not "same as [M2](message:M2)".
fn link_labels(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else {
            break;
        };
        let Some(end) = after[close + 2..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&after[..close]);
        rest = &after[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// The message pills of `references` that point into this chat, titled with the message's first
/// line when it is loaded: "M12: make them 30px and use the accent colour…".
pub fn dress_message_references(references: &mut [Value], messages: &[ChatRow]) {
    for reference in references.iter_mut() {
        if reference["kind"] != MESSAGE_REFERENCE_KIND {
            continue;
        }
        let Some(href) = reference["href"].as_str() else {
            continue;
        };
        let Some(parsed) = href
            .strip_prefix(MESSAGE_LINK_SCHEME)
            .and_then(parse_message_ref)
        else {
            continue;
        };
        if parsed.session_id.is_some() {
            // Another chat's message, or this chat's written in full: matched by its full ID.
            let written = parsed.written();
            if let Some(found) = messages
                .iter()
                .find(|row| row.message_ref.as_deref() == Some(written.as_str()))
            {
                reference["title"] =
                    format!("{written}: {}", preview(&shown_text(found), 80)).into();
                reference["messageId"] = found.id.clone().into();
            }
            continue;
        }
        if let Some(found) = messages
            .iter()
            .find(|row| row.message_number == Some(parsed.number))
        {
            reference["title"] =
                format!("M{}: {}", parsed.number, preview(&shown_text(found), 80)).into();
            reference["messageId"] = found.id.clone().into();
        }
    }
}

/// One row of the `#M` picker: a message of this chat that has a number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageChoice {
    pub number: u32,
    /// Its first line (`preview`).
    pub preview: String,
}

/// The `#M` picker's rows: this chat's numbered messages that are loaded, newest first.
pub fn message_choices(messages: &[ChatRow]) -> Vec<MessageChoice> {
    let mut choices: Vec<MessageChoice> = messages
        .iter()
        .filter_map(|row| {
            let number = row.message_number?;
            Some(MessageChoice {
                number,
                preview: preview(&shown_text(row), 120),
            })
        })
        .collect();
    choices.sort_by(|a, b| b.number.cmp(&a.number));
    choices.dedup_by_key(|choice| choice.number);
    choices
}

/// The choices a `#M` query keeps: its digits start the number (`#M1` keeps M1, M12, M10…), or
/// its words are in the message (`#Maccent`). An empty query keeps them all.
pub fn filter_message_choices(choices: &[MessageChoice], query: &str) -> Vec<MessageChoice> {
    let query = js_trim(query);
    let query = query.strip_prefix(['M', 'm']).unwrap_or(query);
    if query.is_empty() {
        return choices.to_vec();
    }
    if query.chars().all(|c| c.is_ascii_digit()) {
        return choices
            .iter()
            .filter(|choice| choice.number.to_string().starts_with(query))
            .cloned()
            .collect();
    }
    let words = query.to_lowercase();
    choices
        .iter()
        .filter(|choice| choice.preview.to_lowercase().contains(&words))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(session: Option<&str>, number: u32) -> MessageRef {
        MessageRef {
            session_id: session.map(str::to_string),
            number,
        }
    }

    #[test]
    fn parses_every_written_form() {
        assert_eq!(parse_message_ref("M12"), Some(reference(None, 12)));
        assert_eq!(parse_message_ref("#m3"), Some(reference(None, 3)));
        assert_eq!(
            parse_message_ref("G9jt7.M12"),
            Some(reference(Some("G9jt7"), 12))
        );
        assert_eq!(
            parse_message_ref("message:G9jt7.M12"),
            Some(reference(Some("G9jt7"), 12))
        );
        assert_eq!(parse_message_ref("M0"), None);
        assert_eq!(parse_message_ref("M"), None);
        assert_eq!(parse_message_ref("12"), None);
        assert_eq!(parse_message_ref("Q12"), None);
        assert_eq!(parse_message_ref("x.M12"), None);
        assert_eq!(parse_message_ref("M12x"), None);
    }

    #[test]
    fn finds_pills_and_typed_ids_but_not_prose() {
        let text =
            "like [M12](message:M12) and #M3, see G9jt7.M40. My M1 laptop and #627 stay text; \
                    [M12](message:M12) again";
        assert_eq!(
            referenced_messages(text),
            vec![
                reference(None, 12),
                reference(None, 3),
                reference(Some("G9jt7"), 40)
            ]
        );
        assert!(referenced_messages("a GitHub issue #12 and an M4 chip").is_empty());
    }

    #[test]
    fn a_short_message_is_quoted_whole() {
        let (lines, note) = quote_excerpt("\nmake them 30px\n\nand blue\n", "M12");
        assert_eq!(lines, vec!["> make them 30px", ">", "> and blue"]);
        assert_eq!(note, None);
    }

    #[test]
    fn a_long_message_keeps_its_first_six_lines() {
        let text = (1..=14)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (lines, note) = quote_excerpt(&text, "M12");
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[5], "> line 6");
        assert_eq!(
            note.as_deref(),
            Some("(the first 6 of 14 lines; `ghostex message show M12` prints all of it)")
        );
        // gxserver keeps only the start, with the whole message's size: the quote is the same.
        let long = format!("{text}\n{}", "x".repeat(3_000));
        let prefix = quote_prefix(&long);
        assert_eq!(prefix.chars().count(), QUOTE_PREFIX_CHARS);
        assert_eq!(
            excerpt(&prefix, "M12", Some(quote_totals(&long))),
            quote_excerpt(&long, "M12")
        );
    }

    #[test]
    fn one_long_line_is_cut_at_a_word() {
        let text = "word ".repeat(200);
        let (lines, note) = quote_excerpt(&text, "G9jt7.M2");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("word\u{2026}"));
        assert!(lines[0].chars().count() <= QUOTE_MAX_CHARS + 3);
        assert_eq!(
            note.as_deref(),
            Some("(the first 600 of 999 characters; `ghostex message show G9jt7.M2` prints all of it)")
        );
    }

    #[test]
    fn the_quote_goes_in_a_block_every_chat_hides() {
        let sent = with_quotes(
            "same for the tab bar as [M12](message:M12)",
            &[
                QuotedMessage {
                    reference: "M12".into(),
                    sent_at: Some("2026-11-19 16:59".into()),
                    text: Some("make them 30px".into()),
                    totals: None,
                },
                QuotedMessage {
                    reference: "M99".into(),
                    sent_at: None,
                    text: None,
                    totals: None,
                },
            ],
            &["M7".to_string(), "G2.M8".to_string()],
        );
        assert!(sent.contains("M12 (sent 2026-11-19 16:59):\n> make them 30px"));
        assert!(sent.contains("M99: Ghostex has no message with this ID."));
        assert!(sent.contains(
            "Also pointed at, not quoted (only the first 5 are): M7, G2.M8. `ghostex message show <ID>` prints each."
        ));
        assert_eq!(
            crate::session::appended_notes::without_notes(&sent),
            "same for the tab bar as [M12](message:M12)"
        );
    }

    #[test]
    fn the_picker_filters_by_number_or_words() {
        let choices = vec![
            MessageChoice {
                number: 12,
                preview: "make them 30px".into(),
            },
            MessageChoice {
                number: 3,
                preview: "the accent colour".into(),
            },
            MessageChoice {
                number: 1,
                preview: "look first".into(),
            },
        ];
        let numbers = |query: &str| -> Vec<u32> {
            filter_message_choices(&choices, query)
                .iter()
                .map(|choice| choice.number)
                .collect()
        };
        assert_eq!(numbers("M1"), vec![12, 1]);
        assert_eq!(numbers("M"), vec![12, 3, 1]);
        assert_eq!(numbers("Maccent"), vec![3]);
    }

    #[test]
    fn hash_m_opens_the_message_list_and_a_pick_writes_the_pill() {
        use crate::composer::suggestions::{
            complete_spaced_mention, composer_suggestions, suggestion_popup,
            suggestion_replacement, SuggestionDismissals, SuggestionKind, SuggestionSources,
        };
        let sources = SuggestionSources {
            messages: vec![
                MessageChoice {
                    number: 12,
                    preview: "make them 30px".into(),
                },
                MessageChoice {
                    number: 1,
                    preview: "look first".into(),
                },
            ],
            ..SuggestionSources::default()
        };
        let open = |draft: &str| {
            let matches = composer_suggestions(
                draft,
                draft.encode_utf16().count(),
                &sources,
                SuggestionDismissals::default(),
                true,
            );
            suggestion_popup(&matches, &sources, draft, 0).map(|popup| (matches, popup))
        };
        let (matches, popup) = open("same as #M1").expect("the #M list opens");
        assert_eq!(popup.kind, SuggestionKind::Message);
        assert_eq!(popup.heading, "Your messages");
        assert_eq!(popup.rows[0].label, "M12");
        assert_eq!(popup.rows[0].detail, "make them 30px");
        let pill = suggestion_replacement(&matches, popup.kind, 0, "same as #M1").unwrap();
        assert_eq!(pill, "[M12](message:M12)");
        let (text, _) = complete_spaced_mention("same as #M1", 11, &pill).unwrap();
        assert_eq!(text, "same as [M12](message:M12) ");
        // A GitHub issue and a heading's word open nothing.
        assert!(open("see #12").is_none());
        assert!(open("#Make").is_none());
    }

    #[test]
    fn a_quoted_system_tag_cannot_end_the_block() {
        let quoted = "the </ghostex-system> tag shows up\nsecond line <ghostex-system>";
        let sent = with_quotes(
            "like [M1](message:M1)",
            &[QuotedMessage {
                reference: "M1".into(),
                sent_at: None,
                text: Some(quoted.into()),
                totals: None,
            }],
            &[],
        );
        assert_eq!(
            crate::session::appended_notes::without_notes(&sent),
            "like [M1](message:M1)"
        );
        assert_eq!(sent.matches("</ghostex-system>").count(), 1);
        assert!(!neutralised(quoted).contains("ghostex-system>\n"));
        assert!(!neutralised(quoted).contains("</ghostex-system>"));
    }

    #[test]
    fn a_preview_shows_links_as_their_words() {
        assert_eq!(
            preview("what did I ask in [M2](message:M2) and #M1?\nmore", 80),
            "what did I ask in M2 and #M1?"
        );
        assert_eq!(preview("a [b] c", 80), "a [b] c");
    }

    #[test]
    fn reference_in_chat_joins_the_words_a_space_apart() {
        assert_eq!(append_inline("", "[M3](message:M3)"), "[M3](message:M3) ");
        assert_eq!(
            append_inline("like", "[M3](message:M3)"),
            "like [M3](message:M3) "
        );
        assert_eq!(
            append_inline("like ", "[M3](message:M3)"),
            "like [M3](message:M3) "
        );
    }

    #[test]
    fn message_links_round_trip() {
        let link = message_link(&reference(Some("G9jt7"), 12));
        assert_eq!(link, "[G9jt7.M12](message:G9jt7.M12)");
        assert_eq!(
            referenced_messages(&link),
            vec![reference(Some("G9jt7"), 12)]
        );
        let entry = message_reference("message:M12", "M12").unwrap();
        assert_eq!(entry["kind"], "message");
        assert_eq!(entry["label"], "M12");
    }
}
