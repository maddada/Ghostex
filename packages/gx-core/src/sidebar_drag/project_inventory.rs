//! The group order a project move is computed against, and the three lookups every arm of it uses.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! A DRAWN INDEX IS NOT A STORED INDEX, and for the project moves the list in question is
//! `sidebarStore`'s `groupOrder`, which is EVERY group the projection built for the machine, in the
//! order it built them: the Chats collection, then each project followed by its own user-made
//! groups. It is not the drawn list: a Space, a tag filter or Show Hidden can take a project off
//! screen and it keeps its place in this order, so a drag computed against the drawn rows would
//! insert the project somewhere else entirely and the saved order would not be the order the user
//! saw. The same defect class has decided the answer four times in this port, so the order is
//! rebuilt here from the same `project_order` and `project_members` the view model builds its group
//! plans from rather than read off the view.
//!
//! The three lookups are `describeNativeSidebarMachine`'s: `resolveProjectId` (a project or
//! subgroup group answers its project's id, the Chats collection answers nothing),
//! `nativeProjectFamily` (a project, its worktrees, and every user-made group of all of them) and
//! `nativeCollectionGroups` (every group of one collection).
//!
//! Ported from the TypeScript `describeNativeSidebarMachine` and project drag (frozen in the
//! deleted `tooling/gx-core/sidebar-page-frozen/space-navigation.ts` and `project-drag.ts`) and
//! `spliceWorkspaceSubgroups` in the deleted `gxserver-runtime/sidebar-groups.ts`, which is what
//! gave a user-made group the same `projectContext` its project has; see git history.

use crate::core::Core;
use crate::keys::{MachineId, ProjectKey};
use crate::project_docs::{CollectionsDocument, ProjectOrderItem};
use crate::sidebar_view::membership::project_members;
use crate::sidebar_view::projects::build_project_meta;
use crate::sidebar_view::SidebarInputs;

use super::inventory::{chats_group_id, workspace_project_order};

/// One row of `groupOrder`, with the three facts every project move reads off `groupsById`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectGroupRow {
    pub(crate) group_id: String,
    /// `describeNativeSidebarMachine.resolveProjectId`. `None` for the machine's Chats collection,
    /// which spans projects and carries no `projectContext`.
    ///
    /// CDXC:RemoteMachines 2026-09-21 WHY:
    /// This is the RAW project id on either machine, and on a remote one that is deliberately NOT
    /// `projectContext.editor.projectId`: `resolveProjectId` answers
    /// `remoteMachineContext.projectId` for a remote row, and that machine's own collections and
    /// Spaces documents are keyed by it, so a port that used the scoped
    /// `remote:<machine>:project:<id>` form here would look every membership up under a key the
    /// remote daemon has never written. The two forms are the same string on this computer.
    pub(crate) project_id: Option<String>,
    /// `groupsById[id].projectContext.worktree.parentProjectId`, which a user-made group carries
    /// too because it is built from the same project row.
    pub(crate) parent_project_id: Option<String>,
    pub(crate) is_chat_collection: bool,
}

/// One machine's groups in `groupOrder` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectSection {
    pub(crate) machine: MachineId,
    /// `local`, or `remote:<machine>`.
    pub(crate) section_key: String,
    pub(crate) rows: Vec<ProjectGroupRow>,
}

