// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the add-project dialog param builders,
// OS-integration path/script helpers, and workspace terminal bridge scripts.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::path::{Path, PathBuf};

use crate::app::helpers::*;

pub(crate) fn gpui_add_project_dialog_params(
    operation: GpuiAddProjectDialogOperation,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut forwarded = serde_json::Map::new();
    match operation {
        GpuiAddProjectDialogOperation::ListMachines => {}
        GpuiAddProjectDialogOperation::Browse => {
            let partial_path =
                gpui_remote_path_like_string_from_command(params, "partialPath", true)?;
            forwarded.insert("partialPath".to_string(), serde_json::json!(partial_path));
            if let Some(cwd) = gpui_remote_path_like_string_from_command(params, "cwd", false) {
                forwarded.insert("cwd".to_string(), serde_json::json!(cwd));
            }
        }
        GpuiAddProjectDialogOperation::Add => {
            /*
            The daemon derives the project name from the resolved leaf folder,
            which is also what it does for a created-on-demand workspace root.
            Sending a client-side guess would only be able to disagree with it.
            */
            let path = gpui_remote_path_like_string_from_command(params, "path", false)?;
            forwarded.insert("path".to_string(), serde_json::json!(path));
            if params
                .get("createIfMissing")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                forwarded.insert("createIfMissing".to_string(), serde_json::json!(true));
            }
        }
        GpuiAddProjectDialogOperation::CreateDirectory => {
            /*
            CDXC:AddProjectNewFolder 2026-08-18:
            The new-folder request names an existing parent directory plus one
            bounded segment. The daemon re-validates both, so the bridge only
            has to keep the segment from carrying a path.
            */
            let parent_path =
                gpui_remote_path_like_string_from_command(params, "parentPath", false)?;
            let name = gpui_add_project_dialog_bounded_text(params, "name", 255)?;
            forwarded.insert("name".to_string(), serde_json::json!(name));
            forwarded.insert("parentPath".to_string(), serde_json::json!(parent_path));
        }
        GpuiAddProjectDialogOperation::DiscoverSourceControl => {}
        GpuiAddProjectDialogOperation::LookupRepository => {
            let provider = gpui_add_project_dialog_bounded_text(params, "provider", 64)?;
            let repository = gpui_add_project_dialog_bounded_text(params, "repository", 512)?;
            forwarded.insert("provider".to_string(), serde_json::json!(provider));
            forwarded.insert("repository".to_string(), serde_json::json!(repository));
        }
        GpuiAddProjectDialogOperation::PreviewClone | GpuiAddProjectDialogOperation::StartClone => {
            let remote_url = gpui_add_project_dialog_bounded_text(params, "remoteUrl", 4_096)?;
            let destination_path =
                gpui_remote_path_like_string_from_command(params, "destinationPath", false)?;
            forwarded.insert(
                "destinationPath".to_string(),
                serde_json::json!(destination_path),
            );
            forwarded.insert("remoteUrl".to_string(), serde_json::json!(remote_url));
            if let Some(branch_name) =
                gpui_add_project_dialog_bounded_text(params, "branchName", 1_024)
            {
                forwarded.insert("branchName".to_string(), serde_json::json!(branch_name));
            }
            if params
                .get("cloneMainOnly")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                forwarded.insert("cloneMainOnly".to_string(), serde_json::json!(true));
            }
            if params
                .get("shallowClone")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                forwarded.insert("shallowClone".to_string(), serde_json::json!(true));
            }
        }
        GpuiAddProjectDialogOperation::CancelCloneJob
        | GpuiAddProjectDialogOperation::ReadCloneJob => {
            let job_id = gpui_add_project_dialog_bounded_text(params, "jobId", 256)?;
            forwarded.insert("jobId".to_string(), serde_json::json!(job_id));
        }
    }
    Some(serde_json::Value::Object(forwarded))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_add_project_dialog_translate_local_windows_paths(
    operation: GpuiAddProjectDialogOperation,
    mut params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:AddProjectWindowsWslPaths 2026-08-02:
    The Windows shell deliberately runs its one local gxserver inside WSL2.
    The shared dialog accepts Win32 drive and UNC paths on a Win32 machine,
    but those paths are not absolute to the Linux daemon. Translate only the
    filesystem fields for this computer before they cross the localhost RPC
    boundary. WSL-native paths, relative paths already resolved by the dialog,
    repository URLs, job ids, and every remote-machine request remain exactly
    as supplied; gxserver continues to store one canonical WSL project path.
    */
    let fields: &[&str] = match operation {
        GpuiAddProjectDialogOperation::Add => &["path"],
        GpuiAddProjectDialogOperation::Browse => &["partialPath", "cwd"],
        GpuiAddProjectDialogOperation::PreviewClone | GpuiAddProjectDialogOperation::StartClone => {
            &["destinationPath"]
        }
        GpuiAddProjectDialogOperation::CreateDirectory => &["parentPath"],
        GpuiAddProjectDialogOperation::CancelCloneJob
        | GpuiAddProjectDialogOperation::DiscoverSourceControl
        | GpuiAddProjectDialogOperation::ListMachines
        | GpuiAddProjectDialogOperation::LookupRepository
        | GpuiAddProjectDialogOperation::ReadCloneJob => &[],
    };
    let Some(object) = params.as_object_mut() else {
        return Err("The add-project request was invalid.".to_string());
    };
    for field in fields {
        let Some(path) = object
            .get(*field)
            .and_then(serde_json::Value::as_str)
            .filter(|path| gpui_add_project_dialog_is_windows_absolute_path(path))
        else {
            continue;
        };
        let translated = windows_terminal_backend::wsl_path_for_windows_path(Path::new(path))
            .map_err(|_| {
                "The selected Windows path could not be translated into WSL.".to_string()
            })?;
        object.insert((*field).to_string(), serde_json::Value::String(translated));
    }
    Ok(params)
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_add_project_dialog_is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("\\\\")
        || (bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\')))
}

pub(crate) fn gpui_add_project_dialog_local_platform() -> &'static str {
    /*
    CDXC:AddProject 2026-07-30:
    The dialog reads a machine's platform in `navigator.platform` spelling to
    decide the submit modifier label and whether Windows-style paths are legal,
    so report this computer in that vocabulary rather than Rust's target names.
    */
    if cfg!(target_os = "macos") {
        "MacIntel"
    } else if cfg!(target_os = "windows") {
        "Win32"
    } else {
        "Linux"
    }
}

