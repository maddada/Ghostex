//! Which single newlines in an agent's reply stay line breaks.
//!
//! Markdown joins the lines of a paragraph, so the phone's renderer showed a question and the
//! lettered options under it as one run of text. The decision is made here and travels inside the
//! Markdown as a backslash hard break, which every renderer already draws as a new line (the GPUI
//! `TextView` and the phone's parser in `apps/mobile/app/src/chat/native/transcript/markdown/`).
//! The GPUI `TextView` also draws a soft newline as a line break on its own, so today this changes
//! only what the phone shows.
//!
//! CDXC:SessionChat 2026-09-26 DECISION:
//! User: a line that starts like a lettered option (`A. `, `B) `, `A1. `) stays on its own line in every agent's reply. Pi, OMP, Hermes, Antigravity and ZCode replies keep every single newline, because their terminals paint one row per line (2026-09-14/17, ported from the deleted React chat); Claude, Codex, Grok Build and Cursor otherwise follow Markdown. Supersedes the React-only rule deleted with the React chat on 2026-09-25.

use markdown::mdast::Node;
use markdown::ParseOptions;

/// How an agent's reply treats a newline inside a paragraph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentLineBreaks {
    /// Markdown's joining, except before a lettered option line.
    #[default]
    Markdown,
    /// Every newline is a line break.
    Every,
}

impl AgentLineBreaks {
    /// The rule for a session, from its transcript family or, before a read names one, its
    /// launch agent.
    pub fn for_agent(agent: Option<&str>, session_agent: Option<&str>) -> Self {
        match crate::extras::agents::transcript_agent([agent, session_agent]) {
            Some("pi" | "hermes" | "antigravity" | "zcode") => Self::Every,
            _ => Self::Markdown,
        }
    }
}

/// The reply with the chosen soft line breaks turned into hard breaks. Code, tables, headings and
/// HTML are untouched: only the text of paragraphs is looked at.
pub fn agent_line_breaks(markdown: &str, mode: AgentLineBreaks) -> String {
    if !markdown.contains('\n') {
        return markdown.to_string();
    }
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::gfm()) else {
        return markdown.to_string();
    };
    let mut offsets = Vec::new();
    collect(&tree, false, markdown, mode, &mut offsets);
    if offsets.is_empty() {
        return markdown.to_string();
    }
    offsets.sort_unstable();
    offsets.dedup();
    let mut out = String::with_capacity(markdown.len() + offsets.len());
    let mut cursor = 0;
    for offset in offsets {
        out.push_str(&markdown[cursor..offset]);
        out.push('\\');
        cursor = offset;
    }
    out.push_str(&markdown[cursor..]);
    out
}

fn collect(
    node: &Node,
    in_paragraph: bool,
    source: &str,
    mode: AgentLineBreaks,
    offsets: &mut Vec<usize>,
) {
    if in_paragraph {
        if let Node::Text(text) = node {
            if let Some(position) = &text.position {
                soft_breaks(
                    source,
                    position.start.offset,
                    position.end.offset,
                    mode,
                    offsets,
                );
            }
            return;
        }
    }
    let in_paragraph = in_paragraph || matches!(node, Node::Paragraph(_));
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect(child, in_paragraph, source, mode, offsets);
    }
}

/// The insertion points, one per chosen newline in a text span: right before its line ending.
fn soft_breaks(
    source: &str,
    start: usize,
    end: usize,
    mode: AgentLineBreaks,
    offsets: &mut Vec<usize>,
) {
    let Some(span) = source.get(start..end) else {
        return;
    };
    for (index, _) in span.match_indices('\n') {
        let newline = start + index;
        let line_end = if source[..newline].ends_with('\r') {
            newline - 1
        } else {
            newline
        };
        let line = &source[..line_end];
        // Already a hard break: two trailing spaces, or an odd run of backslashes.
        let backslashes = line.len() - line.trim_end_matches('\\').len();
        if line.ends_with("  ") || backslashes % 2 == 1 {
            continue;
        }
        let next_line = source[newline + 1..].split('\n').next().unwrap_or_default();
        if mode == AgentLineBreaks::Every || starts_with_option(next_line) {
            offsets.push(line_end);
        }
    }
}

/// `A. `, `B) `, `A1. ` after indentation or a quote prefix.
fn starts_with_option(line: &str) -> bool {
    let text = line.trim_start_matches([' ', '\t', '>']);
    let mut characters = text.chars();
    if !characters
        .next()
        .is_some_and(|letter| letter.is_ascii_uppercase())
    {
        return false;
    }
    let rest = characters
        .as_str()
        .trim_start_matches(|digit: char| digit.is_ascii_digit());
    rest.strip_prefix(['.', ')'])
        .is_some_and(|after| after.starts_with([' ', '\t']))
}
