//! What a Markdown href points at, and the editor coordinates it carries.
//!
//! **To fold into family b.** `packages/shared/session-chat-presentation/links.ts` and
//! `file-position.ts` (deleted 2026-10-01) are family b's on the port list, but the reference menu and the transcript
//! menu (both family d's pure queries) are the only callers that have landed so far. When family b
//! ports them, delete this file and call theirs; the behaviour below is the same walk.

use serde::{Deserialize, Serialize};

use crate::transcript::jsstr::js_trim;
use crate::transcript::links::{drive_path_without_leading_slash, file_uri_path};

/// Trailing editor coordinates on a path: `:913`, `:913-940`, or `:42:8`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePosition {
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// What the chat can do with an href.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Url(String),
    File(String),
    /// Nothing the chat's own surfaces can show.
    Inert,
}

/// A path split from a valid line, line-range, or line-and-column suffix.
///
/// `/:(\d{1,7})(?:-(\d{1,7})|(?::(\d{1,7})))?$/`, with the same "not a safe integer, not a
/// position" refusals.
pub fn split_file_position(value: &str) -> (String, Option<FilePosition>) {
    let Some((head, first)) = trailing_number(value) else {
        return (value.to_string(), None);
    };
    // The optional group is greedy, so a range or a column is tried before the bare line.
    if let Some(separator) = head.chars().last() {
        if separator == '-' || separator == ':' {
            let inner = &head[..head.len() - 1];
            if let Some((path, line)) = trailing_number(inner) {
                if let Some(path) = path.strip_suffix(':') {
                    let valid = if separator == '-' {
                        first >= line
                    } else {
                        first >= 1
                    };
                    if line >= 1 && valid {
                        return (
                            path.to_string(),
                            Some(FilePosition {
                                line,
                                end_line: (separator == '-').then_some(first),
                                column: (separator == ':').then_some(first),
                            }),
                        );
                    }
                    // A refused range or column is not retried as a bare line: the regex's own
                    // backtracking would, but every shorter alternative fails the `$` anchor.
                    return (value.to_string(), None);
                }
            }
        }
    }
    match head.strip_suffix(':') {
        Some(path) if first >= 1 => (
            path.to_string(),
            Some(FilePosition {
                line: first,
                end_line: None,
                column: None,
            }),
        ),
        _ => (value.to_string(), None),
    }
}

/// One trailing run of 1 to 7 digits, with what precedes it.
fn trailing_number(value: &str) -> Option<(&str, u32)> {
    let digits = value.len() - value.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 || digits > 7 {
        // More than seven digits cannot be `\d{1,7}` at the end of the string.
        return None;
    }
    let head = &value[..value.len() - digits];
    value[value.len() - digits..]
        .parse::<u32>()
        .ok()
        .map(|number| (head, number))
}

/// Classifies a Markdown href into what the chat can do with it.
pub fn classify_link_href(href: &str) -> LinkTarget {
    // `href.trim()` in the TypeScript, which is `String.prototype.trim`: it trims U+FEFF and
    // leaves U+0085, where Rust's `str::trim` does the exact opposite.
    let trimmed = js_trim(href);
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return LinkTarget::Inert;
    }
    let lowered = trimmed.to_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        return LinkTarget::Url(trimmed.to_string());
    }
    if lowered.starts_with("file://") {
        return LinkTarget::File(file_path_from_href(file_uri_path(
            &trimmed["file://".len()..],
        )));
    }
    if !is_windows_drive_path(trimmed) && has_uri_scheme(&split_file_position(trimmed).0) {
        return LinkTarget::Inert;
    }
    LinkTarget::File(file_path_from_href(trimmed))
}

/// The coordinates a Markdown destination carries, after path decoding.
pub fn file_position_from_href(href: &str) -> Option<FilePosition> {
    split_file_position(&decoded_file_href(href)).1
}

fn file_path_from_href(href: &str) -> String {
    split_file_position(&drive_path_without_leading_slash(decoded_file_href(href))).0
}

/// `decodeURI`, which decodes every `%XX` except the ones that spell a reserved delimiter.
///
/// A malformed escape leaves the raw href, exactly as the `catch` does.
fn decoded_file_href(href: &str) -> String {
    match decode_uri(href) {
        Some(decoded) => decoded,
        None => href.to_string(),
    }
}

/// The delimiters `decodeURI` leaves encoded: `;/?:@&=+$,#`.
const URI_RESERVED: &[u8] = b";/?:@&=+$,#";

fn decode_uri(href: &str) -> Option<String> {
    let bytes = href.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        let hex = href.get(index + 1..index + 3)?;
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let byte = u8::from_str_radix(hex, 16).ok()?;
        if byte < 0x80 && URI_RESERVED.contains(&byte) {
            out.extend_from_slice(&bytes[index..index + 3]);
        } else {
            out.push(byte);
        }
        index += 3;
    }
    String::from_utf8(out).ok()
}

/// `/^[a-z]:[\\/]/i`.
fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// `/^[a-z][a-z0-9+.-]*:/i`.
fn has_uri_scheme(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    for character in characters {
        if character == ':' {
            return true;
        }
        if !(character.is_ascii_alphanumeric()
            || character == '+'
            || character == '.'
            || character == '-')
        {
            return false;
        }
    }
    false
}
