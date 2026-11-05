//! The top level of the list: which groups are drawn, the Space buttons, the collections, the
//! machine tab counts, and the empty state.
//!
//! SEE-ALSO: the deleted sidebar page's `model.ts` (`createNativeSidebarSnapshot`),
//! collections.ts, empty-state.ts, and space-navigation.ts.

use std::collections::{BTreeMap, BTreeSet};

use super::collections::{project_sidebar_collections, CollectionItem, CollectionsState};
use super::daemon_wait::{DaemonWait, TRY_NOW_LABEL};
use super::groups::{group_summary, GroupBuild, GroupPlan};
use super::inputs::{
    effective_sidebar_mode, SidebarHostInputs, SidebarMode, SidebarSettings, SidebarUiState,
    UnavailableState, LOCAL_MACHINE_ID,
};
use super::projects::ProjectMeta;
use super::spaces::{
    resolve_selected_space, selection_shows_project, space_for_group, SpaceSelection, SpacesState,
    OTHER_SPACE_ICON, OTHER_SPACE_ID, OTHER_SPACE_LABEL,
};
use super::view::{
    CollectionView, EmptyState, GroupSummary, GroupView, MachineSummary, MachineTabView, NeedsYou,
    OrderItem, OrderKind, ShowAllProjectsRow, SidebarView, SpaceProject, SpaceView,
};
use crate::keys::{parse_workspace_subgroup_id, MachineId};

