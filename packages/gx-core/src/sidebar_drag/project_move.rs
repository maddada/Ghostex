//! The project moves: reorder, a project into and out of a collection, and Space membership.
//!
//! CDXC:Projects 2026-09-21 WHY:
//! **These are not separable from each other.** `moveGroup` calls `updateNativeProjectDropMembership`
//! before it posts the order, so a project dragged into the middle of a collection JOINS that
//! collection in the same gesture, and a port that wrote only the project order would reorder the
//! row and silently drop it out of its folder. The same gesture therefore touches THREE documents:
//! the collections document (membership, and the project order inside each folder), the Spaces
//! document (a drop onto a Space button), and the workspace session groups document (the project
//! order, through the `syncGroupOrder` message this plan posts).
//!
//! **Every write below is the TypeScript's, including the ones that write nothing new.**
//! `saveNativeCollections` writes the key and posts the update for whatever it is handed, an
//! unchanged document included, and `post({ type: 'syncGroupOrder' })` goes out after a drop the
//! worktree rules forbade and left the order exactly as it was. Both are reproduced rather than
//! optimized away, for the reason the session moves reproduced theirs: the parity gate compared
//! what the shipped code did, and a port that wrote less would make one fewer push per drag.
//! The TypeScript was frozen in `tooling/gx-core/sidebar-page-frozen/` (`reorder.ts`,
//! `project-drag.ts`, `membership.ts`), since deleted; see git history.
//!
//! SEE-ALSO: packages/gx-core/src/project_docs/, apps/desktop/src/app/gx_store/project_docs.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::MachineId;
use crate::project_docs::{
    create_collection, move_members_to_space, move_projects_to_collection,
    move_projects_with_worktrees, reorder_collection_projects, reorder_spaces, toggle_space_member,
    CollectionsDocument, DropPosition, SpaceMemberKind, SpacesDocument,
};
use crate::sidebar_view::{SidebarInputs, OTHER_SPACE_ID};

use super::project_inventory::{project_section, selected_machine, ProjectSection};

/// The payload types this file answers.
pub const PROJECT_MOVE_COMMAND_TYPES: &[&str] = &[
    "moveGroup",
    "moveSpace",
    "moveToSpace",
    "moveToCollection",
    "moveCollection",
    "projectMembership",
    "spaceMembership",
];

/// One thing the host does for a project move, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectWrite {
    /// `saveNativeCollections`: the collections document after the edit. It goes through the
    /// pending-push guard, which writes the stored key and books the debounced push.
    EditCollections { document: CollectionsDocument },
    /// `ui.metadata.updateSpaces`: the Spaces document after the edit, through its own guard.
    EditSpaces { document: SpacesDocument },
    /// `sidebarStore.setState({ groupOrder })` plus `post({ type: 'syncGroupOrder', groupIds })`.
    /// The order goes into the store so the rows move under the user's finger, and the message is
    /// what edits the workspace session groups document's `projectOrder`.
    GroupOrder { group_ids: Vec<String> },
    /// `ui.renameRequest`: the Rename dialog opens on the collection that was just created, so the
    /// user names it instead of living with "Group 7".
    RequestCollectionRename { collection_id: String },
    /// `openAppModal({ type: 'open', modal: 'sidebarSpaceEditor', mode: 'create', ... })`: the New
    /// Space item of a membership menu, which creates the Space AND puts the member in it.
    OpenSpaceEditor {
        section_key: String,
        /// `...(ui.selectedMachineId === 'local' ? {} : { remoteMachineId })`: the dialog's result
        /// has to name the machine whose document it edits, and an absent key is not the same as a
        /// `null` one to the dialog that reads it.
        remote_machine_id: Option<String>,
        member_collection_id: Option<String>,
        member_project_id: Option<String>,
    },
}

impl ProjectWrite {
    /// The write as the parity gate compared it while the TypeScript ran. Each document is compared WHOLE, because it is
    /// the thing a stale echo would undo and a subset would hide a member that moved.
    pub fn to_json(&self) -> Value {
        match self {
            Self::EditCollections { document } => json!({
                "write": "editCollections",
                "document": document.to_storage_json(),
                "wire": document.to_wire_json(),
            }),
            Self::EditSpaces { document } => {
                json!({ "write": "editSpaces", "wire": document.to_wire_json() })
            }
            Self::GroupOrder { group_ids } => {
                json!({ "write": "groupOrder", "groupIds": group_ids })
            }
            Self::RequestCollectionRename { collection_id } => {
                json!({ "write": "renameCollection", "collectionId": collection_id })
            }
            Self::OpenSpaceEditor {
                section_key,
                remote_machine_id,
                member_collection_id,
                member_project_id,
            } => json!({
                "write": "openSpaceEditor",
                "sectionKey": section_key,
                "remoteMachineId": remote_machine_id,
                "memberCollectionId": member_collection_id,
                "memberProjectId": member_project_id,
            }),
        }
    }
}

