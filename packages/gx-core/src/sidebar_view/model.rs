//! The sidebar view model: one cache that derives the list from the store, keeps it up to date
//! from a [`ChangeSummary`], and can rebuild it from scratch to the same result.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! Every publish of the TypeScript projection re-derived every row of the machine (title, tooltip, tag lookup, sorting), which cost 30 to 40 ms on the service thread while agents were running. Here a row is derived once and kept behind an `Arc` until the store says that session changed, a group is rebuilt only when one of its own inputs moved, and an update with an empty change summary and unchanged inputs does no work at all. The from-scratch build exists so the two can be compared: anything the incremental path forgets to invalidate shows up as a difference.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::change::ChangeSummary;
use crate::core::Core;
use crate::keys::{MachineId, ProjectKey, SessionKey, CHATS_GROUP_ID};
use crate::presentation_store::PresentationStore;

use super::assemble::{assemble, AssembleInput};
use super::collections::CollectionsState;
use super::groups::{
    build_group, FocusKey, GroupBuild, GroupKind, GroupPlan, ProjectContextInput, RowRef,
};
use super::inputs::{BrowserTabInput, SectionCollapse, SidebarInputs, LOCAL_MACHINE_ID};
use super::machines::machine_tab_summary;
use super::membership::{project_members, ProjectMembers};
use super::projects::ProjectMeta;
use super::rows::{browser_row, session_row, RowContext};
use super::spaces::SpacesState;
use super::tags::TagCatalog;
use super::view::{MachineSummary, RemoteMachineView, SidebarView};

/// The inputs of one group, so a group that nothing touched is kept as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GroupKey {
    /// The cache's number for every row of the group, in order. A rebuilt row gets a new number,
    /// so this says both "the same rows" and "the same values" without holding on to anything.
    rows: Vec<u64>,
    title: String,
    storage_id: String,
    /// Identity of the project facts; a changed project gives a new one.
    project: Option<usize>,
    is_active: bool,
    /// Focus only matters for the active group; every other group draws no focused row.
    focused_session_id: Option<String>,
    visible_session_ids: Vec<String>,
    active_project_matches: bool,
    selected_rows: Vec<usize>,
    tag_filters: Vec<String>,
    collapsed: bool,
    expanded: bool,
    hover_actions_expanded: bool,
    section_collapse: SectionCollapse,
    enable_parking: bool,
    compact_count: u32,
    sort_mode: super::inputs::SessionSortMode,
    /// The machine's stream dropped while its rows are held, which fades the whole group.
    is_stale: bool,
    /// The remote machine's name as the settings spell it; it rides in the group's Copy Details.
    machine_name: Option<String>,
}

struct CachedGroup {
    key: GroupKey,
    build: GroupBuild,
}

/// What one update actually did.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// An update that takes twenty milliseconds and one that takes twenty microseconds are the same
/// call with the same arguments from outside, and the difference is always which caches it had to
/// drop. Counts only, never a clock: this crate reads no clock, and the host times the call it
/// makes anyway. The flags are the three that invalidate everything at once, which is what a slow
/// update almost always is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SidebarUpdateWork {
    /// The whole cache was dropped: another machine, a machine that (un)loaded, or a reload.
    pub reset: bool,
    /// Every row was re-derived: the tag catalog or Debugging Mode moved.
    pub rows_all_dirty: bool,
    /// Every project fact was rebuilt.
    pub meta_dirty: bool,
    /// Rows derived from a session this time; the rest were kept.
    pub rows_built: usize,
    /// Rows derived from a browser tab this time.
    pub browser_rows_built: usize,
    /// Groups ordered, sectioned and summarised this time.
    pub groups_built: usize,
    /// Machines whose badge was counted off the store this time.
    pub machine_summaries_built: usize,
    /// What the update ended up holding, so a count is read against a size.
    pub group_count: usize,
    pub row_count: usize,
}

struct CacheState {
    machine: MachineId,
    inputs: SidebarInputs,
    focus: FocusKey,
    catalog: TagCatalog,
    /// This computer's catalog alone; see [`TagCatalog`].
    filter_catalog: TagCatalog,
    meta: Arc<ProjectMeta>,
    membership: BTreeMap<String, Arc<ProjectMembers>>,
    project_contexts: BTreeMap<String, Arc<ProjectContextInput>>,
    rows: BTreeMap<String, BTreeMap<String, RowRef>>,
    browser_rows: BTreeMap<String, (Vec<BrowserTabInput>, Vec<RowRef>)>,
    /// The next row number to mint; monotonic while the cache lives.
    next_row_id: u64,
    groups: BTreeMap<String, CachedGroup>,
    view: SidebarView,
    next_deadline_ms: Option<u64>,
    machine_loaded: bool,
    /// The badge counts of every machine tab, kept until that machine's rows move. The selected
    /// machine's come out of its built groups; the others are counted straight off the store.
    machine_summaries: BTreeMap<MachineId, MachineSummary>,
    work: SidebarUpdateWork,
}

