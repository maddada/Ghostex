use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use super::*;

/*
CDXC:Docs 2026-08-09:
Every response carries the path the Docs page addressed, mount segment
included, never the path relative to whichever root answered. A preview that
answered with a bare inner path would hand the page an address that means the
project root next time it is used.
*/
pub(super) fn project_file_preview(
    context: DocsContext<'_>,
    path: Option<&str>,
) -> Result<Value, String> {
    project_file_preview_with_baseline(context, path, true)
}

pub(super) fn project_file_preview_with_baseline(
    context: DocsContext<'_>,
    path: Option<&str>,
    include_baseline: bool,
) -> Result<Value, String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to preview.".to_string());
    }
    // A file that is simply not there answers `missing` (as `project_file_metadata` does), so the
    // Files view can drop a remembered tab quietly instead of showing `existing_path`'s
    // outside-the-project error. SEE-ALSO: `manage_project_file_preview_with_baseline`.
    if !path.inner.split(['/', '\\']).any(|part| part == "..")
        && fs::symlink_metadata(path.root.join(&path.inner))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(json!({ "missing": true, "path": path.outer }));
    }
    let target = existing_path(&path)?;
    validate_accessible_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to preview.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to preview.".to_string());
    }
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if metadata.len() > FILE_PREVIEW_MAX_BYTES && opens_in_markdown_editor(name) {
        return Ok(unsupported_preview(
            "File is too large to preview.",
            name,
            &path.outer,
            &path.display(context),
            &metadata,
        ));
    }
    let data = fs::read(&target).map_err(|_| "Could not read project file.".to_string())?;
    if data.contains(&0) {
        return Ok(unsupported_preview(
            "Binary files are not previewed.",
            name,
            &path.outer,
            &path.display(context),
            &metadata,
        ));
    }
    let Ok(content) = String::from_utf8(data) else {
        return Ok(unsupported_preview(
            "This file is not valid UTF-8 text.",
            name,
            &path.outer,
            &path.display(context),
            &metadata,
        ));
    };
    Ok(json!({
        "content": content,
        /*
        CDXC:Docs 2026-08-09:
        `path` stays the routing address the page must send back; `displayPath`
        is the same file named the way the tree names it, so the header never
        shows the reserved mount segment. Mirrors gpui/src/main.rs.
        */
        "displayPath": path.display(context),
        "gitBaseline": if include_baseline { git_baseline(path.root, &target, &path.inner) } else { Value::Null },
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": name,
        "path": path.outer,
        "size": metadata.len(),
    }))
}

pub(super) fn project_file_metadata(
    context: DocsContext<'_>,
    path: Option<&str>,
) -> Result<Value, String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to inspect.".to_string());
    }
    // CDXC:Docs 2026-10-15 WHY: the Files view's change watch has to tell a deleted file from a
    // failed check, which `existing_path` cannot (it reports every unresolvable path as outside
    // the project), so a path that is simply not there answers `missing` instead of an error.
    // SEE-ALSO: `manage_project_file_metadata` in apps/desktop/src/app/helpers/manage_docs/
    // file_operations.rs (local projects, served in-process by the desktop).
    if fs::symlink_metadata(path.root.join(&path.inner))
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(json!({ "missing": true, "path": path.outer }));
    }
    let target = existing_path(&path)?;
    validate_accessible_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to inspect.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to inspect.".to_string());
    }
    Ok(json!({
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": target.file_name().and_then(|name| name.to_str()).unwrap_or(""),
        "path": path.outer,
        "size": metadata.len(),
    }))
}

fn unsupported_preview(
    error: &str,
    name: &str,
    path: &str,
    display_path: &str,
    metadata: &fs::Metadata,
) -> Value {
    json!({
        "displayPath": display_path,
        "error": error,
        "kind": "unsupported",
        "modifiedAt": modified_at(metadata),
        "name": name,
        "path": path,
        "size": metadata.len(),
    })
}

