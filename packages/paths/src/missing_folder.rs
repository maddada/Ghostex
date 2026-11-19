//! Why a folder that should be there is not: a link along it whose target is gone, a drive that
//! isn't connected, a folder along it that was renamed (with the folder found under the new name),
//! or none of these.
//!
//! CDXC:Projects 2026-11-20 DECISION: User, of a project whose folder had been renamed outside
//! Ghostex, picked (Q4244) a mark on the project's header that opens a popover for that one
//! project: "just need an icon on the actual project itself ... i want to fix folder by folder by
//! clicking on that icon". The popover names the cause and offers the folder found, so gxserver
//! reads the disk once here and every client words the same answer
//! (server/src/project_folder_problem.rs publishes it as the project's `pathProblem`).

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Why the folder is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingFolderCause {
    /// A folder along the path is gone and a folder with a close name holds the rest of it.
    Renamed,
    /// A link along the path points at a folder that no longer exists (`found` may still name
    /// where that folder went).
    Link,
    /// The path is on a drive that isn't connected.
    Drive,
    /// None of these: moved, renamed beyond recognition, or deleted.
    Gone,
}

impl MissingFolderCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Renamed => "renamed",
            Self::Link => "link",
            Self::Drive => "drive",
            Self::Gone => "gone",
        }
    }
}

/// What [`diagnose_missing_folder`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingFolderDiagnosis {
    pub cause: MissingFolderCause,
    /// The folder that is most likely the missing one now: the same place inside the folder that
    /// took the missing folder's place under a close name.
    pub found: Option<String>,
    /// The folder along the path that is gone (the renamed one), as the disk spells its parent.
    pub missing: Option<String>,
    /// The folder that took `missing`'s place under a close name; `found` is inside it or is it.
    pub renamed_to: Option<String>,
    /// The link along the path whose target is gone, and that target.
    pub link: Option<String>,
    pub link_target: Option<String>,
    /// The drive that isn't connected (`/Volumes/Files`, `E:`).
    pub drive: Option<String>,
}

