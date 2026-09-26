//! Inserting and removing the numbered references the composer writes.
//!
//! Port of `packages/shared/session-chat-presentation/references.ts`. Every offset here is a UTF-16
//! code unit, the same unit `references.ts` counts, because the host applies them to its own text
//! field.

use serde::Serialize;

use crate::composer::reference_pills::{ends_with_image_extension, media_kind, path_noun};
use crate::composer::text::Utf16Text;

/// The composer text plus where the caret ends up, both in UTF-16 code units.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ComposerEdit {
    pub text: String,
    pub caret: usize,
}

/// `/\[Image #(\d+)·?\]\(/g`: the highest existing image number in the draft, plus one.
pub fn next_image_reference_index(text: &str) -> u32 {
    next_named_reference_index(text, "Image")
}

/// Images, videos, audio, and PDFs each count on their own: `/\[<noun> #(\d+)·?\]\(/g`, plus one.
fn next_named_reference_index(text: &str, noun: &str) -> u32 {
    next_reference_index(text, |text, start| read_named_label(text, start, noun))
}

/// `/\[(?:\\.|[^\]\\\r\n])* #(\d+)\]\(/g`: numbered file references include attachment labels and
/// descriptive picker labels alike.
pub fn next_file_reference_index(text: &str) -> u32 {
    next_reference_index(text, read_numbered_label)
}

fn next_reference_index(
    text: &str,
    read: impl Fn(&Utf16Text, usize) -> Option<(u32, usize)>,
) -> u32 {
    let indexed = Utf16Text::new(text);
    let mut highest = 0;
    let mut cursor = 0;
    while cursor < indexed.len() {
        match read(&indexed, cursor) {
            Some((index, end)) => {
                highest = highest.max(index);
                cursor = end;
            }
            None => cursor += 1,
        }
    }
    highest + 1
}

/// `\[<noun> #(\d+)·?\]\(` anchored at `start`.
fn read_named_label(text: &Utf16Text, start: usize, noun: &str) -> Option<(u32, usize)> {
    let mut index = start;
    for expected in format!("[{noun} #").chars() {
        if text.at(index) != Some(expected) {
            return None;
        }
        index += 1;
    }
    let (number, after) = read_digits(text, index)?;
    let after = if text.at(after) == Some('\u{b7}') {
        after + 1
    } else {
        after
    };
    if text.at(after) != Some(']') || text.at(after + 1) != Some('(') {
        return None;
    }
    Some((number, after + 2))
}

/// `\[(?:\\.|[^\]\\\r\n])* #(\d+)\]\(` anchored at `start`.
///
/// The label may be empty here (`*`, not `+`), and it must be followed by a space, the digits, and
/// the `](` opener. The label alternation cannot hold an unescaped `]`, so the run ends at the
/// first one and no shorter run can end at a `]` either.
fn read_numbered_label(text: &Utf16Text, start: usize) -> Option<(u32, usize)> {
    if text.at(start) != Some('[') {
        return None;
    }
    let mut index = start + 1;
    // The second half of a `\\.` pair is consumed as one alternative, so the engine can never
    // backtrack to a boundary inside it; a ` #` spelled as an escaped space is not a separator.
    let mut escaped = Vec::new();
    loop {
        match text.at(index) {
            Some('\\') => {
                text.at(index + 1)?;
                escaped.push(index + 1);
                index += 2;
            }
            Some(']') | Some('\r') | Some('\n') | None => break,
            Some(_) => index += 1,
        }
    }
    if text.at(index) != Some(']') || text.at(index + 1) != Some('(') {
        return None;
    }
    // Backtrack over the label for ` #<digits>` immediately before the `]`.
    let mut digits_start = index;
    while digits_start > start + 1
        && text
            .at(digits_start - 1)
            .is_some_and(|c| c.is_ascii_digit())
    {
        digits_start -= 1;
    }
    if digits_start == index || digits_start < start + 3 {
        return None;
    }
    if text.at(digits_start - 1) != Some('#') || text.at(digits_start - 2) != Some(' ') {
        return None;
    }
    if escaped.contains(&(digits_start - 2)) {
        return None;
    }
    let (number, _) = read_digits(text, digits_start)?;
    Some((number, index + 2))
}

