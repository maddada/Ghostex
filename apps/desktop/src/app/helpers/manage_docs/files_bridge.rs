use crate::app::helpers::web_bridge_types::SidebarRuntimeSettingsSnapshot;
use std::path::PathBuf;

use anyhow::Result;

use super::*;
use crate::app::helpers::*;
use crate::*;

pub(crate) fn manage_workarea_runtime_url_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    /*
    CDXC:CefRuntime 2026-06-24-11:03:
    Manage runtime URL authority is the bundled first-party CEF page plus explicit project/manage identity only. The project root stays in the Rust bridge from the in-memory sidebar snapshot, so the Manage page URL remains pathless while CEF replaces the old WKWebView runtime surface.
    */
    if !snapshot.feature_availability.manage || snapshot.is_quick_projectless {
        return None;
    }
    let active_project_id = snapshot.active_project_id.as_ref()?.0.clone();
    let surface_id = snapshot.surface_ids.manage_workspace_id.as_ref()?.clone();
    snapshot.in_memory_project_path.as_ref()?;
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_MANAGE_URL", "manage.html").ok()?;
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
        base_url,
        &[
            ("projectId", active_project_id),
            ("projectEditorId", surface_id),
        ],
    ))
}

pub(crate) fn gpui_manage_additional_docs_folders_text(
    settings: &SidebarRuntimeSettingsSnapshot,
) -> String {
    serde_json::from_str::<serde_json::Value>(&settings.saved_settings_json)
        .ok()
        .and_then(|value| {
            value
                .get("manageAdditionalDocsFolders")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

pub(crate) fn gpui_global_docs_directory_text(settings: &SidebarRuntimeSettingsSnapshot) -> String {
    serde_json::from_str::<serde_json::Value>(&settings.saved_settings_json)
        .ok()
        .and_then(|value| {
            value
                .get("globalDocsDirectory")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

pub(crate) enum ManageFilesBridgeSideEffect {
    AddToSessionContext(String),
    CopyFullPath(String),
    OpenInCodeView(PathBuf),
    /// The file, and the app to open it with when the user picked one under Open With.
    OpenWithSystemApp(PathBuf, Option<PathBuf>),
    RevealInFinder(PathBuf),
}

pub(crate) struct ManageFilesBridgeOutcome {
    pub(crate) action: String,
    pub(crate) request_id: String,
    pub(crate) response: serde_json::Value,
    pub(crate) side_effect: Option<ManageFilesBridgeSideEffect>,
}

pub(crate) fn manage_files_bridge_outcome(
    action: String,
    request_id: String,
    result: Result<serde_json::Value, String>,
) -> ManageFilesBridgeOutcome {
    let mut response = match result {
        Ok(response) => response,
        Err(error) => manage_files_bridge_error_response(&action, &request_id, &error),
    };
    let side_effect = if response.get("error").is_some() {
        None
    } else {
        let object = response.as_object_mut();
        match action.as_str() {
            "addToSessionContext" => object
                .and_then(|object| object.remove("contextPrompt"))
                .and_then(|value| value.as_str().map(str::to_string))
                .map(ManageFilesBridgeSideEffect::AddToSessionContext),
            "copyFullPath" => object
                .and_then(|object| object.remove("fullPath"))
                .and_then(|value| value.as_str().map(str::to_string))
                .map(ManageFilesBridgeSideEffect::CopyFullPath),
            "revealInFinder" => object
                .and_then(|object| object.remove("revealPath"))
                .and_then(|value| value.as_str().map(PathBuf::from))
                .map(ManageFilesBridgeSideEffect::RevealInFinder),
            "openInCodeView" => object
                .and_then(|object| object.remove("codeViewPath"))
                .and_then(|value| value.as_str().map(PathBuf::from))
                .map(ManageFilesBridgeSideEffect::OpenInCodeView),
            "openWithSystemApp" => object.and_then(|object| {
                let app = object
                    .remove("appPath")
                    .and_then(|value| value.as_str().map(PathBuf::from));
                object
                    .remove("openPath")
                    .and_then(|value| value.as_str().map(PathBuf::from))
                    .map(|path| ManageFilesBridgeSideEffect::OpenWithSystemApp(path, app))
            }),
            _ => None,
        }
    };
    ManageFilesBridgeOutcome {
        action,
        request_id,
        response,
        side_effect,
    }
}

pub(crate) fn run_manage_files_bridge_request_for_project_snapshot(
    payload: &str,
    snapshot: Option<&GpuiProjectSnapshot>,
    additional_docs_folders_text: &str,
    global_docs_directory_text: &str,
    project_scope: bool,
) -> ManageFilesBridgeOutcome {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let action = request
        .get("action")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let request_id = request
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    manage_files_bridge_outcome(
        action,
        request_id,
        manage_files_bridge_result(
            &request,
            snapshot,
            additional_docs_folders_text,
            global_docs_directory_text,
            project_scope,
        ),
    )
}

pub(crate) fn manage_files_bridge_result(
    request: &serde_json::Value,
    snapshot: Option<&GpuiProjectSnapshot>,
    additional_docs_folders_text: &str,
    global_docs_directory_text: &str,
    project_scope: bool,
) -> Result<serde_json::Value, String> {
    /*
    macOS `runManageFilesBridgeRequest` parity: the bridge is DOCS-scoped, not
    a general project browser. The project root's listing walks the docs/
    folder, configured additional Docs folders, and root
    Markdown/HTML/Excalidraw artifacts; a configured Docs directory is mounted
    ALONGSIDE it and walks its whole tree (CDXC:Docs,
    CDXC:Docs). Either way
    read/stat/save/rename/duplicate/delete/createFolder/move all validate against
    the allowlist of the root the path was routed to, and text previews carry the
    Git HEAD baseline for meo's gutter. Every response's rootName is the fixed
    docs scope name.
    */
    let action = manage_request_string(request, "action").unwrap_or_default();
    let request_id = manage_request_string(request, "requestId").unwrap_or_default();
    let snapshot = snapshot.ok_or_else(|| "No active project root is available.".to_string())?;
    manage_validate_request_identity(request, snapshot)?;
    let chat_authorization = snapshot
        .active_project_id
        .as_ref()
        .and_then(|id| resolve_manage_chat_file(&id.0, request.get("path")?.as_str()?));
    let roots = manage_docs_root(
        snapshot.active_project_id.as_ref().map(|id| id.0.as_str()),
        snapshot.in_memory_project_path.as_deref(),
        global_docs_directory_text,
        chat_authorization
            .as_ref()
            .map(|authorization| authorization.root.clone()),
        chat_authorization.map(|authorization| authorization.file_name),
    )?;
    let context = ManageDocsContext {
        additional_docs_folders_text,
        project_scope,
        roots: &roots,
    };

    let _mutation = matches!(
        action.as_str(),
        "save" | "rename" | "delete" | "duplicate" | "createFolder" | "move"
    )
    .then(ghostex_docs::directory::MutationGuard::new);

    if project_scope {
        if action == "list" {
            return manage_list_project_directory(context, request);
        }
        if action == "search" {
            return manage_search_project(context, request);
        }
        if action == "openWithSystemApp" {
            let path =
                manage_docs_path(context, manage_request_string(request, "path").as_deref())?;
            let target = manage_existing_url(&path)?;
            manage_validate_accessible_relative_path(&path, context)?;
            if !target.is_file() {
                return Err("Select a file to open.".to_string());
            }
            return Ok(serde_json::json!({
                "action": action,
                "appPath": manage_request_string(request, "appPath"),
                "openPath": target.to_string_lossy(),
                "requestId": request_id,
            }));
        }
    }

    if action == "list"
        && request
            .get("directoryOnly")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return manage_list_directory(context, request);
    }
    if action == "read"
        && request
            .get("deferGitBaseline")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return Ok(
            serde_json::json!({"action": action, "requestId": request_id, "deferredGitBaseline": true, "file": manage_project_file_preview_with_baseline(context, manage_request_string(request, "path").as_deref(), false)?}),
        );
    }
    match action.as_str() {
        "gitBaseline" => Ok(
            serde_json::json!({"action": action, "requestId": request_id, "gitBaseline": manage_file_git_baseline(context, manage_request_string(request, "path").as_deref())?}),
        ),
        "list" => Ok(serde_json::json!({
            "action": action,
            "entries": manage_project_file_entries(context)?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "read" => Ok(serde_json::json!({
            "action": action,
            "file": manage_project_file_preview(
                context,
                manage_request_string(request, "path").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "stat" => Ok(serde_json::json!({
            "action": action,
            "file": manage_project_file_metadata(
                context,
                manage_request_string(request, "path").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "save" => Ok(serde_json::json!({
            "action": action,
            "file": manage_save_project_file(
                context,
                manage_request_string(request, "path").as_deref(),
                manage_request_string(request, "content").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "rename" => Ok(serde_json::json!({
            "action": action,
            "file": manage_rename_project_file(
                context,
                manage_request_string(request, "path").as_deref(),
                manage_request_string(request, "newPath").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "duplicate" => Ok(serde_json::json!({
            "action": action,
            "file": manage_duplicate_project_file(
                context,
                manage_request_string(request, "path").as_deref(),
                manage_request_string(request, "newPath").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "delete" => {
            manage_delete_project_file(context, manage_request_string(request, "path").as_deref())?;
            Ok(serde_json::json!({
                "action": action,
                "requestId": request_id,
                "rootName": MANAGE_DOCS_RELATIVE_PATH,
            }))
        }
        "createFolder" => {
            manage_create_project_folder(
                context,
                manage_request_string(request, "path").as_deref(),
            )?;
            Ok(serde_json::json!({
                "action": action,
                "requestId": request_id,
                "rootName": MANAGE_DOCS_RELATIVE_PATH,
            }))
        }
        "move" => Ok(serde_json::json!({
            "action": action,
            "file": manage_move_project_item(
                context,
                manage_request_string(request, "path").as_deref(),
                manage_request_string(request, "newPath").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "openInCodeView" => Ok(serde_json::json!({
            "action": action,
            "codeViewPath": manage_docs_action_item_path(
                context,
                manage_request_string(request, "path").as_deref(),
                "Select a file to open in VS Code view.",
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "revealInFinder" => Ok(serde_json::json!({
            "action": action,
            "requestId": request_id,
            "revealPath": manage_docs_action_item_path(
                context,
                manage_request_string(request, "path").as_deref(),
                "Select an item to reveal.",
            )?,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "copyFullPath" => Ok(serde_json::json!({
            "action": action,
            "fullPath": manage_docs_action_item_path(
                context,
                manage_request_string(request, "path").as_deref(),
                "Select an item to copy its full path.",
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        "addToSessionContext" => Ok(serde_json::json!({
            "action": action,
            "contextPrompt": manage_session_context_prompt(
                context,
                manage_request_string(request, "path").as_deref(),
            )?,
            "requestId": request_id,
            "rootName": MANAGE_DOCS_RELATIVE_PATH,
        })),
        // A SQLite database's tables and one page of rows; gxserver answers the same action for
        // a remote project (server/src/project_docs/actions.rs).
        "sqlite" => {
            let path =
                manage_docs_path(context, manage_request_string(request, "path").as_deref())?;
            let target = manage_existing_url(&path)?;
            manage_validate_accessible_relative_path(&path, context)?;
            if path.inner.is_empty() || !target.is_file() {
                return Err("Select a database to open.".to_string());
            }
            let params = request.as_object().cloned().unwrap_or_default();
            let mut answer = ghostex_docs::sqlite::sqlite_action(&target, &params)?;
            answer["action"] = serde_json::json!(action);
            answer["requestId"] = serde_json::json!(request_id);
            Ok(answer)
        }
        _ => Err("Unsupported Files action.".to_string()),
    }
}
