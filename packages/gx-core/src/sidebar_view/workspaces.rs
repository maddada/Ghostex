//! The workspace a window shows: which of this computer's projects and Spaces it lists.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: each workspace has its own Spaces and projects, and a window shows one workspace at a
//! time. A project or Space with no `workspaceId` belongs to the default workspace, and a worktree
//! project follows its parent checkout's.
//!
//! CDXC:Workspaces 2026-10-09 WHY:
//! Only this computer's projects are filtered one by one: the window's workspace is one of this
//! computer's workspaces, and a remote machine's projects carry that machine's own workspace ids,
//! so a remote machine's tab is placed whole instead (`window_machine_tabs`). A daemon without
//! workspaces (no `sidebarWorkspaces`) is never filtered.

use std::borrow::Cow;
use std::collections::BTreeSet;

use ghostex_gx_protocol::{
    PresentationProject, SidebarSpacesState, SidebarWorkspace, SidebarWorkspacesState,
};

use crate::keys::MachineId;
use crate::presentation_store::{LoadedPresentation, PresentationStore};

use super::inputs::{MachineTabInput, SidebarInputs};

/// One machine's workspaces and the one this window shows there.
pub struct WindowWorkspace<'a> {
    pub state: &'a SidebarWorkspacesState,
    pub workspace_id: &'a str,
    /// The other workspaces whose Spaces this window shows after its own (the window workspace's
    /// `showSpacesFrom`), while Spaces are on; empty otherwise.
    pub joined: Vec<&'a str>,
    /// The projects of `joined` workspaces one of their Spaces claims: the only projects of
    /// theirs this window lists.
    joined_projects: BTreeSet<String>,
}

impl<'a> WindowWorkspace<'a> {
    pub fn workspace(&self) -> Option<&'a SidebarWorkspace> {
        self.state.workspaces.get(self.workspace_id)
    }

    /// Whether a Space with this `workspaceId` belongs to the window's own workspace. The Spaces
    /// row also draws the joined workspaces' Spaces ([`Self::row_shows_space`]); the places that
    /// file a project or a new Space into a Space ask this.
    pub fn shows_space(&self, space_workspace_id: Option<&str>) -> bool {
        self.state.resolve(space_workspace_id) == self.workspace_id
    }

    /// Whether the Spaces row draws a Space with this `workspaceId`: the window's own, or one of a
    /// joined workspace (CDXC:Workspaces 2026-11-26 in gx-protocol side_state.rs).
    pub fn row_shows_space(&self, space_workspace_id: Option<&str>) -> bool {
        let workspace_id = self.state.resolve(space_workspace_id);
        workspace_id == self.workspace_id || self.joined.contains(&workspace_id)
    }

    /// The joined workspace a Space with this `workspaceId` comes from; `None` for the window's
    /// own Spaces and any other workspace's.
    pub fn joined_workspace_of_space(&self, space_workspace_id: Option<&str>) -> Option<&'a str> {
        let workspace_id = self.state.resolve(space_workspace_id);
        self.joined.iter().copied().find(|id| *id == workspace_id)
    }

    /// Whether a project belongs to the window's workspace, or is a joined workspace's project
    /// one of that workspace's Spaces claims. A joined workspace's projects in none of its Spaces
    /// stay in its own windows (the mockup's rule: put them in a Space, or switch to it).
    pub fn shows_project(&self, loaded: &LoadedPresentation, project_id: &str) -> bool {
        match loaded.project(project_id) {
            Some(project) => {
                project.every_workspace
                    || project_workspace_id(self.state, loaded, project) == self.workspace_id
                    || self.joined_projects.contains(project_id)
            }
            // Rows the presentation does not hold (chats, placeholders) are in every workspace.
            None => true,
        }
    }

    /// Whether a project is a joined workspace's, listed here through one of its Spaces.
    pub fn is_joined_project(&self, project_id: &str) -> bool {
        self.joined_projects.contains(project_id)
    }
}

