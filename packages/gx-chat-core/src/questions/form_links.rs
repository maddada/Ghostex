//! The links in a question form's words: a question's title and detail and a choice's detail turn
//! into Markdown the chat draws the way it draws a message, so a `[#407](https://…)` link or a bare
//! `https://…` address opens on a click, and every other character stays exactly as written.
//!
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User, on a form asking about "#407 (Arc-style layout)": "i'm annoyed u asked me here about #407 but i can't click on the link to open that pr. please fix for customers and for all future ones". A question's title and detail and a choice's detail show Markdown links (`[label](https://…)` or a file path) and bare http(s) addresses as links that open the way a message's links do, on the desktop and the phone. Text with no link stays plain text, drawn as before. Only the links are read: the rest is escaped, so `*`, `_` or a leading `1.` read as written. A choice's label stays plain (the whole row is its button) and shows a link's words without the markup. A bare `#407` is left for the renderer: the desktop links it to the project's GitHub issues (`issue_links`, the chat's own rule); the phone has no such helper and keeps it text. Agents are told to link what they ask about (skills/ghostex-visuals/SKILL.md, the coordinator playbook).
//!
//! CDXC:SessionChat 2026-11-04 SEE-ALSO:
//! apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx draw these; server/src/visual_pages/kit.rs links a page question's title the same way.

use serde_json::{json, Value};

use crate::transcript::links::{classify_link_href, LinkTarget};
use crate::transcript::markdown_links::markdown_link;

/// One piece of a text: words, or a link's words and where it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRun {
    pub text: String,
    pub href: Option<String>,
}

fn push_text(runs: &mut Vec<LinkRun>, text: &str) {
    if text.is_empty() {
        return;
    }
    match runs.last_mut() {
        Some(last) if last.href.is_none() => last.text.push_str(text),
        _ => runs.push(LinkRun {
            text: text.to_string(),
            href: None,
        }),
    }
}

/// Where a link's destination may go: a web address or a file, never an inert or unknown scheme.
fn openable(href: &str) -> bool {
    !href.is_empty() && classify_link_href(href) != LinkTarget::Inert
}

/// `[label](destination "title")` starting at `start` (its `[`): the label, the destination and
/// the byte after the `)`.
fn markdown_link_at(text: &str, start: usize) -> Option<(String, String, usize)> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut close = None;
    let mut at = start;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(at);
                    break;
                }
            }
            b'\n' => return None,
            _ => {}
        }
        at += 1;
    }
    let close = close?;
    if bytes.get(close + 1) != Some(&b'(') {
        return None;
    }
    let mut at = close + 2;
    while bytes.get(at) == Some(&b' ') {
        at += 1;
    }
    let (destination, mut after) = if bytes.get(at) == Some(&b'<') {
        let end = at + 1 + text[at + 1..].find(['>', '\n'])?;
        if bytes[end] != b'>' {
            return None;
        }
        (&text[at + 1..end], end + 1)
    } else {
        let mut parens = 0usize;
        let mut end = at;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => parens += 1,
                b')' if parens == 0 => break,
                b')' => parens -= 1,
                byte if byte.is_ascii_whitespace() => break,
                _ => {}
            }
            end += 1;
        }
        (&text[at..end], end)
    };
    while bytes.get(after) == Some(&b' ') {
        after += 1;
    }
    // An optional title: `"…"` or `'…'`.
    if let Some(&quote @ (b'"' | b'\'')) = bytes.get(after) {
        let end = after + 1 + text[after + 1..].find(quote as char)?;
        after = end + 1;
        while bytes.get(after) == Some(&b' ') {
            after += 1;
        }
    }
    if bytes.get(after) != Some(&b')') {
        return None;
    }
    let label = text[start + 1..close].trim();
    let destination = destination.trim();
    openable(destination).then(|| {
        (
            if label.is_empty() { destination } else { label }.to_string(),
            destination.to_string(),
            after + 1,
        )
    })
}

