//! What a file-change card says before either renderer lays it out.
//!
//! Ported from `packages/shared/session-chat-presentation/file-change-rows.ts`. GPUI
//! (`native_chat/file_change_card.rs`, through the projection) reads these, so a card cannot count
//! or shorten differently from one platform to the next.

use crate::transcript::diff::{DiffKind, DiffLine};
use crate::transcript::jsstr::utf16_len;

/// How many code lines a card shows while Settings > Chat has previews on.
pub const FILE_CHANGE_PREVIEW_LINES: usize = 7;

/// GPUI has no start-ellipsis, so the folder half of a long path is shortened here instead. React
/// kept its CSS `direction: rtl` truncation, which is width-aware; this is the character budget the
/// native card falls back to.
pub const FILE_CHANGE_PARENT_BUDGET: usize = 52;

fn is_windows_root(directory: &str) -> bool {
    let bytes = directory.as_bytes();
    (bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && matches!(bytes.get(2), Some(b'\\' | b'/')))
        || directory.starts_with("\\\\")
}

fn normalize(value: &str, windows: bool) -> String {
    let swapped = if windows {
        value.replace('\\', "/")
    } else {
        value.to_string()
    };
    swapped.trim_end_matches('/').to_string()
}

fn relative_to(normalized_path: &str, root: &str, windows: bool) -> Option<String> {
    let comparable = |value: &str| {
        if windows {
            value.to_lowercase()
        } else {
            value.to_string()
        }
    };
    // The comparison folds case but the SLICE must not use the folded length: Rust's
    // `to_lowercase` is full Unicode and changes the byte length of several characters (U+212A
    // folds 3 bytes to 1), so `root.len() + 1` could land past the end of the path or inside a
    // character. The prefix is taken from the original string instead, and only after the folded
    // test agrees.
    if comparable(normalized_path).starts_with(&format!("{}/", comparable(root))) {
        if let Some(rest) = normalized_path.get(root.len() + 1..) {
            return Some(rest.to_string());
        }
        // A case fold that changed the byte length: fall back to the longest prefix of
        // `normalized_path` whose fold equals the root's, which is what the test just proved
        // exists.
        let folded_root = comparable(root);
        for (offset, _) in normalized_path
            .char_indices()
            .chain(std::iter::once((normalized_path.len(), ' ')))
        {
            if comparable(&normalized_path[..offset]) == folded_root {
                return normalized_path
                    .get(offset + 1..)
                    .map(std::string::ToString::to_string);
            }
        }
    }
    None
}

/// `^(?:\/Users\/[^/]+|\/home\/[^/]+|\/root)(?=\/|$)`, and its Windows counterpart.
fn home_prefix(directory: &str, windows: bool) -> Option<&str> {
    let with_user = |prefix: &str| -> Option<&str> {
        let rest = directory.strip_prefix(prefix)?;
        let user = rest.split('/').next()?;
        if user.is_empty() {
            return None;
        }
        Some(&directory[..prefix.len() + user.len()])
    };
    if let Some(found) = with_user("/Users/").or_else(|| with_user("/home/")) {
        return Some(found);
    }
    if directory == "/root" || directory.starts_with("/root/") {
        return Some("/root");
    }
    if !windows {
        return None;
    }
    let bytes = directory.as_bytes();
    if bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && directory.len() > 9
        && directory[2..9].eq_ignore_ascii_case("/Users/")
    {
        let user = directory[9..].split('/').next()?;
        if !user.is_empty() {
            return Some(&directory[..9 + user.len()]);
        }
    }
    None
}

/// CDXC:SessionChat 2026-09-13 DECISION:
/// User: diff paths start at the current folder; files outside the project keep their full path,
/// with the home directory shortened to ~/ where possible.
pub fn file_change_display_path(path: &str, working_directory: Option<&str>) -> String {
    let Some(working_directory) = working_directory.filter(|value| !value.is_empty()) else {
        return path.to_string();
    };
    let windows = is_windows_root(working_directory);
    let directory = normalize(working_directory, windows);
    let normalized_path = normalize(path, windows);
    if let Some(relative) = relative_to(&normalized_path, &directory, windows) {
        return relative;
    }
    if let Some(home) = home_prefix(&directory, windows) {
        if let Some(relative) = relative_to(&normalized_path, home, windows) {
            return format!("~/{relative}");
        }
    }
    path.to_string()
}

/// The added, removed and code-line counts beside a card's path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileChangeCounts {
    pub added: usize,
    pub removed: usize,
    /// Diff lines that are not `@@` hunk markers; the preview and fold count these.
    pub code: usize,
}

pub fn file_change_counts(lines: &[DiffLine]) -> FileChangeCounts {
    let mut counts = FileChangeCounts::default();
    for line in lines {
        if line.kind == DiffKind::Meta {
            continue;
        }
        counts.code += 1;
        match line.kind {
            DiffKind::Add => counts.added += 1,
            DiffKind::Del => counts.removed += 1,
            _ => {}
        }
    }
    counts
}

/// A card with previews on already shows everything it has, so it only opens when there is more
/// than the preview or when the write itself failed.
pub fn file_change_expandable(
    counts: FileChangeCounts,
    preview_enabled: bool,
    failed: bool,
) -> bool {
    !preview_enabled || counts.code > FILE_CHANGE_PREVIEW_LINES || failed
}

/// The visible path, split into the folder half and the file name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileChangePathParts {
    pub display_path: String,
    /// Everything up to and including the last separator, already shortened.
    pub parent: String,
    pub filename: String,
}

/// `budget` of `None` is the TypeScript's `Number.POSITIVE_INFINITY`, which the native card passes
/// because the renderer shortens the folder half against the row's real width.
pub fn file_change_path_parts(
    path: &str,
    working_directory: Option<&str>,
    budget: Option<usize>,
) -> FileChangePathParts {
    let display_path = file_change_display_path(path, working_directory);
    let last = match display_path.rfind(['/', '\\']) {
        Some(index) => &display_path[index + 1..],
        None => display_path.as_str(),
    };
    let filename = if last.is_empty() {
        display_path.clone()
    } else {
        last.to_string()
    };
    let parent = display_path[..display_path.len().saturating_sub(filename.len())].to_string();
    let parent = match budget {
        Some(budget) if utf16_len(&parent) > budget => {
            let keep = utf16_len(&parent) - (budget - 1);
            let mut kept = parent.as_str();
            let mut dropped = 0;
            while dropped < keep {
                let character = kept
                    .chars()
                    .next()
                    .expect("a shortened parent is non-empty");
                dropped += character.len_utf16();
                kept = &kept[character.len_utf8()..];
            }
            format!("\u{2026}{kept}")
        }
        _ => parent,
    };
    FileChangePathParts {
        display_path,
        parent,
        filename,
    }
}
