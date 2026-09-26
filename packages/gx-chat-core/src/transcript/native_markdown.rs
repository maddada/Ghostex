//! The rich-Markdown decisions the GPUI transcript cannot make for itself.
//!
//! Ported from `packages/shared/session-chat-presentation/native-markdown.ts`.
//!
//! The native transcript hands one Markdown string to a single `TextView`, which parses and lays it
//! out in Rust. React reaches the same features through remark and rehype passes over its own AST,
//! and none of that is reachable from the renderer. So every decision stays here and travels to the
//! native renderer inside the Markdown itself, marked with a private-use character no agent writes:
//!
//!   * a fenced block that names a file gets its header's label, icon, and open-file target
//!     appended to the fence's info string;
//!   * a GitHub alert quote (`> [!NOTE]`) becomes a marked, unquoted section;
//!   * a picture written into prose becomes a marked token carrying its image source;
//!   * a file reference written as inline code, or a path somebody typed into a prompt, becomes a
//!     real Markdown link, so the native reference pill and the React file chip point at the same
//!     file.
//!
//! Rust only splits on the markers and lays the pieces out.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The marker vocabulary below is mirrored in apps/desktop/src/app/native_chat/rich_markdown.rs and
//! apps/desktop/src/app/native_chat/code_block.rs. Change them together.

use markdown::mdast::Node;
use markdown::ParseOptions;
use serde_json::Value;

use crate::composer::reference_pills::markdown_destination;
use crate::transcript::file_paths::{
    bare_file_paths, fence_title, file_path_icon_name, resolve_fence_title_file_path,
    resolve_inline_code_file_path,
};
use crate::transcript::file_position::file_position_suffix;
use crate::transcript::images::{image_source_pairs, ImageRef};
use crate::transcript::json_text::stringify_pairs;
use crate::transcript::jsstr::{
    ascii_lower, decode_uri, is_js_space, js_trim, split_lf, utf16_len, utf16_take,
};

/// Private-use character: never written by an agent, never rendered.
pub const NATIVE_MARK: char = '\u{e000}';
pub const NATIVE_ALERT_OPEN: &str = "\u{e000}alert:";
pub const NATIVE_ALERT_CLOSE: &str = "\u{e000}/alert";
pub const NATIVE_TABLE_OPEN: &str = "\u{e000}table";
pub const NATIVE_TABLE_CLOSE: &str = "\u{e000}/table";
pub const NATIVE_IMAGE_OPEN: &str = "\u{e000}image:";

/// What the native code-block header shows, as JSON on the fence's info string.
fn fence_header(info: &str) -> Option<String> {
    let trimmed = js_trim(info);
    let language_len: usize = trimmed
        .chars()
        .take_while(|character| !is_js_space(*character))
        .map(char::len_utf8)
        .sum();
    let meta = js_trim(&trimmed[language_len..]);
    let title = fence_title(if meta.is_empty() { None } else { Some(meta) })?;
    let separator = title.rfind(['/', '\\']).map_or(0, |at| at + 1);
    let reference = resolve_fence_title_file_path(title);
    let mut entries: Vec<(&str, Value)> = vec![
        ("label", title.into()),
        ("icon", file_path_icon_name(&title[separator..]).into()),
    ];
    if let Some(reference) = reference {
        entries.push((
            "href",
            format!(
                "{}{}",
                reference.path,
                file_position_suffix(reference.position.as_ref())
            )
            .into(),
        ));
    }
    Some(format!("{NATIVE_MARK}{}", stringify_pairs(&entries)))
}

