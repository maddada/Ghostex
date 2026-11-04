//! The diff's syntax colouring: a small language-agnostic tokenizer (keywords, types, strings,
//! numbers, comments) that works on one line at a time, and tab expansion.
//! Moved from the retired commit review window (window/git_commit_modal/diff.rs).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffTokenKind {
    Attribute,
    Comment,
    Function,
    Keyword,
    Literal,
    Number,
    Operator,
    Plain,
    String,
    Type,
}

const KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "defer",
    "do",
    "else",
    "enum",
    "export",
    "extension",
    "final",
    "for",
    "from",
    "func",
    "function",
    "guard",
    "if",
    "implements",
    "import",
    "in",
    "interface",
    "let",
    "mut",
    "new",
    "override",
    "package",
    "private",
    "protected",
    "protocol",
    "public",
    "return",
    "static",
    "struct",
    "super",
    "switch",
    "throws",
    "throw",
    "try",
    "type",
    "var",
    "where",
    "while",
];

const LITERALS: &[&str] = &[
    "false",
    "nil",
    "null",
    "None",
    "self",
    "Self",
    "this",
    "true",
    "undefined",
];

const PRIMITIVE_TYPES: &[&str] = &[
    "Array",
    "Bool",
    "Boolean",
    "Date",
    "Dictionary",
    "Double",
    "Float",
    "Int",
    "Map",
    "Number",
    "Object",
    "Set",
    "String",
    "Void",
];

fn is_space(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
}

fn is_identifier_part(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn is_operator(ch: char) -> bool {
    "{}[]().,:;?<>+=*/%!&|^~-".contains(ch)
}

/// The code's tokens as byte ranges of `content` (a diff line without its marker); tokens never
/// overlap and cover it in order.
pub(crate) fn tokenize_diff_line(content: &str) -> Vec<(std::ops::Range<usize>, DiffTokenKind)> {
    let mut tokens = Vec::new();
    let offset = 0;
    let code: Vec<(usize, char)> = content[offset..]
        .char_indices()
        .map(|(index, ch)| (index + offset, ch))
        .collect();
    let end = content.len();
    let at = |index: usize| code.get(index).map(|(_, ch)| *ch);
    let byte = |index: usize| code.get(index).map(|(byte, _)| *byte).unwrap_or(end);
    let mut index = 0;
    while index < code.len() {
        let ch = code[index].1;
        let start = byte(index);
        if is_space(ch) {
            while at(index).is_some_and(is_space) {
                index += 1;
            }
            tokens.push((start..byte(index), DiffTokenKind::Plain));
            continue;
        }
        if ch == '/' && at(index + 1) == Some('/') {
            tokens.push((start..end, DiffTokenKind::Comment));
            break;
        }
        if ch == '/' && at(index + 1) == Some('*') {
            let mut cursor = index + 2;
            while cursor < code.len() && !(at(cursor) == Some('*') && at(cursor + 1) == Some('/')) {
                cursor += 1;
            }
            let cursor = (cursor + 2).min(code.len());
            tokens.push((start..byte(cursor), DiffTokenKind::Comment));
            index = cursor;
            continue;
        }
        if ch == '#' && (index == 0 || at(index - 1).is_some_and(is_space)) {
            tokens.push((start..end, DiffTokenKind::Comment));
            break;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            let mut cursor = index + 1;
            let mut escaped = false;
            while cursor < code.len() {
                let next = code[cursor].1;
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == ch {
                    cursor += 1;
                    break;
                }
                cursor += 1;
            }
            tokens.push((start..byte(cursor), DiffTokenKind::String));
            index = cursor;
            continue;
        }
        if ch == '@' && at(index + 1).is_some_and(is_identifier_start) {
            let mut cursor = index + 1;
            while at(cursor).is_some_and(is_identifier_part) {
                cursor += 1;
            }
            tokens.push((start..byte(cursor), DiffTokenKind::Attribute));
            index = cursor;
            continue;
        }
        if ch.is_ascii_digit() {
            while at(index).is_some_and(|c| c.is_ascii_hexdigit() || "._xXoObB".contains(c)) {
                index += 1;
            }
            tokens.push((start..byte(index), DiffTokenKind::Number));
            continue;
        }
        if is_identifier_start(ch) {
            let mut cursor = index;
            while at(cursor).is_some_and(is_identifier_part) {
                cursor += 1;
            }
            let word = &content[start..byte(cursor)];
            let next = (cursor..code.len())
                .map(|i| code[i].1)
                .find(|c| !is_space(*c));
            let kind = if KEYWORDS.contains(&word) {
                DiffTokenKind::Keyword
            } else if LITERALS.contains(&word) {
                DiffTokenKind::Literal
            } else if PRIMITIVE_TYPES.contains(&word)
                || word.starts_with(|c: char| c.is_ascii_uppercase())
            {
                DiffTokenKind::Type
            } else if next == Some('(') {
                DiffTokenKind::Function
            } else {
                DiffTokenKind::Plain
            };
            tokens.push((start..byte(cursor), kind));
            index = cursor;
            continue;
        }
        if is_operator(ch) {
            while at(index).is_some_and(is_operator) {
                index += 1;
            }
            tokens.push((start..byte(index), DiffTokenKind::Operator));
            continue;
        }
        tokens.push((start..byte(index + 1), DiffTokenKind::Plain));
        index += 1;
    }
    tokens
}

/// `tab-size: 2`: each tab advances to the next even column. Returns the expanded text and a map
/// from the expanded text back to the source bytes, so token ranges can be carried over.
pub(crate) fn expand_tabs(content: &str) -> (String, Vec<usize>) {
    if !content.contains('\t') {
        return (content.to_string(), Vec::new());
    }
    let mut out = String::with_capacity(content.len() + 8);
    // `offsets[source_byte]` = byte in `out`.
    let mut offsets = Vec::with_capacity(content.len() + 1);
    let mut column = 0usize;
    for ch in content.chars() {
        for _ in 0..ch.len_utf8() {
            offsets.push(out.len());
        }
        if ch == '\t' {
            let width = 2 - column % 2;
            out.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    offsets.push(out.len());
    (out, offsets)
}
