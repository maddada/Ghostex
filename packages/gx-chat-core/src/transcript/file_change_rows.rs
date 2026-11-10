//! What a file-change card says before either renderer lays it out.
//!
//! Ported from `packages/shared/session-chat-presentation/file-change-rows.ts`. GPUI
//! (`native_chat/file_change_card.rs`, through the projection) reads these, so a card cannot count
//! or shorten differently from one platform to the next.

use serde_json::Value;

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

/// How many blocks a file row's change bar has.
pub const FILE_CHANGE_BAR_BLOCKS: usize = 5;

/// A file row's change bar, block by block: `add`, `del`, or `none` for an unfilled block. GitHub's
/// rule: a change of five lines or more fills every block, split by its share of added and removed
/// lines, and a smaller one fills one block per line.
pub fn file_change_bars(added: usize, removed: usize) -> Vec<&'static str> {
    let total = added + removed;
    let filled = total.min(FILE_CHANGE_BAR_BLOCKS);
    let mut green = if total == 0 {
        0
    } else {
        (filled * added + total / 2) / total
    };
    if added > 0 && green == 0 {
        green = 1;
    }
    if removed > 0 && green == filled && filled > 1 {
        green = filled - 1;
    }
    (0..FILE_CHANGE_BAR_BLOCKS)
        .map(|block| {
            if block < green {
                "add"
            } else if block < filled {
                "del"
            } else {
                "none"
            }
        })
        .collect()
}

/// How many of a diff's lines its preview shows: up to and including its seventh code line, with
/// the `@@` lines between them.
pub fn file_change_preview_end(lines: &[DiffLine]) -> usize {
    let mut code = 0;
    for (index, line) in lines.iter().enumerate() {
        if line.kind == DiffKind::Meta {
            continue;
        }
        code += 1;
        if code == FILE_CHANGE_PREVIEW_LINES {
            return index + 1;
        }
    }
    lines.len()
}

/// "Show all 41 changed lines": the button under a preview that has more to show.
pub fn file_change_show_all_label(changed: usize) -> String {
    format!(
        "Show all {changed} changed {}",
        if changed == 1 { "line" } else { "lines" }
    )
}

/// The folder every row's file sits in, written the way the rows write their own folders.
fn common_folder(files: &[Value]) -> String {
    let mut common: Option<Vec<&str>> = None;
    // Splitting drops an absolute folder's root, which read "Users/me/project" in the header.
    let rooted = files.iter().all(|file| {
        file.get("parent")
            .and_then(Value::as_str)
            .is_some_and(|parent| parent.starts_with('/'))
    });
    for file in files {
        let parent = file
            .get("parent")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let segments: Vec<&str> = parent
            .split(['/', '\\'])
            .filter(|segment| !segment.is_empty())
            .collect();
        common = Some(match common {
            None => segments,
            Some(known) => known
                .into_iter()
                .zip(segments)
                .take_while(|(left, right)| left == right)
                .map(|(left, _)| left)
                .collect(),
        });
    }
    let folder = common.unwrap_or_default().join("/");
    if rooted && !folder.is_empty() {
        format!("/{folder}")
    } else {
        folder
    }
}

