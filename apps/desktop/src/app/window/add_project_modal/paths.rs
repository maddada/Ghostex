//! Path helpers of the Add Project dialog: the Rust twin of
//! packages/core-ui/remote-project-picker/remote-project-paths.ts (deleted 2026-10-01), `filterBrowseEntries` from
//! remote-command-palette-logic.ts (deleted 2026-10-01) and `isRepositoryCloneBranchNameInputValid` from
//! packages/shared/repository-clone.ts (deleted 2026-10-01). Separators are ASCII, so byte offsets stand in for the
//! UTF-16 offsets the TypeScript slices at.
use super::model::AddProjectBrowseEntry;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AbsolutePathKind {
    Unix,
    Windows,
}

fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\'))
}

fn is_unc_path(value: &str) -> bool {
    value.starts_with("\\\\")
}

fn is_windows_absolute_path(value: &str) -> bool {
    is_unc_path(value) || is_windows_drive_path(value)
}

pub(crate) fn is_explicit_relative_project_path(value: &str) -> bool {
    value == "."
        || value == ".."
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
}

fn is_root_path(value: &str) -> bool {
    if value == "/" || value == "\\" {
        return true;
    }
    let bytes = value.as_bytes();
    (bytes.len() == 2 || bytes.len() == 3)
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\'))
}

fn absolute_path_kind(value: &str) -> Option<AbsolutePathKind> {
    if is_windows_drive_path(value) || is_unc_path(value) {
        return Some(AbsolutePathKind::Windows);
    }
    value.starts_with('/').then_some(AbsolutePathKind::Unix)
}

fn trim_trailing_path_separators(value: &str) -> String {
    if value.is_empty() || is_root_path(value) {
        return value.to_string();
    }
    let trimmed = if absolute_path_kind(value) == Some(AbsolutePathKind::Unix) {
        value.trim_end_matches('/')
    } else {
        value.trim_end_matches(['/', '\\'])
    };
    if trimmed.is_empty() {
        return value.to_string();
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return format!("{trimmed}\\");
    }
    trimmed.to_string()
}

fn preferred_path_separator(value: &str) -> char {
    match absolute_path_kind(value) {
        Some(AbsolutePathKind::Windows) => '\\',
        Some(AbsolutePathKind::Unix) => '/',
        None if value.contains('\\') => '\\',
        None => '/',
    }
}

pub(crate) fn has_trailing_path_separator(value: &str) -> bool {
    if absolute_path_kind(value) == Some(AbsolutePathKind::Unix) {
        value.ends_with('/')
    } else {
        value.ends_with('/') || value.ends_with('\\')
    }
}

fn split_path_segments(value: &str, separator: char) -> Vec<&str> {
    if separator == '/' {
        value.split('/').filter(|part| !part.is_empty()).collect()
    } else {
        value
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .collect()
    }
}

fn last_path_separator_index(value: &str) -> Option<usize> {
    if absolute_path_kind(value) == Some(AbsolutePathKind::Unix) {
        return value.rfind('/');
    }
    value.rfind(['/', '\\'])
}

struct AbsolutePath<'a> {
    root: String,
    separator: char,
    segments: Vec<&'a str>,
}

fn split_absolute_path(value: &str) -> Option<AbsolutePath<'_>> {
    if is_windows_drive_path(value) {
        let root = format!("{}\\", &value[..2]);
        let rest = value.get(root.len()..).unwrap_or("");
        return Some(AbsolutePath {
            segments: split_path_segments(rest, '\\'),
            root,
            separator: '\\',
        });
    }
    if is_unc_path(value) {
        let segments = split_path_segments(value, '\\');
        if segments.len() < 2 {
            return None;
        }
        return Some(AbsolutePath {
            root: format!("\\\\{}\\{}\\", segments[0], segments[1]),
            separator: '\\',
            segments: segments[2..].to_vec(),
        });
    }
    if value.starts_with('/') {
        return Some(AbsolutePath {
            root: "/".to_string(),
            separator: '/',
            segments: split_path_segments(&value[1..], '/'),
        });
    }
    None
}

/// `isFilesystemBrowseQuery`: Windows drive and UNC paths count only on a `Win*` platform.
pub(crate) fn is_filesystem_browse_query(value: &str, platform: &str) -> bool {
    let allow_windows_paths = platform.starts_with("Win");
    value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
        || value.starts_with('/')
        || value.starts_with("~/")
        || (allow_windows_paths && is_windows_absolute_path(value))
}

pub(crate) fn is_unsupported_windows_project_path(value: &str, platform: &str) -> bool {
    is_windows_absolute_path(value) && !platform.starts_with("Win")
}

