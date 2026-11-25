//! The reference links a message's Markdown carries, parsed once when the message changes.
//!
//! Ported from `packages/shared/session-chat-presentation/markdown-links.ts`. The TypeScript parsed
//! with `mdast-util-from-markdown` and no extensions, which is plain CommonMark; the Rust side uses
//! `markdown::to_mdast` with `ParseOptions::default()`, the same author's port of the same parser
//! with the same default. GFM is deliberately off on both sides, which is why the autolink scan
//! below exists at all.

use markdown::mdast::Node;
use markdown::ParseOptions;
use serde_json::{Map, Value};

use crate::composer::reference_pills::{reference_kind, reference_path_label};
use crate::transcript::file_position::file_position_suffix;
use crate::transcript::jsstr::{is_js_space, js_trim};
use crate::transcript::links::{classify_link_href, file_position_from_href, LinkTarget};

/// One Markdown reference as the renderer looks it up.
pub fn markdown_link(href: &str, label: &str) -> Option<Value> {
    if let Some(reference) = question_reference(href, label) {
        return Some(reference);
    }
    if let Some(reference) = crate::transcript::session_mentions::session_reference(href, label) {
        return Some(reference);
    }
    if let Some(reference) = crate::extras::coordinator_board::board_reference(href, label) {
        return Some(reference);
    }
    // One of the user's earlier messages (`message_numbers.rs`): its ID in a pill.
    if let Some(reference) = crate::transcript::message_numbers::message_reference(href, label) {
        return Some(reference);
    }
    // A question, Live page or (above, `board:`) work item pointed at (`item_references.rs`).
    if let Some(reference) = crate::transcript::item_references::question_reference(href, label) {
        return Some(reference);
    }
    if let Some(reference) = crate::transcript::item_references::page_reference(href, label) {
        return Some(reference);
    }
    // A Boost the message carried (`extras/boosts.rs`): its name in a pill, what it asked of the
    // agent in its hover.
    if crate::extras::boosts::boost_id_of_href(href).is_some() {
        let boost = crate::extras::boosts::boost_of_href(href);
        let mut entry = Map::new();
        entry.insert("href".to_string(), href.into());
        entry.insert("sourceLabel".to_string(), label.into());
        entry.insert(
            "label".to_string(),
            boost.map_or(label, |boost| boost.label).into(),
        );
        entry.insert(
            "title".to_string(),
            match boost {
                Some(boost) => format!("Boost: {}", boost.description),
                None => "Your Boost".to_string(),
            }
            .into(),
        );
        entry.insert("kind".to_string(), "boost".into());
        return Some(Value::Object(entry));
    }
    // What a Reply in chat box answered: its title in a pill, its tags only in the link
    // (`questions/answer_pills.rs`). The renderers add how its questions stand from the
    // document's `questionStates` (`answer_pills::tooltip`).
    if crate::questions::answer_pills::parse_href(href).is_some() {
        let mut entry = Map::new();
        entry.insert("href".to_string(), href.into());
        entry.insert("sourceLabel".to_string(), label.into());
        entry.insert(
            "label".to_string(),
            crate::transcript::session_mentions::pill_label(label).into(),
        );
        entry.insert(
            "title".to_string(),
            crate::questions::answer_pills::tooltip(label, href, &Value::Null).into(),
        );
        entry.insert("kind".to_string(), "answer".into());
        return Some(Value::Object(entry));
    }
    if let LinkTarget::Url(url) = classify_link_href(href) {
        let mut entry = Map::new();
        entry.insert("href".to_string(), href.into());
        entry.insert("sourceLabel".to_string(), label.into());
        entry.insert(
            "label".to_string(),
            if label.is_empty() {
                url.as_str()
            } else {
                label
            }
            .into(),
        );
        entry.insert("title".to_string(), url.into());
        entry.insert("kind".to_string(), "url".into());
        return Some(Value::Object(entry));
    }
    markdown_reference(href, label)
}