/// Everything one project move does.
///
/// An EMPTY list is a real answer and not a refusal: every guard in `runNativeProjectDrop` and
/// `reorderNativeSidebar` is a bare `return`, so "this drop is not allowed" and "nothing happens"
/// were the same thing on both sides, and the gate compared that rather than skipping it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectMovePlan {
    pub writes: Vec<ProjectWrite>,
    /// Why the plan is empty, for the record line. Never compared: the TypeScript has no such word.
    pub refusal: Option<&'static str>,
    /// Whether a project order write was taken out because the gesture was made on a remote
    /// machine's tab. For the host's counter only; see `without_group_order`.
    pub dropped_group_order: bool,
}

impl ProjectMovePlan {
    fn refused(reason: &'static str) -> Self {
        Self {
            writes: Vec::new(),
            refusal: Some(reason),
            dropped_group_order: false,
        }
    }

    fn of(writes: Vec<ProjectWrite>) -> Self {
        Self {
            writes,
            refusal: None,
            dropped_group_order: false,
        }
    }

    /// The plan with its project order write taken out, which is what a gesture on a REMOTE
    /// machine's tab does.
    ///
    /// CDXC:RemoteMachines 2026-09-21 WHY:
    /// **Reproduced, not fixed.** `reorderNativeSidebar` and `runNativeProjectDrop` splice
    /// `state.groupOrder`, which holds EVERY machine's groups, and post that whole list;
    /// `syncWorkspaceGroupOrder` then returns without writing as soon as the ids name more than one
    /// machine, so dragging a project on a remote tab has never saved an order. This store builds
    /// the order from ONE machine's section, so posting it would send a single-machine list that
    /// the runtime WOULD write, and a port is not the place to start saving something the app never
    /// saved. The membership half of the same gesture still lands, exactly as it does today.
    /// Declared difference: a workspace whose only groups are that machine's posts a single-machine
    /// list today too, and there the order did save; this drops it there as well.
    fn without_group_order(mut self) -> Self {
        let before = self.writes.len();
        self.writes
            .retain(|write| !matches!(write, ProjectWrite::GroupOrder { .. }));
        self.dropped_group_order = self.writes.len() != before;
        self
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.writes.iter().map(ProjectWrite::to_json).collect())
    }
}

/// Whether this payload is one this file answers, without building anything.
///
/// CDXC:Projects 2026-09-21 WHY:
/// **`projectMembership` with `action: 'hide'` is NOT one of them**, even though every other
/// `projectMembership` is. It writes no document at all: it toggles `ui.hiddenItems.groupIds`, which
/// is K3, the sidebar's own state, and the store already answers it as a sidebar-UI intent
/// (`apps/desktop/src/app/gx_store/sidebar_ui_commands.rs`). Claiming it here took the command out
/// of the dispatch before that arm ran and before the command was forwarded, so the toggle was
/// applied once and correctly while the sidebar PAGE's copy of the hidden items stopped moving, and
/// which of the two arms ran depended on whether a stored key had been read yet. One owner, and it
/// is the one that also tells the page.
pub fn owns_project_move_command(command: &Value) -> bool {
    let kind = command.get("type").and_then(Value::as_str);
    if kind == Some("projectMembership") && command.get("action") == Some(&Value::from("hide")) {
        return false;
    }
    kind.is_some_and(|kind| PROJECT_MOVE_COMMAND_TYPES.contains(&kind))
}