pub(super) fn save_project_file(
    context: DocsContext<'_>,
    path: Option<&str>,
    content: Option<&str>,
) -> Result<Value, String> {
    let content = content.ok_or_else(|| "No file content was provided.".to_string())?;
    if content.len() > FILE_SAVE_MAX_BYTES && path.is_some_and(opens_in_markdown_editor) {
        return Err("File is too large to save from the Files view.".to_string());
    }
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to save.".to_string());
    }
    let target = writable_path(&path)?;
    validate_accessible_path(&path, context)?;
    if fs::metadata(&target).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("Select a file to save.".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "Select a file to save.".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "Could not save project file.".to_string())?;
    let temp = parent.join(format!(".ghostex-docs-save-{}.tmp", Uuid::new_v4()));
    fs::write(&temp, content).map_err(|_| "Could not save project file.".to_string())?;
    fs::rename(&temp, &target).map_err(|_| "Could not save project file.".to_string())?;
    project_file_preview(context, Some(&path.outer))
}

pub(super) fn rename_project_file(
    context: DocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<Option<Value>, String> {
    let source_path = docs_path(context, path)?;
    let destination_path = docs_path(context, new_path)?;
    require_same_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || path_is_docs_root_node(&source_path, context)
        || path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select an item to rename.".to_string());
    }
    let source = operation_path(&source_path)?;
    let destination = operation_path(&destination_path)?;
    validate_action_path(&source_path, context)?;
    validate_action_path(&destination_path, context)?;
    if parent_relative_path(&source_path.inner) != parent_relative_path(&destination_path.inner) {
        return Err("Rename cannot move items.".to_string());
    }
    let metadata = fs::metadata(&source).map_err(|_| "Select an item to rename.".to_string())?;
    if !source_path.extra && is_root_artifact(&source_path.inner) && metadata.is_dir() {
        return Err("Select a file to rename.".to_string());
    }
    if source_path.outer == destination_path.outer {
        return if metadata.is_dir() {
            Ok(None)
        } else {
            project_file_preview(context, Some(&source_path.outer)).map(Some)
        };
    }
    require_existing_parent(source_path.root, &destination)
        .map_err(|_| "Rename target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(&source, &destination).map_err(|_| "Could not rename item.".to_string())?;
    if metadata.is_dir() {
        Ok(None)
    } else {
        project_file_preview(context, Some(&destination_path.outer)).map(Some)
    }
}

pub(super) fn delete_project_file(
    context: DocsContext<'_>,
    path: Option<&str>,
) -> Result<(), String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select an item to delete.".to_string());
    }
    let target = operation_path(&path)?;
    validate_action_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select an item to delete.".to_string())?;
    if !path.extra && is_root_artifact(&path.inner) && metadata.is_dir() {
        return Err("Select a file to delete.".to_string());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(target)
    } else {
        fs::remove_file(target)
    }
    .map_err(|_| "Could not delete item.".to_string())
}

pub(super) fn duplicate_project_file(
    context: DocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<Value, String> {
    let source_path = docs_path(context, path)?;
    let destination_path = docs_path(context, new_path)?;
    require_same_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || path_is_docs_root_node(&source_path, context)
        || path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select a file to duplicate.".to_string());
    }
    let source = operation_path(&source_path)?;
    let destination = operation_path(&destination_path)?;
    validate_action_path(&source_path, context)?;
    validate_action_path(&destination_path, context)?;
    if parent_relative_path(&source_path.inner) != parent_relative_path(&destination_path.inner) {
        return Err("Duplicate cannot move files.".to_string());
    }
    if fs::metadata(&source)
        .map_err(|_| "Select a file to duplicate.".to_string())?
        .is_dir()
    {
        return Err("Select a file to duplicate.".to_string());
    }
    require_existing_parent(source_path.root, &destination)
        .map_err(|_| "Duplicate target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file with that name already exists.".to_string());
    }
    fs::copy(source, destination).map_err(|_| "Could not duplicate file.".to_string())?;
    project_file_preview(context, Some(&destination_path.outer))
}

pub(super) fn create_project_folder(
    context: DocsContext<'_>,
    path: Option<&str>,
) -> Result<(), String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() || path_is_docs_root_node(&path, context) {
        return Err("Select a folder to create.".to_string());
    }
    let target = operation_path(&path)?;
    validate_tree_path(&path, context)?;
    if !path.extra && path.inner.starts_with(&format!("{DOCS_RELATIVE_PATH}/")) {
        fs::create_dir_all(path.root.join(DOCS_RELATIVE_PATH))
            .map_err(|_| "Could not create folder.".to_string())?;
    }
    require_existing_parent(path.root, &target)
        .map_err(|_| "Folder parent is unavailable.".to_string())?;
    if target.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::create_dir(target).map_err(|_| "Could not create folder.".to_string())
}

