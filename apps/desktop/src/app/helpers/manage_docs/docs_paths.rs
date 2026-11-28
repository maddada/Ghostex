use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::Result;

use super::*;
use crate::app::helpers::*;
use crate::*;

/// The persistent project/configured roots plus the current chat-authorized
/// document folder, carried together so resolution and validation agree.
#[derive(Clone, Copy)]
pub(crate) struct ManageDocsContext<'a> {
    pub(crate) additional_docs_folders_text: &'a str,
    /// The native Files view: the whole project and every file type, not only the Docs folders.
    pub(crate) project_scope: bool,
    pub(crate) roots: &'a ManageDocsRoots,
}

/// Mirrors `DocsPath`: a Docs path routed to its root. `outer` is what the Docs
/// page addresses, `inner` is what the filesystem under `root` sees.
pub(crate) struct ManageDocsPath<'a> {
    pub(crate) chat: bool,
    pub(crate) extra: bool,
    pub(crate) inner: String,
    pub(crate) outer: String,
    pub(crate) root: &'a Path,
}

impl ManageDocsPath<'_> {
    /// What a human is shown: the mount's own name, never the reserved segment.
    pub(crate) fn display(&self, context: ManageDocsContext<'_>) -> String {
        if self.chat {
            return self.root.join(&self.inner).to_string_lossy().into_owned();
        }
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

/*
CDXC:Docs 2026-08-09:
Mirrors `docs_path` in `server/src/project_docs/roots.rs`. A reserved mount segment
routes the configured root, an absolute path routes the chat-authorized file, and
every other path is project-relative.
One Docs address can therefore only ever mean one root.
*/
pub(crate) fn manage_docs_path<'a>(
    context: ManageDocsContext<'a>,
    path: Option<&str>,
) -> Result<ManageDocsPath<'a>, String> {
    if let Some(address) = path
        .map(str::trim)
        .filter(|path| manage_chat_file_is_address(path))
    {
        // An outside file is addressed by its real path; only its granted file routes.
        let unavailable = || "Reopen this file from its chat link to restore access in Files.";
        let root = context.roots.chat.as_deref().ok_or_else(unavailable)?;
        let inner = context
            .roots
            .chat_file_name
            .clone()
            .ok_or_else(unavailable)?;
        if comparable_path(Path::new(address)) != comparable_path(&root.join(&inner)) {
            return Err(unavailable().to_string());
        }
        return Ok(ManageDocsPath {
            chat: true,
            extra: false,
            inner,
            outer: address.to_string(),
            root,
        });
    }
    let outer = manage_normalized_relative_path(path)?;
    if let Some(drawings) = &context.roots.drawings
        && let Some(inner) = ghostex_docs::project::drawings_inner_path(&outer)
    {
        return Ok(ManageDocsPath {
            chat: false,
            extra: false,
            inner: inner.to_string(),
            outer,
            root: drawings.as_path(),
        });
    }
    let Some(inner) = manage_extra_root_relative_path(&outer) else {
        return Ok(ManageDocsPath {
            chat: false,
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
    Ok(ManageDocsPath {
        chat: false,
        extra: true,
        inner,
        outer,
        root,
    })
}

/// `Some(inner path)` when the path addresses the mounted Docs directory.
pub(crate) fn manage_extra_root_relative_path(outer: &str) -> Option<String> {
    if outer == MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT {
        return Some(String::new());
    }
    outer
        .strip_prefix(&format!("{MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT}/"))
        .map(str::to_string)
}

pub(crate) fn manage_additional_docs_folder_relative_paths(
    additional_docs_folders_text: &str,
    docs_is_implicit_root: bool,
) -> Vec<String> {
    let mut folders = Vec::new();
    let mut seen = HashSet::new();
    for raw_folder in additional_docs_folders_text.split(',') {
        let trimmed = raw_folder.trim();
        let normalized_separators = trimmed.replace('\\', "/");
        let parts = normalized_separators
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty()
            || normalized_separators.contains('\0')
            || normalized_separators.starts_with('~')
            || normalized_separators.starts_with('/')
            || parts.iter().any(|part| *part == "." || *part == "..")
        {
            continue;
        }
        let folder = parts.join("/");
        let key = folder.to_lowercase();
        if (docs_is_implicit_root && MANAGE_BUILT_IN_DOCS_RELATIVE_PATHS.contains(&key.as_str()))
            || !seen.insert(key)
        {
            continue;
        }
        folders.push(folder);
    }
    folders
}

fn manage_is_built_in_docs_folder_name(name: &str) -> bool {
    MANAGE_BUILT_IN_DOCS_RELATIVE_PATHS
        .iter()
        .any(|folder| folder.eq_ignore_ascii_case(name))
}

fn manage_is_skipped_first_level_docs_parent(name: &str) -> bool {
    name.starts_with('.')
        || manage_is_built_in_docs_folder_name(name)
        || MANAGE_IGNORED_DIRECTORY_NAMES.contains(&name)
}

fn manage_relative_path_segments(relative_path: &str) -> Vec<&str> {
    relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect()
}

fn manage_path_is_in_built_in_docs_scan_root(relative_path: &str) -> bool {
    let parts = manage_relative_path_segments(relative_path);
    match parts.as_slice() {
        [] => false,
        [first, ..] if manage_is_built_in_docs_folder_name(first) => true,
        [parent, folder, ..]
            if !manage_is_skipped_first_level_docs_parent(parent)
                && manage_is_built_in_docs_folder_name(folder) =>
        {
            true
        }
        _ => false,
    }
}

fn manage_path_is_built_in_docs_scan_root(relative_path: &str) -> bool {
    let parts = manage_relative_path_segments(relative_path);
    match parts.as_slice() {
        [name] if manage_is_built_in_docs_folder_name(name) => true,
        [parent, folder]
            if !manage_is_skipped_first_level_docs_parent(parent)
                && manage_is_built_in_docs_folder_name(folder) =>
        {
            true
        }
        _ => false,
    }
}

fn manage_nested_built_in_docs_relative_paths(root: &Path) -> Vec<String> {
    let mut folders = Vec::new();
    let mut seen = HashSet::new();
    let Ok(entries) = fs::read_dir(root) else {
        return folders;
    };
    let mut parents = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if manage_is_skipped_first_level_docs_parent(&name) {
                return None;
            }
            entry.metadata().ok().filter(|metadata| metadata.is_dir())?;
            Some(name)
        })
        .collect::<Vec<_>>();
    parents.sort_unstable();
    for parent in parents {
        let parent_path = root.join(&parent);
        let Ok(children) = fs::read_dir(&parent_path) else {
            continue;
        };
        let mut nested = children
            .filter_map(Result::ok)
            .filter_map(|child| {
                let name = child.file_name().to_string_lossy().to_string();
                if !manage_is_built_in_docs_folder_name(&name) {
                    return None;
                }
                child.metadata().ok().filter(|metadata| metadata.is_dir())?;
                Some(name)
            })
            .collect::<Vec<_>>();
        nested.sort_unstable();
        for name in nested {
            let relative = format!("{parent}/{name}");
            if manage_project_directory(root, &relative).is_none() {
                continue;
            }
            if seen.insert(relative.to_lowercase()) {
                folders.push(relative);
            }
        }
    }
    folders
}

