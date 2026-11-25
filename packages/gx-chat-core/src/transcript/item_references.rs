//! References to the things a chat names by a tag, beside a message's own (`message_numbers.rs`):
//! a question (`[Which layout should I build?](question:Q4933)`), a Live page
//! (`[Settings redesign](page:7fb3c974…)`) and a work item (`[Fix the login check](board:B1071)`,
//! the board link `extras/coordinator_board.rs` already reads). Every one is a Markdown link whose
//! words are a short label ([`reference_label`]) and whose destination names the thing, so a
//! paste shows the pill, the agent reads the tag, and gxserver quotes what it points at at the
//! bottom of the prompt, as it quotes a message (server/src/chat_message_numbers.rs).
//!
//! CDXC:SessionChat 2026-11-25 DECISION:
//! User, on the "Point at questions and work items from the chat box" page: Copy reference and
//! Reference in chat for questions, work items, Live pages and threads ("make threads and session
//! say Copy Reference"), on the dock and Needs you rows, work cards and Work panel items, each
//! question of a chat form, Live page cards and the phone's sheets; one pair of names everywhere;
//! the chip is the title with the kind's icon, the tag in its hover; and the agent gets the
//! question, the work item and the page quoted. And of messages: "include it's first non empty and
//! not super short line there in the message reference but up to 25 chatacters then truncate ...
//! this is so we dont show the message id raw and instead show some text": every reference's
//! words are [`reference_label`] of its text or title.
//! SEE-ALSO: transcript/message_numbers.rs (messages), extras/coordinator_board.rs (`board:`),
//! questions/answer_pills.rs (`answering:` and `regarding:`, which answer; these only point).

use serde_json::{Map, Value};

use crate::transcript::jsstr::js_trim;

/// The scheme of a question's link: `question:Q4933`, `question:Q12.Q13` for a form's open ones,
/// `question:G9jt7.Q4933` copied for pasting into another chat.
pub const QUESTION_LINK_SCHEME: &str = "question:";
/// The scheme of a Live page's link: `page:<the page's id>`.
pub const PAGE_LINK_SCHEME: &str = "page:";
/// The `kind` of a question pill's reference entry.
pub const QUESTION_REFERENCE_KIND: &str = "questionRef";
/// The `kind` of a Live page pill's reference entry.
pub const PAGE_REFERENCE_KIND: &str = "pageRef";

/// The most characters a reference's words keep, the ellipsis included.
pub const LABEL_MAX_CHARS: usize = 25;
/// A line shorter than this ("ok", "-", "1.") is passed over for the next one.
const SHORT_LINE_CHARS: usize = 4;

/// A reference's words: the first line of `text` that is not empty and not super short, with list
/// and quote marks and link markup taken off, cut to [`LABEL_MAX_CHARS`] with "…"; the first line
/// that is not empty when every line is short; `fallback` (the tag) when there is none.
pub fn reference_label(text: &str, fallback: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(clean_line)
        .filter(|line| !line.is_empty())
        .collect();
    let line = lines
        .iter()
        .find(|line| line.chars().count() >= SHORT_LINE_CHARS)
        .or_else(|| lines.first())
        .cloned()
        .unwrap_or_else(|| fallback.trim().to_string());
    cut(&line, LABEL_MAX_CHARS)
}

