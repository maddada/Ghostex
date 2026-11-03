//! Where a Ghostex Capture prompt can go: every project on every machine with its sessions, in the
//! order the sidebar draws them.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: the "send to" list of the floating prompt box orders projects and sessions "exactly same
//! as they're ordered in the sidebar of the ghostex app", with a new session as the default.

use ghostex_gx_core::{ChangeSummary, MachineId, OrderKind, ProjectKey, SidebarViewModel};

use crate::GhostexGpuiApp;

#[derive(Clone, Debug)]
pub(crate) struct CaptureTargetSession {
    pub(crate) sidebar_session_id: String,
    pub(crate) title: String,
    pub(crate) activity: String,
    pub(crate) question: bool,
    pub(crate) sleeping: bool,
    pub(crate) last_interaction_at: Option<String>,
    /// The session focused in the main window, the one the sidebar marks active.
    pub(crate) focused: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CaptureTargetProject {
    /// `ProjectKey::to_workspace_project_id`: remote projects carry their machine.
    pub(crate) workspace_project_id: String,
    pub(crate) title: String,
    /// The machine's name for a remote project.
    pub(crate) machine: Option<String>,
    pub(crate) sessions: Vec<CaptureTargetSession>,
}

impl GhostexGpuiApp {
    pub(crate) fn gx_store_capture_targets(&self, now_ms: u64) -> Vec<CaptureTargetProject> {
        let store = &self.gx_store;
        let presentation = store.core.presentation();
        let mut machines: Vec<MachineId> = vec![MachineId::Local];
        machines.extend(
            presentation
                .machines()
                .map(|(machine, _)| machine.clone())
                .filter(|machine| !machine.is_local()),
        );
        let mut projects = Vec::new();
        for machine in &machines {
            let mut inputs = store.sidebar_list.last_inputs.clone();
            inputs.ui.selected_machine_id = match machine.remote_id() {
                Some(id) => id.to_string(),
                None => ghostex_gx_core::LOCAL_MACHINE_ID.to_string(),
            };
            // One machine's list each, or the merged one would list the others twice.
            inputs.host.merge_machines = false;
            let mut model = SidebarViewModel::new();
            model.update(&store.core, &inputs, &ChangeSummary::default(), now_ms);
            let view = model.view();
            let group_ids = view.order.iter().flat_map(|item| match item.kind {
                OrderKind::Project => vec![item.id.clone()],
                OrderKind::Collection => view
                    .collections
                    .iter()
                    .find(|collection| collection.collection_id == item.id)
                    .map(|collection| collection.group_ids.clone())
                    .unwrap_or_default(),
            });
            for group_id in group_ids {
                let Some(group) = view.group(&group_id) else {
                    continue;
                };
                let core = &group.core;
                let Some(context) = core.project_context.as_ref() else {
                    continue;
                };
                let workspace_project_id = ProjectKey {
                    machine: machine.clone(),
                    project_id: context.project_id.clone(),
                }
                .to_workspace_project_id();
                if projects.iter().any(|project: &CaptureTargetProject| {
                    project.workspace_project_id == workspace_project_id
                }) {
                    continue;
                }
                projects.push(CaptureTargetProject {
                    workspace_project_id,
                    title: core.title.clone(),
                    machine: core
                        .remote_machine
                        .as_ref()
                        .map(|remote| remote.machine_name.clone()),
                    sessions: core
                        .sessions
                        .iter()
                        .filter(|session| !session.row.is_browser && session.row.key.is_some())
                        .map(|session| (&session.row, session.is_focused))
                        .map(|(row, focused)| CaptureTargetSession {
                            sidebar_session_id: row.sidebar_session_id.clone(),
                            title: if row.display_title.trim().is_empty() {
                                row.alias.clone()
                            } else {
                                row.display_title.clone()
                            },
                            activity: row.activity.clone(),
                            question: row.has_question(),
                            sleeping: row.lifecycle_state == "sleeping",
                            last_interaction_at: row.last_interaction_at.clone(),
                            focused,
                        })
                        .collect(),
                });
            }
        }
        projects
    }
}
