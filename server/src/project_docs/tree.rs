use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;

/*
CDXC:Docs 2026-08-09:
The project's own entries come first and are discovered exactly as they have
always been, so setting a Docs directory can never take the repo's README.md,
CLAUDE.md, or docs/ away. The mounted Docs directory is appended after them.
*/
pub(super) fn project_file_entries(context: DocsContext<'_>) -> Result<Vec<Value>, String> {
    let mut entries = project_root_file_entries(context.roots.project.as_path(), context, true)?;
    if let Some(mount) = context.roots.extra.as_ref() {
        append_extra_root_entries(&mut entries, mount);
    }
    Ok(entries)
}

pub(super) fn project_root_file_entries(
    root: &Path,
    context: DocsContext<'_>,
    recursive: bool,
) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let mut scanned_directory_entries = 0;
    let roots = scan_roots(root, context.additional_docs_folders);
    for relative_path in &roots {
        if entries.len() >= FILE_LIST_MAX_ENTRIES {
            return Err(docs_scan_cap_error());
        }
        let Some(directory) = project_directory(root, relative_path) else {
            continue;
        };
        let metadata = fs::metadata(&directory).ok();
        entries.push(json!({
            "depth": 0,
            "kind": "directory",
            "modifiedAt": metadata.as_ref().and_then(modified_at),
            "name": relative_path,
            "path": relative_path,
            "size": Value::Null,
        }));
    }
    append_root_artifacts(&mut entries, root, &mut scanned_directory_entries)?;
    if !recursive {
        return Ok(entries);
    }
    for relative_path in &roots {
        let Some(directory) = project_directory(root, relative_path) else {
            continue;
        };
        append_file_entries(
            &mut entries,
            root,
            &directory,
            relative_path,
            1,
            &mut scanned_directory_entries,
            true,
        )?;
    }
    Ok(entries)
}

/*
CDXC:Docs 2026-08-09:
Whole-tree discovery for the mounted Docs directory: it is walked to the bottom
so a note nested five folders deep in a vault is listed like any other. Files
are narrowed to the extensions Docs actually renders, because a vault's image
and attachment folders are not documents.

CDXC:Docs 2026-08-09:
The mount is one top-level folder named after the directory, and every failure
lands on that node's label instead of on the listing: an unopenable directory,
and the entry cap (the depth cap only stops descending). Losing the whole panel, including the
project's own README.md, because a vault is too deep is the one thing this
must not do, and a tree that silently stopped at 20,000 entries reads exactly
like a vault that only has that many, so the cap is named on the node.
*/
fn append_extra_root_entries(entries: &mut Vec<Value>, mount: &DocsExtraMount) {
    let root = match mount.location.as_deref() {
        Ok(root) => root,
        Err(error) => {
            entries.push(unavailable_extra_root_entry(&mount.name, error));
            return;
        }
    };
    let mut tree = Vec::new();
    let mut scanned_directory_entries = 0;
    if let Err(error) = append_docs_tree_entries(
        &mut tree,
        root,
        root,
        EXTRA_ROOT_MOUNT_SEGMENT,
        1,
        &mut scanned_directory_entries,
        true,
    ) {
        entries.push(unavailable_extra_root_entry(&mount.name, &error));
        return;
    }
    let metadata = fs::metadata(root).ok();
    entries.push(json!({
        "depth": 0,
        "displayPath": mount.name,
        "kind": "directory",
        "modifiedAt": metadata.as_ref().and_then(modified_at),
        "name": mount.name,
        "path": EXTRA_ROOT_MOUNT_SEGMENT,
        "size": Value::Null,
    }));
    name_extra_root_tree_entries(&mut tree, &mount.name);
    entries.append(&mut tree);
}

