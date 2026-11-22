//! The desktop Files view's requests for a remote project (`scope: "project"`): one folder's
//! children and the search over the whole project.
//!
//! CDXC:Docs 2026-09-27 SEE-ALSO: `manage_list_project_directory` and `manage_search_project` in `apps/desktop/src/app/helpers/manage_docs_listing.rs` answer the same requests for a local project; `packages/project-docs/src/project.rs` holds the skip list and the search both use.

use super::*;
use serde_json::{json, Value};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
};

const SEARCH_MAX_MATCHES: usize = 500;
const SEARCH_MAX_FILTERED_FILES: usize = 3_000;
const SEARCH_MAX_SCANNED: usize = 200_000;

/// One folder's children, every file type, without the skipped folders. `path` "" lists the
/// project root plus the configured Docs directory's mount.
pub(super) fn list_project_directory(
    context: DocsContext<'_>,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let requested = string_param(params, "path").unwrap_or_default();
    if params.get("force").and_then(Value::as_bool) == Some(true) {
        ghostex_docs::directory::invalidate();
    }
    let mut entries = Vec::new();
    if requested.is_empty() {
        append_children(
            &mut entries,
            &context.roots.project,
            &context.roots.project,
            "",
            "",
            0,
        )?;
        if let Some(mount) = &context.roots.extra {
            match &mount.location {
                Ok(root) => entries.push(json!({
                    "path": EXTRA_ROOT_MOUNT_SEGMENT, "name": mount.name,
                    "displayPath": mount.name, "kind": "directory", "depth": 0, "size": Value::Null,
                    "modifiedAt": fs::metadata(root).ok().as_ref().and_then(modified_at),
                })),
                Err(error) => entries.push(unavailable_extra_root_entry(&mount.name, error)),
            }
        }
    } else {
        let path = docs_path(context, Some(&requested))?;
        let directory = existing_path(&path)?;
        let display_prefix = path.display(context);
        let depth = requested.split('/').filter(|part| !part.is_empty()).count();
        append_children(
            &mut entries,
            path.root,
            &directory,
            &path.outer,
            &display_prefix,
            depth,
        )?;
    }
    let mut revision_hash = DefaultHasher::new();
    context.roots.project.hash(&mut revision_hash);
    entries.hash(&mut revision_hash);
    let revision = format!("{:x}", revision_hash.finish());
    let unchanged = params.get("revision").and_then(Value::as_str) == Some(revision.as_str());
    let mut response = json!({"action": "list", "progressive": true, "requestId": params.get("requestId").cloned().unwrap_or(Value::Null), "path": requested, "revision": revision, "unchanged": unchanged});
    if !unchanged {
        response["entries"] = json!(entries);
    }
    Ok(response)
}

/// Appends `directory`'s children, folders first, as entries addressed `outer_prefix/name` and
/// named `display_prefix/name`. A symlink that leads outside `root` is left out.
fn append_children(
    entries: &mut Vec<Value>,
    root: &Path,
    directory: &Path,
    outer_prefix: &str,
    display_prefix: &str,
    depth: usize,
) -> Result<(), String> {
    let mut scanned = 0;
    let mut children = ghostex_docs::directory::children(
        directory,
        &mut scanned,
        FILE_LIST_MAX_ENTRIES,
        docs_scan_cap_error,
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
        if (is_directory && ghostex_docs::project::is_skipped_directory(&name))
            || ghostex_docs::project::is_skipped_file(&name)
        {
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
            "modifiedAt": modified_at(&metadata),
            "name": name,
            "path": join(outer_prefix, &name),
            "size": if is_directory { None } else { Some(metadata.len()) },
        }));
    }
    Ok(())
}

/// Every file and folder in the project (and the configured Docs directory) whose path holds all
/// of the query's words, as flat entries.
pub(super) fn search_project(
    context: DocsContext<'_>,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let query = string_param(params, "query").unwrap_or_default();
    // The desktop files list's type filter: only these extensions, and with no query every such
    // file (SEE-ALSO: `manage_search_project` in apps/desktop/src/app/helpers/manage_docs_listing.rs).
    let extensions: Vec<String> = params
        .get("extensions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_ascii_lowercase)
        .collect();
    let max_matches = if query.trim().is_empty() && !extensions.is_empty() {
        SEARCH_MAX_FILTERED_FILES
    } else {
        SEARCH_MAX_MATCHES
    };
    let mut roots = vec![(context.roots.project.clone(), String::new(), String::new())];
    if let Some(mount) = &context.roots.extra {
        if let Ok(location) = &mount.location {
            roots.push((
                location.clone(),
                EXTRA_ROOT_MOUNT_SEGMENT.to_string(),
                mount.name.clone(),
            ));
        }
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
            SEARCH_MAX_SCANNED,
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
        "requestId": params.get("requestId").cloned().unwrap_or(Value::Null),
        "truncated": truncated,
        "incomplete": incomplete,
    }))
}

/// The Files view's Search sidebar for a remote project: every matching line of the project's
/// files, answered whole (the desktop streams a local project's search itself).
///
/// SEE-ALSO: packages/project-docs/src/content_search.rs (the engine and its file rules).
pub(super) fn search_content(
    context: DocsContext<'_>,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let options = ghostex_docs::content_search::options_from_json(&Value::Object(params.clone()));
    let mut response =
        ghostex_docs::content_search::search_content_json(&context.roots.project, &options)?;
    response["action"] = json!("searchContent");
    response["requestId"] = params.get("requestId").cloned().unwrap_or(Value::Null);
    Ok(response)
}
