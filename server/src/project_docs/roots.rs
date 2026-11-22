use serde_json::{json, Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;

/*
CDXC:Docs 2026-08-06:
Docs filesystem authority belongs to the gxserver that owns the registered
project, not to the client displaying it. This project-id-scoped operation is
the reusable data boundary for GPUI's remote Docs pane and a later web client:
callers provide only Docs action data and configured relative scan folders;
gxserver resolves and canonicalizes its own project root, applies one bounded
allowlist to every read/write operation, and returns the existing Docs bridge
response shape without exposing a generic filesystem API.
*/
pub fn run_project_docs_action(root: &ProjectDocsRoot, params: &Map<String, Value>) -> Value {
    let action = string_param(params, "action").unwrap_or_default();
    let request_id = string_param(params, "requestId").unwrap_or_default();
    let additional_docs_folders = string_param(params, "additionalDocsFolders").unwrap_or_default();
    let result = run_action(root, params, &additional_docs_folders);
    match result {
        Ok(response) => response,
        Err(error) => json!({
            "action": action,
            "error": error,
            "requestId": request_id,
        }),
    }
}

/*
CDXC:Docs 2026-08-09:
The Docs directory is the project's own, then the Docs directory Global Default,
then none at all. Callers resolve it here so the daemon and its remote clients
agree on one cascade, and so `run_project_docs_action` keeps taking a plain root.

CDXC:Docs 2026-08-09:
A configured directory is mounted IN ADDITION to the project root rather than
replacing it, so a path that cannot be opened no longer fails the whole panel.
It becomes an unavailable mount instead: the project's own docs still list and
the mount node names the path that failed. That is still not a silent fallback —
a silent revert reads exactly like "my vault is empty" and hides the typo that
caused it.
*/
pub fn resolve_project_docs_root(project: &Value, project_path: &str) -> ProjectDocsRoot {
    let configured = crate::global_project_defaults::resolve_with_global_default(
        project
            .get("projectBoardConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("docsDirectory"))
            .and_then(Value::as_str),
        &crate::global_project_defaults::read_global_project_defaults().docs_directory,
    );
    ProjectDocsRoot {
        project_path: PathBuf::from(project_path),
        extra: configured.as_deref().map(resolve_project_docs_extra_root),
    }
}

/// Absolute (after expanding a leading `~`) and an existing folder. A failure
/// is carried, not raised: it labels this one mount and leaves the project's
/// own docs listing.
fn resolve_project_docs_extra_root(configured: &str) -> ProjectDocsExtraRoot {
    let path = expanded_docs_directory_path(configured);
    let error = if !path.is_absolute() {
        Some(format!(
            "Docs directory must be an absolute path: {configured}"
        ))
    } else {
        match fs::metadata(&path) {
            Err(_) => Some(format!("Docs directory does not exist: {}", path.display())),
            Ok(metadata) if !metadata.is_dir() => Some(format!(
                "Docs directory is not a folder: {}",
                path.display()
            )),
            Ok(_) => None,
        }
    };
    ProjectDocsExtraRoot { path, error }
}

/*
CDXC:Docs 2026-08-09:
Docs mounts two roots, never one. The project root is always present and keeps
the docs/-plus-root-artifacts discovery it has always had; a configured Docs
directory is mounted in addition, as a single top-level folder named after
itself that expands to its whole recursive tree. Pointing Docs at a vault
therefore ADDS the vault to the panel instead of hiding the repo's own
CLAUDE.md.
*/
pub struct ProjectDocsRoot {
    pub project_path: PathBuf,
    pub extra: Option<ProjectDocsExtraRoot>,
}

/// A configured Docs directory. `error` is set when the folder could not be
/// used, which labels its mount node instead of failing the whole listing.
pub struct ProjectDocsExtraRoot {
    pub path: PathBuf,
    pub error: Option<String>,
}

/// The two mounted roots as one request sees them, canonicalized.
pub(super) struct DocsRoots {
    pub(super) project: PathBuf,
    pub(super) extra: Option<DocsExtraMount>,
}

/// The mounted Docs directory: what the tree calls it, and either where it is
/// or why it could not be opened.
pub(super) struct DocsExtraMount {
    pub(super) location: Result<PathBuf, String>,
    pub(super) name: String,
}

/// Everything a Docs operation needs: both mounted roots, and the project's
/// configured Docs folders. Carried together so no operation can resolve
/// against one root while validating against the other.
#[derive(Clone, Copy)]
pub(super) struct DocsContext<'a> {
    pub(super) additional_docs_folders: &'a str,
    /// The desktop's Files view (`scope: "project"`): the whole project and every file type.
    pub(super) project_scope: bool,
    pub(super) roots: &'a DocsRoots,
}

/// A Docs path that has been routed to its root. `outer` is what the Docs page
/// addresses, `inner` is what the filesystem under `root` sees, and `extra`
/// records which root answered.
pub(super) struct DocsPath<'a> {
    pub(super) extra: bool,
    pub(super) inner: String,
    pub(super) outer: String,
    pub(super) root: &'a Path,
}