/// `line` as words a pill can hold: no leading list, quote or heading marks, links as their
/// words, no square brackets (they would end the link), spaces run together.
fn clean_line(line: &str) -> String {
    let mut rest = line.trim();
    loop {
        let before = rest;
        for mark in ["- ", "* ", "+ ", "> ", "• "] {
            rest = rest.strip_prefix(mark).unwrap_or(rest).trim_start();
        }
        let hashes = rest.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && rest[hashes..].starts_with(' ') {
            rest = rest[hashes..].trim_start();
        }
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && (rest[digits..].starts_with(". ") || rest[digits..].starts_with(") ")) {
            rest = rest[digits + 2..].trim_start();
        }
        if rest == before {
            break;
        }
    }
    let rest = if matches!(rest, "-" | "*" | "+" | ">" | "•") {
        ""
    } else {
        rest
    };
    crate::transcript::message_numbers::link_labels(rest)
        .replace(['[', ']'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` cut to `max` characters, the last one "…" when it was longer.
pub fn cut(text: &str, max: usize) -> String {
    let characters: Vec<char> = text.chars().collect();
    if characters.len() <= max {
        return text.to_string();
    }
    let mut out: String = characters[..max.saturating_sub(1)]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_string();
    out.push('\u{2026}');
    out
}

/// What a question link names: its question tags (`Q4933`), and the session when it was copied
/// from another chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionRef {
    pub session_id: Option<String>,
    pub tags: Vec<String>,
}

impl QuestionRef {
    /// `Q4933`, `Q12.Q13` or `G9jt7.Q4933`, as the link writes it.
    pub fn written(&self) -> String {
        let tags = self.tags.join(".");
        match &self.session_id {
            Some(session) => format!("{session}.{tags}"),
            None => tags,
        }
    }

    /// Its tags as words: `Q4933`, `Q12, Q13`.
    pub fn tags_text(&self) -> String {
        self.tags.join(", ")
    }
}

fn is_session_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next() == Some('G')
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// `question:Q4933` (or the part after the scheme) as a [`QuestionRef`]; `None` for anything else.
pub fn parse_question_href(href: &str) -> Option<QuestionRef> {
    let value = js_trim(href);
    let value = value.strip_prefix(QUESTION_LINK_SCHEME)?;
    let mut parts: Vec<&str> = value.split('.').collect();
    let session_id = match parts.first() {
        Some(first) if is_session_id(first) => {
            let session = first.to_string();
            parts.remove(0);
            Some(session)
        }
        _ => None,
    };
    let tags: Vec<String> = parts
        .iter()
        .map(|tag| {
            let digits = tag.strip_prefix(['Q', 'q'])?;
            (!digits.is_empty()
                && digits.len() <= 9
                && digits.chars().all(|c| c.is_ascii_digit())
                && digits.parse::<u32>().ok()? > 0)
                .then(|| format!("Q{digits}"))
        })
        .collect::<Option<Vec<_>>>()?;
    (!tags.is_empty()).then_some(QuestionRef { session_id, tags })
}

/// The id a `page:` link names; `None` for anything else.
pub fn parse_page_href(href: &str) -> Option<String> {
    let id = js_trim(href).strip_prefix(PAGE_LINK_SCHEME)?.trim();
    (!id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| id.to_ascii_lowercase())
}

/// A question's link: `[<label of its title>](question:<tags>)`, the session first when
/// `session_id` is given (Copy reference, for pasting into any chat). `None` with no valid tag.
pub fn question_link(title: &str, session_id: Option<&str>, tags: &[String]) -> Option<String> {
    let tags: Vec<String> = tags
        .iter()
        .filter_map(|tag| {
            parse_question_href(&format!("{QUESTION_LINK_SCHEME}{tag}")).map(|r| r.tags)
        })
        .flatten()
        .collect();
    let first = tags.first()?.clone();
    let reference = QuestionRef {
        session_id: session_id.filter(|id| is_session_id(id)).map(str::to_string),
        tags,
    };
    Some(format!(
        "[{}]({QUESTION_LINK_SCHEME}{})",
        reference_label(title, &first),
        reference.written()
    ))
}

/// A Live page's link: `[<label of its title>](page:<id>)`. `None` without an id.
pub fn page_link(title: &str, id: &str) -> Option<String> {
    let id = parse_page_href(&format!("{PAGE_LINK_SCHEME}{id}"))?;
    Some(format!(
        "[{}]({PAGE_LINK_SCHEME}{id})",
        reference_label(title, "Live page")
    ))
}

/// A work item's link: `[<label of its title>](board:B<n>)`, from its number or its `W<n>`/`B<n>`.
pub fn work_item_link(title: &str, reference: &str) -> Option<String> {
    let number: u64 = reference
        .trim()
        .trim_start_matches(['W', 'w', 'B', 'b'])
        .parse()
        .ok()
        .filter(|number| *number > 0)?;
    Some(format!(
        "[{}](board:B{number})",
        reference_label(title, &format!("W{number}"))
    ))
}

/// A session's link with its title cut as every reference's words are: what a session's Copy
/// Reference and Reference in chat write. The Global Ref stays whole.
pub fn session_link(title: &str, global_ref: &str) -> String {
    format!("[{}]({})", reference_label(title, "Session"), global_ref.trim())
}

/// A sent message's pill for a `question:` link, named by its link text. Its hover names the tags
/// and, once the renderer has the document's `questionStates`, how they stand ([`tooltip`]).
pub fn question_reference(href: &str, label: &str) -> Option<Value> {
    let reference = parse_question_href(href)?;
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    entry.insert(
        "label".to_string(),
        crate::transcript::session_mentions::pill_label(label).into(),
    );
    entry.insert(
        "title".to_string(),
        tooltip(label, href, &Value::Null).into(),
    );
    entry.insert("kind".to_string(), QUESTION_REFERENCE_KIND.into());
    entry.insert("tags".to_string(), reference.tags.into());
    Some(Value::Object(entry))
}

/// A sent message's pill for a `page:` link, named by its link text.
pub fn page_reference(href: &str, label: &str) -> Option<Value> {
    let id = parse_page_href(href)?;
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    entry.insert(
        "label".to_string(),
        crate::transcript::session_mentions::pill_label(label).into(),
    );
    entry.insert(
        "title".to_string(),
        format!("Live page: {}", label.trim()).into(),
    );
    entry.insert("kind".to_string(), PAGE_REFERENCE_KIND.into());
    entry.insert("pageId".to_string(), id.into());
    Some(Value::Object(entry))
}

/// A question pill's hover: its tags and words, then how they stand when `states` (the document's
/// `questionStates`) knows them.
pub fn tooltip(label: &str, href: &str, states: &Value) -> String {
    let Some(reference) = parse_question_href(href) else {
        return label.trim().to_string();
    };
    let head = format!("{}: {}", reference.written(), label.trim());
    let known: Vec<&str> = reference
        .tags
        .iter()
        .filter_map(|tag| states.get(tag.as_str())?.as_str())
        .collect();
    match known.first() {
        Some(first) if known.iter().all(|state| state == first) => {
            format!("{head}\n{}", state_words(first))
        }
        Some(_) => format!("{head}\nSome answered, some open"),
        None => head,
    }
}

fn state_words(state: &str) -> &str {
    match state {
        "open" => "Open, waiting for your answer",
        "answered" => "Answered",
        "withdrawn" => "Withdrawn",
        "skipped" => "Skipped",
        other => other,
    }
}

/// Every question, page and work item `text` points at by link, in the order it first names them,
/// for gxserver's quote under the prompt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointedAt {
    pub questions: Vec<QuestionRef>,
    pub pages: Vec<String>,
    pub work_items: Vec<u64>,
}

impl PointedAt {
    pub fn is_empty(&self) -> bool {
        self.questions.is_empty() && self.pages.is_empty() && self.work_items.is_empty()
    }
}

/// The question, page and work item links in `text` (`](question:…)`, `](page:…)`,
/// `](board:B…)`).
pub fn pointed_at(text: &str) -> PointedAt {
    let mut found = PointedAt::default();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        let after = &rest[at + 2..];
        let Some(close) = after.find(')') else {
            break;
        };
        let href = &after[..close];
        if let Some(reference) = parse_question_href(href) {
            if !found.questions.contains(&reference) {
                found.questions.push(reference);
            }
        } else if let Some(id) = parse_page_href(href) {
            if !found.pages.contains(&id) {
                found.pages.push(id);
            }
        } else if let Some(item) = crate::extras::coordinator_board::board_item(href) {
            if !found.work_items.contains(&item) {
                found.work_items.push(item);
            }
        }
        rest = &after[close..];
    }
    found
}

/// One thing a prompt points at, looked up by gxserver: what the quote block says of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotedItem {
    /// The heading line: `Q4933 (asked 2026-11-25 14:02, open)`, `W1071 Fix the login check (in
    /// progress)`, `Live page "Settings redesign" (sent …)`.
    pub heading: String,
    /// The lines under it, already written; `None` when Ghostex has nothing with this tag or id.
    pub lines: Option<Vec<String>>,
}