/*
CDXC:Docs 2026-08-10:
Every mounted entry carries the name the tree shows it under beside the routing
address it answers to, for the same reason a preview does: `path` starts with
the reserved segment, which is an implementation detail no reader asked for, and
it reaches humans through Copy Path and through text pasted into a terminal. The
walk builds the routing paths, so the display names are derived from them here
rather than threaded through every recursion.
*/
pub(super) fn name_extra_root_tree_entries(tree: &mut [Value], mount_name: &str) {
    for entry in tree {
        let Some(relative_path) = entry
            .get("path")
            .and_then(Value::as_str)
            .and_then(|path| path.strip_prefix(EXTRA_ROOT_MOUNT_SEGMENT))
        else {
            continue;
        };
        let display_path = format!("{mount_name}{relative_path}");
        if let Some(entry) = entry.as_object_mut() {
            entry.insert("displayPath".to_string(), Value::String(display_path));
        }
    }
}

/// The mount still shows when its folder does not, carrying the reason in the
/// only field the Docs tree renders. A missing vault must look missing, not
/// look empty.
pub(super) fn unavailable_extra_root_entry(name: &str, error: &str) -> Value {
    json!({
        "depth": 0,
        "kind": "directory",
        "displayPath": name,
        "modifiedAt": Value::Null,
        "name": format!("{name}: {error}"),
        "path": EXTRA_ROOT_MOUNT_SEGMENT,
        "size": Value::Null,
    })
}

