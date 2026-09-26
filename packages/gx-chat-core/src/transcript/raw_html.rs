//! Raw HTML in chat Markdown shows as the text somebody wrote.
//!
//! CDXC:SessionChat 2026-09-27 WHY:
//! The GPUI transcript's Markdown view renders HTML it knows and drops every tag it does not, so
//! "reply with COLOR=<my answer>" lost `<my answer>` and "returns Option<String>" lost `<String>`.
//! A transcript is untrusted text (agents quote web pages, repositories and tool output), and the
//! React chat's contract was that raw HTML is never markup (`session-chat-details.ts`). Every HTML
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
        result.push_str(&markdown[start..end].replace('<', "&lt;"));
        cursor = end;
    }
    result.push_str(&markdown[cursor..]);
    result
}
