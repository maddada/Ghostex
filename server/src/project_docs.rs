use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use uuid::Uuid;

const FILE_LIST_MAX_ENTRIES: usize = 1_200;
const FILE_LIST_MAX_DEPTH: usize = 8;
/*
CDXC:DocsRootRecursive 2026-08-09:
A mounted Docs directory is a notes tree, not a repo, so it gets its own far
larger bounds. They are still bounds: a directory pointed at a home folder must
fail loudly naming the cap instead of walking forever or returning a tree that
silently stops.
*/
const DOCS_TREE_MAX_ENTRIES: usize = 20_000;
const DOCS_TREE_MAX_DEPTH: usize = 12;
const FILE_PREVIEW_MAX_BYTES: u64 = 2_000_000;
pub const FILE_SAVE_MAX_BYTES: usize = 2_000_000;
const GIT_BASELINE_MAX_BYTES: usize = 1024 * 1024;
const RESOURCE_MAX_BYTES: u64 = 12 * 1024 * 1024;
const SESSION_CONTEXT_MAX_BYTES: usize = 300_000;
const DOCS_RELATIVE_PATH: &str = "docs";
const BUILT_IN_DOCS_RELATIVE_PATHS: &[&str] = &[DOCS_RELATIVE_PATH, "artifacts", "ai"];
/*
CDXC:DocsRootAdditive 2026-08-09:
The reserved first path segment that addresses the mounted Docs directory.
Every other Docs path is project-relative, so one relative path can only ever
mean one root and no read, save, rename, delete, move, or reveal can resolve
out of the root it was addressed to. The cost of that guarantee is that a
project folder with this exact name is not reachable from Docs, which is a
better trade than a vault named `docs` quietly shadowing the repo's own.
*/
const EXTRA_ROOT_MOUNT_SEGMENT: &str = ".ghostex-docs-root";
const ANNOTATIONS_SIDECAR_RELATIVE_PATH: &str = ".ghostex/manage-annotations.json";
const ROOT_ARTIFACT_FILE_EXTENSIONS: &[&str] = &[
    "excalidraw",
    "htm",
    "html",
    "markdown",
    "md",
    "mdown",
    "mkdn",
];
const IGNORED_DIRECTORY_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".ghostex",
    ".gradle",
    ".next",
    ".nuxt",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".turbo",
    ".tox",
    ".venv",
    ".vite",
    "DerivedData",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "storybook-static",
    "target",
    "tmp",
    "venv",
    "zig-out",
];

