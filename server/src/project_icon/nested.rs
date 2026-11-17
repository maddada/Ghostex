use super::{discover_icon_in_directory, ProjectIcon};
use std::{collections::VecDeque, path::Path};

/// CDXC:Icons 2026-09-24 WHY:
/// Monorepos can ship their favicon in a named child app (goodwatch-webapp) or apps/web rather than at the checkout root. Probe two levels in stable breadth-first order, with a shared entry and probe budget, excluding dependencies and generated output.
///
/// CDXC:Icons 2026-11-17 DECISION:
/// User (Q78, after a folder of separate repos like ~/dev borrowed one repo's icon): "Yes, only look inside one repo". The two-level search runs only when the project folder is inside a git work tree (a `.git` folder or file in it or above it), and it never probes or descends into a child folder that has a `.git` of its own (a separate repo, a submodule or a worktree). A folder of separate repos therefore shows its letter tile; a monorepo's app icon is still found, and an icon the folder has itself or one set by hand is unaffected.
pub(super) fn discover_nested_icon(root: &Path) -> Option<ProjectIcon> {
    if !root.ancestors().any(has_own_git) {
        return None;
    }
    let mut pending = VecDeque::from([(String::new(), 0)]);
    let mut entries_left = 4096usize;
    let mut probes_left = 128usize;
    while let Some((directory, depth)) = pending.pop_front() {
        let Ok(entries) = std::fs::read_dir(root.join(&directory)) else {
            continue;
        };
        let mut children = Vec::new();
        for entry in entries.take(entries_left) {
            entries_left -= 1;
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.')
                || matches!(
                    name,
                    "node_modules"
                        | "vendor"
                        | "dist"
                        | "build"
                        | "out"
                        | "target"
                        | "coverage"
                        | "storybook-static"
                        | "tmp"
                        | "artifacts"
                        | "DerivedData"
                        | "zig-out"
                )
            {
                continue;
            }
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) || has_own_git(&entry.path()) {
                continue;
            }
            children.push(if directory.is_empty() {
                name.to_owned()
            } else {
                format!("{directory}/{name}")
            });
        }
        children.sort();
        for child in children {
            if probes_left == 0 {
                return None;
            }
            probes_left -= 1;
            if let Some(icon) = discover_icon_in_directory(root, &child) {
                return Some(icon);
            }
            if depth == 0 {
                pending.push_back((child, depth + 1));
            }
        }
    }
    None
}

/// Whether a folder is the top of a git work tree: `.git` is a folder in a plain checkout and a file in a submodule or linked worktree.
fn has_own_git(directory: &Path) -> bool {
    directory.join(".git").symlink_metadata().is_ok()
}