impl ProjectSection {
    pub(crate) fn group_ids(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.group_id.clone()).collect()
    }

    pub(crate) fn row(&self, group_id: &str) -> Option<&ProjectGroupRow> {
        self.rows.iter().find(|row| row.group_id == group_id)
    }

    /// `section.resolveProjectId(groupId)`.
    pub(crate) fn project_of_group(&self, group_id: &str) -> Option<String> {
        self.row(group_id).and_then(|row| row.project_id.clone())
    }

    /// The items `moveProjectsWithWorktrees` orders, which is `state.groupOrder` mapped.
    ///
    /// A group with no project id takes its OWN id as the project id, which is the TypeScript's
    /// `?? id` and is what keeps the Chats collection out of every family.
    pub(crate) fn order_items(&self) -> Vec<ProjectOrderItem> {
        self.rows
            .iter()
            .map(|row| ProjectOrderItem {
                order_id: row.group_id.clone(),
                project_id: row
                    .project_id
                    .clone()
                    .unwrap_or_else(|| row.group_id.clone()),
                parent_project_id: row.parent_project_id.clone(),
                is_chat: row.is_chat_collection,
            })
            .collect()
    }

    /// `nativeProjectFamily`: the group's project, every worktree of it, and every user-made group
    /// of all of them, in `groupOrder` order.
    ///
    /// An EMPTY answer is meaningful and is what `runNativeProjectDrop` returns on: the group is
    /// not there, it has no project, or it belongs to another machine.
    pub(crate) fn project_family(&self, group_id: &str) -> Vec<String> {
        let Some(row) = self.row(group_id) else {
            return Vec::new();
        };
        let Some(project_id) = row.project_id.clone() else {
            return Vec::new();
        };
        // `group.projectContext?.worktree?.parentProjectId ?? projectId`: a worktree drags its
        // whole family, its parent included, not just itself.
        let parent = row
            .parent_project_id
            .clone()
            .unwrap_or_else(|| project_id.clone());
        self.rows
            .iter()
            .filter(|candidate| {
                candidate.project_id.as_deref() == Some(parent.as_str())
                    || candidate.parent_project_id.as_deref() == Some(parent.as_str())
            })
            .map(|candidate| candidate.group_id.clone())
            .collect()
    }

    /// The project ids of a family, in order, with the duplicates a project's user-made groups
    /// contribute left in. `moveProjectsToSidebarCollection` de-duplicates, and a port that
    /// de-duplicated here would also change the order `reorderSidebarProjectCollections` sees.
    pub(crate) fn project_ids_of(&self, group_ids: &[String]) -> Vec<String> {
        group_ids
            .iter()
            .filter_map(|group_id| self.project_of_group(group_id))
            .collect()
    }

    /// `nativeCollectionGroups`: every group whose project resolves to this collection, worktree
    /// inheritance included.
    pub(crate) fn collection_groups(
        &self,
        collections: &CollectionsDocument,
        collection_id: &str,
    ) -> Vec<String> {
        let membership = self.collection_id_by_project(collections);
        self.rows
            .iter()
            .filter(|row| {
                let project_id = row.project_id.clone().unwrap_or_default();
                membership
                    .iter()
                    .any(|(candidate, id)| *candidate == project_id && id == collection_id)
            })
            .map(|row| row.group_id.clone())
            .collect()
    }

    /// `createProjectCollectionIdByProjectId`: the collection each project resolves to, including
    /// what a worktree inherits from its parent.
    pub(crate) fn collection_id_by_project(
        &self,
        collections: &CollectionsDocument,
    ) -> Vec<(String, String)> {
        let mut result: Vec<(String, String)> = Vec::new();
        for collection in &collections.state.collections {
            for project_id in &collection.project_ids {
                set_entry(
                    &mut result,
                    project_id.clone(),
                    collection.collection_id.clone(),
                );
            }
        }
        for row in &self.rows {
            let inherited = row.parent_project_id.as_ref().and_then(|parent| {
                result
                    .iter()
                    .find(|(candidate, _)| candidate == parent)
                    .map(|(_, collection_id)| collection_id.clone())
            });
            if let (Some(project_id), Some(inherited)) = (row.project_id.clone(), inherited) {
                set_entry(&mut result, project_id, inherited);
            }
        }
        result
    }

    /// The collection a project belongs to, or `None`.
    pub(crate) fn collection_of_project(
        &self,
        collections: &CollectionsDocument,
        project_id: &str,
    ) -> Option<String> {
        self.collection_id_by_project(collections)
            .into_iter()
            .find(|(candidate, _)| candidate == project_id)
            .map(|(_, collection_id)| collection_id)
    }
}