/// The projects of the `joined` workspaces that one of their Spaces claims on this computer: a
/// grouped project follows its group's Space, an ungrouped one its own, a worktree its parent's
/// (gx-core `space_claims_project`).
fn joined_space_projects(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    state: &SidebarWorkspacesState,
    joined: &[&str],
) -> BTreeSet<String> {
    let mut claimed = BTreeSet::new();
    if joined.is_empty() {
        return claimed;
    }
    let Some(entry) = store.machine(&MachineId::Local) else {
        return claimed;
    };
    let (Some(spaces), Some(loaded)) = (entry.side_state().spaces.as_ref(), entry.loaded()) else {
        return claimed;
    };
    let joined_spaces: Vec<_> = spaces
        .spaces
        .values()
        .filter(|space| joined.contains(&state.resolve(space.workspace_id.as_deref())))
        .collect();
    if joined_spaces.is_empty() {
        return claimed;
    }
    let collections = match entry.side_state().project_collections.as_ref() {
        Some(wire) => super::collections::CollectionsState::from_wire(wire),
        None => inputs
            .host
            .stored_project_collections
            .as_ref()
            .map(super::collections::CollectionsState::from_local_json)
            .unwrap_or_default(),
    };
    let collection_of = |project_id: &str| {
        collections
            .collections
            .iter()
            .find(|collection| collection.project_ids.iter().any(|id| id == project_id))
            .map(|collection| collection.collection_id.as_str())
    };
    for project in loaded.projects() {
        if !joined.contains(&project_workspace_id(state, loaded, project)) {
            continue;
        }
        let parent = project
            .worktree
            .as_ref()
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(|id| id.as_str());
        let owner = parent.unwrap_or(project.project_id.as_str());
        let collection = collection_of(owner);
        let is_claimed = joined_spaces.iter().any(|space| match collection {
            Some(collection_id) => space
                .member_collection_ids
                .iter()
                .any(|member| member == collection_id),
            None => space
                .member_project_ids
                .iter()
                .any(|member| member == owner),
        });
        if is_claimed {
            claimed.insert(project.project_id.clone());
        }
    }
    claimed
}

/// The workspace a project belongs to: its own, or its parent checkout's for a worktree project.
pub fn project_workspace_id<'a>(
    state: &'a SidebarWorkspacesState,
    loaded: &'a LoadedPresentation,
    project: &'a PresentationProject,
) -> &'a str {
    let parent = project
        .worktree
        .as_ref()
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(|id| id.as_str())
        .and_then(|parent_id| loaded.project(parent_id));
    let id = parent
        .and_then(|parent| parent.workspace_id.as_deref())
        .or(project.workspace_id.as_deref());
    state.resolve(id)
}

/// A remote machine's projects as a window of this computer places them: each one in the
/// workspace it was moved to here, else in its machine tab's (CDXC:Workspaces 2026-11-19 in
/// server/src/workspaces/store.rs `assign_remote_project_workspace_in`).
pub struct RemoteWindowWorkspace<'a> {
    pub state: &'a SidebarWorkspacesState,
    pub workspace_id: &'a str,
    pub machine_id: &'a str,
}

impl<'a> RemoteWindowWorkspace<'a> {
    /// The workspace of this computer a remote project shows in; a worktree project follows its
    /// parent checkout's.
    pub fn project_workspace_id(
        &self,
        loaded: &'a LoadedPresentation,
        project: &'a PresentationProject,
    ) -> &'a str {
        self.state
            .remote_project_workspace(self.machine_id, placed_project_id(loaded, project))
    }

    /// Whether a remote project belongs to the window's workspace.
    pub fn shows_project(&self, loaded: &'a LoadedPresentation, project_id: &str) -> bool {
        match loaded.project(project_id) {
            Some(project) => self.project_workspace_id(loaded, project) == self.workspace_id,
            None => true,
        }
    }
}

/// The project whose placement a remote project follows: its parent checkout's for a worktree
/// project the machine also publishes, else its own. Move to workspace moves that one.
pub fn placed_project_id<'a>(
    loaded: &'a LoadedPresentation,
    project: &'a PresentationProject,
) -> &'a str {
    project
        .worktree
        .as_ref()
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(|id| id.as_str())
        .filter(|parent_id| loaded.project(parent_id).is_some())
        .unwrap_or(project.project_id.as_str())
}

/// This computer's workspaces and the window's, for placing a remote machine's projects; `None`
/// for this computer or while its daemon has no workspaces.
pub fn remote_window_workspace<'a>(
    store: &'a PresentationStore,
    inputs: &'a SidebarInputs,
    machine: &'a MachineId,
) -> Option<RemoteWindowWorkspace<'a>> {
    let machine_id = machine.remote_id()?;
    let state = store
        .machine(&MachineId::Local)?
        .side_state()
        .workspaces
        .as_ref()?;
    Some(RemoteWindowWorkspace {
        state,
        workspace_id: state.resolve(inputs.host.window_workspace_id.as_deref()),
        machine_id,
    })
}

/// The workspace the window shows on `machine`; `None` when that section is not filtered.
pub fn window_workspace<'a>(
    store: &'a PresentationStore,
    inputs: &'a SidebarInputs,
    machine: &MachineId,
) -> Option<WindowWorkspace<'a>> {
    if !machine.is_local() {
        return None;
    }
    let state = store.machine(machine)?.side_state().workspaces.as_ref()?;
    let workspace_id = state.resolve(inputs.host.window_workspace_id.as_deref());
    // Another workspace's Spaces join the row only while the row is drawn: with Spaces off nothing
    // changes (the switch in Settings > Workspaces is off by default too).
    let joined = if super::machine_spaces::spaces_enabled_on(store, inputs, machine) {
        state.joined_workspace_ids(workspace_id)
    } else {
        Vec::new()
    };
    let joined_projects = joined_space_projects(store, inputs, state, &joined);
    Some(WindowWorkspace {
        state,
        workspace_id,
        joined,
        joined_projects,
    })
}