/// A Q-tag link (`#gx-question=Q80;open`, `open_questions.rs`) as a `question` reference whose
/// `state` the renderers draw its pill from.
fn question_reference(href: &str, label: &str) -> Option<Value> {
    use crate::transcript::open_questions::{TagState, QUESTION_LINK_PREFIX};
    let (tag, rest) = href.strip_prefix(QUESTION_LINK_PREFIX)?.split_once(';')?;
    let mut flags = rest.split(';');
    let state = TagState::parse(flags.next()?)?;
    let here = flags.any(|flag| flag == "here");
    let title = match state {
        TagState::Open => format!("{tag}: waiting for your answer"),
        TagState::Answered => format!("{tag}: answered"),
        TagState::Withdrawn => format!("{tag}: withdrawn, no answer needed"),
        TagState::Skipped => format!("{tag}: skipped"),
    };
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    entry.insert("label".to_string(), label.into());
    entry.insert("title".to_string(), title.into());
    entry.insert("kind".to_string(), "question".into());
    entry.insert("state".to_string(), state.as_str().into());
    // On its own question: a click answers it there (the Reply popover) instead of jumping.
    if here {
        entry.insert("here".to_string(), true.into());
        entry.insert("tag".to_string(), tag.into());
    }
    Some(Value::Object(entry))
}

/// The same, restricted to the file references a pill is drawn from.
pub fn markdown_reference(href: &str, label: &str) -> Option<Value> {
    if let Some(comment) = crate::composer::review_comments::ReviewComment::from_href(href) {
        let mut entry = Map::new();
        entry.insert("href".to_string(), href.into());
        entry.insert("sourceLabel".to_string(), label.into());
        entry.insert("label".to_string(), js_trim(label).into());
        entry.insert("title".to_string(), comment.tooltip().into());
        entry.insert("kind".to_string(), "comment".into());
        entry.insert("path".to_string(), comment.file.into());
        return Some(Value::Object(entry));
    }
    let LinkTarget::File(path) = classify_link_href(href) else {
        return None;
    };
    let position = file_position_from_href(href);
    let suffix = file_position_suffix(position.as_ref());
    let trimmed = js_trim(label);
    let source_label = if trimmed.is_empty() {
        path.as_str()
    } else {
        trimmed
    };
    let kind = reference_kind(source_label, &path);
    let shown =
        reference_path_label(source_label, &path, kind).unwrap_or_else(|| source_label.to_string());
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    entry.insert(
        "label".to_string(),
        if !suffix.is_empty() && !shown.ends_with(&suffix) {
            format!("{shown}{suffix}")
        } else {
            shown
        }
        .into(),
    );
    entry.insert("title".to_string(), format!("{path}{suffix}").into());
    entry.insert("kind".to_string(), kind.as_str().into());
    entry.insert("path".to_string(), path.into());
    // `position` is `undefined` when there is none, which `JSON.stringify` drops.
    if let Some(position) = position {
        entry.insert(
            "position".to_string(),
            serde_json::to_value(position).unwrap_or(Value::Null),
        );
    }
    Some(Value::Object(entry))
}

/// `[?!.,:*_~]+$` removed.
fn strip_autolink_trailing(url: &str) -> &str {
    url.trim_end_matches(|character| "?!.,:*_~".contains(character))
}

fn autolink_literal(candidate: &str) -> &str {
    let mut url = strip_autolink_trailing(candidate);
    while url.ends_with(')') && url.matches(')').count() > url.matches('(').count() {
        url = strip_autolink_trailing(&url[..url.len() - 1]);
    }
    url
}

