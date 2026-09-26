//! The Add Worktree dialog's Create and Open Existing (`createProjectWorktree`).
//!
//! CDXC:Worktrees 2026-09-25 WHY:
//! This computer's create used to orchestrate about eight calls client-side (register the parent,
//! find a free branch and folder, `git worktree add`, register the checkout, Beads hooks, the setup
//! command) while a remote machine made the one id-scoped call `/api/createProjectWorktree`. Both
//! make that call now, and Open Existing is `/api/openProjectWorktree` by daemon key on both. The
//! first agent session is started here afterwards, as before.
//!
//! CDXC:Worktrees 2026-06-24-14:06:
//! A create that names a prompt starts the chosen agent in the new worktree with that prompt;
//! Open Existing with no prompt selects the worktree without starting an agent.

use ghostex_gx_core::ProjectKey;
use ghostex_gx_core::git_menu::{
    GitToastLevel, worktree_slug_from_prompt, worktree_user_visible_error,
};
use serde_json::{Value, json};

use super::calls;
use super::prompt_agent::{HudAgent, PromptAgentLaunch};
use super::scope::{GitScope, worktree_facts};
use super::toasts::{ToastOptions, new_toast_id};
use super::worktree_list::WorktreeDialogScope;
use crate::GhostexGpuiApp;

const CREATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);
const OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// The dialog's create or open request.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorktreeCreateRequest {
    pub(crate) dialog: WorktreeDialogScope,
    pub(crate) agent_id: Option<String>,
    pub(crate) base_branch: Option<String>,
    pub(crate) existing_worktree_key: Option<String>,
    pub(crate) existing_worktree_path: Option<String>,
    pub(crate) open_existing: bool,
    pub(crate) prompt: String,
}

impl WorktreeCreateRequest {
    pub(crate) fn read(command: &Value) -> Self {
        let text = |key: &str, max: usize| {
            command
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.chars().count() <= max)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let existing_worktree_key = text("existingWorktreeKey", 600);
        let existing_worktree_path = text("existingWorktreePath", 1024);
        Self {
            dialog: WorktreeDialogScope::read(command),
            agent_id: text("agentId", 300),
            base_branch: text("baseBranch", 300),
            open_existing: command.get("mode").and_then(Value::as_str) == Some("openExisting")
                || existing_worktree_key.is_some()
                || existing_worktree_path.is_some(),
            existing_worktree_key,
            existing_worktree_path,
            prompt: text("prompt", 20_000).unwrap_or_default(),
        }
    }
}

impl GhostexGpuiApp {
    pub(crate) fn git_create_project_worktree(
        &mut self,
        request: WorktreeCreateRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let toast_id = new_toast_id("worktree");
        let open = request.open_existing;
        self.git_toast(
            GitToastLevel::Info,
            if open {
                "Opening worktree"
            } else {
                "Creating worktree"
            },
            ToastOptions::persistent(&toast_id),
            cx,
        );
        let prepared = self.git_prepare_worktree_create(&request);
        let (source, call) = match prepared {
            Ok(prepared) => prepared,
            Err(message) => return self.git_worktree_create_failed(open, &toast_id, &message, cx),
        };
        let remote = source.remote.clone();
        let agent = request.agent_id.as_deref().map(|agent_id| {
            self.git_hud_agent(agent_id).unwrap_or(HudAgent {
                agent_id: agent_id.to_string(),
                ..HudAgent::default()
            })
        });
        cx.spawn(async move |this, cx| {
            let result = calls::call(remote, call.0, call.1, Some(call.2)).await;
            let _ = this.update(cx, |this, cx| {
                let answer = match result {
                    Ok(answer) => answer,
                    Err(error) => {
                        return this.git_worktree_create_failed(
                            open,
                            &toast_id,
                            &error.message,
                            cx,
                        );
                    }
                };
                let project = &answer["project"];
                let Some(project_id) = project
                    .get("projectId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                else {
                    return this.git_worktree_create_failed(
                        open,
                        &toast_id,
                        "gxserver did not register the new checkout as a worktree project.",
                        cx,
                    );
                };
                if worktree_facts(project.get("worktree")).is_none() {
                    let message = match open {
                        true => "The selected checkout is not a registered worktree.",
                        false => {
                            "gxserver did not register the new checkout as a worktree project."
                        }
                    };
                    return this.git_worktree_create_failed(open, &toast_id, message, cx);
                }
                let worktree = GitScope {
                    key: ProjectKey {
                        machine: source.key.machine.clone(),
                        project_id: project_id.clone(),
                    },
                    remote: source.remote.clone(),
                    title: project
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    path: project
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    preferences: source.preferences,
                    worktree: worktree_facts(project.get("worktree")),
                    projectless: false,
                };
                this.gx_store.git.trusted_worktrees = None;
                this.git_worktree_ready(&worktree, agent, request.prompt, open, &toast_id, cx);
            });
        })
        .detach();
    }