/// What a project move does, or `None` when the store must not answer it.
///
/// **Refused, with the reason at each refusal.** The one that is left is a shape this store cannot
/// see the whole of, never a rule it disagrees with: **the payload is malformed.** A missing id or
/// an unknown `position` would make a port guess an order the app never posts.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// A REMOTE machine's tab is no longer refused. Every arm below already read the section it was
/// given rather than this computer's, so the generalisation is the caller's: it resolves
/// `ui.selectedMachineId` into a machine, and the HOST hands in THAT machine's two documents (its
/// side state) and sends the edits back down its tunnel as
/// `updateSidebarProjectCollections` / `updateSidebarSpaces` carrying `remoteMachineId`, which is
/// what the sidebar page did and all it did. The order write is the one arm that differs; see
/// `without_group_order`.
///
/// CDXC:Projects 2026-09-21 WHY:
/// A remote machine being CONNECTED is not a refusal, although it was one until the user enabled a
/// remote machine and it started refusing every project drag on this computer's tab. The
/// TypeScript splices into `state.groupOrder`, which holds every machine's groups, and posts that
/// whole list; its own `syncWorkspaceGroupOrder` then returns without writing when the ids name more
/// than one machine, so with any remote machine drawn a local reorder moved the row for one frame
/// and saved nothing (the Project Group membership half of the same gesture still landed). The
/// order here is built from THIS computer's section only (`project_section`), so it is the list the
/// TypeScript posts when no other machine is drawn, which is the order the user dragged into.
/// Declared difference 36.
pub fn plan_project_move(
    core: &Core,
    inputs: &SidebarInputs,
    collections: &CollectionsDocument,
    spaces: Option<&SpacesDocument>,
    command: &Value,
    now_ms: i64,
) -> Option<ProjectMovePlan> {
    if !owns_project_move_command(command) {
        return None;
    }
    let machine = selected_machine(&inputs.ui.selected_machine_id);
    let section = project_section(core, inputs, &machine)?;
    // `nativeSidebarSettings().sidebarSpacesEnabled ? ui.metadata.spaces[machineId] : undefined`.
    // The Spaces REORDER deliberately does not go through this: `reorderNativeSidebar` reads
    // `ui.metadata.spaces[...]` directly, so a Space row can be dragged with the setting off.
    let section_spaces = match inputs.settings.sidebar_spaces_enabled {
        true => spaces,
        false => None,
    };
    let plan = match command.get("type").and_then(Value::as_str)? {
        "moveGroup" => plan_move_group(&section, collections, command),
        "moveSpace" => plan_move_space(spaces, command),
        "moveToSpace" => plan_move_to_space(&section, collections, section_spaces, command),
        "moveToCollection" => plan_move_to_collection(&section, collections, command),
        "moveCollection" => plan_move_collection(&section, collections, command),
        "projectMembership" => plan_project_membership(&section, collections, command, now_ms),
        "spaceMembership" => plan_space_membership(&section, inputs, section_spaces, command),
        _ => None,
    }?;
    Some(match machine {
        MachineId::Local => plan,
        MachineId::Remote(_) => plan.without_group_order(),
    })
}

/// `reorderNativeSidebar`'s `moveGroup` arm: the order, then the membership, then the post.
fn plan_move_group(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    command: &Value,
) -> Option<ProjectMovePlan> {
    let group_id = command.get("groupId")?.as_str()?;
    let target_group_id = command.get("targetGroupId")?.as_str()?;
    let position = DropPosition::parse(command.get("position")?.as_str()?)?;
    // `if (!source || !target || source.remoteMachineContext?.machineId !== target...) return`. The
    // machine test is already answered by the section: both rows are this computer's or they are
    // not in it.
    let (Some(_), Some(_)) = (section.row(group_id), section.row(target_group_id)) else {
        return Some(ProjectMovePlan::refused("groupMissing"));
    };
    let next: Vec<String> =
        move_projects_with_worktrees(&section.order_items(), group_id, target_group_id, position)
            .into_iter()
            .map(|item| item.order_id)
            .collect();
    let mut writes =
        update_project_drop_membership(section, collections, group_id, target_group_id, &next);
    writes.push(ProjectWrite::GroupOrder { group_ids: next });
    Some(ProjectMovePlan::of(writes))
}

/// `updateNativeProjectDropMembership`: the dragged family joins whatever collection the row it
/// landed on belongs to, and every folder's projects are re-sorted into the new order.
///
/// The target's collection is read from the membership map, so a drop onto a WORKTREE of a project
/// in a folder joins that folder too, through the inheritance the map applies.
fn update_project_drop_membership(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    group_id: &str,
    target_group_id: &str,
    order: &[String],
) -> Vec<ProjectWrite> {
    // `membership.get(resolveProjectId(targetGroupId) ?? '')`: a target with no project (the Chats
    // collection) asks for the empty string, which no collection holds, so the family is moved OUT
    // of every collection rather than left where it was.
    let collection_id = section.collection_of_project(
        collections,
        &section
            .project_of_group(target_group_id)
            .unwrap_or_default(),
    );
    let family = section.project_ids_of(&section.project_family(group_id));
    let moved = move_projects_to_collection(collections, &family, collection_id.as_deref());
    let document = reorder_collection_projects(&moved, &section.project_ids_of(order));
    vec![ProjectWrite::EditCollections { document }]
}

