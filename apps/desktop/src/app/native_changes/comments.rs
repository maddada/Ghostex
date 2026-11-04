//! Line comments: what a picked range carries, the text that goes into the session's composer, and
//! the Sent markers that stay on their lines until those lines change.

use super::model::{ChangedFile, ChangesData, ChangesScope, HunkLine, LineAnchor, LineKind};
use ghostex_gx_chat_core::composer::review_comments::ReviewComment;

/// The lines a comment is about, captured when the editor opens so a refresh under it cannot
/// move them.
#[derive(Clone, Debug)]
pub(crate) struct CommentRange {
    pub(crate) file: String,
    /// The comment's box follows this line.
    pub(crate) after: LineAnchor,
    /// Every picked line with the text it had, which a refresh compares to resolve the comment.
    pub(crate) lines: Vec<(LineAnchor, String)>,
    /// The picked lines as diff lines (`+`, `-` or space first).
    pub(crate) excerpt: Vec<String>,
    pub(crate) label: String,
}

impl CommentRange {
    pub(crate) fn from_lines(file: &ChangedFile, lines: &[&HunkLine]) -> Option<Self> {
        let last = lines.last()?;
        let after = LineAnchor::of(file, last)?;
        let anchors: Vec<(LineAnchor, String)> = lines
            .iter()
            .filter_map(|line| Some((LineAnchor::of(file, line)?, line.text.clone())))
            .collect();
        let excerpt = lines
            .iter()
            .map(|line| {
                let marker = match line.kind {
                    LineKind::Added => '+',
                    LineKind::Removed => '-',
                    LineKind::Context => ' ',
                };
                format!("{marker}{}", line.text)
            })
            .collect();
        Some(Self {
            file: file.path.clone(),
            label: range_label(lines),
            after,
            lines: anchors,
            excerpt,
        })
    }
}

/// `L186`, `L186 to L189`, with `(before)` when every line is a removed one.
fn range_label(lines: &[&HunkLine]) -> String {
    let all_removed = lines.iter().all(|line| line.kind == LineKind::Removed);
    let number = |line: &HunkLine| {
        if all_removed {
            line.old
        } else {
            line.new.or(line.old)
        }
    };
    let numbers: Vec<u32> = lines
        .iter()
        .filter(|line| all_removed || line.kind != LineKind::Removed)
        .filter_map(|line| number(line))
        .collect();
    let (Some(first), Some(last)) = (numbers.first(), numbers.last()) else {
        return "line".to_string();
    };
    let range = if first == last {
        format!("L{first}")
    } else {
        format!("L{first} to L{last}")
    };
    if all_removed {
        format!("{range} (before)")
    } else {
        range
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommentState {
    /// In the session's composer since this moment (milliseconds since the epoch).
    Sent { at_ms: u64 },
    /// Its lines changed (`auto`) or the user resolved it.
    Resolved { auto: bool },
}

#[derive(Clone, Debug)]
pub(crate) struct ChangesComment {
    pub(crate) id: u64,
    pub(crate) range: CommentRange,
    pub(crate) text: String,
    pub(crate) state: CommentState,
    /// Where it went: "Claude's chat", "Codex's terminal", "the clipboard".
    pub(crate) target: String,
}

impl ChangesComment {
    pub(crate) fn is_open(&self) -> bool {
        matches!(self.state, CommentState::Sent { .. })
    }
}

/// The comment as the chat core carries it: a pill in the chat box, the block the agent reads.
pub(crate) fn review_comment(
    range: &CommentRange,
    text: &str,
    scope: ChangesScope,
) -> ReviewComment {
    ReviewComment {
        file: range.file.clone(),
        lines: range.label.clone(),
        scope: match scope {
            ChangesScope::AgainstMain => "the branch's changes",
            ChangesScope::Uncommitted => "the uncommitted changes",
        }
        .to_string(),
        excerpt: range.excerpt.clone(),
        text: text.trim().to_string(),
    }
}

/// A Sent comment resolves itself once any of its lines is gone or reads differently.
pub(crate) fn auto_resolve(comments: &mut [ChangesComment], data: &ChangesData) {
    for comment in comments.iter_mut().filter(|comment| comment.is_open()) {
        let Some(file) = data
            .files
            .iter()
            .find(|file| file.path == comment.range.file)
        else {
            comment.state = CommentState::Resolved { auto: true };
            continue;
        };
        let unchanged = comment.range.lines.iter().all(|(anchor, text)| {
            let now = if anchor.old_side {
                file.old_line_text(anchor.line)
            } else {
                file.new_line_text(anchor.line)
            };
            now == Some(text.as_str())
        });
        if !unchanged {
            comment.state = CommentState::Resolved { auto: true };
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// "just now", "4m ago", "2h ago".
pub(crate) fn ago(at_ms: u64) -> String {
    let seconds = now_ms().saturating_sub(at_ms) / 1000;
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", seconds / 60),
        _ => format!("{}h ago", seconds / 3600),
    }
}