/// Everything the top level reads.
pub(crate) struct AssembleInput<'a> {
    pub(crate) plans: &'a [GroupPlan],
    pub(crate) builds: &'a BTreeMap<String, GroupBuild>,
    pub(crate) meta: &'a ProjectMeta,
    pub(crate) ui: &'a SidebarUiState,
    pub(crate) settings: &'a SidebarSettings,
    pub(crate) host: &'a SidebarHostInputs,
    pub(crate) spaces: Option<SpacesState>,
    /// The section's machine shows Spaces (`machine_spaces::spaces_enabled_on`).
    pub(crate) spaces_enabled: bool,
    pub(crate) collections: CollectionsState,
    /// THIS COMPUTER's first snapshot has arrived, which is what `ready` and the loading and error
    /// branches of the empty state are about on every tab.
    pub(crate) local_machine_loaded: bool,
    /// The ticked tag filters' names, for the empty list's hint.
    pub(crate) tag_filter_labels: Vec<String>,
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
    // The bot groups, which only Bots mode draws and no Space shows or counts.
    let mut bot_groups: BTreeSet<&str> = BTreeSet::new();
    let mut bot_runs_today = 0;
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
            if project.bot_profile.is_some() {
                bot_groups.insert(plan.group_id.as_str());
                bot_runs_today += project.bot_runs_today;
            }
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
    // CDXC:Spaces 2026-09-28 WHY:
    // A user-made session group has no project of its own, and the ported rule showed such a group
    // only in Other, so New Group and Move to New Group looked like they did nothing in any Space.
    // It is filed under the project it was made in: it shows, counts and reveals in that project's
    // Space.
    let space_project_of_group = |group_id: &str| -> Option<SpaceProject> {
        let (project_id, parent_project_id) =
            projects_by_group.get(group_id).copied().or_else(|| {
                let (project, _) = parse_workspace_subgroup_id(group_id)?;
                projects_by_group
                    .get(project.to_sidebar_group_id().as_str())
                    .copied()
            })?;
        Some(SpaceProject {
            project_id: project_id.to_string(),
            parent_project_id: parent_project_id.map(str::to_string),
            collection_id: collection_id_by_project.get(project_id).cloned(),
        })
    };
    let bots_mode = effective_sidebar_mode(input.settings, input.ui) == SidebarMode::Bots;
    let is_bot_group = |group_id: &str| bot_groups.contains(group_id);

    // A section has a Space row only when its machine has Spaces on and its daemon published a
    // Space document at all; an older daemon has no Spaces and is never filtered.
    let spaces_document = input.spaces.clone().filter(|_| input.spaces_enabled);
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
    // Bots belong to no Space: a Space only ever shows and counts projects.
    let shows_group = |selection: &SpaceSelection, group_id: &str| -> bool {
        if is_bot_group(group_id) {
            return false;
        }
        let project = space_project_of_group(group_id);
        selection_shows_project(
            selection,
            &spaces_state,
            project.as_ref().map(|project| project.project_id.as_str()),
            project
                .as_ref()
                .and_then(|project| project.collection_id.as_deref()),
            project
                .as_ref()
                .and_then(|project| project.parent_project_id.as_deref()),
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
        (Some(selection), Some(group_id)) if !is_bot_group(group_id) => {
            let project = space_project_of_group(group_id);
            Some(space_for_group(
                &spaces_state,
                selection,
                project.as_ref().map(|project| project.project_id.as_str()),
                project
                    .as_ref()
                    .and_then(|project| project.collection_id.as_deref()),
                project
                    .as_ref()
                    .and_then(|project| project.parent_project_id.as_deref()),
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
            let mut question_count = 0;
            let mut approval_count = 0;
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
                    match session.row.needs_you() {
                        Some(NeedsYou::Approval) => approval_count += 1,
                        Some(NeedsYou::Question) => question_count += 1,
                        Some(NeedsYou::Attention) => attention_count += 1,
                        None => {}
                    }
                    if session.row.shows_background_work() {
                        background_work_count += 1;
                    }
                }
            }
            // The Hermes button is the selected slot while Bots is showing.
            row.selected = !bots_mode && selection.space_id() == row.id;
            row.contains_active_session = active_space_id.as_deref() == Some(row.id.as_str());
            // CDXC:Spaces 2026-10-21 DECISION:
            // User: "if a space is active please don't show the dots under it's space button". The Space being shown has its sessions on screen, so only the other Spaces carry status counts.
            if !row.selected {
                row.working_count = working_count;
                row.attention_count = attention_count;
                row.background_work_count = background_work_count;
                row.question_count = question_count;
                row.approval_count = approval_count;
            }
        }
        spaces = rows;
    }

    // CDXC:Sidebar 2026-10-31 DECISION:
    // User (Q851, issue #359, "show all row"): `sidebarHideProjectsWithoutSessions` leaves out a project with no open session of any kind (working, idle, asleep, parked or snoozed, its session groups' included), with a Show all projects row at the end of the list that turns the full list back on until it is clicked again. The project open now always stays. Tag filters don't count here: a project they empty is the filter's business.
    let hiding_on = input.settings.hide_projects_without_sessions && !bots_mode;
    let projects_with_sessions: BTreeSet<String> = if hiding_on {
        input
            .plans
            .iter()
            .filter(|plan| {
                input
                    .builds
                    .get(&plan.group_id)
                    .is_some_and(|build| !build.store_rows.is_empty())
            })
            .map(|plan| {
                owner_project_group_id(&plan.group_id).unwrap_or_else(|| plan.group_id.clone())
            })
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut empty_projects = 0usize;
    let mut groups: Vec<GroupView> = Vec::new();
    for plan in input.plans {
        let Some(build) = input.builds.get(&plan.group_id) else {
            continue;
        };
        if is_bot_group(&plan.group_id) != bots_mode {
            continue;
        }
        if let Some(selection) = selection.as_ref().filter(|_| !bots_mode) {
            if !shows_group(selection, &plan.group_id) {
                continue;
            }
        }
        if !input.ui.show_hidden && input.ui.hidden_items.group_ids.contains(&plan.group_id) {
            continue;
        }
        if hiding_on
            && plan.project.is_some()
            && !build.core.is_active
            && !projects_with_sessions.contains(&plan.group_id)
        {
            empty_projects += 1;
            if !input.ui.show_all_projects {
                continue;
            }
        }
        if build.tag_filtered_out {
            continue;
        }
        groups.push(GroupView {
            core: build.core.clone(),
            collection_color: None,
            collection_id: project_of_group(&plan.group_id)
                .and_then(|project_id| collection_id_by_project.get(&project_id).cloned()),
            space_project: space_project_of_group(&plan.group_id),
            holds_active_session_group: false,
        });
    }

    // A session group is drawn as part of its project, so it is left out of the list while that
    // project is collapsed. The group itself stays built, so a reveal still finds it and opens
    // the project (reveal.rs).
    let rendered_ids: Vec<String> = groups
        .iter()
        .map(|group| group.core.group_id.clone())
        .filter(|group_id| !owner_project_collapsed(group_id, input.ui))
        .collect();
    fold_hidden_group_summaries(&mut groups, input.ui);
    mark_projects_holding_active_session_group(&mut groups);
    let mut collections: Vec<CollectionView> = Vec::new();
    let mut order: Vec<OrderItem> = Vec::new();
    // Collections group projects; the bots are one flat list.
    let collection_items = if bots_mode {
        rendered_ids
            .iter()
            .map(|group_id| CollectionItem::Project {
                group_id: group_id.clone(),
            })
            .collect()
    } else {
        project_sidebar_collections(
            &rendered_ids,
            &input.collections,
            &project_of_group,
            &parent_project_of_group,
        )
    };
    // A collection draws its projects' session groups under them (the desktop's
    // session_group_tree.rs), so they join the collection's ids right after their project instead
    // of standing at the top level: everything that walks the list in drawn order (the rows, the
    // next/previous session, slots, the close successor) then meets them where they are drawn.
    let session_groups_of = |project: &str| {
        rendered_ids
            .iter()
            .filter(|group_id| owner_project_group_id(group_id).as_deref() == Some(project))
            .cloned()
            .collect::<Vec<_>>()
    };
    let nested_session_groups: BTreeSet<String> = collection_items
        .iter()
        .filter_map(|item| match item {
            CollectionItem::Collection { group_ids, .. } => Some(group_ids),
            CollectionItem::Project { .. } => None,
        })
        .flatten()
        .flat_map(|project| session_groups_of(project))
        .collect();
    let mut hidden_collection_group_ids: BTreeSet<String> = BTreeSet::new();
    for item in collection_items {
        match item {
            CollectionItem::Project { group_id } => {
                if nested_session_groups.contains(&group_id) {
                    continue;
                }
                order.push(OrderItem {
                    kind: OrderKind::Project,
                    id: group_id,
                })
            }
            CollectionItem::Collection {
                collection,
                group_ids,
            } => {
                let group_ids: Vec<String> = group_ids
                    .into_iter()
                    .flat_map(|project| {
                        let session_groups = session_groups_of(&project);
                        std::iter::once(project).chain(session_groups)
                    })
                    .collect();
                let storage_id = format!("{section_key}:{}", collection.collection_id);
                if !input.ui.show_hidden
                    && input.ui.hidden_items.collection_keys.contains(&storage_id)
                {
                    hidden_collection_group_ids.extend(group_ids);
                    continue;
                }
                for group in &mut groups {
                    if group_ids.contains(&group.core.group_id) {
                        group.collection_color = Some(collection.color.clone());
                    }
                }
                let sessions: Vec<_> = collection_groups(&groups, &group_ids)
                    .flat_map(|group| group.core.sessions.clone())
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
                    question_count: summary.question_count,
                    approval_count: summary.approval_count,
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
        // A coordinator's thread on another computer counts on that computer's tab.
        for session in build
            .store_rows
            .iter()
            .filter(|session| !session.row.is_borrowed_thread())
        {
            if session.row.activity == "working" {
                machine_summary.working_count += 1;
            }
            match session.row.needs_you() {
                Some(NeedsYou::Approval) => machine_summary.approval_count += 1,
                Some(NeedsYou::Question) => machine_summary.question_count += 1,
                Some(NeedsYou::Attention) => machine_summary.attention_count += 1,
                None => {}
            }
            if session.row.shows_background_work() {
                machine_summary.background_work_count += 1;
            }
        }
    }
    // The selected machine's tab draws the counts of the list just built; every other tab draws the
    // ones counted off the store, which is the same number by another route. The one-list sidebar
    // has no machine tabs (`merged.rs`).
    let machines: Vec<MachineTabView> = input
        .host
        .machines
        .iter()
        .filter(|_| !input.host.merge_machines)
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
                question_count: counts.question_count,
                approval_count: counts.approval_count,
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
            match &selection {
                _ if bots_mode => "bots",
                Some(selection) => selection.space_id(),
                None => "all",
            }
        ),
        machine: machine_summary,
        spaces_enabled: selection.is_some(),
        spaces,
        bots_enabled: input.settings.bots_enabled,
        bots_mode,
        automations_row: bots_mode && input.settings.bot_automations_enabled,
        automations_today: bot_runs_today,
        groups,
        collections,
        order,
        machine_notice: super::machine_connection::machine_notice(
            input.host.machine(&input.ui.selected_machine_id),
            input.ui,
        ),
        hidden_collection_group_ids,
        show_all_projects_row: (empty_projects > 0).then_some(ShowAllProjectsRow {
            empty_projects,
            showing: input.ui.show_all_projects,
        }),
        empty_state: {
            let empty = empty_state(&input, selection.as_ref(), empty_projects);
            // Bots changes only the copy of a loaded, empty list; loading and a load failure read
            // the same in both modes.
            if bots_mode && !empty.loading && !empty.error {
                EmptyState {
                    copy: "No Hermes profiles found.".to_string(),
                    ..EmptyState::default()
                }
            } else {
                empty
            }
        },
    }
}

/// `createNativeEmptyState`.
fn empty_state(
    input: &AssembleInput<'_>,
    selection: Option<&SpaceSelection>,
    empty_projects: usize,
) -> EmptyState {
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
        // A machine tab that is not connected never says "No projects": its `machine_notice`
        // explains the connection instead.
        return EmptyState::default();
    }
    // `unavailable` is the local placeholder group, so the loading and waiting branches are this
    // computer's on every tab.
    let unavailable = !input.local_machine_loaded;
    // CDXC:ServerDaemon 2026-10-28 WHY:
    // A service that cannot start (its port taken, an exit right after starting) used to leave
    // "Loading sessions…" up for the three-minute start patience with the reason only in a log.
    // Once the app knows why, the sidebar says so here, at once, with a button that starts it again.
    if let Some(reason) = input
        .host
        .start_failure
        .as_deref()
        .filter(|reason| unavailable && !reason.trim().is_empty())
    {
        return EmptyState {
            loading: false,
            error: true,
            can_add_project,
            copy: super::daemon_wait::SERVICE_START_FAILED_TITLE.to_string(),
            detail: reason.trim().to_string(),
            action_label: super::daemon_wait::TRY_AGAIN_LABEL.to_string(),
            ..EmptyState::default()
        };
    }
    let wait = unavailable.then(|| DaemonWait::at(input.host.unavailable, input.now_ms));
    let error = wait.is_some_and(|wait| wait != DaemonWait::Skeleton);
    let loading = wait == Some(DaemonWait::Skeleton);
    if let Some(wait) = wait.filter(|_| error) {
        let (copy, detail) = wait.copy();
        return EmptyState {
            loading,
            error,
            can_add_project,
            copy: copy.to_string(),
            detail: detail.to_string(),
            action_label: TRY_NOW_LABEL.to_string(),
            ..EmptyState::default()
        };
    }
    // `hasKnownSidebarProjectInventory` reads `workspaceGroupIds`, which spans every machine, so a
    // user whose only projects are on a remote machine is not shown first-run copy.
    let known = input.meta.project_settings_count > 0
        || input.host.recent_project_count > 0
        || !input.plans.is_empty()
        || (input.any_machine_draws_a_project)();
    // CDXC:Sidebar 2026-10-31 DECISION:
    // User (Q853, issue #359, "hint and heading"): a tag filter that matches nothing says what happened and how to tag a session, with a Clear filter button, instead of reading "No projects". The reporter: "A short hint in the empty filter result ("No sessions tagged X. Right-click a session > Tag As") would help."
    if known && !input.ui.selected_tag_filters.is_empty() {
        return EmptyState {
            loading,
            error,
            can_add_project,
            copy: format!(
                "No sessions tagged {}.",
                input.tag_filter_labels.join(" or ")
            ),
            detail: "Right-click a session > Tag As".to_string(),
            clear_tag_filters: input.ui.selected_tag_filters.clone(),
            ..EmptyState::default()
        };
    }
    let copy = if !known {
        "No projects added yet."
    } else if empty_projects > 0 {
        "No projects with sessions."
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
        ..EmptyState::default()
    }
}

