//! Where a project the Add Project dialog just added lands.
//!
//! CDXC:Spaces 2026-09-15 DECISION:
//! User: a project added through the Add Project dialog joins the Space that is open in the sidebar
//! and goes to the top of it. Two writes, and they happen at different moments: the MEMBERSHIP is
//! written the instant the dialog reports the project, because the reveal that follows the
//! activation resolves the project's Space and must find it there; the ORDER can only be written
//! once the daemon has listed the project and the sidebar has a group for it, which is one or more
//! snapshots later, so the host holds the project id until then.
//!
//! The membership is written only when the open view is a real Space (never the built-in Other,
//! which claims nothing) and only when the project is not already shown by it: a project the Space
//! already inherits through its Project Group is left alone rather than pinned to the Space
//! directly, which would survive the group moving out of it.
//!
//! SEE-ALSO: the deleted sidebar page's `events.ts`,
//! packages/core-ui/spaces.ts (deleted 2026-10-01) (`addSpaceProjectMember`),
//! packages/core-ui/sidebar-app/drag-drop-geometry.ts (deleted 2026-10-01) (`moveProjectGroupFamilyToStart`),
//! apps/desktop/src/app/gx_store/added_project.rs.

use crate::core::Core;
use crate::keys::MachineId;
use crate::project_docs::{
    toggle_space_member, CollectionsDocument, SpaceMemberKind, SpacesDocument,
};
use crate::sidebar_view::machine_spaces::spaces_enabled_on;
use crate::sidebar_view::spaces::{
    resolve_selected_space, selection_shows_project, SpaceSelection,
};
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::SidebarInputs;

use super::project_inventory::{project_section, ProjectSection};

/// Whether the sidebar has a group for the added project yet, and what its arrival owes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddedProjectPlacement {
    /// The daemon has not listed the project yet. The host keeps holding it.
    Waiting,
    /// The group is in the list. The host stops holding the project, and writes this order when it
    /// is not the order already held. `group_id` is the project's sidebar group, which a project
    /// group picked in the Add Project form is filed under.
    Placed {
        group_ids: Option<Vec<String>>,
        group_id: String,
    },
}

/// A Space or a project group the Add Project form offers for a new project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewProjectPlaceChoice {
    pub id: String,
    pub label: String,
    /// For a project group: the Spaces that list it, which the project joins through the group.
    pub space_ids: Vec<String>,
}

/// Where the Add Project form puts a new project unless the user changes it, and what it offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewProjectPlacement {
    /// The machine's Spaces in the Space row's order; `None` when Spaces are off there.
    pub spaces: Option<Vec<NewProjectPlaceChoice>>,
    /// The machine's project groups.
    pub groups: Vec<NewProjectPlaceChoice>,
    /// The Space open in the sidebar; `None` for Other.
    pub space_id: Option<String>,
    /// The project group of the selected project, when the open Space shows that project.
    pub group_id: Option<String>,
}

