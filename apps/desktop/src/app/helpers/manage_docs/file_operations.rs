use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use super::*;
use crate::app::helpers::*;

/*
CDXC:Docs 2026-08-09:
Every response carries the path the Docs page addressed, mount segment included,
never the path relative to whichever root answered. A preview that answered with
a bare inner path would hand the page an address that means the project root
next time it is used.
*/
pub(crate) fn manage_project_file_preview(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<serde_json::Value, String> {
    manage_project_file_preview_with_baseline(context, path, true)
}

pub(crate) fn manage_project_file_preview_with_baseline(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    include_baseline: bool,
) -> Result<serde_json::Value, String> {
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to preview.".to_string());
    }
    // A file that is simply not there answers `missing` (as `manage_project_file_metadata` does),
    // so the Files view can drop a remembered tab quietly instead of showing
    // `manage_existing_url`'s outside-the-project error. SEE-ALSO: `project_file_preview_with_baseline`
    // in server/src/project_docs/file_ops.rs (remote projects).
    if !path.inner.split(['/', '\\']).any(|part| part == "..")
        && fs::symlink_metadata(path.root.join(&path.inner))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(serde_json::json!({ "missing": true, "path": path.outer }));
    }
    let target = manage_existing_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to preview.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to preview.".to_string());
    }
    let size = metadata.len();
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    if size > MANAGE_FILE_PREVIEW_MAX_BYTES && manage_file_opens_in_markdown_editor(&name) {
        return Ok(manage_unsupported_file_preview(
            "File is too large to preview.",
            &name,
            &path.outer,
            &path.display(context),
            size,
            &metadata,
        ));
    }
    let data = fs::read(&target).map_err(|_| "Could not read project file.".to_string())?;
    if data.contains(&0) {
        return Ok(manage_unsupported_file_preview(
            "Binary files are not previewed.",
            &name,
            &path.outer,
            &path.display(context),
            size,
            &metadata,
        ));
    }
    let Ok(content) = String::from_utf8(data) else {
        return Ok(manage_unsupported_file_preview(
            "This file is not valid UTF-8 text.",
            &name,
            &path.outer,
            &path.display(context),
            size,
            &metadata,
        ));
    };
    Ok(serde_json::json!({
        "content": content,
        /*
        CDXC:Docs 2026-08-09:
        `path` stays the routing address the page must send back; `displayPath`
        is the same file named the way the tree names it, so the header never
        shows the reserved mount segment.
        */
        "displayPath": path.display(context),
        "gitBaseline": if include_baseline { manage_git_baseline_payload(path.root, &target, &path.inner) } else { serde_json::Value::Null },
        "kind": "text",
        "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
        "name": name,
        "path": path.outer,
        "size": size,
    }))
}

pub(crate) fn manage_project_file_metadata(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<serde_json::Value, String> {
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to inspect.".to_string());
    }
    // CDXC:Docs 2026-10-15 WHY: the Files view's change watch has to tell a deleted file from a
    // failed check, which `manage_existing_url` cannot (it reports every unresolvable path as
    // outside the project), so a path that is simply not there answers `missing` instead of an
    // error. SEE-ALSO: `project_file_metadata` in server/src/project_docs/file_ops.rs (remote
    // projects).
    if fs::symlink_metadata(path.root.join(&path.inner))
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(serde_json::json!({ "missing": true, "path": path.outer }));
    }
    let target = manage_existing_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to inspect.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to inspect.".to_string());
    }
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    Ok(serde_json::json!({
        "kind": "text",
        "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
        "name": name,
        "path": path.outer,
        "size": metadata.len(),
    }))
}

pub(crate) fn manage_unsupported_file_preview(
    error: &str,
    name: &str,
    relative_path: &str,
    display_path: &str,
    size: u64,
    metadata: &fs::Metadata,
) -> serde_json::Value {
    serde_json::json!({
        "displayPath": display_path,
        "error": error,
        "kind": "unsupported",
        "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
        "name": name,
        "path": relative_path,
        "size": size,
    })
}

pub(crate) fn manage_save_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    content: Option<&str>,
) -> Result<serde_json::Value, String> {
    let content = content.ok_or_else(|| "No file content was provided.".to_string())?;
    if content.len() > MANAGE_FILE_SAVE_MAX_BYTES
        && path.is_some_and(manage_file_opens_in_markdown_editor)
    {
        return Err("File is too large to save from the Files view.".to_string());
    }
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to save.".to_string());
    }
    let target = manage_writable_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    if path.chat && context.roots.chat_file_name.as_deref() != Some(path.inner.as_str()) {
        return Err("Only the document explicitly opened from chat can be saved.".to_string());
    }
    if fs::metadata(&target)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("Select a file to save.".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "Select a file to save.".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "Could not save project file.".to_string())?;
    let temp = parent.join(format!(
        ".ghostex-gpui-manage-save-{}.tmp",
        system_time_epoch_millis_string(std::time::SystemTime::now())
    ));
    fs::write(&temp, content).map_err(|_| "Could not save project file.".to_string())?;
    fs::rename(&temp, &target).map_err(|_| "Could not save project file.".to_string())?;
    manage_project_file_preview(context, Some(&path.outer))
}