fn read_digits(text: &Utf16Text, start: usize) -> Option<(u32, usize)> {
    let mut index = start;
    let mut digits = String::new();
    while let Some(character) = text.at(index) {
        if !character.is_ascii_digit() {
            break;
        }
        digits.push(character);
        index += 1;
    }
    if digits.is_empty() {
        return None;
    }
    // `Number.parseInt` saturates into a double, and `Number.isFinite` keeps it; a run longer than
    // a `u32` cannot be a real reference number, so it clamps rather than failing the whole scan.
    Some((digits.parse::<u32>().unwrap_or(u32::MAX), index))
}

/// The reference the `@` picker and the attachment bridge insert for `path`: `[Image #N](path)`,
/// `[Video #N](path)`, and so on, falling back to `[File #N](path)`.
pub fn native_path_reference(path: &str, text: &str) -> String {
    if ends_with_image_extension(path) {
        format!("[Image #{}]({path})", next_image_reference_index(text))
    } else if media_kind(path).is_some() {
        let noun = path_noun(path);
        format!(
            "[{noun} #{}]({path})",
            next_named_reference_index(text, noun)
        )
    } else {
        format!("[File #{}]({path})", next_file_reference_index(text))
    }
}

/// Inserts one reference per uploaded path into a question answer, the way the composer inserts a
/// pasted attachment (the user's `CDXC:Clipboard` decision on `finish_answer_attachments`). The
/// upload can finish after more typing; the selection captured when the paste started then no
/// longer belongs to the draft, so the references go at the end instead.
pub fn insert_answer_attachments(
    current: &str,
    paths: &[&str],
    original: &str,
    start: usize,
    end: usize,
) -> ComposerEdit {
    let unchanged = current == original;
    let mut edit = ComposerEdit {
        text: current.to_string(),
        caret: if unchanged {
            start
        } else {
            current.encode_utf16().count()
        },
    };
    let mut finish = if unchanged { end } else { edit.caret };
    for path in paths {
        let reference = native_path_reference(path, &edit.text);
        edit = insert_reference(&edit.text, &reference, edit.caret, finish);
        finish = edit.caret;
    }
    edit
}

/// Inserts `reference` at `[start, end)`, adding the spaces the sentence around it needs.
pub fn insert_reference(current: &str, reference: &str, start: usize, end: usize) -> ComposerEdit {
    let indexed = Utf16Text::new(current);
    let start_index = indexed.index_of_offset(start);
    let end_index = indexed.index_of_offset(end);
    let needs_leading_space =
        start_index > 0 && !indexed.at(start_index - 1).is_some_and(is_js_whitespace);
    let inserted = format!("{}{reference} ", if needs_leading_space { " " } else { "" });
    ComposerEdit {
        text: format!(
            "{}{inserted}{}",
            indexed.slice(0, start_index),
            indexed.slice(end_index, indexed.len())
        ),
        caret: start + inserted.encode_utf16().count(),
    }
}

/// Drops the reference an attachment thumbnail stands for, the way removing a pasted image did in
/// `packages/core-ui/chat/session-chat-composer.tsx`: one leading space and one trailing space go
/// with it so the surrounding sentence keeps its spacing.
pub fn remove_reference(current: &str, start: usize, end: usize) -> ComposerEdit {
    let indexed = Utf16Text::new(current);
    let start_index = indexed.index_of_offset(start);
    let end_index = indexed.index_of_offset(end);
    let from = if start_index > 0
        && indexed
            .at(start_index - 1)
            .is_some_and(is_horizontal_whitespace)
    {
        start_index - 1
    } else {
        start_index
    };
    let to = if indexed.at(end_index).is_some_and(is_horizontal_whitespace) {
        end_index + 1
    } else {
        end_index
    };
    ComposerEdit {
        text: format!(
            "{}{}",
            indexed.slice(0, from),
            indexed.slice(to, indexed.len())
        ),
        caret: indexed.offset(from),
    }
}

/// `/[^\S\r\n]/`: whitespace that is not a line break.
fn is_horizontal_whitespace(character: char) -> bool {
    is_js_whitespace(character) && character != '\r' && character != '\n'
}

/// `/\s/` as JavaScript defines it, which is Unicode whitespace plus the BOM.
pub fn is_js_whitespace(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}
