//! The Changes view's data: gxserver's patch split into files, hunks and lines with their file
//! line numbers, the word-level spans that changed inside a replaced line, and the flat rows the
//! list draws.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChangesScope {
    /// Everything the branch changed, committed or not, against `merge-base(main, HEAD)`.
    #[default]
    AgainstMain,
    /// Only what is not committed yet, against HEAD.
    Uncommitted,
}

impl ChangesScope {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::AgainstMain => "main",
            Self::Uncommitted => "uncommitted",
        }
    }

    /// The scope menu's name for it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AgainstMain => "Whole branch",
            Self::Uncommitted => "Uncommitted only",
        }
    }

    /// The scope menu's line under the name; `base` is the branch it compares against.
    pub(crate) fn detail(self, base: &str) -> String {
        match self {
            Self::AgainstMain => {
                format!("Everything this branch changed against {base}, committed or not")
            }
            Self::Uncommitted => "Only what is not committed yet".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileStatus {
    Added,
    Deleted,
    Modified,
}

impl FileStatus {
    pub(crate) fn letter(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Modified => "M",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug)]
pub(crate) struct HunkLine {
    pub(crate) kind: LineKind,
    pub(crate) old: Option<u32>,
    pub(crate) new: Option<u32>,
    /// The line without its `+`, `-` or space marker.
    pub(crate) text: String,
    /// Byte ranges of `text` that differ from the line it replaced (or was replaced by).
    pub(crate) changed: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Hunk {
    /// The `@@ … @@` line, and after it the function git names for the hunk.
    pub(crate) header: String,
    pub(crate) lines: Vec<HunkLine>,
    /// The hunk exactly as gxserver sent it (header and lines), which `changesRevert` matches.
    pub(crate) raw: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ChangedFile {
    pub(crate) path: String,
    pub(crate) status: FileStatus,
    pub(crate) binary: bool,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
    pub(crate) hunks: Vec<Hunk>,
    /// Part of the file is not committed yet, so a commit can take it.
    pub(crate) uncommitted: bool,
}

impl ChangedFile {
    pub(crate) fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub(crate) fn folder(&self) -> &str {
        match self.path.rfind('/') {
            Some(index) => &self.path[..index + 1],
            None => "",
        }
    }

    /// The new-side text of line `number`, when the diff shows it.
    pub(crate) fn new_line_text(&self, number: u32) -> Option<&str> {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .find_map(|line| {
                (line.new == Some(number) && line.kind != LineKind::Removed)
                    .then_some(line.text.as_str())
            })
    }

    pub(crate) fn old_line_text(&self, number: u32) -> Option<&str> {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .find_map(|line| {
                (line.old == Some(number) && line.kind == LineKind::Removed)
                    .then_some(line.text.as_str())
            })
    }
}

/// What `changesDiff` answered, parsed.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChangesData {
    pub(crate) files: Vec<ChangedFile>,
    pub(crate) base_label: String,
    pub(crate) branch: Option<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) on_default_branch: bool,
    pub(crate) truncated: bool,
    pub(crate) uncommitted_paths: Vec<String>,
    /// The patch text, to tell a refresh that changed nothing from one that did.
    pub(crate) fingerprint: u64,
}

impl ChangesData {
    pub(crate) fn from_answer(answer: &serde_json::Value) -> Self {
        let text = |key: &str| answer.get(key).and_then(serde_json::Value::as_str);
        let list = |key: &str| -> Vec<String> {
            answer
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let patch = text("patch").unwrap_or_default();
        let uncommitted_paths = list("uncommittedPaths");
        let uncommitted: HashSet<&str> = uncommitted_paths.iter().map(String::as_str).collect();
        let mut files = parse_patch(patch);
        for file in &mut files {
            file.uncommitted = uncommitted.contains(file.path.as_str());
        }
        for path in list("tooLargePaths") {
            files.push(ChangedFile {
                uncommitted: true,
                path,
                status: FileStatus::Added,
                binary: true,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
        }
        Self {
            files,
            base_label: text("baseLabel").unwrap_or("HEAD").to_string(),
            branch: text("branch").map(str::to_string),
            default_branch: text("defaultBranch").map(str::to_string),
            on_default_branch: answer
                .get("onDefaultBranch")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
            truncated: answer.get("truncated").and_then(serde_json::Value::as_bool) == Some(true),
            uncommitted_paths,
            fingerprint: fingerprint(patch),
        }
    }

    pub(crate) fn additions(&self) -> usize {
        self.files.iter().map(|file| file.additions).sum()
    }

    pub(crate) fn deletions(&self) -> usize {
        self.files.iter().map(|file| file.deletions).sum()
    }
}

fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// `@@ -a,b +c,d @@`: the two start lines.
fn hunk_starts(header: &str) -> (u32, u32) {
    let mut parts = header.split_whitespace().skip(1);
    let number = |part: Option<&str>, sign: char| {
        part.and_then(|part| part.strip_prefix(sign))
            .and_then(|part| part.split(',').next())
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let old = number(parts.next(), '-');
    let new = number(parts.next(), '+');
    (old, new)
}

/// `diff --git a/x b/x`: the path after ` b/`.
fn path_from_diff_line(line: &str) -> String {
    let rest = line.strip_prefix("diff --git ").unwrap_or(line);
    match rest.find(" b/") {
        Some(index) => rest[index + 3..].to_string(),
        None => rest.trim_start_matches("a/").to_string(),
    }
}

pub(crate) fn parse_patch(patch: &str) -> Vec<ChangedFile> {
    let mut files: Vec<ChangedFile> = Vec::new();
    let mut old_line = 0u32;
    let mut new_line = 0u32;
    for raw in patch.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with("diff --git ") {
            files.push(ChangedFile {
                path: path_from_diff_line(line),
                status: FileStatus::Modified,
                binary: false,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
                uncommitted: false,
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if line.starts_with("@@") {
            (old_line, new_line) = hunk_starts(line);
            file.hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
                raw: raw.to_string(),
            });
            continue;
        }
        let Some(hunk) = file.hunks.last_mut() else {
            if line.starts_with("new file mode") {
                file.status = FileStatus::Added;
            } else if line.starts_with("deleted file mode") {
                file.status = FileStatus::Deleted;
            } else if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
                file.binary = true;
            } else if let Some(path) = line.strip_prefix("+++ b/") {
                file.path = path.to_string();
            }
            continue;
        };
        hunk.raw.push_str(raw);
        let (kind, text) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            // `\ No newline at end of file` and stray lines carry no file line.
            _ => continue,
        };
        let (old, new) = match kind {
            LineKind::Context => {
                old_line += 1;
                new_line += 1;
                (Some(old_line - 1), Some(new_line - 1))
            }
            LineKind::Added => {
                new_line += 1;
                file.additions += 1;
                (None, Some(new_line - 1))
            }
            LineKind::Removed => {
                old_line += 1;
                file.deletions += 1;
                (Some(old_line - 1), None)
            }
        };
        hunk.lines.push(HunkLine {
            kind,
            old,
            new,
            text: text.to_string(),
            changed: Vec::new(),
        });
    }
    for file in &mut files {
        for hunk in &mut file.hunks {
            mark_word_changes(&mut hunk.lines);
        }
    }
    files
}

/// Pairs each run of removed lines with the added lines after it and marks, in each pair, the
/// middle part that differs (the common start and end stay plain). A line with nothing in common
/// with its partner is left unmarked: the whole row's tint already says it changed.
fn mark_word_changes(lines: &mut [HunkLine]) {
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind != LineKind::Removed {
            index += 1;
            continue;
        }
        let removed_start = index;
        while index < lines.len() && lines[index].kind == LineKind::Removed {
            index += 1;
        }
        let added_start = index;
        while index < lines.len() && lines[index].kind == LineKind::Added {
            index += 1;
        }
        let pairs = (added_start - removed_start).min(index - added_start);
        for offset in 0..pairs {
            let (old, new) = (removed_start + offset, added_start + offset);
            let (old_range, new_range) = differing_middle(&lines[old].text, &lines[new].text);
            let keeps_something =
                old_range.len() < lines[old].text.len() || new_range.len() < lines[new].text.len();
            if keeps_something {
                if !old_range.is_empty() {
                    lines[old].changed.push(old_range);
                }
                if !new_range.is_empty() {
                    lines[new].changed.push(new_range);
                }
            }
        }
    }
}

fn differing_middle(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map(|((index, _), _)| index)
        .unwrap_or(old.len().min(new.len()));
    let prefix = (0..=prefix)
        .rev()
        .find(|&i| old.is_char_boundary(i) && new.is_char_boundary(i))
        .unwrap_or(0);
    let old_rest = &old[prefix..];
    let new_rest = &new[prefix..];
    let suffix = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    let suffix = suffix.min(old_rest.len()).min(new_rest.len());
    (prefix..old.len() - suffix, prefix..new.len() - suffix)
}

/// One drawn row of the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    File(usize),
    Hunk(usize, usize),
    /// A unified row: one hunk line.
    Line(usize, usize, usize),
    /// A split row: the removed or context line on the left and the added one on the right.
    Split(usize, usize, Option<usize>, Option<usize>),
    Binary(usize),
    /// A comment's box or marker, after the row it belongs to.
    Comment(u64),
    /// The open comment editor.
    Editor,
}

/// Which line a comment or a selection is on: the file, the side and the file line number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LineAnchor {
    pub(crate) file: String,
    /// True for a removed line (numbered in the old file).
    pub(crate) old_side: bool,
    pub(crate) line: u32,
}

impl LineAnchor {
    pub(crate) fn of(file: &ChangedFile, line: &HunkLine) -> Option<Self> {
        match line.kind {
            LineKind::Removed => Some(Self {
                file: file.path.clone(),
                old_side: true,
                line: line.old?,
            }),
            _ => Some(Self {
                file: file.path.clone(),
                old_side: false,
                line: line.new?,
            }),
        }
    }
}

/// Builds the rows for the files that are open. `after` names the rows a comment or the editor
/// follows: the anchor of a comment's last line.
pub(crate) fn build_rows(
    data: &ChangesData,
    split: bool,
    collapsed: &HashSet<String>,
    comments_after: &HashMap<LineAnchor, Vec<u64>>,
    editor_after: Option<&LineAnchor>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let follow = |rows: &mut Vec<Row>, anchor: Option<LineAnchor>| {
        let Some(anchor) = anchor else {
            return;
        };
        if let Some(ids) = comments_after.get(&anchor) {
            rows.extend(ids.iter().map(|id| Row::Comment(*id)));
        }
        if editor_after == Some(&anchor) {
            rows.push(Row::Editor);
        }
    };
    for (file_index, file) in data.files.iter().enumerate() {
        rows.push(Row::File(file_index));
        if collapsed.contains(&file.path) {
            continue;
        }
        if file.binary || file.hunks.is_empty() {
            rows.push(Row::Binary(file_index));
            continue;
        }
        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            rows.push(Row::Hunk(file_index, hunk_index));
            if !split {
                for (line_index, line) in hunk.lines.iter().enumerate() {
                    rows.push(Row::Line(file_index, hunk_index, line_index));
                    follow(&mut rows, LineAnchor::of(file, line));
                }
                continue;
            }
            let lines = &hunk.lines;
            let mut index = 0;
            while index < lines.len() {
                if lines[index].kind == LineKind::Context {
                    rows.push(Row::Split(file_index, hunk_index, Some(index), Some(index)));
                    follow(&mut rows, LineAnchor::of(file, &lines[index]));
                    index += 1;
                    continue;
                }
                let removed_start = index;
                while index < lines.len() && lines[index].kind == LineKind::Removed {
                    index += 1;
                }
                let added_start = index;
                while index < lines.len() && lines[index].kind == LineKind::Added {
                    index += 1;
                }
                let removed = added_start - removed_start;
                let added = index - added_start;
                for offset in 0..removed.max(added) {
                    let left = (offset < removed).then_some(removed_start + offset);
                    let right = (offset < added).then_some(added_start + offset);
                    rows.push(Row::Split(file_index, hunk_index, left, right));
                    for side in [left, right].into_iter().flatten() {
                        follow(&mut rows, LineAnchor::of(file, &lines[side]));
                    }
                }
            }
        }
    }
    rows
}