/// `runNativeProjectDrop`'s `moveToSpace` arm.
fn plan_move_to_space(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    spaces: Option<&SpacesDocument>,
    command: &Value,
) -> Option<ProjectMovePlan> {
    let source_kind = command.get("sourceKind")?.as_str()?;
    let source_id = command.get("sourceId")?.as_str()?;
    let space_id = command.get("spaceId")?.as_str()?;
    let from_collection = source_kind == "collection";
    let moved_group_ids = match from_collection {
        true => section.collection_groups(collections, source_id),
        false => section.project_family(source_id),
    };
    if moved_group_ids.is_empty() {
        return Some(ProjectMovePlan::refused("noGroupsMoved"));
    }
    // `if (!spaces || (spaceId !== 'other' && !spaces.spaces[spaceId])) return`. Note the order:
    // the empty-family return above comes FIRST, so a drop of nothing onto an unknown Space is the
    // same no-op either way.
    let Some(spaces) = spaces else {
        return Some(ProjectMovePlan::refused("noSpacesDocument"));
    };
    if space_id != OTHER_SPACE_ID && !spaces.state.spaces.contains_key(space_id) {
        return Some(ProjectMovePlan::refused("unknownSpace"));
    }
    let (kind, member_ids) = match from_collection {
        true => (SpaceMemberKind::Collection, vec![source_id.to_string()]),
        false => (
            SpaceMemberKind::Project,
            section.project_ids_of(&moved_group_ids),
        ),
    };
    let mut writes = Vec::new();
    // A PROJECT dropped onto a Space leaves its collection first, because a project in a folder
    // takes its Space from the folder and a direct membership beside it would be ignored.
    if !from_collection {
        writes.push(ProjectWrite::EditCollections {
            document: move_projects_to_collection(collections, &member_ids, None),
        });
    }
    writes.push(ProjectWrite::EditSpaces {
        document: move_members_to_space(spaces, space_id, kind, &member_ids),
    });
    // The moved groups go to the FRONT of the order, which is what puts a project the user just
    // filed at the top of the Space they filed it into.
    let remaining: Vec<String> = section
        .group_ids()
        .into_iter()
        .filter(|id| !moved_group_ids.contains(id))
        .collect();
    let group_ids: Vec<String> = moved_group_ids.into_iter().chain(remaining).collect();
    writes.push(ProjectWrite::GroupOrder { group_ids });
    Some(ProjectMovePlan::of(writes))
}

/// `runNativeProjectDrop`'s `moveToCollection` arm: the family joins the named collection, or
/// leaves every collection when none is named. No order is posted at all.
fn plan_move_to_collection(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    command: &Value,
) -> Option<ProjectMovePlan> {
    let source_kind = command.get("sourceKind")?.as_str()?;
    let source_id = command.get("sourceId")?.as_str()?;
    let collection_id = command.get("collectionId").and_then(Value::as_str);
    let moved_group_ids = match source_kind == "collection" {
        true => section.collection_groups(collections, source_id),
        false => section.project_family(source_id),
    };
    if moved_group_ids.is_empty() {
        return Some(ProjectMovePlan::refused("noGroupsMoved"));
    }
    let family = section.project_ids_of(&moved_group_ids);
    Some(ProjectMovePlan::of(vec![ProjectWrite::EditCollections {
        document: move_projects_to_collection(collections, &family, collection_id),
    }]))
}