pub(crate) fn manage_rename_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    /*
    macOS `manageRenameProjectFile` parity: same-parent rename of a Docs file
    or folder (or a root artifact file), never a move API, never an overwrite,
    with sanitized errors only.
    */
    let source_path = manage_docs_path(context, path)?;
    let destination_path = manage_docs_path(context, new_path)?;
    manage_require_same_docs_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || manage_path_is_docs_root_node(&source_path, context)
        || manage_path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select an item to rename.".to_string());
    }
    let source = manage_operation_url(&source_path)?;
    let destination = manage_operation_url(&destination_path)?;
    manage_validate_docs_action_relative_path(&source_path, context)?;
    manage_validate_docs_action_relative_path(&destination_path, context)?;
    if manage_parent_relative_path(&source_path.inner)
        != manage_parent_relative_path(&destination_path.inner)
    {
        return Err("Rename cannot move items.".to_string());
    }
    let source_metadata =
        fs::metadata(&source).map_err(|_| "Select an item to rename.".to_string())?;
    let source_is_directory = source_metadata.is_dir();
    if !source_path.extra
        && manage_is_root_artifact_file_relative_path(&source_path.inner)
        && source_is_directory
    {
        return Err("Select a file to rename.".to_string());
    }
    if source_path.outer == destination_path.outer {
        if !source_is_directory {
            return manage_project_file_preview(context, Some(&source_path.outer));
        }
        return Ok(serde_json::Value::Null);
    }
    manage_require_existing_destination_parent(source_path.root, &destination)
        .map_err(|_| "Rename target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(&source, &destination).map_err(|_| "Could not rename item.".to_string())?;
    if source_is_directory {
        return Ok(serde_json::Value::Null);
    }
    manage_project_file_preview(context, Some(&destination_path.outer))
}

pub(crate) fn manage_delete_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<(), String> {
    /*
    Files or folders inside Docs scan roots (recursive for folders), the scan
    roots themselves, and root artifact files are deletable. The mounted Docs
    directory remains protected because its routed inner path is empty.
    */
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select an item to delete.".to_string());
    }
    let target = manage_operation_url(&path)?;
    manage_validate_docs_action_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select an item to delete.".to_string())?;
    let is_directory = metadata.is_dir();
    if !path.extra && manage_is_root_artifact_file_relative_path(&path.inner) && is_directory {
        return Err("Select a file to delete.".to_string());
    }
    let removed = if is_directory {
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
    };
    removed.map_err(|_| "Could not delete item.".to_string())
}

pub(crate) fn manage_duplicate_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    // macOS `manageDuplicateProjectFile` parity: file-only same-folder copy;
    // the page chooses the " (n)" suffix, native rejects overwrites.
    let source_path = manage_docs_path(context, path)?;
    let destination_path = manage_docs_path(context, new_path)?;
    manage_require_same_docs_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || manage_path_is_docs_root_node(&source_path, context)
        || manage_path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select a file to duplicate.".to_string());
    }
    let source = manage_operation_url(&source_path)?;
    let destination = manage_operation_url(&destination_path)?;
    manage_validate_docs_action_relative_path(&source_path, context)?;
    manage_validate_docs_action_relative_path(&destination_path, context)?;
    if manage_parent_relative_path(&source_path.inner)
        != manage_parent_relative_path(&destination_path.inner)
    {
        return Err("Duplicate cannot move files.".to_string());
    }
    let source_metadata =
        fs::metadata(&source).map_err(|_| "Select a file to duplicate.".to_string())?;
    if source_metadata.is_dir() {
        return Err("Select a file to duplicate.".to_string());
    }
    manage_require_existing_destination_parent(source_path.root, &destination)
        .map_err(|_| "Duplicate target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file with that name already exists.".to_string());
    }
    fs::copy(&source, &destination).map_err(|_| "Could not duplicate file.".to_string())?;
    manage_project_file_preview(context, Some(&destination_path.outer))
}