pub(crate) fn normalize_project_path_for_dispatch(value: &str) -> String {
    trim_trailing_path_separators(value.trim())
}

pub(crate) fn resolve_project_path_for_dispatch(value: &str, cwd: Option<&str>) -> String {
    let trimmed = value.trim();
    let Some(cwd) = cwd.filter(|_| is_explicit_relative_project_path(trimmed)) else {
        return normalize_project_path_for_dispatch(trimmed);
    };
    let normalized_cwd = normalize_project_path_for_dispatch(cwd);
    let Some(base) = split_absolute_path(&normalized_cwd) else {
        return normalize_project_path_for_dispatch(trimmed);
    };
    let mut segments: Vec<&str> = base.segments.clone();
    for segment in trimmed.split(['/', '\\']) {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            segments.pop();
            continue;
        }
        segments.push(segment);
    }
    let joined = segments.join(&base.separator.to_string());
    if joined.is_empty() {
        return normalize_project_path_for_dispatch(&base.root);
    }
    normalize_project_path_for_dispatch(&format!("{}{joined}", base.root))
}

pub(crate) fn get_browse_leaf_path_segment(current: &str) -> String {
    match last_path_separator_index(current) {
        Some(index) => current[index + 1..].to_string(),
        None => current.to_string(),
    }
}

pub(crate) fn get_browse_directory_path(current: &str) -> String {
    if has_trailing_path_separator(current) {
        return current.to_string();
    }
    match last_path_separator_index(current) {
        Some(index) => current[..index + 1].to_string(),
        None => current.to_string(),
    }
}

pub(crate) fn ensure_browse_directory_path(current: &str) -> String {
    let trimmed = current.trim();
    if trimmed.is_empty() || has_trailing_path_separator(trimmed) {
        return trimmed.to_string();
    }
    format!("{trimmed}{}", preferred_path_separator(trimmed))
}

pub(crate) fn get_browse_parent_path(current: &str) -> Option<String> {
    let directory = get_browse_directory_path(current);
    let trimmed = trim_trailing_path_separators(&directory);
    if trimmed.is_empty() || is_root_path(&trimmed) {
        return None;
    }
    let index = last_path_separator_index(&trimmed)?;
    Some(trimmed[..index + 1].to_string())
}

/// `/^[a-z]:[/\\]$/iu`: the browse directory is a Windows drive root.
pub(crate) fn is_windows_drive_root(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

pub(crate) struct FilteredBrowseEntries {
    pub(crate) exact: Option<AddProjectBrowseEntry>,
    pub(crate) filtered: Vec<AddProjectBrowseEntry>,
    pub(crate) highlighted: Option<AddProjectBrowseEntry>,
}

/// `filterBrowseEntries`: a case-insensitive prefix filter that hides dot-folders unless the
/// filter itself starts with a dot.
pub(crate) fn filter_browse_entries(
    entries: &[AddProjectBrowseEntry],
    filter: &str,
    highlighted_value: Option<&str>,
) -> FilteredBrowseEntries {
    let lower_filter = filter.to_lowercase();
    let show_hidden = filter.starts_with('.');
    let filtered: Vec<AddProjectBrowseEntry> = entries
        .iter()
        .filter(|entry| {
            entry.name.to_lowercase().starts_with(&lower_filter)
                && (show_hidden || !entry.name.starts_with('.'))
        })
        .cloned()
        .collect();
    let highlighted = highlighted_value
        .and_then(|value| value.strip_prefix("browse:"))
        .and_then(|path| filtered.iter().find(|entry| entry.full_path == path))
        .cloned();
    let exact = (!filter.is_empty())
        .then(|| filtered.iter().find(|entry| entry.name == filter).cloned())
        .flatten();
    FilteredBrowseEntries {
        exact,
        filtered,
        highlighted,
    }
}

const MAX_REPOSITORY_BRANCH_NAME_LENGTH: usize = 255;

/// `isRepositoryCloneBranchNameInputValid`: empty means "the default branch" and is valid.
pub(crate) fn is_repository_clone_branch_name_input_valid(input: &str) -> bool {
    let branch = input.trim();
    if branch.is_empty() {
        return true;
    }
    let forbidden = |character: char| {
        character.is_whitespace()
            || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
            || (character as u32) < 0x20
            || character as u32 == 0x7f
    };
    if branch.encode_utf16().count() > MAX_REPOSITORY_BRANCH_NAME_LENGTH
        || branch == "@"
        || branch.starts_with('-')
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.ends_with('.')
        || branch.contains("..")
        || branch.contains("@{")
        || branch.chars().any(forbidden)
    {
        return false;
    }
    branch.split('/').all(|segment| {
        !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
    })
}