/// Punctuation that ends a bare address, and may stand right before one: CJK and full-width
/// punctuation, curly quotes and the ellipsis. The native renderer ends its links at the same
/// characters (`ends_bare_link` in gpui-component's text/format/markdown.rs), so
/// `页（http://a/?x=1），见` links `http://a/?x=1` in both (#762). The letters among the CJK
/// symbols (々 〆 〇, the Hangzhou numerals, the kana repeat marks, 〸 〹 〺 〻 〼) carry on, so
/// `https://ja.wikipedia.org/wiki/佐々木` stays whole.
pub(crate) fn ends_bare_link(character: char) -> bool {
    matches!(
        character,
        '\u{2018}'..='\u{201F}'
            | '\u{2026}'
            | '\u{3000}'..='\u{3004}'
            | '\u{3008}'..='\u{3020}'
            | '\u{302A}'..='\u{3030}'
            | '\u{3036}'..='\u{3037}'
            | '\u{303D}'..='\u{303F}'
            | '\u{FE10}'..='\u{FE1F}'
            | '\u{FE30}'..='\u{FE6B}'
            | '\u{FF01}'..='\u{FF0F}'
            | '\u{FF1A}'..='\u{FF20}'
            | '\u{FF3B}'..='\u{FF40}'
            | '\u{FF5B}'..='\u{FF65}'
    )
}

/// Whether `address` written up to a dot look-alike (。 ． ｡) is still in its host: past any
/// `scheme://` and before the first `/`, `?` or `#`, with a letter, digit or other address
/// character after the dot (`after`). Ending the address there would link another domain
/// (`https://example.com。au/login` is not `https://example.com`), so such an address is not
/// linked at all. A dot that ends the sentence (whitespace, the end of the text, or a CJK
/// character after it, as in `见 https://example.com。` or `https://example.com。我们`) is
/// punctuation, and the address before it is linked (re-check of #762). The native renderer
/// applies the same rule (`ends_in_host` in gpui-component's text/format/markdown.rs).
fn ends_in_host(address: &str, end: char, after: Option<char>) -> bool {
    if !matches!(end, '\u{3002}' | '\u{FF0E}' | '\u{FF61}') || dot_ends_sentence(after) {
        return false;
    }
    let host = match address.split_once("://") {
        Some((scheme, rest))
            if !scheme.is_empty()
                && scheme.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "+.-".contains(character)
                }) =>
        {
            rest
        }
        _ => address,
    };
    !host.contains(['/', '?', '#'])
}

/// Whether what follows a dot look-alike makes it a sentence's full stop: nothing (the end of the
/// text), whitespace, or a CJK character (ideographs, kana, Hangul, CJK and full-width
/// punctuation).
fn dot_ends_sentence(after: Option<char>) -> bool {
    after.is_none_or(|character| {
        is_js_space(character)
            || ends_bare_link(character)
            || matches!(
                character,
                '\u{2E80}'..='\u{9FFF}'
                    | '\u{AC00}'..='\u{D7AF}'
                    | '\u{F900}'..='\u{FAFF}'
                    | '\u{FF00}'..='\u{FFEF}'
                    | '\u{20000}'..='\u{3FFFF}'
            )
    })
}

/// `(^|[\s*_~(])(https?:\/\/[^\s<]+)` case-insensitively, every match, where wide punctuation
/// (`ends_bare_link`) also opens and ends an address, except one it would end inside the host
/// (`ends_in_host`).
fn autolink_candidates(markdown: &str) -> Vec<&str> {
    let lower = markdown.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut cursor = 0;
    while cursor < markdown.len() {
        let Some(at) = lower[cursor..].find("http") else {
            break;
        };
        let start = cursor + at;
        let scheme = ["https://", "http://"]
            .into_iter()
            .find(|scheme| lower[start..].starts_with(scheme));
        let Some(_) = scheme else {
            cursor = start + 1;
            continue;
        };
        let before = markdown[..start].chars().next_back();
        let opens = match before {
            None => true,
            Some(character) => {
                is_js_space(character) || "*_~(".contains(character) || ends_bare_link(character)
            }
        };
        if !opens {
            cursor = start + 1;
            continue;
        }
        let run: usize = markdown[start..]
            .chars()
            .take_while(|character| {
                !is_js_space(*character) && *character != '<' && !ends_bare_link(*character)
            })
            .map(char::len_utf8)
            .sum();
        let address = &markdown[start..start + run];
        let mut rest = markdown[start + run..].chars();
        let ended_by = rest.next();
        let after = rest.next();
        if !ended_by.is_some_and(|end| ends_in_host(address, end, after)) {
            found.push(address);
        }
        cursor = start + run;
    }
    found
}