/// The next host time the empty state's waiting copy moves on its own, while this computer has not
/// loaded.
pub(crate) fn empty_state_deadline_ms(
    local_machine_loaded: bool,
    unavailable: UnavailableState,
    now_ms: u64,
) -> Option<u64> {
    (!local_machine_loaded)
        .then(|| DaemonWait::next_change_ms(unavailable, now_ms))
        .flatten()
}

/// The project group that owns a user-made session group, `None` for any other group. The
/// renderers draw a session group inside its project's block, so it folds with the project.
pub fn owner_project_group_id(group_id: &str) -> Option<String> {
    parse_workspace_subgroup_id(group_id).map(|(project, _)| project.to_sidebar_group_id())
}

/// The groups Collapse All and Expand Previous act on, out of `group_ids`: the projects without
/// their session groups, which keep their own collapse inside their project.
///
/// CDXC:Projects 2026-10-24 WHY:
/// Counted with the projects, the session groups a collapsed project hides read as expanded. With every project collapsed by hand, the More menu still offered Collapse All, which then collapsed only those hidden groups, and Expand Previous reopened only them, so the projects never opened and the button looked dead. A project group's own Collapse Projects leaves them out for the same reason.
pub fn toggle_all_project_ids<'a>(group_ids: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    group_ids
        .into_iter()
        .filter(|group_id| owner_project_group_id(group_id).is_none())
        .cloned()
        .collect()
}