pub(super) fn move_project_item(
    context: DocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<Option<Value>, String> {
    let source_path = docs_path(context, path)?;
    let destination_path = docs_path(context, new_path)?;
    require_same_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || path_is_docs_root_node(&source_path, context)
        || path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select an item to move.".to_string());
    }
    let source = operation_path(&source_path)?;
    let destination = operation_path(&destination_path)?;
    validate_action_path(&source_path, context)?;
    validate_tree_path(&destination_path, context)?;
    if source_path.outer == destination_path.outer {
        return if fs::metadata(&source).is_ok_and(|metadata| metadata.is_dir()) {
            Ok(None)
        } else {
            project_file_preview(context, Some(&source_path.outer)).map(Some)
        };
    }
    let metadata = fs::metadata(&source).map_err(|_| "Select an item to move.".to_string())?;
    if !source_path.extra && is_root_artifact(&source_path.inner) && metadata.is_dir() {
        return Err("Select a file to move.".to_string());
    }
    if metadata.is_dir()
        && destination_path
            .inner
            .starts_with(&format!("{}/", source_path.inner))
    {
        return Err("Folders cannot be moved inside themselves.".to_string());
    }
    require_existing_parent(source_path.root, &destination)
        .map_err(|_| "Move target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(source, destination).map_err(|_| "Could not move item.".to_string())?;
    if metadata.is_dir() {
        Ok(None)
    } else {
        project_file_preview(context, Some(&destination_path.outer)).map(Some)
    }
}

pub(super) fn normalized_relative_path(path: Option<&str>) -> Result<String, String> {
    let trimmed = path.unwrap_or("").trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains('\0') || trimmed.starts_with('/') || trimmed.contains('\\') {
        return Err("Paths must be project-relative.".to_string());
    }
    let components = trimmed
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| *component == "." || *component == "..")
    {
        return Err("Paths must stay inside the project.".to_string());
    }
    Ok(components.join("/"))
}

/*
CDXC:Docs 2026-08-09:
Confinement is per root and it is the root the path was ROUTED to, so a `..`
chain or an outward symlink under one mount can never surface inside the other.
*/
pub(super) fn existing_path(path: &DocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(&path.inner);
    let resolved =
        fs::canonicalize(target).map_err(|_| "Paths must stay inside the project.".to_string())?;
    if !resolved.starts_with(path.root) {
        return Err("Paths must stay inside the project.".to_string());
    }
    Ok(resolved)
}

fn writable_path(path: &DocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(&path.inner);
    let parent = target
        .parent()
        .ok_or_else(|| "Select a project file to save.".to_string())?;
    let ancestor = nearest_existing_ancestor(parent)
        .ok_or_else(|| "Paths must stay inside the project.".to_string())?;
    let resolved = fs::canonicalize(ancestor)
        .map_err(|_| "Paths must stay inside the project.".to_string())?;
    if !resolved.starts_with(path.root) {
        return Err("Paths must stay inside the project.".to_string());
    }
    Ok(target)
}

pub(super) fn operation_path(path: &DocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(&path.inner);
    if let Ok(resolved) = fs::canonicalize(&target) {
        if !resolved.starts_with(path.root) {
            return Err("Paths must stay inside the project.".to_string());
        }
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| "Paths must stay inside the project.".to_string())?;
        let ancestor = nearest_existing_ancestor(parent)
            .ok_or_else(|| "Paths must stay inside the project.".to_string())?;
        let resolved = fs::canonicalize(ancestor)
            .map_err(|_| "Paths must stay inside the project.".to_string())?;
        if !resolved.starts_with(path.root) {
            return Err("Paths must stay inside the project.".to_string());
        }
    }
    Ok(target)
}

fn nearest_existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors().find(|candidate| candidate.exists())
}

fn require_existing_parent(root: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "unavailable".to_string())?;
    let resolved = fs::canonicalize(parent).map_err(|_| "unavailable".to_string())?;
    if !fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_dir())
        || !resolved.starts_with(root)
    {
        return Err("unavailable".to_string());
    }
    Ok(())
}

fn parent_relative_path(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

pub(super) fn modified_at(metadata: &fs::Metadata) -> Option<String> {
    metadata.modified().ok().map(|time| {
        DateTime::<Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    })
}
