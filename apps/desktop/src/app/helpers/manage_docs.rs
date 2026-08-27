// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;
use futures::StreamExt as _;
use gpui::{Action, AppContext as _, Entity, ParentElement as _, prelude::FluentBuilder as _};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_terminal_markdown_image_reference_path(value: &str) -> Option<&str> {
    if !value.starts_with("[Image #") || !value.ends_with(')') {
        return None;
    }
    let open_paren = value.find('(')?;
    let path = value.get(open_paren + 1..value.len() - 1)?.trim();
    let path = path
        .strip_prefix('<')
        .and_then(|path| path.strip_suffix('>'))
        .unwrap_or(path)
        .trim();
    (!path.is_empty()).then_some(path)
}

pub(crate) fn gpui_terminal_file_link_path(link: &str) -> Option<PathBuf> {
    let decoded_file_url;
    let path = if let Some(file_url_path) = link
        .get(..7)
        .filter(|prefix| prefix.eq_ignore_ascii_case("file://"))
        .and_then(|_| link.get(7..))
    {
        decoded_file_url = browser_favicon_percent_decode(file_url_path, 2048)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| file_url_path.to_string());
        decoded_file_url.as_str()
    } else {
        if gpui_terminal_link_has_scheme(link) && !gpui_terminal_link_is_windows_drive_path(link) {
            return None;
        }
        link
    };
    let path = gpui_terminal_file_link_path_without_coordinates(path);
    #[cfg(target_os = "windows")]
    let path = path
        .strip_prefix('/')
        .filter(|candidate| gpui_terminal_link_is_windows_drive_path(candidate))
        .unwrap_or(path);
    (!path.is_empty()).then(|| gpui_expand_terminal_link_path(path))
}

pub(crate) fn gpui_terminal_file_link_path_without_coordinates(path: &str) -> &str {
    let mut path = path;
    for _ in 0..2 {
        let Some((candidate, coordinate)) = path.rsplit_once(':') else {
            break;
        };
        if coordinate.is_empty() || !coordinate.bytes().all(|byte| byte.is_ascii_digit()) {
            break;
        }
        path = candidate;
    }
    path
}

