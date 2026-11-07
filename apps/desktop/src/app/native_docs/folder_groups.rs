//! The files list while the type filter is on or a name is typed: the matches grouped by folder.
//!
//! CDXC:Docs 2026-11-07 DECISION:
//! User: "the files list in the files sidepanel is bad man when i filter by html", then, on the
//! options page: folder groups (each folder with matches is one header row, its path relative to
//! the folder they all share, with its files one step under it and never deeper), for the type
//! filter and typed search alike, always on with no way back to the nested tree, the shared folder
//! said once above the list, and long file names cut at the end as before. The filter used to
//! open every folder from the project root down, 18px per level, so deep matches showed an icon
//! and no name in the 260px panel.

use std::collections::{BTreeSet, HashMap};

use super::files::{DocsTreeRow, parent_path};
use super::state::{DocsEntry, DocsEntryKind};

/// The grouped rows, and the folder every group sits under (said once above the list).
pub(crate) struct FolderGroups {
    pub(crate) rows: Vec<DocsTreeRow>,
    pub(crate) shared_root: Option<String>,
}

struct Group<'a> {
    path: &'a str,
    display: &'a str,
    items: Vec<&'a DocsEntry>,
}

/// Groups `entries` by their folder. `by_path` is the type filter's list with no name typed: only
/// its files, in path order. A typed search keeps its best-first order, so the group of the best
/// match comes first, and a folder found by name is a row of its own.
/// `collapsed` holds folded groups' paths; `root_label` names the group of files at the project
/// root.
pub(crate) fn folder_groups(
    entries: &[DocsEntry],
    by_path: bool,
    collapsed: &BTreeSet<String>,
    root_label: &str,
) -> FolderGroups {
    let mut ordered: Vec<&DocsEntry> = entries
        .iter()
        .filter(|entry| !by_path || entry.kind == DocsEntryKind::File)
        .collect();
    if by_path {
        ordered.sort_by(|a, b| a.path.cmp(&b.path));
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for &entry in &ordered {
        let path = parent_path(&entry.path);
        let at = *index.entry(path).or_insert_with(|| {
            groups.push(Group {
                path,
                display: parent_path(&entry.display_path),
                items: Vec::new(),
            });
            groups.len() - 1
        });
        groups[at].items.push(entry);
    }
    // A folder found by name that also heads a group is that group's header already.
    for group in &mut groups {
        group.items.retain(|entry| {
            entry.kind != DocsEntryKind::Directory || !index.contains_key(entry.path.as_str())
        });
    }
    groups.retain(|group| !group.items.is_empty());

    let shared = shared_root_segments(groups.iter().map(|group| group.display));
    let shared_root = (shared > 0).then(|| {
        groups[0]
            .display
            .split('/')
            .take(shared)
            .collect::<Vec<_>>()
            .join("/")
    });
    let mut rows = Vec::with_capacity(groups.len() + ordered.len());
    for group in groups {
        let label = if group.display.is_empty() {
            root_label.to_string()
        } else {
            group
                .display
                .split('/')
                .skip(shared)
                .collect::<Vec<_>>()
                .join("/")
        };
        let expanded = !collapsed.contains(group.path);
        rows.push(DocsTreeRow {
            path: group.path.to_string(),
            display_path: group.display.to_string(),
            name: label,
            kind: DocsEntryKind::Directory,
            depth: 0,
            expanded,
            group: Some(group.items.len()),
        });
        if expanded {
            rows.extend(group.items.into_iter().map(|entry| DocsTreeRow {
                path: entry.path.clone(),
                display_path: entry.display_path.clone(),
                name: entry.name.clone(),
                kind: entry.kind,
                depth: 0,
                expanded: false,
                group: None,
            }));
        }
    }
    FolderGroups { rows, shared_root }
}

/// How many leading folders every group shares, leaving each group at least its own last folder.
/// None shared when a group is the project root itself.
fn shared_root_segments<'a>(folders: impl Iterator<Item = &'a str>) -> usize {
    let mut common: Option<Vec<&str>> = None;
    for folder in folders {
        if folder.is_empty() {
            return 0;
        }
        let parts: Vec<&str> = folder.split('/').collect();
        // Every group keeps at least its last folder as its header.
        let cap = parts.len() - 1;
        common = Some(match common {
            None => parts[..cap].to_vec(),
            Some(common) => common
                .iter()
                .zip(&parts[..cap])
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    common.map_or(0, |common| common.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> DocsEntry {
        entry(path, DocsEntryKind::File)
    }

    fn entry(path: &str, kind: DocsEntryKind) -> DocsEntry {
        DocsEntry {
            path: path.to_string(),
            display_path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            kind,
            depth: path.matches('/').count(),
            size: None,
            modified_at: None,
        }
    }

    fn labels(groups: &FolderGroups) -> Vec<String> {
        groups
            .rows
            .iter()
            .map(|row| match row.group {
                Some(count) => format!("# {} ({count})", row.name),
                None => row.name.clone(),
            })
            .collect()
    }

    #[test]
    fn deep_matches_group_under_their_folder_with_the_shared_root_said_once() {
        let entries = [
            file("ref/vscode/src/vs/editor/test/common/viewLayout/__snapshots__/b.html"),
            file("ref/vscode/src/vs/code/browser/workbench/workbench.html"),
            file("ref/vscode/src/vs/editor/test/common/viewLayout/__snapshots__/a.html"),
            file("ref/vscode/src/vs/code/browser/workbench/callback.html"),
            file("ref/vscode/src/vs/base/test/node/pfs/fixtures/index.html"),
        ];
        let groups = folder_groups(&entries, true, &BTreeSet::new(), "project");
        assert_eq!(groups.shared_root.as_deref(), Some("ref/vscode/src/vs"));
        assert_eq!(
            labels(&groups),
            [
                "# base/test/node/pfs/fixtures (1)",
                "index.html",
                "# code/browser/workbench (2)",
                "callback.html",
                "workbench.html",
                "# editor/test/common/viewLayout/__snapshots__ (2)",
                "a.html",
                "b.html",
            ]
        );
        // Every row sits at one indent: headers and files alike never nest deeper.
        assert!(groups.rows.iter().all(|row| row.depth == 0));
        // A header is its folder, so drag, drop and the folder menu act on the real folder.
        assert_eq!(
            groups.rows[2].path,
            "ref/vscode/src/vs/code/browser/workbench"
        );
    }

    #[test]
    fn a_single_group_keeps_its_own_folder_as_the_header() {
        let entries = [file("docs/guides/setup.html"), file("docs/guides/faq.html")];
        let groups = folder_groups(&entries, true, &BTreeSet::new(), "project");
        assert_eq!(groups.shared_root.as_deref(), Some("docs"));
        assert_eq!(labels(&groups)[0], "# guides (2)");
    }

    #[test]
    fn files_at_the_project_root_get_the_root_label_and_no_shared_root() {
        let entries = [file("index.html"), file("site/about.html")];
        let groups = folder_groups(&entries, true, &BTreeSet::new(), "Ghostex");
        assert_eq!(groups.shared_root, None);
        assert_eq!(
            labels(&groups),
            ["# Ghostex (1)", "index.html", "# site (1)", "about.html"]
        );
    }

    #[test]
    fn a_folded_group_hides_its_files_but_keeps_its_count() {
        let entries = [file("a/x/one.html"), file("a/y/two.html")];
        let collapsed = BTreeSet::from(["a/x".to_string()]);
        let groups = folder_groups(&entries, true, &collapsed, "project");
        assert_eq!(labels(&groups), ["# x (1)", "# y (1)", "two.html"]);
        assert!(!groups.rows[0].expanded);
    }

    #[test]
    fn typed_search_keeps_best_first_order_and_folds_a_found_folder_into_its_header() {
        let entries = [
            file("src/zeta/workbench.html"),
            entry("src/alpha", DocsEntryKind::Directory),
            entry("src/zeta", DocsEntryKind::Directory),
            file("src/alpha/work.md"),
        ];
        let groups = folder_groups(&entries, false, &BTreeSet::new(), "project");
        assert_eq!(
            labels(&groups),
            ["# zeta (1)", "workbench.html", "# alpha (1)", "work.md"]
        );
        assert_eq!(groups.shared_root.as_deref(), Some("src"));
    }

    #[test]
    fn a_found_folder_whose_folder_has_no_other_match_stays_a_row() {
        let entries = [entry("src/alpha", DocsEntryKind::Directory)];
        let groups = folder_groups(&entries, false, &BTreeSet::new(), "project");
        assert_eq!(labels(&groups), ["# src (1)", "alpha"]);
        assert_eq!(groups.rows[1].kind, DocsEntryKind::Directory);
    }
}
