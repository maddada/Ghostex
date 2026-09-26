//! What a Git menu row, the project header's Git button, a Git hotkey and the sidebar's Git and
//! worktree menu items do. Ported from the old runtime's `runSidebarGitAction`,
//! `runSidebarGitPromptAction`, `runSidebarGitPullRequestAgentWorkflow`, `runGitMutation` and their
//! remote twins; the decisions are gx-core's (`plan_git_action_before_read`,
//! `plan_git_action_after_read`).

use ghostex_gx_core::git_menu::{
    GitAction, GitActionStep, GitMutation, GitState, GitToastLevel, agent_workflow_title,
    plan_git_action_after_read, plan_git_action_before_read, pull_request_agent_prompt,
    user_visible_git_error,
};
use ghostex_gx_core::{Event, Intent, ProjectKey};
use serde_json::{Value, json};

use super::calls;
use super::hud::GitReadOptions;
use super::prompt_agent::{HudAgent, PromptAgentLaunch};
use super::scope::{GitScope, ScopeMiss};
use super::toasts::{ToastOptions, new_toast_id};
use crate::GhostexGpuiApp;
use crate::app::consts::{
    GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
    GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
};
use crate::app::gx_store::host::now_ms;

/// Where a Git action came from.
#[derive(Clone, Debug)]
pub(crate) enum GitActionTarget {
    /// A sidebar group (a project's own, a user-made one inside it, or a remote project's).
    Group(String),
    /// A scoped project id (the titlebar's active project).
    Project(String),
}

/// The pull request agent workflow's inputs.
#[derive(Clone, Debug, Default)]
pub(crate) struct PullRequestWorkflow {
    pub(crate) agent_id: Option<String>,
    pub(crate) file_paths: Option<Vec<String>>,
    pub(crate) has_explicit_file_selection: bool,
    pub(crate) has_commit: bool,
    pub(crate) message: String,
}

impl GhostexGpuiApp {
    /// A titlebar Git menu row, the Commit button or a Git hotkey: a fixed selector only
    /// (`handleGpuiTitlebarGitAction`). `refresh` is the menu opening.
    pub(crate) fn gx_store_git_titlebar_action(
        &mut self,
        selector: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if selector == crate::app::consts::GPUI_TITLEBAR_GIT_ACTION_REFRESH_SELECTOR {
            self.git_refresh_titlebar_menu(cx);
            return true;
        }
        let Some(action) = GitAction::from_selector(selector) else {
            return false;
        };
        match self.git_active_scoped_project_id() {
            Some(active) => self.git_run_action(GitActionTarget::Project(active), action, cx),
            None => self.git_toast(
                GitToastLevel::Warning,
                "Git unavailable",
                ToastOptions::described("No active gxserver project is available."),
                cx,
            ),
        }
        true
    }