pub(crate) fn gpui_add_project_dialog_bounded_text(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Option<String> {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= max_chars)
        .filter(|value| !value.contains('\0'))
        .filter(|value| !value.chars().any(|character| character.is_control()))
        .map(str::to_string)
}

pub(crate) fn gpui_workspace_project_key_allowed(value: &str) -> bool {
    gpui_remote_sidebar_project_id_allowed(value)
        || gpui_remote_project_reference_from_project_id(value).is_some()
}

/*
CDXC:GPUIRemoteBrowserTabs 2026-07-12:
Browser tab models are keyed by project id strings. Local projects use the
plain workspace id, and remote projects use their machine-scoped
`remote:<machine>:project:<id>` identity so their tabs park, persist, and
restore per remote project exactly like local ones.
*/
pub(crate) fn gpui_workspace_folder_picked_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceFolderPicked==='function'){{bridge.onWorkspaceFolderPicked(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceFolderPicks)?bridge.pendingWorkspaceFolderPicks:[];pending.push(payload);bridge.pendingWorkspaceFolderPicks=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_os_integration_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onOsIntegrationCommand==='function'){{bridge.onOsIntegrationCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingOsIntegrationCommands)?bridge.pendingOsIntegrationCommands:[];pending.push(payload);bridge.pendingOsIntegrationCommands=pending;}}}})(); undefined;"
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_path_is_script(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| matches!(extension.as_str(), "command" | "tool" | "sh"))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_expand_tilde_path(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

/// macOS `resolveExistingDirectoryForOpenRequest` parity: a requested cwd is
/// honored only if it is an existing directory; otherwise the quick terminal
/// roots at the home directory.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_resolved_terminal_cwd(cwd: Option<String>) -> String {
    let requested = cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(gpui_os_integration_expand_tilde_path);
    match requested {
        Some(path) if path.is_dir() => path.to_string_lossy().to_string(),
        _ => home_dir().to_string_lossy().to_string(),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_project_root_for_path(path: &Path) -> Option<PathBuf> {
    let path = gpui_os_integration_expand_tilde_path(path.to_string_lossy().as_ref());
    let metadata = std::fs::metadata(&path).ok()?;
    let base = if metadata.is_dir() {
        path
    } else {
        path.parent()?.to_path_buf()
    };
    Some(gpui_os_integration_git_root_for_path(&base).unwrap_or(base))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_git_root_for_path(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path.to_path_buf());
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// macOS `scriptRunCommand` parity: executable scripts run as `./name` from
/// their own directory; non-executable ones run through the user's shell.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_script_run_command(path: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let executable = std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false);
    if executable {
        return format!("./{}", gpui_os_integration_shell_quote(&file_name));
    }
    let shell = std::env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());
    format!(
        "{} {}",
        gpui_os_integration_shell_quote(&shell),
        gpui_os_integration_shell_quote(path.to_string_lossy().as_ref())
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn gpui_workspace_terminal_bell_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalBell==='function'){{bridge.onWorkspaceTerminalBell(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalBells)?bridge.pendingWorkspaceTerminalBells:[];pending.push(payload);bridge.pendingWorkspaceTerminalBells=pending;}}}})(); undefined;"
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_workspace_terminal_title_changed_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalTitleChanged==='function'){{bridge.onWorkspaceTerminalTitleChanged(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalTitleChanges)?bridge.pendingWorkspaceTerminalTitleChanges:[];pending.push(payload);bridge.pendingWorkspaceTerminalTitleChanges=pending;}}}})(); undefined;"
    )
}

