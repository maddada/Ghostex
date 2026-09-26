//! What a turn's text reads as once the markup is taken off it: the reasoning headline, the user
//! turn's normalized body, and the Markdown a copy puts on the clipboard.
//!
//! Ported from `packages/shared/session-chat-presentation/message-text.ts`. Every regular
//! expression there is reproduced here as a scanner, including the backtracking the two greedy ones
//! depend on, because this crate has no regular-expression dependency.

use ghostex_gx_protocol::ChatBlock;

use crate::transcript::jsstr::{is_js_space, js_trim, js_trim_start, split_newlines};

fn is_ascii_alphanumeric(value: Option<char>) -> bool {
    value.is_some_and(|character| character.is_ascii_alphanumeric())
}

/// `markdown.replace(/```(?:[^\n]*)\n?([\s\S]*?)```/g, '$1')`.
///
/// `[^\n]*` is greedy and `[\s\S]*?` lazy, so an opener is retried with a shorter info string until
/// a closing fence is reachable. That is what makes a one-line ` ```a```b ` collapse to `b`.
fn strip_fences(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(found) = value[cursor..].find("```") {
        let open = cursor + found;
        let line_end = value[open + 3..]
            .find('\n')
            .map_or(value.len(), |at| open + 3 + at);
        let mut matched = None;
        let mut info_end = line_end;
        'info: loop {
            for newline in [true, false] {
                if newline && bytes.get(info_end) != Some(&b'\n') {
                    continue;
                }
                let start = info_end + usize::from(newline);
                if let Some(at) = value[start..].find("```") {
                    matched = Some((start, start + at));
                    break 'info;
                }
            }
            if info_end == open + 3 {
                break;
            }
            info_end -= 1;
            while !value.is_char_boundary(info_end) {
                info_end -= 1;
            }
        }
        match matched {
            Some((body_start, body_end)) => {
                out.push_str(&value[cursor..open]);
                out.push_str(&value[body_start..body_end]);
                cursor = body_end + 3;
            }
            None => {
                out.push_str(&value[cursor..open + 3]);
                cursor = open + 3;
            }
        }
    }
    out.push_str(&value[cursor..]);
    out
}

/// `![alt](src)` and `[label](href)` reduced to their visible text, in that order.
fn strip_links(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let rest = &value[cursor..];
        let image = rest.starts_with("![");
        let bracket = if image {
            2
        } else {
            usize::from(rest.starts_with('['))
        };
        if bracket == 0 {
            let step = rest.chars().next().map_or(1, char::len_utf8);
            out.push_str(&rest[..step]);
            cursor += step;
            continue;
        }
        let label = match rest[bracket..].find(']') {
            Some(at) => &rest[bracket..bracket + at],
            None => {
                out.push_str(&rest[..bracket]);
                cursor += bracket;
                continue;
            }
        };
        let after_label = bracket + label.len() + 1;
        // `[^\]]*` for an image, `[^\]]+` for a link: an empty link label is not a match.
        let destination = if (image || !label.is_empty()) && rest[after_label..].starts_with('(') {
            rest[after_label + 1..]
                .find(')')
                .map(|at| &rest[after_label + 1..after_label + 1 + at])
        } else {
            None
        };
        match destination {
            Some(destination) if !destination.contains(')') => {
                out.push_str(label);
                cursor += after_label + destination.len() + 2;
            }
            _ => {
                out.push_str(&rest[..bracket]);
                cursor += bracket;
            }
        }
    }
    out
}

/// `` `code` `` reduced to its content.
fn strip_inline_code(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(found) = value[cursor..].find('`') {
        let open = cursor + found;
        let close = value[open + 1..].find('`').map(|at| open + 1 + at);
        match close {
            Some(close) if close > open + 1 => {
                out.push_str(&value[cursor..open]);
                out.push_str(&value[open + 1..close]);
                cursor = close + 1;
            }
            _ => {
                out.push_str(&value[cursor..open + 1]);
                cursor = open + 1;
            }
        }
    }
    out.push_str(&value[cursor..]);
    out
}

