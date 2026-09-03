// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds native project-path action JSON parsing/execution and sidebar command action/run-end/hotkey JSON parsing.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_sidebar_native_project_path_action_from_json(
    text: &str,
) -> Result<GpuiSidebarNativeProjectPathActionMessage, ()> {
    /*
    CDXC:Projects 2026-06-24-14:18:
    Sidebar-native project path actions intentionally contain no path field. Keep their parsing strict and pathless so renderer compromise cannot turn copy/open project actions into arbitrary filesystem operations; only gxserver project ids may authorize those project-path side effects.

    CDXC:Git 2026-06-24-15:43:
    The same fixed native side-effect bridge now accepts `filePath` only for the changed-file IDE-open action. Treat it as a project-relative candidate to re-validate against gxserver Git state; all project path and PR actions remain pathless, and no renderer URL or absolute path is authoritative.

    CDXC:RemoteMachines 2026-06-24-19:06:
    Remote session actions reuse the `projectId` string slot for a machine-scoped remote presentation session id because the bridge remains fixed-shape and pathless. Rust must parse that id before side effects and reject any payload that tries to add SSH details, paths, tokens, URLs, command text, or daemon responses.

    CDXC:RemoteMachines 2026-06-24-19:25:
    Remote project actions reuse the `projectId` string slot for a machine-scoped remote presentation project id. The parser still accepts only fixed action names and an optional relative file candidate for changed-file opens; remote paths, PR URLs, SSH details, command text, tokens, and daemon responses are never accepted from CEF.

    CDXC:Projects 2026-08-14:
    Remote Recent Projects terminal creation enters this parser as `openRemoteProjectTerminal`. The renderer may identify only the saved machine and project id; Rust must restore the parked project and own remote gxserver creation plus SSH attach preparation.

    CDXC:RemoteMachines 2026-06-24-20:26:
    Remote IDE opens use the same pathless fixed-action bridge as copy-path and PR browser opens. Rust must resolve saved machine settings and remote gxserver project paths before constructing fixed editor argv/URI targets; CEF must not send remote paths, URI strings, SSH details, or Settings editor command text.

    CDXC:RemoteMachines 2026-06-24-21:33:
    Zed remote opens are fixed native actions using Zed's documented `zed ssh://[user@]host[:port]/path` CLI target after Rust resolves the remote path. Keep Cursor, Windsurf, VSCodium, Sublime, and custom remote editor commands unsupported until they have an equally reviewed deterministic remote opener.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object.keys().any(|key| {
        ![
            "version",
            "type",
            "action",
            "projectId",
            "filePath",
            "preferredInterface",
        ]
        .contains(&key.as_str())
    }) {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE)
    {
        return Err(());
    }
    let action = object
        .get("action")
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiSidebarNativeProjectPathAction::from_str)
        .ok_or(())?;
    let file_path = object
        .get("filePath")
        .map(|value| {
            value
                .as_str()
                .and_then(gpui_normalized_relative_git_file_path)
                .ok_or(())
        })
        .transpose()?;
    if action.requires_file_path() != file_path.is_some() {
        return Err(());
    }
    let preferred_interface = match object.get("preferredInterface") {
        None => GpuiPreferredAgentInterface::Terminal,
        Some(value) => value
            .as_str()
            .and_then(GpuiPreferredAgentInterface::from_str)
            .ok_or(())?,
    };
    if object.contains_key("preferredInterface")
        && action != GpuiSidebarNativeProjectPathAction::OpenRemoteSessionTerminal
    {
        return Err(());
    }
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|project_id| {
            !project_id.is_empty()
                && project_id.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !project_id.contains('/')
                && !project_id.contains('\\')
        })
        .ok_or(())?
        .to_string();
    Ok(GpuiSidebarNativeProjectPathActionMessage {
        action,
        file_path,
        preferred_interface,
        project_id,
    })
}

pub(crate) fn gpui_sidebar_command_action_from_json(text: &str) -> Result<GpuiTitlebarAction, ()> {
    /*
    CDXC:CommandPane 2026-06-24-23:17:
    Sidebar command-action payloads are fixed action metadata from the live gxserver HUD projection. Accept only command id, name, action type, and the one action target needed for that type; reject project paths, renderer cwd, env, stdout/stderr, terminal content, shell-state fields, generic IPC names, and mismatched command/url pairs before the existing action runner can create a Browser tab or command-pane launch payload.

    CDXC:CommandPane 2026-06-25-10:29:
    `runMode:"debug"` is a terminal-only control bit from the shared sidebar click contract. It may select the visible debug workspace-terminal path, but it must not allow browser Actions, project paths, cwd/env, logs, or renderer-provided shell metadata to influence command-pane execution.

    CDXC:CommandPane 2026-06-26-04:59:
    `closeTerminalOnExit` remains accepted only as legacy terminal Action metadata for saved-config compatibility. Browser Actions must not carry it, terminal payloads must reject non-booleans, and command-pane runtime must still normalize it to false instead of inferring close behavior from renderer strings, command text, URLs, paths, or shell state.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object.keys().any(|key| {
        ![
            "version",
            "type",
            "actionType",
            "closeTerminalOnExit",
            "commandId",
            "icon",
            "links",
            "name",
            "playCompletionSound",
            "runMode",
            "command",
            "url",
        ]
        .contains(&key.as_str())
    }) {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE)
    {
        return Err(());
    }
    let command_id = gpui_trimmed_json_string_field(object, "commandId")
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .ok_or(())?
        .to_string();
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .unwrap_or_default()
        .to_string();
    let icon = match object.get("icon") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .and_then(gpui_sidebar_command_icon)
                .map(str::to_string)
                .ok_or(())?,
        ),
    };
    let command = object
        .get("command")
        .and_then(serde_json::Value::as_str)
        .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
        .map(str::to_string);
    let url = object
        .get("url")
        .and_then(serde_json::Value::as_str)
        .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
        .map(str::to_string);
    let action_type = match object.get("actionType").and_then(serde_json::Value::as_str) {
        Some("browser") if url.is_some() && command.is_none() => GpuiTitlebarActionType::Browser,
        Some("terminal") if command.is_some() && url.is_none() => GpuiTitlebarActionType::Terminal,
        _ => return Err(()),
    };
    let play_completion_sound = match object.get("playCompletionSound") {
        None => action_type == GpuiTitlebarActionType::Terminal,
        Some(value) if action_type == GpuiTitlebarActionType::Terminal => {
            value.as_bool().ok_or(())?
        }
        Some(_) => return Err(()),
    };
    let close_terminal_on_exit = match object.get("closeTerminalOnExit") {
        None => false,
        Some(value) if action_type == GpuiTitlebarActionType::Terminal => {
            value.as_bool().ok_or(())?
        }
        Some(_) => return Err(()),
    };
    let run_mode = match object.get("runMode").and_then(serde_json::Value::as_str) {
        None | Some("default") => GpuiTitlebarActionRunMode::Default,
        Some("debug") if action_type == GpuiTitlebarActionType::Terminal => {
            GpuiTitlebarActionRunMode::Debug
        }
        Some(_) => return Err(()),
    };
    /*
    CDXC:Projects 2026-07-31-12:00:
    Saved links are terminal-only Action metadata from the trusted HUD command.
    Reject links on browser Actions, non-array shapes, unknown per-link keys,
    unknown targets, and empty or oversized URLs instead of stripping them.
    */
    let links = match object.get("links") {
        None => Vec::new(),
        Some(value) if action_type == GpuiTitlebarActionType::Terminal => value
            .as_array()
            .ok_or(())?
            .iter()
            .map(|item| {
                let item = item.as_object().ok_or(())?;
                if item
                    .keys()
                    .any(|key| !["target", "url"].contains(&key.as_str()))
                {
                    return Err(());
                }
                let url = item
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
                    .filter(|url| url.chars().count() <= GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS)
                    .ok_or(())?
                    .to_string();
                let target = match item.get("target").and_then(serde_json::Value::as_str) {
                    Some("external") => GpuiTitlebarActionLinkTarget::External,
                    Some("integrated") => GpuiTitlebarActionLinkTarget::Integrated,
                    _ => return Err(()),
                };
                Ok(GpuiTitlebarActionLink { target, url })
            })
            .collect::<Result<Vec<_>, ()>>()?,
        Some(_) => return Err(()),
    };
    Ok(GpuiTitlebarAction {
        action_type,
        close_terminal_on_exit,
        command,
        command_id,
        icon,
        links,
        name,
        play_completion_sound,
        run_mode,
        url,
    })
}

