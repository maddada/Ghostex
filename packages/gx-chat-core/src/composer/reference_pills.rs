//! The composer's reference pills: what a `[label](path)` in the draft is, and how wide it draws.
//!
//! Port of `packages/shared/session-chat-presentation/reference-pills.ts`. The TypeScript scanned
//! with `String.matchAll`; the regexes it used have no alternation a hand scanner cannot follow,
//! so the scan below is the same walk without a regex engine.
//!
//! CDXC:SessionChat 2026-09-18 DECISION:
//! User: the GPUI composer shows `[Image #1](/path)` as the same clickable reference pill the React
//! composer shows. Both read this one projection so a pill's kind, label, and width cannot drift.
//! Offsets count UTF-16 code units, which is what a JS string index is; the host converts them.

use serde::{Deserialize, Serialize};

use crate::composer::text::Utf16Text;

/// The marker that turns an expanded reference back into plain text.
pub const SESSION_CHAT_REFERENCE_REVEAL_MARKER: char = '·';

const REFERENCE_PILL_MAX_LABEL_CHARACTERS: usize = 18;
const REFERENCE_PILL_ICON_SPACE: &str = "\u{a0}\u{a0}\u{a0}\u{a0}\u{2009}";
const REFERENCE_PILL_TRAILING_SPACE: &str = "\u{2009}";

const EXTENSIONLESS_FILE_NAMES: &[&str] = &[
    "AGENTS",
    "AUTHORS",
    "BUILD",
    "Brewfile",
    "CHANGELOG",
    "CODEOWNERS",
    "COPYING",
    "Caddyfile",
    "Containerfile",
    "Dockerfile",
    "Gemfile",
    "LICENSE",
    "Makefile",
    "NOTICE",
    "Podfile",
    "Procfile",
    "README",
    "SKILL",
    "WORKSPACE",
];

/// What a reference points at, which picks its icon and its styling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceKind {
    File,
    Folder,
    Image,
    Skill,
    Url,
}

impl ReferenceKind {
    /// The wire spelling, which is what the identity string and the document carry.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
            Self::Image => "image",
            Self::Skill => "skill",
            Self::Url => "url",
        }
    }
}

/// One `[label](path)` the composer draws as a pill.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerReference {
    /// UTF-16 offset just past the closing `)`.
    pub end: usize,
    pub identity: String,
    pub kind: ReferenceKind,
    pub label: String,
    pub path: String,
    /// UTF-16 offset of the opening `[`.
    pub start: usize,
}

fn unescape_markdown(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
            // A trailing backslash has nothing to unescape, and `/\\(.)/g` leaves it in place.
            else {
                out.push('\\');
            }
        } else {
            out.push(character);
        }
    }
    out
}

/// `/\.(?:avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)(?:[?#].*)?$/i`.
///
/// The optional tail is `.*`, which never crosses a line break, so a query string containing a
/// newline is not an image path.
fn image_path_with_query(path: &str) -> bool {
    if ends_with_image_extension(path) {
        return true;
    }
    for (index, character) in path.char_indices() {
        if character != '?' && character != '#' {
            continue;
        }
        if path[index + character.len_utf8()..].contains('\n') {
            continue;
        }
        if ends_with_image_extension(&path[..index]) {
            return true;
        }
    }
    false
}

const IMAGE_EXTENSIONS: &[&str] = &[
    ".avif", ".bmp", ".gif", ".heic", ".heif", ".ico", ".jpg", ".jpeg", ".png", ".svg", ".tif",
    ".tiff", ".webp",
];

/// `/\.(avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)$/i`, the plain form.
pub fn ends_with_image_extension(path: &str) -> bool {
    let lowered = crate::transcript::jsstr::ascii_lower(path);
    IMAGE_EXTENSIONS
        .iter()
        .any(|extension| lowered.ends_with(extension))
}

/// `/(?:^|[\\/])SKILL\.md$/i`.
fn is_skill_file(path: &str) -> bool {
    let lowered = crate::transcript::jsstr::ascii_lower(path);
    if lowered == "skill.md" {
        return true;
    }
    lowered.ends_with("/skill.md") || lowered.ends_with("\\skill.md")
}