/// The block marker a line opens with, as `^\s{0,3}(?:#{1,6}|>|[-+*]|\d+[.)])\s+` reads it.
///
/// Returns the length of the whole match, including the leading whitespace and the run of
/// whitespace after the marker.
fn block_marker_len(value: &str, start: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut indents: Vec<usize> = Vec::with_capacity(4);
    let mut at = start;
    for _ in 0..3 {
        let Some(character) = value[at..].chars().next() else {
            break;
        };
        if !is_js_space(character) {
            break;
        }
        at += character.len_utf8();
        indents.push(at);
    }
    indents.push(start);
    indents.reverse();
    for indent in indents {
        let mut after = indent;
        let hashes = value[indent..]
            .bytes()
            .take_while(|byte| *byte == b'#')
            .count()
            .min(6);
        if hashes > 0 {
            after = indent + hashes;
        } else if matches!(bytes.get(indent), Some(b'>' | b'-' | b'+' | b'*')) {
            after = indent + 1;
        } else {
            let digits = value[indent..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if digits > 0 && matches!(bytes.get(indent + digits), Some(b'.' | b')')) {
                after = indent + digits + 1;
            }
        }
        if after == indent {
            continue;
        }
        let spaces: usize = value[after..]
            .chars()
            .take_while(|character| is_js_space(*character))
            .map(char::len_utf8)
            .sum();
        if spaces > 0 {
            return Some(after + spaces - start);
        }
    }
    None
}

/// `markdown.replace(/^\s{0,3}(?:#{1,6}|>|[-+*]|\d+[.)])\s+/gm, '')`.
fn strip_block_markers(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    let mut line_start = Some(0usize);
    while let Some(start) = line_start {
        if start >= cursor {
            if let Some(length) = block_marker_len(value, start) {
                out.push_str(&value[cursor..start]);
                cursor = start + length;
            }
        }
        line_start = value[start..].find('\n').map(|at| start + at + 1);
    }
    out.push_str(&value[cursor..]);
    out
}

/// `markdown.replace(/(?:\*\*|\*|~~|(?<![A-Za-z0-9])_+|_+(?![A-Za-z0-9]))/g, '')`.
///
/// Underscores drop only where they mark emphasis; the ones inside snake_case identifiers are part
/// of the word and stay. The lookarounds read the original text, which is what makes a doubled
/// underscore between two words drop one character per pass and vanish.
fn strip_emphasis(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let rest = &value[cursor..];
        if rest.starts_with("**") {
            cursor += 2;
            continue;
        }
        if rest.starts_with('*') {
            cursor += 1;
            continue;
        }
        if rest.starts_with("~~") {
            cursor += 2;
            continue;
        }
        if rest.starts_with('_') {
            let run = rest.bytes().take_while(|byte| *byte == b'_').count();
            let before = value[..cursor].chars().next_back();
            if !is_ascii_alphanumeric(before) {
                cursor += run;
                continue;
            }
            // `_+(?![A-Za-z0-9])`, greedy with backtracking: the longest run whose next character
            // is not alphanumeric, which for an interior run is the run minus its last underscore.
            let mut taken = run;
            while taken > 0 {
                let next = value[cursor + taken..].chars().next();
                if !is_ascii_alphanumeric(next) {
                    break;
                }
                taken -= 1;
            }
            if taken > 0 {
                cursor += taken;
                continue;
            }
        }
        let step = rest.chars().next().map_or(1, char::len_utf8);
        out.push_str(&rest[..step]);
        cursor += step;
    }
    let _ = bytes;
    out
}

/// ``markdown.replace(/\\([\\`*_[\]{}()#+\-.!>])/g, '$1')``.
///
/// A separate pass, exactly as in the TypeScript: the emphasis pass above runs first and has
/// already eaten the asterisk out of a `\*`, so folding the two into one scan would keep an escaped
/// character the shipped rules drop.
fn strip_escapes(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let rest = &value[cursor..];
        if let Some(after) = rest.strip_prefix('\\') {
            if let Some(escaped) = after.chars().next() {
                if "\\`*_[]{}()#+-.!>".contains(escaped) {
                    out.push(escaped);
                    cursor += 1 + escaped.len_utf8();
                    continue;
                }
            }
        }
        let step = rest.chars().next().map_or(1, char::len_utf8);
        out.push_str(&rest[..step]);
        cursor += step;
    }
    out
}