/// The group order a project move is computed against, for a probe. Public so a gate can record
/// the list the planner really used rather than describe one beside it: a record that can drift
/// from the list that decided is a record that agrees with any answer.
pub fn sidebar_project_group_order(core: &Core, inputs: &SidebarInputs) -> Option<Vec<String>> {
    project_section(core, inputs, &MachineId::Local).map(|section| section.group_ids())
}

/// `map.set(key, value)`: a later write REPLACES an earlier one, which is how a worktree's
/// inherited collection overrides the one its own row named.
fn set_entry(entries: &mut Vec<(String, String)>, key: String, value: String) {
    match entries.iter_mut().find(|(candidate, _)| *candidate == key) {
        Some((_, held)) => *held = value,
        None => entries.push((key, value)),
    }
}

/// One machine's `groupOrder`, rebuilt from the projection's own inputs.
pub(crate) fn project_section(
    core: &Core,
    inputs: &SidebarInputs,
    machine: &MachineId,
) -> Option<ProjectSection> {
    let store = core.presentation();
    let entry = store.machine(machine)?;
    let meta = build_project_meta(
        entry,
        workspace_project_order(core, machine),
        inputs.host.parked_project_ids(machine),
    );
    // The Chats collection is FIRST, exactly as the group plans build it, and it is in the order
    // even when the list draws no row for it: `groupOrder` is the projection's list, not the drawn
    // one, and `moveProjectsWithWorktrees` puts every `isChat` item at the front.
    let mut rows = vec![ProjectGroupRow {
        group_id: chats_group_id(machine),
        project_id: None,
        parent_project_id: None,
        is_chat_collection: true,
    }];
    for project_id in &meta.project_order {
        let project = ProjectKey {
            machine: machine.clone(),
            project_id: project_id.clone(),
        };
        // The raw project id, which is what `resolveProjectId` answers on either machine and
        // therefore what every collection and Space membership in THAT machine's documents is
        // keyed by.
        let resolved_project_id = project.project_id.clone();
        let parent_project_id = meta
            .overlay(project_id)
            .and_then(|overlay| overlay.worktree.as_ref())
            .map(|worktree| worktree.parent_project_id.clone());
        rows.push(ProjectGroupRow {
            group_id: project.to_sidebar_group_id(),
            project_id: Some(resolved_project_id),
            parent_project_id: parent_project_id.clone(),
            is_chat_collection: false,
        });
        for subgroup in project_members(store, entry, machine, project_id, true).subgroups {
            // CDXC:Projects 2026-09-21 WHY:
            // A user-made group carries NO `projectContext`, so `resolveProjectId` answers
            // `undefined` for it and every project lookup treats it as its OWN thing: it is not in
            // its project's family, it inherits no collection, and `moveProjectsWithWorktrees`
            // gives it its own group id as the project id through `?? id`. The first cut of this
            // file assumed the opposite, because `spliceWorkspaceSubgroups` builds a subgroup from
            // its project's row and it looked like it would carry the project's context; the gate
            // proved it does not, and the symptom was a project drag emitting its worktrees once
            // per user-made group and leaving the groups behind at the old position. Checked
            // against the shipped projection rather than reasoned about.
            rows.push(ProjectGroupRow {
                group_id: subgroup.sidebar_group_id,
                project_id: None,
                parent_project_id: None,
                is_chat_collection: false,
            });
        }
    }
    Some(ProjectSection {
        machine: machine.clone(),
        section_key: section_key(machine),
        rows,
    })
}

/// The machine tab a gesture was made on, which every arm of the project moves reads as
/// `ui.selectedMachineId`.
pub(crate) fn selected_machine(selected_machine_id: &str) -> MachineId {
    match selected_machine_id == crate::sidebar_view::LOCAL_MACHINE_ID {
        true => MachineId::Local,
        false => MachineId::Remote(selected_machine_id.to_string()),
    }
}

/// `machineId === 'local' ? 'local' : `remote:${machineId}``.
pub(crate) fn section_key(machine: &MachineId) -> String {
    match machine {
        MachineId::Local => "local".to_string(),
        MachineId::Remote(machine_id) => format!("remote:{}", machine_id.as_str()),
    }
}
