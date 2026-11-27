//! Finding the repos a folder covers, from the disk alone: no git runs, so it is cheap enough for
//! the background pass to repeat every minute and for a request to ask once.
//!
//! A folder that is a repo covers itself and its checked-out submodules. A folder that is not
//! one covers the repos one level down, plus one more level inside plain folders (`packages/ui`).
//! Either way a `.code-workspace` file in the folder adds the repos it lists, even beside the
//! folder (`../billing`). Linked worktrees of other repos (`api-ab12cd34`, made by today's
//! single-repo worktrees) are not repos of their own and are skipped, except inside a session
//! folder, whose record says what it holds.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{join_rel, record, relative_path, RepoEntry, RepoKind, RepoSet, SessionFolder};

/// The most repos one project covers.
pub const MAX_REPOS: usize = 64;

/// Folders never looked into for repos: dependencies and build output.
const SKIPPED_FOLDERS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    "Pods",
    "DerivedData",
    "coverage",
];

/// What `.git` in `dir` says: a repo with its own history, a linked worktree of another repo,
/// or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitMark {
    None,
    OwnHistory,
    LinkedWorktree,
}

pub(crate) fn git_mark(dir: &Path) -> GitMark {
    let dot_git = dir.join(".git");
    let Ok(meta) = fs::symlink_metadata(&dot_git) else {
        return GitMark::None;
    };
    if meta.is_dir() {
        return GitMark::OwnHistory;
    }
    // A `.git` file points at the real git folder: a submodule's (`.git/modules/x`) is its own
    // history, a linked worktree's (`.git/worktrees/x`) belongs to another repo.
    match fs::read_to_string(&dot_git) {
        Ok(text) => {
            let target = text
                .trim()
                .trim_start_matches("gitdir:")
                .trim()
                .replace('\\', "/");
            if target.contains("/worktrees/") {
                GitMark::LinkedWorktree
            } else {
                GitMark::OwnHistory
            }
        }
        Err(_) => GitMark::None,
    }
}

/// The git folder a `.git` file in `dir` points at, when it is a file.
pub(crate) fn linked_git_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    if !fs::symlink_metadata(&dot_git).ok()?.is_file() {
        return None;
    }
    let text = fs::read_to_string(&dot_git).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let target = Path::new(target);
    Some(if target.is_absolute() {
        target.to_path_buf()
    } else {
        dir.join(target)
    })
}

/// Whether some folder above `dir` (not `dir` itself) is a repo: `dir` is then a subfolder of
/// one (a monorepo package), which stays a plain project.
fn inside_repo(dir: &Path) -> bool {
    dir.ancestors()
        .skip(1)
        .take(32)
        .any(|ancestor| fs::symlink_metadata(ancestor.join(".git")).is_ok())
}

fn visible_subfolders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            !name.starts_with('.')
                && !SKIPPED_FOLDERS.contains(&name.as_str())
                // Symbolic links are not followed, so a link cannot loop or reach outside.
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
        })
        .map(|entry| entry.path())
        .collect();
    folders.sort_by(|left, right| natural_cmp(&left.to_string_lossy(), &right.to_string_lossy()));
    folders
}

/// Orders `repo2` before `repo10`.
fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let key = |text: &str| -> Vec<(String, u64)> {
        let mut parts = Vec::new();
        let mut word = String::new();
        let mut number = String::new();
        for ch in text.to_lowercase().chars() {
            if ch.is_ascii_digit() {
                number.push(ch);
            } else {
                if !number.is_empty() {
                    parts.push((std::mem::take(&mut word), number.parse().unwrap_or(0)));
                    number.clear();
                }
                word.push(ch);
            }
        }
        parts.push((word, number.parse().unwrap_or(0)));
        parts
    };
    key(left).cmp(&key(right))
}

/// The submodule paths a `.gitmodules` file declares (`path = ...` in `[submodule "x"]`
/// sections), `/`-separated.
pub(crate) fn gitmodules_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_submodule = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_submodule = line[1..]
                .trim_start()
                .to_lowercase()
                .starts_with("submodule");
            continue;
        }
        if !in_submodule {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("path") {
            let value = value.trim().trim_matches('"').replace('\\', "/");
            let value = value.trim_end_matches('/').to_string();
            if !value.is_empty() && !value.split('/').any(|part| part == "..") {
                paths.push(value);
            }
        }
    }
    paths
}

/// A `.code-workspace` file's text without comments and trailing commas (it is JSON with
/// comments), parsed.
pub(crate) fn parse_code_workspace(text: &str) -> Option<Value> {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    let mut in_string = false;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            out.push(ch);
            if ch == '\\' && index + 1 < chars.len() {
                out.push(chars[index + 1]);
                index += 2;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            index += 1;
        } else if ch == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
        } else {
            out.push(ch);
            index += 1;
        }
    }
    // Trailing commas before `}` or `]`.
    let mut cleaned = String::with_capacity(out.len());
    let bytes: Vec<char> = out.chars().collect();
    let mut in_string = false;
    for (index, ch) in bytes.iter().enumerate() {
        if *ch == '"' && (index == 0 || bytes[index - 1] != '\\') {
            in_string = !in_string;
        }
        if !in_string && *ch == ',' {
            let next = bytes[index + 1..].iter().find(|next| !next.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        cleaned.push(*ch);
    }
    serde_json::from_str(&cleaned).ok()
}

/// The first `.code-workspace` file in `dir`, by name.
pub(crate) fn code_workspace_file(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("code-workspace")
                && path.is_file()
        })
        .collect();
    files.sort();
    files.into_iter().next()
}

