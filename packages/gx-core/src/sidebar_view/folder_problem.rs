//! The mark a project whose folder is missing wears on its header, and the words of the popover it
//! opens: the cause gxserver found (`PresentationProject::path_problem`) and the fixes.
//!
//! CDXC:Projects 2026-11-20 DECISION: User picked (Q4244) a computer with a slash through it,
//! "should be only on the project header, not on the actual sessions", knowing a computer's icon
//! turns soft red when that computer is offline; so the mark keeps its own amber (grey while a
//! drive is away, since nothing is broken then). Clicking it opens a popover for that one project:
//! what broke, Use <the folder found>, Locate Folder…, Ask AI to fix and Remove project; no Fix all,
//! no pop-up and no banner. After a fix the popover says so for a moment, closes and the mark goes.
//! SEE-ALSO: server/src/project_folder_problem.rs (the diagnosis), the phone's copy of these words
//! in apps/mobile/app/src/contract/folderProblem.ts.

use ghostex_gx_protocol::presentation::{PathProblem, PathState};

/// Why the folder is missing, as the popover tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderProblemCause {
    Renamed,
    Link,
    Drive,
    Gone,
}

/// One labelled path in the popover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderProblemPath {
    pub label: &'static str,
    pub path: String,
    /// The path is the one that no longer exists (drawn struck through).
    pub gone: bool,
}

/// What a project's missing folder draws: the mark on its header and the popover the mark opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderProblemView {
    pub cause: FolderProblemCause,
    /// The project's folder as stored.
    pub folder: String,
    /// The mark is grey, not amber: the folder is on a drive that isn't connected and comes back
    /// by itself.
    pub quiet: bool,
    pub tooltip: String,
    pub title: String,
    pub line: String,
    pub paths: Vec<FolderProblemPath>,
    /// The folder offered as the fix, and its button's label (`Use shortpoint-repos/app`).
    pub use_folder: Option<String>,
    pub use_label: Option<String>,
    /// Locate Folder… is the main button (no folder was found and the drive is not just away).
    pub locate_primary: bool,
    pub locate_label: &'static str,
}

/// The last part of a path, `\` or `/` separated.
fn base_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

/// `found` from the folder that holds `renamed_to` on, for the Use button
/// (`shortpoint-repos/shortpoint-ai`); just its name without one.
fn use_label_path(found: &str, renamed_to: Option<&str>) -> String {
    let Some(renamed_to) = renamed_to else {
        return base_name(found).to_string();
    };
    let holder_len = renamed_to.len() - base_name(renamed_to).len();
    found
        .get(holder_len..)
        .filter(|rest| found.starts_with(&renamed_to[..holder_len]) && !rest.is_empty())
        .unwrap_or_else(|| base_name(found))
        .to_string()
}

/// The paths with the folders they all start with written as `…`, when there are two or more and
/// they share more than the root (`…/shortpoint/app` and `…/shortpoint-repos/app`).
fn shorten_shared_start(paths: &mut [FolderProblemPath]) {
    if paths.len() < 2 {
        return;
    }
    let split = |path: &str| -> Vec<String> {
        path.split(['/', '\\'])
            .map(str::to_string)
            .collect()
    };
    let parts: Vec<Vec<String>> = paths.iter().map(|path| split(&path.path)).collect();
    let mut shared = 0;
    // Keep at least the last two parts of every path.
    let shortest = parts.iter().map(Vec::len).min().unwrap_or(0);
    while shared + 2 < shortest && parts.iter().all(|path| path[shared] == parts[0][shared]) {
        shared += 1;
    }
    if shared < 3 {
        return;
    }
    let separator = if paths[0].path.contains('\\') && !paths[0].path.contains('/') {
        "\\"
    } else {
        "/"
    };
    for (path, parts) in paths.iter_mut().zip(parts) {
        path.path = format!("…{separator}{}", parts[shared..].join(separator));
    }
}

