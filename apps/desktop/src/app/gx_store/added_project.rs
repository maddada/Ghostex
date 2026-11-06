//! The project the Add Project dialog just added, joining the open Space and going to the top.
//!
//! CDXC:Spaces 2026-09-15 DECISION:
//! User: a project added through the Add Project dialog joins the Space that is open in the sidebar
//! and goes to the top of it. The sidebar page owned both halves (`receiveNativeSidebarEvent`), and
//! it is going away, so the app owns them: the membership is written the moment the dialog reports
//! the project, and the project id is then HELD until the daemon lists it and the sidebar has a
//! group for it, which is when the order can be written at all. A project added to a REMOTE machine
//! takes the same two halves, with the Spaces document sent down that machine's tunnel
//! (`gx_store/remote_project_docs.rs`) and the order posted as the single-machine list
//! `receiveNativeSidebarEvent` posted, which the runtime does write.
//!
//! The held id survives nothing: it is memory only and a launch that never sees the project
//! forgets it, exactly as the page's `pendingAddedProject` did.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/added_project.rs,
//! the deleted sidebar page's `events.ts`,
//! apps/desktop/src/app/gx_store/project_docs.rs.

use ghostex_gx_core::{
    AddedProjectPlacement, MachineId, NewProjectPlacement, ProjectKey, SpacesDocument,
    plan_added_project_placement, plan_added_project_space_membership, plan_new_project_placement,
    plan_new_project_space_membership,
};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// `message.remoteMachineId` as the planners ask for it.
fn machine_of(remote_machine_id: Option<&str>) -> MachineId {
    match remote_machine_id {
        Some(machine_id) => MachineId::Remote(machine_id.to_string()),
        None => MachineId::Local,
    }
}

/// What Add Project's New project form picked for the project it made (window/add_project_modal/new_project.rs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NewProjectChoice {
    /// The Space to file the project in when it is in no project group; `None` for none.
    pub(crate) space_id: Option<String>,
    /// The project group it joins, which also gives it that group's Spaces.
    pub(crate) collection_id: Option<String>,
    pub(crate) start: NewProjectStart,
    /// The folder to save as Settings > Projects > Default projects folder, when none was set.
    pub(crate) save_default_folder: Option<String>,
}

/// What the new project starts once it is added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum NewProjectStart {
    /// The default agent, as any added project starts (an old dialog sent no pick).
    #[default]
    Default,
    Agent(String),
    Terminal,
    Nothing,
}

/// Client storage key of the last Start the form made a project with.
const NEW_PROJECT_START_KEY: &str = "ghostex-add-project-start";

/// The last Start the New project form made a project with (`agent:<id>`, `terminal`, `none`).
pub(crate) fn read_new_project_start() -> Option<String> {
    super::sidebar_ui_storage::read_preference_value(NEW_PROJECT_START_KEY)
        .ok()
        .flatten()
        .filter(|value| value.len() <= 160)
}

impl NewProjectChoice {
    /// The add request's `newProject`, every string bounded; `None` when there is none.
    pub(crate) fn from_params(value: Option<&Value>) -> Option<Self> {
        let object = value?.as_object()?;
        let text = |key: &str, max: usize| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| {
                    !text.is_empty()
                        && text.chars().count() <= max
                        && !text.chars().any(char::is_control)
                })
                .map(str::to_string)
        };
        Some(Self {
            space_id: text("spaceId", 256),
            collection_id: text("collectionId", 256),
            start: text("start", 160)
                .as_deref()
                .map(NewProjectStart::from_wire)
                .unwrap_or_default(),
            save_default_folder: text("saveDefaultFolder", 4_096),
        })
    }
}

impl NewProjectStart {
    /// `agent:<id>`, `terminal` or `none`, as the form sends and stores it.
    pub(crate) fn from_wire(value: &str) -> Self {
        match value {
            "terminal" => Self::Terminal,
            "none" => Self::Nothing,
            other => other
                .strip_prefix("agent:")
                .map(str::trim)
                .filter(|agent_id| !agent_id.is_empty() && agent_id.len() <= 128)
                .map(|agent_id| Self::Agent(agent_id.to_string()))
                .unwrap_or_default(),
        }
    }

    pub(crate) fn wire(&self) -> Option<String> {
        match self {
            Self::Default => None,
            Self::Agent(agent_id) => Some(format!("agent:{agent_id}")),
            Self::Terminal => Some("terminal".to_string()),
            Self::Nothing => Some("none".to_string()),
        }
    }
}

