use super::*;
use serde_json::{Value, json};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
};

/// CDXC:Docs 2026-09-11 DECISION:
/// User approved progressive folder loading with background indexing for complete search, while keeping existing Docs actions working.
/// Unloaded directories remain expandable; only a completed directory request can establish that one is empty.
pub(crate) fn manage_list_directory(
    context: ManageDocsContext<'_>,
    request: &Value,
) -> Result<Value, String> {
    let requested = manage_request_string(request, "path").unwrap_or_default();
    if request.get("force").and_then(Value::as_bool) == Some(true) {
        ghostex_docs::directory::invalidate();
    }
    let mut entries = Vec::new();
    let mut scanned = 0;
    if requested.is_empty() {
        entries = manage_project_root_file_entries(&context.roots.project, context, false)?;
        if let Some(mount) = &context.roots.extra {
            match &mount.location {
                Ok(root) => entries.push(json!({
                    "path": MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT, "name": mount.name,
                    "displayPath": mount.name, "kind": "directory", "depth": 0, "size": Value::Null,
                    "modifiedAt": fs::metadata(root).ok().and_then(|m| m.modified().ok()).map(gpui_iso8601_utc),
                })),
                Err(error) => {
                    let mut entry = manage_unavailable_docs_extra_root_entry(&mount.name, error);
                    entry["childrenLoaded"] = json!(true);
                    entries.push(entry);
                }
            }
        }
    } else {
        let path = manage_docs_path(context, Some(&requested))?;
        if path.chat {
            return Err("Chat-opened files are not in the Files tree.".to_string());
        }
        manage_validate_docs_tree_relative_path(&path, context)?;
        let directory = manage_existing_url(&path)?;
        if path.extra {
            let depth = path.inner.split('/').filter(|s| !s.is_empty()).count() + 1;
            manage_append_docs_tree_entries(
                &mut entries,
                path.root,
                &directory,
                &path.outer,
                depth,
                &mut scanned,
                false,
            )?;
            if let Some(mount) = &context.roots.extra {
                manage_name_docs_extra_root_tree_entries(&mut entries, &mount.name);
            }
        } else {
            let parts: Vec<_> = path.inner.split('/').collect();
            let root_len = (1..=parts.len())
                .find(|count| {
                    manage_path_is_docs_scan_root(
                        &parts[..*count].join("/"),
                        context.additional_docs_folders_text,
                    )
                })
                .ok_or_else(|| "Select a folder to list.".to_string())?;
            manage_append_project_file_entries(
                &mut entries,
                path.root,
                &directory,
                &path.outer,
                parts.len() - root_len + 1,
                &mut scanned,
                false,
            )?;
        }
    }
    let mut scope_hash = DefaultHasher::new();
    context.roots.project.hash(&mut scope_hash);
    context.additional_docs_folders_text.hash(&mut scope_hash);
    if let Some(mount) = &context.roots.extra {
        format!("{:?}:{}", mount.location, mount.name).hash(&mut scope_hash);
    }
    let scope = format!("{:x}", scope_hash.finish());
    let mut revision_hash = DefaultHasher::new();
    scope.hash(&mut revision_hash);
    entries.hash(&mut revision_hash);
    let revision = format!("{:x}", revision_hash.finish());
    let unchanged = request.get("revision").and_then(Value::as_str) == Some(revision.as_str());
    let mut response = json!({"action": "list", "progressive": true, "requestId": request["requestId"], "rootName": MANAGE_DOCS_RELATIVE_PATH, "path": requested, "scope": scope, "revision": revision, "unchanged": unchanged, "scannedEntries": scanned.max(entries.len())});
    if !unchanged {
        response["entries"] = json!(entries);
    }
    Ok(response)
}

pub(crate) fn manage_file_git_baseline(
    context: ManageDocsContext<'_>,
    requested: Option<&str>,
) -> Result<Value, String> {
    let path = manage_docs_path(context, requested)?;
    let target = manage_existing_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    if !target.is_file() {
        return Err("Select a file to inspect.".to_string());
    }
    Ok(manage_git_baseline_payload(path.root, &target, &path.inner))
}