/*
CDXC:RemoteProjectDocs 2026-08-06:
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
CDXC:DocsRootDirectory 2026-08-09:
The Docs directory is the project's own, then the Docs directory Global Default,
then none at all. Callers resolve it here so the daemon and its remote clients
agree on one cascade, and so `run_project_docs_action` keeps taking a plain root.

CDXC:DocsRootAdditive 2026-08-09:
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
        extra: configured
            .as_deref()
            .map(resolve_project_docs_extra_root),
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
            Ok(metadata) if !metadata.is_dir() => {
                Some(format!("Docs directory is not a folder: {}", path.display()))
            }
            Ok(_) => None,
        }
    };
    ProjectDocsExtraRoot { path, error }
}

/*
CDXC:DocsRootAdditive 2026-08-09:
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
struct DocsRoots {
    project: PathBuf,
    extra: Option<DocsExtraMount>,
}

/// The mounted Docs directory: what the tree calls it, and either where it is
/// or why it could not be opened.
struct DocsExtraMount {
    location: Result<PathBuf, String>,
    name: String,
}

/// Everything a Docs operation needs: both mounted roots, and the project's
/// configured Docs folders. Carried together so no operation can resolve
/// against one root while validating against the other.
#[derive(Clone, Copy)]
struct DocsContext<'a> {
    additional_docs_folders: &'a str,
    roots: &'a DocsRoots,
}

/// A Docs path that has been routed to its root. `outer` is what the Docs page
/// addresses, `inner` is what the filesystem under `root` sees, and `extra`
/// records which root answered.
struct DocsPath<'a> {
    extra: bool,
    inner: String,
    outer: String,
    root: &'a Path,
}

impl DocsPath<'_> {
    /// What a human is shown: the mount's own name, never the reserved segment.
    fn display(&self, context: DocsContext<'_>) -> String {
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

fn docs_roots(root: &ProjectDocsRoot) -> Result<DocsRoots, String> {
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
fn docs_path<'a>(context: DocsContext<'a>, path: Option<&str>) -> Result<DocsPath<'a>, String> {
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

fn run_action(
    root: &ProjectDocsRoot,
    params: &Map<String, Value>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let roots = docs_roots(root)?;
    let context = DocsContext {
        additional_docs_folders,
        roots: &roots,
    };
    let action = string_param(params, "action").unwrap_or_default();
    let request_id = string_param(params, "requestId").unwrap_or_default();
    let response = |file: Option<Value>, entries: Option<Vec<Value>>| {
        let mut response = Map::from_iter([
            ("action".to_string(), Value::String(action.clone())),
            ("requestId".to_string(), Value::String(request_id.clone())),
            (
                "rootName".to_string(),
                Value::String(DOCS_RELATIVE_PATH.to_string()),
            ),
        ]);
        if let Some(file) = file {
            response.insert("file".to_string(), file);
        }
        if let Some(entries) = entries {
            response.insert("entries".to_string(), Value::Array(entries));
        }
        Value::Object(response)
    };

    match action.as_str() {
        "list" => Ok(response(None, Some(project_file_entries(context)?))),
        "read" => Ok(response(
            Some(project_file_preview(
                context,
                string_param(params, "path").as_deref(),
            )?),
            None,
        )),
        "stat" => Ok(response(
            Some(project_file_metadata(
                context,
                string_param(params, "path").as_deref(),
            )?),
            None,
        )),
        "save" => Ok(response(
            Some(save_project_file(
                context,
                string_param(params, "path").as_deref(),
                string_param(params, "content").as_deref(),
            )?),
            None,
        )),
        "rename" => Ok(response(
            rename_project_file(
                context,
                string_param(params, "path").as_deref(),
                string_param(params, "newPath").as_deref(),
            )?,
            None,
        )),
        "duplicate" => Ok(response(
            Some(duplicate_project_file(
                context,
                string_param(params, "path").as_deref(),
                string_param(params, "newPath").as_deref(),
            )?),
            None,
        )),
        "delete" => {
            delete_project_file(context, string_param(params, "path").as_deref())?;
            Ok(response(None, None))
        }
        "createFolder" => {
            create_project_folder(context, string_param(params, "path").as_deref())?;
            Ok(response(None, None))
        }
        "move" => Ok(response(
            move_project_item(
                context,
                string_param(params, "path").as_deref(),
                string_param(params, "newPath").as_deref(),
            )?,
            None,
        )),
        "copyFullPath" => {
            let full_path = docs_action_item_path(
                context,
                string_param(params, "path").as_deref(),
                "Select an item to copy its full path.",
            )?;
            Ok(json!({
                "action": action,
                "fullPath": full_path,
                "requestId": request_id,
                "rootName": DOCS_RELATIVE_PATH,
            }))
        }
        "addToSessionContext" => Ok(json!({
            "action": action,
            "contextPrompt": session_context_prompt(
                context,
                string_param(params, "path").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": DOCS_RELATIVE_PATH,
        })),
        "readResource" => {
            let path = docs_path(context, string_param(params, "path").as_deref())?;
            if path.inner.is_empty() {
                return Err("Select a Docs resource to read.".to_string());
            }
            let target = existing_path(&path)?;
            validate_accessible_path(&path, context)?;
            let metadata =
                fs::metadata(&target).map_err(|_| "Docs resource is unavailable.".to_string())?;
            if !metadata.is_file() || metadata.len() > RESOURCE_MAX_BYTES {
                return Err("Docs resource is unavailable.".to_string());
            }
            let data = fs::read(target).map_err(|_| "Docs resource is unavailable.".to_string())?;
            Ok(json!({
                "action": action,
                "dataBase64": BASE64_STANDARD.encode(data),
                "requestId": request_id,
            }))
        }
        _ => Err("Unsupported Docs file action.".to_string()),
    }
}

fn docs_action_item<'a>(
    context: DocsContext<'a>,
    path: Option<&str>,
    unavailable_message: &str,
) -> Result<(PathBuf, DocsPath<'a>, fs::Metadata), String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err(unavailable_message.to_string());
    }
    let target = operation_path(&path)?;
    validate_action_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| unavailable_message.to_string())?;
    Ok((target, path, metadata))
}

fn docs_action_item_path(
    context: DocsContext<'_>,
    path: Option<&str>,
    unavailable_message: &str,
) -> Result<String, String> {
    let (target, _, _) = docs_action_item(context, path, unavailable_message)?;
    Ok(target.to_string_lossy().into_owned())
}

fn session_context_prompt(context: DocsContext<'_>, path: Option<&str>) -> Result<String, String> {
    let unavailable = "Select a file to add to session context.";
    let (target, path, metadata) = docs_action_item(context, path, unavailable)?;
    /*
    CDXC:DocsRootAdditive 2026-08-09:
    The prompt names the file the way the Docs tree does — the mount's own name,
    not the reserved routing segment — because this text is read by a human and
    by the agent in the terminal it is pasted into.
    */
    let relative_path = path.display(context);
    if !metadata.is_file() {
        return Err(unavailable.to_string());
    }
    if metadata.len() > SESSION_CONTEXT_MAX_BYTES as u64 {
        return Err("File is too large to add to session context.".to_string());
    }
    let data = fs::read(&target).map_err(|_| unavailable.to_string())?;
    if data.len() > SESSION_CONTEXT_MAX_BYTES {
        return Err("File is too large to add to session context.".to_string());
    }
    if data.contains(&0) {
        return Err("Only UTF-8 text files can be added to session context.".to_string());
    }
    let text = String::from_utf8(data)
        .map_err(|_| "Only UTF-8 text files can be added to session context.".to_string())?;
    let fence = session_context_fence(&text);
    let language = session_context_language(&relative_path);
    let fence_header = if language.is_empty() {
        fence.clone()
    } else {
        format!("{fence}{language}")
    };
    Ok(format!(
        "\nFile context: {relative_path}\n\n{fence_header}\n{text}\n{fence}\n"
    ))
}