/// Each file's path and its lines as `(marker, old number, new number, text)`, for tests outside
/// this module that check what the rows would draw.
#[cfg(test)]
pub(crate) fn parse_patch_for_tests(
    patch: &str,
) -> Vec<(String, Vec<(char, Option<u32>, Option<u32>, String)>)> {
    parse_patch(patch)
        .into_iter()
        .map(|file| {
            let lines = file
                .hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .map(|line| {
                    let marker = match line.kind {
                        LineKind::Added => '+',
                        LineKind::Removed => '-',
                        LineKind::Context => ' ',
                    };
                    (marker, line.old, line.new, line.text.clone())
                })
                .collect();
            (file.path, lines)
        })
        .collect()
}

#[cfg(test)]
mod scope_tests {
    use super::ChangesScope;

    #[test]
    fn the_scope_menu_names_both_scopes_and_the_base() {
        assert_eq!(ChangesScope::AgainstMain.label(), "Whole branch");
        assert_eq!(ChangesScope::Uncommitted.label(), "Uncommitted only");
        assert_eq!(
            ChangesScope::AgainstMain.detail("develop"),
            "Everything this branch changed against develop, committed or not"
        );
        assert_eq!(
            ChangesScope::Uncommitted.detail("develop"),
            "Only what is not committed yet"
        );
    }
}