/*
CDXC:Docs 2026-08-09:
Mirrors `scan_roots` in `server/src/project_docs/scan_roots.rs`. Docs folders is
project-root-relative again, the meaning it had before a custom root existed:
built-in Docs folders plus each configured folder. Round 2 made it narrow the
custom root instead; with additive mounting that is no longer coherent, because
the mounted Docs directory always shows its whole tree.
*/
pub(crate) fn manage_docs_scan_root_relative_paths(
    additional_docs_folders_text: &str,
) -> Vec<String> {
    let mut roots = MANAGE_BUILT_IN_DOCS_RELATIVE_PATHS
        .iter()
        .map(|path| (*path).to_string())
        .collect::<Vec<_>>();
    roots.extend(manage_additional_docs_folder_relative_paths(
        additional_docs_folders_text,
        true,
    ));
    roots
}

pub(crate) fn manage_docs_project_scan_root_relative_paths(
    project_root: &Path,
    additional_docs_folders_text: &str,
) -> Vec<String> {
    let mut roots = manage_docs_scan_root_relative_paths(additional_docs_folders_text);
    let mut seen = roots
        .iter()
        .map(|path| path.to_lowercase())
        .collect::<HashSet<_>>();
    for nested in manage_nested_built_in_docs_relative_paths(project_root) {
        let key = nested.to_lowercase();
        if roots.iter().any(|existing| {
            key == existing.to_lowercase()
                || key.starts_with(&format!("{}/", existing.to_lowercase()))
        }) {
            continue;
        }
        if seen.insert(key) {
            roots.push(nested);
        }
    }
    roots
}

