//! One-click answers an agent writes into its reply: a question line followed by one Markdown link
//! per answer, each pointing at `ghostex-answer:<tag>`.
//!
//! ```text
//! - PR #414: Review and merge it? [Yes](ghostex-answer:Q901) [No](ghostex-answer:Q901) [Later](ghostex-answer:Q901)
//! - Delete the temporary branch? [Delete](ghostex-answer:Q902 "recommended") [Keep](ghostex-answer:Q902=keep)
//! ```
//!
//! The link's text is the answer; `=value` sends a different value than the label, and the title
//! `"recommended"` marks the agent's pick. The question is the line's own words before its first
//! answer link; when the links sit on a line or sub-bullet of their own, it is the line above them
//! that names the tag, else their parent bullet, else the line above ([`asking_line`]).
//!
//! Each question becomes a one-question form ([`inline_forms`]), so everything a ```questions
//! form has works for it unchanged: the send (`questions/forms.rs`), the queued and sending states,
//! the answers card, the "?" pill and its Q-tag links, withdraw, and gxserver's sidebar count. The
//! reply's Markdown keeps the question line without its links, and a marked line under it
//! ([`ANSWERS_OPEN`]) tells the renderers to draw the answers there as one segmented pill.
//!
//! CDXC:SessionChat 2026-11-05 DECISION:
//! User: "for this kind of small question that u asked: "PR #414: Review and merge it?" i want to see the answers written as links next to it and when i click them they're send to u directly". From the mockups (issues-1027/inline-answers/inline-answers.html) they picked Q1245 C, a compact segmented pill on the line under the question; Q1246 C, a click sends the same `<ghostex-page-answers>` envelope a form sends, so the agent reads it like any form answer; Q1247 B, while the answer still waits in a queue a new pick replaces it, and once the agent has read it a new pick goes as a correction marked as changed from the old answer (`questions/inline_answers.rs`).
//! SEE-ALSO: questions/inline_answers.rs (clicks and changes), apps/desktop/src/app/native_chat/inline_answers.rs and apps/mobile/app/src/chat/native/transcript/markdown/InlineAnswers.tsx (the pill), skills/ghostex-visuals/SKILL.md and skills/ghostex-orchestrator/SKILL.md (how agents write it). Change them together.

use ghostex_gx_protocol::{ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::transcript::jsstr::{decode_uri, split_lf};
use crate::transcript::native_markdown::{fence_close, fence_open, NATIVE_MARK};
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_forms::{parse_form, QuestionForm};

/// The address an answer link carries.
pub const ANSWER_SCHEME: &str = "ghostex-answer:";
/// A marked line under a question: the renderers draw its answers as one segmented pill. The JSON
/// after the colon is [`InlineQuestion::mark_json`].
pub const ANSWERS_OPEN: &str = "\u{e000}answers:";
/// The most answers one question offers; links past it stay ordinary links.
const MAX_CHOICES: usize = 6;

/// One answer link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineChoice {
    pub label: String,
    pub value: String,
    pub recommended: bool,
}

/// One question with its answers, as a reply's line wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineQuestion {
    pub tag: String,
    pub question: String,
    pub choices: Vec<InlineChoice>,
}

impl InlineQuestion {
    /// The form's body: what [`parse_form`] reads, and what the form's id is hashed from. The
    /// `inline` key keeps the id apart from a ```questions block that asks the same thing.
    fn form_body(&self) -> String {
        let choices: Vec<Value> = self
            .choices
            .iter()
            .map(|choice| {
                json!({
                    "label": choice.label,
                    "value": choice.value,
                    "recommended": choice.recommended,
                })
            })
            .collect();
        json!({
            "inline": true,
            "title": self.question,
            "submit": "Send",
            "questions": [{
                "tag": self.tag,
                "name": self.tag,
                "title": self.question,
                "choices": choices,
            }],
        })
        .to_string()
    }