// Bridge script for `ghostex.gpui.sidebar.workspaceTerminalEscapePressed`.
pub(crate) fn gpui_workspace_terminal_escape_pressed_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalEscapePressed==='function'){{bridge.onWorkspaceTerminalEscapePressed(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalEscapePresses)?bridge.pendingWorkspaceTerminalEscapePresses:[];pending.push(payload);bridge.pendingWorkspaceTerminalEscapePresses=pending;}}}})(); undefined;"
    )
}

// Bridge script for `ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel`.
pub(crate) fn gpui_workspace_first_prompt_title_generation_cancel_script(
    message: &serde_json::Value,
) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceFirstPromptTitleGenerationCancel==='function'){{bridge.onWorkspaceFirstPromptTitleGenerationCancel(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceFirstPromptTitleGenerationCancels)?bridge.pendingWorkspaceFirstPromptTitleGenerationCancels:[];pending.push(payload);bridge.pendingWorkspaceFirstPromptTitleGenerationCancels=pending;}}}})(); undefined;"
    )
}

// Bridge script for `ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge`.
pub(crate) fn gpui_workspace_session_attention_acknowledge_script(
    message: &serde_json::Value,
) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceSessionAttentionAcknowledge==='function'){{bridge.onWorkspaceSessionAttentionAcknowledge(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceSessionAttentionAcknowledgements)?bridge.pendingWorkspaceSessionAttentionAcknowledgements:[];pending.push(payload);bridge.pendingWorkspaceSessionAttentionAcknowledgements=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_workspace_terminal_runtime_action_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalRuntimeAction==='function'){{bridge.onWorkspaceTerminalRuntimeAction(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalRuntimeActions)?bridge.pendingWorkspaceTerminalRuntimeActions:[];pending.push(payload);bridge.pendingWorkspaceTerminalRuntimeActions=pending;}}}})(); undefined;"
    )
}

/*
CDXC:GPUISidebarPointerTracking 2026-08-02:
`data-native-pointer-inside` is a pure CSS state flag whose only writer is the
native pointer observer, so it is set directly on `document.body` rather than
through a page bridge: the attribute exists from the first paint, no page code
has to be mounted for the write to land, and an absent attribute is already the
correct "pointer position unknown, hover normally" state.
*/
pub(crate) fn gpui_workspace_terminal_lifecycle_request_script(
    message: &serde_json::Value,
) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalLifecycleRequest==='function'&&typeof bridge.postWorkspaceTerminalLifecycleResult==='function'){{bridge.onWorkspaceTerminalLifecycleRequest(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalLifecycleRequests)?bridge.pendingWorkspaceTerminalLifecycleRequests:[];pending.push(payload);bridge.pendingWorkspaceTerminalLifecycleRequests=pending;}}}})(); undefined;"
    )
}