    /// The source project and the one call, or the refusal the dialog's toast shows.
    fn git_prepare_worktree_create(
        &self,
        request: &WorktreeCreateRequest,
    ) -> Result<(GitScope, (&'static str, Value, std::time::Duration)), String> {
        let remote = request.dialog.remote_machine_id.is_some();
        let source =
            self.git_worktree_dialog_source(&request.dialog)
                .map_err(|error| match remote {
                    true => "Reconnect the remote machine before creating a worktree.".to_string(),
                    false => error,
                })?;
        if !remote && source.path.is_none() {
            return Err("Open an active code project before creating a worktree.".to_string());
        }
        let prompt = request.prompt.trim();
        let agent_ok = |agent_id: Option<&str>| {
            agent_id.is_some_and(|agent_id| {
                remote
                    || self
                        .git_hud_agent(agent_id)
                        .is_some_and(|agent| agent.has_command())
            })
        };
        if request.open_existing {
            let key = request.existing_worktree_key.as_deref();
            if key.is_none() && request.existing_worktree_path.is_none() {
                return Err("Choose an existing worktree.".to_string());
            }
            let key = key
                .filter(|key| self.git_trusted_worktree_key(&source, key))
                .ok_or_else(|| {
                    match remote {
                        true => "Choose an existing remote worktree from the latest worktree list.",
                        false => "Choose an existing worktree from the latest worktree list.",
                    }
                    .to_string()
                })?;
            if !prompt.is_empty() && !agent_ok(request.agent_id.as_deref()) {
                return Err(match remote {
                    true => "Choose an agent before starting a remote worktree prompt.",
                    false => "Choose an agent with a configured command.",
                }
                .to_string());
            }
            return Ok((
                source.clone(),
                (
                    "/api/openProjectWorktree",
                    json!({
                        "projectId": source.project_id(),
                        "runSetupCommand": false,
                        "worktreeKey": key,
                    }),
                    OPEN_TIMEOUT,
                ),
            ));
        }
        if prompt.is_empty() {
            return Err("Worktree prompt is empty.".to_string());
        }
        let Some(base_branch) = request.base_branch.as_deref() else {
            return Err("Choose a base branch.".to_string());
        };
        if !agent_ok(request.agent_id.as_deref()) {
            return Err(match remote {
                true => "Choose an agent before creating a remote worktree.",
                false => "Choose an agent with a configured command.",
            }
            .to_string());
        }
        Ok((
            source.clone(),
            (
                "/api/createProjectWorktree",
                json!({
                    "baseRef": base_branch,
                    "nameHint": worktree_slug_from_prompt(prompt),
                    "projectId": source.project_id(),
                }),
                CREATE_TIMEOUT,
            ),
        ))
    }

    fn git_worktree_create_failed(
        &mut self,
        open: bool,
        toast_id: &str,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.git_toast(
            GitToastLevel::Error,
            if open {
                "Could not open worktree"
            } else {
                "Could not create worktree"
            },
            ToastOptions::keyed(toast_id).with_description(worktree_user_visible_error(message)),
            cx,
        );
    }

    /// The checkout is registered: start the agent with the prompt, or select the worktree.
    fn git_worktree_ready(
        &mut self,
        worktree: &GitScope,
        agent: Option<HudAgent>,
        prompt: String,
        open: bool,
        toast_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = worktree.is_remote();
        let started = match (agent, prompt.trim().is_empty()) {
            (Some(agent), false) => Some(self.git_start_prompt_agent(
                worktree,
                agent,
                PromptAgentLaunch {
                    prompt,
                    ..PromptAgentLaunch::default()
                },
                cx,
            )),
            _ => {
                self.git_select_worktree_project(worktree, cx);
                None
            }
        };
        let toast_id = toast_id.to_string();
        cx.spawn(async move |this, cx| {
            let failure = match started {
                Some(started) => started.await.err(),
                None => None,
            };
            let _ = this.update(cx, |this, cx| match failure {
                Some(message) => this.git_worktree_create_failed(open, &toast_id, &message, cx),
                None => this.git_toast(
                    GitToastLevel::Success,
                    if remote {
                        "Remote worktree ready"
                    } else {
                        "Worktree ready"
                    },
                    ToastOptions::keyed(&toast_id),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// Open Existing without a prompt: select the worktree's remembered session, and never start an
    /// agent in an empty worktree (`activateGpuiProject` with `createSessionIfEmpty: false`).
    ///
    /// CDXC:Worktrees 2026-09-23 WHY:
    /// Changing only the active project leaves the parent's session focused, so the next refresh
    /// selects the parent again; the worktree's own session has to be selected.
    fn git_select_worktree_project(&mut self, worktree: &GitScope, cx: &mut gpui::Context<Self>) {
        let has_sessions = self
            .gx_store
            .core
            .presentation()
            .loaded(&worktree.key.machine)
            .is_some_and(|loaded| {
                loaded
                    .server_sessions()
                    .any(|session| session.project_id == worktree.key.project_id)
            });
        if has_sessions {
            self.dispatch_gpui_menu_bar_project_activation(&worktree.scoped_id(), cx);
            return;
        }
        self.git_focus_project(&worktree.key, cx);
    }
}
