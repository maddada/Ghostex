//! Opening a project from outside its sidebar rows: the menu bar's project rows, a Quick Access
//! project row, the Automate board's Focus Project, a finished Add Project or folder pick, a
//! restored Recent Project, a new worktree, `ghostex switch-project` and `focus-group`.
//!
//! CDXC:Projects 2026-09-25 WHY:
//! This was the app runtime's `activateGpuiProject`, reached through `onMenuBarProjectActivation`.
//! It reads the project's machine afresh (`/api/readPresentationSnapshot`, as the runtime's
//! refresh did), lands on the remembered session or the first agent or terminal
//! (gx-core `plan_project_activation`), and creates the default agent (or a terminal) in a project
//! that has none, through the store's own create paths. Two rapid activations settle on the newer
//! one; the runtime ran both in turn and ended on the same project.
//!
//! SEE-ALSO: packages/gx-core/src/project_activation.rs, apps/desktop/src/app/gx_store/focus_perform.rs.

use ghostex_gx_core::protocol::PresentationSnapshot;
use ghostex_gx_core::{Intent, ProjectActivation, ProjectKey, plan_project_activation};
use serde_json::json;

use super::focus_perform::RowFocusOptions;
use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::model::GpuiPreferredAgentInterface;

/// What this app run activated. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectActivationCounters {
    pub(crate) requests: u64,
    pub(crate) focused: u64,
    pub(crate) created: u64,
    pub(crate) failed: u64,
    /// Overtaken by a newer activation before its read came back.
    pub(crate) superseded: u64,
}

#[derive(Default)]
pub(crate) struct ProjectActivationHost {
    pub(crate) counters: ProjectActivationCounters,
    /// Bumped by every activation, so a late answer can tell it is no longer the newest.
    generation: u64,
}

impl GhostexGpuiApp {
    /// `activateGpuiProject(projectId)` for a workspace project id (machine-scoped for a remote
    /// project). Returns whether the id named a project at all.
    pub(crate) fn gx_store_activate_project(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(project) = ProjectKey::parse_workspace_project_id(project_id.trim()) else {
            return false;
        };
        let host = &mut self.gx_store.project_activation;
        host.counters.requests += 1;
        host.generation += 1;
        let generation = host.generation;
        let remote = match project.machine.remote_id() {
            None => None,
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.gx_store_project_activation_failed(cx);
                    return true;
                }
            },
        };
        let remembered = Self::gx_store_read_remembered_session(&project);
        cx.spawn(async move |this, cx| {
            let read = gx_rpc(remote, "/api/readPresentationSnapshot", json!({})).await;
            // The answer is `{ snapshot }`, as the runtime's `fetchPresentationSnapshot` read it.
            let snapshot = read
                .ok()
                .and_then(|mut value| value.get_mut("snapshot").map(serde_json::Value::take))
                .and_then(|value| serde_json::from_value::<PresentationSnapshot>(value).ok());
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.project_activation.generation != generation {
                    this.gx_store.project_activation.counters.superseded += 1;
                    return;
                }
                let plan = snapshot.as_ref().and_then(|snapshot| {
                    plan_project_activation(snapshot, &project, remembered.as_deref())
                });
                match plan {
                    Some(plan) => this.gx_store_perform_project_activation(&project, plan, cx),
                    None => this.gx_store_project_activation_failed(cx),
                }
            });
        })
        .detach();
        true
    }

    fn gx_store_perform_project_activation(
        &mut self,
        project: &ProjectKey,
        plan: ProjectActivation,
        cx: &mut gpui::Context<Self>,
    ) {
        match plan {
            ProjectActivation::Focus { session, is_agent } => {
                self.gx_store.project_activation.counters.focused += 1;
                let row_id = session.to_sidebar_session_id();
                self.gx_store_focus_session_row(
                    &row_id,
                    RowFocusOptions {
                        keep_view: false,
                        keep_sleeping: true,
                        preferred_interface: Some(match is_agent {
                            true => GpuiPreferredAgentInterface::Chat,
                            false => GpuiPreferredAgentInterface::Terminal,
                        }),
                    },
                    cx,
                );
                // The landing row is scrolled into view, as the runtime's pending reveal did.
                let request_id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_micros() as u64);
                self.gx_store_note_local_sidebar_reveal(&row_id, request_id);
                self.gx_store_note_sidebar_reveal(&row_id, request_id, cx);
                self.gx_store_sidebar_state_changed(cx);
            }
            ProjectActivation::Empty => {
                self.gx_store.project_activation.counters.created += 1;
                self.gx_store_focus_intent(
                    Intent::FocusProject {
                        project: project.clone(),
                    },
                    cx,
                );
                self.gx_store_publish_workspace_focus(cx);
                let group_id = project.to_sidebar_group_id();
                let agent_id = self.git_default_prompt_agent_id(None);
                if self.gx_store_preferred_interface(&agent_id) == "chat" {
                    self.gx_store_request_agent_launch(&agent_id, Some(&group_id), None, cx);
                } else {
                    self.gx_store_create_terminal(Some(&group_id), cx).detach();
                }
            }
        }
    }

    fn gx_store_project_activation_failed(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.project_activation.counters.failed += 1;
        self.receive_gpui_app_toast_bridge_message(
            &json!({
                "level": "warning",
                "title": "Could not open the project session.",
                "type": "toast",
            }),
            cx,
        );
    }
}