/// One picture, as the native renderer needs it: the same source fields the projected image blocks
/// carry, so `images.rs` loads an authored picture through the transport it already uses for
/// attachments.
///
/// `fallback` overwrites `label` with React's own stand-in text (the link's words, or the alt),
/// because that is what React shows when the bytes cannot be read.
fn image_mark(href: &str, fallback: &str, alt: &str) -> String {
    let destination = js_trim(href);
    let lower = ascii_lower(destination);
    let block = if lower.starts_with("http:")
        || lower.starts_with("https:")
        || lower.starts_with("data:")
    {
        ImageRef {
            path: None,
            url: Some(destination.to_string()),
            alt: Some(alt.to_string()),
        }
    } else {
        // Markdown destinations arrive percent-encoded; a machine path needs the literal characters
        // back. Malformed escapes: the raw destination is the best reading of it.
        let path = decode_uri(destination).unwrap_or_else(|| destination.to_string());
        ImageRef {
            path: Some(path),
            url: None,
            alt: Some(alt.to_string()),
        }
    };
    let source = image_source_pairs(&block, Some(fallback));
    format!(
        "{NATIVE_IMAGE_OPEN}{}{NATIVE_MARK}",
        stringify_pairs(&source)
    )
}

/// `` `[^`\r\n]*` `` spans on one line, as byte ranges.
fn code_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    while let Some(at) = line[cursor..].find('`') {
        let open = cursor + at;
        let body = &line[open + 1..];
        let end = body
            .char_indices()
            .find(|(_, character)| matches!(character, '`' | '\r' | '\n'))
            .filter(|(_, character)| *character == '`')
            .map(|(index, _)| open + 1 + index);
        match end {
            Some(close) => {
                spans.push((open, close + 1));
                cursor = close + 1;
            }
            None => {
                cursor = open + 1;
            }
        }
    }
    spans
}

/// An inline image or link at `start`, returning (bang, label, decoded href, byte length).
fn inline_image_at(line: &str, start: usize) -> Option<(bool, &str, String, usize)> {
    let rest = &line[start..];
    let bang = rest.starts_with('!');
    let after_bang = usize::from(bang);
    if !rest[after_bang..].starts_with('[') {
        return None;
    }
    let label_start = after_bang + 1;
    let label_end = rest[label_start..]
        .char_indices()
        .find(|(_, character)| matches!(character, ']' | '\r' | '\n'))
        .filter(|(_, character)| *character == ']')
        .map(|(index, _)| label_start + index)?;
    if !rest[label_end + 1..].starts_with('(') {
        return None;
    }
    let href_start = label_end + 2;
    let (href, consumed) = markdown_destination(&rest[href_start..])?;
    if href.is_empty() {
        return None;
    }
    Some((
        bang,
        &rest[label_start..label_end],
        href,
        href_start + consumed,
    ))
}

/// `\.(avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)$` on a destination with its query and
/// hash cut off.
fn names_a_picture(href: &str) -> bool {
    let without_query = js_trim(href)
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();
    let Some(dot) = without_query.rfind('.') else {
        return false;
    };
    matches!(
        ascii_lower(&without_query[dot + 1..]).as_str(),
        "avif"
            | "bmp"
            | "gif"
            | "heic"
            | "heif"
            | "ico"
            | "jpg"
            | "jpeg"
            | "png"
            | "svg"
            | "tif"
            | "tiff"
            | "webp"
    )
}

/// Marks the pictures on one line: every Markdown image, and every link whose destination names a
/// picture.
fn mark_inline_images(line: &str) -> String {
    if !line.contains("](") {
        return line.to_string();
    }
    let code = code_spans(line);
    let mut result = String::new();
    let mut cursor = 0;
    let mut scan = 0;
    while scan < line.len() {
        if !line.is_char_boundary(scan) {
            scan += 1;
            continue;
        }
        let Some((picture, text, href, length)) = inline_image_at(line, scan) else {
            scan += 1;
            continue;
        };
        let end = scan + length;
        if code.iter().any(|(from, to)| scan < *to && end > *from) {
            scan += 1;
            continue;
        }
        if !picture && !names_a_picture(&href) {
            scan += 1;
            continue;
        }
        let text = js_trim(text);
        result.push_str(&line[cursor..scan]);
        let fallback = if text.is_empty() { "Image" } else { text };
        let alt = if picture { text } else { fallback };
        result.push_str(&image_mark(&href, fallback, alt));
        cursor = end;
        scan = end;
    }
    if cursor == 0 {
        line.to_string()
    } else {
        result + &line[cursor..]
    }
}