/// The project the dialog reported and what this run did with the ones before it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddedProjectHost {
    /// The project id waiting for a sidebar group, and the machine it belongs to. Memory only.
    ///
    /// The machine rides with it because `ui.pendingAddedProject` carried one: the dialog can
    /// report a remote project while the user is looking at this computer's tab, and the order is
    /// written for the machine that owns the project, not the one on screen.
    pub(super) pending_project: Option<(Option<String>, String)>,
    /// The project group the New project form picked for the held project, filed once its sidebar
    /// group is there.
    pub(super) pending_collection_id: Option<String>,
    pub(super) counters: AddedProjectCounters,
}

/// What this app run did with the added projects. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddedProjectCounters {
    /// Projects the dialog reported for this computer.
    pub(crate) added: u64,
    /// Of those, the ones that joined the open Space.
    pub(crate) space_memberships: u64,
    /// Projects whose group appeared and whose family was moved to the top.
    pub(crate) placements: u64,
    /// Projects whose group appeared where the order was already the one wanted.
    pub(crate) placements_unchanged: u64,
    /// Of the projects, the ones reported for a REMOTE machine.
    pub(crate) remotes: u64,
}

impl GhostexGpuiApp {
    /// What Add Project's New project form starts from on a machine: the open Space, the selected
    /// project's group (only on the machine tab the sidebar shows, the one a group can be filed on),
    /// and the folder that holds the selected project's folder there.
    pub(crate) fn gx_store_new_project_placement(
        &mut self,
        remote_machine_id: Option<&str>,
    ) -> (NewProjectPlacement, Option<String>) {
        let machine = machine_of(remote_machine_id);
        let (collections, spaces) = self.gx_store_project_documents(remote_machine_id);
        let selected: Option<ProjectKey> = self
            .gx_store
            .core
            .focus()
            .active_project
            .clone()
            .filter(|project| project.machine == machine);
        let store = &self.gx_store;
        let mut placement = plan_new_project_placement(
            &store.core,
            &store.sidebar_list.last_inputs,
            &machine,
            &collections,
            spaces.as_ref(),
            selected.as_ref().map(|project| project.project_id.as_str()),
        );
        if self.gx_store_selected_remote_machine_id().as_deref() != remote_machine_id {
            placement.groups.clear();
            placement.group_id = None;
        }
        let parent = selected.and_then(|project| {
            let loaded = store.core.presentation().loaded(&machine)?;
            let row = loaded.project(&project.project_id)?;
            // A worktree's folder sits under its checkout's worktrees folder; its checkout's does not.
            let row = row
                .worktree
                .as_ref()
                .and_then(|worktree| worktree.get("parentProjectId"))
                .and_then(Value::as_str)
                .and_then(|parent_id| loaded.project(parent_id))
                .unwrap_or(row);
            let path = std::path::Path::new(row.path.as_deref()?.trim());
            let parent = path.parent()?.to_string_lossy().to_string();
            if parent.is_empty() || parent == "/" {
                return None;
            }
            Some(match (machine.is_local(), dirs_home()) {
                (true, Some(home)) if parent == home => "~".to_string(),
                (true, Some(home)) => parent
                    .strip_prefix(&format!("{home}/"))
                    .map(|rest| format!("~/{rest}"))
                    .unwrap_or(parent),
                _ => parent,
            })
        });
        (placement, parent)
    }

    /// A project the New project form made: its Start is remembered for the next one, and its folder
    /// becomes the Default projects folder when this computer had none (user, Q1310).
    pub(crate) fn gx_store_note_new_project_made(
        &mut self,
        choice: &NewProjectChoice,
        local: bool,
    ) {
        if let Some(start) = choice.start.wire() {
            let _ = super::sidebar_ui_storage::write_client_document_value(
                NEW_PROJECT_START_KEY,
                Some(&start),
            );
        }
        let Some(folder) = choice.save_default_folder.as_deref().filter(|_| local) else {
            return;
        };
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        let unset = snapshot
            .object()
            .get("defaultProjectsFolder")
            .and_then(Value::as_str)
            .is_none_or(|current| current.trim().is_empty());
        if unset {
            let mut settings = snapshot.object().clone();
            settings.insert("defaultProjectsFolder".to_string(), json!(folder));
            let _ = crate::shared_settings::write_shared_sidebar_settings_object(settings);
        }
    }