/// Marks the project whose session group holds the focused session, so a collapsed project, which
/// hides its session groups, still draws as active.
fn mark_projects_holding_active_session_group(groups: &mut [GroupView]) {
    let Some(owner) = groups
        .iter()
        .find(|group| group.core.is_active)
        .and_then(|group| owner_project_group_id(&group.core.group_id))
    else {
        return;
    };
    if let Some(project) = groups.iter_mut().find(|group| group.core.group_id == owner) {
        project.holds_active_session_group = true;
    }
}

/// The groups a collection holds, `group_ids` being its ids: its projects and their session
/// groups, including those a collapsed project hides and leaves out of the ids, so the
/// collection's header counts their sessions and its menu and Select All act on them too.
pub fn collection_groups<'a>(
    groups: &'a [GroupView],
    group_ids: &'a [String],
) -> impl Iterator<Item = &'a GroupView> + 'a {
    groups.iter().filter(|group| {
        group_ids.contains(&group.core.group_id)
            || owner_project_group_id(&group.core.group_id)
                .is_some_and(|owner| group_ids.contains(&owner))
    })
}

/// Whether `group_id` is a session group whose project is collapsed.
pub(crate) fn owner_project_collapsed(group_id: &str, ui: &SidebarUiState) -> bool {
    owner_project_group_id(group_id)
        .is_some_and(|owner| ui.collapse.collapsed_groups.contains(&owner))
}

