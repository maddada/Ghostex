//! The top level of the list: which groups are drawn, the Space buttons, the collections, the
//! machine tab counts, and the empty state.
//!
//! SEE-ALSO: the deleted sidebar page's `model.ts` (`createNativeSidebarSnapshot`),
//! collections.ts, empty-state.ts, and space-navigation.ts.

use std::collections::{BTreeMap, BTreeSet};

use super::collections::{CollectionItem, CollectionsState, project_sidebar_collections};
use super::groups::{GroupBuild, GroupKind, GroupPlan, group_summary};
use super::inputs::{LOCAL_MACHINE_ID, SidebarHostInputs, SidebarSettings, SidebarUiState};
use super::projects::ProjectMeta;
use super::spaces::{
    OTHER_SPACE_ICON, OTHER_SPACE_ID, OTHER_SPACE_LABEL, SpaceSelection, SpacesState,
    resolve_selected_space, selection_shows_project, space_for_group,
};
use super::view::{
    CollectionView, EmptyState, GroupView, MachineSummary, MachineTabView, OrderItem, OrderKind,
    SidebarView, SpaceView,
};
use crate::keys::MachineId;

/// Everything the top level reads.
pub(crate) struct AssembleInput<'a> {
    pub(crate) plans: &'a [GroupPlan],
    pub(crate) builds: &'a BTreeMap<String, GroupBuild>,
    pub(crate) meta: &'a ProjectMeta,
    pub(crate) ui: &'a SidebarUiState,
    pub(crate) settings: &'a SidebarSettings,
    pub(crate) host: &'a SidebarHostInputs,
    pub(crate) spaces: Option<SpacesState>,
    pub(crate) collections: CollectionsState,
    /// THIS COMPUTER's first snapshot has arrived, which is what `ready` and the loading and error
    /// branches of the empty state are about on every tab.
    pub(crate) local_machine_loaded: bool,
    /// The badge counts of every machine tab.
    pub(crate) machine_summaries: &'a BTreeMap<MachineId, MachineSummary>,
    /// Whether ANY machine draws a project group. Asked only when the two cheaper inventory tests
    /// both say no, so it is a closure rather than a value.
    pub(crate) any_machine_draws_a_project: &'a dyn Fn() -> bool,
    pub(crate) now_ms: u64,
}