/// A bare `http://` or `https://` address starting at `start`, without the punctuation that ends
/// the sentence around it or a `)` it did not open (GFM's rule).
fn bare_url_at(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    let head = rest.get(..8)?.to_ascii_lowercase();
    if !head.starts_with("http://") && !head.starts_with("https://") {
        return None;
    }
    let run = rest
        .find(|character: char| character.is_whitespace() || character == '<')
        .unwrap_or(rest.len());
    let mut url = &rest[..run];
    loop {
        let trimmed = url.trim_end_matches(|character| ".,:;!?\"'*_~".contains(character));
        let trimmed = if trimmed.ends_with(')')
            && trimmed.matches(')').count() > trimmed.matches('(').count()
        {
            &trimmed[..trimmed.len() - 1]
        } else {
            trimmed
        };
        if trimmed.len() == url.len() {
            break;
        }
        url = trimmed;
    }
    let scheme = if head.starts_with("https") { 8 } else { 7 };
    (url.len() > scheme).then_some(start + url.len())
}

/// A text as words and links: Markdown links, `<https://…>` and bare http(s) addresses.
pub fn link_runs(text: &str) -> Vec<LinkRun> {
    let mut runs = Vec::new();
    let bytes = text.as_bytes();
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        let found = match bytes[at] {
            b'[' => markdown_link_at(text, at),
            b'<' => text[at + 1..]
                .find('>')
                .map(|end| (&text[at + 1..at + 1 + end], at + end + 2))
                .filter(|(inside, _)| {
                    !inside.contains(char::is_whitespace)
                        && bare_url_at(inside, 0) == Some(inside.len())
                })
                .map(|(inside, end)| (inside.to_string(), inside.to_string(), end)),
            b'h' | b'H'
                if at == 0
                    || !(bytes[at - 1].is_ascii_alphanumeric()
                        || matches!(bytes[at - 1], b'/' | b'_')) =>
            {
                bare_url_at(text, at)
                    .map(|end| (text[at..end].to_string(), text[at..end].to_string(), end))
            }
            _ => None,
        };
        match found {
            Some((label, href, end)) => {
                push_text(&mut runs, &text[copied..at]);
                runs.push(LinkRun {
                    text: label,
                    href: Some(href),
                });
                at = end;
                copied = end;
            }
            None => {
                at += 1;
                while !text.is_char_boundary(at) {
                    at += 1;
                }
            }
        }
    }
    push_text(&mut runs, &text[copied..]);
    runs
}

/// A `#` and a digit at the start of a word: a bare `#407` the desktop may link to the project's
/// GitHub issues (its `issue_links`, which decides the rest).
fn has_issue_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(2).enumerate().any(|(at, pair)| {
        pair[0] == b'#'
            && pair[1].is_ascii_digit()
            && (at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_'))
    })
}

/// Words as Markdown that reads exactly as written: every character Markdown would take as markup
/// is escaped, and each line's leading spaces go (four of them make a code block).
fn escape_text(text: &str, out: &mut String) {
    let mut line_start = out.is_empty() || out.ends_with('\n');
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if line_start && character == ' ' {
            continue;
        }
        if character == '\n' {
            // A line break the reader saw stays one.
            out.push_str("  \n");
            line_start = true;
            continue;
        }
        let at_line_start = line_start;
        line_start = false;
        // A `#` opens a heading only before a space (`#407` stays bare for the issue link).
        let heading = character == '#'
            && chars
                .peek()
                .is_none_or(|next| matches!(next, ' ' | '\t' | '\n' | '#'));
        if "\\`*_[]<>~|&$!{}".contains(character)
            || (at_line_start && (heading || ">-+=".contains(character)))
        {
            out.push('\\');
            out.push(character);
            continue;
        }
        out.push(character);
        // `1.` or `1)` at a line's start opens a list.
        if at_line_start && character.is_ascii_digit() {
            while let Some(&digit) = chars.peek().filter(|next| next.is_ascii_digit()) {
                out.push(digit);
                chars.next();
            }
            if let Some(&mark @ ('.' | ')')) = chars.peek() {
                out.push('\\');
                out.push(mark);
                chars.next();
            }
        }
    }
}

