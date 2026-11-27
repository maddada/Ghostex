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
    /// What the composer held, which the moved chat opens with.
    pub(crate) text: String,
    /// The selected model value, kept across the move.
    pub(crate) agent_model: Option<String>,
    /// The selected effort value, kept across the move.
    pub(crate) agent_effort: Option<String>,
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

/// The params of `createAgentSession` with the moved draft's text, model and effort staged for
/// its chat, the same way `ghostex create-agent --first-input-draft` does.
pub(in crate::app) fn with_moved_draft_text(
    mut params: Value,
    draft: &Option<PendingDraftMove>,
) -> Value {
    if let Some(draft) = draft.as_ref() {
        if !draft.text.trim().is_empty() {
            params["runtimeSettings"]["firstUserInputDraft"] = json!(draft.text);
        }
        if let Some(model) = draft
            .agent_model
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            params["agentModel"] = json!(model);
        }
        if let Some(effort) = draft
            .agent_effort
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            params["agentEffort"] = json!(effort);
        }
    }
    params
}

impl GhostexGpuiApp {
    /// The project a one-click New session (button or Cmd+N) lands in: the open session's project
    /// when there is one (ignoring Quick Automations and hidden system projects drawn under Main
    /// Hub), else this computer's No project ([`no_project_row`]), else the active project or the
    /// first non-hidden local project.
    pub(crate) fn gx_store_new_session_default_group_id(&self) -> Option<String> {
        let presentation = self.gx_store.core.presentation();
        let is_hidden_system_project = |project: &ProjectKey| {
            project.project_id == ghostex_gx_core::QUICK_AUTOMATIONS_PROJECT_ID
                || presentation
                    .loaded(&project.machine)
                    .and_then(|loaded| loaded.project(&project.project_id))
                    .is_some_and(|row| {
                        row.is_help_home
                            || row.gbots_project
                            || row.gbots_bot.is_some()
                            || row.autopilot_supervisor_project
                    })
        };
        if let Some(session) = self.gx_store.core.focus().focused_session.as_ref() {
            let project = session.project_key();
            if !is_hidden_system_project(&project) {
                return Some(project.to_sidebar_group_id());
            }
        }
        if let Some(local) = presentation.loaded(&MachineId::Local) {
            if let Some(home) = no_project_row(local.projects()) {
                return Some(ProjectKey::local(home.project_id.clone()).to_sidebar_group_id());
            }
        }
        if let Some(project) = self.gx_store.core.focus().active_project.as_ref()
            && !is_hidden_system_project(project)
        {
            return Some(project.to_sidebar_group_id());
        }
        presentation.loaded(&MachineId::Local).and_then(|local| {
            local
                .projects()
                .iter()
                .find(|project| {
                    !project.is_help_home
                        && !project.gbots_project
                        && project.gbots_bot.is_none()
                        && !project.autopilot_supervisor_project
                })
                .map(|project| ProjectKey::local(project.project_id.clone()).to_sidebar_group_id())
        })
    }

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
            let space = self.gx_store.unified_sidebar.space_for_new_session(
                &self.gx_store.core,
                &self.gx_store.sidebar_list.last_inputs,
                &target.machine_id,
                &target.project_id,
            );
            if let Some((section_key, space_id)) = space {
                self.gx_store_apply_sidebar_ui_intent(
                    ghostex_gx_core::SidebarUiIntent::SetSectionSpace {
                        section_key,
                        space_id,
                    },
                    cx,
                );
            }
            self.gx_store_unified_sidebar_changed(&ghostex_gx_core::ChangeSummary::default(), cx);
        }
    }

    /// Starts a new chat draft in `project_id` on `machine_id` (no picker).
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
        if let Some(repo_key) = self.gx_store_repo_key_of(&machine, project_id) {
            self.gx_store_new_session_started(
                &UnifiedNewSessionTarget {
                    repo_key,
                    machine_id: machine_id.to_string(),
                    project_id: project_id.to_string(),
                },
                cx,
            );
        }
        let group_id = group_id_of(machine_id, project_id);
        self.start_new_agent_session_in_group(group_id, cx);
    }

    /// Starts a new draft in `group_id` (or the default group) with Coordinator mode toggled on
    /// and shows the coordinator toast (no picker or New Coordinator modal).
    pub(crate) fn start_new_coordinator_draft(
        &mut self,
        group_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store
            .unified_sidebar
            .carry_coordinator_draft
            .set(true);
        self.gx_store_create_toast(
            "info",
            "Coordinator",
            Some("Pick a model and effort, then click Send to start this coordinator."),
            cx,
        );
        match group_id {
            Some(group_id) => self.start_new_agent_session_in_group(group_id, cx),
            None => self.start_new_agent_session(cx),
        }
    }

    /// Starts a new chat draft in `repo_key` on the machine it starts on (no picker).
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
        let group_id = group_id_of(&machine.machine_id, &machine.project_id);
        self.gx_store_new_session_started(
            &UnifiedNewSessionTarget {
                repo_key: repo_key.to_string(),
                machine_id: machine.machine_id.clone(),
                project_id: machine.project_id.clone(),
            },
            cx,
        );
        let menu_host = self.gx_store_menu_host();
        let place = ghostex_gx_core::AgentChoicePlace {
            project: Some(machine.project_id.clone()),
            ..Default::default()
        };
        let primary_id = menu_host
            .primary_agent_at(&place)
            .map(|agent| agent.agent_id.clone());
        let mut command = json!({
            "type": "projectAction",
            "groupId": group_id,
            "action": "agent",
        });
        if let Some(agent_id) = primary_id {
            command["agentId"] = json!(agent_id);
        }
        self.dispatch_native_sidebar_ui(command, cx);
    }

    /// The merged header chevron menu for a Unified project group (`agent_launcher_items`).
    pub(crate) fn gx_store_unified_project_launcher_menu(
        &mut self,
        repo_key: &str,
    ) -> Option<Value> {
        let choices = self.gx_store_project_choices();
        let choice = choices.iter().find(|choice| choice.repo_key == repo_key)?;
        let machine = choice.selected_machine()?;
        let group_id = group_id_of(&machine.machine_id, &machine.project_id);
        let menu_host = self.gx_store_menu_host();
        let place = ghostex_gx_core::AgentChoicePlace {
            project: Some(machine.project_id.clone()),
            ..Default::default()
        };
        let items = ghostex_gx_core::agent_launcher_items(&group_id, &place, &menu_host);
        Some(ghostex_gx_core::menu_to_json(&items))
    }

    /// Whether `(machine_id, project_id)` is the project a computer's "No project" drafts go to
    /// ([`no_project_row`]).
    pub(crate) fn gx_store_is_no_project_target(&self, machine_id: &str, project_id: &str) -> bool {
        let machine = if machine_id == LOCAL_MACHINE_ID {
            MachineId::Local
        } else {
            MachineId::Remote(machine_id.to_string())
        };
        self.gx_store
            .core
            .presentation()
            .loaded(&machine)
            .and_then(|loaded| no_project_row(loaded.projects()))
            .is_some_and(|project| project.project_id == project_id)
    }

    /// Every computer's No project grouped as one choice, for the "No project" mode's computer
    /// picker (each computer has its own No project, with its own id).
    pub(crate) fn gx_store_no_project_choice(
        &self,
        preferred_machine_id: &str,
    ) -> Option<UnifiedProjectChoice> {
        let inputs = &self.gx_store.sidebar_list.last_inputs;
        let store = self.gx_store.core.presentation();
        let mut machine_list: Vec<(MachineId, String, String, bool)> = vec![(
            MachineId::Local,
            LOCAL_MACHINE_ID.to_string(),
            "Local".to_string(),
            true,
        )];
        for tab in &inputs.host.machines {
            if tab.machine_id == LOCAL_MACHINE_ID {
                machine_list[0].2.clone_from(&tab.label);
            } else if tab.fed && !tab.this_computer {
                machine_list.push((
                    MachineId::Remote(tab.machine_id.clone()),
                    tab.machine_id.clone(),
                    tab.label.clone(),
                    tab.state == "connected",
                ));
            }
        }
        let mut machines = Vec::new();
        for (machine, machine_id, label, connected) in machine_list {
            let Some(loaded) = store.loaded(&machine) else {
                continue;
            };
            let Some(home) = no_project_row(loaded.projects()) else {
                continue;
            };
            machines.push(ghostex_gx_core::unified_sidebar::UnifiedChoiceMachine {
                machine_id,
                label,
                project_id: home.project_id.clone(),
                connected,
            });
        }
        if machines.is_empty() {
            return None;
        }
        let selected = machines
            .iter()
            .position(|machine| machine.machine_id == preferred_machine_id)
            .unwrap_or(0);
        Some(UnifiedProjectChoice {
            repo_key: "no-project".to_string(),
            label: ghostex_gx_core::NO_PROJECT_NAME.to_string(),
            has_sessions: true,
            recent: false,
            detail: None,
            machines,
            selected,
        })
    }

    /// Toggles a draft between "No project" and its previous/default project.
    pub(crate) fn gx_store_toggle_draft_no_project(
        &mut self,
        draft: PendingDraftMove,
        agent_id: &str,
        currently_no_project: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if !currently_no_project {
            let from_machine = if draft.from_machine_id == LOCAL_MACHINE_ID {
                MachineId::Local
            } else {
                MachineId::Remote(draft.from_machine_id.clone())
            };
            if let Some(repo_key) = self.gx_store_repo_key_of(&from_machine, &draft.from_project_id)
            {
                self.gx_store.unified_sidebar.no_project_return = Some(UnifiedNewSessionTarget {
                    repo_key,
                    machine_id: draft.from_machine_id.clone(),
                    project_id: draft.from_project_id.clone(),
                });
            }
            let Some(home_choice) = self.gx_store_no_project_choice(&draft.from_machine_id) else {
                return;
            };
            let Some(target) = home_choice
                .machines
                .get(home_choice.selected)
                .filter(|machine| machine.connected)
                .or_else(|| {
                    home_choice
                        .machines
                        .iter()
                        .find(|machine| machine.connected)
                })
            else {
                return;
            };
            let target_machine_id = target.machine_id.clone();
            let target_project_id = target.project_id.clone();
            self.gx_store_move_draft_to_project(
                draft,
                agent_id,
                &home_choice.repo_key,
                &target_machine_id,
                &target_project_id,
                cx,
            );
            return;
        }
        let choices = self.gx_store_project_choices();
        let saved_target = self
            .gx_store
            .unified_sidebar
            .no_project_return
            .take()
            .filter(|saved| {
                !self.gx_store_is_no_project_target(&saved.machine_id, &saved.project_id)
                    && choices.iter().any(|choice| {
                        choice.machines.iter().any(|machine| {
                            machine.connected
                                && machine.machine_id == saved.machine_id
                                && machine.project_id == saved.project_id
                        })
                    })
            });
        let target = saved_target.or_else(|| {
            // Prefer the most recent non-Main-Hub project on the draft's current computer, else any connected computer.
            let find_in = |require_same_machine: bool| {
                choices.iter().find_map(|choice| {
                    let machine = choice.selected_machine().filter(|machine| {
                        machine.connected
                            && (!require_same_machine
                                || machine.machine_id == draft.from_machine_id)
                            && !self.gx_store_is_no_project_target(
                                &machine.machine_id,
                                &machine.project_id,
                            )
                    })?;
                    Some(UnifiedNewSessionTarget {
                        repo_key: choice.repo_key.clone(),
                        machine_id: machine.machine_id.clone(),
                        project_id: machine.project_id.clone(),
                    })
                })
            };
            find_in(true).or_else(|| find_in(false))
        });
        if let Some(target) = target {
            self.gx_store_move_draft_to_project(
                draft,
                agent_id,
                &target.repo_key,
                &target.machine_id,
                &target.project_id,
                cx,
            );
        }
    }

    /// A pick in the tray's project or computer chip: move the same draft in place when staying on
    /// the same computer (`/api/moveDraftSession`), preserving its session id, typed text, agent,
    /// model and effort. Cross-computer moves (or older gxservers that predate `/api/moveDraftSession`)
    /// fall back to starting the draft in the target project with the same agent, model, effort and
    /// typed text, then closing the old draft.
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
        if draft.from_machine_id != machine_id {
            self.gx_store_move_draft_via_create(draft, agent_id, machine_id, project_id, cx);
            return;
        }
        let mut params = json!({
            "projectId": draft.from_project_id,
            "sessionId": draft.from_session_id,
            "targetProjectId": project_id,
            "text": draft.text,
        });
        if let Some(model) = draft.agent_model.as_deref() {
            params["agentModel"] = json!(model);
        }
        if let Some(effort) = draft.agent_effort.as_deref() {
            params["agentEffort"] = json!(effort);
        }
        let agent_id = agent_id.to_string();
        let machine_id = machine_id.to_string();
        let target_project_id = project_id.to_string();
        let preferred_interface = self.gx_store_preferred_interface(&agent_id);
        if machine_id == LOCAL_MACHINE_ID {
            cx.spawn(async move |this, cx| {
                let result = super::rpc::gx_rpc(None, "/api/moveDraftSession", params).await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok(response) => {
                        let moved_project_id = response
                            .get("projectId")
                            .and_then(Value::as_str)
                            .unwrap_or(&target_project_id);
                        let moved_session_id = response
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .unwrap_or(&draft.from_session_id);
                        this.gx_store_focus_created_session(
                            moved_project_id,
                            moved_session_id,
                            true,
                            Some(&preferred_interface),
                            cx,
                        );
                    }
                    Err(error)
                        if is_move_draft_unsupported_error(
                            error.code.as_deref(),
                            &error.message,
                        ) =>
                    {
                        this.gx_store_move_draft_via_create(
                            draft,
                            &agent_id,
                            &machine_id,
                            &target_project_id,
                            cx,
                        );
                    }
                    Err(error) => {
                        this.gx_store_create_toast(
                            "warning",
                            "Could not move draft",
                            Some(&error.message),
                            cx,
                        );
                    }
                });
            })
            .detach();
        } else {
            let task = self.start_gpui_remote_sidebar_rpc(
                &machine_id,
                "/api/moveDraftSession",
                Some(params),
                std::time::Duration::from_secs(15),
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            );
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok(response) => {
                        let moved_project_id = response
                            .get("projectId")
                            .and_then(Value::as_str)
                            .unwrap_or(&target_project_id);
                        let moved_session_id = response
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .unwrap_or(&draft.from_session_id);
                        let session = ghostex_gx_core::SessionKey::remote(
                            machine_id.as_str(),
                            moved_project_id,
                            moved_session_id,
                        );
                        let payload = ghostex_gx_core::open_remote_session_terminal(
                            &session.to_sidebar_session_id(),
                            true,
                            (preferred_interface == "chat").then_some("chat"),
                        );
                        this.receive_sidebar_native_project_path_action_payload(
                            &payload.to_string(),
                            cx,
                        );
                    }
                    Err(error)
                        if is_move_draft_unsupported_error(
                            None,
                            &crate::app::remote_conn::sidebar_rpc::gpui_remote_sidebar_rpc_failure_reason(
                                &error,
                                "",
                            ),
                        ) =>
                    {
                        this.gx_store_move_draft_via_create(
                            draft,
                            &agent_id,
                            &machine_id,
                            &target_project_id,
                            cx,
                        );
                    }
                    Err(error) => {
                        let reason = crate::app::remote_conn::sidebar_rpc::gpui_remote_sidebar_rpc_failure_reason(
                            &error,
                            "Could not move the draft to that project.",
                        );
                        this.gx_store_create_toast("warning", "Could not move draft", Some(&reason), cx);
                    }
                });
            })
            .detach();
        }
    }

    fn gx_store_move_draft_via_create(
        &mut self,
        draft: PendingDraftMove,
        agent_id: &str,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.unified_sidebar.pending_draft_move = Some(draft);
        let message = json!({
            "type": "runSidebarAgent",
            "agentId": agent_id,
            "groupId": group_id_of(machine_id, project_id),
        });
        self.stage_agent_launch_placeholder(&message, cx);
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }

    /// Remembers the target repo and computer for Recent/last-machine without touching the
    /// sidebar's Space or widening Unified filters (Q4850 `"keep-space"`).
    pub(crate) fn gx_store_remember_new_session_keep_space(
        &mut self,
        repo_key: &str,
        machine_id: &str,
    ) {
        if repo_key.trim().is_empty() {
            return;
        }
        let host = &mut self.gx_store.unified_sidebar;
        host.ensure_saved_read();
        host.saved.remember_new_session(repo_key, machine_id);
        host.persist();
    }

    /// Commits a draft's pending target destination ("No project" or another project or
    /// computer) at the moment the user presses Send (Q4849 `"follow"` + Q4850 `"keep-space"`).
    /// Runs the server move/create while the current view stays up showing the 0ms optimistic sent
    /// message, then transfers the live `NativeChatView` into the target project in a single frame
    /// without switching the sidebar's Space or widening filters.
    pub(crate) fn gx_store_commit_pending_draft_send(
        &mut self,
        view: gpui::Entity<crate::app::native_chat::state::NativeChatView>,
        send: crate::app::native_chat::state::PendingDraftSend,
        cx: &mut gpui::Context<Self>,
    ) {
        if send.from_machine_id != send.target.machine_id {
            self.gx_store_commit_pending_draft_send_via_create(view, send, cx);
            return;
        }
        let mut params = json!({
            "projectId": send.from_project_id,
            "sessionId": send.from_session_id,
            "targetProjectId": send.target.project_id,
        });
        if let Some(model) = send.agent_model.as_deref() {
            params["agentModel"] = json!(model);
        }
        if let Some(effort) = send.agent_effort.as_deref() {
            params["agentEffort"] = json!(effort);
        }
        if send.starting_coordinator {
            params["promoteCoordinator"] = json!({
                "forDraft": true,
                "skillOnlyStart": send.skill_only_start,
            });
        }
        let machine_id = send.target.machine_id.clone();
        if machine_id == LOCAL_MACHINE_ID {
            cx.spawn(async move |this, cx| {
                let result = super::rpc::gx_rpc(None, "/api/moveDraftSession", params).await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok(response) => {
                        let moved_project_id = response
                            .get("projectId")
                            .and_then(Value::as_str)
                            .unwrap_or(&send.target.project_id)
                            .to_string();
                        let moved_session_id = response
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .unwrap_or(&send.from_session_id)
                            .to_string();
                        this.gx_store_remember_new_session_keep_space(
                            &send.target.repo_key,
                            &machine_id,
                        );
                        this.follow_moved_draft_chat(
                            view,
                            send,
                            machine_id,
                            moved_project_id,
                            moved_session_id,
                            cx,
                        );
                    }
                    Err(error)
                        if is_move_draft_unsupported_error(
                            error.code.as_deref(),
                            &error.message,
                        ) =>
                    {
                        this.gx_store_commit_pending_draft_send_via_create(view, send, cx);
                    }
                    Err(error) => {
                        view.update(cx, |chat, cx| {
                            chat.restore_failed_pending_send(
                                &send.send_request_id,
                                &send.original_draft,
                                send.starting_coordinator,
                                cx,
                            );
                        });
                        this.gx_store_create_toast(
                            "warning",
                            "Could not move draft",
                            Some(&error.message),
                            cx,
                        );
                    }
                });
            })
            .detach();
        } else {
            let task = self.start_gpui_remote_sidebar_rpc(
                &machine_id,
                "/api/moveDraftSession",
                Some(params),
                std::time::Duration::from_secs(15),
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            );
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok(response) => {
                        let moved_project_id = response
                            .get("projectId")
                            .and_then(Value::as_str)
                            .unwrap_or(&send.target.project_id)
                            .to_string();
                        let moved_session_id = response
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .unwrap_or(&send.from_session_id)
                            .to_string();
                        this.gx_store_remember_new_session_keep_space(
                            &send.target.repo_key,
                            &machine_id,
                        );
                        this.follow_moved_draft_chat(
                            view,
                            send,
                            machine_id,
                            moved_project_id,
                            moved_session_id,
                            cx,
                        );
                    }
                    Err(error)
                        if is_move_draft_unsupported_error(
                            None,
                            &crate::app::remote_conn::sidebar_rpc::gpui_remote_sidebar_rpc_failure_reason(
                                &error,
                                "",
                            ),
                        ) =>
                    {
                        this.gx_store_commit_pending_draft_send_via_create(view, send, cx);
                    }
                    Err(error) => {
                        let reason = crate::app::remote_conn::sidebar_rpc::gpui_remote_sidebar_rpc_failure_reason(
                            &error,
                            "Could not move the draft to that project.",
                        );
                        view.update(cx, |chat, cx| {
                            chat.restore_failed_pending_send(
                                &send.send_request_id,
                                &send.original_draft,
                                send.starting_coordinator,
                                cx,
                            );
                        });
                        this.gx_store_create_toast("warning", "Could not move draft", Some(&reason), cx);
                    }
                });
            })
            .detach();
        }
    }

    /// Cross-computer (or older-gxserver) path for committing a pending draft send: creates the
    /// draft on the target computer, promotes it if Coordinator mode is on, quietly closes the old
    /// empty draft, and transfers the live `NativeChatView` into the new session in one frame.
    fn gx_store_commit_pending_draft_send_via_create(
        &mut self,
        view: gpui::Entity<crate::app::native_chat::state::NativeChatView>,
        send: crate::app::native_chat::state::PendingDraftSend,
        cx: &mut gpui::Context<Self>,
    ) {
        let target_machine_id = send.target.machine_id.clone();
        let target_project_id = send.target.project_id.clone();
        let mut params = json!({
            "projectId": target_project_id,
            "agentId": send.agent_id,
            "draft": true,
        });
        if let Some(model) = send.agent_model.as_deref() {
            params["agentModel"] = json!(model);
        }
        if let Some(effort) = send.agent_effort.as_deref() {
            params["agentEffort"] = json!(effort);
        }
        let finish_old = PendingDraftMove {
            from_machine_id: send.from_machine_id.clone(),
            from_project_id: send.from_project_id.clone(),
            from_session_id: send.from_session_id.clone(),
            text: String::new(),
            agent_model: None,
            agent_effort: None,
        };
        if target_machine_id == LOCAL_MACHINE_ID {
            cx.spawn(async move |this, cx| {
                let result = super::rpc::gx_rpc(None, "/api/createAgentSession", params).await;
                let Ok(response) = result else {
                    let msg = result
                        .err()
                        .map(|error| error.message)
                        .unwrap_or_else(|| "Could not start the chat in that project.".to_string());
                    let _ = this.update(cx, |this, cx| {
                        view.update(cx, |chat, cx| {
                            chat.restore_failed_pending_send(
                                &send.send_request_id,
                                &send.original_draft,
                                send.starting_coordinator,
                                cx,
                            );
                        });
                        this.gx_store_create_toast(
                            "warning",
                            "Could not start chat",
                            Some(&msg),
                            cx,
                        );
                    });
                    return;
                };
                let created_project_id = response
                    .pointer("/session/projectId")
                    .or_else(|| response.get("projectId"))
                    .and_then(Value::as_str)
                    .unwrap_or(&target_project_id)
                    .to_string();
                let Some(created_session_id) = response
                    .pointer("/session/sessionId")
                    .or_else(|| response.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                else {
                    let _ = this.update(cx, |this, cx| {
                        view.update(cx, |chat, cx| {
                            chat.restore_failed_pending_send(
                                &send.send_request_id,
                                &send.original_draft,
                                send.starting_coordinator,
                                cx,
                            );
                        });
                        this.gx_store_create_toast(
                            "warning",
                            "Could not start chat",
                            Some("gxserver did not return the new session id."),
                            cx,
                        );
                    });
                    return;
                };
                if send.starting_coordinator {
                    let promote_params = json!({
                        "projectId": created_project_id,
                        "sessionId": created_session_id,
                        "goal": "",
                        "forDraft": true,
                        "skillOnlyStart": send.skill_only_start,
                    });
                    let _ =
                        super::rpc::gx_rpc(None, "/api/promoteCoordinator", promote_params).await;
                }
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_remember_new_session_keep_space(
                        &send.target.repo_key,
                        &target_machine_id,
                    );
                    this.gx_store_finish_draft_move(Some(finish_old), cx);
                    this.follow_moved_draft_chat(
                        view,
                        send,
                        target_machine_id,
                        created_project_id,
                        created_session_id,
                        cx,
                    );
                });
            })
            .detach();
        } else {
            let task = self.start_gpui_remote_sidebar_rpc(
                &target_machine_id,
                "/api/createAgentSession",
                Some(params),
                std::time::Duration::from_secs(20),
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            );
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let Ok(response) = result else {
                    let reason = result
                        .err()
                        .map(|error| {
                            crate::app::remote_conn::sidebar_rpc::gpui_remote_sidebar_rpc_failure_reason(
                                &error,
                                "Could not start the chat on that computer.",
                            )
                        })
                        .unwrap_or_else(|| "Could not start the chat on that computer.".to_string());
                    let _ = this.update(cx, |this, cx| {
                        view.update(cx, |chat, cx| {
                            chat.restore_failed_pending_send(
                                &send.send_request_id,
                                &send.original_draft,
                                send.starting_coordinator,
                                cx,
                            );
                        });
                        this.gx_store_create_toast("warning", "Could not start chat", Some(&reason), cx);
                    });
                    return;
                };
                let created_project_id = response
                    .pointer("/session/projectId")
                    .or_else(|| response.get("projectId"))
                    .and_then(Value::as_str)
                    .unwrap_or(&target_project_id)
                    .to_string();
                let Some(created_session_id) = response
                    .pointer("/session/sessionId")
                    .or_else(|| response.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                else {
                    let _ = this.update(cx, |this, cx| {
                        view.update(cx, |chat, cx| {
                            chat.restore_failed_pending_send(
                                &send.send_request_id,
                                &send.original_draft,
                                send.starting_coordinator,
                                cx,
                            );
                        });
                        this.gx_store_create_toast(
                            "warning",
                            "Could not start chat",
                            Some("The computer did not return the new session id."),
                            cx,
                        );
                    });
                    return;
                };
                if send.starting_coordinator {
                    let promote_params = json!({
                        "projectId": created_project_id,
                        "sessionId": created_session_id,
                        "goal": "",
                        "forDraft": true,
                        "skillOnlyStart": send.skill_only_start,
                    });
                    let promote_task = this.update(cx, |this, cx| {
                        this.start_gpui_remote_sidebar_rpc(
                            &target_machine_id,
                            "/api/promoteCoordinator",
                            Some(promote_params),
                            std::time::Duration::from_secs(15),
                            GpuiRemoteSidebarRpcMode::Awaited,
                            cx,
                        )
                    });
                    if let Ok(promote_task) = promote_task {
                        let _ = promote_task.await;
                    }
                }
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_remember_new_session_keep_space(
                        &send.target.repo_key,
                        &target_machine_id,
                    );
                    this.gx_store_finish_draft_move(Some(finish_old), cx);
                    this.follow_moved_draft_chat(
                        view,
                        send,
                        target_machine_id,
                        created_project_id,
                        created_session_id,
                        cx,
                    );
                });
            })
            .detach();
        }
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