fn session_context_fence(text: &str) -> String {
    let mut length = 3;
    while text.contains(&"`".repeat(length)) {
        length += 1;
    }
    "`".repeat(length)
}

fn session_context_language(relative_path: &str) -> &'static str {
    match Path::new(relative_path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("css") => "css",
        Some("excalidraw" | "json") => "json",
        Some("htm" | "html") => "html",
        Some("js" | "mjs") => "javascript",
        Some("md" | "markdown" | "mdown" | "mkdn") => "markdown",
        Some("sh" | "zsh") => "shell",
        Some("swift") => "swift",
        Some("ts" | "tsx") => "typescript",
        _ => "",
    }
}

fn string_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(str::to_string)
}

fn project_root(path: &Path) -> Result<PathBuf, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "The active project root is unavailable.".to_string())?;
    if !metadata.is_dir() {
        return Err("The active project root is unavailable.".to_string());
    }
    fs::canonicalize(path).map_err(|_| "The active project root is unavailable.".to_string())
}

/*
CDXC:DocsRootAdditive 2026-08-09:
Docs folders is project-root-relative again, the meaning it had before a custom
root ever existed. Round 2 made it narrow the custom root instead; with additive
mounting that is no longer coherent, because the mounted Docs directory always
shows its whole tree.
*/
fn additional_docs_folder_relative_paths(value: &str, docs_is_implicit_root: bool) -> Vec<String> {
    let mut folders = Vec::new();
    let mut seen = HashSet::new();
    for raw_folder in value.split(',') {
        let normalized = raw_folder.trim().replace('\\', "/");
        let parts = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty()
            || normalized.contains('\0')
            || normalized.starts_with('~')
            || normalized.starts_with('/')
            || parts.iter().any(|part| *part == "." || *part == "..")
        {
            continue;
        }
        let folder = parts.join("/");
        let key = folder.to_lowercase();
        if docs_is_implicit_root && BUILT_IN_DOCS_RELATIVE_PATHS.contains(&key.as_str()) {
            continue;
        }
        if seen.insert(key) {
            folders.push(folder);
        }
    }
    folders
}

