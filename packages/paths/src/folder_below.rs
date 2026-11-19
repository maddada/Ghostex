//! Whether a stored path is a folder or lies inside it, for paths that may no longer exist: a
//! folder that moved or was renamed, which the disk can no longer answer for.
//!
//! CDXC:Worktrees 2026-11-19 WHY: Rename Worktree (server/src/server/worktree_ops/rename.rs), Locate
//! Folder… on a missing project (`relocate_project_moving_sessions` in
//! server/src/domain/repository/project.rs) both decide which stored folders sat inside a folder
//! that is gone, so they share one matcher: paths compared part by part, `\` separating parts
//! in a Windows path as `/` does, a `\\?\` prefix ignored, letter case ignored where the volume
//! ignores it, and a folder reached through a symlink matched under the link's target too (the
//! user's `~/dev/shortpoint` was a link into a folder on another drive, and sessions stored either
//! spelling).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A path with a drive letter (`C:\…`, `C:/…`) or a UNC or verbatim prefix (`\\server\…`).
pub fn is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with(r"\\")
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// A path's parts: split on `/`, and on `\` too in a Windows path, whose `\\?\` prefix is dropped.
pub fn split_path_parts(path: &str, windows: bool) -> Vec<&str> {
    let path = if windows {
        path.strip_prefix(r"\\?\").unwrap_or(path)
    } else {
        path
    };
    path.split(|c| c == '/' || (windows && c == '\\'))
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

/// The parts of `path` after the parts of `root`, or `None` when `path` is not `root` or inside it.
pub fn path_parts_below<'a>(
    path: &'a str,
    root: &str,
    windows: bool,
    ignore_case: bool,
) -> Option<Vec<&'a str>> {
    let path_parts = split_path_parts(path, windows);
    let root_parts = split_path_parts(root, windows);
    if root_parts.is_empty() || path_parts.len() < root_parts.len() {
        return None;
    }
    let same = |a: &str, b: &str| {
        if ignore_case {
            a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    };
    path_parts
        .iter()
        .zip(&root_parts)
        .all(|(a, b)| same(a, b))
        .then(|| path_parts[root_parts.len()..].to_vec())
}

/// A folder that may be gone, as one match root: its spellings (as stored and through its links)
/// and whether its volume ignores letter case. Read from the disk once, when made.
#[derive(Clone, Debug)]
pub struct MissingFolderRoot {
    spellings: Vec<String>,
    ignore_case: bool,
}

impl MissingFolderRoot {
    pub fn new(folder: &str) -> Self {
        let folder = folder.trim();
        let mut spellings = vec![folder.to_string()];
        if let Some(resolved) = resolve_links(Path::new(folder)) {
            let resolved = resolved.to_string_lossy().into_owned();
            if !spellings.contains(&resolved) {
                spellings.push(resolved);
            }
        }
        let ignore_case = spellings
            .iter()
            .any(|spelling| nearest_folder_ignores_case(Path::new(spelling)));
        Self {
            spellings,
            ignore_case,
        }
    }

    /// The parts of `path` below this folder, or `None` when `path` is not it or inside it.
    pub fn parts_below<'a>(&self, path: &'a str) -> Option<Vec<&'a str>> {
        self.spellings.iter().find_map(|root| {
            let windows = cfg!(windows) || is_windows_path(root) || is_windows_path(path);
            path_parts_below(path, root, windows, windows || self.ignore_case)
        })
    }

    /// `path` is this folder or lies inside it.
    pub fn contains(&self, path: &str) -> bool {
        self.parts_below(path).is_some()
    }
}

/// `path` with every symlink along it followed, including a last link whose target is gone (which
/// `canonicalize` refuses); `None` when no link was followed.
fn resolve_links(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let parts = |path: &Path| -> Vec<OsString> {
        path.components()
            .map(|component| component.as_os_str().to_os_string())
            .rev()
            .collect()
    };
    let mut rest = parts(path);
    let mut current = PathBuf::new();
    let mut followed = 0;
    while let Some(part) = rest.pop() {
        if part == ".." {
            current.pop();
            continue;
        }
        let next = current.join(&part);
        let is_link =
            std::fs::symlink_metadata(&next).is_ok_and(|meta| meta.file_type().is_symlink());
        if !is_link {
            current = next;
            continue;
        }
        // 40 links is the system's own limit for a loop.
        followed += 1;
        if followed > 40 {
            return None;
        }
        let target = std::fs::read_link(&next).ok()?;
        if target.is_absolute() {
            current = PathBuf::new();
        }
        rest.extend(parts(&target));
    }
    (followed > 0).then_some(current)
}

/// Whether the volume `folder` is on ignores letter case, asked of the nearest folder along it that
/// still exists and has letters in its name (`folder` itself is often gone by now).
fn nearest_folder_ignores_case(folder: &Path) -> bool {
    folder
        .ancestors()
        .filter(|ancestor| ancestor.is_dir())
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.chars().any(char::is_alphabetic))
        })
        .is_some_and(crate::same_folder::volume_ignores_case)
}