fn leading_indent(line: &str, max: usize) -> Option<usize> {
    let spaces = line.bytes().take_while(|byte| *byte == b' ').count();
    (spaces <= max).then_some(spaces)
}

/// The indent and fence run a line opens with, and whatever follows it.
fn fence_run(line: &str) -> Option<(u8, usize, &str)> {
    let indent = leading_indent(line, 3)?;
    let rest = &line[indent..];
    let character = rest.as_bytes().first().copied()?;
    if character != b'`' && character != b'~' {
        return None;
    }
    let run = rest.bytes().take_while(|byte| *byte == character).count();
    (run >= 3).then(|| (character, run, &rest[run..]))
}

/// `^( {0,3})(`{3,}|~{3,})(.*)$`.
///
/// `.` does not match a line terminator and `$` without the `m` flag only matches the end of the
/// string, so an info string carrying a stray carriage return fails the match entirely and the line
/// is NOT treated as a fence opener. A CRLF transcript therefore gets no code-block headers, which
/// is what the shipped rules do and what this reproduces.
fn fence_open(line: &str) -> Option<(u8, usize, &str)> {
    let (character, run, rest) = fence_run(line)?;
    (!rest.contains(['\r', '\n', '\u{2028}', '\u{2029}'])).then_some((character, run, rest))
}

/// `^ {0,3}(`{3,}|~{3,})\s*$`, where `\s` does match a carriage return.
fn fence_close(line: &str) -> Option<(u8, usize)> {
    let (character, run, rest) = fence_run(line)?;
    js_trim(rest).is_empty().then_some((character, run))
}

fn is_quote_line(line: &str) -> bool {
    leading_indent(line, 3).is_some_and(|indent| line[indent..].starts_with('>'))
}

/// `line.replace(/^ {0,3}> ?/, '')`, the one marker at the start.
fn strip_quote_marker(line: &str) -> &str {
    let Some(indent) = leading_indent(line, 3) else {
        return line;
    };
    let Some(rest) = line[indent..].strip_prefix('>') else {
        return line;
    };
    rest.strip_prefix(' ').unwrap_or(rest)
}

const ALERT_NAMES: [&str; 5] = ["note", "tip", "important", "warning", "caution"];

/// `^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$` case-insensitively.
fn alert_marker(line: &str) -> Option<&'static str> {
    let rest = line.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let name = ascii_lower(&rest[..close]);
    if !js_trim(&rest[close + 1..]).is_empty() {
        return None;
    }
    ALERT_NAMES.into_iter().find(|candidate| *candidate == name)
}

/// `^ {0,3}> ?\[!(?:NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]` case-insensitively.
fn is_quoted_alert(text: &str) -> bool {
    let Some(indent) = leading_indent(text, 3) else {
        return false;
    };
    let Some(rest) = text[indent..].strip_prefix('>') else {
        return false;
    };
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    let Some(rest) = rest.strip_prefix("[!") else {
        return false;
    };
    // Byte ranges through `get`, not `&rest[..n]`: the names are ASCII but `rest` is the user's
    // text, and a name-length cut can land inside a character (`[!no\u{20ac}]]` cut at byte 4).
    ALERT_NAMES.into_iter().any(|name| {
        rest.as_bytes()
            .get(..name.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(name.as_bytes()))
            && rest.as_bytes().get(name.len()) == Some(&b']')
    })
}