pub(crate) fn gpui_sidebar_command_run_end_from_json(text: &str) -> Result<String, ()> {
    /*
    CDXC:CommandPane 2026-06-25-10:34:
    Sidebar command-run-end payloads close the existing live Action tab by command id only. Keep the parser stricter than the launch bridge so closing a run cannot carry command text, URLs, project paths, cwd/env, run ids, status paths, terminal output, persisted shell state, or generic IPC fields.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object
        .keys()
        .any(|key| !["version", "type", "commandId"].contains(&key.as_str()))
    {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE)
    {
        return Err(());
    }
    let command_id = gpui_trimmed_json_string_field(object, "commandId")
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .ok_or(())?
        .to_string();
    Ok(command_id)
}

pub(crate) fn gpui_sidebar_ghostex_hotkey_action_from_json(text: &str) -> Result<String, ()> {
    /*
    CDXC:CommandPalette 2026-06-27-08:17:
    Command-palette hotkey payloads are selector authority only. Accept `type` plus a bounded non-empty `actionId`, then let the existing hotkey dispatcher decide support; reject renderer-owned command text, cwd/env, session ids, paths, URLs, launch metadata, generic IPC fields, and versioned action payloads before they can influence command-pane focus or modal routing.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object
        .keys()
        .any(|key| !["type", "actionId"].contains(&key.as_str()))
    {
        return Err(());
    }
    if object.get("type").and_then(serde_json::Value::as_str) != Some("runGhostexHotkeyAction") {
        return Err(());
    }
    let action_id = gpui_trimmed_json_string_field(object, "actionId")
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .ok_or(())?
        .to_string();
    Ok(action_id)
}

pub(crate) fn execute_gpui_sidebar_native_project_path_action(
    message: GpuiSidebarNativeProjectPathActionMessage,
) -> Result<GpuiSidebarNativeProjectPathActionResult, String> {
    if message.action == GpuiSidebarNativeProjectPathAction::OpenExistingPullRequestInBrowser {
        return gpui_open_existing_project_pull_request_in_browser(&message.project_id)
            .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened);
    }
    if message.action == GpuiSidebarNativeProjectPathAction::OpenSidebarGitChangedFileInIde {
        let file_path = message
            .file_path
            .as_deref()
            .ok_or_else(|| "Choose a changed file from the current Git state.".to_string())?;
        return gpui_open_sidebar_git_changed_file_in_ide(&message.project_id, file_path)
            .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened);
    }
    let path = if message.action.uses_recent_projects() {
        gpui_gxserver_recent_project_path_by_id(&message.project_id)?
    } else {
        gpui_gxserver_workspace_project_path_by_id(&message.project_id)?
    };
    if message.action.copies_path() {
        return Ok(GpuiSidebarNativeProjectPathActionResult::Copied(
            gpui_path_string(&path),
        ));
    }
    if message.action.opens_in_ide() {
        return gpui_open_project_path_for_native_ide_action(message.action, &path)
            .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened);
    }
    gpui_open_path(&path)
        .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened)
        .map_err(|_| "GPUI could not open that project in Finder.".to_string())
}

pub(crate) fn gpui_open_sidebar_git_changed_file_in_ide(
    project_id: &str,
    file_path: &str,
) -> Result<(), String> {
    /*
    CDXC:Git 2026-06-24-15:43:
    Changed-file IDE opens resolve project id plus a project-relative file candidate in Rust. Rebuild the current gxserver changed-file set before joining under the project root so CEF cannot open arbitrary absolute paths, sibling paths, URLs, command text, or stale renderer-only filenames.
    */
    let relative_file_path = gpui_normalized_relative_git_file_path(file_path)
        .ok_or_else(|| "Choose a changed file from the current Git state.".to_string())?;
    let changed_files = gpui_project_git_changed_file_paths(project_id)?;
    if !changed_files.contains(&relative_file_path) {
        return Err("Choose a changed file from the current Git state.".to_string());
    }
    let project_path = gpui_gxserver_workspace_project_path_by_id(project_id)?;
    let absolute_file_path = project_path.join(&relative_file_path);
    if !path_is_inside_or_equal(&absolute_file_path, &project_path) {
        return Err("Choose a changed file from the current Git state.".to_string());
    }
    gpui_open_project_path_in_default_editor(&absolute_file_path)
        .map_err(|_| "Configured editor could not open that file.".to_string())
}
