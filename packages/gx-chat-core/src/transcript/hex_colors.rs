//! Hex colours in a chat's Markdown (`#abc`, `#fa0c`, `#ff8800`, `#ff8800cc`): the one rule for
//! which `#` tokens are colours, so every chat draws a swatch beside the same ones.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User, on the phone parity page: "Build it with the next app release" for "Colour swatches next
//! to hex colours". The rule the desktop drew with since 2026-10-09 (`is_hex_color_digits`,
//! `hex_colors` and `swatch_color` in gpui-component's `text/inline_code.rs`, CDXC:SessionChat
//! 2026-10-09 there) lives here now, and the core marks what it finds in the Markdown it projects
//! ([`mark_hex_colors`], the way `issue_links.rs` marks `#123`):
//!
//!   * a colour in prose becomes `[#ff8800](ghostex-color:ff8800)`, its words unchanged;
//!   * an inline code span that names a colour and nothing else becomes
//!     ``[`#ff8800`](ghostex-color:ff8800)``, so the chip keeps its code look with the swatch in it;
//!   * a colour inside a fenced block, a link, a picture or raw HTML, or among other words in a code
//!     span, stays as it was (the desktop draws no swatch there).
//!
//! The phone draws the swatch from the mark. The desktop's text view finds colours itself
//! (gpui-component cannot take them from the core), so it turns the marks back into the text they
//! were ([`unmark_hex_colors`]) and draws exactly as before; its copy of the rule in gpui-component
//! must change together with this one.
//! SEE-ALSO: apps/desktop/src/app/native_chat/rich_markdown.rs (`issue_linked`, the desktop),
//! apps/mobile/app/src/chat/native/transcript/markdown/Markdown.tsx (the phone),
//! issue_links.rs (`#123`, which never takes a colour: a colour has six or eight digits or a letter).

use std::borrow::Cow;
use std::ops::Range;

use markdown::mdast::Node;
use markdown::ParseOptions;

/// The destination [`mark_hex_colors`] writes, the colour's hex digits appended.
pub const COLOR_LINK_SCHEME: &str = "ghostex-color:";

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `hex` (the digits after a `#`) is a colour: a CSS hex length (3, 4, 6 or 8) of hex
/// digits, where the short lengths need a letter a-f so a bare number (`#123`, `#1234`) stays a
/// number. A token with a non-hex letter (`#todo`) is neither.
pub fn is_hex_color_digits(hex: &str) -> bool {
    hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        && match hex.len() {
            6 | 8 => true,
            3 | 4 => hex.bytes().any(|byte| byte.is_ascii_alphabetic()),
            _ => false,
        }
}

/// The colour a whole span names (`#ff8800`, trimmed), as its hex digits.
pub fn color_of_span(text: &str) -> Option<&str> {
    let hex = text.trim().strip_prefix('#')?;
    is_hex_color_digits(hex).then_some(hex)
}

/// `[r, g, b, a]` of hex digits [`is_hex_color_digits`] accepts, each 0-255: `#rgb` and `#rgba`
/// repeat each digit, the others carry two per channel, and no alpha digits is opaque.
pub fn rgba(hex: &str) -> Option<[u8; 4]> {
    if !is_hex_color_digits(hex) {
        return None;
    }
    let short = hex.len() <= 4;
    let channel = |index: usize| -> Option<u8> {
        if short {
            u8::from_str_radix(hex.get(index..index + 1)?, 16)
                .ok()
                .map(|nibble| nibble * 17)
        } else {
            u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()
        }
    };
    let alpha = if matches!(hex.len(), 4 | 8) {
        channel(3)?
    } else {
        255
    };
    Some([channel(0)?, channel(1)?, channel(2)?, alpha])
}

/// A hex colour in `text` at byte `at`: the end of its digits when it is one. A whole token: a `#`
/// that does not follow a word character, `/` or `#`, then the digits of a colour with no word
/// character, `/` or `-` after them.
pub fn hex_color_at(text: &str, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'#') {
        return None;
    }
    if at > 0 && (word(bytes[at - 1]) || matches!(bytes[at - 1], b'/' | b'#')) {
        return None;
    }
    let end = at
        + 1
        + bytes[at + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_hexdigit())
            .count();
    let closes = end >= bytes.len() || !(word(bytes[end]) || matches!(bytes[end], b'/' | b'-'));
    (closes && is_hex_color_digits(&text[at + 1..end])).then_some(end)
}

/// Every hex colour written in a run of text.
pub fn hex_colors(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find('#') {
        let start = at + offset;
        match hex_color_at(text, start) {
            Some(end) => {
                found.push(start..end);
                at = end;
            }
            None => at = start + 1,
        }
    }
    found
}

/// Whether `markdown` has a `#` before a hex digit anywhere, the only text a colour can be.
fn has_color_evidence(markdown: &str) -> bool {
    markdown
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'#' && pair[1].is_ascii_hexdigit())
}