/// The project root's scan roots: `docs` plus each configured Docs folder.
fn scan_roots(additional_docs_folders: &str) -> Vec<String> {
    let mut roots = BUILT_IN_DOCS_RELATIVE_PATHS
        .iter()
        .map(|path| (*path).to_string())
        .collect::<Vec<_>>();
    roots.extend(additional_docs_folder_relative_paths(
        additional_docs_folders,
        true,
    ));
    roots
}

fn path_is_in_scan_root(relative_path: &str, additional_docs_folders: &str) -> bool {
    scan_roots(additional_docs_folders)
        .iter()
        .any(|root| relative_path == root || relative_path.starts_with(&format!("{root}/")))
}

fn path_is_scan_root(relative_path: &str, additional_docs_folders: &str) -> bool {
    scan_roots(additional_docs_folders)
        .iter()
        .any(|root| relative_path == root)
}

/// The nodes no operation may rename, move, or delete: the project root's scan
/// roots, and the mounted Docs directory itself.
fn path_is_docs_root_node(path: &DocsPath<'_>, context: DocsContext<'_>) -> bool {
    if path.extra {
        return path.inner.is_empty();
    }
    path_is_scan_root(&path.inner, context.additional_docs_folders)
}

/// The extensions the Docs surface renders. One list for root artifacts and for
/// custom-root tree discovery, so the two can never drift apart.
fn has_docs_artifact_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        ROOT_ARTIFACT_FILE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

fn is_root_artifact(relative_path: &str) -> bool {
    if relative_path.is_empty() || relative_path.contains('/') {
        return false;
    }
    has_docs_artifact_extension(relative_path)
}

/*
CDXC:DocsRootAdditive 2026-08-09:
The mounted Docs directory serves its whole tree, so a path that routed there
needs no further allowlist: it was already confined to that root by
canonicalization. Project-root paths keep exactly the allowlist they have
always had.
*/
fn validate_accessible_path(path: &DocsPath<'_>, context: DocsContext<'_>) -> Result<(), String> {
    if path.extra
        || path.inner == ANNOTATIONS_SIDECAR_RELATIVE_PATH
        || path_is_in_scan_root(&path.inner, context.additional_docs_folders)
        || is_root_artifact(&path.inner)
    {
        return Ok(());
    }
    Err("Docs files must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files.".to_string())
}

fn validate_tree_path(path: &DocsPath<'_>, context: DocsContext<'_>) -> Result<(), String> {
    if path.extra || path_is_in_scan_root(&path.inner, context.additional_docs_folders) {
        Ok(())
    } else {
        Err("Docs items must be inside configured Docs folders.".to_string())
    }
}

fn validate_action_path(path: &DocsPath<'_>, context: DocsContext<'_>) -> Result<(), String> {
    if path.extra
        || path_is_in_scan_root(&path.inner, context.additional_docs_folders)
        || is_root_artifact(&path.inner)
    {
        Ok(())
    } else {
        Err("Docs items must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files.".to_string())
    }
}

/// Two operations must never straddle the mount: a rename, duplicate, or move
/// that crosses roots is refused rather than silently rewriting one root's file
/// into the other.
fn require_same_root(source: &DocsPath<'_>, destination: &DocsPath<'_>) -> Result<(), String> {
    if source.extra == destination.extra {
        return Ok(());
    }
    Err("Docs cannot move items between the project and the Docs directory.".to_string())
}

/*
CDXC:DocsRootAdditive 2026-08-09:
The project's own entries come first and are discovered exactly as they have
always been, so setting a Docs directory can never take the repo's README.md,
CLAUDE.md, or docs/ away. The mounted Docs directory is appended after them.
*/
fn project_file_entries(context: DocsContext<'_>) -> Result<Vec<Value>, String> {
    let mut entries = project_root_file_entries(context.roots.project.as_path(), context)?;
    if let Some(mount) = context.roots.extra.as_ref() {
        append_extra_root_entries(&mut entries, mount);
    }
    Ok(entries)
}