/// Builds the whole list from the groups that were already built.
pub(crate) fn assemble(input: AssembleInput<'_>) -> SidebarView {
    let section_key = input.ui.section_key();
    let group_ids: Vec<String> = input
        .plans
        .iter()
        .map(|plan| plan.group_id.clone())
        .collect();
    // Both are asked once per group per Space, so the lookup is a map rather than a scan.
    let mut projects_by_group: BTreeMap<&str, (&str, Option<&str>)> = BTreeMap::new();
    for plan in input.plans {
        if let Some(project) = &plan.project {
            projects_by_group.insert(
                plan.group_id.as_str(),
                (
                    project.project_id.as_str(),
                    project
                        .worktree
                        .as_ref()
                        .map(|worktree| worktree.parent_project_id.as_str()),
                ),
            );
        }
    }
    let project_of_group = |group_id: &str| -> Option<String> {
        projects_by_group
            .get(group_id)
            .map(|(project_id, _)| (*project_id).to_string())
    };
    let parent_project_of_group = |group_id: &str| -> Option<String> {
        projects_by_group
            .get(group_id)
            .and_then(|(_, parent)| *parent)
            .map(str::to_string)
    };
    let collection_id_by_project = input.collections.collection_id_by_project(
        &group_ids,
        &project_of_group,
        &parent_project_of_group,
    );

    // A section has a Space row only when the setting is on and its daemon published a Space
    // document at all; an older daemon has no Spaces and is never filtered.
    let spaces_document = input
        .spaces
        .clone()
        .filter(|_| input.settings.sidebar_spaces_enabled);
    let spaces_state = spaces_document.clone().unwrap_or_default();
    let selection = spaces_document.as_ref().map(|state| {
        resolve_selected_space(
            state,
            input
                .ui
                .collapse
                .selected_space_by_section
                .get(&section_key)
                .map(String::as_str),
        )
    });
    let shows_group = |selection: &SpaceSelection, group_id: &str| -> bool {
        let project = projects_by_group.get(group_id).copied();
        selection_shows_project(
            selection,
            &spaces_state,
            project.map(|(project_id, _)| project_id),
            project
                .and_then(|(project_id, _)| collection_id_by_project.get(project_id))
                .map(String::as_str),
            project.and_then(|(_, parent)| parent),
        )
    };

    // The group that owns the focused row, in section order.
    let active_group_id = group_ids.iter().find(|group_id| {
        input
            .builds
            .get(*group_id)
            .is_some_and(GroupBuild::contains_focused_session)
    });
    let active_space_id = match (&selection, active_group_id) {
        (Some(selection), Some(group_id)) => {
            let project_id = project_of_group(group_id);
            Some(space_for_group(
                &spaces_state,
                selection,
                project_id.as_deref(),
                project_id
                    .as_deref()
                    .and_then(|project_id| collection_id_by_project.get(project_id))
                    .map(String::as_str),
                parent_project_of_group(group_id).as_deref(),
            ))
        }
        _ => None,
    };

    let mut spaces: Vec<SpaceView> = Vec::new();
    if let Some(selection) = &selection {
        let mut rows: Vec<SpaceView> = spaces_state
            .ordered()
            .into_iter()
            .map(|space| SpaceView {
                id: space.space_id.clone(),
                name: space.name.clone(),
                icon: space.icon.clone(),
                color: space.color.clone(),
                ..SpaceView::default()
            })
            .collect();
        rows.push(SpaceView {
            id: OTHER_SPACE_ID.to_string(),
            name: OTHER_SPACE_LABEL.to_string(),
            icon: OTHER_SPACE_ICON.to_string(),
            color: String::new(),
            ..SpaceView::default()
        });
        for row in &mut rows {
            let view = if row.id == OTHER_SPACE_ID {
                SpaceSelection::Other
            } else {
                SpaceSelection::Space(row.id.clone())
            };
            let mut session_ids: BTreeSet<&str> = BTreeSet::new();
            let mut working_count = 0;
            let mut attention_count = 0;
            let mut background_work_count = 0;
            for group_id in group_ids
                .iter()
                .filter(|group_id| shows_group(&view, group_id))
            {
                let Some(build) = input.builds.get(group_id) else {
                    continue;
                };
                for session in &build.store_rows {
                    if !session_ids.insert(session.row.sidebar_session_id.as_str()) {
                        continue;
                    }
                    if session.row.activity == "working" {
                        working_count += 1;
                    }
                    if session.row.activity == "attention" || session.row.pending_question_count > 0
                    {
                        attention_count += 1;
                    }
                    if session.row.shows_background_work() {
                        background_work_count += 1;
                    }
                }
            }
            row.selected = selection.space_id() == row.id;
            row.contains_active_session = active_space_id.as_deref() == Some(row.id.as_str());
            row.working_count = working_count;
            row.attention_count = attention_count;
            row.background_work_count = background_work_count;
        }
        spaces = rows;
    }

    let mut groups: Vec<GroupView> = Vec::new();
    for plan in input.plans {
        if plan.kind == GroupKind::Chats {
            continue;
        }
        let Some(build) = input.builds.get(&plan.group_id) else {
            continue;
        };
        if let Some(selection) = &selection {
            if !shows_group(selection, &plan.group_id) {
                continue;
            }
        }
        if !input.ui.show_hidden && input.ui.hidden_items.group_ids.contains(&plan.group_id) {
            continue;
        }
        if build.tag_filtered_out {
            continue;
        }
        groups.push(GroupView {
            core: build.core.clone(),
            collection_color: None,
            collection_id: project_of_group(&plan.group_id)
                .and_then(|project_id| collection_id_by_project.get(&project_id).cloned()),
        });
    }

    let rendered_ids: Vec<String> = groups
        .iter()
        .map(|group| group.core.group_id.clone())
        .collect();
    let mut collections: Vec<CollectionView> = Vec::new();
    let mut order: Vec<OrderItem> = Vec::new();
    for item in project_sidebar_collections(
        &rendered_ids,
        &input.collections,
        &project_of_group,
        &parent_project_of_group,
    ) {
        match item {
            CollectionItem::Project { group_id } => order.push(OrderItem {
                kind: OrderKind::Project,
                id: group_id,
            }),
            CollectionItem::Collection {
                collection,
                group_ids,
            } => {
                let storage_id = format!("{section_key}:{}", collection.collection_id);
                if !input.ui.show_hidden
                    && input.ui.hidden_items.collection_keys.contains(&storage_id)
                {
                    continue;
                }
                for group in &mut groups {
                    if group_ids.contains(&group.core.group_id) {
                        group.collection_color = Some(collection.color.clone());
                    }
                }
                let sessions: Vec<_> = group_ids
                    .iter()
                    .filter_map(|group_id| {
                        groups
                            .iter()
                            .find(|group| group.core.group_id == *group_id)
                            .map(|group| group.core.sessions.clone())
                    })
                    .flatten()
                    .collect();
                let summary = group_summary(&sessions);
                collections.push(CollectionView {
                    collection_id: collection.collection_id.clone(),
                    storage_id,
                    title: collection.title.clone(),
                    color: collection.color.clone(),
                    group_ids,
                    collapsed: input
                        .ui
                        .collapse
                        .collapsed_collections
                        .contains(&format!("{section_key}:{}", collection.collection_id)),
                    contains_active_session: sessions.iter().any(|session| session.is_focused),
                    working_count: summary.working_count,
                    attention_count: summary.attention_count,
                    background_work_count: summary.background_work_count,
                    awake_count: summary.awake_count,
                });
                order.push(OrderItem {
                    kind: OrderKind::Collection,
                    id: collection.collection_id,
                });
            }
        }
    }

    let mut machine_summary = MachineSummary::default();
    for build in group_ids
        .iter()
        .filter_map(|group_id| input.builds.get(group_id))
    {
        for session in &build.store_rows {
            if session.row.activity == "working" {
                machine_summary.working_count += 1;
            }
            if session.row.activity == "attention" || session.row.pending_question_count > 0 {
                machine_summary.attention_count += 1;
            }
            if session.row.shows_background_work() {
                machine_summary.background_work_count += 1;
            }
        }
    }
    // The selected machine's tab draws the counts of the list just built; every other tab draws the
    // ones counted off the store, which is the same number by another route.
    let machines: Vec<MachineTabView> = input
        .host
        .machines
        .iter()
        .map(|machine| {
            let counts = if machine.machine_id == input.ui.selected_machine_id {
                machine_summary
            } else {
                input
                    .machine_summaries
                    .get(&machine_key(&machine.machine_id))
                    .copied()
                    .unwrap_or_default()
            };
            MachineTabView {
                id: machine.machine_id.clone(),
                label: machine.label.clone(),
                state: machine.state.clone(),
                message: machine.message.clone(),
                working_count: counts.working_count,
                attention_count: counts.attention_count,
                background_work_count: counts.background_work_count,
            }
        })
        .collect();

    SidebarView {
        // `state.hasReceivedSnapshot` is this computer's, whichever machine tab is selected, and
        // the host's unavailable clock stands in for it once it has been seen down.
        ready: input.local_machine_loaded || input.host.unavailable.since_ms.is_some(),
        // This computer always, and a remote machine while the host feeds its presentation into
        // the store. A tab this says `false` for keeps whatever the caller drew.
        supported: input.host.feeds(&input.ui.selected_machine_id),
        selected_machine_id: input.ui.selected_machine_id.clone(),
        machines,
        scroll_scope: format!(
            "{}|{}",
            input.ui.selected_machine_id,
            selection
                .as_ref()
                .map_or("all", |selection| selection.space_id())
        ),
        machine: machine_summary,
        spaces_enabled: selection.is_some(),
        spaces,
        groups,
        collections,
        order,
        empty_state: empty_state(&input, selection.as_ref()),
    }
}

