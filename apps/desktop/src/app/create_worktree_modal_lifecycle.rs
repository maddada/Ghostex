//! Open, sidebar bridge, and result plumbing for the native Add Worktree dialog.
//! SEE-ALSO: apps/desktop/src/app/window/create_worktree_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/gx_store/git/modal_commands.rs (`forward_gpui_worktree_modal_command_to_sidebar`), apps/desktop/src/app/sidebar_dispatch.rs (`handle_gpui_pick_worktree_images_message`).
use crate::app::window::*;
use crate::*;

/// The project the dialog was opened for: the optional `worktree` open-message
/// fields the React host kept in `WorktreeModalState` and echoed on every command.
#[derive(Clone, Debug, Default)]
struct CreateWorktreeModalScope {
    project_id: Option<String>,
    project_path: Option<String>,
    remote_machine_id: Option<String>,
}

fn insert_optional(
    message: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&String>,
) {
    if let Some(value) = value {
        message.insert(key.to_string(), serde_json::json!(value));
    }
}

fn optional_text(message: &serde_json::Value, key: &str) -> Option<String> {
    message
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

impl GhostexGpuiApp {
    /*
    CDXC:Worktrees 2026-09-25 WHY:
    The branch and worktree listing, the trusted Open Existing keys, the
    gxserver worktree creation and the first agent session are Rust's since the
    app runtime port (gx_store/git/worktree_list.rs, worktree_create.rs), which
    supersedes the 2026-09-15 note that the sidebar runtime owned them. The
    dialog still posts the same `requestProjectWorktrees` and
    `createProjectWorktree` commands through the same field allowlist and
    receives the same `projectWorktreesResult` and `worktreeImageFilesPicked`
    answers.
    */
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `worktree` modal kind. The React host accepts any object here (every
    /// field is optional), so only a non-object payload is refused.
    pub(crate) fn open_gpui_create_worktree_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if !message.is_object() {
            return;
        }
        let scope = CreateWorktreeModalScope {
            project_id: optional_text(message, "projectId"),
            project_path: optional_text(message, "projectPath"),
            remote_machine_id: optional_text(message, "remoteMachineId"),
        };
        let hud = message
            .get("latestSidebarStateMessage")
            .and_then(|state| state.get("hud"));
        let agents = hud
            .and_then(|hud| hud.get("agents"))
            .and_then(serde_json::Value::as_array)
            .map(|agents| gpui_create_worktree_command_agents(agents))
            .unwrap_or_else(|| {
                gpui_create_worktree_command_agents(
                    self.new_thread_picker_agents.as_deref().unwrap_or(&[]),
                )
            });
        let default_agent_id = hud
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let config = CreateWorktreeModalConfig {
            agents,
            default_agent_id,
            palette: self.gpui_native_modal_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_create_worktree_modal_command(&scope, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::Worktree,
            CREATE_WORKTREE_MODAL_WIDTH,
            CREATE_WORKTREE_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiCreateWorktreeModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_create_worktree_modal_command(
        &mut self,
        scope: &CreateWorktreeModalScope,
        command: CreateWorktreeModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::Worktree;
        let mut message = serde_json::Map::new();
        insert_optional(&mut message, "projectId", scope.project_id.as_ref());
        insert_optional(&mut message, "projectPath", scope.project_path.as_ref());
        insert_optional(
            &mut message,
            "remoteMachineId",
            scope.remote_machine_id.as_ref(),
        );
        match command {
            CreateWorktreeModalCommand::RequestWorktrees { request_id } => {
                message.insert("requestId".to_string(), serde_json::json!(request_id));
                self.forward_gpui_worktree_modal_command_to_sidebar(
                    "requestProjectWorktrees",
                    &message,
                    cx,
                );
            }
            CreateWorktreeModalCommand::PickImages => {
                self.handle_gpui_pick_worktree_images_message(cx);
            }
            CreateWorktreeModalCommand::Create(draft) => {
                match draft {
                    CreateWorktreeDraft::Create {
                        agent_id,
                        base_branch,
                        prompt,
                    } => {
                        message.insert("agentId".to_string(), serde_json::json!(agent_id));
                        message.insert("baseBranch".to_string(), serde_json::json!(base_branch));
                        message.insert("mode".to_string(), serde_json::json!("create"));
                        message.insert("prompt".to_string(), serde_json::json!(prompt));
                    }
                    CreateWorktreeDraft::OpenExisting {
                        agent_id,
                        prompt,
                        existing_worktree_key,
                        existing_worktree_path,
                    } => {
                        insert_optional(&mut message, "agentId", agent_id.as_ref());
                        insert_optional(
                            &mut message,
                            "existingWorktreeKey",
                            existing_worktree_key.as_ref(),
                        );
                        message.insert(
                            "existingWorktreePath".to_string(),
                            serde_json::json!(existing_worktree_path),
                        );
                        message.insert("mode".to_string(), serde_json::json!("openExisting"));
                        insert_optional(&mut message, "prompt", prompt.as_ref());
                    }
                }
                self.forward_gpui_worktree_modal_command_to_sidebar(
                    "createProjectWorktree",
                    &message,
                    cx,
                );
                self.release_native_app_modal_window(kind, cx);
            }
            CreateWorktreeModalCommand::Cancel => {
                self.release_native_app_modal_window(kind, cx);
            }
        }
    }

    /// Delivers `projectWorktreesResult` and `worktreeImageFilesPicked` to the
    /// open native dialog. Returns false when no native Add Worktree dialog is
    /// open or the message is of another type, so the React host can have it.
    pub(crate) fn receive_gpui_create_worktree_modal_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let kind = GpuiAppModalKind::Worktree;
        match message.get("type").and_then(serde_json::Value::as_str) {
            Some("projectWorktreesResult") => {
                let Some(request_id) = message
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return false;
                };
                let ok = message.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
                let error = optional_text(message, "error");
                let branches = normalize_worktree_base_branch_options(message.get("branches"));
                let worktrees = normalize_existing_worktree_options(message.get("worktrees"));
                self.update_native_app_modal(
                    kind,
                    cx,
                    |modal: &mut GpuiCreateWorktreeModalWindow, _window, cx| {
                        modal.receive_project_worktrees_result(
                            &request_id,
                            ok,
                            error,
                            branches,
                            worktrees,
                            cx,
                        );
                    },
                )
                .is_some()
            }
            Some("worktreeImageFilesPicked") => {
                let paths: Vec<String> = message
                    .get("paths")
                    .and_then(serde_json::Value::as_array)
                    .map(|paths| {
                        paths
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .filter(|path| !path.trim().is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                self.update_native_app_modal(
                    kind,
                    cx,
                    |modal: &mut GpuiCreateWorktreeModalWindow, window, cx| {
                        modal.receive_image_files_picked(paths, window, cx);
                    },
                )
                .is_some()
            }
            _ => false,
        }
    }
}

/// The HUD agents that can start the first session: those with a non-empty
/// command, the filter `WorktreeCreateModal` applies to `agents`.
fn gpui_create_worktree_command_agents(agents: &[serde_json::Value]) -> Vec<CreateWorktreeAgent> {
    agents
        .iter()
        .filter_map(|value| {
            let field = |key: &str| {
                value
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
            };
            field("command")?;
            Some(CreateWorktreeAgent {
                agent_id: field("agentId")?,
                name: field("name")?,
            })
        })
        .collect()
}