pub(crate) fn manage_create_project_folder(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<(), String> {
    // macOS `manageCreateProjectFolder` parity: docs-scoped folder creation;
    // the docs/ root is created on demand, overwrites are rejected.
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() || manage_path_is_docs_root_node(&path, context) {
        return Err("Select a folder to create.".to_string());
    }
    let target = manage_operation_url(&path)?;
    manage_validate_docs_tree_relative_path(&path, context)?;
    if !path.extra
        && path
            .inner
            .starts_with(&format!("{MANAGE_DOCS_RELATIVE_PATH}/"))
    {
        let docs = path.root.join(MANAGE_DOCS_RELATIVE_PATH);
        fs::create_dir_all(&docs).map_err(|_| "Could not create folder.".to_string())?;
    }
    manage_require_existing_destination_parent(path.root, &target)
        .map_err(|_| "Folder parent is unavailable.".to_string())?;
    if target.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::create_dir(&target).map_err(|_| "Could not create folder.".to_string())
}

pub(crate) fn manage_move_project_item(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    /*
    macOS `manageMoveProjectItem` parity: drag/drop moves Docs items (and root
    artifact files) into docs-scoped destinations only, rejecting overwrites
    and directory self-nesting.
    */
    let source_path = manage_docs_path(context, path)?;
    let destination_path = manage_docs_path(context, new_path)?;
    manage_require_same_docs_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || manage_path_is_docs_root_node(&source_path, context)
        || manage_path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select an item to move.".to_string());
    }
    let source = manage_operation_url(&source_path)?;
    let destination = manage_operation_url(&destination_path)?;
    manage_validate_docs_action_relative_path(&source_path, context)?;
    manage_validate_docs_tree_relative_path(&destination_path, context)?;
    if source_path.outer == destination_path.outer {
        let is_file = fs::metadata(&source)
            .map(|metadata| !metadata.is_dir())
            .unwrap_or(false);
        if is_file {
            return manage_project_file_preview(context, Some(&source_path.outer));
        }
        return Ok(serde_json::Value::Null);
    }
    let source_metadata =
        fs::metadata(&source).map_err(|_| "Select an item to move.".to_string())?;
    let source_is_directory = source_metadata.is_dir();
    if !source_path.extra
        && manage_is_root_artifact_file_relative_path(&source_path.inner)
        && source_is_directory
    {
        return Err("Select a file to move.".to_string());
    }
    if source_is_directory
        && destination_path
            .inner
            .starts_with(&format!("{}/", source_path.inner))
    {
        return Err("Folders cannot be moved inside themselves.".to_string());
    }
    manage_require_existing_destination_parent(source_path.root, &destination)
        .map_err(|_| "Move target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(&source, &destination).map_err(|_| "Could not move item.".to_string())?;
    if source_is_directory {
        return Ok(serde_json::Value::Null);
    }
    manage_project_file_preview(context, Some(&destination_path.outer))
}

/// macOS `manageFileOperationURL` parity: resolve symlinks for the escape
/// check, but return the UNRESOLVED path so a listed symlink entry is operated
/// on as the entry itself.
pub(crate) fn manage_operation_url(path: &ManageDocsPath<'_>) -> Result<PathBuf, String> {
    let target = if path.inner.is_empty() {
        path.root.to_path_buf()
    } else {
        path.root.join(PathBuf::from(&path.inner))
    };
    if let Ok(resolved) = fs::canonicalize(&target) {
        if !path_is_inside_or_equal(&resolved, path.root) {
            return Err("Paths must stay inside the project.".to_string());
        }
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| "Paths must stay inside the project.".to_string())?;
        let nearest_existing_parent = nearest_existing_ancestor(parent)
            .ok_or_else(|| "Paths must stay inside the project.".to_string())?;
        let resolved_parent = fs::canonicalize(nearest_existing_parent)
            .map_err(|_| "Paths must stay inside the project.".to_string())?;
        if !path_is_inside_or_equal(&resolved_parent, path.root) {
            return Err("Paths must stay inside the project.".to_string());
        }
    }
    Ok(target)
}

pub(crate) fn manage_require_existing_destination_parent(
    root: &Path,
    destination: &Path,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "unavailable".to_string())?;
    let resolved_parent = fs::canonicalize(parent).map_err(|_| "unavailable".to_string())?;
    let metadata = fs::metadata(&resolved_parent).map_err(|_| "unavailable".to_string())?;
    if !metadata.is_dir() || !path_is_inside_or_equal(&resolved_parent, root) {
        return Err("unavailable".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod missing_file_tests {
    use super::*;
    use crate::app::helpers::manage_docs::docs_paths::{ManageDocsContext, ManageDocsRoots};

    /// A file deleted since the Files view last showed it answers `missing`, not the
    /// outside-the-project error; a path that leaves the project still errors.
    #[test]
    fn reading_a_deleted_file_answers_missing() {
        let project = std::env::temp_dir().join(format!("gx-missing-file-{}", std::process::id()));
        fs::create_dir_all(&project).expect("project dir");
        let roots = ManageDocsRoots {
            chat: None,
            chat_file_name: None,
            extra: None,
            project: fs::canonicalize(&project).expect("canonical project"),
        };
        let context = ManageDocsContext {
            additional_docs_folders_text: "",
            project_scope: true,
            roots: &roots,
        };
        let read = manage_project_file_preview(context, Some("LINKTEST.md")).expect("read answers");
        assert_eq!(read["missing"], true);
        assert_eq!(read["path"], "LINKTEST.md");
        let context = ManageDocsContext {
            additional_docs_folders_text: "",
            project_scope: true,
            roots: &roots,
        };
        assert!(manage_project_file_preview(context, Some("../outside.md")).is_err());
        let _ = fs::remove_dir_all(&project);
    }
}