/// Whether a `/api/moveDraftSession` failure means the server predates in-place draft moves (HTTP
/// 404 or unknown/unsupported operation), so the client should fall back to create + close.
fn is_move_draft_unsupported_error(code: Option<&str>, message: &str) -> bool {
    if matches!(
        code,
        Some("notFound" | "unsupportedOperation" | "unknownEndpoint" | "conflict")
    ) {
        return true;
    }
    let trimmed = message.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return true;
    }
    let lower = trimmed.to_lowercase();
    lower.contains("404")
        || lower.contains("not found")
        || lower.contains("unknown endpoint")
        || lower.contains("unsupported")
        || lower.contains("not allowed")
        || lower.contains("rejected the request")
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

/// The project a computer's "No project" drafts and Cmd+N go to: its No project (gxserver
/// `isNoProject`), or its Main Hub on a computer whose gxserver predates No project.
///
/// CDXC:Projects 2026-11-27 DECISION:
/// User (Q5260 C, Q5285 A): chats without a project start in No project, and the draft pill
/// keeps reading "No project". Before, the pill and Cmd+N aimed at Main Hub (the project at ~);
/// a connected computer on an older gxserver has no No project, so it keeps that.
pub(crate) fn no_project_row(
    projects: &[ghostex_gx_protocol::PresentationProject],
) -> Option<&ghostex_gx_protocol::PresentationProject> {
    projects
        .iter()
        .find(|project| project.is_no_project)
        .or_else(|| {
            projects
                .iter()
                .find(|project| project.is_home && !project.is_help_home)
        })
}
