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
//!   * a finished ```mermaid fence is wrapped in marks, so the renderer draws the diagram, and a
//!     finished ```visual (or ```vega-lite) fence likewise, so it draws the chart or page card, and
//!     a finished ```questions fence, with the form's id, so it draws the question form;
//!   * a picture written into prose becomes a marked token carrying its image source;
//!   * a file reference written as inline code, or a path somebody typed into a prompt, becomes a
//!     real Markdown link, so the native reference pill and the React file chip point at the same
//!     file.
//!
//!   * an agent's one-click answer links (`[Yes](ghostex-answer:Q901)`) leave the question's line
//!     for a marked line under it (`inline_answers.rs`), which the renderer draws as one pill.
//!
//! Rust only splits on the markers and lays the pieces out.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The marker vocabulary below is mirrored in apps/desktop/src/app/native_chat/rich_markdown.rs,
//! apps/desktop/src/app/native_chat/code_block.rs, apps/desktop/src/app/native_chat/mermaid.rs and
//! apps/mobile/app/src/chat/native/transcript/markdown/parse.ts. Change them together.

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
pub const NATIVE_MERMAID_OPEN: &str = "\u{e000}mermaid";
pub const NATIVE_MERMAID_CLOSE: &str = "\u{e000}/mermaid";
/// A finished ```visual (or ```vega-lite) fence: the chat draws the block's chart, stats, table or
/// page card in place of the code.
pub const NATIVE_VISUAL_OPEN: &str = "\u{e000}visual";
pub const NATIVE_VISUAL_CLOSE: &str = "\u{e000}/visual";
/// A finished ```questions fence: the chat draws the form whose id follows the colon
/// (`question_forms.rs`), which the document's `questionForms` holds.
pub const NATIVE_QUESTIONS_OPEN: &str = "\u{e000}questions:";
pub const NATIVE_QUESTIONS_CLOSE: &str = "\u{e000}/questions";