/// The Files view's listing: one folder's children, every file type, without what
/// `ghostex_docs::project::is_hidden_entry` hides (the skipped folders, and all of `.ghostex` but
/// its `drawings`). `path` "" lists the project root plus the configured Docs directory's mount.
///
/// CDXC:Docs 2026-09-27 SEE-ALSO: `list_project_directory` in `server/src/project_docs/project_scope.rs` answers the same request for a remote project; `packages/project-docs/src/project.rs` holds the skip list both use.
pub(crate) fn manage_list_project_directory(
    context: ManageDocsContext<'_>,
    request: &Value,
) -> Result<Value, String> {
    let requested = manage_request_string(request, "path").unwrap_or_default();
    if request.get("force").and_then(Value::as_bool) == Some(true) {
        ghostex_docs::directory::invalidate();
    }
    let mut entries = Vec::new();
    let drawings = context.roots.drawings.as_deref();
    if let (Some(drawings), true) = (
        drawings,
        requested == ghostex_docs::project::GHOSTEX_FOLDER_NAME,
    ) {
        // No project's `.ghostex` holds only its drawings folder (`~/.ghostex/drawings`).
        entries.push(manage_drawings_folder_entry(
            ghostex_docs::project::DRAWINGS_FOLDER_NAME,
            ghostex_docs::project::DRAWINGS_RELATIVE_PATH,
            drawings,
            1,
        ));
    } else if requested.is_empty() {
        manage_append_project_scope_children(
            &mut entries,
            &context.roots.project,
            &context.roots.project,
            "",
            "",
            "",
            0,
        )?;
        if let Some(drawings) = drawings {
            let ghostex = ghostex_docs::project::GHOSTEX_FOLDER_NAME;
            entries.retain(|entry| entry["path"] != ghostex);
            entries.insert(
                0,
                manage_drawings_folder_entry(ghostex, ghostex, drawings, 0),
            );
        }
        if let Some(mount) = &context.roots.extra {
            match &mount.location {
                Ok(root) => entries.push(json!({
                    "path": MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT, "name": mount.name,
                    "displayPath": mount.name, "kind": "directory", "depth": 0, "size": Value::Null,
                    "modifiedAt": fs::metadata(root).ok().and_then(|m| m.modified().ok()).map(gpui_iso8601_utc),
                })),
                Err(error) => entries.push(manage_unavailable_docs_extra_root_entry(&mount.name, error)),
            }
        }
    } else {
        let path = manage_docs_path(context, Some(&requested))?;
        if path.chat {
            return Err(
                "Files opened from outside the project have no folder to list.".to_string(),
            );
        }
        let directory = manage_existing_url(&path)?;
        let display_prefix = path.display(context);
        let depth = requested.split('/').filter(|part| !part.is_empty()).count();
        manage_append_project_scope_children(
            &mut entries,
            path.root,
            &directory,
            &path.inner,
            &path.outer,
            &display_prefix,
            depth,
        )?;
    }
    let mut revision_hash = DefaultHasher::new();
    context.roots.project.hash(&mut revision_hash);
    entries.hash(&mut revision_hash);
    let revision = format!("{:x}", revision_hash.finish());
    let unchanged = request.get("revision").and_then(Value::as_str) == Some(revision.as_str());
    let mut response = json!({"action": "list", "progressive": true, "requestId": request["requestId"], "path": requested, "revision": revision, "unchanged": unchanged});
    if !unchanged {
        response["entries"] = json!(entries);
    }
    Ok(response)
}

/// A folder row No project shows for its drawings (`.ghostex` and `.ghostex/drawings`), dated
/// by the drawings folder.
fn manage_drawings_folder_entry(name: &str, path: &str, drawings: &Path, depth: usize) -> Value {
    json!({
        "depth": depth,
        "displayPath": path,
        "kind": "directory",
        "modifiedAt": fs::metadata(drawings).ok().and_then(|m| m.modified().ok()).map(gpui_iso8601_utc),
        "name": name,
        "path": path,
        "size": Value::Null,
    })
}

