//! The Git, worktree and export dialogs' commands, each through a fixed type and field allowlist,
//! answered here instead of in the old runtime (`onWorktreeModalCommand`, `onGitCommitModalCommand`,
//! `onExportTranscriptModalCommand`). The names are the ones every dialog already calls.
//!
//! CDXC:Worktrees 2026-08-09-18:40:
//! The rename confirmation carries the typed name across the dialog boundary. It is NOT a path:
//! the project is revalidated and gxserver derives the destination folder from the name itself.

use ghostex_gx_core::git_menu::GitAction;
use serde_json::{Map, Value, json};

use super::actions::GitActionTarget;
use super::worktree_create::WorktreeCreateRequest;
use super::worktree_list::WorktreeDialogScope;
use crate::GhostexGpuiApp;

fn copy_strings(command: &Map<String, Value>, message: &mut Map<String, Value>, fields: &[&str]) {
    for field in fields {
        if let Some(value) = command.get(*field).and_then(Value::as_str) {
            message.insert((*field).to_string(), json!(value));
        }
    }
}

fn copy_bools(command: &Map<String, Value>, message: &mut Map<String, Value>, fields: &[&str]) {
    for field in fields {
        if let Some(value) = command.get(*field).and_then(Value::as_bool) {
            message.insert((*field).to_string(), json!(value));
        }
    }
}

impl GhostexGpuiApp {
    /// A worktree dialog's command. The project and worktree identity are revalidated against the
    /// store and gxserver before any write.
    pub(crate) fn forward_gpui_worktree_modal_command_to_sidebar(
        &mut self,
        command_type: &str,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut message = Map::new();
        match command_type {
            "requestProjectWorktrees" => copy_strings(
                command,
                &mut message,
                &["requestId", "projectId", "projectPath", "remoteMachineId"],
            ),
            "createProjectWorktree" => copy_strings(
                command,
                &mut message,
                &[
                    "agentId",
                    "baseBranch",
                    "existingWorktreeKey",
                    "existingWorktreePath",
                    "mode",
                    "projectId",
                    "projectPath",
                    "prompt",
                    "remoteMachineId",
                ],
            ),
            "confirmDeleteWorktree" => {
                copy_strings(command, &mut message, &["projectId"]);
                copy_bools(
                    command,
                    &mut message,
                    &["deleteLocalBranch", "deleteRemoteBranch"],
                );
            }
            "confirmRenameWorktree" => {
                copy_strings(command, &mut message, &["projectId", "name"]);
                copy_bools(command, &mut message, &["renameBranch"]);
            }
            "commitWorktreeBeforeDelete" => copy_strings(command, &mut message, &["groupId"]),
            _ => return false,
        }
        let message = Value::Object(message);
        let text = |key: &str, max: usize| {
            message
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.chars().count() <= max)
                .map(str::to_string)
        };
        let flag = |key: &str| message.get(key).and_then(Value::as_bool) == Some(true);
        match command_type {
            "requestProjectWorktrees" => {
                if let Some(request_id) = text("requestId", 120) {
                    let dialog = WorktreeDialogScope::read(&message);
                    self.git_request_project_worktrees(request_id, dialog, cx);
                }
            }
            "createProjectWorktree" => {
                self.git_create_project_worktree(WorktreeCreateRequest::read(&message), cx);
            }
            "confirmDeleteWorktree" => {
                if let Some(project_id) = text("projectId", 300) {
                    self.git_confirm_delete_worktree(
                        &project_id,
                        flag("deleteLocalBranch"),
                        flag("deleteRemoteBranch"),
                        cx,
                    );
                }
            }
            "confirmRenameWorktree" => {
                if let (Some(project_id), Some(name)) = (text("projectId", 300), text("name", 200))
                {
                    self.git_confirm_rename_worktree(&project_id, &name, flag("renameBranch"), cx);
                }
            }
            "commitWorktreeBeforeDelete" => {
                if let Some(group_id) = text("groupId", 300) {
                    self.git_run_action(GitActionTarget::Group(group_id), GitAction::Commit, cx);
                }
            }
            _ => {}
        }
        true
    }

    /// A commit review dialog's command. Request ids and selected paths are revalidated against the
    /// review that request id opened.
    pub(crate) fn forward_gpui_git_commit_modal_command_to_sidebar(
        &mut self,
        command_type: &str,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let fields: &[&str] = match command_type {
            "confirmSidebarGitCommit" | "confirmSidebarGitDirectMerge" => {
                &["agentId", "message", "requestId"]
            }
            "runSidebarGitMultipleCommits" => &["agentId", "requestId"],
            "openSidebarGitChangedFileDiff" | "openSidebarGitChangedFile" => {
                &["filePath", "requestId"]
            }
            "cancelSidebarGitCommit" => &["requestId"],
            _ => return false,
        };
        let mut message = Map::new();
        message.insert("type".to_string(), json!(command_type));
        copy_strings(command, &mut message, fields);
        if command_type == "openSidebarGitChangedFile" {
            copy_bools(command, &mut message, &["openLocation"]);
        }
        if matches!(
            command_type,
            "confirmSidebarGitCommit" | "confirmSidebarGitDirectMerge"
        ) {
            copy_bools(
                command,
                &mut message,
                &["commitOnNewRef", "deleteWorktreeAfter"],
            );
            if let Some(file_paths) = command.get("filePaths").and_then(Value::as_array) {
                let file_paths: Vec<&str> = file_paths.iter().filter_map(Value::as_str).collect();
                message.insert("filePaths".to_string(), json!(file_paths));
            }
        }
        self.git_commit_modal_command(&Value::Object(message), cx);
        true
    }

    /// The Export Transcript dialog's commands: its include toggles, the agent it picked, and the
    /// request id that ties them to the open dialog.
    pub(crate) fn forward_gpui_export_transcript_modal_command_to_sidebar(
        &mut self,
        command_type: &str,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut message = Map::new();
        message.insert("type".to_string(), json!(command_type));
        if let Some(request_id) = command
            .get("requestId")
            .and_then(Value::as_str)
            .filter(|request_id| !request_id.trim().is_empty() && request_id.chars().count() <= 128)
        {
            message.insert("requestId".to_string(), json!(request_id));
        }
        copy_strings(command, &mut message, &["agentId"]);
        copy_bools(
            command,
            &mut message,
            &["includeCommands", "includePatches", "includeReasoning"],
        );
        self.git_export_transcript_modal_command(&Value::Object(message), cx);
        true
    }
}