/// The most lines a question, page or work item's quote keeps under its heading.
pub const ITEM_QUOTE_MAX_LINES: usize = 8;
/// How many of the questions, pages and work items a message points at are looked up and quoted;
/// the rest are named in one line ([`items_block`]).
pub const MAX_QUOTED_ITEMS: usize = 5;
/// The lines under a quote's heading that [`quoted_lines`] leaves for the facts after the words
/// (a question's choices, its answer and the command that prints it whole).
const ITEM_FACT_LINES: usize = 3;

/// The block under a prompt that quotes the questions, pages and work items it points at, in one
/// `<ghostex-system>` block, as [`crate::transcript::message_numbers::quotes_block`] quotes
/// messages. `unquoted` names the ones past the first [`MAX_QUOTED_ITEMS`]. `None` for neither.
pub fn items_block(items: &[QuotedItem], unquoted: &[String]) -> Option<String> {
    if items.is_empty() && unquoted.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = items
        .iter()
        .map(|item| match &item.lines {
            Some(lines) => {
                let mut part = format!("{}:", item.heading);
                for line in lines.iter().take(ITEM_QUOTE_MAX_LINES) {
                    part.push('\n');
                    part.push_str(&crate::transcript::message_numbers::neutralised(line));
                }
                part
            }
            None => format!("{}: Ghostex has nothing with this tag in this chat.", item.heading),
        })
        .collect();
    if !unquoted.is_empty() {
        parts.push(format!(
            "Also pointed at, not quoted (only the first {MAX_QUOTED_ITEMS} are): {}. `ghostex question show <tag>` prints a question.",
            unquoted.join(", ")
        ));
    }
    use crate::session::appended_notes::{SYSTEM_CLOSE, SYSTEM_OPEN};
    Some(format!(
        "{SYSTEM_OPEN}\nThe user pointed at these above; Ghostex quotes each:\n{}\n{SYSTEM_CLOSE}",
        parts.join("\n\n")
    ))
}

/// `text` with [`items_block`] under it, a blank line apart.
pub fn with_item_quotes(text: &str, items: &[QuotedItem], unquoted: &[String]) -> String {
    match items_block(items, unquoted) {
        Some(block) => format!("{}\n\n{block}", text.trim_end()),
        None => text.to_string(),
    }
}

/// Words quoted as `> ` lines, at most `max_chars` characters over the lines a quote has left once
/// its facts are written ([`ITEM_QUOTE_MAX_LINES`] less three), the last cut line ending in "…".
pub fn quoted_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut budget = max_chars;
    for line in text.lines().map(str::trim_end) {
        if out.len() >= ITEM_QUOTE_MAX_LINES - ITEM_FACT_LINES || budget == 0 {
            break;
        }
        if line.trim().is_empty() {
            if !out.is_empty() {
                out.push(">".to_string());
            }
            continue;
        }
        let count = line.chars().count();
        if count <= budget {
            out.push(format!("> {line}"));
            budget -= count;
        } else {
            out.push(format!("> {}", cut(line, budget.max(2))));
            break;
        }
    }
    while out.last().is_some_and(|line| line == ">") {
        out.pop();
    }
    out
}
