//! The store and its per-machine entries.

use std::borrow::Cow;
use std::collections::BTreeMap;

use ghostex_gx_protocol::{
    CustomSessionTagsState, PresentationSession, SidebarProjectCollectionsState,
    SidebarSpacesState, WorkspaceSessionGroupsState,
};
use serde_json::Value;

use super::loaded::LoadedPresentation;
use crate::connection::ConnectionState;
use crate::keys::MachineId;
use crate::overlay::{Overlays, SessionPatch};
use crate::selectors::is_chat_project_path;

/// The side-state documents of one machine. `None` means the daemon never published that
/// document (an older daemon), which is different from an empty document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SideState {
    pub workspace_groups: Option<WorkspaceSessionGroupsState>,
    pub project_collections: Option<SidebarProjectCollectionsState>,
    pub spaces: Option<SidebarSpacesState>,
    pub custom_session_tags: Option<CustomSessionTagsState>,
}

/// One side-state replacement, from a change frame or from a local edit of a client-owned
/// document. `Serialize`/`Deserialize` because `Intent` carries it and `Intent` crosses the
/// boundary.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SideStateUpdate {
    WorkspaceGroups(WorkspaceSessionGroupsState),
    ProjectCollections(SidebarProjectCollectionsState),
    Spaces(SidebarSpacesState),
    CustomSessionTags(CustomSessionTagsState),
}

/// Whether a machine's presentation has arrived.
///
/// `NotLoaded` and "loaded with no rows" are different facts and must stay different types: a consumer that reads "no tab sessions" before the first snapshot clears every restored tab, split, and session mapping.
#[derive(Clone, Debug, PartialEq)]
pub enum PresentationState {
    NotLoaded,
    Loaded(Box<LoadedPresentation>),
}

/// Everything the store holds for one machine.
#[derive(Clone, Debug, PartialEq)]
pub struct MachinePresentation {
    pub(crate) state: PresentationState,
    pub(crate) side: SideState,
    pub(crate) overlays: Overlays,
    /// Full domain project rows by project id, as far as the store has seen them (deltas carry
    /// one; the host can load the full list). Kept loose until a narrow typed view is needed.
    pub(crate) domain_projects: BTreeMap<String, Value>,
    pub(crate) connection: ConnectionState,
    /// A resubscribe was requested and no stream snapshot has answered it yet. Keeps the core from
    /// asking again for every frame it has to ignore in the meantime.
    pub(crate) resubscribe_requested: bool,
}

impl Default for MachinePresentation {
    fn default() -> Self {
        Self {
            state: PresentationState::NotLoaded,
            side: SideState::default(),
            overlays: Overlays::default(),
            domain_projects: BTreeMap::new(),
            connection: ConnectionState::default(),
            resubscribe_requested: false,
        }
    }
}

