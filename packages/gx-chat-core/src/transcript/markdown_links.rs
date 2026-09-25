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

use crate::composer::reference_pills::reference_kind;
use crate::transcript::file_position::file_position_suffix;
use crate::transcript::jsstr::{is_js_space, js_trim};
use crate::transcript::links::{classify_link_href, file_position_from_href, LinkTarget};

/// One Markdown reference as the renderer looks it up.
pub fn markdown_link(href: &str, label: &str) -> Option<Value> {
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

/// The same, restricted to the file references a pill is drawn from.
pub fn markdown_reference(href: &str, label: &str) -> Option<Value> {
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
    let mut entry = Map::new();
    entry.insert("href".to_string(), href.into());
    entry.insert("sourceLabel".to_string(), label.into());
    entry.insert(
        "label".to_string(),
        if !suffix.is_empty() && !source_label.ends_with(&suffix) {
            format!("{source_label}{suffix}")
        } else {
            source_label.to_string()
        }
        .into(),
    );
    entry.insert("title".to_string(), format!("{path}{suffix}").into());
    entry.insert(
        "kind".to_string(),
        reference_kind(source_label, &path).as_str().into(),
    );
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

/// `(^|[\s*_~(])(https?:\/\/[^\s<]+)` case-insensitively, every match.
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
            Some(character) => is_js_space(character) || "*_~(".contains(character),
        };
        if !opens {
            cursor = start + 1;
            continue;
        }
        let run: usize = markdown[start..]
            .chars()
            .take_while(|character| !is_js_space(*character) && *character != '<')
            .map(char::len_utf8)
            .sum();
        found.push(&markdown[start..start + run]);
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