/// The sidebar list of one machine, kept up to date from the store.
#[derive(Default)]
pub struct SidebarViewModel {
    state: Option<CacheState>,
}

impl SidebarViewModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// The list as it stands. Empty until the first [`Self::update`].
    pub fn view(&self) -> &SidebarView {
        static EMPTY: std::sync::OnceLock<SidebarView> = std::sync::OnceLock::new();
        match &self.state {
            Some(state) => &state.view,
            None => EMPTY.get_or_init(SidebarView::default),
        }
    }

    /// `resolveCloseProjectSuccessorSessionId`: the session Close Project focuses before it parks
    /// the project, or `None` when no candidate holds an awake row.
    ///
    /// The rows are read from the group's UNFILTERED, unsorted set, which is what the TypeScript's
    /// `sessionIdsByGroup` is: the drawn `GroupCore::sessions` is the display layout after the tag
    /// filter, and a successor chosen out of that would move with a filter the close has nothing to
    /// do with.
    pub fn close_project_successor_session_id(&self, closing_group_id: &str) -> Option<String> {
        let state = self.state.as_ref()?;
        super::close_successor::close_project_successor_candidates(&state.view, closing_group_id)
            .into_iter()
            .find_map(|group_id| {
                let build = state.groups.get(&group_id)?;
                super::close_successor::first_awake_successor_session_id(&build.build.store_rows)
                    .map(str::to_string)
            })
    }

    /// Every group this model built, drawn or not, in the order it planned them, with each one's
    /// rows before any filter: the Chats group, then each project followed by its user-made groups.
    /// Quick Access lists the sessions of hidden, filtered and Chats groups too, which is what the
    /// old runtime's `sessionIdsByGroup` held.
    pub fn built_groups(&self) -> Vec<(&super::view::GroupCore, Vec<&super::view::SessionRow>)> {
        let Some(state) = &self.state else {
            return Vec::new();
        };
        let mut ids: Vec<String> = vec![chats_group_id(&state.machine)];
        for project_id in &state.meta.project_order {
            ids.push(
                ProjectKey {
                    machine: state.machine.clone(),
                    project_id: project_id.clone(),
                }
                .to_sidebar_group_id(),
            );
            if let Some(members) = state.membership.get(project_id) {
                ids.extend(
                    members
                        .subgroups
                        .iter()
                        .map(|subgroup| subgroup.sidebar_group_id.clone()),
                );
            }
        }
        ids.iter()
            .filter_map(|id| state.groups.get(id))
            .map(|cached| {
                (
                    cached.build.core.as_ref(),
                    cached
                        .build
                        .store_rows
                        .iter()
                        .map(|session| session.row.as_ref())
                        .collect(),
                )
            })
            .collect()
    }

    /// The next host time at which a row moves on its own (a new session stops leading the list, a
    /// snooze ends). The host re-runs the update then; nothing else has to.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.state.as_ref().and_then(|state| state.next_deadline_ms)
    }

    /// What the newest [`Self::update`] had to rebuild. An update that returned early because
    /// nothing moved leaves the previous answer standing, which is why the host only reads this
    /// for an update it timed.
    pub fn last_work(&self) -> SidebarUpdateWork {
        self.state
            .as_ref()
            .map(|state| state.work)
            .unwrap_or_default()
    }

    /// Builds the whole list without any cache. The incremental update must give the same result;
    /// the replay tool compares the two on every event.
    pub fn build_from_scratch(core: &Core, inputs: &SidebarInputs, now_ms: u64) -> SidebarView {
        let mut model = Self::new();
        model.update(core, inputs, &full_change_summary(), now_ms);
        model.view().clone()
    }

    /// Applies what changed. Returns whether the list itself moved.
    pub fn update(
        &mut self,
        core: &Core,
        inputs: &SidebarInputs,
        changes: &ChangeSummary,
        now_ms: u64,
    ) -> bool {
        let machine = machine_id(&inputs.ui.selected_machine_id);
        let focus = focus_key(core, &machine);
        let store = core.presentation();
        let machine_loaded = store.loaded(&machine).is_some();

        let reset = match &self.state {
            None => true,
            Some(state) => {
                state.machine != machine
                    || state.machine_loaded != machine_loaded
                    || changes.machines_reloaded.contains(&machine)
            }
        };
        if !reset {
            let state = self.state.as_ref().expect("checked above");
            let time_passed = state
                .next_deadline_ms
                .is_some_and(|deadline| now_ms >= deadline);
            if changes.is_empty() && !time_passed && state.focus == focus && state.inputs == *inputs
            {
                return false;
            }
        }
        if reset {
            self.state = None;
        }

        let mut previous = self.state.take();
        // What a tag id RESOLVES to: every machine's catalog, this computer's first, which is
        // `getSessionTagCatalogs`. A row's label and its tag icon read this.
        let catalog = TagCatalog::merged(
            std::iter::once(MachineId::Local)
                .chain(
                    store
                        .machines()
                        .map(|(machine, _)| machine.clone())
                        .filter(|machine| !machine.is_local()),
                )
                .map(|machine| {
                    store
                        .machine(&machine)
                        .and_then(|entry| entry.side_state().custom_session_tags.as_ref())
                })
                .collect::<Vec<_>>(),
        );
        // Which tag FILTERS the sidebar offers, and which ticked ones survive a prune: this
        // computer's catalog alone, whichever machine tab is selected.
        let filter_catalog = TagCatalog::from_state(
            store
                .machine(&MachineId::Local)
                .and_then(|machine| machine.side_state().custom_session_tags.as_ref()),
        );
        let rows_all_dirty = previous.as_ref().is_none_or(|state| {
            state.catalog != catalog
                || state.filter_catalog != filter_catalog
                || state.inputs.settings.debugging_mode != inputs.settings.debugging_mode
        });
        let meta_dirty = previous.as_ref().is_none_or(|previous| {
            !changes.projects_changed.is_empty()
                || !changes.projects_removed.is_empty()
                || !changes.project_order_changed.is_empty()
                || !changes.chat_collection_changed.is_empty()
                || changes.side_state.workspace_groups
                || previous.inputs.host.recent_project_ids != inputs.host.recent_project_ids
                || previous.inputs.host.remote_recent_project_ids
                    != inputs.host.remote_recent_project_ids
        });
        // The projection prunes the ticked filters to the ones the Sort & Filter menu still
        // offers before it reads them, so a filter whose tag was turned off in settings (or whose
        // custom tag the daemon dropped) filters nothing.
        let enabled_filters = inputs.settings.enabled_tag_filters(&filter_catalog);
        let effective_inputs: Cow<'_, SidebarInputs> = if inputs
            .ui
            .selected_tag_filters
            .iter()
            .all(|tag| enabled_filters.contains(tag))
        {
            Cow::Borrowed(inputs)
        } else {
            let mut pruned = inputs.clone();
            pruned
                .ui
                .selected_tag_filters
                .retain(|tag| enabled_filters.contains(tag));
            Cow::Owned(pruned)
        };
        let effective = effective_inputs.as_ref();
        let row_context = RowContext {
            catalog: &catalog,
            debugging_mode: effective.settings.debugging_mode,
            always_show_state_tooltip: !machine.is_local(),
        };

        // 1. Project facts.
        let meta = match (&previous, meta_dirty) {
            (Some(previous), false) => previous.meta.clone(),
            _ => Arc::new(match store.machine(&machine) {
                Some(entry) => super::projects::build_project_meta(
                    entry,
                    // The machine's OWN order document. `orderGpuiRemotePresentationGroups` reads
                    // `presentation.workspaceGroups.projectOrder` of the remote daemon, so reading
                    // this computer's here would order a remote machine's projects by a list of
                    // project ids that belong to another daemon.
                    entry
                        .side_state()
                        .workspace_groups
                        .as_ref()
                        .map(|groups| groups.project_order.as_slice())
                        .unwrap_or_default(),
                    inputs.host.parked_project_ids(&machine),
                ),
                None => ProjectMeta::default(),
            }),
        };

        // 2. Which sessions and projects moved.
        let mut dirty_projects: BTreeSet<&str> = BTreeSet::new();
        let mut dirty_rows: BTreeSet<(&str, &str)> = BTreeSet::new();
        for project in changes
            .session_order_changed
            .iter()
            .filter(|project| project.machine == machine)
        {
            dirty_projects.insert(project.project_id.as_str());
        }
        for session in changes
            .sessions_changed
            .iter()
            .chain(&changes.sessions_removed)
            .filter(|session| session.machine == machine)
        {
            dirty_projects.insert(session.project_id.as_str());
            dirty_rows.insert((session.project_id.as_str(), session.session_id.as_str()));
        }
        let timer_keys: BTreeSet<(String, String)> = match &previous {
            Some(previous) if !rows_all_dirty => changed_timer_keys(&previous.inputs, inputs)
                .iter()
                .filter_map(|key| parse_sidebar_session_key(key))
                .collect(),
            _ => BTreeSet::new(),
        };
        // Projects whose git numbers moved; their header and tooltip are rebuilt.
        let diff_stats_dirty: BTreeSet<String> = match &previous {
            Some(previous) => previous
                .inputs
                .host
                .project_diff_stats
                .keys()
                .chain(inputs.host.project_diff_stats.keys())
                .filter(|project_id| {
                    previous.inputs.host.project_diff_stats.get(*project_id)
                        != inputs.host.project_diff_stats.get(*project_id)
                })
                .cloned()
                .collect(),
            None => BTreeSet::new(),
        };
        for (project_id, session_id) in &timer_keys {
            dirty_rows.insert((project_id.as_str(), session_id.as_str()));
        }

        // Never restarts, not even when every row is dropped: the cached groups live on, and a
        // group is kept when its `rows` numbers match. Minting 1, 2, 3 again for re-derived rows
        // would hand a rebuilt group the number sequence of the one it replaces, and every group
        // whose membership did not move would keep serving its pre-catalog tag label and tooltip.
        let next_row_id = previous.as_ref().map_or(1, |previous| previous.next_row_id);
        // 3. The rows themselves: kept from the previous round unless the store or a host timer
        // touched them.
        // The rows are moved out of the previous round rather than copied: a machine with two
        // hundred sessions would otherwise clone every key on every update.
        let mut rows = match (&mut previous, rows_all_dirty) {
            (Some(previous), false) => std::mem::take(&mut previous.rows),
            _ => BTreeMap::new(),
        };
        for (project_id, session_id) in &dirty_rows {
            if let Some(sessions) = rows.get_mut(*project_id) {
                sessions.remove(*session_id);
            }
        }
        for project in changes
            .projects_removed
            .iter()
            .filter(|project| project.machine == machine)
        {
            rows.remove(&project.project_id);
        }

        // 4. Membership, per project.
        let membership_all_dirty = previous.is_none() || meta_dirty;
        let mut membership: BTreeMap<String, Arc<ProjectMembers>> = BTreeMap::new();
        for project_id in meta.project_order.iter().chain(&meta.chat_order) {
            let reuse = !membership_all_dirty && !dirty_projects.contains(project_id.as_str());
            let members = match (&previous, reuse) {
                (Some(previous), true) => previous.membership.get(project_id).cloned(),
                _ => None,
            };
            let members = members.unwrap_or_else(|| {
                Arc::new(match store.machine(&machine) {
                    Some(entry) => project_members(
                        store,
                        entry,
                        &machine,
                        project_id,
                        meta.project_order.iter().any(|id| id == project_id),
                    ),
                    None => ProjectMembers::default(),
                })
            });
            membership.insert(project_id.clone(), members);
        }

        // 5. Group plans, with every row resolved.
        let mut state = CacheState {
            machine: machine.clone(),
            inputs: inputs.clone(),
            focus,
            catalog: catalog.clone(),
            filter_catalog,
            meta: meta.clone(),
            membership,
            project_contexts: BTreeMap::new(),
            rows,
            browser_rows: BTreeMap::new(),
            next_row_id,
            groups: BTreeMap::new(),
            view: SidebarView::default(),
            next_deadline_ms: None,
            machine_loaded,
            machine_summaries: BTreeMap::new(),
            work: SidebarUpdateWork {
                reset,
                rows_all_dirty,
                meta_dirty,
                ..SidebarUpdateWork::default()
            },
        };
        // A machine whose rows are held while its stream is not live draws them faded rather than
        // dropping them, which is what `createRemoteSidebarGroups` does with its last-seen copy.
        // Never for this computer: the old projection has an unavailable placeholder group for it
        // and marks no group stale.
        //
        // CDXC:RemoteMachines 2026-09-20 WHY:
        // "Not live" rather than `Stale`, which is what this first tested. A client that cannot
        // reach its daemon walks a reconnect ladder, and every rung emits `Connecting` before the
        // failure emits `Lost`; the store maps only the second of those to `Stale`, so an
        // unreachable machine alternated faded and not faded once per rung, and each flip cost a
        // badge recount and a full install of the list.
        let is_stale = !machine.is_local()
            && store.machine(&machine).is_some_and(|entry| {
                entry.loaded().is_some() && entry.connection().phase != crate::ConnectionPhase::Live
            });
        let machine_name = machine
            .remote_id()
            .map(|machine_id| {
                effective
                    .host
                    .machine(machine_id)
                    .map(|tab| tab.label.clone())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let remote_context = |project_id: Option<&str>| -> Option<RemoteMachineView> {
            machine.remote_id().map(|machine_id| RemoteMachineView {
                machine_id: machine_id.to_string(),
                machine_name: machine_name.clone(),
                project_id: project_id.map(str::to_string),
            })
        };
        let mut plans: Vec<GroupPlan> = Vec::new();
        let chat_members: Vec<(String, String)> = meta
            .chat_order
            .iter()
            .flat_map(|project_id| {
                state
                    .membership
                    .get(project_id)
                    .map(|members| members.session_ids.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|session_id| (project_id.clone(), session_id))
            })
            .collect();
        let chat_rows = chat_members
            .iter()
            .filter_map(|(project_id, session_id)| {
                resolve_row(
                    &mut state,
                    store,
                    &machine,
                    project_id,
                    session_id,
                    effective,
                    &row_context,
                )
            })
            .collect();
        plans.push(GroupPlan {
            group_id: chats_group_id(&machine),
            storage_id: storage_id(&machine, &chats_group_id(&machine)),
            title: "Chats".to_string(),
            kind: GroupKind::Chats,
            rows: chat_rows,
            project: None,
            remote_machine: remote_context(None),
            is_stale,
        });
        for project_id in meta.project_order.iter() {
            let project_key = ProjectKey {
                machine: machine.clone(),
                project_id: project_id.clone(),
            };
            let members = state
                .membership
                .get(project_id)
                .cloned()
                .unwrap_or_default();
            // The app's browser tabs carry the MACHINE-SCOPED project id for a remote project and
            // the raw id for a local one, which is the same string. The desktop host stopped
            // feeding them on 2026-09-20, when browser tabs left the sidebar for the view panel's
            // tab strip; this path is still here for a caller that supplies its own tabs.
            let tab_project_id = project_key.to_workspace_project_id();
            let mut rows: Vec<RowRef> = browser_rows_for_project(
                &mut state,
                previous.as_ref().filter(|_| !rows_all_dirty),
                project_id,
                &tab_project_id,
                effective,
                &row_context,
            );
            for session_id in &members.session_ids {
                if let Some(row) = resolve_row(
                    &mut state,
                    store,
                    &machine,
                    project_id,
                    session_id,
                    effective,
                    &row_context,
                ) {
                    rows.push(row);
                }
            }
            // A project's facts are kept until the project row, the overlays, or the host's git
            // numbers move.
            let project_dirty = meta_dirty
                || dirty_projects.contains(project_id.as_str())
                || diff_stats_dirty.contains(project_id);
            let project = match (&previous, project_dirty) {
                (Some(previous), false) => previous.project_contexts.get(project_id).cloned(),
                _ => None,
            }
            .or_else(|| {
                project_context(store, &machine, &meta, project_id, effective).map(Arc::new)
            });
            if let Some(project) = &project {
                state
                    .project_contexts
                    .insert(project_id.clone(), project.clone());
            }
            plans.push(GroupPlan {
                group_id: project_key.to_sidebar_group_id(),
                // `projectNativeSidebarGroup` keys a project group by
                // `projectContext.editor.projectId`, which is the workspace project id: the raw id
                // locally, `remote:<machine>:project:<id>` on a remote machine. The prefix below
                // is then applied on top of it, which is what the persisted collapse state holds.
                storage_id: storage_id(&machine, &tab_project_id),
                title: project
                    .as_ref()
                    .map(|project| project.title.clone())
                    .unwrap_or_default(),
                kind: GroupKind::Project,
                rows,
                project,
                remote_machine: remote_context(Some(project_id)),
                is_stale,
            });
            for subgroup in &members.subgroups {
                let mut rows: Vec<RowRef> = Vec::new();
                for session_id in &subgroup.session_ids {
                    if let Some(row) = resolve_row(
                        &mut state,
                        store,
                        &machine,
                        project_id,
                        session_id,
                        effective,
                        &row_context,
                    ) {
                        rows.push(row);
                    }
                }
                plans.push(GroupPlan {
                    group_id: subgroup.sidebar_group_id.clone(),
                    storage_id: storage_id(&machine, &subgroup.sidebar_group_id),
                    title: subgroup.title.clone(),
                    kind: GroupKind::Subgroup,
                    rows,
                    project: None,
                    remote_machine: remote_context(Some(project_id)),
                    is_stale,
                });
            }
        }

        let mut builds: BTreeMap<String, GroupBuild> = BTreeMap::new();
        for plan in &plans {
            let key = group_key(plan, &state.focus, effective);
            let reuse = previous
                .as_mut()
                .and_then(|previous| previous.groups.remove(&plan.group_id))
                .filter(|cached| cached.key == key)
                .filter(|cached| {
                    cached
                        .build
                        .deadline_ms
                        .is_none_or(|deadline| now_ms < deadline)
                });
            let build = match reuse {
                Some(cached) => cached.build,
                None => {
                    state.work.groups_built += 1;
                    build_group(
                        plan,
                        &state.focus,
                        &effective.ui,
                        &effective.settings,
                        now_ms,
                    )
                }
            };
            state.next_deadline_ms = min_deadline(state.next_deadline_ms, build.deadline_ms);
            builds.insert(plan.group_id.clone(), build.clone());
            state
                .groups
                .insert(plan.group_id.clone(), CachedGroup { key, build });
        }

        // 6. The machine tabs. The selected machine's counts come out of the groups just built;
        // every other machine's are counted off the store and kept until its rows move.
        let (summaries, summaries_built) = machine_summaries(
            store,
            &machine,
            effective,
            previous
                .as_ref()
                .map(|previous| &previous.machine_summaries),
            changes,
        );
        state.machine_summaries = summaries;
        state.work.machine_summaries_built = summaries_built;

        // 7. The list itself.
        let side_state = store.machine(&machine).map(|entry| entry.side_state());
        state.view = assemble(AssembleInput {
            plans: &plans,
            builds: &builds,
            meta: &meta,
            ui: &effective.ui,
            settings: &effective.settings,
            host: &effective.host,
            spaces: side_state
                .and_then(|side| side.spaces.as_ref())
                .map(SpacesState::from_wire),
            collections: match side_state.and_then(|side| side.project_collections.as_ref()) {
                // A document has arrived for this machine, so the daemon is authoritative from
                // here on, an empty one included: after the user deletes their last collection
                // the stored copy still holds it for as long as it takes the sidebar to write,
                // and falling back to it there would draw the collection the user just removed.
                Some(state) => CollectionsState::from_wire(state),
                None => effective
                    .host
                    .stored_project_collections
                    .as_ref()
                    .map(CollectionsState::from_local_json)
                    .unwrap_or_default(),
            },
            local_machine_loaded: store.loaded(&MachineId::Local).is_some(),
            machine_summaries: &state.machine_summaries,
            // Asked only when the two cheap tests before it both say no, which on a populated
            // sidebar they never do; walking every machine's projects is not per-update work.
            any_machine_draws_a_project: &|| {
                super::machines::any_machine_draws_a_project(store, &effective.host)
            },
            now_ms,
        });
        state.work.group_count = state.view.groups.len();
        state.work.row_count = state
            .view
            .groups
            .iter()
            .map(|group| group.core.sessions.len())
            .sum();
        let changed = previous
            .as_ref()
            .is_none_or(|previous| previous.view != state.view);
        self.state = Some(state);
        changed
    }
}

/// Every row of a project's browser tabs, kept until the tabs themselves change.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// `previous` is withheld when every row is dirty, and that is the whole point of the argument
/// rather than a detail. A browser row is built from the same [`RowContext`] as a session row, so
/// its tooltip carries the tag catalog and Debugging Mode, and reusing it on the strength of an
/// unchanged tab list alone kept a pre-catalog tooltip on a row whose id never moved, which kept
/// its group's cache key unchanged, which kept the whole group. It survived every replay because
/// no recording carries browser tabs; it took a live sidebar with a browser row open and one tag
/// catalog change to show, as one row differing from a build with no cache at all.
fn browser_rows_for_project(
    state: &mut CacheState,
    previous: Option<&CacheState>,
    project_id: &str,
    tab_project_id: &str,
    inputs: &SidebarInputs,
    context: &RowContext<'_>,
) -> Vec<RowRef> {
    let tabs: Vec<BrowserTabInput> = inputs
        .host
        .browser_tabs
        .iter()
        .filter(|tab| tab.project_id == tab_project_id)
        .cloned()
        .collect();
    if tabs.is_empty() {
        return Vec::new();
    }
    let reuse = previous
        .and_then(|previous| previous.browser_rows.get(project_id))
        .filter(|(cached_tabs, _)| *cached_tabs == tabs)
        .map(|(_, rows)| rows.clone());
    let rows = reuse.unwrap_or_else(|| {
        tabs.iter()
            .map(|tab| {
                state.work.browser_rows_built += 1;
                state.next_row_id += 1;
                RowRef {
                    id: state.next_row_id,
                    row: Arc::new(browser_row(tab, context)),
                }
            })
            .collect()
    });
    state
        .browser_rows
        .insert(project_id.to_string(), (tabs, rows.clone()));
    rows
}

/// One session's row: the one already held when nothing touched it, a fresh one otherwise. The
/// rows of sessions the store reported as changed were dropped before the plans were built.
fn resolve_row(
    state: &mut CacheState,
    store: &PresentationStore,
    machine: &MachineId,
    project_id: &str,
    session_id: &str,
    inputs: &SidebarInputs,
    context: &RowContext<'_>,
) -> Option<RowRef> {
    if let Some(row) = state
        .rows
        .get(project_id)
        .and_then(|sessions| sessions.get(session_id))
    {
        return Some(row.clone());
    }
    let session = store
        .machine(machine)?
        .effective_session(project_id, session_id)?;
    let key = SessionKey {
        machine: machine.clone(),
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let sidebar_id = key.to_sidebar_session_id();
    state.next_row_id += 1;
    state.work.rows_built += 1;
    let row = RowRef {
        id: state.next_row_id,
        row: Arc::new(session_row(
            key,
            &session,
            super::inputs::CloseAfterDoneInput::from_session(&session).as_ref(),
            inputs.host.local_delayed_sends.get(&sidebar_id),
            context,
        )),
    };
    state
        .rows
        .entry(project_id.to_string())
        .or_default()
        .insert(session_id.to_string(), row.clone());
    Some(row)
}

/// The project facts a project group draws.
fn project_context(
    store: &PresentationStore,
    machine: &MachineId,
    meta: &ProjectMeta,
    project_id: &str,
    inputs: &SidebarInputs,
) -> Option<ProjectContextInput> {
    let loaded = store.loaded(machine)?;
    let project = loaded.project(project_id)?;
    let overlay = meta.overlay(project_id);
    let worktree = overlay.and_then(|overlay| overlay.worktree.clone());
    // `project-sections.ts` counts the GROUPS the sidebar holds, so a parked or otherwise
    // unlisted worktree project does not raise the number.
    //
    // It compares a candidate's `worktree.parentProjectId`, which is the RAW project id of the
    // daemon that published it, against this project's `projectContext.editor.projectId`, which is
    // the WORKSPACE project id. The two are the same string for a local project and never the same
    // for a remote one, so a remote project's worktree count is zero there and has to be zero
    // here: the number on a remote header is the TypeScript's, not a better one.
    let owner_id = ProjectKey {
        machine: machine.clone(),
        project_id: project_id.to_string(),
    }
    .to_workspace_project_id();
    let worktree_count = meta
        .project_order
        .iter()
        .filter(|candidate| {
            meta.overlay(candidate)
                .and_then(|overlay| overlay.worktree.as_ref())
                .is_some_and(|worktree| worktree.parent_project_id == owner_id)
        })
        .count();
    Some(ProjectContextInput {
        project_id: project_id.to_string(),
        title: project.title.clone(),
        path: project.path.clone().unwrap_or_default(),
        icon_data_url: overlay.and_then(|overlay| overlay.icon_data_url.clone()),
        discovered_icon_data_url: project.discovered_icon_data_url.clone(),
        worktree,
        diff_stats: inputs
            .host
            .project_diff_stats
            .get(project_id)
            .copied()
            .unwrap_or_default(),
        worktree_count,
        // `Tri` keeps "not probed" apart from "probed, no origin"; the menu tests the value for
        // truthiness, so both collapse to nothing here.
        git_remote_origin_url: project
            .git_remote_origin_url
            .value()
            .filter(|url| !url.is_empty())
            .cloned(),
    })
}

fn group_key(plan: &GroupPlan, focus: &FocusKey, inputs: &SidebarInputs) -> GroupKey {
    let is_active = focus.active_group_id.as_deref() == Some(plan.group_id.as_str());
    GroupKey {
        rows: plan.rows.iter().map(|row| row.id).collect(),
        title: plan.title.clone(),
        storage_id: plan.storage_id.clone(),
        project: plan
            .project
            .as_ref()
            .map(|project| Arc::as_ptr(project) as usize),
        is_active,
        focused_session_id: is_active
            .then(|| focus.focused_session_id.clone())
            .flatten(),
        visible_session_ids: if is_active {
            focus.visible_session_ids.clone()
        } else {
            Vec::new()
        },
        active_project_matches: plan.project.as_ref().is_some_and(|project| {
            focus.active_project_id.as_deref() == Some(project.project_id.as_str())
        }),
        selected_rows: plan
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                inputs
                    .ui
                    .selected_session_ids
                    .iter()
                    .any(|selected| *selected == row.row.sidebar_session_id)
            })
            .map(|(index, _)| index)
            .collect(),
        tag_filters: inputs.ui.selected_tag_filters.clone(),
        collapsed: inputs.ui.collapse.collapsed_groups.contains(&plan.group_id),
        expanded: inputs
            .ui
            .collapse
            .expanded_session_lists
            .contains(&plan.storage_id),
        hover_actions_expanded: inputs
            .ui
            .collapse
            .expanded_hover_actions
            .contains(&plan.storage_id),
        section_collapse: inputs
            .ui
            .collapse
            .section_collapse
            .get(&plan.storage_id)
            .copied()
            .unwrap_or_default(),
        enable_parking: inputs.settings.enable_session_parking,
        compact_count: inputs.settings.project_session_list_collapsed_count,
        sort_mode: inputs.settings.sort_mode,
        is_stale: plan.is_stale,
        machine_name: plan
            .remote_machine
            .as_ref()
            .map(|remote| remote.machine_name.clone()),
    }
}