/// The URLs GFM turns into links without any markup around them.
///
/// The native renderer parses with GFM on, so it already draws these as links and already opens
/// them; this only supplies the icon, colour, and tooltip that a written-out `[label](url)` gets, so
/// a bare address and a marked-up one look the same in both renderers. An entry that does not match
/// a link the renderer found is simply never looked up.
fn autolink_references(markdown: &str) -> Vec<Value> {
    // Long tool outputs are scanned on every projection, so the cheap substring test runs before
    // the one that has to consider every whitespace character.
    if !markdown.contains("://") {
        return Vec::new();
    }
    autolink_candidates(markdown)
        .into_iter()
        .filter_map(|candidate| {
            let url = autolink_literal(candidate);
            (!url.is_empty()).then(|| markdown_link(url, url))?
        })
        .collect()
}

fn node_value(node: &Node) -> Option<&str> {
    Some(match node {
        Node::Text(value) => &value.value,
        Node::InlineCode(value) => &value.value,
        Node::InlineMath(value) => &value.value,
        Node::Code(value) => &value.value,
        Node::Math(value) => &value.value,
        Node::Html(value) => &value.value,
        Node::Yaml(value) => &value.value,
        Node::Toml(value) => &value.value,
        _ => return None,
    })
}

/// `'value' in node ? node.value : 'children' in node ? node.children.map(label).join('') : ''`.
fn node_label(node: &Node) -> String {
    if let Some(value) = node_value(node) {
        return value.to_string();
    }
    match node.children() {
        Some(children) => children.iter().map(node_label).collect(),
        None => String::new(),
    }
}

fn walk<'a>(nodes: &'a [Node], visit: &mut dyn FnMut(&'a Node)) {
    for node in nodes {
        visit(node);
        if let Some(children) = node.children() {
            walk(children, visit);
        }
    }
}

/// Whether the text can carry a link at all.
///
/// CDXC:SessionChat 2026-09-18 WHY:
/// The full mdast parse ran for every tool output that merely contained a bracket, and it was most
/// of the cost of opening a long transcript in the native runtime. A link needs an inline
/// destination `](`, a definition `]:`, or an autolink scheme `<scheme:`; text without any of them
/// cannot carry one, so it skips the parser.
fn may_hold_a_link(markdown: &str) -> bool {
    if markdown.contains("](") || markdown.contains("]:") {
        return true;
    }
    let mut cursor = 0;
    while let Some(at) = markdown[cursor..].find('<') {
        let start = cursor + at;
        cursor = start + 1;
        let rest = &markdown[cursor..];
        if !rest.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
            continue;
        }
        let end = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
            .count();
        if rest.as_bytes().get(end) == Some(&b':') {
            return true;
        }
    }
    false
}