impl MachinePresentation {
    pub fn state(&self) -> &PresentationState {
        &self.state
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self.state, PresentationState::Loaded(_))
    }

    pub fn loaded(&self) -> Option<&LoadedPresentation> {
        match &self.state {
            PresentationState::Loaded(loaded) => Some(loaded),
            PresentationState::NotLoaded => None,
        }
    }

    pub(crate) fn loaded_mut(&mut self) -> Option<&mut LoadedPresentation> {
        match &mut self.state {
            PresentationState::Loaded(loaded) => Some(loaded),
            PresentationState::NotLoaded => None,
        }
    }

    pub fn side_state(&self) -> &SideState {
        &self.side
    }

    pub fn domain_project(&self, project_id: &str) -> Option<&Value> {
        self.domain_projects.get(project_id)
    }

    /// Every domain project row the store holds, by project id.
    pub fn domain_projects(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.domain_projects.iter()
    }

    pub fn is_session_hidden(&self, project_id: &str, session_id: &str) -> bool {
        self.overlays.is_session_hidden(project_id, session_id)
    }

    pub fn is_project_hidden(&self, project_id: &str) -> bool {
        self.overlays.hidden_projects.contains(project_id)
    }

    pub fn session_patch(&self, project_id: &str, session_id: &str) -> Option<&SessionPatch> {
        self.overlays
            .session_patches
            .get(project_id, session_id)
            .map(|stored| &stored.patch)
    }

    /// Whether a project is a projectless Chats container: flagged as chat or quick on its domain
    /// row, or stored under a Ghostex chats folder (domain row path or presentation path).
    pub fn is_chat_project(&self, project_id: &str) -> bool {
        let domain = self.domain_projects.get(project_id);
        let domain_flag = |name: &str| {
            domain.is_some_and(|project| {
                project.get(name).and_then(Value::as_bool) == Some(true)
                    || project
                        .get("launchSettings")
                        .and_then(|settings| settings.get(name))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
        };
        domain_flag("isChat")
            || domain_flag("isQuick")
            || domain
                .and_then(|project| project.get("path"))
                .and_then(Value::as_str)
                .is_some_and(is_chat_project_path)
            || self
                .loaded()
                .and_then(|loaded| loaded.project(project_id))
                .and_then(|project| project.path.as_deref())
                .is_some_and(is_chat_project_path)
    }

    pub fn connection(&self) -> &ConnectionState {
        &self.connection
    }

    /// The session as the UI should see it: the daemon row with the local patch applied, or
    /// `None` when it does not exist or is hidden locally.
    pub fn effective_session(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> Option<Cow<'_, PresentationSession>> {
        if self.overlays.is_session_hidden(project_id, session_id) {
            return None;
        }
        let server = self.loaded()?.server_session(project_id, session_id)?;
        Some(
            // A stored patch is always pending: one the daemon caught up with, or moved past, is
            // dropped the moment that row arrives.
            match self.overlays.session_patches.get(project_id, session_id) {
                Some(stored) => Cow::Owned(stored.patch.apply_to(server)),
                None => Cow::Borrowed(server),
            },
        )
    }
}

/// The presentation of every machine, keyed by [`MachineId`]. The same reducer serves the local
/// daemon and every remote one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresentationStore {
    pub(super) machines: BTreeMap<MachineId, MachinePresentation>,
}

impl PresentationStore {
    pub fn machine(&self, machine: &MachineId) -> Option<&MachinePresentation> {
        self.machines.get(machine)
    }

    pub fn machines(&self) -> impl Iterator<Item = (&MachineId, &MachinePresentation)> {
        self.machines.iter()
    }

    /// The loaded state of a machine; `None` while it is not loaded.
    pub fn loaded(&self, machine: &MachineId) -> Option<&LoadedPresentation> {
        self.machines.get(machine)?.loaded()
    }

    /// The loaded state of a machine THIS RUN's stream delivered; `None` for one not loaded and
    /// for one whose rows are the stored last-seen copy.
    ///
    /// CDXC:RemoteMachines 2026-09-21 WHY:
    /// The difference exists for the planners that resolve a SET of rows and act on it. Drawing a
    /// last-seen machine is the point of keeping the copy, and `is_stale` fades what it draws; but
    /// the old runtime resolves those sets from `this.remotePresentations`, which holds only what a
    /// stream delivered, NOT from `remoteLastSeenPresentations`, which is the map it draws from. So
    /// a project's Sleep on an offline remote machine has always done nothing at all over there,
    /// and a port that answered it from the last-seen rows would send one doomed request per row
    /// down a tunnel that does not exist, each one a warning toast. (The `not loaded` refusal in
    /// `sidebar_actions/bulk.rs` used to give the right answer here by accident, because the store
    /// held nothing for such a machine until the copy was read back.)
    pub fn loaded_live(&self, machine: &MachineId) -> Option<&LoadedPresentation> {
        self.loaded(machine).filter(|loaded| !loaded.last_seen)
    }

    pub(super) fn machine_mut(&mut self, machine: &MachineId) -> &mut MachinePresentation {
        self.machines.entry(machine.clone()).or_default()
    }
}
