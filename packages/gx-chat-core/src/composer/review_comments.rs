//! Review comments from the Changes view: the pill a comment becomes in the chat box, the block
//! the agent receives in its place, and the pill a sent block turns back into in the transcript.
//!
//! CDXC:Changes 2026-10-25 DECISION:
//! User: "i want the comments to be attached like pills inside the chat, not like text like they do now", and "when i send 2 comments from changes view to composer then only last one is there". A comment joins the chat box as a `[file L12: first words](ghostex-comment:…)` reference pill after whatever is there; on send each pill becomes the full block (file, lines as a diff, the comment), and a sent block shows as the same pill in the transcript. A terminal has no pills, so it still gets the block as text.
//! SEE-ALSO: apps/desktop/src/app/native_changes/comments.rs builds the comment; server/src/session_chat_queue_runtime/send.rs expands the pills on the way to the agent; the pill's colours and icon are `comment` in packages/gx-chat-core/visual/reference-visual.json, which apps/mobile/app/src/chat/native/transcript/theme.ts keeps equal.

use serde::{Deserialize, Serialize};

use crate::composer::reference_pills::{composer_references, ReferenceKind};

/// The destination scheme a review comment pill carries; its payload is the comment as JSON.
pub const REVIEW_COMMENT_SCHEME: &str = "ghostex-comment:";

/// How much of the comment a pill's label keeps.
const LABEL_WORDS_CHARACTERS: usize = 24;

/// One comment on picked diff lines.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    /// The file's path in the project.
    pub file: String,
    /// `L186`, `L186 to L189`, `L12 (before)`.
    pub lines: String,
    /// What the lines were picked from: `the branch's changes`, `the uncommitted changes`.
    pub scope: String,
    /// The picked lines as diff lines (`+`, `-` or a space first).
    pub excerpt: Vec<String>,
    pub text: String,
}

impl ReviewComment {
    /// The block the agent reads: where, the comment, then the lines as a diff.
    pub fn block(&self) -> String {
        let fence_len = self
            .excerpt
            .iter()
            .flat_map(|line| line.split(|ch| ch != '`'))
            .map(str::len)
            .max()
            .unwrap_or(0)
            .max(2)
            + 1;
        let fence = "`".repeat(fence_len);
        format!(
            "Review comment on `{}` {} ({}):\n{}\n{fence}diff\n{}\n{fence}\n",
            self.file,
            self.lines,
            self.scope,
            self.text.trim(),
            self.excerpt.join("\n"),
        )
    }

    /// The pill's words: the file's name, the lines and the comment's start.
    pub fn label(&self) -> String {
        let name = self.file.rsplit(['/', '\\']).next().unwrap_or(&self.file);
        let words: String = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut start: String = words.chars().take(LABEL_WORDS_CHARACTERS).collect();
        if words.chars().count() > LABEL_WORDS_CHARACTERS {
            start = format!("{}\u{2026}", start.trim_end());
        }
        let label = if start.is_empty() {
            format!("{name} {}", self.lines)
        } else {
            format!("{name} {}: {start}", self.lines)
        };
        // A label cannot hold an unescaped bracket or a backslash in a Markdown link.
        label
            .chars()
            .map(|character| match character {
                '[' => '(',
                ']' => ')',
                '\\' => '/',
                other => other,
            })
            .collect()
    }

    /// What hovering the pill shows: where, then the comment.
    pub fn tooltip(&self) -> String {
        format!("{} {}\n{}", self.file, self.lines, self.text.trim())
    }

    /// The `[label](ghostex-comment:…)` the chat box shows as a pill.
    pub fn reference(&self) -> String {
        let payload = serde_json::to_string(self).unwrap_or_default();
        format!(
            "[{}]({REVIEW_COMMENT_SCHEME}{})",
            self.label(),
            percent_encode(&payload)
        )
    }

    /// The comment a pill's destination carries.
    pub fn from_href(href: &str) -> Option<Self> {
        let payload = href.trim().strip_prefix(REVIEW_COMMENT_SCHEME)?;
        serde_json::from_str(&percent_decode(payload)?).ok()
    }

    /// One sent block read back, or `None` when `lines` does not start with one.
    ///
    /// Returns the comment and how many lines the block took.
    fn parse_block(lines: &[&str]) -> Option<(Self, usize)> {
        let header = lines.first()?.strip_prefix("Review comment on `")?;
        let (file, rest) = header.split_once("` ")?;
        let rest = rest.strip_suffix("):")?;
        let (range, scope) = rest.rsplit_once(" (")?;
        let open = lines
            .iter()
            .position(|line| is_diff_fence(line).is_some())
            .filter(|index| *index > 1)?;
        let fence = is_diff_fence(lines[open])?;
        let close = open
            + 1
            + lines[open + 1..]
                .iter()
                .position(|line| line.trim_end() == fence)?;
        Some((
            Self {
                file: file.to_string(),
                lines: range.to_string(),
                scope: scope.to_string(),
                excerpt: lines[open + 1..close]
                    .iter()
                    .map(|line| line.to_string())
                    .collect(),
                text: lines[1..open].join("\n").trim().to_string(),
            },
            close + 1,
        ))
    }
}

/// The fence of a "```diff" line, three backticks or more.
fn is_diff_fence(line: &str) -> Option<&str> {
    let fence = line.trim_end().strip_suffix("diff")?;
    (fence.len() >= 3 && fence.chars().all(|ch| ch == '`')).then_some(fence)
}