    /// The sidebar commands this family answers. Returns whether it did, in which case the command
    /// must not also reach the old runtime.
    pub(crate) fn gx_store_run_sidebar_git(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        let text = |key: &str| {
            message
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        match message.get("type").and_then(Value::as_str) {
            Some("runSidebarGitAction") => {
                let Some(action) =
                    text("action").and_then(|action| GitAction::from_selector(&action))
                else {
                    return true;
                };
                let target = match (text("groupId"), text("projectId")) {
                    (Some(group_id), _) => GitActionTarget::Group(group_id),
                    (None, Some(project_id)) => GitActionTarget::Project(project_id),
                    (None, None) => match self.git_active_scoped_project_id() {
                        Some(active) => GitActionTarget::Project(active),
                        None => {
                            self.git_toast(
                                GitToastLevel::Warning,
                                "Git unavailable",
                                ToastOptions::described("No active gxserver project is available."),
                                cx,
                            );
                            return true;
                        }
                    },
                };
                self.git_run_action(target, action, cx);
                true
            }
            Some("exportSessionTranscript") => {
                if let Some(session_id) = text("sessionId") {
                    self.git_export_session_transcript(&session_id, None, cx);
                }
                true
            }
            Some("promptDeleteWorktreeForGroup") => {
                if let Some(group_id) = text("groupId") {
                    self.git_prompt_delete_worktree(&group_id, cx);
                }
                true
            }
            Some("promptRenameWorktreeForGroup") => {
                if let Some(group_id) = text("groupId") {
                    self.git_prompt_rename_worktree(&group_id, cx);
                }
                true
            }
            _ => false,
        }
    }

    /// `runSidebarGitAction`.
    pub(crate) fn git_run_action(
        &mut self,
        target: GitActionTarget,
        action: GitAction,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.git.counters.actions += 1;
        let names_remote = match &target {
            GitActionTarget::Group(id) | GitActionTarget::Project(id) => id.starts_with("remote:"),
        };
        let resolved = match &target {
            GitActionTarget::Group(group_id) => self.git_scope_for_group(group_id),
            GitActionTarget::Project(project_id) => self.git_scope_for_project(project_id),
        };
        let scope = match resolved {
            Ok(scope) => scope,
            Err(ScopeMiss::RemoteUnavailable) => {
                return self.git_remote_actions_unavailable(cx);
            }
            Err(ScopeMiss::Unknown) if names_remote => {
                return self.git_remote_actions_unavailable(cx);
            }
            Err(ScopeMiss::Unknown) => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Git unavailable",
                    ToastOptions::described("No active gxserver project is available."),
                    cx,
                );
            }
        };
        if !scope.is_remote() && !self.git_scope_is_active(&scope) {
            self.git_focus_project(&scope.key, cx);
        }
        if let Some(GitActionStep::PromptWorkflow { title, prompt }) =
            plan_git_action_before_read(action)
        {
            self.git_run_prompt_workflow(scope, title, prompt, None, None, cx);
            return;
        }
        let read = self.git_refresh_state(
            scope.clone(),
            match scope.is_remote() {
                true => GitReadOptions::default(),
                false => GitReadOptions {
                    force: true,
                    publish_busy: true,
                    toast_on_failure: true,
                    ..GitReadOptions::default()
                },
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            let state = read.await;
            let _ = this.update(cx, |this, cx| {
                this.git_act_on_state(scope, action, state, cx);
            });
        })
        .detach();
    }

    fn git_remote_actions_unavailable(&mut self, cx: &mut gpui::Context<Self>) {
        self.git_toast(
            GitToastLevel::Warning,
            "Remote Git unavailable",
            ToastOptions::described("Reconnect the remote machine before using Git actions."),
            cx,
        );
    }

    /// `focusProjectId`: the store's focus moves to the project and, on this computer, the Agents
    /// workspace follows it (`resolveGitProjectForMessage`, a worktree delete's parent).
    pub(crate) fn git_focus_project(&mut self, project: &ProjectKey, cx: &mut gpui::Context<Self>) {
        let output = self.gx_store.core.handle(
            Event::Intent(Intent::FocusProject {
                project: project.clone(),
            }),
            now_ms(),
        );
        if !output.changes.is_empty() {
            self.gx_store.sidebar_list.note_changes(&output.changes);
            self.gx_store_update_sidebar_list(cx);
        }
        if project.machine.is_local() {
            self.swap_agents_workspace_to_project_id(Some(project.project_id.clone()), cx);
        }
    }

    fn git_act_on_state(
        &mut self,
        scope: GitScope,
        action: GitAction,
        state: GitState,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(step) =
            plan_git_action_after_read(action, &state, scope.is_worktree(), scope.is_remote())
        else {
            return;
        };
        match step {
            GitActionStep::Toast(toast) => self.git_planned_toast(&toast, cx),
            GitActionStep::PromptWorkflow { title, prompt } => {
                self.git_run_prompt_workflow(scope, title, prompt, None, Some(state), cx);
            }
            GitActionStep::Review(review) => self.git_open_review(&scope, &state, review, cx),
            GitActionStep::OpenExistingPullRequest => {
                self.git_open_existing_pull_request(&scope, cx);
            }
            GitActionStep::PullRequestAgentWorkflow => {
                self.git_run_pull_request_agent_workflow(
                    scope,
                    state,
                    PullRequestWorkflow::default(),
                    cx,
                );
            }
            GitActionStep::Mutation {
                mutation,
                started,
                finished,
            } => {
                let params = match mutation {
                    GitMutation::Push => json!({ "projectId": scope.project_id(), "push": true }),
                    GitMutation::Sync => json!({ "projectId": scope.project_id(), "sync": true }),
                };
                self.git_run_mutation(&scope, started, finished, params, cx)
                    .detach();
            }
        }
    }

    /// The agent a workflow names, or the default prompt agent, as far as this app knows it.
    pub(crate) fn git_workflow_agent(&self, requested: Option<&str>) -> HudAgent {
        let agent_id = self.git_default_prompt_agent_id(requested);
        self.git_hud_agent(&agent_id).unwrap_or(HudAgent {
            agent_id,
            ..HudAgent::default()
        })
    }

    /// `runSidebarGitPromptAction` / `runRemoteSidebarGitPromptAction`: start a prompt agent on a
    /// repository with a fixed prompt. `known` is a state the caller read a moment ago.
    pub(crate) fn git_run_prompt_workflow(
        &mut self,
        scope: GitScope,
        title: &'static str,
        prompt: String,
        agent_id: Option<String>,
        known: Option<GitState>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.git.counters.workflows += 1;
        let read = match known {
            Some(state) => gpui::Task::ready(state),
            None => self.git_refresh_state(
                scope.clone(),
                match scope.is_remote() {
                    true => GitReadOptions::default(),
                    false => GitReadOptions {
                        force: true,
                        publish_busy: true,
                        toast_on_failure: true,
                        ..GitReadOptions::default()
                    },
                },
                cx,
            ),
        };
        cx.spawn(async move |this, cx| {
            let state = read.await;
            let started = this.update(cx, |this, cx| {
                if !state.is_repo {
                    let (title, description) = match scope.is_remote() {
                        true => (
                            "Remote Git unavailable",
                            "Open a Git repository on the remote machine to use this workflow.",
                        ),
                        false => (
                            "Git unavailable",
                            "Open a Git repository to use this workflow.",
                        ),
                    };
                    this.git_toast(
                        GitToastLevel::Warning,
                        title,
                        ToastOptions::described(description),
                        cx,
                    );
                    return None;
                }
                let agent = this.git_workflow_agent(agent_id.as_deref());
                if !scope.is_remote() && !agent.has_command() {
                    this.git_toast(
                        GitToastLevel::Error,
                        "Agent unavailable",
                        ToastOptions::described(
                            "Choose a configured prompt agent before starting this Git workflow.",
                        ),
                        cx,
                    );
                    return None;
                }
                Some(this.git_start_prompt_agent(
                    &scope,
                    agent,
                    PromptAgentLaunch {
                        prompt,
                        title: Some(agent_workflow_title(title)),
                        ..PromptAgentLaunch::default()
                    },
                    cx,
                ))
            });
            let Ok(Some(started)) = started else {
                return;
            };
            let result = started.await;
            let _ = this.update(cx, |this, cx| match (result, scope.is_remote()) {
                (Ok(_), false) => this.git_toast(
                    GitToastLevel::Success,
                    "Git workflow started",
                    ToastOptions::default(),
                    cx,
                ),
                (Ok(_), true) => this.git_toast(
                    GitToastLevel::Success,
                    "Remote Git workflow started",
                    ToastOptions::default(),
                    cx,
                ),
                // The old runtime let a local failure go unreported here.
                (Err(_), false) => {}
                (Err(_), true) => this.git_toast(
                    GitToastLevel::Error,
                    "Remote Git workflow failed",
                    ToastOptions::described(
                        "The remote gxserver could not start the selected prompt agent.",
                    ),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// `runSidebarGitPullRequestAgentWorkflow` / its remote twin.
    ///
    /// CDXC:Git 2026-06-24-16:45:
    /// Visible PR-agent workflows are for user-observable, non-delete PR creation only. The terminal
    /// session can report gxserver lifecycle/activity, but it cannot prove that `gh pr create`
    /// produced an open PR; delete-after cleanup must stay on the direct gxserver PR result path.
    pub(crate) fn git_run_pull_request_agent_workflow(
        &mut self,
        scope: GitScope,
        state: GitState,
        workflow: PullRequestWorkflow,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.git.counters.workflows += 1;
        let agent = self.git_workflow_agent(workflow.agent_id.as_deref());
        if !scope.is_remote() && !agent.has_command() {
            return self.git_toast(
                GitToastLevel::Error,
                "Agent unavailable",
                ToastOptions::described(
                    "Choose a configured prompt agent before creating a pull request.",
                ),
                cx,
            );
        }
        let selected: Vec<String> = match &workflow.file_paths {
            Some(paths) if !paths.is_empty() => paths.clone(),
            _ => state.files.iter().map(|file| file.path.clone()).collect(),
        };
        let prompt = pull_request_agent_prompt(
            workflow.has_explicit_file_selection,
            workflow.has_commit,
            workflow.message.trim(),
            &selected,
        );
        let remote = scope.is_remote();
        let started = self.git_start_prompt_agent(
            &scope,
            agent,
            PromptAgentLaunch {
                prompt,
                title: Some(agent_workflow_title("Commit, Push & PR")),
                ..PromptAgentLaunch::default()
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = started.await;
            let _ = this.update(cx, |this, cx| {
                let (level, title, description) = match (result.is_ok(), remote) {
                    (true, false) => (
                        GitToastLevel::Success,
                        "Pull request workflow started",
                        None,
                    ),
                    (true, true) => (
                        GitToastLevel::Success,
                        "Remote pull request workflow started",
                        None,
                    ),
                    (false, false) => (
                        GitToastLevel::Error,
                        "Pull request workflow failed",
                        Some("gxserver could not start the selected prompt agent."),
                    ),
                    (false, true) => (
                        GitToastLevel::Error,
                        "Remote pull request workflow failed",
                        Some("The remote gxserver could not start the selected prompt agent."),
                    ),
                };
                let options = description.map(ToastOptions::described).unwrap_or_default();
                this.git_toast(level, title, options, cx);
            });
        })
        .detach();
    }

    /// `runGitMutation` / `runRemoteGitMutation`: one gxserver ship workflow inside the started and
    /// finished toasts. The task answers the workflow's result on success.
    pub(crate) fn git_run_mutation(
        &mut self,
        scope: &GitScope,
        started: &'static str,
        finished: &'static str,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Option<Value>> {
        let toast_id = new_toast_id("git");
        self.git_toast(
            GitToastLevel::Info,
            started,
            ToastOptions::persistent(&toast_id),
            cx,
        );
        if !scope.is_remote() {
            self.git_set_busy(true, cx);
            // CDXC:Git 2026-07-29: every Git write drops the project's lease before it runs, so a
            // write that fails halfway cannot leave the pre-write state to be drawn again.
            self.git_forget_leases(&scope.project_id(), false);
        }
        let scope = scope.clone();
        let remote = scope.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = calls::run_ship_workflow(remote, params).await;
            let refreshed = this
                .update(cx, |this, cx| {
                    if scope.is_remote() {
                        return None;
                    }
                    this.git_refresh_diff_stats(None, &scope.key, cx);
                    match &result {
                        Ok(_) => Some(this.git_refresh_state(
                            scope.clone(),
                            GitReadOptions {
                                force: true,
                                ..GitReadOptions::default()
                            },
                            cx,
                        )),
                        Err(_) => {
                            this.git_set_busy(false, cx);
                            None
                        }
                    }
                })
                .ok()
                .flatten();
            if let Some(refreshed) = refreshed {
                refreshed.await;
            }
            this.update(cx, |this, cx| match result {
                Ok(value) => {
                    this.git_toast(
                        GitToastLevel::Success,
                        finished,
                        ToastOptions::keyed(&toast_id),
                        cx,
                    );
                    Some(value)
                }
                Err(error) => {
                    let fallback = match scope.is_remote() {
                        true => "Remote gxserver Git operation failed.",
                        false => "gxserver Git operation failed.",
                    };
                    this.git_toast(
                        GitToastLevel::Error,
                        &format!("{started} failed"),
                        ToastOptions::keyed(&toast_id)
                            .with_description(user_visible_git_error(&error.message, fallback)),
                        cx,
                    );
                    None
                }
            })
            .ok()
            .flatten()
        })
    }

    /// Opens the project's open pull request (`openExistingPullRequestInBrowser`, or its remote
    /// twin). Rust re-reads the pull request before it opens anything (`CDXC:Git 2026-06-24-15:43`).
    pub(crate) fn git_open_existing_pull_request(
        &mut self,
        scope: &GitScope,
        cx: &mut gpui::Context<Self>,
    ) {
        let action = match scope.is_remote() {
            true => "openRemoteExistingPullRequestInBrowser",
            false => "openExistingPullRequestInBrowser",
        };
        self.git_native_project_action(action, &scope.scoped_id(), None, cx);
    }

    /// One fixed native project-path action (`postNativeProjectPathAction`).
    pub(crate) fn git_native_project_action(
        &mut self,
        action: &str,
        scoped_project_id: &str,
        file_path: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut payload = json!({
            "action": action,
            "projectId": scoped_project_id,
            "type": GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
        });
        if let Some(file_path) = file_path {
            payload["filePath"] = json!(file_path);
        }
        self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
    }
}
