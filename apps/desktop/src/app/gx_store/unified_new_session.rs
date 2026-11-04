//! Where a new session starts and how a draft changes project: the projects the New thread picker
//! and the new chat's tray offer on any sidebar, where the picker starts, what a start remembers,
//! and the draft move itself.

use ghostex_gx_core::unified_sidebar::{
    ActiveProject, ProjectChoiceInput, UnifiedChoiceMachineInput, UnifiedFilter,
    UnifiedNewSessionTarget, UnifiedProjectChoice, repo_identity, unified_new_session_target,
    unified_project_choices, widen_filter_for_new_session,
};
use ghostex_gx_core::{LOCAL_MACHINE_ID, MachineId, ProjectKey};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// A draft on its way to another project: what starts there, and what closes once it has.
#[derive(Clone, Debug)]
pub(crate) struct PendingDraftMove {
    pub(crate) from_machine_id: String,
    pub(crate) from_project_id: String,
    pub(crate) from_session_id: String,
    /// What the composer held, which the new chat opens with.
    pub(crate) text: String,
}

pub(crate) fn group_id_of(machine_id: &str, project_id: &str) -> String {
    let machine = if machine_id == LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(machine_id.to_string())
    };
    ProjectKey {
        machine,
        project_id: project_id.to_string(),
    }
    .to_sidebar_group_id()
}

/// The params of `createAgentSession` with the moved draft's text staged for its chat, the same way
/// `ghostex create-agent --first-input-draft` does.
pub(in crate::app) fn with_moved_draft_text(
    mut params: Value,
    draft: &Option<PendingDraftMove>,
) -> Value {
    if let Some(draft) = draft.as_ref().filter(|draft| !draft.text.trim().is_empty()) {
        params["runtimeSettings"]["firstUserInputDraft"] = json!(draft.text);
    }
    params
}