/// `createNativeEmptyState`.
fn empty_state(input: &AssembleInput<'_>, selection: Option<&SpaceSelection>) -> EmptyState {
    let is_local = input.ui.selected_machine_id == LOCAL_MACHINE_ID;
    // Adding a project needs somewhere to add it: this computer always, a remote machine while its
    // connection is up. That is a fact of the remote transport, which is why it comes from the host
    // rather than from the store.
    let can_add_project = is_local
        || input
            .host
            .machine(&input.ui.selected_machine_id)
            .is_some_and(super::inputs::MachineTabInput::is_connected);
    if !is_local && !can_add_project {
        // `createNativeEmptyState` returns early here: a machine tab that is not connected says
        // nothing at all rather than "No projects".
        return EmptyState::default();
    }
    // `unavailable` is the local placeholder group, so the loading and error branches are this
    // computer's on every tab.
    let unavailable = !input.local_machine_loaded;
    let error = unavailable
        && (input.host.unavailable.observed_available
            || input
                .host
                .unavailable
                .since_ms
                .is_some_and(|since| input.now_ms.saturating_sub(since) >= 20_000));
    let loading = !error && unavailable;
    // `hasKnownSidebarProjectInventory` reads `workspaceGroupIds`, which spans every machine, so a
    // user whose only projects are on a remote machine is not shown first-run copy.
    let known = input.meta.project_settings_count > 0
        || input.host.recent_project_count > 0
        || input.plans.iter().any(|plan| plan.kind != GroupKind::Chats)
        || (input.any_machine_draws_a_project)();
    let copy = if error {
        "Unable to load sessions."
    } else if !known {
        "No projects added yet."
    } else if selection.is_some_and(|selection| !selection.is_other()) {
        "No projects in this Space."
    } else {
        "No projects"
    };
    EmptyState {
        loading,
        error,
        can_add_project,
        copy: copy.to_string(),
    }
}

/// The store's key for a machine tab id.
fn machine_key(machine_id: &str) -> MachineId {
    if machine_id == LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(machine_id.to_string())
    }
}