/// What the Add Project form starts from on `machine`: the open Space and the selected project's
/// group, with every Space and group it can switch to.
///
/// CDXC:AddProject 2026-11-05 DECISION:
/// User (Q1309): a new project goes where you are: the Space open in the sidebar and the project
/// group of the selected project, both shown in the form and changeable before it is made. A
/// selected project the open Space does not show lends no group, so a project made while looking
/// at another Space never lands outside it.
pub fn plan_new_project_placement(
    core: &Core,
    inputs: &SidebarInputs,
    machine: &MachineId,
    collections: &CollectionsDocument,
    spaces: Option<&SpacesDocument>,
    selected_project_id: Option<&str>,
) -> NewProjectPlacement {
    let spaces = spaces.filter(|_| spaces_enabled_on(core.presentation(), inputs, machine));
    let section = project_section(core, inputs, machine);
    let workspace =
        crate::sidebar_view::workspaces::window_workspace(core.presentation(), inputs, machine);
    let selection = match (spaces, &section) {
        (Some(spaces), Some(section)) => Some(resolve_selected_space(
            &spaces.state,
            inputs
                .ui
                .collapse
                .selected_space_by_section
                .get(&section.section_key)
                .map(String::as_str),
            inputs.settings.active_projects,
        )),
        _ => None,
    };
    let space_choices = spaces.map(|spaces| {
        spaces
            .state
            .ordered()
            .into_iter()
            .filter(|space| {
                workspace
                    .as_ref()
                    .is_none_or(|workspace| workspace.shows_space(space.workspace_id.as_deref()))
            })
            .map(|space| NewProjectPlaceChoice {
                id: space.space_id.clone(),
                label: space.name.clone(),
                space_ids: Vec::new(),
            })
            .collect::<Vec<_>>()
    });
    let groups = collections
        .state
        .collections
        .iter()
        .map(|collection| NewProjectPlaceChoice {
            id: collection.collection_id.clone(),
            label: collection.title.clone(),
            space_ids: spaces
                .map(|spaces| {
                    spaces
                        .state
                        .ordered()
                        .into_iter()
                        .filter(|space| {
                            space
                                .member_collection_ids
                                .contains(&collection.collection_id)
                        })
                        .map(|space| space.space_id.clone())
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    let space_id = match &selection {
        Some(SpaceSelection::Space(space_id))
            if space_choices
                .as_ref()
                .is_some_and(|choices| choices.iter().any(|choice| &choice.id == space_id)) =>
        {
            Some(space_id.clone())
        }
        _ => None,
    };
    let group_id = selected_project_id
        .map(js_trim)
        .filter(|project_id| !project_id.is_empty())
        .zip(section.as_ref())
        .and_then(|(project_id, section)| {
            let collection_id = section.collection_of_project(collections, project_id)?;
            let shown = match (&selection, spaces) {
                (Some(selection), Some(spaces)) => {
                    let parent = section
                        .rows
                        .iter()
                        .find(|row| row.project_id.as_deref() == Some(project_id))
                        .and_then(|row| row.parent_project_id.clone());
                    selection_shows_project(
                        selection,
                        &spaces.state,
                        Some(project_id),
                        Some(&collection_id),
                        parent.as_deref(),
                    )
                }
                _ => true,
            };
            shown.then_some(collection_id)
        });
    NewProjectPlacement {
        spaces: space_choices,
        groups,
        space_id,
        group_id,
    }
}

/// The Spaces document with `project_id` filed in `space_id`, or `None` when it is there already or
/// the Space is gone: the Space the Add Project form picked for a project in no project group.
pub fn plan_new_project_space_membership(
    spaces: &SpacesDocument,
    space_id: &str,
    project_id: &str,
) -> Option<SpacesDocument> {
    let project_id = js_trim(project_id);
    if project_id.is_empty() {
        return None;
    }
    add_space_project_member(spaces, space_id, project_id)
}

/// `addSpaceProjectMember` against the open Space, or `None` when nothing is written.
///
/// `None` covers every `return state` of the TypeScript in one answer, because none of them writes:
/// Spaces off, no document, the built-in Other view, a project the Space already shows, a project
/// id that is blank after the trim, and a Space id the document does not hold.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// The machine is a parameter because `receiveNativeSidebarEvent` reads
/// `describeNativeSidebarMachine(ui, message.remoteMachineId)`, not the selected tab: a project
/// added to a remote machine joins the Space open in THAT machine's section even while the user is
/// looking at this computer's. `message.projectId` is the raw id there, which is what the section's
/// rows and that machine's Spaces document are both keyed by.
pub fn plan_added_project_space_membership(
    core: &Core,
    inputs: &SidebarInputs,
    machine: &MachineId,
    collections: &CollectionsDocument,
    spaces: Option<&SpacesDocument>,
    project_id: &str,
) -> Option<SpacesDocument> {
    let project_id = js_trim(project_id);
    if project_id.is_empty() {
        return None;
    }
    // `nativeSidebarSettings().sidebarSpacesEnabled ? ui.metadata.spaces[machineId] : undefined`,
    // with the switch read off the machine itself (`machine_spaces.rs`).
    if !spaces_enabled_on(core.presentation(), inputs, machine) {
        return None;
    }
    let spaces = spaces?;
    let section = project_section(core, inputs, machine)?;
    let selection = resolve_selected_space(
        &spaces.state,
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .get(&section.section_key)
            .map(String::as_str),
        inputs.settings.active_projects,
    );
    // `section.selection?.kind === 'space'`: the built-in Other view claims no member, so adding
    // one to it is not a thing the document can express.
    let SpaceSelection::Space(space_id) = &selection else {
        return None;
    };
    let group_id = section
        .rows
        .iter()
        .find(|row| row.project_id.as_deref() == Some(project_id))
        .map(|row| row.group_id.clone());
    // `if (!groupId || !section.isVisible(groupId))`: a project the open Space already shows, by
    // its own membership or through its Project Group, keeps that membership.
    if let Some(group_id) = &group_id {
        if group_visible(&section, collections, spaces, &selection, group_id) {
            return None;
        }
    }
    add_space_project_member(spaces, space_id, project_id)
}

/// Whether the sidebar can place the added project yet, and the order that places it.
///
/// `moveProjectGroupFamilyToStart`: the project, its worktrees and their user-made groups move to
/// the FRONT of this machine's group order, everything else keeping its sequence.
pub fn plan_added_project_placement(
    core: &Core,
    inputs: &SidebarInputs,
    machine: &MachineId,
    project_id: &str,
) -> AddedProjectPlacement {
    let Some(section) = project_section(core, inputs, machine) else {
        return AddedProjectPlacement::Waiting;
    };
    let Some(group_id) = section
        .rows
        .iter()
        .find(|row| row.project_id.as_deref() == Some(project_id))
        .map(|row| row.group_id.clone())
    else {
        return AddedProjectPlacement::Waiting;
    };
    let current = section.group_ids();
    let family: Vec<String> = section.project_ids_of(&section.project_family(&group_id));
    // `projectIds.length > 0 ? [...new Set(projectIds)] : [projectId]`: the family always holds the
    // dragged project itself, so the fallback only matters for a row with no project at all, which
    // cannot be the one just added.
    let is_family = |candidate: &String| {
        section
            .project_of_group(candidate)
            .is_some_and(|project| family.contains(&project))
    };
    let next: Vec<String> = current
        .iter()
        .filter(|group_id| is_family(group_id))
        .chain(current.iter().filter(|group_id| !is_family(group_id)))
        .cloned()
        .collect();
    AddedProjectPlacement::Placed {
        // `if (groupIds.some((id, index) => id !== section.groupIds[index]))`: an order that did not
        // move posts nothing, which is the one place in the project family where an unchanged write
        // really is skipped.
        group_ids: (next != current).then_some(next),
        group_id,
    }
}

/// `addSpaceProjectMember`: a project that is already a direct member, or a Space the document does
/// not hold, leaves the document alone.
fn add_space_project_member(
    document: &SpacesDocument,
    space_id: &str,
    project_id: &str,
) -> Option<SpacesDocument> {
    // `state.spaces[spaceId]?.memberProjectIds.includes(trimmed) !== false` is `true` for BOTH an
    // unknown Space (`undefined !== false`) and a project already in it, and either way the
    // TypeScript returns the state untouched.
    let space = document.state.spaces.get(space_id)?;
    if space
        .member_project_ids
        .iter()
        .any(|member| member == project_id)
    {
        return None;
    }
    Some(toggle_space_member(
        document,
        space_id,
        SpaceMemberKind::Project,
        project_id,
    ))
}

/// `createSelectedSidebarSpaceVisibility(...)(groupId)`.
fn group_visible(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    spaces: &SpacesDocument,
    selection: &SpaceSelection,
    group_id: &str,
) -> bool {
    let Some(row) = section.row(group_id) else {
        return false;
    };
    let collection_id = row
        .project_id
        .as_deref()
        .and_then(|project_id| section.collection_of_project(collections, project_id));
    selection_shows_project(
        selection,
        &spaces.state,
        row.project_id.as_deref(),
        collection_id.as_deref(),
        row.parent_project_id.as_deref(),
    )
}