/// `/^https?:\/\//i`.
pub fn is_web_url(path: &str) -> bool {
    let lowered = crate::transcript::jsstr::ascii_lower(path);
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

/// `/^Image #\d+$/`, `/^File #\d+$/`, `/^Folder #\d+$/`.
fn numbered_label(label: &str, prefix: &str) -> bool {
    let Some(rest) = label.strip_prefix(prefix) else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
}

fn explicit_reference_kind(label: &str) -> Option<ReferenceKind> {
    if label.ends_with(SESSION_CHAT_REFERENCE_REVEAL_MARKER) {
        return None;
    }
    if numbered_label(label, "Image #") {
        return Some(ReferenceKind::Image);
    }
    if numbered_label(label, "File #") {
        return Some(ReferenceKind::File);
    }
    if numbered_label(label, "Folder #") {
        return Some(ReferenceKind::Folder);
    }
    if label.starts_with('$') {
        return Some(ReferenceKind::Skill);
    }
    None
}

/// The compact label shared by every editable reference-pill backend.
pub fn reference_display_label(label: &str, kind: ReferenceKind) -> String {
    if kind == ReferenceKind::Skill {
        return label.to_string();
    }
    let characters: Vec<char> = label.chars().collect();
    if characters.len() <= REFERENCE_PILL_MAX_LABEL_CHARACTERS {
        return label.to_string();
    }
    let mut out: String = characters[..REFERENCE_PILL_MAX_LABEL_CHARACTERS - 1]
        .iter()
        .collect();
    out.push('\u{2026}');
    out
}

/// Visible text whose measured width owns the pill icon and label.
pub fn reference_pill_text(label: &str, kind: ReferenceKind) -> String {
    format!(
        "{REFERENCE_PILL_ICON_SPACE}{}{REFERENCE_PILL_TRAILING_SPACE}",
        reference_display_label(label, kind).replace(' ', "\u{a0}")
    )
}

/// `/\b(?:folder|directory)\b/i` over a label.
fn mentions_folder(label: &str) -> bool {
    let lowered = crate::transcript::jsstr::ascii_lower(label);
    let bytes = lowered.as_bytes();
    for word in ["folder", "directory"] {
        let mut from = 0;
        while let Some(found) = lowered[from..].find(word) {
            let start = from + found;
            let end = start + word.len();
            let before_is_word = start > 0 && is_word_byte(bytes[start - 1]);
            let after_is_word = end < bytes.len() && is_word_byte(bytes[end]);
            if !before_is_word && !after_is_word {
                return true;
            }
            from = start + 1;
        }
    }
    false
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// `value.replace(/:\d+(?::\d+)?$/, '')`.
fn without_position(value: &str) -> &str {
    let trimmed = strip_digit_group(value);
    match trimmed {
        Some(rest) => strip_digit_group(rest).unwrap_or(rest),
        None => value,
    }
}

/// Removes one trailing `:\d+`, or `None` when there is none.
fn strip_digit_group(value: &str) -> Option<&str> {
    let digits = value.len() - value.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let head = &value[..value.len() - digits];
    head.strip_suffix(':')
}

/// `/\.[A-Za-z][A-Za-z0-9_+-]*$/` over a basename.
fn has_file_extension(basename: &str) -> bool {
    let Some(dot) = basename.rfind('.') else {
        return false;
    };
    let rest = &basename[dot + 1..];
    let mut characters = rest.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    characters.all(|character| {
        character.is_ascii_alphanumeric()
            || character == '_'
            || character == '+'
            || character == '-'
    })
}

/// Classifies any rendered machine-path link for the shared pill styling.
pub fn reference_kind(label: &str, path: &str) -> ReferenceKind {
    // `explicitReferenceKind(label.trim())`: `String.prototype.trim` strips U+FEFF and leaves
    // U+0085, and Rust's `str::trim` does the exact opposite.
    let explicit = explicit_reference_kind(crate::transcript::jsstr::js_trim(label));
    if let Some(kind) = explicit {
        if kind != ReferenceKind::Skill || is_skill_file(path) {
            return kind;
        }
    }
    if is_web_url(path) {
        return ReferenceKind::Url;
    }
    if image_path_with_query(path) {
        return ReferenceKind::Image;
    }
    if mentions_folder(label) || path.ends_with('/') || path.ends_with('\\') {
        return ReferenceKind::Folder;
    }
    let trimmed = without_position(path);
    let separator = trimmed
        .rfind(['/', '\\'])
        .map(|index| index as isize)
        .unwrap_or(-1);
    let basename = &trimmed[(separator + 1) as usize..];
    let dotfile = basename.starts_with('.') && basename.len() > 1 && !basename[1..].contains('.');
    if !basename.is_empty()
        && !has_file_extension(basename)
        && !EXTENSIONLESS_FILE_NAMES.contains(&basename)
        && !dotfile
    {
        return ReferenceKind::Folder;
    }
    ReferenceKind::File
}

struct Destination {
    /// Code point index just past the closing `)`.
    end: usize,
    path: String,
}

/// The `(...)` half of a Markdown link, angle-bracketed or nested-paren form.
fn linked_destination(text: &Utf16Text, destination_start: usize) -> Option<Destination> {
    if text.at(destination_start) == Some('<') {
        let mut index = destination_start + 1;
        while index < text.len() {
            let character = text.at(index)?;
            if character == '\n' || character == '\r' {
                return None;
            }
            if character == '\\' {
                index += 2;
                continue;
            }
            if character == '>' && text.at(index + 1) == Some(')') {
                return Some(Destination {
                    end: index + 2,
                    path: unescape_markdown(&text.slice(destination_start + 1, index)),
                });
            }
            index += 1;
        }
        return None;
    }

    let mut depth = 1;
    let mut index = destination_start;
    while index < text.len() {
        let character = text.at(index)?;
        if character == '\n' || character == '\r' {
            return None;
        }
        if character == '\\' {
            index += 2;
            continue;
        }
        if character == '(' {
            depth += 1;
            index += 1;
            continue;
        }
        if character != ')' {
            index += 1;
            continue;
        }
        depth -= 1;
        if depth == 0 {
            return Some(Destination {
                end: index + 1,
                path: unescape_markdown(&text.slice(destination_start, index)),
            });
        }
        index += 1;
    }
    None
}

/// `/\[((?:\\.|[^\]\\\r\n])+)]\(/g` from `start`, as one leftmost match.
///
/// The label alternation cannot contain an unescaped `]`, so the greedy run ends at the first one
/// and no shorter match can end at a `]` either: the scan below is the regex engine's whole search.
fn next_label(text: &Utf16Text, from: usize) -> Option<(usize, String, usize)> {
    let mut start = from;
    while start < text.len() {
        if text.at(start) != Some('[') {
            start += 1;
            continue;
        }
        let mut index = start + 1;
        let mut empty = true;
        let mut label = String::new();
        loop {
            match text.at(index) {
                Some('\\') => {
                    let Some(escaped) = text.at(index + 1) else {
                        break;
                    };
                    label.push('\\');
                    label.push(escaped);
                    index += 2;
                    empty = false;
                }
                Some(']') => break,
                Some('\r') | Some('\n') | None => break,
                Some(character) => {
                    label.push(character);
                    index += 1;
                    empty = false;
                }
            }
        }
        if !empty && text.at(index) == Some(']') && text.at(index + 1) == Some('(') {
            return Some((start, label, index + 2));
        }
        start += 1;
    }
    None
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

/// `/^(?:[a-z]:[\\/]|file:\/\/|https?:\/\/)/i`.
fn is_openable_scheme(value: &str) -> bool {
    let lowered = crate::transcript::jsstr::ascii_lower(value);
    if lowered.starts_with("file://") || is_web_url(&lowered) {
        return true;
    }
    let bytes = lowered.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// `value.replace(/:\d+(?:-\d+|:\d+)?$/, '')`, the composer's own coordinate strip.
fn path_without_coordinates(value: &str) -> &str {
    // The optional tail is tried first (greedy `?`), then the bare `:\d+`.
    let digits = value.len() - value.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return value;
    }
    let head = &value[..value.len() - digits];
    if let Some(before) = head.strip_suffix('-').or_else(|| head.strip_suffix(':')) {
        if let Some(stripped) = strip_digit_group(before) {
            return stripped;
        }
    }
    head.strip_suffix(':').unwrap_or(value)
}

/// Finds file, skill, image, and HTTP(S) links for every composer backend.
pub fn composer_references(text: &str) -> Vec<ComposerReference> {
    let indexed = Utf16Text::new(text);
    let mut references = Vec::new();
    let mut cursor = 0;
    while let Some((start, source_label, destination_start)) = next_label(&indexed, cursor) {
        cursor = destination_start;
        let label = unescape_markdown(&source_label);
        if label.ends_with(SESSION_CHAT_REFERENCE_REVEAL_MARKER)
            || (start > 0 && indexed.at(start - 1) == Some('!'))
        {
            continue;
        }
        let Some(destination) = linked_destination(&indexed, destination_start) else {
            continue;
        };
        if destination.path.is_empty() {
            continue;
        }
        let bare = path_without_coordinates(&destination.path);
        if destination.path.starts_with('#') || (has_uri_scheme(bare) && !is_openable_scheme(bare))
        {
            continue;
        }
        let kind = reference_kind(&label, &destination.path);
        if label.starts_with('$') && !is_skill_file(&destination.path) {
            continue;
        }
        references.push(ComposerReference {
            end: indexed.offset(destination.end),
            identity: format!("{}:{label}", kind.as_str()),
            kind,
            label,
            path: destination.path,
            start: indexed.offset(start),
        });
    }
    references
}