/// The folders a `.code-workspace` file lists, resolved against `dir` (where the file is).
fn code_workspace_folders(dir: &Path, file: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(file) else {
        return Vec::new();
    };
    let Some(document) = parse_code_workspace(&text) else {
        return Vec::new();
    };
    document
        .get("folders")
        .and_then(Value::as_array)
        .map(|folders| {
            folders
                .iter()
                .filter_map(|folder| folder.get("path").and_then(Value::as_str))
                // Only the end is trimmed: a folder name may start with a space.
                .map(|path| path.trim_end())
                .filter(|path| !path.is_empty())
                .map(|path| {
                    let expanded = if let Some(rest) = path.strip_prefix("~/") {
                        dirs_home().map(|home| home.join(rest))
                    } else {
                        None
                    };
                    expanded.unwrap_or_else(|| {
                        let candidate = Path::new(path);
                        if candidate.is_absolute() {
                            candidate.to_path_buf()
                        } else {
                            join_rel(dir, path)
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn display_name(root: &Path, path: &Path, rel: &str) -> String {
    if rel.is_empty() {
        return root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());
    }
    if rel.starts_with("..") {
        return path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| rel.to_string());
    }
    rel.to_string()
}

/// The repos `folder` covers, found on disk. `None` when `folder` is not a folder or sits inside
/// a repo without being its root.
pub fn discover(folder: &Path) -> Option<RepoSet> {
    if !folder.is_dir() {
        return None;
    }
    if let Some((session_root, session_record)) = record::find(folder) {
        return Some(from_record(folder, &session_root, session_record));
    }
    let mark = git_mark(folder);
    let root_is_repo = mark != GitMark::None;
    if !root_is_repo && inside_repo(folder) {
        return None;
    }
    let mut repos: Vec<RepoEntry> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut push = |repos: &mut Vec<RepoEntry>, path: PathBuf, kind: RepoKind| {
        if repos.len() >= MAX_REPOS {
            return;
        }
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            return;
        }
        let Some(rel) = relative_path(folder, &path) else {
            return;
        };
        let rel = if rel == "." { String::new() } else { rel };
        repos.push(RepoEntry {
            name: display_name(folder, &path, &rel),
            rel,
            path: path.to_string_lossy().to_string(),
            kind,
        });
    };
    if root_is_repo {
        push(&mut repos, folder.to_path_buf(), RepoKind::Root);
        if let Ok(text) = fs::read_to_string(folder.join(".gitmodules")) {
            for rel in gitmodules_paths(&text) {
                let path = join_rel(folder, &rel);
                // Only a checked-out submodule is a repo here; an empty folder has nothing.
                if git_mark(&path) != GitMark::None {
                    push(&mut repos, path, RepoKind::Submodule);
                }
            }
        }
    } else {
        for child in visible_subfolders(folder) {
            match git_mark(&child) {
                GitMark::OwnHistory => push(&mut repos, child, RepoKind::Folder),
                GitMark::LinkedWorktree => {}
                GitMark::None => {
                    for grandchild in visible_subfolders(&child) {
                        if git_mark(&grandchild) == GitMark::OwnHistory {
                            push(&mut repos, grandchild, RepoKind::Folder);
                        }
                    }
                }
            }
        }
    }
    if let Some(file) = code_workspace_file(folder) {
        for path in code_workspace_folders(folder, &file) {
            if git_mark(&path) == GitMark::OwnHistory {
                push(&mut repos, path, RepoKind::Workspace);
            }
        }
    }
    Some(RepoSet {
        root: folder.to_string_lossy().to_string(),
        root_is_repo,
        repos,
        session: None,
    })
}

/// The repos a folder inside a session folder covers: the ones the record names that are still
/// there, placed relative to `folder`.
fn from_record(
    folder: &Path,
    session_root: &Path,
    session_record: record::SessionRecord,
) -> RepoSet {
    let mut repos = Vec::new();
    for repo in &session_record.repos {
        let path = join_rel(session_root, &repo.rel);
        if !path.is_dir() {
            continue;
        }
        let Some(rel) = relative_path(folder, &path) else {
            continue;
        };
        let rel = if rel == "." { String::new() } else { rel };
        repos.push(RepoEntry {
            name: repo.name.clone(),
            rel,
            path: path.to_string_lossy().to_string(),
            kind: repo.kind,
        });
    }
    RepoSet {
        root: folder.to_string_lossy().to_string(),
        root_is_repo: git_mark(folder) != GitMark::None,
        repos,
        session: Some(SessionFolder {
            root: session_root.to_string_lossy().to_string(),
            record: session_record,
        }),
    }
}