/// A collapsed project counts the working, attention, question and awake rows of the session
/// groups it hides, so its header's status still shows an agent working in one of them.
fn fold_hidden_group_summaries(groups: &mut [GroupView], ui: &SidebarUiState) {
    let mut hidden: BTreeMap<String, GroupSummary> = BTreeMap::new();
    for group in groups.iter() {
        if !owner_project_collapsed(&group.core.group_id, ui) {
            continue;
        }
        let Some(owner) = owner_project_group_id(&group.core.group_id) else {
            continue;
        };
        let total = hidden.entry(owner).or_default();
        let summary = &group.core.summary;
        total.working_count += summary.working_count;
        total.attention_count += summary.attention_count;
        total.background_work_count += summary.background_work_count;
        total.question_count += summary.question_count;
        total.approval_count += summary.approval_count;
        total.awake_count += summary.awake_count;
    }
    if hidden
        .values()
        .all(|total| *total == GroupSummary::default())
    {
        return;
    }
    for group in groups.iter_mut() {
        let Some(total) = hidden.get(&group.core.group_id) else {
            continue;
        };
        if *total == GroupSummary::default() {
            continue;
        }
        let mut core = (*group.core).clone();
        core.summary.working_count += total.working_count;
        core.summary.attention_count += total.attention_count;
        core.summary.background_work_count += total.background_work_count;
        core.summary.question_count += total.question_count;
        core.summary.approval_count += total.approval_count;
        core.summary.awake_count += total.awake_count;
        group.core = std::sync::Arc::new(core);
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

#[cfg(test)]
mod tests {
    use super::toggle_all_project_ids;
    use crate::keys::{encode_workspace_subgroup_id, ProjectKey};
    use crate::sidebar_ui::{SidebarUiIntent, SidebarUiStore, ToggleAllProjectsInput};

    /// Every project collapsed by hand while a session group inside one is still open: the toggle
    /// sees only projects, so its first press expands them instead of collapsing the hidden group.
    #[test]
    fn toggle_all_projects_leaves_out_the_session_groups_a_collapsed_project_hides() {
        let project = ProjectKey::local("p1");
        let project_id = project.to_sidebar_group_id();
        let other_id = ProjectKey::local("p2").to_sidebar_group_id();
        let session_group_id = encode_workspace_subgroup_id(&project, "g1");
        let drawn = vec![
            project_id.clone(),
            session_group_id.clone(),
            other_id.clone(),
        ];

        let group_ids = toggle_all_project_ids(&drawn);
        assert_eq!(group_ids, vec![project_id.clone(), other_id.clone()]);

        let mut store = SidebarUiStore::new();
        for group_id in [&project_id, &other_id] {
            store.apply(SidebarUiIntent::ToggleGroupCollapsed {
                group_id: group_id.clone(),
            });
        }
        store.apply(SidebarUiIntent::ToggleAllProjects(ToggleAllProjectsInput {
            machine_id: "local".to_string(),
            group_ids,
        }));
        let collapsed = &store.state().collapse.collapsed_groups;
        assert!(!collapsed.contains(&project_id));
        assert!(!collapsed.contains(&other_id));
        assert!(!collapsed.contains(&session_group_id));
    }
}