    /// The Add Project dialog reported a project. Writes the Space membership and starts holding
    /// the project id for the order. `choice` is what the New project form picked: its Space or
    /// project group replaces the open Space.
    pub(crate) fn gx_store_note_added_project(
        &mut self,
        project_id: &str,
        remote_machine_id: Option<&str>,
        choice: Option<&NewProjectChoice>,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_machine_id = remote_machine_id.map(str::to_string);
        self.gx_store.added_project.counters.added += 1;
        if remote_machine_id.is_some() {
            self.gx_store.added_project.counters.remotes += 1;
        }
        let (collections, spaces) = self.gx_store_project_documents(remote_machine_id.as_deref());
        let machine = machine_of(remote_machine_id.as_deref());
        let plan =
            match choice {
                None => {
                    let store = &self.gx_store;
                    plan_added_project_space_membership(
                        &store.core,
                        &store.sidebar_list.last_inputs,
                        &machine,
                        &collections,
                        spaces.as_ref(),
                        project_id,
                    )
                }
                // A project in a project group takes the group's Spaces.
                Some(choice) if choice.collection_id.is_some() => None,
                Some(choice) => choice.space_id.as_deref().zip(spaces.as_ref()).and_then(
                    |(space_id, spaces)| {
                        plan_new_project_space_membership(spaces, space_id, project_id)
                    },
                ),
            };
        if let Some(document) = plan {
            self.gx_store.added_project.counters.space_memberships += 1;
            match &remote_machine_id {
                Some(machine_id) => self.gx_store_send_remote_spaces(machine_id, &document, cx),
                None => self.gx_document_edit::<SpacesDocument>(document, cx),
            }
        }
        // Held even when the membership wrote nothing: the ORDER is owed for every added project,
        // and the two halves of `receiveNativeSidebarEvent` are independent of each other.
        self.gx_store.added_project.pending_project =
            Some((remote_machine_id, project_id.to_string()));
        self.gx_store.added_project.pending_collection_id =
            choice.and_then(|choice| choice.collection_id.clone());
        self.gx_store.diagnostics.added_project_noted(
            self.gx_store.added_project.counters,
            self.gx_store.added_project.pending_project.is_some(),
        );
        // The group may already be there: the dialog can be reporting a project the daemon lists
        // this instant, and the page ran its own pending check in the same call.
        self.gx_store_place_added_project(cx);
    }

    /// Moves a held project's family to the top of the group order once the sidebar has a group
    /// for it. Called on every burst, and costs one `None` check per burst when nothing is held.
    pub(crate) fn gx_store_place_added_project(&mut self, cx: &mut gpui::Context<Self>) {
        let Some((remote_machine_id, project_id)) =
            self.gx_store.added_project.pending_project.clone()
        else {
            return;
        };
        let machine = machine_of(remote_machine_id.as_deref());
        let placement = {
            let store = &self.gx_store;
            plan_added_project_placement(
                &store.core,
                &store.sidebar_list.last_inputs,
                &machine,
                &project_id,
            )
        };
        let AddedProjectPlacement::Placed {
            group_ids,
            group_id,
        } = placement
        else {
            return;
        };
        self.gx_store.added_project.pending_project = None;
        let collection_id = self.gx_store.added_project.pending_collection_id.take();
        match group_ids {
            None => self.gx_store.added_project.counters.placements_unchanged += 1,
            Some(group_ids) => {
                self.gx_store.added_project.counters.placements += 1;
                // The same entry point the project moves' own order write reaches, so the project
                // order has one implementation and not two.
                let message = json!({ "type": "syncGroupOrder", "groupIds": group_ids });
                if !self.gx_store_run_project_order_message(&message, cx) {
                    self.dispatch_native_sidebar_command(message, cx);
                }
            }
        }
        self.gx_store
            .diagnostics
            .added_project_noted(self.gx_store.added_project.counters, false);
        // The project group the New project form picked: the same Add to Group edit the project's
        // menu makes, so the project joins the group's Space as that does.
        if let Some(collection_id) = collection_id {
            let message = json!({
                "type": "projectMembership",
                "action": "moveCollection",
                "groupId": group_id,
                "collectionId": collection_id,
            });
            if !self.gx_store_run_project_move(&message, cx) {
                self.dispatch_native_sidebar_command(message, cx);
            }
        }
    }
}

/// This computer's home folder, for writing a suggested folder as `~/…`.
fn dirs_home() -> Option<String> {
    let home = ghostex_paths::GhostexPaths::resolve().home_dir;
    let home = home.to_string_lossy().trim_end_matches('/').to_string();
    (!home.is_empty()).then_some(home)
}