/// The sidebar session ids whose host-owned timers changed.
fn changed_timer_keys(previous: &SidebarInputs, next: &SidebarInputs) -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for key in previous
        .host
        .local_delayed_sends
        .keys()
        .chain(next.host.local_delayed_sends.keys())
    {
        if previous.host.local_delayed_sends.get(key) != next.host.local_delayed_sends.get(key) {
            keys.insert(key.clone());
        }
    }
    keys
}

/// `combined-session:<project>:<session>` back to its parts.
fn parse_sidebar_session_key(sidebar_session_id: &str) -> Option<(String, String)> {
    SessionKey::parse_sidebar_session_id(sidebar_session_id)
        .map(|key| (key.project_id, key.session_id))
}

/// The id a group's own UI state is stored under.
///
/// `projectNativeSidebarGroup` prefixes every group of a remote machine with `remote:<machine>:`,
/// on top of an id that already names the machine for a project group. The double prefix is what
/// the persisted collapse state holds, so it is reproduced rather than tidied up.
fn storage_id(machine: &MachineId, raw: &str) -> String {
    match machine.remote_id() {
        None => raw.to_string(),
        Some(machine_id) => format!("remote:{machine_id}:{raw}"),
    }
}

/// The badge counts of every machine tab the host offers EXCEPT the selected one, whose counts
/// come out of the groups the list just built.
///
/// A machine whose rows the burst did not touch keeps the count it had: the walk is over every
/// session of that machine and a tab strip is redrawn far more often than a machine's rows move.
/// Leaving the selected machine out is what keeps a one-machine sidebar paying nothing for this.
fn machine_summaries(
    store: &PresentationStore,
    selected: &MachineId,
    inputs: &SidebarInputs,
    previous: Option<&BTreeMap<MachineId, MachineSummary>>,
    changes: &ChangeSummary,
) -> (BTreeMap<MachineId, MachineSummary>, usize) {
    let mut summaries: BTreeMap<MachineId, MachineSummary> = BTreeMap::new();
    let mut built = 0usize;
    let wanted = inputs
        .host
        .machines
        .iter()
        .map(|machine| machine_id(&machine.machine_id))
        .chain(std::iter::once(MachineId::Local));
    for machine in wanted {
        if machine == *selected || summaries.contains_key(&machine) {
            continue;
        }
        let touched = changes.machines_reloaded.contains(&machine)
            || changes.connection_changed.contains(&machine)
            || changes.project_order_changed.contains(&machine)
            || changes.chat_collection_changed.contains(&machine)
            || changes
                .sessions_changed
                .iter()
                .chain(&changes.sessions_removed)
                .any(|session| session.machine == machine)
            || changes
                .projects_changed
                .iter()
                .chain(&changes.projects_removed)
                .any(|project| project.machine == machine)
            || changes
                .session_order_changed
                .iter()
                .any(|project| project.machine == machine)
            // The user-made groups of every machine live in this computer's document.
            || changes.side_state.workspace_groups;
        let summary = match previous
            .filter(|_| !touched)
            .and_then(|held| held.get(&machine))
        {
            Some(summary) => *summary,
            None => {
                built += 1;
                machine_tab_summary(store, &machine, inputs.host.parked_project_ids(&machine))
            }
        };
        summaries.insert(machine, summary);
    }
    (summaries, built)
}