/// RFC 3986 scheme prefix (alpha, then alphanumeric/`+`/`-`/`.`, then `:`)
/// splits URLs from file paths the way the macOS host does; path matches
/// like `src/file.rs:12` fail it because `/` appears before the colon.
pub(crate) fn gpui_terminal_link_has_scheme(link: &str) -> bool {
    let Some(colon) = link.find(':') else {
        return false;
    };
    let mut chars = link[..colon].chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

pub(crate) fn gpui_terminal_link_is_windows_drive_path(link: &str) -> bool {
    let bytes = link.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// Expand a leading `~/` to the user's home directory so home-relative
/// path links resolve like the macOS host's standardizing conversion.
pub(crate) fn gpui_expand_terminal_link_path(link: &str) -> PathBuf {
    if let Some(rest) = link.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(link)
}

/// Per-terminal runtime state reported by Ghostty OSC sequences and runtime
/// actions (window title, working directory, bell, hovered link, search).
/// Runtime-only: keyed by runtime session identity and never persisted into
/// shell-layout state.
#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiTerminalRuntimeOscState {
    pub(crate) title: Option<String>,
    pub(crate) pwd: Option<String>,
    pub(crate) bell_count: u64,
    pub(crate) hovered_link_url: Option<String>,
    pub(crate) search: Option<GpuiTerminalSearchState>,
}

pub(crate) struct PendingGpuiTerminalPasteConfirmation {
    pub(crate) text: String,
    pub(crate) view: Entity<terminal_element::TerminalView>,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_keyboard_owner_uses_docs_editor_hotkeys(owner: GpuiKeyboardOwner) -> bool {
    matches!(
        owner,
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectWorkarea(ProjectWorkareaCefSurfaceSlotKey::Manage)
        ))
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_terminal_key_event_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_TERMINAL_KEY_EVENT_CALLBACK_TARGETS.with(|targets| {
        targets.borrow_mut().insert(
            gpui_root_view as usize,
            GpuiTerminalKeyEventCallbackTarget { app, async_app },
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_terminal_key_event_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
) {
    GPUI_TERMINAL_KEY_EVENT_CALLBACK_TARGETS.with(|targets| {
        targets.borrow_mut().remove(&(gpui_root_view as usize));
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminal_key_event_callback_target_for_native_view(
    native_view: *mut std::ffi::c_void,
) -> Option<GpuiTerminalKeyEventCallbackTarget> {
    if native_view.is_null() {
        return None;
    }
    GPUI_TERMINAL_KEY_EVENT_CALLBACK_TARGETS.with(|targets| {
        targets.borrow().iter().find_map(|(root_key, target)| {
            cef::native_view_contains_responder(*root_key as *mut std::ffi::c_void, native_view)
                .then(|| target.clone())
        })
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_first_responder_programmatic_depth(
    gpui_root_view: *mut std::ffi::c_void,
) -> u32 {
    GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS.with(|depths| {
        depths
            .borrow()
            .get(&(gpui_root_view as usize))
            .copied()
            .unwrap_or(0)
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_first_responder_transition(
    gpui_root_view: *mut std::ffi::c_void,
    responder: *mut std::ffi::c_void,
) {
    /*
    CDXC:GPUIFirstResponderLifetime 2026-07-11:
    `responder` arrives +1 retained from the AppKit KVO hook
    (GpuiCefAppKitHooks.m): responder churn is often caused by the teardown
    that deallocates the outgoing responder view, so a raw pointer would be
    dangling by the time the deferred classification below walks its
    superview chain (use-after-free on the main thread). Every path out of
    this function must balance the retain via
    GhostexGpuiReleaseRetainedResponder — after classification in the
    deferred task, or immediately when no callback target exists yet.
    */
    unsafe extern "C" {
        fn GhostexGpuiReleaseRetainedResponder(responder: *mut std::ffi::c_void);
    }
    let Some(target) = gpui_first_responder_callback_target(gpui_root_view) else {
        unsafe { GhostexGpuiReleaseRetainedResponder(responder) };
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    let responder = responder as usize;
    let suppressed_by_programmatic_focus =
        gpui_first_responder_programmatic_depth(gpui_root_view) > 0;
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                this.receive_first_responder_transition(
                    responder as *mut std::ffi::c_void,
                    suppressed_by_programmatic_focus,
                    window,
                    cx,
                );
            });
            unsafe { GhostexGpuiReleaseRetainedResponder(responder as *mut std::ffi::c_void) };
        })
        .detach();
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiFirstResponderDidChange(
    gpui_root_view: *mut std::ffi::c_void,
    responder: *mut std::ffi::c_void,
) {
    queue_gpui_first_responder_transition(gpui_root_view, responder);
}

/*
CDXC:GPUISidebarPointerTracking 2026-08-02:
The sidebar renderer cannot observe the pointer once it crosses into a native
sibling (GPUI chrome, a Ghostty terminal host, another CEF pane), so Chromium
keeps the last hovered row's :hover state — which is what pinned the hover-only
Close button on a session row after the pointer had already left — and an open
sidebar context menu never learns about clicks that land outside its document.
The AppKit sendEvent observer reports both facts here; both are forwarded into
the page through the sidebar's existing app-owned script boundary.
*/
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSidebarPointerInsideChanged(inside: bool) {
    let Some(target) = gpui_sidebar_pointer_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update(&mut async_app, |this, cx| {
                this.dispatch_gpui_sidebar_pointer_inside(inside, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSidebarOutsideMouseDown() {
    let Some(target) = gpui_sidebar_pointer_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update(&mut async_app, |this, cx| {
                this.dispatch_gpui_sidebar_dismiss_context_menus(cx);
            });
        })
        .detach();
}

pub(crate) fn manage_workarea_runtime_url_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    /*
    CDXC:GPUIProjectWorkareaRuntimeCefBundles 2026-06-24-11:03:
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
    settings: &cef::SidebarRuntimeSettingsSnapshot,
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

pub(crate) fn gpui_global_docs_directory_text(
    settings: &cef::SidebarRuntimeSettingsSnapshot,
) -> String {
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
    chat_docs_root: Option<PathBuf>,
    chat_docs_file_name: Option<String>,
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
            chat_docs_root,
            chat_docs_file_name,
        ),
    )
}

pub(crate) fn manage_files_bridge_result(
    request: &serde_json::Value,
    snapshot: Option<&GpuiProjectSnapshot>,
    additional_docs_folders_text: &str,
    global_docs_directory_text: &str,
    chat_docs_root: Option<PathBuf>,
    chat_docs_file_name: Option<String>,
) -> Result<serde_json::Value, String> {
    /*
    macOS `runManageFilesBridgeRequest` parity: the bridge is DOCS-scoped, not
    a general project browser. The project root's listing walks the docs/
    folder, configured additional Docs folders, and root
    Markdown/HTML/Excalidraw artifacts; a configured Docs directory is mounted
    ALONGSIDE it and walks its whole tree (CDXC:DocsRootAdditive,
    CDXC:DocsRootRecursive). Either way
    read/stat/save/rename/duplicate/delete/createFolder/move all validate against
    the allowlist of the root the path was routed to, and text previews carry the
    Git HEAD baseline for meo's gutter. Every response's rootName is the fixed
    docs scope name.
    */
    let action = manage_request_string(request, "action").unwrap_or_default();
    let request_id = manage_request_string(request, "requestId").unwrap_or_default();
    let snapshot = snapshot.ok_or_else(|| "No active project root is available.".to_string())?;
    let roots = manage_docs_root(
        snapshot.active_project_id.as_ref().map(|id| id.0.as_str()),
        snapshot.in_memory_project_path.as_deref(),
        global_docs_directory_text,
        chat_docs_root,
        chat_docs_file_name,
    )?;
    manage_validate_request_identity(request, snapshot)?;
    let context = ManageDocsContext {
        additional_docs_folders_text,
        roots: &roots,
    };

    match action.as_str() {
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
        _ => Err("Unsupported Docs file action.".to_string()),
    }
}

pub(crate) fn manage_docs_action_item<'a>(
    context: ManageDocsContext<'a>,
    path: Option<&str>,
    unavailable_message: &str,
) -> Result<(PathBuf, ManageDocsPath<'a>, fs::Metadata), String> {
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err(unavailable_message.to_string());
    }
    let target = manage_operation_url(&path)?;
    manage_validate_docs_action_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| unavailable_message.to_string())?;
    Ok((target, path, metadata))
}

pub(crate) fn manage_docs_action_item_path(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    unavailable_message: &str,
) -> Result<String, String> {
    let (target, _, _) = manage_docs_action_item(context, path, unavailable_message)?;
    Ok(target.to_string_lossy().into_owned())
}

pub(crate) fn manage_session_context_prompt(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<String, String> {
    /*
    CDXC:ManageFileActions 2026-08-08:
    Session-context staging reads only a validated Docs file, caps it before
    and after the read, rejects binary/non-UTF-8 content, and formats a fenced
    relative-path block. The CEF response strips this private prompt before
    dispatch; only the selected live agent terminal receives it.
    */
    let unavailable = "Select a file to add to session context.";
    let (target, path, metadata) = manage_docs_action_item(context, path, unavailable)?;
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
    if metadata.len() > MANAGE_SESSION_CONTEXT_MAX_BYTES as u64 {
        return Err("File is too large to add to session context.".to_string());
    }
    let data = fs::read(&target).map_err(|_| unavailable.to_string())?;
    if data.len() > MANAGE_SESSION_CONTEXT_MAX_BYTES {
        return Err("File is too large to add to session context.".to_string());
    }
    if data.contains(&0) {
        return Err("Only UTF-8 text files can be added to session context.".to_string());
    }
    let text = String::from_utf8(data)
        .map_err(|_| "Only UTF-8 text files can be added to session context.".to_string())?;
    let fence = manage_session_context_fence(&text);
    let language = manage_session_context_language(&relative_path);
    let fence_header = if language.is_empty() {
        fence.clone()
    } else {
        format!("{fence}{language}")
    };
    Ok(format!(
        "\nFile context: {relative_path}\n\n{fence_header}\n{text}\n{fence}\n"
    ))
}

pub(crate) fn manage_session_context_fence(text: &str) -> String {
    let mut length = 3;
    while text.contains(&"`".repeat(length)) {
        length += 1;
    }
    "`".repeat(length)
}

pub(crate) fn manage_session_context_language(relative_path: &str) -> &'static str {
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

/// The persistent project/configured roots plus the current chat-authorized
/// document folder, carried together so resolution and validation agree.
#[derive(Clone, Copy)]
pub(crate) struct ManageDocsContext<'a> {
    pub(crate) additional_docs_folders_text: &'a str,
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
CDXC:DocsRootAdditive 2026-08-09:
Mirrors `docs_path` in `server/src/project_docs.rs`. Reserved mount segments
route configured and chat-authorized roots; every other path is project-relative.
One Docs address can therefore only ever mean one root.
*/
pub(crate) fn manage_docs_path<'a>(
    context: ManageDocsContext<'a>,
    path: Option<&str>,
) -> Result<ManageDocsPath<'a>, String> {
    let outer = manage_normalized_relative_path(path)?;
    if let Some(inner) = manage_chat_file_root_relative_path(&outer) {
        let root = context
            .roots
            .chat
            .as_deref()
            .ok_or_else(|| "That chat file is no longer authorized for Docs.".to_string())?;
        return Ok(ManageDocsPath {
            chat: true,
            extra: false,
            inner,
            outer,
            root,
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

/// `Some(inner path)` when a chat-opened document addresses its bounded mount.
pub(crate) fn manage_chat_file_root_relative_path(outer: &str) -> Option<String> {
    if outer == MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT {
        return Some(String::new());
    }
    outer
        .strip_prefix(&format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/"))
        .map(str::to_string)
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

/*
CDXC:DocsRootAdditive 2026-08-09:
Mirrors `scan_roots` in `server/src/project_docs.rs`. Docs folders is
project-root-relative again, the meaning it had before a custom root existed:
`docs` plus each configured folder. Round 2 made it narrow the custom root
instead; with additive mounting that is no longer coherent, because the mounted
Docs directory always shows its whole tree.
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

pub(crate) fn manage_path_is_in_docs_scan_root(
    relative_path: &str,
    additional_docs_folders_text: &str,
) -> bool {
    manage_docs_scan_root_relative_paths(additional_docs_folders_text)
        .iter()
        .any(|root| relative_path == root || relative_path.starts_with(&format!("{root}/")))
}

pub(crate) fn manage_path_is_docs_scan_root(
    relative_path: &str,
    additional_docs_folders_text: &str,
) -> bool {
    manage_docs_scan_root_relative_paths(additional_docs_folders_text)
        .iter()
        .any(|root| relative_path == root)
}

/// The nodes no operation may rename, move, or delete: the project root's scan
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
CDXC:DocsRootAdditive 2026-08-09:
The configured Docs directory serves its whole tree. A chat-authorized mount
serves supported document types from the explicitly selected file's folder.
Project-root paths keep exactly the allowlist they have always had.
*/
pub(crate) fn manage_validate_accessible_relative_path(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> Result<(), String> {
    if path.chat && manage_has_docs_artifact_extension(&path.inner) {
        return Ok(());
    }
    if path.extra
        || path.inner == MANAGE_ANNOTATIONS_SIDECAR_RELATIVE_PATH
        || manage_path_is_in_docs_scan_root(&path.inner, context.additional_docs_folders_text)
        || manage_is_root_artifact_file_relative_path(&path.inner)
    {
        return Ok(());
    }
    Err(
        "Docs files must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files."
            .to_string(),
    )
}

pub(crate) fn manage_validate_docs_tree_relative_path(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> Result<(), String> {
    if path.extra
        || manage_path_is_in_docs_scan_root(&path.inner, context.additional_docs_folders_text)
    {
        return Ok(());
    }
    Err("Docs items must be inside configured Docs folders.".to_string())
}

pub(crate) fn manage_validate_docs_action_relative_path(
    path: &ManageDocsPath<'_>,
    context: ManageDocsContext<'_>,
) -> Result<(), String> {
    if path.chat {
        return Err(
            "Chat-opened files can be edited here but are not Docs tree items.".to_string(),
        );
    }
    if path.extra
        || manage_path_is_in_docs_scan_root(&path.inner, context.additional_docs_folders_text)
        || manage_is_root_artifact_file_relative_path(&path.inner)
    {
        return Ok(());
    }
    Err(
        "Docs items must be inside configured Docs folders or be root Markdown, HTML, or Excalidraw files."
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
    Err("Docs cannot move items between the project and the Docs directory.".to_string())
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
CDXC:DocsRootDirectory 2026-08-09:
The ONE place the local Docs roots are resolved: the project's own Docs
directory, then the Docs directory Global Default. Every local Docs caller (the
CEF files bridge and the Docs resource scope) goes through here, so the cascade
exists once.

CDXC:DocsRootAdditive 2026-08-09:
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
    if configured.is_empty() {
        return Ok(ManageDocsRoots {
            chat: chat_root,
            chat_file_name,
            project,
            extra: None,
        });
    }
    Ok(ManageDocsRoots {
        chat: chat_root,
        chat_file_name,
        project,
        extra: Some(ManageDocsExtraMount {
            location: manage_configured_docs_root(&configured),
            name: manage_docs_extra_root_name(&configured),
        }),
    })
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
CDXC:DocsRootAdditive 2026-08-09:
The persistent project/configured roots mirror `DocsRoots` in
`server/src/project_docs.rs`; `chat` is one runtime-only folder explicitly
authorized by a file click. The configured mount carries either its location
or its error because that failure belongs on one tree node.
*/
pub(crate) struct ManageDocsRoots {
    pub(crate) chat: Option<PathBuf>,
    pub(crate) chat_file_name: Option<String>,
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

pub(crate) fn manage_files_bridge_error_response(
    action: &str,
    request_id: &str,
    error: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "error": error,
        "requestId": request_id,
    })
}

/*
CDXC:DocsRootAdditive 2026-08-09:
The project's own entries come first and are discovered exactly as they have
always been, so setting a Docs directory can never take the repo's README.md,
CLAUDE.md, or docs/ away. The mounted Docs directory is appended after them.
*/
pub(crate) fn manage_project_file_entries(
    context: ManageDocsContext<'_>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut entries = manage_project_root_file_entries(context.roots.project.as_path(), context)?;
    if let Some(mount) = context.roots.extra.as_ref() {
        manage_append_docs_extra_root_entries(&mut entries, mount);
    }
    Ok(entries)
}

pub(crate) fn manage_project_root_file_entries(
    root: &Path,
    context: ManageDocsContext<'_>,
) -> Result<Vec<serde_json::Value>, String> {
    /*
    macOS `manageProjectFileEntries` parity: docs/ and each configured Docs
    folder render as their own top-level directory entries, direct repo-root
    Markdown/HTML/Excalidraw artifacts join them, and only the scan roots are
    walked (bounded), so the Docs sidebar never becomes a broad repo browser.
    */
    let mut entries = Vec::new();
    let scan_roots = manage_docs_scan_root_relative_paths(context.additional_docs_folders_text);
    for relative_path in &scan_roots {
        let Some(directory) = manage_project_directory(root, relative_path) else {
            continue;
        };
        let modified_at = fs::metadata(&directory)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(gpui_iso8601_utc);
        entries.push(serde_json::json!({
            "depth": 0,
            "kind": "directory",
            "modifiedAt": modified_at,
            "name": relative_path,
            "path": relative_path,
            "size": serde_json::Value::Null,
        }));
    }
    manage_append_project_root_artifact_file_entries(&mut entries, root)?;
    for relative_path in &scan_roots {
        if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
            break;
        }
        let Some(directory) = manage_project_directory(root, relative_path) else {
            continue;
        };
        manage_append_project_file_entries(&mut entries, root, &directory, relative_path, 1)?;
    }
    Ok(entries)
}

pub(crate) fn manage_project_directory(root: &Path, relative_path: &str) -> Option<PathBuf> {
    let directory = root.join(PathBuf::from(relative_path));
    let metadata = fs::metadata(&directory).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    let resolved = fs::canonicalize(&directory).ok()?;
    path_is_inside_or_equal(&resolved, root).then_some(resolved)
}

/*
CDXC:DocsRootRecursive 2026-08-09:
Mirrors `append_extra_root_entries` in `server/src/project_docs.rs`, so the
local Docs pane and a remote project's Docs pane list the same tree. The mounted
Docs directory is walked to the bottom and files are narrowed to the extensions
Docs renders.

CDXC:DocsRootAdditive 2026-08-09: every failure lands on the mount node's label
instead of on the listing — an unopenable directory, and the entry and depth caps
alike. Losing the whole panel, including the project's own README.md, because a
vault is too deep is the one thing this must not do, and a tree that silently
stopped at 20,000 entries reads exactly like a vault that only has that many.
*/
pub(crate) fn manage_append_docs_extra_root_entries(
    entries: &mut Vec<serde_json::Value>,
    mount: &ManageDocsExtraMount,
) {
    let root = match mount.location.as_deref() {
        Ok(root) => root,
        Err(error) => {
            entries.push(manage_unavailable_docs_extra_root_entry(&mount.name, error));
            return;
        }
    };
    let mut tree = Vec::new();
    let mut scanned_directory_entries = 0;
    if let Err(error) = manage_append_docs_tree_entries(
        &mut tree,
        root,
        root,
        MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT,
        1,
        &mut scanned_directory_entries,
    ) {
        entries.push(manage_unavailable_docs_extra_root_entry(
            &mount.name,
            &error,
        ));
        return;
    }
    let modified_at = fs::metadata(root)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(gpui_iso8601_utc);
    entries.push(serde_json::json!({
        "depth": 0,
        "displayPath": mount.name,
        "kind": "directory",
        "modifiedAt": modified_at,
        "name": mount.name,
        "path": MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT,
        "size": serde_json::Value::Null,
    }));
    manage_name_docs_extra_root_tree_entries(&mut tree, &mount.name);
    entries.append(&mut tree);
}

/*
CDXC:DocsRootAdditive 2026-08-10:
Mirrors `name_extra_root_tree_entries` in server/src/project_docs.rs: every
mounted entry carries the name the tree shows it under beside the routing
address it answers to, so the reserved segment never reaches Copy Path or text
pasted into a terminal.
*/
pub(crate) fn manage_name_docs_extra_root_tree_entries(
    tree: &mut [serde_json::Value],
    mount_name: &str,
) {
    for entry in tree {
        let Some(relative_path) = entry
            .get("path")
            .and_then(serde_json::Value::as_str)
            .and_then(|path| path.strip_prefix(MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT))
        else {
            continue;
        };
        let display_path = format!("{mount_name}{relative_path}");
        if let Some(entry) = entry.as_object_mut() {
            entry.insert(
                "displayPath".to_string(),
                serde_json::Value::String(display_path),
            );
        }
    }
}

/// The mount still shows when its folder does not, carrying the reason in the
/// only field the Docs tree renders. A missing vault must look missing, not
/// look empty.
pub(crate) fn manage_unavailable_docs_extra_root_entry(
    name: &str,
    error: &str,
) -> serde_json::Value {
    serde_json::json!({
        "depth": 0,
        "kind": "directory",
        "displayPath": name,
        "modifiedAt": serde_json::Value::Null,
        "name": format!("{name} — {error}"),
        "path": MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT,
        "size": serde_json::Value::Null,
    })
}

/// The scan budget the gxserver walk enforces, so a folder full of files Docs
/// does not render costs the same on both sides instead of being free here.
pub(crate) fn manage_bounded_docs_tree_children(
    directory: &Path,
    scanned_directory_entries: &mut usize,
) -> Result<Vec<fs::DirEntry>, String> {
    let mut children = Vec::new();
    for child in fs::read_dir(directory).map_err(|_| "Could not list project files.".to_string())? {
        if *scanned_directory_entries >= MANAGE_DOCS_TREE_MAX_ENTRIES {
            return Err(manage_docs_tree_entry_cap_error());
        }
        *scanned_directory_entries += 1;
        if let Ok(child) = child {
            children.push(child);
        }
    }
    Ok(children)
}

pub(crate) fn manage_append_docs_tree_entries(
    entries: &mut Vec<serde_json::Value>,
    root: &Path,
    directory: &Path,
    relative_directory_path: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
) -> Result<(), String> {
    if depth > MANAGE_DOCS_TREE_MAX_DEPTH {
        return Err(manage_docs_tree_depth_cap_error());
    }
    let mut children = manage_bounded_docs_tree_children(directory, scanned_directory_entries)?;
    children.sort_by(|left, right| {
        let left_is_dir = left
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let right_is_dir = right
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    let mut directories = Vec::new();
    for child in children {
        if entries.len() >= MANAGE_DOCS_TREE_MAX_ENTRIES {
            return Err(manage_docs_tree_entry_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        let metadata = match child.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let is_directory = metadata.is_dir();
        if is_directory {
            if name.starts_with('.') || MANAGE_IGNORED_DIRECTORY_NAMES.contains(&name.as_str()) {
                continue;
            }
        } else if !manage_has_docs_artifact_extension(&name) {
            continue;
        }
        // Confinement, and the reason an outward symlink never joins the tree.
        let resolved = match fs::canonicalize(child.path()) {
            Ok(resolved) => resolved,
            Err(_) => continue,
        };
        if !path_is_inside_or_equal(&resolved, root) {
            continue;
        }
        let relative_path = format!("{relative_directory_path}/{name}");
        entries.push(serde_json::json!({
            "depth": depth,
            "kind": if is_directory { "directory" } else { "file" },
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": relative_path,
            "size": if is_directory { None } else { Some(metadata.len()) },
        }));
        if is_directory
            && !child
                .file_type()
                .map(|file_type| file_type.is_symlink())
                .unwrap_or(false)
        {
            directories.push((child.path(), relative_path));
        }
    }

    for (directory, relative_path) in directories {
        manage_append_docs_tree_entries(
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

pub(crate) fn manage_docs_tree_entry_cap_error() -> String {
    format!(
        "Docs directory holds more than {MANAGE_DOCS_TREE_MAX_ENTRIES} files and folders. Point it at a smaller folder in Settings > Projects."
    )
}

pub(crate) fn manage_docs_tree_depth_cap_error() -> String {
    format!(
        "Docs directory nests deeper than {MANAGE_DOCS_TREE_MAX_DEPTH} folders. Point it at a smaller folder in Settings > Projects."
    )
}

pub(crate) fn manage_append_project_root_artifact_file_entries(
    entries: &mut Vec<serde_json::Value>,
    root: &Path,
) -> Result<(), String> {
    if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
        return Ok(());
    }
    let mut children = fs::read_dir(root)
        .map_err(|_| "Could not list project files.".to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by_key(|child| child.file_name());
    for child in children {
        if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
            return Ok(());
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" || !manage_is_root_artifact_file_relative_path(&name) {
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
        if !path_is_inside_or_equal(&resolved, root) {
            continue;
        }
        entries.push(serde_json::json!({
            "depth": 0,
            "kind": "file",
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": name,
            "size": metadata.len(),
        }));
    }
    Ok(())
}

pub(crate) fn manage_append_project_file_entries(
    entries: &mut Vec<serde_json::Value>,
    root: &Path,
    directory: &Path,
    relative_directory_path: &str,
    depth: usize,
) -> Result<(), String> {
    if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES || depth > MANAGE_FILE_LIST_MAX_DEPTH {
        return Ok(());
    }
    let mut children = fs::read_dir(directory)
        .map_err(|_| "Could not list project files.".to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by(|left, right| {
        let left_is_dir = left
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let right_is_dir = right
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    let mut directories = Vec::new();
    for child in children {
        if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
            return Ok(());
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" {
            continue;
        }
        let metadata = match child.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let is_directory = metadata.is_dir();
        if is_directory && MANAGE_IGNORED_DIRECTORY_NAMES.contains(&name.as_str()) {
            continue;
        }
        let resolved = match fs::canonicalize(child.path()) {
            Ok(resolved) => resolved,
            Err(_) => continue,
        };
        if !path_is_inside_or_equal(&resolved, root) {
            continue;
        }
        let relative_path = if relative_directory_path.is_empty() {
            name.clone()
        } else {
            format!("{relative_directory_path}/{name}")
        };
        entries.push(serde_json::json!({
            "depth": depth,
            "kind": if is_directory { "directory" } else { "file" },
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": relative_path,
            "size": if is_directory { None } else { Some(metadata.len()) },
        }));
        if is_directory
            && !child
                .file_type()
                .map(|file_type| file_type.is_symlink())
                .unwrap_or(false)
            && depth < MANAGE_FILE_LIST_MAX_DEPTH
        {
            directories.push((child.path(), relative_path));
        }
    }

    for (directory, relative_path) in directories {
        manage_append_project_file_entries(entries, root, &directory, &relative_path, depth + 1)?;
    }
    Ok(())
}

/*
CDXC:DocsRootAdditive 2026-08-09:
Every response carries the path the Docs page addressed, mount segment included,
never the path relative to whichever root answered. A preview that answered with
a bare inner path would hand the page an address that means the project root
next time it is used.
*/
pub(crate) fn manage_project_file_preview(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<serde_json::Value, String> {
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to preview.".to_string());
    }
    let target = manage_existing_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to preview.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to preview.".to_string());
    }
    let size = metadata.len();
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    if size > MANAGE_FILE_PREVIEW_MAX_BYTES {
        return Ok(manage_unsupported_file_preview(
            "File is too large to preview.",
            &name,
            &path.outer,
            &path.display(context),
            size,
            &metadata,
        ));
    }
    let data = fs::read(&target).map_err(|_| "Could not read project file.".to_string())?;
    if data.contains(&0) {
        return Ok(manage_unsupported_file_preview(
            "Binary files are not previewed.",
            &name,
            &path.outer,
            &path.display(context),
            size,
            &metadata,
        ));
    }
    let Ok(content) = String::from_utf8(data) else {
        return Ok(manage_unsupported_file_preview(
            "This file is not valid UTF-8 text.",
            &name,
            &path.outer,
            &path.display(context),
            size,
            &metadata,
        ));
    };
    Ok(serde_json::json!({
        "content": content,
        /*
        CDXC:DocsRootAdditive 2026-08-09:
        `path` stays the routing address the page must send back; `displayPath`
        is the same file named the way the tree names it, so the header never
        shows the reserved mount segment.
        */
        "displayPath": path.display(context),
        "gitBaseline": manage_git_baseline_payload(path.root, &target, &path.inner),
        "kind": "text",
        "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
        "name": name,
        "path": path.outer,
        "size": size,
    }))
}

pub(crate) fn manage_project_file_metadata(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<serde_json::Value, String> {
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to inspect.".to_string());
    }
    let target = manage_existing_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select a file to inspect.".to_string())?;
    if metadata.is_dir() {
        return Err("Select a file to inspect.".to_string());
    }
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    Ok(serde_json::json!({
        "kind": "text",
        "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
        "name": name,
        "path": path.outer,
        "size": metadata.len(),
    }))
}

pub(crate) fn manage_unsupported_file_preview(
    error: &str,
    name: &str,
    relative_path: &str,
    display_path: &str,
    size: u64,
    metadata: &fs::Metadata,
) -> serde_json::Value {
    serde_json::json!({
        "displayPath": display_path,
        "error": error,
        "kind": "unsupported",
        "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
        "name": name,
        "path": relative_path,
        "size": size,
    })
}

pub(crate) fn manage_save_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    content: Option<&str>,
) -> Result<serde_json::Value, String> {
    let content = content.ok_or_else(|| "No file content was provided.".to_string())?;
    if content.len() > MANAGE_FILE_SAVE_MAX_BYTES {
        return Err("File is too large to save from Docs.".to_string());
    }
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() {
        return Err("Select a project file to save.".to_string());
    }
    let target = manage_writable_url(&path)?;
    manage_validate_accessible_relative_path(&path, context)?;
    if path.chat && context.roots.chat_file_name.as_deref() != Some(path.inner.as_str()) {
        return Err("Only the document explicitly opened from chat can be saved.".to_string());
    }
    if fs::metadata(&target)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("Select a file to save.".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "Select a file to save.".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "Could not save project file.".to_string())?;
    let temp = parent.join(format!(
        ".ghostex-gpui-manage-save-{}.tmp",
        system_time_epoch_millis_string(std::time::SystemTime::now())
    ));
    fs::write(&temp, content).map_err(|_| "Could not save project file.".to_string())?;
    fs::rename(&temp, &target).map_err(|_| "Could not save project file.".to_string())?;
    manage_project_file_preview(context, Some(&path.outer))
}

pub(crate) fn manage_rename_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    /*
    macOS `manageRenameProjectFile` parity: same-parent rename of a Docs file
    or folder (or a root artifact file), never a move API, never an overwrite,
    with sanitized errors only.
    */
    let source_path = manage_docs_path(context, path)?;
    let destination_path = manage_docs_path(context, new_path)?;
    manage_require_same_docs_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || manage_path_is_docs_root_node(&source_path, context)
        || manage_path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select an item to rename.".to_string());
    }
    let source = manage_operation_url(&source_path)?;
    let destination = manage_operation_url(&destination_path)?;
    manage_validate_docs_action_relative_path(&source_path, context)?;
    manage_validate_docs_action_relative_path(&destination_path, context)?;
    if manage_parent_relative_path(&source_path.inner)
        != manage_parent_relative_path(&destination_path.inner)
    {
        return Err("Docs rename cannot move items.".to_string());
    }
    let source_metadata =
        fs::metadata(&source).map_err(|_| "Select an item to rename.".to_string())?;
    let source_is_directory = source_metadata.is_dir();
    if !source_path.extra
        && manage_is_root_artifact_file_relative_path(&source_path.inner)
        && source_is_directory
    {
        return Err("Select a file to rename.".to_string());
    }
    if source_path.outer == destination_path.outer {
        if !source_is_directory {
            return manage_project_file_preview(context, Some(&source_path.outer));
        }
        return Ok(serde_json::Value::Null);
    }
    manage_require_existing_destination_parent(source_path.root, &destination)
        .map_err(|_| "Docs rename target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(&source, &destination).map_err(|_| "Could not rename item.".to_string())?;
    if source_is_directory {
        return Ok(serde_json::Value::Null);
    }
    manage_project_file_preview(context, Some(&destination_path.outer))
}

pub(crate) fn manage_delete_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<(), String> {
    /*
    macOS `manageDeleteProjectFile` parity: files or folders inside Docs scan
    roots (recursive for folders) plus root artifact files; the scan roots
    themselves, and the mounted Docs directory, are never deletable through
    this path.
    */
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() || manage_path_is_docs_root_node(&path, context) {
        return Err("Select an item to delete.".to_string());
    }
    let target = manage_operation_url(&path)?;
    manage_validate_docs_action_relative_path(&path, context)?;
    let metadata = fs::metadata(&target).map_err(|_| "Select an item to delete.".to_string())?;
    let is_directory = metadata.is_dir();
    if !path.extra && manage_is_root_artifact_file_relative_path(&path.inner) && is_directory {
        return Err("Select a file to delete.".to_string());
    }
    let removed = if is_directory {
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
    };
    removed.map_err(|_| "Could not delete item.".to_string())
}

pub(crate) fn manage_duplicate_project_file(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    // macOS `manageDuplicateProjectFile` parity: file-only same-folder copy;
    // the page chooses the " (n)" suffix, native rejects overwrites.
    let source_path = manage_docs_path(context, path)?;
    let destination_path = manage_docs_path(context, new_path)?;
    manage_require_same_docs_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || manage_path_is_docs_root_node(&source_path, context)
        || manage_path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select a file to duplicate.".to_string());
    }
    let source = manage_operation_url(&source_path)?;
    let destination = manage_operation_url(&destination_path)?;
    manage_validate_docs_action_relative_path(&source_path, context)?;
    manage_validate_docs_action_relative_path(&destination_path, context)?;
    if manage_parent_relative_path(&source_path.inner)
        != manage_parent_relative_path(&destination_path.inner)
    {
        return Err("Docs duplicate cannot move files.".to_string());
    }
    let source_metadata =
        fs::metadata(&source).map_err(|_| "Select a file to duplicate.".to_string())?;
    if source_metadata.is_dir() {
        return Err("Select a file to duplicate.".to_string());
    }
    manage_require_existing_destination_parent(source_path.root, &destination)
        .map_err(|_| "Duplicate target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file with that name already exists.".to_string());
    }
    fs::copy(&source, &destination).map_err(|_| "Could not duplicate file.".to_string())?;
    manage_project_file_preview(context, Some(&destination_path.outer))
}

pub(crate) fn manage_create_project_folder(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
) -> Result<(), String> {
    // macOS `manageCreateProjectFolder` parity: docs-scoped folder creation;
    // the docs/ root is created on demand, overwrites are rejected.
    let path = manage_docs_path(context, path)?;
    if path.inner.is_empty() || manage_path_is_docs_root_node(&path, context) {
        return Err("Select a folder to create.".to_string());
    }
    let target = manage_operation_url(&path)?;
    manage_validate_docs_tree_relative_path(&path, context)?;
    if !path.extra
        && path
            .inner
            .starts_with(&format!("{MANAGE_DOCS_RELATIVE_PATH}/"))
    {
        let docs = path.root.join(MANAGE_DOCS_RELATIVE_PATH);
        fs::create_dir_all(&docs).map_err(|_| "Could not create folder.".to_string())?;
    }
    manage_require_existing_destination_parent(path.root, &target)
        .map_err(|_| "Folder parent is unavailable.".to_string())?;
    if target.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::create_dir(&target).map_err(|_| "Could not create folder.".to_string())
}

pub(crate) fn manage_move_project_item(
    context: ManageDocsContext<'_>,
    path: Option<&str>,
    new_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    /*
    macOS `manageMoveProjectItem` parity: drag/drop moves Docs items (and root
    artifact files) into docs-scoped destinations only, rejecting overwrites
    and directory self-nesting.
    */
    let source_path = manage_docs_path(context, path)?;
    let destination_path = manage_docs_path(context, new_path)?;
    manage_require_same_docs_root(&source_path, &destination_path)?;
    if source_path.inner.is_empty()
        || destination_path.inner.is_empty()
        || manage_path_is_docs_root_node(&source_path, context)
        || manage_path_is_docs_root_node(&destination_path, context)
    {
        return Err("Select an item to move.".to_string());
    }
    let source = manage_operation_url(&source_path)?;
    let destination = manage_operation_url(&destination_path)?;
    manage_validate_docs_action_relative_path(&source_path, context)?;
    manage_validate_docs_tree_relative_path(&destination_path, context)?;
    if source_path.outer == destination_path.outer {
        let is_file = fs::metadata(&source)
            .map(|metadata| !metadata.is_dir())
            .unwrap_or(false);
        if is_file {
            return manage_project_file_preview(context, Some(&source_path.outer));
        }
        return Ok(serde_json::Value::Null);
    }
    let source_metadata =
        fs::metadata(&source).map_err(|_| "Select an item to move.".to_string())?;
    let source_is_directory = source_metadata.is_dir();
    if !source_path.extra
        && manage_is_root_artifact_file_relative_path(&source_path.inner)
        && source_is_directory
    {
        return Err("Select a file to move.".to_string());
    }
    if source_is_directory
        && destination_path
            .inner
            .starts_with(&format!("{}/", source_path.inner))
    {
        return Err("Folders cannot be moved inside themselves.".to_string());
    }
    manage_require_existing_destination_parent(source_path.root, &destination)
        .map_err(|_| "Move target is unavailable.".to_string())?;
    if destination.exists() {
        return Err("A file or folder with that name already exists.".to_string());
    }
    fs::rename(&source, &destination).map_err(|_| "Could not move item.".to_string())?;
    if source_is_directory {
        return Ok(serde_json::Value::Null);
    }
    manage_project_file_preview(context, Some(&destination_path.outer))
}

/// macOS `manageFileOperationURL` parity: resolve symlinks for the escape
/// check, but return the UNRESOLVED path so a listed symlink entry is operated
/// on as the entry itself.
pub(crate) fn manage_operation_url(path: &ManageDocsPath<'_>) -> Result<PathBuf, String> {
    let target = if path.inner.is_empty() {
        path.root.to_path_buf()
    } else {
        path.root.join(PathBuf::from(&path.inner))
    };
    if let Ok(resolved) = fs::canonicalize(&target) {
        if !path_is_inside_or_equal(&resolved, path.root) {
            return Err("Docs paths must stay inside the project.".to_string());
        }
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| "Docs paths must stay inside the project.".to_string())?;
        let nearest_existing_parent = nearest_existing_ancestor(parent)
            .ok_or_else(|| "Docs paths must stay inside the project.".to_string())?;
        let resolved_parent = fs::canonicalize(nearest_existing_parent)
            .map_err(|_| "Docs paths must stay inside the project.".to_string())?;
        if !path_is_inside_or_equal(&resolved_parent, path.root) {
            return Err("Docs paths must stay inside the project.".to_string());
        }
    }
    Ok(target)
}

pub(crate) fn manage_require_existing_destination_parent(
    root: &Path,
    destination: &Path,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "unavailable".to_string())?;
    let resolved_parent = fs::canonicalize(parent).map_err(|_| "unavailable".to_string())?;
    let metadata = fs::metadata(&resolved_parent).map_err(|_| "unavailable".to_string())?;
    if !metadata.is_dir() || !path_is_inside_or_equal(&resolved_parent, root) {
        return Err("unavailable".to_string());
    }
    Ok(())
}

pub(crate) fn manage_run_git(arguments: &[&str], cwd: &Path) -> Option<(i32, Vec<u8>)> {
    let output = std::process::Command::new("/usr/bin/git")
        .args(arguments)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    Some((output.status.code().unwrap_or(-1), output.stdout))
}

pub(crate) fn manage_git_trimmed_output(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout).trim().to_string()
}

pub(crate) fn manage_unavailable_git_baseline(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "baseText": serde_json::Value::Null,
        "headOid": serde_json::Value::Null,
        "maxBytesExceeded": serde_json::Value::Null,
        "reason": reason,
        "tracked": false,
    })
}

pub(crate) fn manage_renderable_git_baseline(
    base_text: Option<String>,
    head_oid: Option<&str>,
    max_bytes_exceeded: Option<bool>,
    reason: Option<&str>,
    tracked: bool,
) -> serde_json::Value {
    serde_json::json!({
        "available": true,
        "baseText": base_text,
        "headOid": head_oid,
        "maxBytesExceeded": max_bytes_exceeded,
        "reason": reason,
        "tracked": tracked,
    })
}

pub(crate) fn manage_git_baseline_payload(
    root: &Path,
    file: &Path,
    relative_path: &str,
) -> serde_json::Value {
    /*
    macOS `manageGitBaselinePayload` parity for meo's CodeMirror Git gutter:
    resolve the repo from the file's parent, reject repos outside the active
    project root, cap baseline text at 1 MB, and return sanitized enum-like
    reasons instead of stderr or filesystem paths.
    */
    if relative_path.is_empty() {
        return manage_unavailable_git_baseline("not-file");
    }
    let Some(parent) = file.parent() else {
        return manage_unavailable_git_baseline("not-repo");
    };
    let Some((exit_code, stdout)) = manage_run_git(&["rev-parse", "--show-toplevel"], parent)
    else {
        return manage_unavailable_git_baseline("git-unavailable");
    };
    if exit_code != 0 {
        return manage_unavailable_git_baseline("not-repo");
    }
    let repo_root_path = manage_git_trimmed_output(&stdout);
    if repo_root_path.is_empty() {
        return manage_unavailable_git_baseline("not-repo");
    }
    let Ok(repo_root) = fs::canonicalize(PathBuf::from(&repo_root_path)) else {
        return manage_unavailable_git_baseline("not-repo");
    };
    if !path_is_inside_or_equal(&repo_root, root) {
        return manage_unavailable_git_baseline("not-repo");
    }
    let Ok(git_path) = file.strip_prefix(&repo_root) else {
        return manage_unavailable_git_baseline("not-repo");
    };
    let git_path = git_path.to_string_lossy().to_string();
    if git_path.is_empty() {
        return manage_unavailable_git_baseline("not-repo");
    }

    let Some((ignore_exit, _)) =
        manage_run_git(&["check-ignore", "-q", "--", &git_path], &repo_root)
    else {
        return manage_unavailable_git_baseline("git-unavailable");
    };
    match ignore_exit {
        0 => return manage_unavailable_git_baseline("ignored"),
        1 => {}
        _ => return manage_unavailable_git_baseline("error"),
    }

    let Some((tracked_exit, _)) = manage_run_git(
        &["ls-files", "--error-unmatch", "--", &git_path],
        &repo_root,
    ) else {
        return manage_unavailable_git_baseline("git-unavailable");
    };
    let tracked = tracked_exit == 0;

    let head_oid = manage_run_git(&["rev-parse", "--verify", "HEAD"], &repo_root)
        .filter(|(exit_code, _)| *exit_code == 0)
        .map(|(_, stdout)| manage_git_trimmed_output(&stdout))
        .filter(|head_oid| !head_oid.is_empty());

    if !tracked || head_oid.is_none() {
        return manage_renderable_git_baseline(None, head_oid.as_deref(), None, None, tracked);
    }
    let head_oid = head_oid.unwrap();
    let head_spec = format!("HEAD:{git_path}");

    let Some((size_exit, size_stdout)) =
        manage_run_git(&["cat-file", "-s", &head_spec], &repo_root)
    else {
        return manage_renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    };
    if size_exit != 0 {
        return manage_renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    }
    if manage_git_trimmed_output(&size_stdout)
        .parse::<u64>()
        .is_ok_and(|size| size > MANAGE_GIT_BASELINE_MAX_BYTES as u64)
    {
        return manage_renderable_git_baseline(
            None,
            Some(&head_oid),
            Some(true),
            Some("too-large"),
            tracked,
        );
    }
    let Some((baseline_exit, baseline_stdout)) =
        manage_run_git(&["cat-file", "-p", &head_spec], &repo_root)
    else {
        return manage_renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    };
    if baseline_exit != 0 {
        return manage_renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    }
    if baseline_stdout.len() > MANAGE_GIT_BASELINE_MAX_BYTES {
        return manage_renderable_git_baseline(
            None,
            Some(&head_oid),
            Some(true),
            Some("too-large"),
            tracked,
        );
    }
    if baseline_stdout.contains(&0) {
        return manage_renderable_git_baseline(
            None,
            Some(&head_oid),
            None,
            Some("binary"),
            tracked,
        );
    }
    manage_renderable_git_baseline(
        Some(String::from_utf8_lossy(&baseline_stdout).to_string()),
        Some(&head_oid),
        None,
        None,
        tracked,
    )
}

/*
CDXC:DocsRootAdditive 2026-08-09:
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
    candidate == root || candidate.starts_with(root)
}

pub(crate) fn system_time_epoch_millis_string(time: std::time::SystemTime) -> String {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