impl DocsPath<'_> {
    /// What a human is shown: the mount's own name, never the reserved segment.
    pub(super) fn display(&self, context: DocsContext<'_>) -> String {
        let Some(mount) = context.roots.extra.as_ref().filter(|_| self.extra) else {
            return self.outer.clone();
        };
        if self.inner.is_empty() {
            mount.name.clone()
        } else {
            format!("{}/{}", mount.name, self.inner)
        }
    }
}

pub(super) fn docs_roots(root: &ProjectDocsRoot) -> Result<DocsRoots, String> {
    Ok(DocsRoots {
        project: project_root(&root.project_path)?,
        extra: root.extra.as_ref().map(docs_extra_mount),
    })
}

fn docs_extra_mount(extra: &ProjectDocsExtraRoot) -> DocsExtraMount {
    let name = extra
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| extra.path.to_string_lossy().into_owned());
    let location = match &extra.error {
        Some(error) => Err(error.clone()),
        None => fs::canonicalize(&extra.path)
            .map_err(|_| format!("Docs directory is unavailable: {}", extra.path.display())),
    };
    DocsExtraMount { location, name }
}

/// Route a Docs path to the root it names. The reserved mount segment is the
/// whole routing vocabulary; anything else is project-relative.
pub(super) fn docs_path<'a>(
    context: DocsContext<'a>,
    path: Option<&str>,
) -> Result<DocsPath<'a>, String> {
    let outer = normalized_relative_path(path)?;
    let Some(inner) = extra_root_relative_path(&outer) else {
        return Ok(DocsPath {
            extra: false,
            inner: outer.clone(),
            outer,
            root: context.roots.project.as_path(),
        });
    };
    let mount = context
        .roots
        .extra
        .as_ref()
        .ok_or_else(|| "No Docs directory is configured.".to_string())?;
    let root = mount.location.as_deref().map_err(|error| error.clone())?;
    Ok(DocsPath {
        extra: true,
        inner,
        outer,
        root,
    })
}

/// `Some(inner path)` when the path addresses the mounted Docs directory.
fn extra_root_relative_path(outer: &str) -> Option<String> {
    if outer == EXTRA_ROOT_MOUNT_SEGMENT {
        return Some(String::new());
    }
    outer
        .strip_prefix(&format!("{EXTRA_ROOT_MOUNT_SEGMENT}/"))
        .map(str::to_string)
}

fn expanded_docs_directory_path(configured: &str) -> PathBuf {
    let Some(rest) = configured.strip_prefix('~') else {
        return PathBuf::from(configured);
    };
    let home = crate::paths::get_gxserver_paths(None).home_dir.clone();
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home
    } else {
        home.join(rest)
    }
}