/// The reasoning text with its markup taken off, as a one-line label can carry it.
pub fn plain_reasoning_text(markdown: &str) -> String {
    let without_fences = strip_fences(markdown);
    let without_links = strip_links(&without_fences);
    let without_code = strip_inline_code(&without_links);
    let without_markers = strip_block_markers(&without_code);
    let without_emphasis = strip_emphasis(&without_markers);
    js_trim(&strip_escapes(&without_emphasis)).to_string()
}

/// The first non-empty line of the stripped reasoning, for a one-line label.
pub fn plain_reasoning_teaser(markdown: &str) -> String {
    plain_reasoning_text(markdown)
        .split('\n')
        .map(js_trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// `^\s{0,3}(?:[-+*]\s|\d+[.)]\s|>|\||```|~~~)`, tested against one line.
///
/// A list item, a table row, a blockquote, or a fence opener means something to the markdown
/// renderer that plain text on the trigger cannot carry, so that line and everything after it stay
/// in the body.
fn non_hoistable_reasoning_line(line: &str) -> bool {
    let mut at = 0;
    for _ in 0..3 {
        let Some(character) = line[at..].chars().next() else {
            break;
        };
        if !is_js_space(character) {
            break;
        }
        at += character.len_utf8();
    }
    // `\s{0,3}` is greedy but every continuation below starts with a non-space, so only the
    // longest indent can match and no backtracking is observable.
    let rest = &line[at..];
    if rest.starts_with('>')
        || rest.starts_with('|')
        || rest.starts_with("```")
        || rest.starts_with("~~~")
    {
        return true;
    }
    let bullet = rest.starts_with(['-', '+', '*']);
    if bullet {
        return rest[1..].chars().next().is_some_and(is_js_space);
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && matches!(rest.as_bytes().get(digits), Some(b'.' | b')'))
        && rest[digits + 1..].chars().next().is_some_and(is_js_space)
}

/// `text.split(/\n[ \t]*\n+/)`.
fn split_paragraphs(value: &str) -> Vec<&str> {
    let bytes = value.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < value.len() {
        if bytes[at] != b'\n' {
            at += 1;
            continue;
        }
        let mut after = at + 1;
        while matches!(bytes.get(after), Some(b' ' | b'\t')) {
            after += 1;
        }
        let newlines = value[after..]
            .bytes()
            .take_while(|byte| *byte == b'\n')
            .count();
        if newlines == 0 {
            at += 1;
            continue;
        }
        parts.push(&value[start..at]);
        at = after + newlines;
        start = at;
    }
    parts.push(&value[start..]);
    parts
}

/// `paragraph.replace(/\s*\n\s*/g, ' ')`.
fn rejoin_wrapped_lines(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(found) = value[cursor..].find('\n') {
        let newline = cursor + found;
        let mut start = newline;
        while let Some(character) = value[cursor..start].chars().next_back() {
            if !is_js_space(character) {
                break;
            }
            start -= character.len_utf8();
        }
        let mut end = newline + 1;
        while let Some(character) = value[end..].chars().next() {
            if !is_js_space(character) {
                break;
            }
            end += character.len_utf8();
        }
        out.push_str(&value[cursor..start]);
        out.push(' ');
        cursor = end;
    }
    out.push_str(&value[cursor..]);
    out
}

/// The heading and the body a reasoning row splits into.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReasoningSplit {
    pub headline: String,
    pub body: String,
}

/// CDXC:SessionChat 2026-09-04 DECISION:
/// User: a reasoning row with tool calls under it must "always show all of the text wrapped"; it
/// used to hoist only the first line and clamp it to one row with an ellipsis, so the reader had to
/// expand the row to finish the sentence. The heading therefore owns every leading line that plain
/// text can carry (paragraphs, headings, emphasis, inline code, links), and the body renders only
/// what follows the first line that needs the markdown renderer, so nothing is printed twice and the
/// chevron folds the tool calls rather than the thought.
pub fn split_reasoning_headline(markdown: &str) -> ReasoningSplit {
    let lines = split_newlines(markdown);
    let split = lines
        .iter()
        .position(|line| non_hoistable_reasoning_line(line))
        .unwrap_or(lines.len());
    let leading = lines[..split].join("\n");
    let headline = split_paragraphs(&plain_reasoning_text(&leading))
        .into_iter()
        .map(|paragraph| js_trim(&rejoin_wrapped_lines(paragraph)).to_string())
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if headline.is_empty() {
        return ReasoningSplit {
            headline: plain_reasoning_teaser(markdown),
            body: markdown.to_string(),
        };
    }
    ReasoningSplit {
        headline,
        body: js_trim(&lines[split..].join("\n")).to_string(),
    }
}

/// `markdown.split(/\r?\n[\t ]*---[\t ]*(?:\r?\n|$)/)`.
fn split_user_turn_separator(markdown: &str) -> Vec<&str> {
    let bytes = markdown.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < markdown.len() {
        if bytes[at] != b'\n' {
            at += 1;
            continue;
        }
        let opening = if at > 0 && bytes[at - 1] == b'\r' {
            at - 1
        } else {
            at
        };
        let mut cursor = at + 1;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        if !markdown[cursor..].starts_with("---") {
            at += 1;
            continue;
        }
        cursor += 3;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        let end = if markdown[cursor..].starts_with("\r\n") {
            cursor + 2
        } else if markdown[cursor..].starts_with('\n') {
            cursor + 1
        } else if cursor == markdown.len() {
            cursor
        } else {
            at += 1;
            continue;
        };
        parts.push(&markdown[start..opening]);
        start = end;
        at = end.max(at + 1);
    }
    parts.push(&markdown[start..]);
    parts
}

/// A user turn whose repeated `---` sections collapse back into what the reader actually wrote.
pub fn normalize_user_message_markdown(markdown: &str) -> String {
    let parts: Vec<&str> = split_user_turn_separator(markdown)
        .into_iter()
        .map(js_trim)
        .collect();
    if parts.len() == 1 {
        return markdown.to_string();
    }
    let mut visible: Vec<String> = Vec::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        match visible
            .iter()
            .position(|candidate| candidate.starts_with(part))
        {
            None => visible.push(part.to_string()),
            Some(index) => {
                let remainder = js_trim_start(&visible[index][part.len()..]).to_string();
                visible[index] = if remainder.is_empty() {
                    part.to_string()
                } else {
                    format!("{part}\n\n{remainder}")
                };
            }
        }
    }
    visible.join("\n\n")
}

/// The Markdown a copy of a user turn puts on the clipboard.
///
/// Legacy agent transcripts carry a picture as a separate image block. Copy has to restore a named
/// reference for those blocks or the reader loses the one thing that names the file they attached.
/// Modern linked references stay in the turn's text, so both their authored position and copyable
/// path survive.
pub fn user_turn_copy_markdown(markdown: &str, images: &[&ChatBlock]) -> String {
    let references: Vec<String> = images
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            ChatBlock::ImageRef { path, url, .. } => path
                .as_deref()
                .or(url.as_deref())
                .map(|href| format!("[Image #{}]({href})", index + 1)),
            _ => None,
        })
        .collect();
    let joined = references.join(" ");
    [joined.as_str(), markdown]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Content requirements for the transcript's message action buttons.
///
/// Ported from `packages/shared/session-chat-presentation/message-actions.ts`.
pub fn message_action_content(markdown: &str) -> serde_json::Value {
    serde_json::json!({
        "copyable": !markdown.is_empty(),
        "canAnnotate": !js_trim(markdown).is_empty(),
        "canSaveMarkdown": split_newlines(markdown).iter().filter(|line| !js_trim(line).is_empty()).count() > 1,
    })
}