/// Appends `directory`'s children, folders first, as entries addressed `outer_prefix/name` and
/// named `display_prefix/name`. A symlink that leads outside `root` is left out.
fn manage_append_project_scope_children(
    entries: &mut Vec<Value>,
    root: &Path,
    directory: &Path,
    parent: &str,
    outer_prefix: &str,
    display_prefix: &str,
    depth: usize,
) -> Result<(), String> {
    let mut scanned = 0;
    let mut children = ghostex_docs::directory::children(
        directory,
        &mut scanned,
        MANAGE_FILE_LIST_MAX_ENTRIES,
        manage_docs_scan_cap_error,
    )?;
    children.sort_by_cached_key(|child| {
        let is_directory = child.metadata().map(|m| m.is_dir()).unwrap_or(false);
        (
            !is_directory,
            child.file_name().to_string_lossy().to_lowercase(),
        )
    });
    let join = |prefix: &str, name: &str| {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        }
    };
    for child in children {
        let name = child.file_name().to_string_lossy().into_owned();
        let Ok(metadata) = child.metadata() else {
            continue;
        };
        let is_directory = metadata.is_dir();
        if ghostex_docs::project::is_hidden_entry(root, parent, &name, is_directory) {
            continue;
        }
        if child.file_type().is_ok_and(|kind| kind.is_symlink())
            && !fs::canonicalize(child.path()).is_ok_and(|resolved| resolved.starts_with(root))
        {
            continue;
        }
        entries.push(json!({
            "depth": depth,
            "displayPath": join(display_prefix, &name),
            "kind": if is_directory { "directory" } else { "file" },
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": join(outer_prefix, &name),
            "size": if is_directory { None } else { Some(metadata.len()) },
        }));
    }
    Ok(())
}

/// The Files view's search: every file and folder in the project (and the configured Docs
/// directory) whose path holds all of the query's words, as flat entries.
pub(crate) fn manage_search_project(
    context: ManageDocsContext<'_>,
    request: &Value,
) -> Result<Value, String> {
    const MAX_MATCHES: usize = 500;
    const MAX_FILTERED_FILES: usize = 3_000;
    const MAX_SCANNED: usize = 200_000;
    let query = manage_request_string(request, "query").unwrap_or_default();
    // The files list's type filter (`native_docs/type_filter.rs`): only these extensions, and with
    // no query every such file, so the list can draw them as a tree.
    let extensions: Vec<String> = request["extensions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_ascii_lowercase)
        .collect();
    let max_matches = if query.trim().is_empty() && !extensions.is_empty() {
        MAX_FILTERED_FILES
    } else {
        MAX_MATCHES
    };
    let mut roots = vec![(context.roots.project.clone(), String::new(), String::new())];
    if let Some(drawings) = &context.roots.drawings {
        let prefix = ghostex_docs::project::DRAWINGS_RELATIVE_PATH.to_string();
        roots.push((drawings.clone(), prefix.clone(), prefix));
    }
    if let Some(mount) = &context.roots.extra
        && let Ok(location) = &mount.location
    {
        roots.push((
            location.clone(),
            MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT.to_string(),
            mount.name.clone(),
        ));
    }
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut incomplete = false;
    for (root, outer_prefix, display_prefix) in roots {
        let outcome = ghostex_docs::project::search_with_extensions(
            &root,
            &query,
            &extensions,
            max_matches,
            MAX_SCANNED,
        );
        truncated |= outcome.truncated;
        incomplete |= outcome.incomplete;
        for found in outcome.matches {
            let prefixed = |prefix: &str| {
                if prefix.is_empty() {
                    found.relative_path.clone()
                } else {
                    format!("{prefix}/{}", found.relative_path)
                }
            };
            let name = found
                .relative_path
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            entries.push(json!({
                "depth": 0,
                "displayPath": prefixed(&display_prefix),
                "kind": if found.is_directory { "directory" } else { "file" },
                "modifiedAt": Value::Null,
                "name": name,
                "path": prefixed(&outer_prefix),
                "size": Value::Null,
            }));
        }
    }
    Ok(json!({
        "action": "search",
        "entries": entries,
        "query": query,
        "requestId": request["requestId"],
        "truncated": truncated,
        "incomplete": incomplete,
    }))
}
