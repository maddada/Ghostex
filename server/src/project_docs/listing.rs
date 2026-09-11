use super::*;
use serde_json::{json, Value};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
};

/// CDXC:Docs 2026-09-11 DECISION:
/// User approved progressive folder loading with background indexing for complete search, while keeping existing Docs actions working.
/// Unloaded directories remain expandable; only a completed directory request can establish that one is empty.
pub(super) fn list_directory(
    context: DocsContext<'_>,
    request: &Map<String, Value>,
) -> Result<Value, String> {
    let requested = string_param(request, "path").unwrap_or_default();
    if request.get("force").and_then(Value::as_bool) == Some(true) {
        ghostex_docs::directory::invalidate();
    }
    let mut entries = Vec::new();
    let mut scanned = 0;
    if requested.is_empty() {
        entries = project_root_file_entries(&context.roots.project, context, false)?;
        if let Some(mount) = &context.roots.extra {
            match &mount.location {
                Ok(root) => entries.push(json!({
                    "path": EXTRA_ROOT_MOUNT_SEGMENT, "name": mount.name,
                    "displayPath": mount.name, "kind": "directory", "depth": 0, "size": Value::Null,
                    "modifiedAt": fs::metadata(root).ok().as_ref().and_then(modified_at),
                })),
                Err(error) => {
                    let mut entry = unavailable_extra_root_entry(&mount.name, error);
                    entry["childrenLoaded"] = json!(true);
                    entries.push(entry);
                }
            }
        }
    } else {
        let path = docs_path(context, Some(&requested))?;
        validate_tree_path(&path, context)?;
        let directory = existing_path(&path)?;
        if path.extra {
            let depth = path.inner.split('/').filter(|s| !s.is_empty()).count() + 1;
            append_docs_tree_entries(
                &mut entries,
                path.root,
                &directory,
                &path.outer,
                depth,
                &mut scanned,
                false,
            )?;
            if let Some(mount) = &context.roots.extra {
                name_extra_root_tree_entries(&mut entries, &mount.name);
            }
        } else {
            let parts: Vec<_> = path.inner.split('/').collect();
            let root_len = (1..=parts.len())
                .find(|count| {
                    path_is_scan_root(&parts[..*count].join("/"), context.additional_docs_folders)
                })
                .ok_or_else(|| "Select a Docs folder to list.".to_string())?;
            append_file_entries(
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
    context.additional_docs_folders.hash(&mut scope_hash);
    if let Some(mount) = &context.roots.extra {
        format!("{:?}:{}", mount.location, mount.name).hash(&mut scope_hash);
    }
    let scope = format!("{:x}", scope_hash.finish());
    let mut revision_hash = DefaultHasher::new();
    scope.hash(&mut revision_hash);
    entries.hash(&mut revision_hash);
    let revision = format!("{:x}", revision_hash.finish());
    let unchanged = request.get("revision").and_then(Value::as_str) == Some(revision.as_str());
    let mut response = json!({"action": "list", "progressive": true, "requestId": request["requestId"], "rootName": DOCS_RELATIVE_PATH, "path": requested, "scope": scope, "revision": revision, "unchanged": unchanged, "scannedEntries": scanned.max(entries.len())});
    if !unchanged {
        response["entries"] = json!(entries);
    }
    Ok(response)
}

pub(super) fn file_git_baseline(
    context: DocsContext<'_>,
    requested: Option<&str>,
) -> Result<Value, String> {
    let path = docs_path(context, requested)?;
    let target = existing_path(&path)?;
    validate_accessible_path(&path, context)?;
    if !target.is_file() {
        return Err("Select a file to inspect.".to_string());
    }
    Ok(git_baseline(path.root, &target, &path.inner))
}
