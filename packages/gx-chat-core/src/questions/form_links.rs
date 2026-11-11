//! The links in a question's words: a question's title and detail, a choice's detail, and the
//! question above each answer on the "You answered" card, as runs of words the renderers draw with
//! links that open on a click. Text with nothing to link stays plain text.
//!
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User, on a form asking about "#407 (Arc-style layout)": "i'm annoyed u asked me here about #407 but i can't click on the link to open that pr. please fix for customers and for all future ones". A question's title and detail, a choice's detail and the question on the "You answered" card show Markdown links (`[label](https://…)` or a file path) and bare http(s) addresses as their words only, opening on a click the way a message's links do, on the desktop and the phone. Then, of the answered card showing raw `([Discord](https://…))`: "should show as links but not blue color, just should have dotted lines under links basically like we did for threads before". So every question link keeps the text's own colour with the faint dotted underline a session name has on a "Message to" card (desktop `session_name_underline` in inter_agent_message.rs), the same on question pages (server/src/visual_pages/kit.rs). Nothing but the links is read, so `*`, `_` or a leading `1.` show as written. A choice's label stays plain (the whole row is its button) and shows a link's words without the markup, as do the "?" list's and the Resources modal's titles (`plain_words`). A bare `#407` is an `issue` run: the desktop links it to the project's GitHub issues when the project has a GitHub `origin` (the chat's own `issue_links` rule); the phone has no such helper and keeps it text. The answer key the agent receives stays the question's title as written, links included, so forms answered before this still match their answers. Agents are told to link what they ask about (skills/ghostex-visuals/SKILL.md, the coordinator playbook).
//!
//! CDXC:SessionChat 2026-11-04 SEE-ALSO:
//! apps/desktop/src/app/native_chat/link_words.rs and apps/mobile/app/src/chat/native/transcript/LinkWords.tsx draw the runs; question_form.rs / QuestionForm.tsx and page_answers.rs / PageAnswersCard.tsx use them.

use serde_json::{json, Value};

use crate::transcript::links::{classify_link_href, LinkTarget};

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

/// A bare `#407` in `text` at `at`: the digits' end when it is one, by the chat's one rule
/// (`transcript/issue_links.rs`).
fn issue_at(text: &str, at: usize) -> Option<usize> {
    crate::transcript::issue_links::issue_reference_at(text, at)
}

fn word_run(text: &str) -> Value {
    json!({ "text": text })
}

/// The words with their links as runs, `{text}`, `{text, href}` or `{text, issue}` (a bare
/// `#407`'s number), or `None` when there is nothing to link and they stay plain text.
pub fn word_runs(text: &str) -> Option<Value> {
    let mut runs = Vec::new();
    let mut linked = false;
    for run in link_runs(text) {
        if let Some(href) = run.href {
            linked = true;
            runs.push(json!({ "text": run.text, "href": href }));
            continue;
        }
        let words = run.text.as_str();
        let mut copied = 0;
        let mut at = 0;
        while let Some(found) = words[at..].find('#') {
            let start = at + found;
            match issue_at(words, start) {
                Some(end) => {
                    if copied < start {
                        runs.push(word_run(&words[copied..start]));
                    }
                    runs.push(
                        json!({ "text": &words[start..end], "issue": &words[start + 1..end] }),
                    );
                    linked = true;
                    copied = end;
                    at = end;
                }
                None => at = start + 1,
            }
        }
        if copied < words.len() {
            runs.push(word_run(&words[copied..]));
        }
    }
    linked.then(|| Value::Array(runs))
}

/// The words a reader sees: each link's words without the Markdown around it.
pub fn plain_words(text: &str) -> String {
    link_runs(text).into_iter().map(|run| run.text).collect()
}

/// A choice label as its row shows it, when it holds a link.
pub fn shown_label(label: &str) -> Option<String> {
    let runs = link_runs(label);
    runs.iter()
        .any(|run| run.href.is_some())
        .then(|| runs.into_iter().map(|run| run.text).collect())
}

/// Adds the runs to one form as the document carries it (`questions/forms.rs`, `project`):
/// `titleRuns` and `detailRuns` on a question, `detailRuns` and `shownLabel` on a choice, each
/// only when there is something to link.
pub fn add_links(form: &mut Value) {
    let Some(questions) = form.get_mut("questions").and_then(Value::as_array_mut) else {
        return;
    };
    for question in questions {
        let Some(question) = question.as_object_mut() else {
            continue;
        };
        for (field, key) in [("title", "titleRuns"), ("detail", "detailRuns")] {
            if let Some(runs) = question
                .get(field)
                .and_then(Value::as_str)
                .and_then(word_runs)
            {
                question.insert(key.to_string(), runs);
            }
        }
        let Some(choices) = question.get_mut("choices").and_then(Value::as_array_mut) else {
            continue;
        };
        for choice in choices {
            let Some(choice) = choice.as_object_mut() else {
                continue;
            };
            if let Some(runs) = choice
                .get("detail")
                .and_then(Value::as_str)
                .and_then(word_runs)
            {
                choice.insert("detailRuns".to_string(), runs);
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
        assert_eq!(word_runs("Which layout? Use *tabs* or 1. pages"), None);
        assert_eq!(
            word_runs("Ship it, colour #fff or #ff0000 or #a1b2c3?"),
            None
        );
        assert_eq!(word_runs("a word http:// alone, a#407, #0, #123456"), None);
    }

    #[test]
    fn the_runs_keep_every_other_character() {
        assert_eq!(
            word_runs("1. Keep ([Discord](https://discord.com/channels/1/2)): *style*?").unwrap(),
            json!([
                {"text": "1. Keep ("},
                {"text": "Discord", "href": "https://discord.com/channels/1/2"},
                {"text": "): *style*?"},
            ])
        );
    }

    #[test]
    fn a_bare_issue_number_is_an_issue_run() {
        assert_eq!(
            word_runs("#407 (Arc-style layout), see #12.").unwrap(),
            json!([
                {"text": "#407", "issue": "407"},
                {"text": " (Arc-style layout), see "},
                {"text": "#12", "issue": "12"},
                {"text": "."},
            ])
        );
    }

    #[test]
    fn plain_words_drop_the_markup() {
        assert_eq!(
            plain_words("randomsand's asks ([Discord](https://discord.com/c/1)): what now?"),
            "randomsand's asks (Discord): what now?"
        );
        assert_eq!(plain_words("No links, #407"), "No links, #407");
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
    fn a_form_gets_its_runs() {
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
            question["titleRuns"],
            json!([
                {"text": "About "},
                {"text": "#407", "href": "https://github.com/o/r/pull/407"},
                {"text": "?"},
            ])
        );
        assert!(question.get("detailRuns").is_none());
        assert_eq!(question["choices"][0]["shownLabel"], "Read the PR");
        assert_eq!(
            question["choices"][0]["detailRuns"],
            json!([{"text": "See "}, {"text": "https://a.dev", "href": "https://a.dev"}])
        );
        assert!(question["choices"][1].get("shownLabel").is_none());
    }
}