impl FolderProblemView {
    /// The mark and popover of `title`'s project, whose folder `path` is in `state`; `None` while
    /// the folder is there or its state is not known.
    pub fn new(
        title: &str,
        path: &str,
        state: Option<&PathState>,
        problem: Option<&PathProblem>,
    ) -> Option<Self> {
        if !matches!(
            state,
            Some(PathState::Missing) | Some(PathState::NotDirectory)
        ) {
            return None;
        }
        let path = path.trim();
        if path.is_empty() {
            return None;
        }
        let title = if title.trim().is_empty() {
            base_name(path)
        } else {
            title.trim()
        };
        let cause = match problem.map(|problem| problem.cause.as_str()) {
            Some("renamed") => FolderProblemCause::Renamed,
            Some("link") => FolderProblemCause::Link,
            Some("drive") => FolderProblemCause::Drive,
            _ => FolderProblemCause::Gone,
        };
        let found = problem.and_then(|problem| problem.found.clone());
        let renamed_to = problem.and_then(|problem| problem.renamed_to.as_deref());
        let missing = problem.and_then(|problem| problem.missing.as_deref());
        let use_label = found
            .as_deref()
            .map(|found| format!("Use {}", use_label_path(found, renamed_to)));
        let mut paths = Vec::new();
        let (tooltip, view_title, line) = match cause {
            FolderProblemCause::Renamed => {
                paths.push(FolderProblemPath {
                    label: "Was",
                    path: path.to_string(),
                    gone: true,
                });
                let line = match (missing, renamed_to) {
                    (Some(missing), Some(renamed_to))
                        if missing.trim_end_matches(['/', '\\'])
                            != path.trim_end_matches(['/', '\\']) =>
                    {
                        format!(
                            "The folder it was in, {}, is now {}, and {title} is inside it.",
                            base_name(missing),
                            base_name(renamed_to)
                        )
                    }
                    (_, Some(renamed_to)) => format!(
                        "Ghostex found a folder with a close name where it was: {}.",
                        base_name(renamed_to)
                    ),
                    _ => "Ghostex found a folder with a close name where it was.".to_string(),
                };
                (
                    "Folder missing: click to fix".to_string(),
                    format!("{title}'s folder was renamed"),
                    line,
                )
            }
            FolderProblemCause::Link => {
                let link = problem
                    .and_then(|problem| problem.link.clone())
                    .unwrap_or_else(|| path.to_string());
                paths.push(FolderProblemPath {
                    label: "Link",
                    path: link,
                    gone: false,
                });
                if let Some(target) = problem.and_then(|problem| problem.link_target.clone()) {
                    paths.push(FolderProblemPath {
                        label: "Links to",
                        path: target,
                        gone: true,
                    });
                }
                let line = match renamed_to {
                    Some(renamed_to) if found.is_some() => format!(
                        "Its folder is a link to a folder that was renamed to {}.",
                        base_name(renamed_to)
                    ),
                    _ => "Its folder is a link to a folder that no longer exists, and nothing with a close name is next to it.".to_string(),
                };
                (
                    "Folder link points nowhere: click to fix".to_string(),
                    format!("{title}'s folder link points nowhere"),
                    line,
                )
            }
            FolderProblemCause::Drive => {
                let drive = problem
                    .and_then(|problem| problem.drive.clone())
                    .unwrap_or_default();
                let name = base_name(&drive).trim_end_matches(':');
                let view_title = if drive.ends_with(':') {
                    format!("Drive {drive} isn't connected")
                } else if name.is_empty() {
                    "Its drive isn't connected".to_string()
                } else {
                    format!("The {name} drive isn't connected")
                };
                if !drive.is_empty() {
                    paths.push(FolderProblemPath {
                        label: "Drive",
                        path: drive,
                        gone: false,
                    });
                }
                (
                    "Drive not connected".to_string(),
                    view_title,
                    format!(
                        "{title} lives on it. Connect the drive and the project comes back by itself; nothing needs fixing."
                    ),
                )
            }
            FolderProblemCause::Gone => {
                paths.push(FolderProblemPath {
                    label: "Was",
                    path: path.to_string(),
                    gone: true,
                });
                (
                    "Folder missing: click to fix".to_string(),
                    format!("{title}'s folder is missing"),
                    "It isn't where it was, and no folder with a close name is nearby. It may have been moved, renamed or deleted.".to_string(),
                )
            }
        };
        if let Some(found) = &found {
            if cause != FolderProblemCause::Drive {
                paths.push(FolderProblemPath {
                    label: "Found",
                    path: found.clone(),
                    gone: false,
                });
            }
        }
        shorten_shared_start(&mut paths);
        let offers_found = found.is_some() && cause != FolderProblemCause::Drive;
        Some(Self {
            cause,
            folder: path.to_string(),
            quiet: cause == FolderProblemCause::Drive,
            tooltip,
            title: view_title,
            line,
            paths,
            use_folder: found.clone().filter(|_| offers_found),
            use_label: use_label.filter(|_| offers_found),
            locate_primary: !offers_found && cause != FolderProblemCause::Drive,
            locate_label: if cause == FolderProblemCause::Drive {
                "It moved: Locate Folder…"
            } else {
                "Locate Folder…"
            },
        })
    }

    /// What the popover says after `folder` became the project's folder.
    pub fn fixed_words(title: &str, folder: &str) -> (String, String) {
        let trimmed = folder.trim_end_matches(['/', '\\']);
        let holder = trimmed
            .rfind(['/', '\\'])
            .map(|index| base_name(&trimmed[..index]))
            .filter(|holder| !holder.is_empty());
        let shown = match holder {
            Some(holder) => format!("{holder}/{}", base_name(trimmed)),
            None => base_name(trimmed).to_string(),
        };
        (
            format!("{} now uses {shown}", title.trim()),
            "Its sessions moved with it.".to_string(),
        )
    }
}