pub(crate) fn manage_path_is_in_docs_scan_root(
    relative_path: &str,
    additional_docs_folders_text: &str,
) -> bool {
    if manage_path_is_in_built_in_docs_scan_root(relative_path) {
        return true;
    }
    manage_additional_docs_folder_relative_paths(additional_docs_folders_text, true)
        .iter()
        .any(|root| relative_path == root || relative_path.starts_with(&format!("{root}/")))
}

pub(crate) fn manage_path_is_docs_scan_root(
    relative_path: &str,
    additional_docs_folders_text: &str,
) -> bool {
    if manage_path_is_built_in_docs_scan_root(relative_path) {
        return true;
    }
    manage_additional_docs_folder_relative_paths(additional_docs_folders_text, true)
        .iter()
        .any(|root| relative_path == root)
}

/// The nodes rename and move operations must preserve: the project root's scan
/// roots, and the mounted Docs directory itself.
pub(crate) fn manage_path_is_docs_root_node(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> bool {
    if path.extra {
        return path.inner.is_empty();
    }
    manage_path_is_docs_scan_root(&path.inner, context.additional_docs_folders_text)
}

/// The extensions the Docs surface renders. One list for root artifacts and for
/// custom-root tree discovery, so the two can never drift apart.
pub(crate) fn manage_has_docs_artifact_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        MANAGE_ROOT_ARTIFACT_FILE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

pub(crate) fn manage_is_root_artifact_file_relative_path(relative_path: &str) -> bool {
    if relative_path.is_empty() || relative_path.contains('/') {
        return false;
    }
    manage_has_docs_artifact_extension(relative_path)
}

/*
CDXC:Docs 2026-08-09:
The configured Docs directory serves its whole tree. A chat-authorized mount
serves supported document types from the explicitly selected file's folder.
Project-root paths keep exactly the allowlist they have always had.
*/
pub(crate) fn manage_validate_accessible_relative_path(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> Result<(), String> {
    if path.chat && (context.project_scope || manage_has_docs_artifact_extension(&path.inner)) {
        return Ok(());
    }
    // Files scope: any path the root-confined resolvers accept (`manage_existing_url` and friends).
    if path.extra
        || (context.project_scope && !path.chat)
        || path.inner == MANAGE_ANNOTATIONS_SIDECAR_RELATIVE_PATH
        || manage_path_is_in_docs_scan_root(&path.inner, context.additional_docs_folders_text)
        || manage_is_root_artifact_file_relative_path(&path.inner)
    {
        return Ok(());
    }
    Err(
        "Files must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files."
            .to_string(),
    )
}

pub(crate) fn manage_validate_docs_tree_relative_path(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> Result<(), String> {
    if path.extra
        || (context.project_scope && !path.chat)
        || manage_path_is_in_docs_scan_root(&path.inner, context.additional_docs_folders_text)
    {
        return Ok(());
    }
    Err("Items must be inside configured Docs folders.".to_string())
}

pub(crate) fn manage_validate_docs_action_relative_path(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> Result<(), String> {
    if path.chat {
        return Err(
            "Chat-opened files can be edited here but are not in the Files tree.".to_string(),
        );
    }
    if path.extra
        || context.project_scope
        || manage_path_is_in_docs_scan_root(&path.inner, context.additional_docs_folders_text)
        || manage_is_root_artifact_file_relative_path(&path.inner)
    {
        return Ok(());
    }
    Err(
        "Items must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files."
            .to_string(),
    )
}

/// Two operations must never straddle the mount: a rename, duplicate, or move
/// that crosses roots is refused rather than silently rewriting one root's file
/// into the other.
pub(crate) fn manage_require_same_docs_root(
    source: &ManageDocsPath<'_>,
    destination: &ManageDocsPath<'_>,
) -> Result<(), String> {
    if source.extra == destination.extra && source.chat == destination.chat {
        return Ok(());
    }
    Err("Items cannot move between the project and the Docs directory.".to_string())
}

pub(crate) fn manage_parent_relative_path(relative_path: &str) -> String {
    let components = relative_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components.len() <= 1 {
        return String::new();
    }
    components[..components.len() - 1].join("/")
}

pub(crate) fn manage_request_string(request: &serde_json::Value, key: &str) -> Option<String> {
    request
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

pub(crate) fn manage_validate_request_identity(
    request: &serde_json::Value,
    snapshot: &GpuiProjectSnapshot,
) -> Result<(), String> {
    let active_project_id = snapshot
        .active_project_id
        .as_ref()
        .map(|id| id.0.as_str())
        .unwrap_or("");
    let manage_surface_id = snapshot
        .surface_ids
        .manage_workspace_id
        .as_deref()
        .unwrap_or("");
    for key in ["projectId", "projectEditorId"] {
        let Some(value) = request
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if value != active_project_id && value != manage_surface_id {
            return Err("Manage request was not sent by this project editor.".to_string());
        }
    }
    Ok(())
}

/*
CDXC:Docs 2026-08-09:
The ONE place the local Docs roots are resolved: the project's own Docs
directory, then the Docs directory Global Default. Every local Docs caller (the
CEF files bridge and the Docs resource scope) goes through here, so the cascade
exists once.

CDXC:Docs 2026-08-09:
The project root is ALWAYS mounted; a configured Docs directory is mounted in
addition to it, never instead of it. Blank is the only value that inherits, and
a configured path that is missing, is not a folder, or is not absolute is
carried as an unavailable mount rather than failing the panel: the project's own
docs keep listing and the mount node names the path that failed. That is still
not a silent fallback — a silent revert reads exactly like "my vault is empty"
and hides the typo that caused it.
*/
pub(crate) fn manage_docs_root(
    project_id: Option<&str>,
    in_memory_project_path: Option<&Path>,
    global_docs_directory: &str,
    chat_root: Option<PathBuf>,
    chat_file_name: Option<String>,
) -> Result<ManageDocsRoots, String> {
    let configured = match manage_project_docs_directory(project_id)? {
        Some(directory) => directory,
        None => global_docs_directory.trim().to_string(),
    };
    let project = manage_in_memory_project_root(in_memory_project_path)?;
    let drawings = manage_no_project_drawings_folder(&project);
    if configured.is_empty() {
        return Ok(ManageDocsRoots {
            chat: chat_root,
            chat_file_name,
            drawings,
            project,
            extra: None,
        });
    }
    Ok(ManageDocsRoots {
        chat: chat_root,
        chat_file_name,
        drawings,
        project,
        extra: Some(ManageDocsExtraMount {
            location: manage_configured_docs_root(&configured),
            name: manage_docs_extra_root_name(&configured),
        }),
    })
}

/// No project's `~/.ghostex/drawings`, which answers for its `.ghostex/drawings` (CDXC:Canvas
/// 2026-11-28 in packages/project-docs/src/project.rs), canonicalized; `None` for any other
/// project, or while the folder does not exist.
pub(crate) fn manage_no_project_drawings_folder(project_root: &Path) -> Option<PathBuf> {
    let paths = shared_settings::ghostex_storage_paths();
    ghostex_docs::project::no_project_drawings_folder(
        project_root,
        &paths.state_dir,
        &paths.home_dir,
    )
    .and_then(|folder| fs::canonicalize(folder).ok())
    .filter(|folder| folder.is_dir())
}

/// The mount's label: the configured folder's own basename, so a vault at
/// `/Users/sven/vault` shows up as a top-level `vault` folder.
pub(crate) fn manage_docs_extra_root_name(configured: &str) -> String {
    let path = manage_expanded_docs_directory_path(configured);
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/*
CDXC:Docs 2026-08-09:
The persistent project/configured roots mirror `DocsRoots` in
`server/src/project_docs/roots.rs`; `chat` is the folder selected by this request's
persisted native file grant. The configured mount carries either its location
or its error because that failure belongs on one tree node.
*/
pub(crate) struct ManageDocsRoots {
    pub(crate) chat: Option<PathBuf>,
    pub(crate) chat_file_name: Option<String>,
    /// The folder that answers for `.ghostex/drawings` instead of the project's own (No
    /// project's `~/.ghostex/drawings`).
    pub(crate) drawings: Option<PathBuf>,
    pub(crate) extra: Option<ManageDocsExtraMount>,
    pub(crate) project: PathBuf,
}

pub(crate) struct ManageDocsExtraMount {
    pub(crate) location: Result<PathBuf, String>,
    pub(crate) name: String,
}

/// The project's own Docs directory, or `None` when it stores none. An
/// unreadable project row is an error, never "no override": answering "no
/// override" would silently point Docs at the wrong folder.
pub(crate) fn manage_project_docs_directory(
    project_id: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(project_id) = gpui_trimmed_nonempty_str(project_id) else {
        return Ok(None);
    };
    let project = gpui_find_gxserver_project_by_id(project_id)
        .map_err(|_| "Ghostex could not read this project's Docs directory setting.".to_string())?;
    Ok(project
        .get("projectBoardConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(|config| gpui_trimmed_json_string_field(config, "docsDirectory"))
        .map(str::to_string))
}

/// Validate a configured Docs directory: absolute (after expanding a leading
/// `~`) and an existing folder.
pub(crate) fn manage_configured_docs_root(configured: &str) -> Result<PathBuf, String> {
    let path = manage_expanded_docs_directory_path(configured);
    if !path.is_absolute() {
        return Err(format!(
            "Docs directory must be an absolute path: {configured}"
        ));
    }
    let metadata = fs::metadata(&path)
        .map_err(|_| format!("Docs directory does not exist: {}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "Docs directory is not a folder: {}",
            path.display()
        ));
    }
    fs::canonicalize(&path)
        .map_err(|_| format!("Docs directory is unavailable: {}", path.display()))
}

pub(crate) fn manage_expanded_docs_directory_path(configured: &str) -> PathBuf {
    let Some(rest) = configured.strip_prefix('~') else {
        return PathBuf::from(configured);
    };
    let home = shared_settings::ghostex_storage_paths().home_dir.clone();
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home
    } else {
        home.join(rest)
    }
}

pub(crate) fn manage_in_memory_project_root(path: Option<&Path>) -> Result<PathBuf, String> {
    let path = path.ok_or_else(|| "No active project root is available.".to_string())?;
    #[cfg(target_os = "windows")]
    let path = windows_terminal_backend::windows_path_for_wsl_path(path)
        .map_err(|_| "The active project root is unavailable.".to_string())?;
    #[cfg(not(target_os = "windows"))]
    let path = path.to_path_buf();
    let metadata =
        fs::metadata(&path).map_err(|_| "The active project root is unavailable.".to_string())?;
    if !metadata.is_dir() {
        return Err("The active project root is unavailable.".to_string());
    }
    fs::canonicalize(&path).map_err(|_| "The active project root is unavailable.".to_string())
}

/*
CDXC:Docs 2026-08-09:
Confinement is per root, and it is the root the path was ROUTED to, so a `..`
chain or an outward symlink under one mount can never surface inside the other.
*/
pub(crate) fn manage_existing_url(path: &ManageDocsPath<'_>) -> Result<PathBuf, String> {
    let target = if path.inner.is_empty() {
        path.root.to_path_buf()
    } else {
        path.root.join(PathBuf::from(&path.inner))
    };
    let resolved = fs::canonicalize(&target)
        .map_err(|_| "Manage paths must stay inside the project.".to_string())?;
    if !path_is_inside_or_equal(&resolved, path.root) {
        return Err("Manage paths must stay inside the project.".to_string());
    }
    Ok(resolved)
}

pub(crate) fn manage_writable_url(path: &ManageDocsPath<'_>) -> Result<PathBuf, String> {
    let target = path.root.join(PathBuf::from(&path.inner));
    let parent = target
        .parent()
        .ok_or_else(|| "Select a project file to save.".to_string())?;
    let nearest_existing_parent = nearest_existing_ancestor(parent)
        .ok_or_else(|| "Manage paths must stay inside the project.".to_string())?;
    let resolved_parent = fs::canonicalize(nearest_existing_parent)
        .map_err(|_| "Manage paths must stay inside the project.".to_string())?;
    if !path_is_inside_or_equal(&resolved_parent, path.root) {
        return Err("Manage paths must stay inside the project.".to_string());
    }
    Ok(target)
}

pub(crate) fn manage_normalized_relative_path(path: Option<&str>) -> Result<String, String> {
    let trimmed = path.unwrap_or("").trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains('\0') || trimmed.starts_with('/') {
        return Err("Manage paths must be project-relative.".to_string());
    }
    let components = trimmed
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| *component == "." || *component == "..")
    {
        return Err("Manage paths must stay inside the project.".to_string());
    }
    Ok(components.join("/"))
}

pub(crate) fn nearest_existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors().find(|candidate| candidate.exists())
}

pub(crate) fn path_is_inside_or_equal(candidate: &Path, root: &Path) -> bool {
    let (candidate, root) = (comparable_path(candidate), comparable_path(root));
    candidate == root || candidate.starts_with(&root)
}

/// `path` spelled without the Windows verbatim prefix, for comparing only (never for opening).
///
/// CDXC:Docs 2026-10-09 WHY: `fs::canonicalize` returns `\\?\C:\x` on Windows while an outside
/// file's grant routes under `C:\x` (CDXC:Docs 2026-10-08 in manage_docs_chat_files.rs), and Rust
/// compares the two prefixes as different components, so a file the user had open read as outside
/// its own grant ("Reopen this file from its chat link…"). Both sides of a confinement check drop
/// the prefix, at any length, so either spelling of a root matches the other.
fn comparable_path(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

pub(crate) fn system_time_epoch_millis_string(time: std::time::SystemTime) -> String {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