/// `runNativeProjectDrop`'s tail: a whole collection dragged to another position.
fn plan_move_collection(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    command: &Value,
) -> Option<ProjectMovePlan> {
    let source_id = command.get("sourceId")?.as_str()?;
    let target_kind = command.get("targetKind")?.as_str()?;
    let target_id = command.get("targetId")?.as_str()?;
    let position = DropPosition::parse(command.get("position")?.as_str()?)?;
    let moved_group_ids = section.collection_groups(collections, source_id);
    if moved_group_ids.is_empty() {
        return Some(ProjectMovePlan::refused("noGroupsMoved"));
    }
    let remaining: Vec<String> = section
        .group_ids()
        .into_iter()
        .filter(|id| !moved_group_ids.contains(id))
        .collect();
    let targets = match target_kind == "collection" {
        true => section.collection_groups(collections, target_id),
        false => section.project_family(target_id),
    };
    // `targets.map(indexOf).filter(>= 0)`: a target group that moved out with the source is not an
    // index at all, and a collection dropped onto ITSELF therefore does nothing.
    let indices: Vec<usize> = targets
        .iter()
        .filter_map(|id| remaining.iter().position(|candidate| candidate == id))
        .collect();
    if indices.is_empty() {
        return Some(ProjectMovePlan::refused("noTargetIndex"));
    }
    let at = match position {
        DropPosition::Before => indices.iter().copied().min().unwrap_or_default(),
        DropPosition::After => indices.iter().copied().max().unwrap_or_default() + 1,
    };
    let mut group_ids = remaining;
    let at = at.min(group_ids.len());
    for (offset, group_id) in moved_group_ids.into_iter().enumerate() {
        group_ids.insert(at + offset, group_id);
    }
    // NOTE: no worktree re-nest here, and no collections edit either. `moveCollection` splices the
    // raw group order and posts it, so a collection dropped between a project and its worktree
    // really does land there until the next projection rebuilds the order.
    Some(ProjectMovePlan::of(vec![ProjectWrite::GroupOrder {
        group_ids,
    }]))
}

/// `reorderNativeSidebar`'s `moveSpace` arm: the Space buttons reordered.
///
/// Reads the Spaces document WITHOUT the `sidebarSpacesEnabled` gate, which is what the shipped
/// code does, so this arm answers where `moveToSpace` would refuse.
fn plan_move_space(spaces: Option<&SpacesDocument>, command: &Value) -> Option<ProjectMovePlan> {
    let space_id = command.get("spaceId")?.as_str()?;
    let target_space_id = command.get("targetSpaceId")?.as_str()?;
    let position = DropPosition::parse(command.get("position")?.as_str()?)?;
    let visible: Vec<String> = command
        .get("visibleSpaceIds")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    // `if (!spaces || ...) return`: the metadata entry has never been set, which is not the same as
    // an empty document and is why the guard is asked whether it holds one.
    let Some(spaces) = spaces else {
        return Some(ProjectMovePlan::refused("noSpacesDocument"));
    };
    if space_id == target_space_id {
        return Some(ProjectMovePlan::refused("sameSpace"));
    }
    let visible: Vec<String> = visible
        .into_iter()
        .filter(|id| spaces.state.order.contains(id))
        .collect();
    if !visible.contains(&space_id.to_string()) {
        return Some(ProjectMovePlan::refused("spaceNotVisible"));
    }
    let mut order: Vec<String> = visible
        .iter()
        .filter(|id| *id != space_id)
        .cloned()
        .collect();
    let Some(index) = order.iter().position(|id| id == target_space_id) else {
        return Some(ProjectMovePlan::refused("targetSpaceNotVisible"));
    };
    order.insert(
        index + usize::from(position == DropPosition::After),
        space_id.to_string(),
    );
    let reordered =
        crate::project_docs::apply_space_row_reorder(&spaces.state.order, &visible, &order);
    Some(ProjectMovePlan::of(vec![ProjectWrite::EditSpaces {
        document: reorder_spaces(spaces, &reordered),
    }]))
}