/// GFM's delimiter row, the one line that tells a table from prose with pipes in it:
/// `^ {0,3}\|?(?: *:?-+:? *\|)+ *(?::?-+:? *\|? *)?$`.
fn is_table_delimiter(line: &str) -> bool {
    let Some(indent) = leading_indent(line, 3) else {
        return false;
    };
    let bytes = line.as_bytes();
    let mut at = indent;
    if bytes.get(at) == Some(&b'|') {
        at += 1;
    }
    let spaces = |at: &mut usize| {
        while bytes.get(*at) == Some(&b' ') {
            *at += 1
        }
    };
    // `(?: *:?-+:? *\|)+`
    let mut cells = 0;
    loop {
        let start = at;
        spaces(&mut at);
        if bytes.get(at) == Some(&b':') {
            at += 1;
        }
        let dashes = line[at..].bytes().take_while(|byte| *byte == b'-').count();
        if dashes == 0 {
            at = start;
            break;
        }
        at += dashes;
        if bytes.get(at) == Some(&b':') {
            at += 1;
        }
        spaces(&mut at);
        if bytes.get(at) != Some(&b'|') {
            at = start;
            break;
        }
        at += 1;
        cells += 1;
    }
    if cells == 0 {
        return false;
    }
    spaces(&mut at);
    if at == line.len() {
        return true;
    }
    // `(?::?-+:? *\|? *)?$`
    if bytes.get(at) == Some(&b':') {
        at += 1;
    }
    let dashes = line[at..].bytes().take_while(|byte| *byte == b'-').count();
    if dashes == 0 {
        return false;
    }
    at += dashes;
    if bytes.get(at) == Some(&b':') {
        at += 1;
    }
    spaces(&mut at);
    if bytes.get(at) == Some(&b'|') {
        at += 1;
    }
    spaces(&mut at);
    at == line.len()
}

/// One pass over the lines: annotate fenced blocks, mark the pictures written into prose, and lift
/// GitHub alert quotes out of the Markdown into a marked section.
///
/// All three are line-structured, so this costs a scan rather than a second parse of every
/// streaming message.
fn mark_blocks(markdown: &str) -> String {
    let lines = split_lf(markdown);
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let mut fence: Option<(u8, usize)> = None;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if let Some((character, length)) = fence {
            if let Some((closing, run)) = fence_close(line) {
                if closing == character && run >= length {
                    fence = None;
                }
            }
            result.push(line.to_string());
            index += 1;
            continue;
        }
        if let Some((character, run, info)) = fence_open(line) {
            fence = Some((character, run));
            result.push(match fence_header(info) {
                Some(header) => format!("{line} {header}"),
                None => line.to_string(),
            });
            index += 1;
            continue;
        }
        if !is_quote_line(line) {
            // A table is marked rather than rewritten: the native renderer draws its toolbar around
            // the section and copies these very lines.
            if line.contains('|')
                && lines
                    .get(index + 1)
                    .is_some_and(|next| is_table_delimiter(next))
            {
                let mut last = index + 1;
                while last + 1 < lines.len() && lines[last + 1].contains('|') {
                    last += 1;
                }
                result.push(NATIVE_TABLE_OPEN.to_string());
                result.extend(lines[index..=last].iter().map(|line| (*line).to_string()));
                result.push(NATIVE_TABLE_CLOSE.to_string());
                index = last + 1;
                continue;
            }
            result.push(mark_inline_images(line));
            index += 1;
            continue;
        }
        let mut end = index;
        while end + 1 < lines.len() && is_quote_line(lines[end + 1]) {
            end += 1;
        }
        let body: Vec<&str> = lines[index..=end]
            .iter()
            .map(|line| strip_quote_marker(line))
            .collect();
        let Some(marker) = body.first().and_then(|line| alert_marker(line)) else {
            result.extend(lines[index..=end].iter().map(|line| (*line).to_string()));
            index = end + 1;
            continue;
        };
        result.push(format!("{NATIVE_ALERT_OPEN}{marker}"));
        // The body is ordinary Markdown once the quote markers are gone, so its own fences still
        // get their headers.
        result.push(mark_blocks(&body[1..].join("\n")));
        result.push(NATIVE_ALERT_CLOSE.to_string());
        index = end + 1;
    }
    result.join("\n")
}