/// Why `folder` (an absolute path) is not a folder, or `None` when it is one or the path is not
/// absolute.
pub fn diagnose_missing_folder(folder: &str) -> Option<MissingFolderDiagnosis> {
    let folder = folder.trim();
    let path = Path::new(folder);
    if !path.is_absolute() || path.is_dir() {
        return None;
    }
    let mut diagnosis = MissingFolderDiagnosis {
        cause: MissingFolderCause::Gone,
        found: None,
        missing: None,
        renamed_to: None,
        link: None,
        link_target: None,
        drive: None,
    };
    if let Some(drive) = missing_windows_drive(path) {
        diagnosis.cause = MissingFolderCause::Drive;
        diagnosis.drive = Some(drive);
        return Some(diagnosis);
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
    let mut gap: Option<(PathBuf, OsString)> = None;
    while let Some(part) = rest.pop() {
        if part == ".." {
            current.pop();
            continue;
        }
        let next = current.join(&part);
        match std::fs::symlink_metadata(&next) {
            Ok(meta) if meta.file_type().is_symlink() => {
                followed += 1;
                // 40 links is the system's own limit for a loop.
                if followed > 40 {
                    return Some(diagnosis);
                }
                let Ok(target) = std::fs::read_link(&next) else {
                    return Some(diagnosis);
                };
                let target_path = if target.is_absolute() {
                    target.clone()
                } else {
                    current.join(&target)
                };
                if diagnosis.link.is_none() && std::fs::metadata(&target_path).is_err() {
                    diagnosis.link = Some(next.to_string_lossy().into_owned());
                    diagnosis.link_target = Some(lexically_normal(&target_path));
                }
                if target.is_absolute() {
                    current = PathBuf::new();
                }
                rest.extend(parts(&target));
            }
            Ok(meta) if meta.is_dir() => current = next,
            // A file where a folder should be: nothing more to find.
            Ok(_) => break,
            Err(_) => {
                gap = Some((current.clone(), part));
                break;
            }
        }
    }
    if diagnosis.link.is_some() {
        diagnosis.cause = MissingFolderCause::Link;
    }
    let Some((parent, missing)) = gap else {
        return Some(diagnosis);
    };
    if let Some(drive) = unmounted_drive(&parent, &missing) {
        diagnosis.cause = MissingFolderCause::Drive;
        diagnosis.drive = Some(drive);
        return Some(diagnosis);
    }
    // `rest` is a stack: the parts after the missing one, last first.
    let below: Vec<OsString> = rest.into_iter().rev().filter(|part| part != "..").collect();
    if let Some((renamed_to, found)) = renamed_neighbour(&parent, &missing, &below) {
        if diagnosis.cause == MissingFolderCause::Gone {
            diagnosis.cause = MissingFolderCause::Renamed;
        }
        diagnosis.missing = Some(parent.join(&missing).to_string_lossy().into_owned());
        diagnosis.renamed_to = Some(renamed_to.to_string_lossy().into_owned());
        diagnosis.found = Some(found.to_string_lossy().into_owned());
    }
    Some(diagnosis)
}

/// `path` with `.` and `..` folded away, as text.
fn lexically_normal(path: &Path) -> String {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

/// The drive a missing `parent/name` would be: `/Volumes/<name>` on a Mac, `/media/<user>/<name>`,
/// `/media/<name>` or `/run/media/<user>/<name>` on Linux.
fn unmounted_drive(parent: &Path, name: &OsString) -> Option<String> {
    let parent_text = parent.to_string_lossy();
    let parent_text = parent_text.trim_end_matches('/');
    let mount_parent = parent_text == "/Volumes"
        || parent_text == "/media"
        || parent
            .parent()
            .is_some_and(|grand| grand == Path::new("/media") || grand == Path::new("/run/media"));
    mount_parent.then(|| parent.join(name).to_string_lossy().into_owned())
}

/// A Windows path on a drive letter that is not there (`E:`).
#[cfg(windows)]
fn missing_windows_drive(path: &Path) -> Option<String> {
    use std::path::Prefix;
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    let letter = match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter as char,
        _ => return None,
    };
    let root = format!("{letter}:\\");
    (!Path::new(&root).exists()).then(|| format!("{letter}:"))
}

#[cfg(not(windows))]
fn missing_windows_drive(_path: &Path) -> Option<String> {
    None
}

/// The folder next to the missing `parent/missing` with the closest name that holds the rest of
/// the path (`below`), when one does.
fn renamed_neighbour(
    parent: &Path,
    missing: &OsString,
    below: &[OsString],
) -> Option<(PathBuf, PathBuf)> {
    let missing = missing.to_string_lossy().to_lowercase();
    let entries = std::fs::read_dir(parent).ok()?;
    let mut best: Option<(usize, PathBuf, PathBuf)> = None;
    for entry in entries.flatten().take(5000) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // With nothing below the missing folder to confirm the match, only a very close name counts.
        let Some(score) = name_closeness(&missing, &name.to_lowercase(), below.is_empty()) else {
            continue;
        };
        let renamed_to = parent.join(&name);
        let mut candidate = renamed_to.clone();
        for part in below {
            candidate.push(part);
        }
        if !candidate.is_dir() {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _, _)| score < *best_score)
        {
            best = Some((score, renamed_to, candidate));
        }
    }
    best.map(|(_, renamed_to, found)| (renamed_to, found))
}

/// How far apart two lowercased folder names are, when they are close enough to be one renamed to
/// the other; lower is closer.
fn name_closeness(missing: &str, name: &str, strict: bool) -> Option<usize> {
    if missing == name {
        return Some(0);
    }
    let shorter = missing.chars().count().min(name.chars().count());
    let longer = missing.chars().count().max(name.chars().count());
    let alnum = |text: &str| -> String { text.chars().filter(|c| c.is_alphanumeric()).collect() };
    if !alnum(missing).is_empty() && alnum(missing) == alnum(name) {
        return Some(1);
    }
    // `shortpoint` → `shortpoint-repos`, `app-old` → `app`.
    if shorter >= 3 && (name.starts_with(missing) || missing.starts_with(name)) {
        let extra = longer - shorter;
        if !strict || extra <= 8 {
            return Some(1 + extra);
        }
    }
    let distance = edit_distance(missing, name);
    let allowed = if strict { 2 } else { (longer / 4).max(2) };
    (distance <= allowed && distance < shorter).then_some(1 + distance)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitute.min(row[j] + 1).min(previous + 1);
        }
    }
    row[b.len()]
}