pub(super) fn append_docs_tree_entries(
    entries: &mut Vec<Value>,
    root: &Path,
    directory: &Path,
    relative_directory: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
    recursive: bool,
) -> Result<(), String> {
    let mut children = bounded_directory_entries(
        directory,
        scanned_directory_entries,
        DOCS_TREE_MAX_ENTRIES,
        docs_tree_entry_cap_error,
    )?;
    children.sort_by(|left, right| {
        let left_is_dir = left.metadata().is_ok_and(|metadata| metadata.is_dir());
        let right_is_dir = right.metadata().is_ok_and(|metadata| metadata.is_dir());
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    let mut directories = Vec::new();
    for child in children {
        if entries.len() >= DOCS_TREE_MAX_ENTRIES {
            return Err(docs_tree_entry_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        let Ok(metadata) = child.metadata() else {
            continue;
        };
        let is_directory = metadata.is_dir();
        if is_directory {
            if name.starts_with('.') || IGNORED_DIRECTORY_NAMES.contains(&name.as_str()) {
                continue;
            }
        } else if !has_docs_artifact_extension(&name) {
            continue;
        }
        // Confinement, and the reason an outward symlink never joins the tree.
        let Ok(resolved) = fs::canonicalize(child.path()) else {
            continue;
        };
        if !resolved.starts_with(root) {
            continue;
        }
        let relative_path = format!("{relative_directory}/{name}");
        entries.push(file_entry(
            depth,
            if is_directory { "directory" } else { "file" },
            &name,
            &relative_path,
            &metadata,
        ));
        if is_directory && child.file_type().is_ok_and(|kind| kind.is_symlink()) {
            if let Some(entry) = entries.last_mut().and_then(Value::as_object_mut) {
                entry.insert("childrenLoaded".to_string(), Value::Bool(true));
            }
        }
        if is_directory
            && !child
                .file_type()
                .is_ok_and(|file_type| file_type.is_symlink())
        {
            directories.push((child.path(), relative_path));
        }
    }
    if !recursive || depth >= DOCS_TREE_MAX_DEPTH {
        return Ok(());
    }
    for (directory, relative_path) in directories {
        append_docs_tree_entries(
            entries,
            root,
            &directory,
            &relative_path,
            depth + 1,
            scanned_directory_entries,
            true,
        )?;
    }
    Ok(())
}

pub(super) fn docs_scan_cap_error() -> String {
    format!("Project Docs exceeds the scan limit of {FILE_LIST_MAX_ENTRIES} files and folders.")
}

fn docs_tree_entry_cap_error() -> String {
    format!(
        "Docs directory holds more than {DOCS_TREE_MAX_ENTRIES} files and folders. Point it at a smaller folder in Settings > Projects."
    )
}

pub(super) fn project_directory(root: &Path, relative_path: &str) -> Option<PathBuf> {
    let directory = root.join(relative_path);
    if !fs::metadata(&directory).ok()?.is_dir() {
        return None;
    }
    let resolved = fs::canonicalize(directory).ok()?;
    resolved.starts_with(root).then_some(resolved)
}

fn bounded_directory_entries(
    directory: &Path,
    scanned_directory_entries: &mut usize,
    limit: usize,
    limit_error: fn() -> String,
) -> Result<Vec<ghostex_docs::directory::Entry>, String> {
    ghostex_docs::directory::children(directory, scanned_directory_entries, limit, limit_error)
}

fn append_root_artifacts(
    entries: &mut Vec<Value>,
    root: &Path,
    scanned_directory_entries: &mut usize,
) -> Result<(), String> {
    let mut children = bounded_directory_entries(
        root,
        scanned_directory_entries,
        FILE_LIST_MAX_ENTRIES,
        docs_scan_cap_error,
    )?;
    children.sort_by_key(|child| child.file_name());
    for child in children {
        if entries.len() >= FILE_LIST_MAX_ENTRIES {
            return Err(docs_scan_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" || !is_root_artifact(&name) {
            continue;
        }
        let Ok(metadata) = child.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            continue;
        }
        let Ok(resolved) = fs::canonicalize(child.path()) else {
            continue;
        };
        if !resolved.starts_with(root) {
            continue;
        }
        entries.push(file_entry(0, "file", &name, &name, &metadata));
    }
    Ok(())
}

pub(super) fn append_file_entries(
    entries: &mut Vec<Value>,
    root: &Path,
    directory: &Path,
    relative_directory: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
    recursive: bool,
) -> Result<(), String> {
    let mut children = bounded_directory_entries(
        directory,
        scanned_directory_entries,
        FILE_LIST_MAX_ENTRIES,
        docs_scan_cap_error,
    )?;
    children.sort_by(|left, right| {
        let left_is_dir = left.metadata().is_ok_and(|metadata| metadata.is_dir());
        let right_is_dir = right.metadata().is_ok_and(|metadata| metadata.is_dir());
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    let mut directories = Vec::new();
    for child in children {
        if entries.len() >= FILE_LIST_MAX_ENTRIES {
            return Err(docs_scan_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" {
            continue;
        }
        let Ok(metadata) = child.metadata() else {
            continue;
        };
        let is_directory = metadata.is_dir();
        if is_directory && IGNORED_DIRECTORY_NAMES.contains(&name.as_str()) {
            continue;
        }
        let Ok(resolved) = fs::canonicalize(child.path()) else {
            continue;
        };
        if !resolved.starts_with(root) {
            continue;
        }
        let relative_path = format!("{relative_directory}/{name}");
        entries.push(file_entry(
            depth,
            if is_directory { "directory" } else { "file" },
            &name,
            &relative_path,
            &metadata,
        ));
        if is_directory && child.file_type().is_ok_and(|kind| kind.is_symlink()) {
            if let Some(entry) = entries.last_mut().and_then(Value::as_object_mut) {
                entry.insert("childrenLoaded".to_string(), Value::Bool(true));
            }
        }
        if is_directory
            && !child
                .file_type()
                .is_ok_and(|file_type| file_type.is_symlink())
        {
            directories.push((child.path(), relative_path));
        }
    }
    if !recursive || depth >= FILE_LIST_MAX_DEPTH {
        return Ok(());
    }
    for (directory, relative_path) in directories {
        append_file_entries(
            entries,
            root,
            &directory,
            &relative_path,
            depth + 1,
            scanned_directory_entries,
            true,
        )?;
    }
    Ok(())
}

fn file_entry(
    depth: usize,
    kind: &str,
    name: &str,
    relative_path: &str,
    metadata: &fs::Metadata,
) -> Value {
    json!({
        "depth": depth,
        "kind": kind,
        "modifiedAt": modified_at(metadata),
        "name": name,
        "path": relative_path,
        "size": if metadata.is_dir() { None } else { Some(metadata.len()) },
    })
}

/// Whether Docs opens this file in its Markdown editor, the only kind the preview and save
/// limits apply to.
pub(super) fn opens_in_markdown_editor(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        matches!(
            extension.to_ascii_lowercase().as_str(),
            "md" | "markdown" | "mdown" | "mkdn" | "mdx"
        )
    })
}