/// `runNativeMembershipAction`'s `projectMembership` arm: the Add to Group menu.
///
/// Hide is not here: it writes no document, and `owns_project_move_command` says why.
fn plan_project_membership(
    section: &ProjectSection,
    collections: &CollectionsDocument,
    command: &Value,
    now_ms: i64,
) -> Option<ProjectMovePlan> {
    let group_id = command.get("groupId")?.as_str()?;
    let action = command.get("action")?.as_str()?;
    // `getProjectCollectionFamilyProjectIds` on this computer and
    // `getRemoteProjectCollectionFamilyProjectIds` on a remote machine are the same walk over two
    // id vocabularies: the local one keys everything by `projectContext.editor.projectId`, the
    // remote one looks that group up and then works in `remoteMachineContext.projectId`. The row's
    // resolved id IS the id its machine's document is keyed by, so one walk answers both, and the
    // remote one's `if (!rawProjectId) return []` is this `None`.
    let Some(scoped_project_id) = section.project_of_group(group_id) else {
        return Some(ProjectMovePlan::refused("groupHasNoProject"));
    };
    // `getProjectCollectionFamilyProjectIds`, which is `nativeProjectFamily` resolved to project
    // ids with the duplicates removed, and which falls back to the project ITSELF when the family
    // comes out empty.
    let family = collection_family_project_ids(section, &scoped_project_id);
    if family.is_empty() {
        return Some(ProjectMovePlan::refused("emptyFamily"));
    }
    if action == "createCollection" {
        let (collection_id, created) = create_collection(collections, &family[0], now_ms);
        let document = move_projects_to_collection(&created, &family, Some(&collection_id));
        return Some(ProjectMovePlan::of(vec![
            ProjectWrite::EditCollections { document },
            ProjectWrite::RequestCollectionRename { collection_id },
        ]));
    }
    // `moveCollection` with no `collectionId` is Remove from Group.
    let collection_id = command.get("collectionId").and_then(Value::as_str);
    Some(ProjectMovePlan::of(vec![ProjectWrite::EditCollections {
        document: move_projects_to_collection(collections, &family, collection_id),
    }]))
}

/// `getProjectCollectionFamilyProjectIds`: the project ids of one family, de-duplicated, falling
/// back to the requested project when nothing matched.
///
/// It walks `groupIds` and asks each group's OWN `projectContext`, so a project with user-made
/// groups contributes its id several times and the de-duplication is what makes that invisible.
fn collection_family_project_ids(section: &ProjectSection, project_id: &str) -> Vec<String> {
    let family_parent = section
        .rows
        .iter()
        .find(|row| row.project_id.as_deref() == Some(project_id))
        .and_then(|row| row.parent_project_id.clone())
        .unwrap_or_else(|| project_id.to_string());
    let mut project_ids: Vec<String> = Vec::new();
    for row in &section.rows {
        let candidate = row.project_id.clone();
        if candidate.as_deref() == Some(family_parent.as_str())
            || row.parent_project_id.as_deref() == Some(family_parent.as_str())
        {
            if let Some(candidate) = candidate {
                if !project_ids.contains(&candidate) {
                    project_ids.push(candidate);
                }
            }
        }
    }
    match project_ids.is_empty() {
        true => vec![project_id.to_string()],
        false => project_ids,
    }
}

/// `runNativeMembershipAction`'s `spaceMembership` arm: the Spaces submenu's ticks, and New Space.
fn plan_space_membership(
    section: &ProjectSection,
    inputs: &SidebarInputs,
    spaces: Option<&SpacesDocument>,
    command: &Value,
) -> Option<ProjectMovePlan> {
    let space_id = command.get("spaceId").and_then(Value::as_str);
    let collection_id = command.get("collectionId").and_then(Value::as_str);
    let project_id = command.get("projectId").and_then(Value::as_str);
    let Some(space_id) = space_id else {
        // New Space: the dialog creates it and puts the member in it, which is
        // `applySidebarSpaceEditorResult` and stays the modal host's.
        return Some(ProjectMovePlan::of(vec![ProjectWrite::OpenSpaceEditor {
            section_key: section.section_key.clone(),
            remote_machine_id: match &section.machine {
                MachineId::Local => None,
                MachineId::Remote(machine_id) => Some(machine_id.clone()),
            },
            member_collection_id: collection_id.map(str::to_string),
            member_project_id: project_id.map(str::to_string),
        }]));
    };
    // `else if (section.spacesState)`: with no Spaces document the tick does nothing at all, and
    // the dialog is NOT opened.
    let Some(spaces) = spaces else {
        return Some(ProjectMovePlan::refused("noSpacesDocument"));
    };
    let _ = inputs;
    let (kind, member_id) = match collection_id {
        Some(collection_id) => (SpaceMemberKind::Collection, collection_id),
        // `toggleSpaceProjectMembership(state, spaceId, command.projectId!)`: a payload carrying
        // neither id reaches `undefined` there, which `withToggledMember` would read as an empty
        // member and return the state unchanged. Refused here instead of trimming a value that is
        // not there, because a port that guessed would toggle the wrong thing.
        None => match project_id {
            Some(project_id) => (SpaceMemberKind::Project, project_id),
            None => return Some(ProjectMovePlan::refused("noMember")),
        },
    };
    Some(ProjectMovePlan::of(vec![ProjectWrite::EditSpaces {
        document: toggle_space_member(spaces, space_id, kind, member_id),
    }]))
}