/// What the native code-block header shows, as JSON on the fence's info string: the file the fence
/// names (`label`, `icon`, `href`), and the command its Run button runs (`run`, `run_command.rs`).
fn fence_header(info: &str, run: Option<String>) -> Option<String> {
    let trimmed = js_trim(info);
    let language_len: usize = trimmed
        .chars()
        .take_while(|character| !is_js_space(*character))
        .map(char::len_utf8)
        .sum();
    let meta = js_trim(&trimmed[language_len..]);
    let mut entries: Vec<(&str, Value)> = Vec::new();
    if let Some(title) = fence_title(if meta.is_empty() { None } else { Some(meta) }) {
        let separator = title.rfind(['/', '\\']).map_or(0, |at| at + 1);
        entries.push(("label", title.into()));
        entries.push(("icon", file_path_icon_name(&title[separator..]).into()));
        if let Some(reference) = resolve_fence_title_file_path(title) {
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
    }
    // The command travels percent-encoded: a fence written without a language has only this header
    // on its info string, whose first word a Markdown parser takes as the language, and a parser
    // also undoes backslash escapes there, which would break a quoted command's JSON.
    if let Some(run) = run {
        entries.push((
            "run",
            crate::transcript::run_command::encode_command(&run).into(),
        ));
    }
    (!entries.is_empty()).then(|| format!("{NATIVE_MARK}{}", stringify_pairs(&entries)))
}

/// The command a finished fence runs: its body, without the fence's own indent, judged by
/// `run_command::fence_command`. A fence still streaming offers nothing.
fn fence_command_of(
    lines: &[&str],
    open: usize,
    character: u8,
    run: usize,
    info: &str,
) -> Option<String> {
    let close = lines[open + 1..].iter().position(|line| {
        fence_close(line).is_some_and(|(closing, length)| closing == character && length >= run)
    })? + open
        + 1;
    let indent = lines[open].len() - lines[open].trim_start_matches(' ').len();
    let body = lines[open + 1..close]
        .iter()
        .map(|line| {
            let own = line.len() - line.trim_start_matches(' ').len();
            &line[own.min(indent)..]
        })
        .collect::<Vec<_>>()
        .join("\n");
    let language = js_trim(info).split(is_js_space).next().unwrap_or_default();
    crate::transcript::run_command::fence_command(language, &body)
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
pub(crate) fn fence_open(line: &str) -> Option<(u8, usize, &str)> {
    let (character, run, rest) = fence_run(line)?;
    (!rest.contains(['\r', '\n', '\u{2028}', '\u{2029}'])).then_some((character, run, rest))
}

/// `^ {0,3}(`{3,}|~{3,})\s*$`, where `\s` does match a carriage return.
pub(crate) fn fence_close(line: &str) -> Option<(u8, usize)> {
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

/// Marked Markdown with its question forms back to plain code blocks: the marks go, the fence lines
/// stay.
pub fn without_question_forms(marked: String) -> String {
    if !marked.contains(NATIVE_QUESTIONS_OPEN) {
        return marked;
    }
    split_lf(&marked)
        .into_iter()
        .filter(|line| !line.starts_with(NATIVE_QUESTIONS_OPEN) && *line != NATIVE_QUESTIONS_CLOSE)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The line that closes a drawn fence (```mermaid, or a ```visual block) opened at `open`, when
/// the fence is finished, with the marks that wrap it.
///
/// CDXC:SessionChat 2026-09-30 WHY: A fence still streaming stays an ordinary code block until its closing run arrives, as the React chat kept an unclosed diagram pending, so the renderer never tries to draw half a diagram on every streamed token. The marked section keeps the fence lines verbatim: the renderer draws the diagram from the lines between them and shows them as its source.
/// CDXC:SessionChat 2026-10-06 DECISION: User: agents show charts, stats and tables inline in the chat as a ```visual block (charts in a strict Vega-Lite subset), drawn natively by every client from the block's JSON with no JavaScript engine. The block is marked exactly as a Mermaid diagram is, so a half-streamed block stays code.
fn closed_drawn_fence(
    lines: &[&str],
    open: usize,
    character: u8,
    run: usize,
    info: &str,
) -> Option<(usize, &'static str, &'static str)> {
    let language = js_trim(info).split(is_js_space).next().unwrap_or_default();
    let (open_mark, close_mark) = if language.eq_ignore_ascii_case("mermaid") {
        (NATIVE_MERMAID_OPEN, NATIVE_MERMAID_CLOSE)
    } else if ghostex_gx_visual::is_visual_language(language) {
        (NATIVE_VISUAL_OPEN, NATIVE_VISUAL_CLOSE)
    } else if crate::transcript::question_forms::is_questions_language(language) {
        (NATIVE_QUESTIONS_OPEN, NATIVE_QUESTIONS_CLOSE)
    } else {
        return None;
    };
    lines[open + 1..]
        .iter()
        .position(|line| {
            fence_close(line).is_some_and(|(closing, length)| closing == character && length >= run)
        })
        .map(|offset| (open + 1 + offset, open_mark, close_mark))
}

/// One pass over the lines: annotate fenced blocks, mark the pictures written into prose, and lift
/// GitHub alert quotes out of the Markdown into a marked section.
///
/// All three are line-structured, so this costs a scan rather than a second parse of every
/// streaming message.
fn mark_blocks(markdown: &str, commands: bool) -> String {
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
            if let Some((close, open_mark, close_mark)) =
                closed_drawn_fence(&lines, index, character, run, info)
            {
                result.push(if open_mark == NATIVE_QUESTIONS_OPEN {
                    let body = lines[index + 1..close].join("\n");
                    format!(
                        "{open_mark}{}",
                        crate::transcript::question_forms::form_id(&body)
                    )
                } else {
                    open_mark.to_string()
                });
                result.extend(lines[index..=close].iter().map(|line| (*line).to_string()));
                result.push(close_mark.to_string());
                index = close + 1;
                continue;
            }
            fence = Some((character, run));
            let run = commands
                .then(|| fence_command_of(&lines, index, character, run, info))
                .flatten();
            result.push(match fence_header(info, run) {
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
            // An agent's one-click answers: the question keeps its words and the answers move to a
            // marked line under it (`inline_answers.rs`).
            if commands && line.contains(crate::transcript::inline_answers::ANSWER_SCHEME) {
                if let Some(marked) = crate::transcript::inline_answers::mark_line(&lines, index) {
                    result.extend(marked.iter().map(|line| mark_inline_images(line)));
                    index += 1;
                    continue;
                }
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
        result.push(mark_blocks(&body[1..].join("\n"), commands));
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
    mentions: &[String],
    commands: bool,
    markdown: &str,
    image_references: &[(usize, usize)],
    edits: &mut Vec<Edit>,
) {
    for node in nodes {
        if let Node::Link(link) = node {
            // Only this pass writes `ghostex-run:` links (below); one the author wrote becomes its
            // own text, so no renderer draws a run chip for a command the reader cannot see.
            if is_run_link_destination(&link.url) {
                if let Some(position) = node.position() {
                    let text = match (
                        link.children.first().and_then(Node::position),
                        link.children.last().and_then(Node::position),
                    ) {
                        (Some(first), Some(last)) => {
                            markdown[first.start.offset..last.end.offset].to_string()
                        }
                        _ => String::new(),
                    };
                    edits.push(Edit {
                        start: position.start.offset,
                        end: position.end.offset,
                        text,
                    });
                }
            }
            continue;
        }
        if let Node::Definition(definition) = node {
            if is_run_link_destination(&definition.url) {
                if let Some(position) = node.position() {
                    edits.push(Edit {
                        start: position.start.offset,
                        end: position.end.offset,
                        text: String::new(),
                    });
                }
            }
            continue;
        }
        if matches!(node, Node::LinkReference(_)) {
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
                // A command keeps its code look: the span, backticks and all, becomes the text of a
                // link the renderers draw as the span with its Copy and Paste in terminal buttons
                // (`run_command.rs`).
                if let Some(command) = commands
                    .then(|| crate::transcript::run_command::inline_command(&code.value))
                    .flatten()
                {
                    edits.push(Edit {
                        start,
                        end,
                        text: format!(
                            "[{}]({})",
                            &markdown[start..end],
                            crate::transcript::run_command::command_href(&command)
                        ),
                    });
                }
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
                mentions,
                commands,
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
            // An `@` mention is a pill only when gxserver found it on disk (the SessionChat
            // 2026-10-28 DECISION in server/src/session_chat_mention_paths.rs); an npm scope or a
            // handle stays text.
            if found.mention && !mentions.contains(&found.path) {
                continue;
            }
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

/// A link destination in the run-command family (`ghostex-run:` and the hover bar's
/// `ghostex-run-copy:`/`ghostex-run-paste:`), whatever its case.
///
/// CDXC:SessionChat 2026-10-25 WHY:
/// The renderers draw a run chip with Copy and Paste in terminal for any `ghostex-run:` link, so an agent reply could write `[`npm test`](ghostex-run:<other command>%0D)` and have Paste type a hidden command and press Enter (security audit 2026-10-25). Links the author wrote with these destinations are reduced to their text here; only the inline commands this pass recognises become run links, and `run_command::command_from_href` refuses any line break or control character.
fn is_run_link_destination(url: &str) -> bool {
    url.trim_start()
        .get(..RUN_LINK_FAMILY.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(RUN_LINK_FAMILY))
}

const RUN_LINK_FAMILY: &str = "ghostex-run";

/// Whether the text mentions a run-command destination, which only an author-written link would.
/// A numeric character reference (`ghostex&#45;run:`) or a backslash escape (`ghostex\-run:`) spells
/// the scheme only once the parser decodes it, so their raw forms count too (security review
/// 2026-11-05): the check below then sees the decoded destination.
fn has_run_link_evidence(markdown: &str) -> bool {
    let bytes = markdown.as_bytes();
    bytes
        .windows(RUN_LINK_FAMILY.len())
        .any(|window| window.eq_ignore_ascii_case(RUN_LINK_FAMILY.as_bytes()))
        || bytes
            .windows(b"ghostex".len())
            .any(|window| window.eq_ignore_ascii_case(b"ghostex"))
        || markdown.contains("&#")
}

/// Turns the file references written into a message into real Markdown links, which is what the
/// native renderer draws reference pills from.
///
/// CDXC:SessionChat 2026-09-19 SEE-ALSO:
/// resolve_inline_code_file_path in file_paths.rs is the single detection rule: a span that becomes
/// a file chip in React becomes a pill here, and ordinary code such as `cargo check` or
/// `chrome_ink().opacity(0.12)` becomes neither.
fn link_file_references(
    markdown: &str,
    bare_paths: bool,
    mentions: &[String],
    commands: bool,
) -> String {
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
        mentions,
        commands,
        markdown,
        &image_references,
        &mut edits,
    );
    // Applied from the back so an earlier edit's offsets stay valid.
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
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

/// A single-line backtick span holding a command the chat offers to run (`run_command.rs`).
fn has_inline_code_command_evidence(markdown: &str) -> bool {
    markdown.contains('`')
        && markdown.lines().any(|line| {
            code_spans(line).into_iter().any(|(start, end)| {
                crate::transcript::run_command::inline_command(&line[start + 1..end - 1]).is_some()
            })
        })
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

/// The root a link destination keeps its backslashes under: a drive (`C:\` with one separator,
/// or `C:/`) or a `\\server` UNC leader. A destination whose separators are already doubled is
/// ordinary escaped Markdown and is left alone, the same test the composer's `linked_destination`
/// makes.
fn is_literal_windows_destination(destination: &[u8]) -> bool {
    let drive = destination.first().is_some_and(u8::is_ascii_alphabetic)
        && destination.get(1) == Some(&b':');
    if drive && destination.get(2) == Some(&b'\\') {
        return destination.get(3) != Some(&b'\\');
    }
    if drive && destination.get(2) == Some(&b'/') {
        return !destination.windows(2).any(|pair| pair == b"\\\\");
    }
    destination.starts_with(b"\\\\") && destination.get(2) != Some(&b'\\')
}

/// Where an inline link destination starting at `start` ends (exclusive), reading backslashes
/// literally: at the `>` of a `<…>` destination, else at whitespace or the `)` that closes it.
fn literal_destination_end(line: &[u8], start: usize, bracketed: bool) -> Option<usize> {
    if bracketed {
        return line[start..]
            .iter()
            .position(|byte| *byte == b'>')
            .map(|offset| start + offset);
    }
    let mut depth = 0usize;
    for (offset, byte) in line[start..].iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' if depth == 0 => return Some(start + offset),
            b')' => depth -= 1,
            b' ' | b'\t' | b'\r' | b'\n' => return Some(start + offset),
            _ => {}
        }
    }
    Some(line.len())
}

/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: keep backslashes literally whenever a link destination is a Windows absolute path (a drive
/// letter followed by `:\` or `:/`, or a `\\server\share` path), so a link to
/// `C:\Users\me\.claude\x.md` opens that file; paths under `.claude` are common on Windows.
/// CommonMark would read `\.` as an escaped dot and open `C:\Users\me.claude\x.md`. Both clients
/// parse this Markdown with plain CommonMark (markdown-rs in the desktop's `TextView`, `parse.ts` on
/// the phone), so the rule is applied once here by doubling the destination's backslashes, which
/// every parser reads back as the literal path; the reference pills are keyed on the same text.
/// SEE-ALSO: apps/mobile/app/src/chat/native/transcript/markdown/parse.ts (`isLiteralWindowsDestination`),
/// composer/reference_pills.rs (`linked_destination`).
fn literal_windows_link_destinations(markdown: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for line in split_lf(markdown) {
        if let Some((character, length)) = fence {
            if fence_close(line).is_some_and(|(closing, run)| closing == character && run >= length)
            {
                fence = None;
            }
            result.push(line.to_string());
            continue;
        }
        if let Some((character, run, _)) = fence_open(line) {
            fence = Some((character, run));
            result.push(line.to_string());
            continue;
        }
        result.push(literal_windows_line(line));
    }
    result.join("\n")
}

/// Where an absolute Windows path written into prose ends, when one starts at `start`: a drive
/// letter with `:\` (`C:\Users\me\_scratch`) or a `\\server\share` path, up to whitespace or a
/// character Windows never allows in a path (`<>"|?*`, plus the backtick and brackets that end it in
/// Markdown). A run that is not one of those, such as `snake\_case`, is ordinary escaped Markdown.
pub(crate) fn windows_prose_path_end(bytes: &[u8], start: usize) -> Option<usize> {
    let before = start.checked_sub(1).map(|at| bytes[at]);
    if before
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'\\' | b':' | b'/'))
    {
        return None;
    }
    let drive = bytes.get(start).is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(start + 1) == Some(&b':')
        && bytes.get(start + 2) == Some(&b'\\');
    let unc = bytes.get(start) == Some(&b'\\')
        && bytes.get(start + 1) == Some(&b'\\')
        && bytes.get(start + 2).is_some_and(u8::is_ascii_alphanumeric);
    if !drive && !unc {
        return None;
    }
    let end = bytes[start..]
        .iter()
        .position(|byte| {
            byte.is_ascii_whitespace()
                || matches!(
                    byte,
                    b'<' | b'>' | b'"' | b'|' | b'?' | b'*' | b'`' | b'[' | b']'
                )
        })
        .map_or(bytes.len(), |offset| start + offset);
    is_literal_windows_destination(&bytes[start..end]).then_some(end)
}

/// CDXC:SessionChat 2026-10-17 WHY:
/// The 2026-10-06 rule for link destinations, extended to a Windows path an agent writes as plain
/// text: CommonMark read `C:\Users\me\dev\_scratch\x` as an escaped `_` and showed
/// `C:\Users\me\dev_scratch\x` on Windows, on the computer and the phone. An absolute Windows path in
/// prose gets its backslashes doubled here, so both parsers print it as written; code is never
/// touched, and `snake\_case` and other escapes outside such a path stay escapes. A path already
/// written with doubled backslashes fails `is_literal_windows_destination` and is left alone, which
/// keeps the phone's own copy of the rule from applying twice.
/// SEE-ALSO: apps/mobile/app/src/chat/native/transcript/markdown/parse.ts (`windowsProsePathEnd`),
/// transcript/message_text.rs (`strip_escapes`).
fn literal_windows_prose_paths(markdown: &str) -> String {
    let mut options = ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    let Ok(tree) = markdown::to_mdast(markdown, &options) else {
        return markdown.to_string();
    };
    let mut texts: Vec<(usize, usize)> = Vec::new();
    collect_text_ranges(&tree, &mut texts);
    let bytes = markdown.as_bytes();
    let mut paths: Vec<(usize, usize)> = Vec::new();
    for (start, end) in texts {
        let mut at = start;
        // Bounded by the text node, so a reply's hard-break backslash after a path stays a break.
        let text = &bytes[..end];
        while at < end {
            match windows_prose_path_end(text, at) {
                Some(path_end) => {
                    paths.push((at, path_end));
                    at = path_end;
                }
                None => at += 1,
            }
        }
    }
    if paths.is_empty() {
        return markdown.to_string();
    }
    let mut out = String::with_capacity(markdown.len() + 16);
    let mut copied = 0;
    for (start, end) in paths {
        out.push_str(&markdown[copied..start]);
        out.push_str(&markdown[start..end].replace('\\', "\\\\"));
        copied = end;
    }
    out.push_str(&markdown[copied..]);
    out
}

/// The source ranges of the prose text nodes: never code, a link destination or raw HTML.
fn collect_text_ranges(node: &Node, ranges: &mut Vec<(usize, usize)>) {
    if let Node::Text(text) = node {
        if let Some(position) = &text.position {
            ranges.push((position.start.offset, position.end.offset));
        }
        return;
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect_text_ranges(child, ranges);
    }
}

fn literal_windows_line(line: &str) -> String {
    if !line.contains("](") {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let spans = code_spans(line);
    let mut out = String::with_capacity(line.len() + 8);
    let mut copied = 0;
    let mut cursor = 0;
    while let Some(at) = line[cursor..].find("](") {
        let open = cursor + at;
        cursor = open + 2;
        if spans
            .iter()
            .any(|(start, end)| *start <= open && open < *end)
        {
            continue;
        }
        let mut start = open + 2;
        while matches!(bytes.get(start), Some(b' ' | b'\t')) {
            start += 1;
        }
        let bracketed = bytes.get(start) == Some(&b'<');
        if bracketed {
            start += 1;
        }
        let Some(end) = literal_destination_end(bytes, start, bracketed) else {
            continue;
        };
        if !is_literal_windows_destination(&bytes[start..end]) {
            continue;
        }
        out.push_str(&line[copied..start]);
        out.push_str(&line[start..end].replace('\\', "\\\\"));
        copied = end;
        cursor = end;
    }
    out.push_str(&line[copied..]);
    out
}

/// The Markdown the GPUI transcript renders for one message.
///
/// Inline-code file references are promoted in every message, the way React promotes them wherever
/// it renders Markdown. `bare_paths` follows React's `chatText` mode on top of that: an unmarked
/// path is only spotted in prose somebody typed, never in an agent's answer, where a path that
/// matters is already inline code or a link.
pub fn native_markdown(markdown: &str, bare_paths: bool) -> String {
    native_markdown_with_mentions(markdown, bare_paths, &[])
}

/// [`native_markdown`] for a message whose `@` mentions gxserver checked: only those in
/// `mentions` (`ChatMessage::mention_paths`) become file pills; with none, every `@…` stays text.
pub fn native_markdown_with_mentions(
    markdown: &str,
    bare_paths: bool,
    mentions: &[String],
) -> String {
    if markdown.is_empty() {
        return markdown.to_string();
    }
    // Every marker the renderers read starts with NATIVE_MARK, so one an agent wrote itself (a
    // fence language line carrying a fake Run header with a hidden command, a fake alert) is
    // dropped before anything is marked (security review 2026-11-05).
    let unmarked;
    let markdown = if markdown.contains(NATIVE_MARK) {
        unmarked = markdown.replace(NATIVE_MARK, "");
        unmarked.as_str()
    } else {
        markdown
    };
    let escaped = crate::transcript::raw_html::escape_raw_html(markdown);
    let markdown = escaped.as_str();
    let windows_links_wanted = markdown.contains("](")
        && (markdown.contains(":\\") || markdown.contains(":/") || markdown.contains("\\\\"));
    let windows_paths_wanted = markdown.contains(":\\") || markdown.contains("\\\\");
    let bare_wanted = bare_paths && has_path_evidence(markdown);
    // Commands are offered in what an agent wrote, never in a prompt somebody typed.
    let commands_wanted = !bare_paths && has_inline_code_command_evidence(markdown);
    let paths_wanted = bare_wanted
        || commands_wanted
        || has_run_link_evidence(markdown)
        || has_inline_code_path_evidence(markdown);
    let blocks_wanted = markdown.contains("```")
        || markdown.contains("~~~")
        || markdown.contains('>')
        || markdown.contains('|')
        || has_image_evidence(markdown)
        || (!bare_paths && markdown.contains(crate::transcript::inline_answers::ANSWER_SCHEME));
    if !paths_wanted && !blocks_wanted && !windows_links_wanted && !windows_paths_wanted {
        return markdown.to_string();
    }
    let linked = if paths_wanted {
        link_file_references(markdown, bare_wanted, mentions, commands_wanted)
    } else {
        markdown.to_string()
    };
    // After the inline-code references become links, so theirs keep their backslashes too.
    let linked = if windows_links_wanted || paths_wanted {
        literal_windows_link_destinations(&linked)
    } else {
        linked
    };
    let linked = if windows_paths_wanted {
        literal_windows_prose_paths(&linked)
    } else {
        linked
    };
    if blocks_wanted {
        mark_blocks(&linked, !bare_paths)
    } else {
        linked
    }
}
