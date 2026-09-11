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
            return Err("Chat-opened files are not Docs tree items.".to_string());
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
                .ok_or_else(|| "Select a Docs folder to list.".to_string())?;
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
