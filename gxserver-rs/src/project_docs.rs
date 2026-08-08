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
const FILE_PREVIEW_MAX_BYTES: u64 = 2_000_000;
pub const FILE_SAVE_MAX_BYTES: usize = 2_000_000;
const GIT_BASELINE_MAX_BYTES: usize = 1024 * 1024;
const RESOURCE_MAX_BYTES: u64 = 12 * 1024 * 1024;
const SESSION_CONTEXT_MAX_BYTES: usize = 300_000;
const DOCS_RELATIVE_PATH: &str = "docs";
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
pub fn run_project_docs_action(project_root: &Path, params: &Map<String, Value>) -> Value {
    let action = string_param(params, "action").unwrap_or_default();
    let request_id = string_param(params, "requestId").unwrap_or_default();
    let additional_docs_folders = string_param(params, "additionalDocsFolders").unwrap_or_default();
    let result = run_action(project_root, params, &additional_docs_folders);
    match result {
        Ok(response) => response,
        Err(error) => json!({
            "action": action,
            "error": error,
            "requestId": request_id,
        }),
    }
}

fn run_action(
    project_root_path: &Path,
    params: &Map<String, Value>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let root = project_root(project_root_path)?;
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
        "list" => Ok(response(
            None,
            Some(project_file_entries(&root, additional_docs_folders)?),
        )),
        "read" => Ok(response(
            Some(project_file_preview(
                &root,
                string_param(params, "path").as_deref(),
                additional_docs_folders,
            )?),
            None,
        )),
        "stat" => Ok(response(
            Some(project_file_metadata(
                &root,
                string_param(params, "path").as_deref(),
                additional_docs_folders,
            )?),
            None,
        )),
        "save" => Ok(response(
            Some(save_project_file(
                &root,
                string_param(params, "path").as_deref(),
                string_param(params, "content").as_deref(),
                additional_docs_folders,
            )?),
            None,
        )),
        "rename" => Ok(response(
            rename_project_file(
                &root,
                string_param(params, "path").as_deref(),
                string_param(params, "newPath").as_deref(),
                additional_docs_folders,
            )?,
            None,
        )),
        "duplicate" => Ok(response(
            Some(duplicate_project_file(
                &root,
                string_param(params, "path").as_deref(),
                string_param(params, "newPath").as_deref(),
                additional_docs_folders,
            )?),
            None,
        )),
        "delete" => {
            delete_project_file(
                &root,
                string_param(params, "path").as_deref(),
                additional_docs_folders,
            )?;
            Ok(response(None, None))
        }
        "createFolder" => {
            create_project_folder(
                &root,
                string_param(params, "path").as_deref(),
                additional_docs_folders,
            )?;
            Ok(response(None, None))
        }
        "move" => Ok(response(
            move_project_item(
                &root,
                string_param(params, "path").as_deref(),
                string_param(params, "newPath").as_deref(),
                additional_docs_folders,
            )?,
            None,
        )),
        "copyFullPath" => {
            let full_path = docs_action_item_path(
                &root,
                string_param(params, "path").as_deref(),
                additional_docs_folders,
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
                &root,
                string_param(params, "path").as_deref(),
                additional_docs_folders,
            )?,
            "requestId": request_id,
            "rootName": DOCS_RELATIVE_PATH,
        })),
        "readResource" => {
            let (target, relative_path) =
                existing_path(&root, string_param(params, "path").as_deref())?;
            if relative_path.is_empty() {
                return Err("Select a Docs resource to read.".to_string());
            }
            validate_accessible_path(&relative_path, additional_docs_folders)?;
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

fn docs_action_item(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
    unavailable_message: &str,
) -> Result<(PathBuf, String, fs::Metadata), String> {
    let (target, relative_path) = operation_path(root, path)?;
    if relative_path.is_empty() {
        return Err(unavailable_message.to_string());
    }
    validate_action_path(&relative_path, additional_docs_folders)?;
    let metadata = fs::metadata(&target).map_err(|_| unavailable_message.to_string())?;
    Ok((target, relative_path, metadata))
}

fn docs_action_item_path(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
    unavailable_message: &str,
) -> Result<String, String> {
    let (target, _, _) =
        docs_action_item(root, path, additional_docs_folders, unavailable_message)?;
    Ok(target.to_string_lossy().into_owned())
}

fn session_context_prompt(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<String, String> {
    let unavailable = "Select a file to add to session context.";
    let (target, relative_path, metadata) =
        docs_action_item(root, path, additional_docs_folders, unavailable)?;
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

fn additional_docs_folder_relative_paths(value: &str) -> Vec<String> {
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
        if folder != DOCS_RELATIVE_PATH && seen.insert(folder.to_lowercase()) {
            folders.push(folder);
        }
    }
    folders
}

fn scan_roots(additional_docs_folders: &str) -> Vec<String> {
    let mut roots = vec![DOCS_RELATIVE_PATH.to_string()];
    roots.extend(additional_docs_folder_relative_paths(
        additional_docs_folders,
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

fn is_root_artifact(relative_path: &str) -> bool {
    if relative_path.is_empty() || relative_path.contains('/') {
        return false;
    }
    relative_path
        .rsplit_once('.')
        .is_some_and(|(_, extension)| {
            ROOT_ARTIFACT_FILE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn validate_accessible_path(
    relative_path: &str,
    additional_docs_folders: &str,
) -> Result<(), String> {
    if relative_path == ANNOTATIONS_SIDECAR_RELATIVE_PATH
        || path_is_in_scan_root(relative_path, additional_docs_folders)
        || is_root_artifact(relative_path)
    {
        return Ok(());
    }
    Err("Docs files must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files.".to_string())
}

fn validate_tree_path(relative_path: &str, additional_docs_folders: &str) -> Result<(), String> {
    if path_is_in_scan_root(relative_path, additional_docs_folders) {
        Ok(())
    } else {
        Err("Docs items must be inside configured Docs folders.".to_string())
    }
}

fn validate_action_path(relative_path: &str, additional_docs_folders: &str) -> Result<(), String> {
    if path_is_in_scan_root(relative_path, additional_docs_folders)
        || is_root_artifact(relative_path)
    {
        Ok(())
    } else {
        Err("Docs items must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files.".to_string())
    }
}

fn project_file_entries(root: &Path, additional_docs_folders: &str) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let mut scanned_directory_entries = 0;
    let roots = scan_roots(additional_docs_folders);
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
) -> Result<Vec<fs::DirEntry>, String> {
    let mut children = Vec::new();
    let directory =
        fs::read_dir(directory).map_err(|_| "Could not list project files.".to_string())?;
    for child in directory {
        if *scanned_directory_entries >= FILE_LIST_MAX_ENTRIES {
            return Err("Docs contains too many directory entries to list safely.".to_string());
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
    let mut children = bounded_directory_entries(root, scanned_directory_entries)?;
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
    let mut children = bounded_directory_entries(directory, scanned_directory_entries)?;
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

fn project_file_preview(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let (target, relative_path) = existing_path(root, path)?;
    if relative_path.is_empty() {
        return Err("Select a project file to preview.".to_string());
    }
    validate_accessible_path(&relative_path, additional_docs_folders)?;
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
            &relative_path,
            &metadata,
        ));
    }
    let data = fs::read(&target).map_err(|_| "Could not read project file.".to_string())?;
    if data.contains(&0) {
        return Ok(unsupported_preview(
            "Binary files are not previewed.",
            name,
            &relative_path,
            &metadata,
        ));
    }
    let Ok(content) = String::from_utf8(data) else {
        return Ok(unsupported_preview(
            "This file is not valid UTF-8 text.",
            name,
            &relative_path,
            &metadata,
        ));
    };
    Ok(json!({
        "content": content,
        "gitBaseline": git_baseline(root, &target, &relative_path),
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": name,
        "path": relative_path,
        "size": metadata.len(),
    }))
}

fn project_file_metadata(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let (target, relative_path) = existing_path(root, path)?;
    if relative_path.is_empty() {
        return Err("Select a project file to inspect.".to_string());
    }
    validate_accessible_path(&relative_path, additional_docs_folders)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to inspect.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to inspect.".to_string());
    }
    Ok(json!({
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": target.file_name().and_then(|name| name.to_str()).unwrap_or(""),
        "path": relative_path,
        "size": metadata.len(),
    }))
}

fn unsupported_preview(error: &str, name: &str, path: &str, metadata: &fs::Metadata) -> Value {
    json!({
        "error": error,
        "kind": "unsupported",
        "modifiedAt": modified_at(metadata),
        "name": name,
        "path": path,
        "size": metadata.len(),
    })
}

fn save_project_file(
    root: &Path,
    path: Option<&str>,
    content: Option<&str>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let content = content.ok_or_else(|| "No file content was provided.".to_string())?;
    if content.len() > FILE_SAVE_MAX_BYTES {
        return Err("File is too large to save from Docs.".to_string());
    }
    let (target, relative_path) = writable_path(root, path)?;
    if relative_path.is_empty() {
        return Err("Select a project file to save.".to_string());
    }
    validate_accessible_path(&relative_path, additional_docs_folders)?;
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
    project_file_preview(root, Some(&relative_path), additional_docs_folders)
}

fn rename_project_file(
    root: &Path,
    path: Option<&str>,
    new_path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<Option<Value>, String> {
    let (source, source_relative) = operation_path(root, path)?;
    let (destination, destination_relative) = operation_path(root, new_path)?;
    if source_relative.is_empty()
        || destination_relative.is_empty()
        || path_is_scan_root(&source_relative, additional_docs_folders)
        || path_is_scan_root(&destination_relative, additional_docs_folders)
    {
        return Err("Select an item to rename.".to_string());
    }
    validate_action_path(&source_relative, additional_docs_folders)?;
    validate_action_path(&destination_relative, additional_docs_folders)?;
    if parent_relative_path(&source_relative) != parent_relative_path(&destination_relative) {
        return Err("Docs rename cannot move items.".to_string());
    }
    let metadata = fs::metadata(&source).map_err(|_| "Select an item to rename.".to_string())?;
    if is_root_artifact(&source_relative) && metadata.is_dir() {
        return Err("Select a file to rename.".to_string());
    }
    if source_relative == destination_relative {
        return if metadata.is_dir() {
            Ok(None)
        } else {
            project_file_preview(root, Some(&source_relative), additional_docs_folders).map(Some)
        };
    }
    require_existing_parent(root, &destination)
        .map_err(|_| "Docs rename target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(&source, &destination).map_err(|_| "Could not rename item.".to_string())?;
    if metadata.is_dir() {
        Ok(None)
    } else {
        project_file_preview(root, Some(&destination_relative), additional_docs_folders).map(Some)
    }
}

fn delete_project_file(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<(), String> {
    let (target, relative_path) = operation_path(root, path)?;
    if relative_path.is_empty() || path_is_scan_root(&relative_path, additional_docs_folders) {
        return Err("Select an item to delete.".to_string());
    }
    validate_action_path(&relative_path, additional_docs_folders)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select an item to delete.".to_string())?;
    if is_root_artifact(&relative_path) && metadata.is_dir() {
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
    root: &Path,
    path: Option<&str>,
    new_path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let (source, source_relative) = operation_path(root, path)?;
    let (destination, destination_relative) = operation_path(root, new_path)?;
    if source_relative.is_empty()
        || destination_relative.is_empty()
        || path_is_scan_root(&source_relative, additional_docs_folders)
        || path_is_scan_root(&destination_relative, additional_docs_folders)
    {
        return Err("Select a file to duplicate.".to_string());
    }
    validate_action_path(&source_relative, additional_docs_folders)?;
    validate_action_path(&destination_relative, additional_docs_folders)?;
    if parent_relative_path(&source_relative) != parent_relative_path(&destination_relative) {
        return Err("Docs duplicate cannot move files.".to_string());
    }
    if fs::metadata(&source)
        .map_err(|_| "Select a file to duplicate.".to_string())?
        .is_dir()
    {
        return Err("Select a file to duplicate.".to_string());
    }
    require_existing_parent(root, &destination)
        .map_err(|_| "Duplicate target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file with that name already exists.".to_string());
    }
    fs::copy(source, destination).map_err(|_| "Could not duplicate file.".to_string())?;
    project_file_preview(root, Some(&destination_relative), additional_docs_folders)
}

fn create_project_folder(
    root: &Path,
    path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<(), String> {
    let (target, relative_path) = operation_path(root, path)?;
    if relative_path.is_empty() || path_is_scan_root(&relative_path, additional_docs_folders) {
        return Err("Select a folder to create.".to_string());
    }
    validate_tree_path(&relative_path, additional_docs_folders)?;
    if relative_path.starts_with(&format!("{DOCS_RELATIVE_PATH}/")) {
        fs::create_dir_all(root.join(DOCS_RELATIVE_PATH))
            .map_err(|_| "Could not create folder.".to_string())?;
    }
    require_existing_parent(root, &target)
        .map_err(|_| "Folder parent is unavailable.".to_string())?;
    if target.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::create_dir(target).map_err(|_| "Could not create folder.".to_string())
}

fn move_project_item(
    root: &Path,
    path: Option<&str>,
    new_path: Option<&str>,
    additional_docs_folders: &str,
) -> Result<Option<Value>, String> {
    let (source, source_relative) = operation_path(root, path)?;
    let (destination, destination_relative) = operation_path(root, new_path)?;
    if source_relative.is_empty()
        || destination_relative.is_empty()
        || path_is_scan_root(&source_relative, additional_docs_folders)
        || path_is_scan_root(&destination_relative, additional_docs_folders)
    {
        return Err("Select an item to move.".to_string());
    }
    validate_action_path(&source_relative, additional_docs_folders)?;
    validate_tree_path(&destination_relative, additional_docs_folders)?;
    if source_relative == destination_relative {
        return if fs::metadata(&source).is_ok_and(|metadata| metadata.is_dir()) {
            Ok(None)
        } else {
            project_file_preview(root, Some(&source_relative), additional_docs_folders).map(Some)
        };
    }
    let metadata = fs::metadata(&source).map_err(|_| "Select an item to move.".to_string())?;
    if is_root_artifact(&source_relative) && metadata.is_dir() {
        return Err("Select a file to move.".to_string());
    }
    if metadata.is_dir() && destination_relative.starts_with(&format!("{source_relative}/")) {
        return Err("Folders cannot be moved inside themselves.".to_string());
    }
    require_existing_parent(root, &destination)
        .map_err(|_| "Move target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(source, destination).map_err(|_| "Could not move item.".to_string())?;
    if metadata.is_dir() {
        Ok(None)
    } else {
        project_file_preview(root, Some(&destination_relative), additional_docs_folders).map(Some)
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

fn existing_path(root: &Path, path: Option<&str>) -> Result<(PathBuf, String), String> {
    let relative_path = normalized_relative_path(path)?;
    let target = root.join(&relative_path);
    let resolved = fs::canonicalize(target)
        .map_err(|_| "Docs paths must stay inside the project.".to_string())?;
    if !resolved.starts_with(root) {
        return Err("Docs paths must stay inside the project.".to_string());
    }
    Ok((resolved, relative_path))
}

fn writable_path(root: &Path, path: Option<&str>) -> Result<(PathBuf, String), String> {
    let relative_path = normalized_relative_path(path)?;
    let target = root.join(&relative_path);
    let parent = target
        .parent()
        .ok_or_else(|| "Select a project file to save.".to_string())?;
    let ancestor = nearest_existing_ancestor(parent)
        .ok_or_else(|| "Docs paths must stay inside the project.".to_string())?;
    let resolved = fs::canonicalize(ancestor)
        .map_err(|_| "Docs paths must stay inside the project.".to_string())?;
    if !resolved.starts_with(root) {
        return Err("Docs paths must stay inside the project.".to_string());
    }
    Ok((target, relative_path))
}

fn operation_path(root: &Path, path: Option<&str>) -> Result<(PathBuf, String), String> {
    let relative_path = normalized_relative_path(path)?;
    let target = root.join(&relative_path);
    if let Ok(resolved) = fs::canonicalize(&target) {
        if !resolved.starts_with(root) {
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
        if !resolved.starts_with(root) {
            return Err("Docs paths must stay inside the project.".to_string());
        }
    }
    Ok((target, relative_path))
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