/// One reference, written as the Markdown link the native renderer draws a pill from.
///
/// The path travels unchanged: escaping is undone by the parser, so the href and the label the pill
/// is looked up by are the literal path again.
fn file_link(path: &str, label: &str) -> String {
    let destination = if path
        .chars()
        .any(|character| is_js_space(character) || character == '(' || character == ')')
    {
        format!(
            "<{}>",
            path.replace('\\', "\\\\")
                .replace('<', "\\<")
                .replace('>', "\\>")
        )
    } else {
        path.to_string()
    };
    let mut escaped = String::with_capacity(label.len());
    for character in label.chars() {
        if "\\`*_[]<>~".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    format!("[{escaped}]({destination})")
}

/// `\[Image #\d+\]\([^)\r\n]+\)`, every match, as byte ranges.
fn composer_image_references(markdown: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(at) = markdown[cursor..].find("[Image #") {
        let start = cursor + at;
        cursor = start + 1;
        let rest = &markdown[start + "[Image #".len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || !rest[digits..].starts_with("](") {
            continue;
        }
        let href = &rest[digits + 2..];
        let Some(close) = href
            .char_indices()
            .find(|(_, character)| matches!(character, ')' | '\r' | '\n'))
        else {
            continue;
        };
        if close.1 != ')' || close.0 == 0 {
            continue;
        }
        let end = start + "[Image #".len() + digits + 2 + close.0 + 1;
        found.push((start, end));
        cursor = end;
    }
    found
}

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

fn collect_edits(
    nodes: &[Node],
    bare: bool,
    markdown: &str,
    image_references: &[(usize, usize)],
    edits: &mut Vec<Edit>,
) {
    for node in nodes {
        if matches!(
            node,
            Node::Link(_) | Node::LinkReference(_) | Node::Definition(_)
        ) {
            continue;
        }
        if matches!(node, Node::Code(_) | Node::Html(_)) {
            continue;
        }
        let span = node
            .position()
            .map(|position| (position.start.offset, position.end.offset));
        if let Node::InlineCode(code) = node {
            let Some((start, end)) = span else {
                continue;
            };
            let Some(reference) = resolve_inline_code_file_path(&code.value) else {
                continue;
            };
            let target = format!(
                "{}{}",
                reference.path,
                file_position_suffix(reference.position.as_ref())
            );
            // The whole span including its backticks becomes the link, so the pill reads as the
            // path React's chip shows rather than as quoted code.
            edits.push(Edit {
                start,
                end,
                text: file_link(&target, &target),
            });
            continue;
        }
        if let Some(children) = node.children() {
            // `markdown.slice(start ?? 0, 60)` in the TypeScript: an end index, not a length, so a
            // blockquote past offset 60 tests the empty string. Reproduced rather than corrected.
            let quoted_alert = matches!(node, Node::Blockquote(_)) && {
                // `markdown.slice(start ?? 0, 60)`: an END index in UTF-16 code units, not a
                // length, so a blockquote past offset 60 tests the empty string. The bound is
                // taken in code units here rather than in bytes, because `&markdown[start..60]`
                // panics whenever byte 60 falls inside a character.
                let start = span.map_or(0, |(start, _)| start).min(markdown.len());
                let before = markdown.get(..start).map_or(0, utf16_len);
                let head = match 60usize.checked_sub(before) {
                    Some(units) => utf16_take(markdown.get(start..).unwrap_or(""), units),
                    None => "",
                };
                is_quoted_alert(head)
            };
            collect_edits(
                children,
                bare && !quoted_alert,
                markdown,
                image_references,
                edits,
            );
            continue;
        }
        let Some((start, end)) = span else {
            continue;
        };
        if !bare || !matches!(node, Node::Text(_)) {
            continue;
        }
        // Scanned against the source rather than the node's value: an escape in the text would
        // otherwise shift every offset after it.
        let source = &markdown[start..end];
        for found in bare_file_paths(source) {
            let from = start + found.start;
            let to = start + found.end;
            if image_references
                .iter()
                .any(|(reference_start, reference_end)| {
                    from < *reference_end && to > *reference_start
                })
            {
                continue;
            }
            edits.push(Edit {
                start: from,
                end: to,
                text: file_link(&found.path, &found.path),
            });
        }
    }
}

/// Turns the file references written into a message into real Markdown links, which is what the
/// native renderer draws reference pills from.
///
/// CDXC:SessionChat 2026-09-19 SEE-ALSO:
/// resolve_inline_code_file_path in file_paths.rs is the single detection rule: a span that becomes
/// a file chip in React becomes a pill here, and ordinary code such as `cargo check` or
/// `chrome_ink().opacity(0.12)` becomes neither.
fn link_file_references(markdown: &str, bare_paths: bool) -> String {
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::default()) else {
        return markdown.to_string();
    };
    let image_references = if bare_paths {
        composer_image_references(markdown)
    } else {
        Vec::new()
    };
    let mut edits: Vec<Edit> = Vec::new();
    let children = tree.children().map(Vec::as_slice).unwrap_or_default();
    collect_edits(
        children,
        bare_paths,
        markdown,
        &image_references,
        &mut edits,
    );
    // Applied from the back so an earlier edit's offsets stay valid.
    edits.sort_by(|left, right| right.start.cmp(&left.start));
    let mut result = markdown.to_string();
    for edit in edits {
        if edit.start > result.len() || edit.end > result.len() {
            continue;
        }
        result = format!(
            "{}{}{}",
            &result[..edit.start],
            edit.text,
            &result[edit.end..]
        );
    }
    result
}

/// `[\\/@]|:\d`.
fn has_path_evidence(markdown: &str) -> bool {
    let bytes = markdown.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        matches!(byte, b'\\' | b'/' | b'@')
            || (*byte == b':' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
    })
}