/// Parse once when a message changes, preserving Markdown definitions and escaped destinations.
pub fn markdown_references(markdown: &str) -> Vec<Value> {
    if !may_hold_a_link(markdown) {
        return autolink_references(markdown);
    }
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::default()) else {
        return autolink_references(markdown);
    };
    let children = tree.children().map(Vec::as_slice).unwrap_or_default();
    let mut definitions: Vec<(String, String)> = Vec::new();
    walk(children, &mut |node| {
        if let Node::Definition(definition) = node {
            if !definitions
                .iter()
                .any(|(key, _)| *key == definition.identifier)
            {
                definitions.push((definition.identifier.clone(), definition.url.clone()));
            }
        }
    });
    let mut references = Vec::new();
    walk(children, &mut |node| {
        let href = match node {
            Node::Link(link) => Some(link.url.clone()),
            Node::LinkReference(reference) => definitions
                .iter()
                .find(|(key, _)| *key == reference.identifier)
                .map(|(_, url)| url.clone()),
            _ => None,
        };
        // An empty destination is falsy in the TypeScript, so it is skipped there too.
        let Some(href) = href.filter(|href| !href.is_empty()) else {
            return;
        };
        if let Some(reference) = markdown_link(&href, &node_label(node)) {
            references.push(reference);
        }
    });
    references.extend(autolink_references(markdown));
    references
}

#[cfg(test)]
mod tests {
    use super::*;

    fn autolinks(markdown: &str) -> Vec<String> {
        autolink_references(markdown)
            .iter()
            .filter_map(|reference| reference["href"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn bare_link_ends_at_wide_punctuation() {
        assert_eq!(autolinks("页（http://a/?x=1），见"), ["http://a/?x=1"]);
        assert_eq!(
            autolinks("https://example.com/a。b"),
            ["https://example.com/a"]
        );
    }

    #[test]
    fn bare_link_with_a_dot_look_alike_in_its_host_is_not_linked() {
        for markdown in [
            "https://example.com。au/login",
            "see https://example.com．au/login now",
            "https://example.com｡au",
        ] {
            assert!(autolinks(markdown).is_empty(), "{markdown}");
        }
    }

    #[test]
    fn bare_link_before_a_sentences_wide_full_stop_is_linked() {
        for markdown in [
            "见 https://example.com。",
            "见 https://example.com。 然后",
            "见 https://example.com。我们",
            "see https://example.com． next",
            "https://example.com｡\nnext line",
        ] {
            assert_eq!(autolinks(markdown), ["https://example.com"], "{markdown}");
        }
        assert!(autolinks("https://example.com。au").is_empty());
        assert!(autolinks("https://example.com。1/x").is_empty());
    }

    #[test]
    fn bare_link_keeps_kana_iteration_marks() {
        assert_eq!(
            autolinks("https://ja.wikipedia.org/wiki/佐々木"),
            ["https://ja.wikipedia.org/wiki/佐々木"]
        );
        assert_eq!(autolinks("http://a/〇〆ゝ〻"), ["http://a/〇〆ゝ〻"]);
    }

    #[test]
    fn bare_link_cut_at_wide_punctuation_is_trimmed_like_gfm() {
        assert_eq!(autolinks("(见 http://a/x)。"), ["http://a/x"]);
        assert_eq!(autolinks("见 http://a/x.，好"), ["http://a/x"]);
        assert_eq!(autolinks("见 http://a/(x)。"), ["http://a/(x)"]);
    }

    #[test]
    fn explicit_link_label_is_not_relinked() {
        let references = markdown_references("[https://trusted.example。x](https://evil.example)");
        let hrefs: Vec<&str> = references
            .iter()
            .filter_map(|reference| reference["href"].as_str())
            .collect();
        assert!(hrefs.contains(&"https://evil.example"), "{hrefs:?}");
        assert!(
            !hrefs.iter().any(|href| href.contains("trusted")),
            "{hrefs:?}"
        );
    }

    #[test]
    fn scanning_many_bare_links_is_linear() {
        let markdown = "http://a.b/x。 ".repeat(20_000);
        let started = std::time::Instant::now();
        let candidates = autolink_candidates(&markdown);
        let elapsed = started.elapsed();
        assert_eq!(candidates.len(), 20_000);
        assert!(candidates
            .iter()
            .all(|candidate| *candidate == "http://a.b/x"));
        assert!(elapsed.as_millis() < 1_000, "took {elapsed:?}");
    }
}
