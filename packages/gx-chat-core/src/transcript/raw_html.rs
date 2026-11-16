//! Raw HTML in chat Markdown shows as the text somebody wrote.
//!
//! CDXC:SessionChat 2026-09-27 WHY:
//! The GPUI transcript's Markdown view renders HTML it knows and drops every tag it does not, so
//! "reply with COLOR=<my answer>" lost `<my answer>` and "returns Option<String>" lost `<String>`.
//! A transcript is untrusted text (agents quote web pages, repositories and tool output), and the
//! React chat's contract was that raw HTML is never markup (`session-chat-details.ts` (deleted 2026-10-01)). Every HTML
//! node is escaped to literal text unless it is only formatting tags the view draws safely; links
//! and images stay text so a transcript can never load a remote picture or smuggle a link.
//! SEE-ALSO: apps/desktop/src/app/native_chat/rich_markdown.rs applies this to card text too.

use markdown::mdast::Node;
use markdown::ParseOptions;

/// Formatting tags drawn as formatting. Unsupported ones in this list (`kbd`, `sub`, `sup`) lose
/// only the tag, never the words between an open and a close tag.
const FORMATTING_TAGS: &[&str] = &[
    "b", "br", "code", "del", "details", "em", "i", "kbd", "mark", "s", "strong", "sub", "summary",
    "sup", "u",
];

/// The options the GPUI Markdown view parses with (gpui-component `MarkdownParseContext`), so
/// the HTML found here is exactly the HTML it would render.
fn parse_options() -> ParseOptions {
    let mut options = ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    options
}

/// True when every `<` in `html` opens or closes a formatting tag.
fn only_formatting_tags(html: &str) -> bool {
    let mut seen = false;
    for (at, _) in html.match_indices('<') {
        let after = &html[at + 1..];
        let body = after.strip_prefix('/').unwrap_or(after);
        let name_len = body
            .find(|ch: char| !ch.is_ascii_alphanumeric())
            .unwrap_or(body.len());
        let name = body[..name_len].to_ascii_lowercase();
        let ends_name = matches!(
            body[name_len..].chars().next(),
            Some('>' | '/' | ' ' | '\t' | '\n' | '\r')
        );
        if name.is_empty() || !ends_name || !FORMATTING_TAGS.contains(&name.as_str()) {
            return false;
        }
        seen = true;
    }
    seen
}

fn collect(node: &Node, spans: &mut Vec<(usize, usize)>) {
    if let Node::Html(html) = node {
        if !only_formatting_tags(&html.value) {
            if let Some(position) = &html.position {
                spans.push((position.start.offset, position.end.offset));
            }
        }
        return;
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect(child, spans);
    }
}

/// One HTML node's source rewritten so it parses as the same visible text.
///
/// The `<` stays raw and the character after it becomes a character reference, which is what
/// stops it opening a tag. `&lt;` looks equivalent but is not: a GFM autolink literal runs until
/// whitespace or a raw `<`, so `https://example.com&lt;/a>` linked (and printed) the whole run.
/// Addresses inside the tag lose their autolink start for the same reason, so `href="https://…"`
/// cannot run on into the next escaped tag and a tag's attribute never becomes a live link.
///
/// CDXC:SessionChat 2026-11-16 WHY:
/// A line that starts with a tag (`<ghostex-system>`) opens an HTML block that runs to the next
/// blank line, so a code span in that paragraph was escaped too, and a code span shows character
/// references as typed: "`item ask <W7> --done <Q12>`" read "item ask <&#87;7> --done <&#81;12>".
/// Once the tag is text the paragraph parses as Markdown again, where nothing in a code span can
/// open a tag, so code spans are copied as they are.
fn as_literal_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 16);
    let mut chars = html.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if ch == '`' {
            let run = html[at..].chars().take_while(|c| *c == '`').count();
            let span_end = code_span_end(html, at, run);
            let end = span_end.unwrap_or(at + run);
            out.push_str(&html[at..end]);
            while chars.peek().is_some_and(|(next, _)| *next < end) {
                chars.next();
            }
            continue;
        }
        match ch {
            '<' => {
                out.push('<');
                if let Some(&(_, next)) = chars.peek() {
                    if next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?') {
                        out.push_str(&format!("&#{};", next as u32));
                        chars.next();
                    }
                }
            }
            ':' if html[at + 1..].starts_with("//") => out.push_str("&#58;"),
            '.' if html[..at].to_ascii_lowercase().ends_with("www") => out.push_str("&#46;"),
            '@' => out.push_str("&#64;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Where the code span opened by the `run` backticks at `at` ends (after its closing run of as many
/// backticks, on the same paragraph), `None` when nothing closes it.
fn code_span_end(html: &str, at: usize, run: usize) -> Option<usize> {
    let mut from = at + run;
    loop {
        let rest = &html[from..];
        let found = rest.find('`')?;
        if rest[..found].contains("\n\n") {
            return None;
        }
        let start = from + found;
        let length = html[start..].chars().take_while(|c| *c == '`').count();
        if length == run {
            return Some(start + length);
        }
        from = start + length;
    }
}

/// `markdown` with every raw HTML node that is not plain formatting turned into literal text.
/// Code spans and fences are untouched because their `<` never parses as HTML. Idempotent.
pub fn escape_raw_html(markdown: &str) -> String {
    if !markdown.contains('<') {
        return markdown.to_string();
    }
    let Ok(tree) = markdown::to_mdast(markdown, &parse_options()) else {
        return markdown.to_string();
    };
    let mut spans = Vec::new();
    collect(&tree, &mut spans);
    if spans.is_empty() {
        return markdown.to_string();
    }
    let mut result = String::with_capacity(markdown.len() + spans.len() * 8);
    let mut cursor = 0;
    for (start, end) in spans {
        if start < cursor || end > markdown.len() {
            continue;
        }
        result.push_str(&markdown[cursor..start]);
        result.push_str(&as_literal_text(&markdown[start..end]));
        cursor = end;
    }
    result.push_str(&markdown[cursor..]);
    result
}