/// A link's words inside `[…]`: only what would end or break the link is escaped, so the label the
/// renderers look the reference up by is the label as written.
fn escape_label(label: &str, out: &mut String) {
    for character in label.chars() {
        if matches!(character, '\\' | '[' | ']' | '`' | '\n') {
            if character == '\n' {
                out.push(' ');
                continue;
            }
            out.push('\\');
        }
        out.push(character);
    }
}

/// A destination written inside `<…>`, which holds anything but angle brackets and line breaks.
fn safe_destination(href: &str) -> String {
    href.replace('<', "%3C")
        .replace('>', "%3E")
        .replace(['\n', '\r'], "")
}

/// The words with their links as `{"markdown", "references"}` for the renderers' Markdown, or
/// `None` when they hold nothing to link and stay plain text.
pub fn linked_text(text: &str) -> Option<Value> {
    let runs = link_runs(text);
    if runs.iter().all(|run| run.href.is_none()) && !has_issue_number(text) {
        return None;
    }
    let mut markdown = String::new();
    let mut references = Vec::new();
    for run in &runs {
        match &run.href {
            None => escape_text(&run.text, &mut markdown),
            Some(href) => {
                let href = safe_destination(href);
                markdown.push('[');
                escape_label(&run.text, &mut markdown);
                markdown.push_str("](<");
                markdown.push_str(&href);
                markdown.push_str(">)");
                references.extend(markdown_link(&href, &run.text));
            }
        }
    }
    Some(json!({ "markdown": markdown, "references": references }))
}

/// A choice label as its row shows it: a link's words without the Markdown around them.
pub fn shown_label(label: &str) -> Option<String> {
    let runs = link_runs(label);
    runs.iter()
        .any(|run| run.href.is_some())
        .then(|| runs.into_iter().map(|run| run.text).collect())
}