    /// The one-question form this question is.
    pub fn form(&self) -> QuestionForm {
        parse_form(&self.form_body())
    }

    /// What the marked line carries: the form's id, the tag, and the answers in order.
    pub fn mark_json(&self) -> String {
        json!({
            "form": self.form().id,
            "tag": self.tag,
            "question": self.question,
            "choices": self.choices.iter().map(|choice| json!({
                "label": choice.label,
                "value": choice.value,
                "recommended": choice.recommended,
            })).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

/// One answer link found on a line: where it sits, its tag, and its answer.
struct Found {
    start: usize,
    end: usize,
    tag: String,
    choice: InlineChoice,
}

/// The byte ranges of inline code on a line.
fn code_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    while let Some(at) = line[cursor..].find('`') {
        let open = cursor + at;
        let run = line[open..]
            .bytes()
            .take_while(|byte| *byte == b'`')
            .count();
        let fence = &line[open..open + run];
        match line[open + run..].find(fence) {
            Some(close) => {
                let end = open + run + close + run;
                spans.push((open, end));
                cursor = end;
            }
            None => cursor = open + run,
        }
    }
    spans
}

/// `ghostex-answer:<tag>[=value]` and an optional `"recommended"` title inside a link's
/// parentheses, as `(tag, value, recommended)`.
fn destination(inside: &str) -> Option<(String, Option<String>, bool)> {
    let inside = inside.trim();
    let rest = inside.strip_prefix(ANSWER_SCHEME)?;
    let (address, title) = match rest.find(char::is_whitespace) {
        Some(at) => (&rest[..at], rest[at..].trim()),
        None => (rest, ""),
    };
    let (tag, value) = match address.split_once('=') {
        Some((tag, value)) => (
            tag,
            Some(decode_uri(value).unwrap_or_else(|| value.to_string())),
        ),
        None => (address, None),
    };
    let tag_ok = !tag.is_empty()
        && tag.len() <= 16
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if !tag_ok {
        return None;
    }
    let title = title
        .trim_matches(|character| matches!(character, '"' | '\'' | '(' | ')'))
        .trim();
    let recommended = title.eq_ignore_ascii_case("recommended");
    Some((
        tag.to_string(),
        value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        recommended,
    ))
}

/// Every answer link on a line, outside inline code, in order.
fn answer_links(line: &str) -> Vec<Found> {
    if !line.contains(ANSWER_SCHEME) {
        return Vec::new();
    }
    let code = code_spans(line);
    let mut found = Vec::new();
    let mut scan = 0;
    while let Some(at) = line[scan..].find('[') {
        let open = scan + at;
        scan = open + 1;
        if code.iter().any(|(from, to)| open >= *from && open < *to) {
            continue;
        }
        // An image (`![…](…)`) is never an answer.
        if open > 0 && line.as_bytes()[open - 1] == b'!' {
            continue;
        }
        let Some(close) = line[open + 1..].find(']').map(|offset| open + 1 + offset) else {
            break;
        };
        let label = line[open + 1..close].trim();
        if label.is_empty() || label.contains('[') || !line[close + 1..].starts_with('(') {
            continue;
        }
        let Some(end) = line[close + 2..].find(')').map(|offset| close + 2 + offset) else {
            continue;
        };
        let Some((tag, value, recommended)) = destination(&line[close + 2..end]) else {
            continue;
        };
        let label = plain_text(label);
        found.push(Found {
            start: open,
            end: end + 1,
            tag,
            choice: InlineChoice {
                value: value.unwrap_or_else(|| label.clone()),
                label,
                recommended,
            },
        });
        scan = end + 1;
    }
    found
}

/// A list item's marker (`-`, `*`, `+`, `1.`, `1)`) and the space after it, at the start of
/// `text`: its length.
fn list_marker_len(text: &str) -> Option<usize> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let marker =
        if digits > 0 && digits <= 9 && matches!(text.as_bytes().get(digits), Some(b'.' | b')')) {
            digits + 1
        } else if matches!(text.as_bytes().first(), Some(b'-' | b'*' | b'+')) {
            1
        } else {
            return None;
        };
    let gap = text[marker..]
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    (gap > 0).then_some(marker + gap)
}

/// The column a line's own words start on: after its indent and its list marker.
fn content_column(line: &str) -> usize {
    let indent = line.len() - line.trim_start_matches(' ').len();
    indent + list_marker_len(&line[indent..]).unwrap_or(0)
}

/// Markdown's emphasis, code ticks, links and handover markers reduced to the words they show.
fn plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](").and_then(|close| {
            after[close + 2..]
                .find(')')
                .map(|end| (close, close + 2 + end))
        }) {
            Some((close, end)) => {
                out.push_str(&after[..close]);
                rest = &after[end + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.chars()
        .filter(|character| !matches!(*character, '*' | '`' | NATIVE_MARK))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|character: char| character == '_' || character == '~')
        .trim()
        .to_string()
}

/// The question a line asks: its words before `before`, without the list marker.
fn question_words(line: &str, before: usize) -> String {
    let text = &line[..before];
    let column = content_column(text).min(text.len());
    plain_text(&text[column..])
}

/// The words a line shows, without its list marker: what links on the next line ask.
pub fn line_words(line: &str) -> String {
    question_words(line, line.len())
}

/// Whether `text` holds `tag` as a word of its own, in any case.
fn names_tag(text: &str, tag: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    let upper = text.to_ascii_uppercase();
    let tag = tag.to_ascii_uppercase();
    while let Some(at) = upper[from..].find(&tag).map(|at| from + at) {
        let end = at + tag.len();
        let word = |byte: Option<&u8>| {
            byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        };
        if !word(at.checked_sub(1).and_then(|before| bytes.get(before))) && !word(bytes.get(end)) {
            return true;
        }
        from = end;
    }
    false
}

/// How far up [`asking_line`] looks for the line that asks.
const ASKING_REACH: usize = 40;

/// The line that asks `tag` for answer links on a line of their own at `lines[index]`: the nearest
/// line above them, inside their list item, that names the tag (the question bullet over a
/// sub-bullet of links, with other sub-bullets between); else their parent bullet; else the line
/// right above. `None` when nothing above them has words.
///
/// CDXC:SessionChat 2026-11-07 WHY:
/// The coordinator wrote Q1652's answer links as the last sub-bullet under "Q1652: Include an iOS TestFlight build with 11.0.0?", after a sibling sub-bullet "First I'd apply the saved iOS 27 crash fix."; the line right above the links became the question, so the reader's answer card was titled with that sentence. The question is now the line that holds the tag, as the reader sees it.
pub fn asking_line(lines: &[&str], index: usize, tag: &str) -> Option<usize> {
    let indent = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let own = indent(lines.get(index)?);
    let mut above: Option<usize> = None;
    let mut parent: Option<usize> = None;
    for at in (index.saturating_sub(ASKING_REACH)..index).rev() {
        let line = lines[at];
        if line.trim().is_empty() {
            // Links at the left edge belong to the paragraph right above them.
            if own == 0 && above.is_some() {
                break;
            }
            continue;
        }
        if fence_open(line).is_some() || fence_close(line).is_some() {
            break;
        }
        // Another line of answers asks its own question.
        if line.contains(ANSWER_SCHEME) && !answer_links(line).is_empty() {
            break;
        }
        above.get_or_insert(at);
        if names_tag(&line_words(line), tag) {
            return Some(at);
        }
        if indent(line) < own {
            parent.get_or_insert(at);
            // Past the parent bullet the lines belong to other items.
            break;
        }
    }
    parent.or(above)
}

/// The words [`asking_line`] finds for `tag`, or none.
fn asked_words(lines: &[&str], index: usize, tag: &str) -> String {
    asking_line(lines, index, tag)
        .map(|at| line_words(lines[at]))
        .unwrap_or_default()
}

/// One line's questions: each tag's answers, the question its words ask (`fallback` of the tag
/// when the line holds only the links), and the line with every answer link taken out.
fn read_line(
    line: &str,
    fallback: impl Fn(&str) -> String,
) -> Option<(Vec<InlineQuestion>, String)> {
    let links = answer_links(line);
    if links.is_empty() {
        return None;
    }
    let mut questions: Vec<InlineQuestion> = Vec::new();
    for link in &links {
        match questions
            .iter_mut()
            .find(|known| known.tag.eq_ignore_ascii_case(&link.tag))
        {
            Some(known) => {
                if known.choices.len() < MAX_CHOICES
                    && !known
                        .choices
                        .iter()
                        .any(|choice| choice.value == link.choice.value)
                {
                    known.choices.push(link.choice.clone());
                }
            }
            None => {
                let words = question_words(line, link.start);
                questions.push(InlineQuestion {
                    tag: link.tag.clone(),
                    question: if words.is_empty() {
                        fallback(&link.tag)
                    } else {
                        words
                    },
                    choices: vec![link.choice.clone()],
                });
            }
        }
    }
    questions.retain(|question| !question.question.is_empty());
    if questions.is_empty() {
        return None;
    }
    let mut stripped = String::with_capacity(line.len());
    let mut cursor = 0;
    for link in &links {
        stripped.push_str(&line[cursor..link.start]);
        cursor = link.end;
    }
    stripped.push_str(&line[cursor..]);
    // The separators an agent puts between links (`·`, `|`, `/`, `,`) go with them.
    let kept = stripped
        .trim_end_matches(|character: char| {
            character.is_whitespace() || matches!(character, '·' | '|' | '/' | ',' | '•')
        })
        .to_string();
    Some((questions, kept))
}

/// Calls `each` with every line of prose outside fenced code, with all the lines and its index.
fn each_prose_line(markdown: &str, mut each: impl FnMut(&[&str], usize)) {
    let lines = split_lf(markdown);
    let mut fence: Option<(u8, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some((character, length)) = fence {
            if fence_close(line).is_some_and(|(closing, run)| closing == character && run >= length)
            {
                fence = None;
            }
            continue;
        }
        if let Some((character, run, _)) = fence_open(line) {
            fence = Some((character, run));
            continue;
        }
        each(&lines, index);
    }
}

/// The questions a reply asks with answer links, in order, once per tag.
pub fn inline_questions(markdown: &str) -> Vec<InlineQuestion> {
    if !markdown.contains(ANSWER_SCHEME) {
        return Vec::new();
    }
    let mut questions: Vec<InlineQuestion> = Vec::new();
    each_prose_line(markdown, |lines, index| {
        if let Some((found, _)) = read_line(lines[index], |tag| asked_words(lines, index, tag)) {
            for question in found {
                if !questions
                    .iter()
                    .any(|known| known.tag.eq_ignore_ascii_case(&question.tag))
                {
                    questions.push(question);
                }
            }
        }
    });
    questions
}

/// The one-question forms the agent's replies ask with answer links, oldest first.
pub fn inline_forms(message: &ChatMessage) -> Vec<QuestionForm> {
    if message.role != ChatRole::Assistant {
        return Vec::new();
    }
    inline_questions(&prose_markdown(&message.blocks))
        .iter()
        .map(InlineQuestion::form)
        .collect()
}

/// One line of prose (`lines[index]`) with its answer links taken out and a marked line under it
/// per question, indented to the line's words so it stays inside the list item. `None` when the
/// line asks nothing. The lines around it give the question of links written on a line of their
/// own ([`asking_line`]).
pub fn mark_line(lines: &[&str], index: usize) -> Option<Vec<String>> {
    let line = *lines.get(index)?;
    let (questions, kept) = read_line(line, |tag| asked_words(lines, index, tag))?;
    let mut lines = Vec::with_capacity(questions.len() + 1);
    // A line that held only the links (beside its list marker or not) leaves nothing to show.
    let column = content_column(&kept).min(kept.len());
    let indent = if kept[column..].trim().is_empty() {
        content_column(line)
    } else {
        lines.push(kept.clone());
        column
    };
    for question in questions {
        lines.push(format!(
            "{}{ANSWERS_OPEN}{}",
            " ".repeat(indent),
            question.mark_json()
        ));
    }
    Some(lines)
}

/// The tags `lines[index]` asks with one-click answers: with the links on it, or as the line that
/// links on a line of their own further down ask ([`asking_line`]).
pub fn tags_asked_by(lines: &[&str], index: usize) -> Vec<String> {
    let Some(line) = lines.get(index) else {
        return Vec::new();
    };
    let mut tags: Vec<String> = read_line(line, |_| String::new())
        .map(|(questions, _)| questions.into_iter().map(|question| question.tag).collect())
        .unwrap_or_default();
    let end = lines.len().min(index + 1 + ASKING_REACH);
    for at in index + 1..end {
        let below = lines[at];
        if fence_open(below).is_some() {
            break;
        }
        if !below.contains(ANSWER_SCHEME) {
            continue;
        }
        for link in answer_links(below) {
            let alone = question_words(below, link.start).is_empty();
            if alone
                && !tags
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(&link.tag))
                && asking_line(lines, at, &link.tag) == Some(index)
            {
                tags.push(link.tag);
            }
        }
    }
    tags
}