fn machine_id(selected_machine_id: &str) -> MachineId {
    if selected_machine_id == LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(selected_machine_id.to_string())
    }
}

fn chats_group_id(machine: &MachineId) -> String {
    match machine {
        MachineId::Local => CHATS_GROUP_ID.to_string(),
        MachineId::Remote(machine_id) => {
            ProjectKey::remote(machine_id.as_str(), CHATS_GROUP_ID).to_sidebar_group_id()
        }
    }
}

/// Focus in the vocabulary the rows compare against: raw session ids of this machine.
fn focus_key(core: &Core, machine: &MachineId) -> FocusKey {
    let focus = core.focus();
    FocusKey {
        active_group_id: focus
            .active_group
            .as_ref()
            .map(crate::focus::ActiveGroup::to_sidebar_group_id),
        active_project_id: focus
            .active_project
            .as_ref()
            .filter(|project| project.machine == *machine)
            .map(|project| project.project_id.clone()),
        focused_session_id: focus
            .focused_session
            .as_ref()
            .filter(|session| session.machine == *machine)
            .map(|session| session.session_id.clone()),
        visible_session_ids: focus
            .visible_sessions
            .iter()
            .filter(|session| session.machine == *machine)
            .map(|session| session.session_id.clone())
            .collect(),
    }
}

fn min_deadline(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (value, None) | (None, value) => value,
    }
}

/// A change summary that invalidates everything, for the from-scratch build.
fn full_change_summary() -> ChangeSummary {
    ChangeSummary::default()
}