/// A single-line backtick span holding something a path needs: a separator or a `:line`.
///
/// Cheap enough to run on every message, and it keeps an answer whose only backticks belong to
/// fences out of the parser.
fn has_inline_code_path_evidence(markdown: &str) -> bool {
    let mut cursor = 0;
    while let Some(at) = markdown[cursor..].find('`') {
        let open = cursor + at;
        let body = &markdown[open + 1..];
        let close = body
            .char_indices()
            .find(|(_, character)| matches!(character, '`' | '\r' | '\n'))
            .filter(|(_, character)| *character == '`');
        let Some((close, _)) = close else {
            cursor = open + 1;
            continue;
        };
        let span = &body[..close];
        let bytes = span.as_bytes();
        let carries = bytes.iter().enumerate().any(|(index, byte)| {
            matches!(byte, b'\\' | b'/')
                || (*byte == b':' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        });
        if carries {
            return true;
        }
        // The regular expression is not anchored, so a span that does not qualify is retried from
        // the character after its opening backtick.
        cursor = open + 1;
    }
    false
}

/// Whether a Markdown image or a link to a picture needs the inline-image pass.
fn has_image_evidence(markdown: &str) -> bool {
    if markdown.contains("![") {
        return true;
    }
    let mut cursor = 0;
    while let Some(at) = markdown[cursor..].find("](") {
        let start = cursor + at + 2;
        cursor = start;
        if markdown_destination(&markdown[start..]).is_some_and(|(href, _)| names_a_picture(&href))
        {
            return true;
        }
    }
    false
}

/// The Markdown the GPUI transcript renders for one message.
///
/// Inline-code file references are promoted in every message, the way React promotes them wherever
/// it renders Markdown. `bare_paths` follows React's `chatText` mode on top of that: an unmarked
/// path is only spotted in prose somebody typed, never in an agent's answer, where a path that
/// matters is already inline code or a link.
pub fn native_markdown(markdown: &str, bare_paths: bool) -> String {
    if markdown.is_empty() {
        return markdown.to_string();
    }
    let bare_wanted = bare_paths && has_path_evidence(markdown);
    let paths_wanted = bare_wanted || has_inline_code_path_evidence(markdown);
    let blocks_wanted = markdown.contains("```")
        || markdown.contains("~~~")
        || markdown.contains('>')
        || markdown.contains('|')
        || has_image_evidence(markdown);
    if !paths_wanted && !blocks_wanted {
        return markdown.to_string();
    }
    let linked = if paths_wanted {
        link_file_references(markdown, bare_wanted)
    } else {
        markdown.to_string()
    };
    if blocks_wanted {
        mark_blocks(&linked)
    } else {
        linked
    }
}
