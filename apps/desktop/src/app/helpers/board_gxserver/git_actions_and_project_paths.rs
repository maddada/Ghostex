// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds project board git-action modal command scripts, typed-
// operation git result parsing, and gxserver recent/workspace project path
// resolution helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{collections::HashSet, path::PathBuf, time::Duration};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_project_board_conversation_request_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onProjectBoardConversationRequest==='function'){{bridge.onProjectBoardConversationRequest(payload);}}else{{const pending=Array.isArray(bridge.pendingProjectBoardConversationRequests)?bridge.pendingProjectBoardConversationRequests:[];pending.push(payload);bridge.pendingProjectBoardConversationRequests=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_worktree_modal_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorktreeModalCommand==='function'){{bridge.onWorktreeModalCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingWorktreeModalCommands)?bridge.pendingWorktreeModalCommands:[];pending.push(payload);bridge.pendingWorktreeModalCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_git_commit_modal_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onGitCommitModalCommand==='function'){{bridge.onGitCommitModalCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingGitCommitModalCommands)?bridge.pendingGitCommitModalCommands:[];pending.push(payload);bridge.pendingGitCommitModalCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_export_transcript_modal_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onExportTranscriptModalCommand==='function'){{bridge.onExportTranscriptModalCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingExportTranscriptModalCommands)?bridge.pendingExportTranscriptModalCommands:[];pending.push(payload);bridge.pendingExportTranscriptModalCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_gxserver_git_action_result(
    project_id: &str,
    action: &str,
) -> Result<serde_json::Value, String> {
    gpui_gxserver_rpc_result(
        "/api/runGitAction",
        &serde_json::json!({
            "action": action,
            "projectId": project_id,
        }),
        Duration::from_secs(10),
    )
}

pub(crate) fn gpui_typed_operation_exit_code(result: &serde_json::Value) -> Option<i64> {
    result.get("exitCode").and_then(serde_json::Value::as_i64)
}

pub(crate) fn gpui_typed_operation_stdout(result: &serde_json::Value) -> &str {
    result
        .get("stdout")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(crate) fn gpui_collect_git_numstat_paths(stdout: &str, files: &mut HashSet<String>) {
    for line in stdout.trim().lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split_whitespace();
        let _additions = parts.next();
        let _deletions = parts.next();
        let path = parts.collect::<Vec<_>>().join(" ");
        if let Some(path) = gpui_normalized_relative_git_file_path(&path) {
            files.insert(path);
        }
    }
}

pub(crate) fn gpui_collect_git_status_porcelain_paths(stdout: &str, files: &mut HashSet<String>) {
    /*
    CDXC:GPUISidebarGit 2026-06-24-18:19:
    Git porcelain rename parsing must stay compatible with Rust toolchains where `split(&str)` is forward-only. Select the rename destination path without requiring DoubleEndedIterator, then pass it through the shared relative-path sanitizer before the IDE-open allowlist uses it.
    */
    for line in stdout.lines().filter(|line| line.chars().count() >= 4) {
        let raw_path = line.chars().skip(3).collect::<String>();
        let candidate = raw_path.trim().split(" -> ").last().unwrap_or("").trim();
        if let Some(path) = gpui_normalized_relative_git_file_path(candidate) {
            files.insert(path);
        }
    }
}

pub(crate) fn gpui_collect_git_zero_delimited_paths(stdout: &str, files: &mut HashSet<String>) {
    for path in stdout.split('\0') {
        if let Some(path) = gpui_normalized_relative_git_file_path(path) {
            files.insert(path);
        }
    }
}

pub(crate) fn gpui_normalized_relative_git_file_path(value: &str) -> Option<String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains('\0')
        || normalized.chars().count() > GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS
    {
        return None;
    }
    let segments = normalized.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return None;
    }
    Some(segments.join("/"))
}

pub(crate) fn gpui_gxserver_recent_project_path_by_id(project_id: &str) -> Result<PathBuf, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listRecentProjects",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let recent_projects = result
        .get("recentProjects")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "GPUI could not resolve that recent project.".to_string())?;
    gpui_gxserver_recent_project_path_from_rows_by_id(recent_projects, project_id)
        .ok_or_else(|| "GPUI could not resolve that recent project.".to_string())
}

pub(crate) fn gpui_gxserver_recent_project_path_from_rows_by_id(
    recent_projects: &[serde_json::Value],
    project_id: &str,
) -> Option<PathBuf> {
    recent_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find_map(|project| {
            (json_string_field(project, "projectId") == Some(project_id))
                .then(|| gpui_project_path_from_gxserver_row(project))
                .flatten()
        })
}

pub(crate) fn gpui_gxserver_workspace_project_path_by_id(project_id: &str) -> Result<PathBuf, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listProjects",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let projects = result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "GPUI could not resolve that workspace project.".to_string())?;
    gpui_gxserver_workspace_project_path_from_rows_by_id(projects, project_id)
        .ok_or_else(|| "GPUI could not resolve that workspace project.".to_string())
}

pub(crate) fn gpui_gxserver_workspace_project_path_from_rows_by_id(
    projects: &[serde_json::Value],
    project_id: &str,
) -> Option<PathBuf> {
    /*
    CDXC:GPUIRecentProjects 2026-06-25-19:54:
    Normal workspace project actions must not resolve explicit parked Recent Project rows even if the Rust action entry point is called directly. Skip only boolean `isRecentProject: true` rows here; recent actions continue to resolve trusted recent ids through `/api/listRecentProjects`, and false or non-boolean flags remain normal workspace rows.
    */
    projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find_map(|project| {
            if gpui_gxserver_project_row_is_explicit_recent_project(project) {
                return None;
            }
            (json_string_field(project, "projectId") == Some(project_id))
                .then(|| gpui_project_path_from_gxserver_row(project))
                .flatten()
        })
}

pub(crate) fn gpui_gxserver_project_row_is_explicit_recent_project(
    project: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    json_bool_field(project, "isRecentProject") == Some(true)
}

pub(crate) fn gpui_project_path_from_gxserver_row(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<PathBuf> {
    let path = json_string_field(project, "path")?.trim();
    if path.is_empty() || path.chars().count() > GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS {
        return None;
    }
    let path = PathBuf::from(path);
    path.is_absolute().then_some(path)
}