/// Whether a link destination is a review comment.
pub fn is_review_comment_href(href: &str) -> bool {
    href.trim().starts_with(REVIEW_COMMENT_SCHEME)
}

/// `draft` with `addition` after it, on its own line when the draft ends in text, so a second
/// comment (or annotation) joins the first instead of replacing it.
pub fn append_to_draft(draft: &str, addition: &str) -> String {
    let kept = draft.trim_end();
    if kept.is_empty() {
        return addition.to_string();
    }
    let separator = if is_review_comment_href_tail(kept) && addition.starts_with('[') {
        " "
    } else {
        "\n\n"
    };
    format!("{kept}{separator}{addition}")
}

/// Whether the draft ends with a review comment pill, so the next pill sits beside it.
fn is_review_comment_href_tail(draft: &str) -> bool {
    composer_references(draft).last().is_some_and(|reference| {
        reference.kind == ReferenceKind::Comment && reference.end == draft.encode_utf16().count()
    })
}

/// The text the agent receives: every review comment pill replaced by its block.
pub fn expand_review_comments(text: &str) -> String {
    let references: Vec<_> = composer_references(text)
        .into_iter()
        .filter(|reference| reference.kind == ReferenceKind::Comment)
        .collect();
    if references.is_empty() {
        return text.to_string();
    }
    let offsets = utf16_byte_offsets(text);
    let mut out = String::with_capacity(text.len() * 2);
    let mut cursor = 0;
    for reference in references {
        let (Some(start), Some(end)) = (offsets(reference.start), offsets(reference.end)) else {
            continue;
        };
        let Some(comment) = ReviewComment::from_href(&reference.path) else {
            continue;
        };
        let before = text[cursor..start].trim_end_matches([' ', '\t']);
        out.push_str(before);
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push_str(if out.ends_with('\n') { "\n" } else { "\n\n" });
        }
        out.push_str(&comment.block());
        cursor = end;
        let rest = text[cursor..].trim_start_matches([' ', '\t']);
        cursor = text.len() - rest.len();
    }
    let rest = &text[cursor..];
    if !rest.trim().is_empty() {
        out.push('\n');
        out.push_str(rest.trim_start_matches('\n'));
    }
    out.trim().to_string()
}

/// A sent message with every review comment block turned back into its pill, for the transcript.
pub fn collapse_review_comments(text: &str) -> String {
    if !text.contains("Review comment on `") {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut index = 0;
    while index < lines.len() {
        if let Some((comment, taken)) = ReviewComment::parse_block(&lines[index..]) {
            let pill = comment.reference();
            // Pills of consecutive comments share one line.
            let joins = out
                .last()
                .is_some_and(|last| last.ends_with(')') && last.contains(REVIEW_COMMENT_SCHEME))
                || (out.last().is_some_and(|last| last.trim().is_empty())
                    && out.len() >= 2
                    && out[out.len() - 2].contains(REVIEW_COMMENT_SCHEME));
            if joins {
                while out.last().is_some_and(|last| last.trim().is_empty()) {
                    out.pop();
                }
                if let Some(last) = out.last_mut() {
                    last.push(' ');
                    last.push_str(&pill);
                }
            } else {
                out.push(pill);
            }
            index += taken;
            continue;
        }
        out.push(lines[index].to_string());
        index += 1;
    }
    out.join("\n")
}

/// Maps a UTF-16 offset in `text` to its byte offset.
fn utf16_byte_offsets(text: &str) -> impl Fn(usize) -> Option<usize> + '_ {
    move |target| {
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            if units == target {
                return Some(byte);
            }
            units += character.len_utf16();
        }
        (units == target).then_some(text.len())
    }
}

/// Every byte outside `A-Za-z0-9-_.~` as `%XX`, so the payload never ends a Markdown link.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pill(file: &str, lines: &str, text: &str) -> String {
        ReviewComment {
            file: file.into(),
            lines: lines.into(),
            scope: "the branch's changes".into(),
            excerpt: vec!["+line".into()],
            text: text.into(),
        }
        .reference()
    }

    #[test]
    fn held_comments_all_survive_in_order() {
        let first = pill("README.md", "L7", "one");
        let second = pill("greet.py", "L18", "two");
        let third = pill("greet.py", "L20", "three");
        let mut held = String::new();
        for addition in [&first, &second, &third] {
            held = append_to_draft(&held, addition);
        }
        assert_eq!(held, format!("{first} {second} {third}"));
    }

    #[test]
    fn delivering_held_comments_keeps_typed_text() {
        let first = pill("README.md", "L7", "one");
        let second = pill("greet.py", "L18", "two");
        let held = append_to_draft(&first, &second);
        let draft = append_to_draft("Please look at these", &held);
        assert_eq!(draft, format!("Please look at these\n\n{first} {second}"));
        let later = pill("greet.py", "L20", "three");
        assert_eq!(
            append_to_draft(&draft, &later),
            format!("Please look at these\n\n{first} {second} {later}")
        );
    }

    #[test]
    fn appending_to_an_empty_draft_is_the_addition() {
        assert_eq!(append_to_draft("  \n", "note"), "note");
        assert_eq!(append_to_draft("text", "note"), "text\n\nnote");
    }
}