fn project_root_file_entries(
    root: &Path,
    context: DocsContext<'_>,
) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let mut scanned_directory_entries = 0;
    let roots = scan_roots(context.additional_docs_folders);
    for relative_path in &roots {
        if entries.len() >= FILE_LIST_MAX_ENTRIES {
            break;
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
    for relative_path in &roots {
        if entries.len() >= FILE_LIST_MAX_ENTRIES
            || scanned_directory_entries >= FILE_LIST_MAX_ENTRIES
        {
            break;
        }
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
        )?;
    }
    Ok(entries)
}

/*
CDXC:DocsRootRecursive 2026-08-09:
Whole-tree discovery for the mounted Docs directory: it is walked to the bottom
so a note nested five folders deep in a vault is listed like any other. Files
are narrowed to the extensions Docs actually renders, because a vault's image
and attachment folders are not documents.

CDXC:DocsRootAdditive 2026-08-09:
The mount is one top-level folder named after the directory, and every failure
lands on that node's label instead of on the listing: an unopenable directory,
and the entry and depth caps alike. Losing the whole panel — including the
project's own README.md — because a vault is too deep is the one thing this
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
CDXC:DocsRootAdditive 2026-08-10:
Every mounted entry carries the name the tree shows it under beside the routing
address it answers to, for the same reason a preview does: `path` starts with
the reserved segment, which is an implementation detail no reader asked for, and
it reaches humans through Copy Path and through text pasted into a terminal. The
walk builds the routing paths, so the display names are derived from them here
rather than threaded through every recursion.
*/
fn name_extra_root_tree_entries(tree: &mut [Value], mount_name: &str) {
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
fn unavailable_extra_root_entry(name: &str, error: &str) -> Value {
    json!({
        "depth": 0,
        "kind": "directory",
        "displayPath": name,
        "modifiedAt": Value::Null,
        "name": format!("{name} — {error}"),
        "path": EXTRA_ROOT_MOUNT_SEGMENT,
        "size": Value::Null,
    })
}

fn append_docs_tree_entries(
    entries: &mut Vec<Value>,
    root: &Path,
    directory: &Path,
    relative_directory: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
) -> Result<(), String> {
    if depth > DOCS_TREE_MAX_DEPTH {
        return Err(docs_tree_depth_cap_error());
    }
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
        if is_directory
            && !child
                .file_type()
                .is_ok_and(|file_type| file_type.is_symlink())
        {
            directories.push((child.path(), relative_path));
        }
    }
    for (directory, relative_path) in directories {
        append_docs_tree_entries(
            entries,
            root,
            &directory,
            &relative_path,
            depth + 1,
            scanned_directory_entries,
        )?;
    }
    Ok(())
}

fn docs_scan_cap_error() -> String {
    "Docs contains too many directory entries to list safely.".to_string()
}

fn docs_tree_entry_cap_error() -> String {
    format!(
        "Docs directory holds more than {DOCS_TREE_MAX_ENTRIES} files and folders. Point it at a smaller folder in Settings > Projects."
    )
}

fn docs_tree_depth_cap_error() -> String {
    format!(
        "Docs directory nests deeper than {DOCS_TREE_MAX_DEPTH} folders. Point it at a smaller folder in Settings > Projects."
    )
}