/// Adds the linked words to one form as the document carries it (`questions/forms.rs`, `project`):
/// `titleLinks` and `detailLinks` on a question, `detailLinks` and `shownLabel` on a choice, each
/// only when there is something to link.
pub fn add_links(form: &mut Value) {
    let Some(questions) = form.get_mut("questions").and_then(Value::as_array_mut) else {
        return;
    };
    for question in questions {
        let Some(question) = question.as_object_mut() else {
            continue;
        };
        for (field, key) in [("title", "titleLinks"), ("detail", "detailLinks")] {
            if let Some(links) = question
                .get(field)
                .and_then(Value::as_str)
                .and_then(linked_text)
            {
                question.insert(key.to_string(), links);
            }
        }
        let Some(choices) = question.get_mut("choices").and_then(Value::as_array_mut) else {
            continue;
        };
        for choice in choices {
            let Some(choice) = choice.as_object_mut() else {
                continue;
            };
            if let Some(links) = choice
                .get("detail")
                .and_then(Value::as_str)
                .and_then(linked_text)
            {
                choice.insert("detailLinks".to_string(), links);
            }
            if let Some(shown) = choice
                .get("label")
                .and_then(Value::as_str)
                .and_then(shown_label)
            {
                choice.insert("shownLabel".to_string(), shown.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(text: &str, href: &str) -> LinkRun {
        LinkRun {
            text: text.to_string(),
            href: Some(href.to_string()),
        }
    }

    fn words(text: &str) -> LinkRun {
        LinkRun {
            text: text.to_string(),
            href: None,
        }
    }

    #[test]
    fn a_markdown_link_is_a_link() {
        assert_eq!(
            link_runs("[#407](https://github.com/o/r/pull/407) (Arc-style layout): reply?"),
            vec![
                link("#407", "https://github.com/o/r/pull/407"),
                words(" (Arc-style layout): reply?"),
            ]
        );
    }

    #[test]
    fn a_bare_address_is_a_link_without_the_sentence_around_it() {
        assert_eq!(
            link_runs("See https://example.com/a_(b)/c. Then (https://x.dev/y)!"),
            vec![
                words("See "),
                link("https://example.com/a_(b)/c", "https://example.com/a_(b)/c"),
                words(". Then ("),
                link("https://x.dev/y", "https://x.dev/y"),
                words(")!"),
            ]
        );
        assert_eq!(
            link_runs("<https://a.dev/x> and HTTPS://B.DEV"),
            vec![
                link("https://a.dev/x", "https://a.dev/x"),
                words(" and "),
                link("HTTPS://B.DEV", "HTTPS://B.DEV"),
            ]
        );
    }

    #[test]
    fn a_file_link_is_a_link_and_an_inert_one_is_not() {
        assert_eq!(
            link_runs("Open [kit.rs](/repo/server/src/kit.rs:12 \"the kit\") first"),
            vec![
                words("Open "),
                link("kit.rs", "/repo/server/src/kit.rs:12"),
                words(" first"),
            ]
        );
        assert_eq!(
            link_runs("[x](javascript:alert(1)) and [y](#top) and [z]"),
            vec![words("[x](javascript:alert(1)) and [y](#top) and [z]")]
        );
    }

    #[test]
    fn text_without_links_stays_plain() {
        assert_eq!(linked_text("Which layout? Use *tabs* or 1. pages"), None);
        assert_eq!(linked_text("Ship it, colour #fff?"), None);
        assert_eq!(linked_text("a word http:// alone"), None);
    }

    #[test]
    fn the_markdown_escapes_everything_but_the_links() {
        let links = linked_text(
            "1. Keep [#407](https://github.com/o/r/pull/407)'s *style* or `snake_case`?",
        )
        .unwrap();
        assert_eq!(
            links["markdown"],
            "1\\. Keep [#407](<https://github.com/o/r/pull/407>)'s \\*style\\* or \\`snake\\_case\\`?"
        );
        assert_eq!(
            links["references"][0]["href"],
            "https://github.com/o/r/pull/407"
        );
        assert_eq!(links["references"][0]["sourceLabel"], "#407");
        assert_eq!(links["references"][0]["kind"], "url");
    }

    #[test]
    fn a_bare_issue_number_reaches_the_renderer_as_text() {
        let links = linked_text("#407 (Arc-style layout): reply?").unwrap();
        assert_eq!(links["markdown"], "#407 (Arc-style layout): reply?");
        assert_eq!(links["references"], json!([]));
        assert_eq!(linked_text("Fix a#407"), None);
    }

    #[test]
    fn a_line_start_cannot_become_a_heading_quote_or_list() {
        let links =
            linked_text("# not a heading\n> nor a quote\n- nor a list https://a.dev").unwrap();
        assert_eq!(
            links["markdown"],
            "\\# not a heading  \n\\> nor a quote  \n\\- nor a list [https://a.dev](<https://a.dev>)"
        );
    }

    #[test]
    fn a_label_shows_the_link_words() {
        assert_eq!(
            shown_label("Reply on [#407](https://github.com/o/r/pull/407)").as_deref(),
            Some("Reply on #407")
        );
        assert_eq!(shown_label("Tabs"), None);
    }

    #[test]
    fn a_form_gets_its_links() {
        let mut form = json!({"questions": [{
            "title": "About [#407](https://github.com/o/r/pull/407)?",
            "detail": "Plain detail.",
            "choices": [
                {"label": "Read [the PR](https://github.com/o/r/pull/407)", "detail": "See https://a.dev"},
                {"label": "Skip", "detail": ""}
            ]
        }]});
        add_links(&mut form);
        let question = &form["questions"][0];
        assert_eq!(
            question["titleLinks"]["markdown"],
            "About [#407](<https://github.com/o/r/pull/407>)?"
        );
        assert!(question.get("detailLinks").is_none());
        assert_eq!(question["choices"][0]["shownLabel"], "Read the PR");
        assert_eq!(
            question["choices"][0]["detailLinks"]["markdown"],
            "See [https://a.dev](<https://a.dev>)"
        );
        assert!(question["choices"][1].get("shownLabel").is_none());
    }
}