/// One source edit: the bytes it replaces and what replaces them.
type Edit = (Range<usize>, String);

fn mark(source: &str, hex: &str) -> String {
    format!("[{source}]({COLOR_LINK_SCHEME}{hex})")
}

/// The marks for every colour in prose and every code span that names one. Code blocks, links,
/// pictures, raw HTML and definitions are never looked into.
fn color_edits(markdown: &str, tree: &Node, edits: &mut Vec<Edit>) {
    match tree {
        Node::Text(_) => {
            let Some(position) = tree.position() else {
                return;
            };
            let (start, end) = (position.start.offset, position.end.offset);
            let Some(text) = markdown.get(start..end) else {
                return;
            };
            let mut at = 0;
            while let Some(offset) = text[at..].find('#') {
                let hash = start + at + offset;
                match hex_color_at(markdown, hash).filter(|&stop| stop <= end) {
                    Some(stop) => {
                        // `\#fff` is the same `#fff` with its `#` escaped: the mark keeps both.
                        let from = if hash > start && markdown.as_bytes()[hash - 1] == b'\\' {
                            hash - 1
                        } else {
                            hash
                        };
                        edits.push((
                            from..stop,
                            mark(&markdown[from..stop], &markdown[hash + 1..stop]),
                        ));
                        at = stop - start;
                    }
                    None => at = hash - start + 1,
                }
            }
        }
        Node::InlineCode(code) => {
            let (Some(position), Some(hex)) = (tree.position(), color_of_span(&code.value)) else {
                return;
            };
            let range = position.start.offset..position.end.offset;
            if let Some(source) = markdown.get(range.clone()) {
                edits.push((range.clone(), mark(source, hex)));
            }
        }
        Node::Link(_)
        | Node::Code(_)
        | Node::Html(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::LinkReference(_)
        | Node::Definition(_)
        | Node::InlineMath(_)
        | Node::Math(_) => {}
        _ => {
            for child in tree.children().into_iter().flatten() {
                color_edits(markdown, child, edits);
            }
        }
    }
}

/// The marks [`mark_hex_colors`] wrote, each back to the text it wrapped.
fn unmark_edits(markdown: &str, tree: &Node, edits: &mut Vec<Edit>) {
    match tree {
        Node::Link(link) if link.url.starts_with(COLOR_LINK_SCHEME) => {
            // The mark is `[<the text>](ghostex-color:<hex>)` as `mark` wrote it.
            let Some(position) = tree.position() else {
                return;
            };
            let range = position.start.offset..position.end.offset;
            let source = markdown
                .get(range.clone())
                .and_then(|link_source| link_source.strip_prefix('['))
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|rest| rest.strip_suffix(&link.url[..]))
                .and_then(|rest| rest.strip_suffix("]("));
            if let Some(source) = source {
                edits.push((range, source.to_string()));
            }
        }
        Node::Code(_) | Node::Html(_) | Node::Definition(_) => {}
        _ => {
            for child in tree.children().into_iter().flatten() {
                unmark_edits(markdown, child, edits);
            }
        }
    }
}

fn apply(markdown: &str, mut edits: Vec<Edit>) -> Cow<'_, str> {
    if edits.is_empty() {
        return Cow::Borrowed(markdown);
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::with_capacity(markdown.len() + edits.len() * 28);
    let mut copied = 0;
    for (range, replacement) in edits {
        if range.start < copied {
            continue;
        }
        out.push_str(&markdown[copied..range.start]);
        out.push_str(&replacement);
        copied = range.end;
    }
    out.push_str(&markdown[copied..]);
    Cow::Owned(out)
}

/// Writes every hex colour in `markdown`'s prose, and every code span that names one, as a
/// `ghostex-color:` link the renderers draw a swatch for. Markdown with no `#` before a hex digit
/// comes back as it was, unparsed.
pub fn mark_hex_colors(markdown: &str) -> Cow<'_, str> {
    if !has_color_evidence(markdown) {
        return Cow::Borrowed(markdown);
    }
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::default()) else {
        return Cow::Borrowed(markdown);
    };
    let mut edits = Vec::new();
    color_edits(markdown, &tree, &mut edits);
    apply(markdown, edits)
}

/// `markdown` with every [`mark_hex_colors`] mark back to the text it wrapped, for a renderer that
/// finds colours itself (the desktop's text view).
pub fn unmark_hex_colors(markdown: &str) -> Cow<'_, str> {
    if !markdown.contains(COLOR_LINK_SCHEME) {
        return Cow::Borrowed(markdown);
    }
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::default()) else {
        return Cow::Borrowed(markdown);
    };
    let mut edits = Vec::new();
    unmark_edits(markdown, &tree, &mut edits);
    apply(markdown, edits)
}