/// `machine`'s Space document as this window sees it: only the Spaces of the window's workspace.
pub fn window_spaces<'a>(
    store: &'a PresentationStore,
    inputs: &'a SidebarInputs,
    machine: &MachineId,
) -> Option<Cow<'a, SidebarSpacesState>> {
    let spaces = store.machine(machine)?.side_state().spaces.as_ref()?;
    let Some(workspace) = window_workspace(store, inputs, machine) else {
        return Some(Cow::Borrowed(spaces));
    };
    let mut filtered = spaces.clone();
    filtered
        .spaces
        .retain(|_, space| workspace.row_shows_space(space.workspace_id.as_deref()));
    filtered
        .order
        .retain(|space_id| filtered.spaces.contains_key(space_id));
    if workspace.joined.is_empty() {
        return Some(Cow::Owned(filtered));
    }
    // The joined workspaces' Spaces keep their place in the one stored order, mixed in with the
    // window's own wherever the user dragged them (CDXC:Workspaces 2026-11-26 in gx-protocol
    // side_state.rs; supersedes grouping them after the window's own).
    // A joined Space the window's workspace hid with Hide Here is hidden in this row only, the way
    // Hide Space hides one everywhere (dimmed while Show Hidden is on).
    if let Some(hidden_here) = workspace
        .workspace()
        .map(|workspace| &workspace.hidden_space_ids)
        .filter(|ids| !ids.is_empty())
    {
        for (space_id, space) in filtered.spaces.iter_mut() {
            if hidden_here.contains(space_id)
                && workspace
                    .joined_workspace_of_space(space.workspace_id.as_deref())
                    .is_some()
            {
                space.hidden = true;
            }
        }
    }
    Some(Cow::Owned(filtered))
}

/// The Spaces of joined workspaces in `machine`'s row, each with its workspace's id and name (the
/// Space's hover card names it); empty while nothing is joined.
pub fn joined_space_workspaces(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    machine: &MachineId,
) -> std::collections::BTreeMap<String, (String, String)> {
    let mut joined = std::collections::BTreeMap::new();
    let Some(workspace) = window_workspace(store, inputs, machine) else {
        return joined;
    };
    if workspace.joined.is_empty() {
        return joined;
    }
    let Some(spaces) = store
        .machine(machine)
        .and_then(|entry| entry.side_state().spaces.as_ref())
    else {
        return joined;
    };
    for space in spaces.spaces.values() {
        if let Some(workspace_id) =
            workspace.joined_workspace_of_space(space.workspace_id.as_deref())
        {
            let name = workspace
                .state
                .workspaces
                .get(workspace_id)
                .map(|workspace| workspace.name.clone())
                .unwrap_or_default();
            joined.insert(space.space_id.clone(), (workspace_id.to_string(), name));
        }
    }
    joined
}

/// The machine tabs a window shows: this computer always, and a remote machine only in the
/// workspace it was put in on this computer (Personal until moved with Move to workspace on its
/// tab), and in every workspace one of its projects was moved to (`RemoteWindowWorkspace`). Every
/// tab while this computer's daemon has no workspaces.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: remote machines' sidebar sections follow the window's workspace too. A remote machine's
/// projects carry that machine's workspace ids, which do not match this computer's, so the rule is
/// the simplest correct one: the machine's tab shows in the workspace it was assigned to on this
/// computer, assignable with Move to workspace on the tab, defaulting to Personal.
pub fn window_machine_tabs(
    store: &PresentationStore,
    window_workspace_id: Option<&str>,
    tabs: &[MachineTabInput],
) -> Vec<MachineTabInput> {
    let Some(state) = store
        .machine(&MachineId::Local)
        .and_then(|machine| machine.side_state().workspaces.as_ref())
    else {
        return tabs.to_vec();
    };
    let window = state.resolve(window_workspace_id);
    tabs.iter()
        .filter(|tab| {
            tab.is_local()
                || state.machine_shows_in(&tab.machine_id, window, |project_id| {
                    // A machine the store holds nothing for may still have the project.
                    store
                        .loaded(&MachineId::Remote(tab.machine_id.clone()))
                        .is_none_or(|loaded| loaded.project(project_id).is_some())
                })
        })
        .cloned()
        .collect()
}