fn project_directory(root: &Path, relative_path: &str) -> Option<PathBuf> {
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
) -> Result<Vec<fs::DirEntry>, String> {
    let mut children = Vec::new();
    let directory =
        fs::read_dir(directory).map_err(|_| "Could not list project files.".to_string())?;
    for child in directory {
        if *scanned_directory_entries >= limit {
            return Err(limit_error());
        }
        *scanned_directory_entries += 1;
        if let Ok(child) = child {
            children.push(child);
        }
    }
    Ok(children)
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
            break;
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

fn append_file_entries(
    entries: &mut Vec<Value>,
    root: &Path,
    directory: &Path,
    relative_directory: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
) -> Result<(), String> {
    if entries.len() >= FILE_LIST_MAX_ENTRIES
        || *scanned_directory_entries >= FILE_LIST_MAX_ENTRIES
        || depth > FILE_LIST_MAX_DEPTH
    {
        return Ok(());
    }
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
            break;
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
        if is_directory
            && !child
                .file_type()
                .is_ok_and(|file_type| file_type.is_symlink())
            && depth < FILE_LIST_MAX_DEPTH
        {
            directories.push((child.path(), relative_path));
        }
    }
    for (directory, relative_path) in directories {
        append_file_entries(
            entries,
            root,
            &directory,
            &relative_path,
            depth + 1,
            scanned_directory_entries,
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

/*
CDXC:DocsRootAdditive 2026-08-09:
Every response carries the path the Docs page addressed, mount segment
included, never the path relative to whichever root answered. A preview that
answered with a bare inner path would hand the page an address that means the
project root next time it is used.
*/
fn project_file_preview(context: DocsContext<'_>, path: Option<&str>) -> Result<Value, String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to preview.".to_string());
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
    if metadata.len() > FILE_PREVIEW_MAX_BYTES {
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
        CDXC:DocsRootAdditive 2026-08-09:
        `path` stays the routing address the page must send back; `displayPath`
        is the same file named the way the tree names it, so the header never
        shows the reserved mount segment. Mirrors gpui/src/main.rs.
        */
        "displayPath": path.display(context),
        "gitBaseline": git_baseline(path.root, &target, &path.inner),
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": name,
        "path": path.outer,
        "size": metadata.len(),
    }))
}

fn project_file_metadata(context: DocsContext<'_>, path: Option<&str>) -> Result<Value, String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to inspect.".to_string());
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

fn save_project_file(
    context: DocsContext<'_>,
    path: Option<&str>,
    content: Option<&str>,
) -> Result<Value, String> {
    let content = content.ok_or_else(|| "No file content was provided.".to_string())?;
    if content.len() > FILE_SAVE_MAX_BYTES {
        return Err("File is too large to save from Docs.".to_string());
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

fn rename_project_file(
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
        return Err("Docs rename cannot move items.".to_string());
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
        .map_err(|_| "Docs rename target is unavailable.".to_string())?;
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

fn delete_project_file(context: DocsContext<'_>, path: Option<&str>) -> Result<(), String> {
    let path = docs_path(context, path)?;
    if path.inner.is_empty() || path_is_docs_root_node(&path, context) {
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

fn duplicate_project_file(
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
        return Err("Docs duplicate cannot move files.".to_string());
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

fn create_project_folder(context: DocsContext<'_>, path: Option<&str>) -> Result<(), String> {
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

fn move_project_item(
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

fn normalized_relative_path(path: Option<&str>) -> Result<String, String> {
    let trimmed = path.unwrap_or("").trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains('\0') || trimmed.starts_with('/') || trimmed.contains('\\') {
        return Err("Docs paths must be project-relative.".to_string());
    }
    let components = trimmed
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| *component == "." || *component == "..")
    {
        return Err("Docs paths must stay inside the project.".to_string());
    }
    Ok(components.join("/"))
}

/*
CDXC:DocsRootAdditive 2026-08-09:
Confinement is per root and it is the root the path was ROUTED to, so a `..`
chain or an outward symlink under one mount can never surface inside the other.
*/
fn existing_path(path: &DocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(&path.inner);
    let resolved = fs::canonicalize(target)
        .map_err(|_| "Docs paths must stay inside the project.".to_string())?;
    if !resolved.starts_with(path.root) {
        return Err("Docs paths must stay inside the project.".to_string());
    }
    Ok(resolved)
}

fn writable_path(path: &DocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(&path.inner);
    let parent = target
        .parent()
        .ok_or_else(|| "Select a project file to save.".to_string())?;
    let ancestor = nearest_existing_ancestor(parent)
        .ok_or_else(|| "Docs paths must stay inside the project.".to_string())?;
    let resolved = fs::canonicalize(ancestor)
        .map_err(|_| "Docs paths must stay inside the project.".to_string())?;
    if !resolved.starts_with(path.root) {
        return Err("Docs paths must stay inside the project.".to_string());
    }
    Ok(target)
}

fn operation_path(path: &DocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(&path.inner);
    if let Ok(resolved) = fs::canonicalize(&target) {
        if !resolved.starts_with(path.root) {
            return Err("Docs paths must stay inside the project.".to_string());
        }
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| "Docs paths must stay inside the project.".to_string())?;
        let ancestor = nearest_existing_ancestor(parent)
            .ok_or_else(|| "Docs paths must stay inside the project.".to_string())?;
        let resolved = fs::canonicalize(ancestor)
            .map_err(|_| "Docs paths must stay inside the project.".to_string())?;
        if !resolved.starts_with(path.root) {
            return Err("Docs paths must stay inside the project.".to_string());
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

fn modified_at(metadata: &fs::Metadata) -> Option<String> {
    metadata.modified().ok().map(|time| {
        DateTime::<Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    })
}

fn run_git(arguments: &[&str], cwd: &Path) -> Option<(i32, Vec<u8>)> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some((output.status.code().unwrap_or(-1), output.stdout))
}

fn unavailable_git_baseline(reason: &str) -> Value {
    json!({
        "available": false,
        "baseText": Value::Null,
        "headOid": Value::Null,
        "maxBytesExceeded": Value::Null,
        "reason": reason,
        "tracked": false,
    })
}

fn renderable_git_baseline(
    base_text: Option<String>,
    head_oid: Option<&str>,
    max_bytes_exceeded: Option<bool>,
    reason: Option<&str>,
    tracked: bool,
) -> Value {
    json!({
        "available": true,
        "baseText": base_text,
        "headOid": head_oid,
        "maxBytesExceeded": max_bytes_exceeded,
        "reason": reason,
        "tracked": tracked,
    })
}

fn git_output(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout).trim().to_string()
}

fn git_baseline(root: &Path, file: &Path, _relative_path: &str) -> Value {
    let Some(parent) = file.parent() else {
        return unavailable_git_baseline("not-repo");
    };
    let Some((0, stdout)) = run_git(&["rev-parse", "--show-toplevel"], parent) else {
        return unavailable_git_baseline("not-repo");
    };
    let Ok(repo_root) = fs::canonicalize(git_output(&stdout)) else {
        return unavailable_git_baseline("not-repo");
    };
    if !repo_root.starts_with(root) {
        return unavailable_git_baseline("not-repo");
    }
    let Ok(git_path) = file.strip_prefix(&repo_root) else {
        return unavailable_git_baseline("not-repo");
    };
    let git_path = git_path.to_string_lossy().to_string();
    match run_git(&["check-ignore", "-q", "--", &git_path], &repo_root) {
        Some((0, _)) => return unavailable_git_baseline("ignored"),
        Some((1, _)) => {}
        Some(_) => return unavailable_git_baseline("error"),
        None => return unavailable_git_baseline("git-unavailable"),
    }
    let tracked = run_git(
        &["ls-files", "--error-unmatch", "--", &git_path],
        &repo_root,
    )
    .is_some_and(|(exit, _)| exit == 0);
    let head_oid = run_git(&["rev-parse", "--verify", "HEAD"], &repo_root)
        .filter(|(exit, _)| *exit == 0)
        .map(|(_, stdout)| git_output(&stdout))
        .filter(|value| !value.is_empty());
    if !tracked || head_oid.is_none() {
        return renderable_git_baseline(None, head_oid.as_deref(), None, None, tracked);
    }
    let head_oid = head_oid.unwrap();
    let head_spec = format!("HEAD:{git_path}");
    let Some((0, size)) = run_git(&["cat-file", "-s", &head_spec], &repo_root) else {
        return renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    };
    if git_output(&size)
        .parse::<u64>()
        .is_ok_and(|size| size > GIT_BASELINE_MAX_BYTES as u64)
    {
        return renderable_git_baseline(
            None,
            Some(&head_oid),
            Some(true),
            Some("too-large"),
            tracked,
        );
    }
    let Some((0, baseline)) = run_git(&["cat-file", "-p", &head_spec], &repo_root) else {
        return renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    };
    if baseline.len() > GIT_BASELINE_MAX_BYTES {
        return renderable_git_baseline(
            None,
            Some(&head_oid),
            Some(true),
            Some("too-large"),
            tracked,
        );
    }
    if baseline.contains(&0) {
        return renderable_git_baseline(None, Some(&head_oid), None, Some("binary"), tracked);
    }
    renderable_git_baseline(
        Some(String::from_utf8_lossy(&baseline).to_string()),
        Some(&head_oid),
        None,
        None,
        tracked,
    )
}