/// The marked line's JSON, when `line` is one (at any indent).
pub fn marked_answers(line: &str) -> Option<Value> {
    serde_json::from_str(line.trim_start().strip_prefix(ANSWERS_OPEN)?).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn links_become_a_question_with_its_answers() {
        let questions = inline_questions(
            "❓ **Needs you:**\n- [PR #414](https://github.com/o/r/pull/414): Review and merge it? [Yes](ghostex-answer:Q901) [No](ghostex-answer:Q901) [Later](ghostex-answer:Q901)\n- Delete `fix/tab`? [Delete](ghostex-answer:Q902 \"recommended\") · [Keep](ghostex-answer:Q902=keep%20it)",
        );
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].tag, "Q901");
        assert_eq!(questions[0].question, "PR #414: Review and merge it?");
        let labels: Vec<&str> = questions[0]
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(labels, ["Yes", "No", "Later"]);
        assert_eq!(questions[1].question, "Delete fix/tab?");
        assert!(questions[1].choices[0].recommended);
        assert_eq!(questions[1].choices[1].value, "keep it");
    }

    #[test]
    fn links_on_their_own_line_ask_the_line_above() {
        let questions = inline_questions(
            "- Release now or wait?\n  [Now](ghostex-answer:Q5) [Wait](ghostex-answer:Q5)",
        );
        assert_eq!(questions[0].question, "Release now or wait?");
    }

    #[test]
    fn code_and_fences_are_left_alone() {
        assert!(inline_questions("Write `[Yes](ghostex-answer:Q1)` like this.").is_empty());
        assert!(inline_questions("```\nOk? [Yes](ghostex-answer:Q1)\n```").is_empty());
    }

    #[test]
    fn the_marked_line_sits_under_the_words_inside_the_item() {
        let lines = mark_line(
            &["- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)"],
            0,
        )
        .unwrap();
        assert_eq!(lines[0], "- Merge it?");
        assert!(lines[1].starts_with(&format!("  {ANSWERS_OPEN}")));
        let mark = marked_answers(&lines[1]).unwrap();
        assert_eq!(mark["tag"], "Q9");
        assert_eq!(mark["choices"][1]["value"], "No");
        let form = inline_questions("- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)")
            [0]
        .form();
        assert_eq!(mark["form"], form.id.as_str());
        assert!(form.error.is_empty());
        assert_eq!(form.questions[0].key, "Q9");
        assert_eq!(form.questions[0].tag, "Q9");
    }

    #[test]
    fn a_line_of_links_alone_keeps_only_the_mark() {
        let lines = mark_line(
            &[
                "Release?",
                "  [Now](ghostex-answer:Q5) [Wait](ghostex-answer:Q5)",
            ],
            1,
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with(&format!("  {ANSWERS_OPEN}")));
    }

    #[test]
    fn ordinary_links_are_not_answers() {
        assert!(mark_line(&["See [the PR](https://x.y/1)."], 0).is_none());
        assert!(mark_line(&["Bad [Yes](ghostex-answer:) tag."], 0).is_none());
    }

    fn message(id: &str, role: ChatRole, text: &str) -> ChatMessage {
        serde_json::from_value(json!({
            "id": id,
            "role": if role == ChatRole::User { "user" } else { "assistant" },
            "source": "transcript",
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    #[test]
    fn an_inline_question_waits_until_its_answers_arrive() {
        use crate::transcript::open_questions::{askers, tag_state, TagState};
        let ask = message(
            "a",
            ChatRole::Assistant,
            "- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)",
        );
        let found = askers(std::slice::from_ref(&ask), &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tags, ["Q9"]);
        assert_eq!(tag_state(&found, "Q9"), Some(TagState::Open));
        let form = inline_questions("- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)")
            [0]
        .form();
        let reply =
            crate::transcript::question_forms::answers_message(&form, &json!({"Q9": "Yes"}));
        let answered = crate::transcript::page_answers::page_answers_in(&[
            ask.clone(),
            message("u", ChatRole::User, &reply),
        ]);
        let ids: Vec<String> = answered
            .iter()
            .map(|answers| answers.page_id.clone())
            .collect();
        let found = askers(std::slice::from_ref(&ask), &ids);
        assert_eq!(tag_state(&found, "Q9"), Some(TagState::Answered));
    }

    #[test]
    fn only_an_agents_reply_is_marked() {
        use crate::transcript::native_markdown::native_markdown;
        let text = "- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)";
        assert!(native_markdown(text, false).contains(ANSWERS_OPEN));
        assert!(!native_markdown(text, true).contains(ANSWERS_OPEN));
    }

    /// The coordinator's first message asking Q1652: the answer links on the last sub-bullet,
    /// under a sibling sub-bullet that is no question.
    pub(crate) const Q1652_FIRST: &str = "❓ **Needs you:**\n- Q1652: Include an iOS TestFlight build with 11.0.0?\n  - It brings these to iPhones:\n    - Push notifications.\n    - The Files browser.\n  - First I'd apply the saved iOS 27 crash fix.\n  - [Yes, build iOS too](ghostex-answer:Q1652 \"recommended\") [No, Android only](ghostex-answer:Q1652)";
    /// Its next message, asking Q1652 again on one line while it was still open.
    pub(crate) const Q1652_AGAIN: &str = "❓ **Needs you:**\n- Q1652: Include an iOS TestFlight build with 11.0.0? It brings the Files browser to iPhones. [Yes, build iOS too](ghostex-answer:Q1652 \"recommended\") [No, Android only](ghostex-answer:Q1652)";

    #[test]
    fn links_on_their_own_sub_bullet_ask_the_bullet_that_names_the_tag() {
        let questions = inline_questions(Q1652_FIRST);
        assert_eq!(questions.len(), 1);
        assert_eq!(
            questions[0].question,
            "Q1652: Include an iOS TestFlight build with 11.0.0?"
        );
        // The pill's marked line carries the same form the answers card is titled from.
        let marked = crate::transcript::native_markdown::native_markdown(Q1652_FIRST, false);
        let mark = marked
            .lines()
            .find_map(marked_answers)
            .expect("a marked line");
        assert_eq!(mark["form"], questions[0].form().id.as_str());
        assert_eq!(mark["question"], questions[0].question.as_str());
        // The Q-tag on the question bullet is the question's own (a click opens Reply).
        let lines = split_lf(Q1652_FIRST);
        assert_eq!(tags_asked_by(&lines, 1), ["Q1652"]);
        assert!(tags_asked_by(&lines, 5).is_empty());
    }

    #[test]
    fn links_alone_without_a_tagged_line_ask_their_parent_bullet_then_the_line_above() {
        let questions = inline_questions(
            "- Ship the build now?\n  - It takes an hour.\n  - [Ship](ghostex-answer:Q8) [Wait](ghostex-answer:Q8)",
        );
        assert_eq!(questions[0].question, "Ship the build now?");
        let questions = inline_questions(
            "Ship the build now?\n[Ship](ghostex-answer:Q8) [Wait](ghostex-answer:Q8)",
        );
        assert_eq!(questions[0].question, "Ship the build now?");
    }

    #[test]
    fn a_tag_asked_in_two_messages_and_answered_on_the_first_is_answered_everywhere() {
        use crate::transcript::open_questions::{askers, tag_state, TagState};
        let first = message("a1", ChatRole::Assistant, Q1652_FIRST);
        let again = message("a2", ChatRole::Assistant, Q1652_AGAIN);
        let form = inline_questions(Q1652_FIRST)[0].form();
        let reply = crate::transcript::question_forms::answers_message(
            &form,
            &json!({"Q1652": "Yes, build iOS too"}),
        );
        // Answered on the first copy before the second was asked, and after it.
        for messages in [
            vec![
                first.clone(),
                message("u", ChatRole::User, &reply),
                again.clone(),
            ],
            vec![
                first.clone(),
                again.clone(),
                message("u", ChatRole::User, &reply),
            ],
        ] {
            let ids: Vec<String> = crate::transcript::page_answers::page_answers_in(&messages)
                .into_iter()
                .map(|answers| answers.page_id)
                .collect();
            assert_eq!(ids, [form.id.clone()]);
            let found = askers(&messages, &ids);
            assert_eq!(found.len(), 2, "one asker per copy");
            assert!(found.iter().all(|asker| asker.answered), "{found:?}");
            assert_eq!(tag_state(&found, "Q1652"), Some(TagState::Answered));
            // Nothing waits: the ? pill's count and Previous question drop it.
            assert!(!found.iter().any(|asker| !asker.answered && !asker.obsolete));
        }
        // Unanswered, the newest copy still waits.
        let found = askers(&[first, again], &[]);
        assert_eq!(tag_state(&found, "Q1652"), Some(TagState::Open));
    }

    #[test]
    fn a_tag_answered_in_a_questions_form_answers_its_one_click_copy() {
        use crate::transcript::open_questions::{askers, tag_state, TagState};
        let fenced = "```questions\n{\"questions\":[{\"tag\":\"Q3\",\"title\":\"Ship?\",\"choices\":[\"Yes\",\"No\"]},{\"tag\":\"Q4\",\"title\":\"Tag it?\",\"choices\":[\"Yes\",\"No\"]}]}\n```";
        let form = crate::transcript::question_forms::parse_form(
            fenced
                .trim_start_matches("```questions\n")
                .trim_end_matches("\n```"),
        );
        assert!(form.error.is_empty(), "{}", form.error);
        let messages = [
            message("a1", ChatRole::Assistant, fenced),
            message(
                "u",
                ChatRole::User,
                &crate::transcript::question_forms::answers_message(
                    &form,
                    &json!({form.questions[0].key.clone(): "Yes", form.questions[1].key.clone(): "No"}),
                ),
            ),
            message(
                "a2",
                ChatRole::Assistant,
                "- Q3: Ship? [Yes](ghostex-answer:Q3) [No](ghostex-answer:Q3)",
            ),
        ];
        let found = askers(&messages, &[form.id.clone()]);
        assert_eq!(tag_state(&found, "Q3"), Some(TagState::Answered));
        assert!(found.iter().all(|asker| asker.answered));
    }
}
