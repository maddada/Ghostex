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
//! answer link, or the line above when the links sit on a line of their own.
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

/// One line's questions: each tag's answers, the question its words ask (`fallback` when the line
/// holds only the links), and the line with every answer link taken out.
fn read_line(line: &str, fallback: &str) -> Option<(Vec<InlineQuestion>, String)> {
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
                        fallback.to_string()
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

/// Calls `each` with every line of prose outside fenced code, with the words of the line before it.
fn each_prose_line(markdown: &str, mut each: impl FnMut(usize, &str, &str)) {
    let lines = split_lf(markdown);
    let mut fence: Option<(u8, usize)> = None;
    let mut previous = String::new();
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
        each(index, line, &previous);
        if !line.trim().is_empty() {
            previous = line_words(line);
        }
    }
}

/// The questions a reply asks with answer links, in order, once per tag.
pub fn inline_questions(markdown: &str) -> Vec<InlineQuestion> {
    if !markdown.contains(ANSWER_SCHEME) {
        return Vec::new();
    }
    let mut questions: Vec<InlineQuestion> = Vec::new();
    each_prose_line(markdown, |_, line, previous| {
        if let Some((found, _)) = read_line(line, previous) {
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

/// One line of prose with its answer links taken out and a marked line under it per question,
/// indented to the line's words so it stays inside the list item. `None` when the line asks
/// nothing. `previous` is the words of the line above, for links written on a line of their own.
pub fn mark_line(line: &str, previous: &str) -> Option<Vec<String>> {
    let (questions, kept) = read_line(line, previous)?;
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

/// The marked line's JSON, when `line` is one (at any indent).
pub fn marked_answers(line: &str) -> Option<Value> {
    serde_json::from_str(line.trim_start().strip_prefix(ANSWERS_OPEN)?).ok()
}

#[cfg(test)]
mod tests {
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
            "- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)",
            "",
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
            "  [Now](ghostex-answer:Q5) [Wait](ghostex-answer:Q5)",
            "Release?",
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with(&format!("  {ANSWERS_OPEN}")));
    }

    #[test]
    fn ordinary_links_are_not_answers() {
        assert!(mark_line("See [the PR](https://x.y/1).", "").is_none());
        assert!(mark_line("Bad [Yes](ghostex-answer:) tag.", "").is_none());
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
}