impl GhostexGpuiApp {
    /// One row per repository across the machines this app holds, for the tray and the "+" of a
    /// group. It reads the store directly, so it answers with the Project sidebar selected too.
    pub(crate) fn gx_store_project_choices(&self) -> Vec<UnifiedProjectChoice> {
        let inputs = &self.gx_store.sidebar_list.last_inputs;
        let store = self.gx_store.core.presentation();
        let mut machines: Vec<(MachineId, String, bool)> =
            vec![(MachineId::Local, "Local".to_string(), true)];
        for tab in &inputs.host.machines {
            if tab.machine_id == LOCAL_MACHINE_ID {
                machines[0].1.clone_from(&tab.label);
            } else if tab.fed && !tab.this_computer {
                machines.push((
                    MachineId::Remote(tab.machine_id.clone()),
                    tab.label.clone(),
                    tab.state == "connected",
                ));
            }
        }
        let hidden = &inputs.ui.hidden_items.group_ids;
        let machine_inputs: Vec<UnifiedChoiceMachineInput> = machines
            .into_iter()
            .filter_map(|(machine, label, connected)| {
                let loaded = store.loaded(&machine)?;
                // Projects with a workspace session (a command pane's terminal isn't one), and when
                // each one's sessions last came back to the user.
                let mut with_sessions: std::collections::BTreeMap<&str, Option<i64>> =
                    std::collections::BTreeMap::new();
                for session in loaded.server_sessions().filter(|session| {
                    session.surface != ghostex_gx_protocol::SessionSurface::Commands
                }) {
                    let at = ghostex_gx_core::unified_sidebar::session_activity_ms(session);
                    let newest = with_sessions
                        .entry(session.project_id.as_str())
                        .or_insert(at);
                    *newest = (*newest).max(at);
                }
                let projects = loaded
                    .projects()
                    .iter()
                    .filter(|project| {
                        inputs.ui.show_hidden
                            || !hidden.contains(
                                &ProjectKey {
                                    machine: machine.clone(),
                                    project_id: project.project_id.clone(),
                                }
                                .to_sidebar_group_id(),
                            )
                    })
                    .filter_map(|project| {
                        Some(ProjectChoiceInput {
                            project_id: project.project_id.clone(),
                            title: project.title.clone(),
                            path: project.path.clone()?,
                            git_remote_origin_url: project.git_remote_origin_url.value().cloned(),
                            worktree_parent_path: project
                                .worktree
                                .as_ref()
                                .and_then(|worktree| worktree.get("parentProjectPath"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            has_sessions: with_sessions.contains_key(project.project_id.as_str()),
                            last_activity_ms: with_sessions
                                .get(project.project_id.as_str())
                                .copied()
                                .flatten(),
                        })
                    })
                    .collect();
                Some(UnifiedChoiceMachineInput {
                    machine,
                    label,
                    connected,
                    projects,
                })
            })
            .collect();
        let saved = &self.gx_store.unified_sidebar.saved;
        unified_project_choices(
            &machine_inputs,
            &saved.last_machines,
            &saved.recent_repos,
            &inputs.ui.selected_machine_id,
        )
    }

    /// The repository key of a project on one machine, as the project list keys its rows.
    pub(crate) fn gx_store_repo_key_of(
        &self,
        machine: &MachineId,
        project_id: &str,
    ) -> Option<String> {
        let project = self
            .gx_store
            .core
            .presentation()
            .loaded(machine)?
            .project(project_id)?
            .clone();
        let parent = project
            .worktree
            .as_ref()
            .and_then(|worktree| worktree.get("parentProjectPath"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(
            repo_identity(
                project.git_remote_origin_url.value().map(String::as_str),
                project.path.as_deref().unwrap_or_default(),
                parent.as_deref(),
                &project.title,
            )
            .key,
        )
    }

    /// Where the New thread picker starts (CDXC:Sidebar 2026-11-03 in gx-core
    /// unified_sidebar/choices.rs): the Unified filter's one repository while the Unified sidebar
    /// is on, else the open project on its machine, else the list's first project.
    pub(crate) fn gx_store_new_session_start(
        &mut self,
        choices: &[UnifiedProjectChoice],
    ) -> Option<UnifiedNewSessionTarget> {
        self.gx_store.unified_sidebar.ensure_saved_read();
        let unified = self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_sidebar;
        let filter = if unified {
            self.gx_store.unified_sidebar.filter.clone()
        } else {
            UnifiedFilter::default()
        };
        let active = self.gx_store.core.focus().active_project.clone();
        let active_repo = active
            .as_ref()
            .and_then(|project| self.gx_store_repo_key_of(&project.machine, &project.project_id));
        let machine_id = active.as_ref().map(|project| {
            project
                .machine
                .remote_id()
                .unwrap_or(LOCAL_MACHINE_ID)
                .to_string()
        });
        let active = active
            .as_ref()
            .zip(active_repo.as_deref())
            .zip(machine_id.as_deref())
            .map(|((project, repo_key), machine_id)| ActiveProject {
                repo_key,
                machine_id,
                project_id: &project.project_id,
            });
        unified_new_session_target(&filter, choices, active)
    }

    /// A session started from the New thread picker (or the tray's move): its machine is where the
    /// project starts next time, the project moves to the top of Recent, and while the Unified
    /// sidebar is on a filter that would hide the session gains its project (CDXC:Sidebar
    /// 2026-11-03 `widen_filter_for_new_session`).
    pub(crate) fn gx_store_new_session_started(
        &mut self,
        target: &UnifiedNewSessionTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let unified = self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_sidebar;
        let host = &mut self.gx_store.unified_sidebar;
        host.ensure_saved_read();
        host.saved
            .remember_new_session(&target.repo_key, &target.machine_id);
        if unified {
            host.pending_project_clear();
            host.filter = widen_filter_for_new_session(
                &host.filter,
                &target.repo_key,
                &target.machine_id,
                None,
            );
        }
        host.persist();
        if unified {
            self.gx_store_unified_sidebar_changed(&ghostex_gx_core::ChangeSummary::default(), cx);
        }
    }

    /// The New thread picker's projects and where it starts: `picker_start` when a "+" set one,
    /// else `gx_store_new_session_start`. `None` when no machine lists a project.
    pub(crate) fn gx_store_new_thread_picker_place(
        &mut self,
    ) -> Option<crate::app::window::NewThreadPickerPlace> {
        use crate::app::window::{
            NewThreadPickerMachine, NewThreadPickerPlace, NewThreadPickerProject,
        };
        // Recent reads the saved starts, which the Project sidebar never reads on its own.
        self.gx_store.unified_sidebar.ensure_saved_read();
        let choices = self.gx_store_project_choices();
        let start = match self.gx_store.unified_sidebar.picker_start.take() {
            Some(start) => Some(start),
            None => self.gx_store_new_session_start(&choices),
        }?;
        let mut projects: Vec<NewThreadPickerProject> = choices
            .iter()
            .map(|choice| NewThreadPickerProject {
                repo_key: choice.repo_key.clone(),
                label: choice.label.clone(),
                has_sessions: choice.has_sessions,
                recent: choice.recent,
                machines: choice
                    .machines
                    .iter()
                    .map(|machine| NewThreadPickerMachine {
                        machine_id: machine.machine_id.clone(),
                        label: machine.label.clone(),
                        connected: machine.connected,
                        project_id: machine.project_id.clone(),
                    })
                    .collect(),
                start_machine: choice.selected,
                detail: choice.detail.clone(),
            })
            .collect();
        let project = projects
            .iter()
            .position(|project| project.repo_key == start.repo_key)?;
        let machine = projects[project]
            .machines
            .iter()
            .position(|machine| machine.machine_id == start.machine_id)?;
        // The start can be a worktree the row doesn't point at; on that machine it is "here".
        projects[project].machines[machine].project_id = start.project_id.clone();
        Some(NewThreadPickerPlace {
            projects,
            project,
            machine,
            hotkey: crate::gpui_configured_hotkey_label("openNewThreadPalette"),
        })
    }

    /// A picker command's pick: the group its launch goes to (the launch reads `picker_target`),
    /// and what a start remembers (`gx_store_new_session_started`).
    pub(crate) fn gx_store_take_new_thread_picker_target(
        &mut self,
        target: Option<crate::app::window::NewThreadPickerTarget>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = target else {
            return;
        };
        self.gx_store.unified_sidebar.picker_target =
            Some(group_id_of(&target.machine_id, &target.project_id));
        self.gx_store_new_session_started(
            &UnifiedNewSessionTarget {
                repo_key: target.repo_key,
                machine_id: target.machine_id,
                project_id: target.project_id,
            },
            cx,
        );
    }

    /// The header's New session button and ⌘N: the New thread picker, starting where
    /// `gx_store_new_session_start` says. Nothing is activated, so it cannot start a default agent
    /// (CDXC:Sidebar 2026-10-31 WHY in gx-core unified_sidebar/choices.rs).
    pub(crate) fn open_unified_new_session(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.unified_sidebar.picker_start = None;
        self.open_gpui_new_thread_picker(cx);
    }

    /// A group's "+" and a filter row's "+": the picker on that project, on that machine.
    pub(crate) fn open_new_session_in(
        &mut self,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = if machine_id == LOCAL_MACHINE_ID {
            MachineId::Local
        } else {
            MachineId::Remote(machine_id.to_string())
        };
        let Some(repo_key) = self.gx_store_repo_key_of(&machine, project_id) else {
            return;
        };
        self.gx_store.unified_sidebar.picker_start = Some(UnifiedNewSessionTarget {
            repo_key,
            machine_id: machine_id.to_string(),
            project_id: project_id.to_string(),
        });
        self.open_gpui_new_thread_picker(cx);
    }

    /// A filter row's "+": the picker on that repository, on the machine it starts on.
    pub(crate) fn open_new_session_in_repo(
        &mut self,
        repo_key: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let choices = self.gx_store_project_choices();
        let Some(choice) = choices.iter().find(|choice| choice.repo_key == repo_key) else {
            return;
        };
        let Some(machine) = choice.selected_machine() else {
            return;
        };
        self.gx_store.unified_sidebar.picker_start = Some(UnifiedNewSessionTarget {
            repo_key: repo_key.to_string(),
            machine_id: machine.machine_id.clone(),
            project_id: machine.project_id.clone(),
        });
        self.open_gpui_new_thread_picker(cx);
    }

    /// A pick in the tray's project chip: start the same agent as a draft there, with the composer's
    /// text, open it, then close this draft.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User (Q847): "Changing the project or machine moves the draft, the same way the tray's move works". A session row can't change project, so like the worktree move this starts the same agent as a new session and closes the empty draft (`/api/draftWorktree {finish, moved}`, which only closes a session still a draft); it happens on the pick rather than on send, because the new chat has to be the one whose tray offers that project's worktrees. It reaches any machine through the launch's machine-scoped group id. If the launch fails the draft stays open with its text.
    pub(crate) fn gx_store_move_draft_to_project(
        &mut self,
        draft: PendingDraftMove,
        agent_id: &str,
        repo_key: &str,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_new_session_started(
            &UnifiedNewSessionTarget {
                repo_key: repo_key.to_string(),
                machine_id: machine_id.to_string(),
                project_id: project_id.to_string(),
            },
            cx,
        );
        self.gx_store.unified_sidebar.pending_draft_move = Some(draft);
        let message = json!({
            "type": "runSidebarAgent",
            "agentId": agent_id,
            "groupId": group_id_of(machine_id, project_id),
        });
        self.stage_agent_launch_placeholder(&message, cx);
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }

    /// The new chat is open: gxserver closes the draft it replaced.
    pub(in crate::app) fn gx_store_finish_draft_move(
        &mut self,
        draft: Option<PendingDraftMove>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = draft else {
            return;
        };
        let params = json!({
            "projectId": draft.from_project_id,
            "sessionId": draft.from_session_id,
            "finish": true,
            "moved": true,
        });
        if draft.from_machine_id == LOCAL_MACHINE_ID {
            cx.spawn(async move |_, _| {
                let _ = super::rpc::gx_rpc(None, "/api/draftWorktree", params).await;
            })
            .detach();
        } else {
            self.start_gpui_remote_sidebar_rpc(
                &draft.from_machine_id,
                "/api/draftWorktree",
                Some(params),
                std::time::Duration::from_secs(15),
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            )
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    /// The stray session: activating a project with no sessions starts its default agent, so the
    /// New session path must only ever aim the launch.
    #[test]
    fn new_session_never_activates_a_project() {
        let source = include_str!("unified_new_session.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(!code.contains(concat!("project_", "activation")));
        assert!(!code.contains(concat!("activate_", "project")));
    }
}
