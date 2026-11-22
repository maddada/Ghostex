use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Map, Value};

use super::*;

pub(super) fn run_action(
    root: &ProjectDocsRoot,
    params: &Map<String, Value>,
    additional_docs_folders: &str,
) -> Result<Value, String> {
    let roots = docs_roots(root)?;
    let context = DocsContext {
        additional_docs_folders,
        project_scope: string_param(params, "scope").as_deref() == Some("project"),
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

    let _mutation = matches!(
        action.as_str(),
        "save" | "rename" | "delete" | "duplicate" | "createFolder" | "move"
    )
    .then(ghostex_docs::directory::MutationGuard::new);

    if context.project_scope && action == "list" {
        return project_scope::list_project_directory(context, params);
    }
    if context.project_scope && action == "search" {
        return project_scope::search_project(context, params);
    }
    if context.project_scope && action == "searchContent" {
        return project_scope::search_content(context, params);
    }
    if action == "list" && params.get("directoryOnly").and_then(Value::as_bool) == Some(true) {
        return listing::list_directory(context, params);
    }
    if action == "read" && params.get("deferGitBaseline").and_then(Value::as_bool) == Some(true) {
        return Ok(
            json!({"action": action, "requestId": request_id, "deferredGitBaseline": true, "file": project_file_preview_with_baseline(context, string_param(params, "path").as_deref(), false)?}),
        );
    }
    match action.as_str() {
        "gitBaseline" => Ok(
            json!({"action": action, "requestId": request_id, "gitBaseline": listing::file_git_baseline(context, string_param(params, "path").as_deref())?}),
        ),
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
                return Err("Select a resource to read.".to_string());
            }
            let target = existing_path(&path)?;
            validate_accessible_path(&path, context)?;
            let metadata =
                fs::metadata(&target).map_err(|_| "Resource is unavailable.".to_string())?;
            if !metadata.is_file() || metadata.len() > RESOURCE_MAX_BYTES {
                return Err("Resource is unavailable.".to_string());
            }
            let data = fs::read(target).map_err(|_| "Resource is unavailable.".to_string())?;
            Ok(json!({
                "action": action,
                "dataBase64": BASE64_STANDARD.encode(data),
                "requestId": request_id,
            }))
        }
        "openLink" => {
            let path = docs_path(context, string_param(params, "path").as_deref())?;
            let target = existing_path(&path)?;
            validate_accessible_path(&path, context)?;
            if path.inner.is_empty() || !target.is_file() {
                return Err("Select a file to open.".to_string());
            }
            Ok(json!({
                "action": action,
                "linkPath": crate::file_links::link_path(path.root, &path.inner),
                "requestId": request_id,
            }))
        }
        // An HTML file the Files view opens: the file on this computer, which gxserver's handler
        // turns into its Ghostex page when it declares itself one (`project_docs_http.rs`,
        // visual_pages/file_page.rs). An absolute path is any file the caller names, as
        // `openSessionFileLink` reads one.
        "openPage" => {
            let named = string_param(params, "path").unwrap_or_default();
            let file = if Path::new(&named).is_absolute() {
                PathBuf::from(&named)
            } else {
                let path = docs_path(context, Some(&named))?;
                let target = existing_path(&path)?;
                validate_accessible_path(&path, context)?;
                target
            };
            if !file.is_file() {
                return Err("Select a file to open.".to_string());
            }
            Ok(json!({
                "action": action,
                "pageFile": file.to_string_lossy(),
                "requestId": request_id,
            }))
        }
        // A file a session showed anywhere on this computer (`filePath`, absolute), which the
        // phone plays or shows itself: a link to that one file (`file_links::file_link_path`).
        "openSessionFileLink" => {
            let file = string_param(params, "filePath")
                .map(PathBuf::from)
                .filter(|file| file.is_absolute())
                .ok_or_else(|| "openSessionFileLink requires an absolute filePath.".to_string())?;
            Ok(json!({
                "action": action,
                "linkPath": crate::file_links::file_link_path(&file)?,
                "requestId": request_id,
            }))
        }
        // A SQLite database's tables and one page of rows, for a remote project's Files view (the
        // desktop answers a local project's itself, through the same `ghostex_docs::sqlite`).
        "sqlite" => {
            let path = docs_path(context, string_param(params, "path").as_deref())?;
            let target = existing_path(&path)?;
            validate_accessible_path(&path, context)?;
            if path.inner.is_empty() || !target.is_file() {
                return Err("Select a database to open.".to_string());
            }
            let mut answer = ghostex_docs::sqlite::sqlite_action(&target, params)?;
            answer["action"] = json!(action);
            answer["requestId"] = json!(request_id);
            Ok(answer)
        }
        _ => Err("Unsupported Files action.".to_string()),
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
    CDXC:Docs 2026-08-09:
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

pub(super) fn string_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(super) fn project_root(path: &Path) -> Result<PathBuf, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "The active project root is unavailable.".to_string())?;
    if !metadata.is_dir() {
        return Err("The active project root is unavailable.".to_string());
    }
    fs::canonicalize(path).map_err(|_| "The active project root is unavailable.".to_string())
}