/// CDXC:SessionChat 2026-10-11 DECISION:
/// User picked the proposed file change card from the chat-cards question page
/// (docs/2026-10-10/chat-cards-questions/04-file-changes.html): one card per message with a header
/// that sums up the edit (how many files, the folder they share, total +/-, a ratio bar and Review
/// all), then a row per file with an icon, the filename before its folder, the counts and a
/// five-block change bar. A row opens to a diff with the file's own line numbers and each hunk's
/// heading, with "Show all N changed lines" and Open file under the preview. Simple mode keeps its
/// "Edited N files" fold, which opens to this card (shown only while its turn's card is open since
/// 2026-11-08, `turn_files_fold`). It replaces the activity-rail stack of one card
/// per file. The rows had an Edited/New/Deleted tag until the user said "remove Edit/New etc next
/// to the file names please, no need"; the file icon's colour still tells them apart.
/// SEE-ALSO: apps/desktop/src/app/native_chat/file_change_card.rs,
/// apps/mobile/app/src/chat/native/transcript/FileChanges.tsx, server/src/session_chat_file_patch.rs.
pub fn file_change_card(files: &[Value]) -> Value {
    let mut paths: Vec<&str> = Vec::new();
    let (mut added, mut removed) = (0u64, 0u64);
    let (edited, failed) =
        crate::transcript::simple::edited_and_failed_paths(files.iter().filter_map(|file| {
            let path = file.get("path").and_then(Value::as_str)?;
            Some((path, file.get("failed") == Some(&Value::Bool(true))))
        }));
    for file in files {
        if let Some(path) = file.get("path").and_then(Value::as_str) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        added += file.get("added").and_then(Value::as_u64).unwrap_or(0);
        removed += file.get("removed").and_then(Value::as_u64).unwrap_or(0);
    }
    let total = added + removed;
    serde_json::json!({
        "title": crate::transcript::simple::edit_label(edited.len(), failed.len()),
        "folder": common_folder(files),
        "added": added,
        "removed": removed,
        // The ratio bar's green share; 0 for a card that changed no lines.
        "addShare": if total == 0 { 0.0 } else { added as f64 / total as f64 },
        "paths": paths,
    })
}

/// What a finished turn's files fold ("Edited N files", "N files changed") shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnFilesFold {
    /// No fold: the turn's card is closed, the turn changed no files, its tool cards already list
    /// every edit, or its rows are still on the server.
    Hidden,
    /// The fold, opening onto the turn's file change card.
    Files,
}

/// CDXC:SessionChat 2026-11-08 DECISION:
/// User, on seeing the files edited twice (the closed "Previous turn" card's "2 files" pill, then a
/// collapsed "Edited 2 files" fold under it): "Just remove \"Edit 2 files\" collapsed area below
/// the Previous turn card and instead make those file edits show collapsed right below the
/// working/previous turn card when it's expanded". A finished turn's card ("Worked for …",
/// "Previous turn") shows its files fold only while the card is open, right under the card and
/// above the work log, and the fold itself starts collapsed; a closed card leaves the files to its
/// "N files" pill. A file row still opens onto its diff as before. Supersedes the fold always
/// drawn under the card (the 2026-10-11 decision on `file_change_card` above).
///
/// CDXC:SessionChat 2026-11-10 DECISION:
/// User, of an opened turn showing "Edited 1 file" above its "Used 5 tools" card: "we don't need
/// \"Edited 1 file\" there anymore since file edits are under the used 5 tools thingy". Where every
/// row of the turn that wrote files draws a tool card (Tool call cards on, Agent work Default),
/// each edit is already a row of that card that opens onto its diff, so the turn has no files
/// fold. With Tool call cards off the rows hide their writes in a finished turn, so the fold stays
/// the one place they show. A turn whose rows are still on the server has no fold either: opening
/// the turn reads them (its notice says so in the log), and the fold comes with them if they draw
/// no tool cards. Supersedes the fold over unread work that read it on open (`Unread`).
/// SEE-ALSO: apps/desktop/src/app/native_chat/completed_work_row.rs,
/// apps/mobile/app/src/chat/native/transcript/TranscriptItemRow.tsx (`CompletedWorkRow`).
pub fn turn_files_fold(item: &crate::TranscriptItem, turn_open: bool) -> TurnFilesFold {
    let crate::TranscriptItem::CompletedWork {
        files,
        work,
        artifacts,
        final_message,
        ..
    } = item
    else {
        return TurnFilesFold::Hidden;
    };
    if !turn_open || files.is_empty() {
        return TurnFilesFold::Hidden;
    }
    // Every row whose writes the turn gathers: its work, the rows kept in view and the reply.
    let writers: Vec<&serde_json::Value> = work
        .iter()
        .chain(artifacts.iter())
        .map(|row| &**row)
        .chain(final_message.as_deref())
        .filter(|row| {
            row["files"]
                .as_array()
                .is_some_and(|files| !files.is_empty())
        })
        .collect();
    let listed = !writers.is_empty() && writers.iter().all(|row| row["toolCard"].is_object());
    if listed {
        TurnFilesFold::Hidden
    } else {
        TurnFilesFold::Files
    }
}
