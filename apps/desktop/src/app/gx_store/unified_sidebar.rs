//! The Unified sidebar's store half: one neutral list per machine (every Space, no filter,
//! collapse or hide), drawn with the classic rows' own snapshot builder so every card carries the
//! same menu, hover buttons and icon, then put in one list by gx-core (`unified_sidebar/`).
//!
//! Nothing here runs while the Project sidebar is selected: the lanes are dropped and the renderer
//! draws the classic list it always did. The web build compiles this file too.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ghostex_gx_core::unified_sidebar::{
    UNIFIED_SAVED_STATE_KEY, UnifiedFilter, UnifiedFilterOption, UnifiedMachineInput,
    UnifiedRevealSpot, UnifiedRow, UnifiedSavedState, UnifiedSettings, UnifiedShelf,
    UnifiedTreeLine, build_unified_sidebar_folded, find_unified_row, group_unified_sidebar,
    move_unified_project, open_unified_reveal, repo_of_project, unified_branch_line,
    unified_lane_inputs, unified_lane_menu_inputs, unified_machine_spaces, unified_shows_machine,
    unified_space_of_project, unified_space_scope,
};
use ghostex_gx_core::{
    ChangeSummary, Core, HoverAction, LOCAL_MACHINE_ID, MachineId, MenuHost, MenuItem, ProjectKey,
    RowNesting, SessionKey, SidebarInputs, SidebarMenus, SidebarViewModel,
    unified_delete_worktree_menu,
};
use serde_json::Value;

use super::sidebar_snapshot::{SnapshotCache, SnapshotInput, snapshot_from_view};
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::OlderThreadsRow;
use crate::app::native_sidebar::model::{NativeSidebarGroup, NativeSidebarSession};
use crate::app::native_sidebar::unified::model::{
    UnifiedCard, UnifiedCardLine, UnifiedGroupView, UnifiedShelfView, UnifiedSidebarSnapshot,
};

/// One machine's neutral list and the snapshot cache its rows are drawn through.
#[derive(Default)]
struct MachineLane {
    model: SidebarViewModel,
    cache: SnapshotCache,
    inputs: SidebarInputs,
}

#[derive(Default)]
pub(crate) struct UnifiedSidebarHost {
    lanes: BTreeMap<MachineId, MachineLane>,
    /// What the header's filter bar keeps.
    pub(crate) filter: UnifiedFilter,
    /// The filter's picks, the open shelves and the folded groups, kept across restarts.
    pub(crate) saved: UnifiedSavedState,
    saved_read: bool,
    /// The order the user just dragged, until the settings fan-out brings it back (and all along
    /// on the web build, which has no settings writes).
    order_override: Option<Vec<String>>,
    /// Every repository the list knows, including what the filter and search keep off screen, for
    /// a drag to keep their places.
    all_repos: Vec<UnifiedFilterOption>,
    /// Each repository's icon as last seen, for a chip whose sessions the filter has all hidden.
    repo_icons: HashMap<String, Arc<gpui::Image>>,
    /// The group the picker's launch goes to, set from the picker's pick as it launches.
    pub(crate) picker_target: Option<String>,
    /// Where the next New thread picker starts: a "+" sets it; `None` starts where
    /// `gx_store_new_session_start` says.
    pub(crate) picker_start: Option<ghostex_gx_core::unified_sidebar::UnifiedNewSessionTarget>,
    /// A draft moving to another project, taken by the launch it asked for.
    pub(crate) pending_draft_move: Option<super::unified_new_session::PendingDraftMove>,
    /// A project picked in Quick Access whose repository the filter is set to once the list
    /// holds it (a reopened project arrives a moment later).
    pending_project: Option<String>,
    /// The branch each card last showed, by machine and row id, for the card's third line while
    /// its branch is not known (`unified_branch_line`).
    last_branches: HashMap<(MachineId, String), String>,
    /// A reveal asked for before the list held its row (the list not built yet, or the row's computer
    /// not loaded, as when the web build opens a session from its link on load), tried again after
    /// each build until the row is there.
    pending_reveal: Option<String>,
    /// The same for a session the web build opened by its key before any list held its row.
    pending_reveal_key: Option<SessionKey>,
    /// The newest reveal's row and the coordinators above it when it was revealed. A computer that
    /// answers later can nest the row under a coordinator it did not have then; when its
    /// coordinators change, the reveal runs again, so the coordinators it now has unfold. A fold the
    /// user makes afterwards leaves the coordinators the same and sticks.
    reveal_watch: Option<RevealWatch>,
    /// The row the newest reveal brought on screen: listed with the coordinators above it while
    /// their other older threads stay tucked (gx-core `UnifiedFolds::reveal`). Not saved.
    reveal_target: Option<String>,
    /// The moment the booked rebuild is for, when a nested thread stops being recent or a snoozed
    /// one wakes (gx-core `UnifiedSidebarView::deadline_ms`).
    deadline_booked: Option<i64>,
}

impl UnifiedSidebarHost {
    /// The store key of a row the Unified sidebar draws, which the Project sidebar's list may not
    /// hold (a session in another Space). The web build resolves a clicked row through it; the
    /// desktop parses the row id instead.
    #[cfg_attr(not(ghostex_web), allow(dead_code))]
    pub(crate) fn session_key_for_row(&self, sidebar_session_id: &str) -> Option<SessionKey> {
        self.lanes
            .values()
            .flat_map(|lane| lane.model.view().groups.iter())
            .flat_map(|group| group.core.sessions.iter())
            .find(|session| session.row.sidebar_session_id == sidebar_session_id)
            .and_then(|session| session.row.key.clone())
    }

    /// The machine list that holds the project a header action names, while the Unified sidebar
    /// is on (CDXC:Sidebar 2026-11-09 in gx-core `view_has_action_project`).
    pub(crate) fn view_holding_project(
        &self,
        group_id: &str,
    ) -> Option<&ghostex_gx_core::SidebarView> {
        self.lanes
            .values()
            .map(|lane| lane.model.view())
            .find(|view| ghostex_gx_core::view_has_action_project(view, group_id))
    }

    /// The row id the Unified lists draw `key` under: a borrowed thread's copy in its coordinator's
    /// lane first, since that is the row nested under the card. The web build reveals it when it
    /// opens a session.
    #[cfg_attr(not(ghostex_web), allow(dead_code))]
    pub(crate) fn row_for_session_key(&self, key: &SessionKey) -> Option<String> {
        let rows = || {
            self.lanes
                .values()
                .flat_map(|lane| lane.model.view().groups.iter())
                .flat_map(|group| group.core.sessions.iter())
                .filter(|session| session.row.key.as_ref() == Some(key))
        };
        rows()
            .find(|session| session.row.is_borrowed_thread())
            .or_else(|| rows().next())
            .map(|session| session.row.sidebar_session_id.clone())
    }

    /// The coordinators' saved folds, with the newest reveal's target.
    fn folds(&self) -> ghostex_gx_core::unified_sidebar::UnifiedFolds {
        let mut folds = self.saved.folds();
        folds.reveal.clone_from(&self.reveal_target);
        folds
    }

    /// A Quick Access pick waiting to filter the list gives way to a filter set since.
    pub(crate) fn pending_project_clear(&mut self) {
        self.pending_project = None;
    }

    /// Reads the saved state once, the first time the list is built.
    pub(crate) fn ensure_saved_read(&mut self) {
        if self.saved_read {
            return;
        }
        self.saved_read = true;
        let raw = super::sidebar_ui_storage::read_preference_value(UNIFIED_SAVED_STATE_KEY)
            .ok()
            .flatten();
        self.saved = UnifiedSavedState::parse(raw.as_deref());
        self.filter = self.saved.filter();
    }

    /// Writes the saved state with the filter's current picks.
    pub(crate) fn persist(&mut self) {
        self.saved.set_filter(&self.filter);
        let raw = self.saved.to_json();
        // A refused write only costs the state after a restart; the list keeps what it holds.
        let _ = super::sidebar_ui_storage::write_client_document_value(
            UNIFIED_SAVED_STATE_KEY,
            Some(&raw),
        );
    }

    /// A card's right-click menu, or one of its hover buttons' submenus, from the machine list
    /// that holds the session. `None` while the Unified sidebar is off (it has no lists then) or
    /// when no list holds the row.
    ///
    /// CDXC:Sidebar 2026-10-30 WHY:
    /// User: "right clicking only works on the currently active session, doesn't work on other one". A card's menu opens as a stub that asks the store for its items (`sessionMenu`), and the store answered only from the Project sidebar's own list: the selected Space's drawn rows, without other Spaces, other machines or folded projects. Any card it did not hold got an empty answer, which closes the menu. The Unified lists hold every session, so they answer first while the Unified sidebar is on.
    pub(crate) fn row_menu(
        &self,
        core: &Core,
        menu_host: &MenuHost,
        sidebar_session_id: &str,
        action: Option<HoverAction>,
        now_ms: u64,
    ) -> Option<Vec<Value>> {
        // A thread on another computer is also held, borrowed, in its coordinator's lane, inside the
        // coordinator's project: its own computer's lane comes first, so its project actions (Delete
        // Worktree) are its own project's.
        let borrowed_here = |lane: &MachineLane| {
            lane.model
                .view()
                .groups
                .iter()
                .flat_map(|group| group.core.sessions.iter())
                .any(|session| {
                    session.row.sidebar_session_id == sidebar_session_id
                        && session.row.is_borrowed_thread()
                })
        };
        let menu_from = |lane: &MachineLane| {
            let menus = SidebarMenus::new(core, lane.model.view(), &lane.inputs, menu_host, now_ms);
            let items = match action {
                // A card's Snooze button opens the submenu whatever the hover strip shows.
                Some(action) => menus.row_action_submenu(sidebar_session_id, action),
                None => menus.unified_row_menu(sidebar_session_id),
            }?;
            (!items.is_empty()).then(|| items.iter().map(MenuItem::to_json).collect())
        };
        self.lanes
            .values()
            .filter(|lane| !borrowed_here(lane))
            .find_map(menu_from)
            .or_else(|| self.lanes.values().find_map(menu_from))
    }

    /// A card's Copy ID text from the machine list that holds the session, its own computer's
    /// first, as `row_menu` picks the list. `None` while the Unified sidebar is off or no list
    /// holds the row.
    pub(crate) fn row_mention(
        &self,
        core: &Core,
        menu_host: &MenuHost,
        sidebar_session_id: &str,
        now_ms: u64,
    ) -> Option<String> {
        let mention_from = |lane: &MachineLane| {
            SidebarMenus::new(core, lane.model.view(), &lane.inputs, menu_host, now_ms)
                .row_mention(sidebar_session_id)
        };
        let own = |lane: &&MachineLane| {
            !lane
                .model
                .view()
                .groups
                .iter()
                .flat_map(|group| group.core.sessions.iter())
                .any(|session| {
                    session.row.sidebar_session_id == sidebar_session_id
                        && session.row.is_borrowed_thread()
                })
        };
        self.lanes
            .values()
            .filter(own)
            .find_map(mention_from)
            .or_else(|| self.lanes.values().find_map(mention_from))
    }

    /// A project's menu as its Project sidebar header shows it, from the machine list that holds
    /// it, with that machine's Spaces (`unified_lane_menu_inputs`).
    pub(crate) fn project_menu(
        &self,
        core: &Core,
        menu_host: &MenuHost,
        group_id: &str,
        now_ms: u64,
    ) -> Option<Vec<Value>> {
        self.lanes.values().find_map(|lane| {
            let group = lane
                .model
                .view()
                .groups
                .iter()
                .find(|group| group.core.group_id == group_id)?;
            let inputs = unified_lane_menu_inputs(&lane.inputs);
            let menus = SidebarMenus::new(core, lane.model.view(), &inputs, menu_host, now_ms);
            let mut menu = menus.project_menu(group);
            // A worktree's own menu already ends with its Delete Worktree.
            if group
                .core
                .project_context
                .as_ref()
                .is_some_and(|project| project.worktree.is_none())
            {
                menu.extend(unified_delete_worktree_menu(&self.repo_worktrees(group_id)));
            }
            Some(menu.iter().map(MenuItem::to_json).collect())
        })
    }

    /// The worktrees of the repository `group_id` is a checkout of, on every machine, as
    /// `(label, group id)` for the Delete Worktree submenu: the worktree's name, and its machine's
    /// when that is not this computer.
    fn repo_worktrees(&self, group_id: &str) -> Vec<(String, String)> {
        let Some(repo) = self
            .all_repos
            .iter()
            .find(|repo| repo.project_group_ids.iter().any(|id| id == group_id))
        else {
            return Vec::new();
        };
        let mut worktrees: Vec<(String, String)> = self
            .lanes
            .values()
            .flat_map(|lane| lane.model.view().groups.iter())
            .filter(|group| {
                repo.project_group_ids
                    .iter()
                    .any(|id| *id == group.core.group_id)
            })
            .filter_map(|group| {
                let worktree = group.core.project_context.as_ref()?.worktree.as_ref()?;
                let name = if worktree.name.is_empty() {
                    &worktree.branch
                } else {
                    &worktree.name
                };
                let label = match &group.core.remote_machine {
                    Some(remote) => format!("{name} · {}", remote.machine_name),
                    None => name.clone(),
                };
                Some((label, group.core.group_id.clone()))
            })
            .collect();
        worktrees.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
        worktrees.dedup_by(|left, right| left.1 == right.1);
        worktrees
    }

    /// Brings every machine's list up to date with `changes` and builds the Unified list.
    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        core: &Core,
        list_inputs: &SidebarInputs,
        changes: &ChangeSummary,
        menu_host: &MenuHost,
        hud: &Arc<Value>,
        now_ms: u64,
    ) -> UnifiedSidebarSnapshot {
        self.ensure_saved_read();
        let store = core.presentation();
        let machines = unified_machines(core, list_inputs);
        self.lanes
            .retain(|machine, _| machines.iter().any(|(id, _)| id == machine));

        let mut groups: HashMap<(MachineId, String), Arc<NativeSidebarGroup>> = HashMap::new();
        let mut sessions: HashMap<(MachineId, String), Arc<NativeSidebarSession>> = HashMap::new();
        // The lane group that holds each session, which a thread's Project row is drawn against.
        let mut session_groups: HashMap<(MachineId, String), String> = HashMap::new();
        for (machine, _) in &machines {
            let lane = self.lanes.entry(machine.clone()).or_default();
            lane.inputs = unified_lane_inputs(list_inputs, machine);
            lane.model.update(core, &lane.inputs, changes, now_ms);
            let menus = SidebarMenus::new(core, lane.model.view(), &lane.inputs, menu_host, now_ms);
            let snapshot = snapshot_from_view(
                lane.model.view(),
                &SnapshotInput {
                    menus: &menus,
                    hud,
                    rename_request: None,
                    reveal_request: None,
                    search_shortcut: None,
                    commands_shortcut: None,
                    settings: &lane.inputs.settings,
                    hidden_items: &lane.inputs.ui.hidden_items,
                    host: menu_host,
                    collapsed_groups: &lane.inputs.ui.collapse.collapsed_groups,
                    show_hidden: lane.inputs.ui.show_hidden,
                    selected_tag_filters: &lane.inputs.ui.selected_tag_filters,
                    now_ms,
                },
                &mut lane.cache,
            );
            for mut group in snapshot.groups {
                for session in &group.sessions {
                    sessions.insert(
                        (machine.clone(), session.session_id.clone()),
                        session.clone(),
                    );
                    session_groups.insert(
                        (machine.clone(), session.session_id.clone()),
                        group.group_id.clone(),
                    );
                }
                // A card never draws its group's other rows.
                group.sessions = Vec::new();
                groups.insert((machine.clone(), group.group_id.clone()), Arc::new(group));
            }
        }

        if let Some(project_id) = self.pending_project.clone() {
            let views: Vec<&ghostex_gx_core::SidebarView> = machines
                .iter()
                .filter_map(|(machine, _)| Some(self.lanes.get(machine)?.model.view()))
                .collect();
            let repo = repo_of_project(&views, &project_id);
            if let Some(repo) = repo {
                self.filter = UnifiedFilter {
                    repos: [repo.key].into(),
                    query: std::mem::take(&mut self.filter.query),
                    ..UnifiedFilter::default()
                };
                self.pending_project = None;
                self.persist();
            }
        }
        let inputs: Vec<UnifiedMachineInput<'_>> = machines
            .iter()
            .filter_map(|(machine, label)| {
                Some(UnifiedMachineInput {
                    machine: machine.clone(),
                    label: label.clone(),
                    view: self.lanes.get(machine)?.model.view(),
                    spaces: unified_machine_spaces(store, list_inputs, machine),
                })
            })
            .collect();
        let settings = unified_settings(list_inputs);
        let has_projects = inputs.iter().any(|input| !input.view.groups.is_empty());
        let scope = unified_space_scope(store, list_inputs);
        let folds = self.folds();
        let mut view = build_unified_sidebar_folded(
            &inputs,
            settings,
            &self.filter,
            scope.as_ref(),
            &folds,
            now_ms,
        );
        // A restored pick of what is hidden or away narrows nothing (`UnifiedFilter::within`), nor
        // does a Space pick of the machine the Space row scopes.
        let filter = self.filter.within(&view.options);
        if filter != self.filter {
            view = build_unified_sidebar_folded(
                &inputs,
                settings,
                &filter,
                scope.as_ref(),
                &folds,
                now_ms,
            );
        }
        if let Some(watch) = self.reveal_watch.as_mut() {
            let ancestors = find_unified_row(&view, &watch.row_id).map(|found| found.ancestors);
            if let Some(ancestors) = ancestors.filter(|ancestors| *ancestors != watch.ancestors) {
                // Only coordinators to unfold call for a new reveal; a row that lost its
                // coordinator is a card again and needs nothing.
                watch.changed = !ancestors.is_empty();
                watch.ancestors = ancestors;
            }
        }
        self.all_repos.clone_from(&view.options.repos);
        let repo_menu_groups = repo_menu_groups(&view.options.repos, &groups);
        // One icon per repository: the first of its projects, on any machine, that has one.
        let mut icons: HashMap<String, Arc<gpui::Image>> = HashMap::new();
        for row in view
            .drafts
            .iter()
            .chain(&view.pinned)
            .chain(&view.active)
            .chain(&view.working)
            .chain(&view.snoozed)
            .chain(&view.parked)
        {
            if icons.contains_key(&row.repo.key) {
                continue;
            }
            let icon = groups
                .get(&(row.machine.clone(), row.group.core.group_id.clone()))
                .and_then(|group| project_icon(group.project_context.as_ref()));
            if let Some(icon) = icon {
                icons.insert(row.repo.key.clone(), icon);
            }
        }
        self.repo_icons
            .extend(icons.iter().map(|(key, icon)| (key.clone(), icon.clone())));
        let repo_icons = filter
            .repos
            .iter()
            .filter_map(|key| Some((key.clone(), self.repo_icons.get(key)?.clone())))
            .collect();
        if self
            .order_override
            .as_ref()
            .is_some_and(|order| *order == list_inputs.settings.unified_project_order)
        {
            self.order_override = None;
        }
        let order = self
            .order_override
            .as_ref()
            .unwrap_or(&list_inputs.settings.unified_project_order);
        let grouped = list_inputs.settings.unified_group_by_project;
        let project_groups = if grouped {
            group_unified_sidebar(&view, order)
        } else {
            Vec::new()
        };
        for row in view
            .drafts
            .iter()
            .chain(&view.pinned)
            .chain(&view.active)
            .chain(&view.working)
            .chain(&view.snoozed)
            .chain(&view.parked)
            .flat_map(UnifiedRow::rows_in_tree)
        {
            if let Some(branch) = &row.session.row.menu_facts.branch {
                self.last_branches.insert(
                    (
                        row.machine.clone(),
                        row.session.row.sidebar_session_id.clone(),
                    ),
                    branch.clone(),
                );
            }
        }
        let last_branches = &self.last_branches;
        // Every computer's name and id (its icon is drawn from the id, gx-core
        // computer_icons.rs), from the machine tabs the host builds.
        let chips: HashMap<MachineId, (String, String)> = list_inputs
            .host
            .machines
            .iter()
            .map(|tab| {
                let machine = if tab.machine_id == LOCAL_MACHINE_ID {
                    MachineId::Local
                } else {
                    MachineId::Remote(tab.machine_id.clone())
                };
                let name = if machine.is_local() {
                    super::machine_short_names::local_computer_name()
                } else {
                    tab.label.clone()
                };
                (machine, (name, tab.machine_id.clone()))
            })
            .collect();
        let base = |row: &UnifiedRow, nesting: RowNesting| -> Option<UnifiedCard> {
            let branch_line = unified_branch_line(
                row,
                last_branches
                    .get(&(
                        row.machine.clone(),
                        row.session.row.sidebar_session_id.clone(),
                    ))
                    .map(String::as_str),
            );
            // The drawn row lives in the lane that holds it: a borrowed thread's is its
            // coordinator's computer (gx-core `UnifiedRow::lane`).
            let session = sessions
                .get(&(row.lane.clone(), row.session.row.sidebar_session_id.clone()))?
                .clone();
            Some(UnifiedCard {
                session: with_unified_nesting(&session, row, &nesting),
                row: row.session.row.clone(),
                repo_icon: icons.get(&row.repo.key).cloned(),
                repo_key: row.repo.key.clone(),
                repo_label: row.repo.label.clone(),
                status: row.status,
                branch_line,
                in_group: grouped && row.shelf != UnifiedShelf::Drafts,
                crew: row.crew.clone(),
                lines: Vec::new(),
                statuses_below: Vec::new(),
            })
        };
        let card = |row: &UnifiedRow| -> Option<UnifiedCard> {
            let mut top = base(row, row.card_nesting())?;
            top.lines = row
                .drawn_lines()
                .into_iter()
                .filter_map(|line| match line {
                    UnifiedTreeLine::Thread {
                        row: thread,
                        nesting,
                    } => {
                        let group = session_groups.get(&(
                            thread.lane.clone(),
                            thread.session.row.sidebar_session_id.clone(),
                        ))?;
                        Some(UnifiedCardLine::Thread {
                            card: Box::new(base(thread, nesting)?),
                            group: groups.get(&(thread.lane.clone(), group.clone()))?.clone(),
                        })
                    }
                    UnifiedTreeLine::Older {
                        coordinator,
                        nesting,
                    } => {
                        let crew = coordinator.crew.as_ref()?;
                        Some(UnifiedCardLine::Older(OlderThreadsRow {
                            coordinator: coordinator.session.row.sidebar_session_id.clone(),
                            unified_key: Some(crew.key.clone()),
                            depth: f32::from(nesting.depth),
                            count: u64::from(crew.older),
                            shown: crew.older_shown,
                            passing_lines: u64::from(nesting.passing_lines_below()),
                        }))
                    }
                })
                .collect();
            top.statuses_below = row
                .rows_in_tree()
                .into_iter()
                .skip(1)
                .filter_map(|below| below.status)
                .collect();
            Some(top)
        };
        let shelves_of = |working: Vec<UnifiedRow>,
                          snoozed: Vec<UnifiedRow>,
                          parked: Vec<UnifiedRow>|
         -> Vec<UnifiedShelfView> {
            [
                (UnifiedShelf::Working, working),
                (UnifiedShelf::Snoozed, snoozed),
                (UnifiedShelf::Parked, parked),
            ]
            .into_iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(shelf, rows)| UnifiedShelfView {
                shelf,
                cards: rows.iter().filter_map(card).collect(),
            })
            .collect()
        };
        let groups = project_groups
            .into_iter()
            .map(|group| {
                let count = group.count();
                // The computers whose projects make up the repository, in the tabs' order.
                let mut machines: Vec<MachineId> = view
                    .options
                    .repos
                    .iter()
                    .find(|option| option.key == group.repo_key)
                    .map(|option| {
                        option
                            .project_group_ids
                            .iter()
                            .filter_map(|id| ProjectKey::parse_sidebar_group_id(id))
                            .map(|project| project.machine)
                            .collect()
                    })
                    .unwrap_or_default();
                machines.sort_by_key(|machine| {
                    list_inputs.host.machines.iter().position(|tab| {
                        machine.remote_id().unwrap_or(LOCAL_MACHINE_ID) == tab.machine_id
                    })
                });
                machines.dedup();
                let named =
                    machines.len() > 1 || machines.iter().any(|machine| !machine.is_local());
                let machines = machines
                    .iter()
                    .filter(|_| named)
                    .filter_map(|machine| chips.get(machine).cloned())
                    .collect();
                let cards: Vec<UnifiedCard> = group
                    .pinned
                    .iter()
                    .chain(&group.active)
                    .filter_map(card)
                    .collect();
                UnifiedGroupView {
                    machines,
                    repo_key: group.repo_key,
                    label: group.label,
                    cards,
                    shelves: shelves_of(group.working, group.snoozed, group.parked),
                    count,
                }
            })
            .collect();
        let drafts = view.drafts.iter().filter_map(card).collect();
        let (list, shelves) = if grouped {
            (Vec::new(), Vec::new())
        } else {
            (
                view.pinned
                    .iter()
                    .chain(&view.active)
                    .filter_map(card)
                    .collect(),
                shelves_of(view.working, view.snoozed, view.parked),
            )
        };
        UnifiedSidebarSnapshot {
            drafts,
            list,
            shelves,
            groups,
            options: view.options,
            filter,
            total: view.total,
            has_projects,
            repo_icons,
            repo_menu_groups,
            space_menu_machine: list_inputs.ui.selected_machine_id.clone(),
            space: scope.map(|scope| scope.label),
            deadline_ms: view.deadline_ms,
        }
    }
}

/// This computer always, then every remote machine the host feeds into the store, under the names
/// their tabs carry.
fn unified_machines(core: &Core, list_inputs: &SidebarInputs) -> Vec<(MachineId, String)> {
    let store = core.presentation();
    let mut machines: Vec<(MachineId, String)> = vec![(MachineId::Local, "Local".to_string())];
    for tab in &list_inputs.host.machines {
        if tab.machine_id == LOCAL_MACHINE_ID {
            machines[0].1.clone_from(&tab.label);
            continue;
        }
        let machine = MachineId::Remote(tab.machine_id.clone());
        // A computer that is not connected gives no cards (`unified_shows_machine`).
        if tab.fed && !tab.this_computer && unified_shows_machine(store, &machine) {
            machines.push((machine, tab.label.clone()));
        }
    }
    machines
}

fn unified_settings(list_inputs: &SidebarInputs) -> UnifiedSettings {
    UnifiedSettings {
        working_section: list_inputs.settings.unified_working_section,
        enable_session_parking: list_inputs.settings.enable_session_parking,
        pinned_session_order: list_inputs.settings.pinned_session_order,
    }
}

/// Where the drawn list has the card of `sidebar_session_id`: Drafts, the list, a shelf, or a
/// project group and a shelf inside it.
fn drawn_reveal_spot(
    snapshot: &UnifiedSidebarSnapshot,
    sidebar_session_id: &str,
) -> Option<UnifiedRevealSpot> {
    // A thread drawn under its card counts: its coordinators are open already.
    let holds = |cards: &[UnifiedCard]| {
        cards.iter().any(|card| {
            card.session.session_id == sidebar_session_id
                || card.lines.iter().any(|line| {
                    matches!(line, UnifiedCardLine::Thread { card, .. }
                        if card.session.session_id == sidebar_session_id)
                })
        })
    };
    let spot = |shelf, repo_key: &str| UnifiedRevealSpot {
        shelf,
        repo_key: repo_key.to_string(),
        ancestors: Vec::new(),
    };
    if holds(&snapshot.drafts) {
        return Some(spot(UnifiedShelf::Drafts, ""));
    }
    if holds(&snapshot.list) {
        return Some(spot(UnifiedShelf::Active, ""));
    }
    if let Some(shelf) = snapshot.shelves.iter().find(|shelf| holds(&shelf.cards)) {
        return Some(spot(shelf.shelf, ""));
    }
    snapshot.groups.iter().find_map(|group| {
        if holds(&group.cards) {
            return Some(spot(UnifiedShelf::Active, &group.repo_key));
        }
        let shelf = group.shelves.iter().find(|shelf| holds(&shelf.cards))?;
        Some(spot(shelf.shelf, &group.repo_key))
    })
}

/// The newest reveal's row (`UnifiedSidebarHost::reveal_watch`).
struct RevealWatch {
    row_id: String,
    /// Its coordinators, outermost first, as the newest build had them.
    ancestors: Vec<String>,
    /// A build found different coordinators above it: reveal it again.
    changed: bool,
}

/// What a reveal opens for a row the drawn list does not show.
struct HiddenReveal {
    spot: UnifiedRevealSpot,
    /// The filter that lets it through, when the current one leaves it out.
    filter: Option<UnifiedFilter>,
    /// The row waits behind its coordinator's older-threads row, so it is listed as the reveal's
    /// target rather than listing every older thread.
    older: bool,
}

impl UnifiedSidebarHost {
    /// The coordinators above `sidebar_session_id` in the list with nothing filtering, outermost
    /// first; empty for a card or a row the list does not hold.
    fn ancestry(
        &self,
        core: &Core,
        list_inputs: &SidebarInputs,
        sidebar_session_id: &str,
        now_ms: u64,
    ) -> Vec<String> {
        let machines = unified_machines(core, list_inputs);
        let inputs: Vec<UnifiedMachineInput<'_>> = machines
            .iter()
            .filter_map(|(machine, label)| {
                Some(UnifiedMachineInput {
                    machine: machine.clone(),
                    label: label.clone(),
                    view: self.lanes.get(machine)?.model.view(),
                    spaces: unified_machine_spaces(core.presentation(), list_inputs, machine),
                })
            })
            .collect();
        let view = build_unified_sidebar_folded(
            &inputs,
            unified_settings(list_inputs),
            &UnifiedFilter::default(),
            unified_space_scope(core.presentation(), list_inputs).as_ref(),
            &self.folds(),
            now_ms,
        );
        find_unified_row(&view, sidebar_session_id)
            .map(|found| found.ancestors)
            .unwrap_or_default()
    }

    /// Where a row the drawn list does not show would sit: a thread inside a folded coordinator,
    /// behind an older-threads row, or a card or thread the filter leaves out (lifted with
    /// `UnifiedFilter::admitting`). It builds the list again with the current filter and, when
    /// that does not hold the row, with nothing filtering, which a reveal pays only when the drawn
    /// list does not draw the row.
    fn hidden_reveal(
        &self,
        core: &Core,
        list_inputs: &SidebarInputs,
        sidebar_session_id: &str,
        now_ms: u64,
    ) -> Option<HiddenReveal> {
        let machines = unified_machines(core, list_inputs);
        let inputs: Vec<UnifiedMachineInput<'_>> = machines
            .iter()
            .filter_map(|(machine, label)| {
                Some(UnifiedMachineInput {
                    machine: machine.clone(),
                    label: label.clone(),
                    view: self.lanes.get(machine)?.model.view(),
                    spaces: unified_machine_spaces(core.presentation(), list_inputs, machine),
                })
            })
            .collect();
        // Inside the active Space: a card of another Space is fetched by the Space switch, not by
        // lifting the filter.
        let scope = unified_space_scope(core.presentation(), list_inputs);
        let folds = self.folds();
        let build = |filter: &UnifiedFilter| {
            build_unified_sidebar_folded(
                &inputs,
                unified_settings(list_inputs),
                filter,
                scope.as_ref(),
                &folds,
                now_ms,
            )
        };
        let reveal = |view: &ghostex_gx_core::unified_sidebar::UnifiedSidebarView| {
            let found = find_unified_row(view, sidebar_session_id)?;
            // A thread the filter keeps only for a sibling's sake is shown once the filter lets
            // it through too.
            let filter = self.filter.admitting(found.row);
            Some(HiddenReveal {
                spot: found.reveal_spot(),
                filter,
                older: !found.row.shown,
            })
        };
        reveal(&build(&self.filter)).or_else(|| {
            if self.filter.keeps_everything() {
                return None;
            }
            reveal(&build(&UnifiedFilter::default()))
        })
    }
}

impl UnifiedSidebarHost {
    /// The Space to switch the Space row to so a session just started in `project_id` on
    /// `machine_id` is in the scoped list, with the section key that selection is stored under;
    /// `None` when it already is or nothing scopes that machine.
    ///
    /// CDXC:Spaces 2026-11-05 DECISION:
    /// A session started from the New thread picker, or a draft moved with the tray's project chip, can land in a project of another Space, and a draft out of the scoped list is a draft you can't see. Its project's Space becomes the active one, as a new session in a filtered-out project adds its chip (`widen_filter_for_new_session`). It is a plain selection, not the Space switch's restore, which would focus that Space's last session over the new one.
    pub(crate) fn space_for_new_session(
        &self,
        core: &Core,
        list_inputs: &SidebarInputs,
        machine_id: &str,
        project_id: &str,
    ) -> Option<(String, String)> {
        let scope = unified_space_scope(core.presentation(), list_inputs)?;
        // Active projects lists a project the moment a session starts in it, so it stays.
        if scope.is_active_projects() {
            return None;
        }
        let machine = match machine_id {
            LOCAL_MACHINE_ID => MachineId::Local,
            remote => MachineId::Remote(remote.to_string()),
        };
        // In the one list the Space scopes every machine (gx-core `UnifiedSpaceScope::every_machine`).
        if !scope.every_machine && scope.machine != machine {
            return None;
        }
        let spaces = unified_machine_spaces(core.presentation(), list_inputs, &machine)?;
        let view = self.lanes.get(&machine)?.model.view();
        let space_id = unified_space_of_project(view, &spaces, project_id)?;
        (space_id != scope.space_id).then(|| (list_inputs.ui.section_key(), space_id))
    }
}

impl GhostexGpuiApp {
    /// The list moved: rebuild the Unified sidebar when it is the one selected, or let it go when
    /// it is not. `changes` is what the store changed since the previous call.
    pub(crate) fn gx_store_unified_sidebar_changed(
        &mut self,
        changes: &ChangeSummary,
        cx: &mut gpui::Context<Self>,
    ) {
        // Read with the Project sidebar selected too: the tray's project chip reads and writes it.
        self.gx_store.unified_sidebar.ensure_saved_read();
        if !self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_sidebar
        {
            // The Project sidebar reveals its own rows.
            self.gx_store.unified_sidebar.pending_reveal = None;
            self.gx_store.unified_sidebar.pending_reveal_key = None;
            self.gx_store.unified_sidebar.reveal_watch = None;
            if self.native_sidebar.unified.is_some() {
                self.gx_store.unified_sidebar.lanes.clear();
                self.native_sidebar.unified = None;
                cx.notify();
            }
            return;
        }
        // Without the HUD nothing can be drawn, and the lanes would miss this change: they start
        // over once it is back.
        let Some(hud) = self.gx_store_sidebar_hud() else {
            self.gx_store.unified_sidebar.lanes.clear();
            return;
        };
        let menu_host = self.gx_store_menu_host();
        let now_ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or_default();
        let store = &mut self.gx_store;
        let snapshot = store.unified_sidebar.update(
            &store.core,
            &store.sidebar_list.last_inputs,
            changes,
            &menu_host,
            &hud,
            now_ms,
        );
        let deadline = snapshot.deadline_ms;
        self.native_sidebar.unified = Some(Arc::new(snapshot));
        self.track_native_unified_folds();
        self.gx_store_book_unified_deadline(deadline, cx);
        if let Some(pending) = self.gx_store.unified_sidebar.pending_reveal.take() {
            self.gx_store_unified_reveal(&pending, cx);
        }
        let rerun = self
            .gx_store
            .unified_sidebar
            .reveal_watch
            .as_mut()
            .filter(|watch| watch.changed)
            .map(|watch| {
                watch.changed = false;
                watch.row_id.clone()
            });
        if let Some(row_id) = rerun {
            self.gx_store_unified_reveal(&row_id, cx);
        }
        let host = &mut self.gx_store.unified_sidebar;
        if let Some(row_id) = host
            .pending_reveal_key
            .as_ref()
            .and_then(|key| host.row_for_session_key(key))
        {
            host.pending_reveal_key = None;
            self.gx_store_unified_reveal(&row_id, cx);
        }
        cx.notify();
    }

    /// Books one rebuild for when a thread under a card stops being recent (it moves behind its
    /// coordinator's older-threads row) or a snoozed thread wakes (it nests again). A repaint
    /// would not do: the tree comes from the build.
    fn gx_store_book_unified_deadline(
        &mut self,
        deadline: Option<i64>,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = &mut self.gx_store.unified_sidebar;
        let Some(deadline) = deadline else {
            host.deadline_booked = None;
            return;
        };
        // A timer for an earlier or equal moment already covers this one.
        if host
            .deadline_booked
            .is_some_and(|booked| booked <= deadline)
        {
            return;
        }
        host.deadline_booked = Some(deadline);
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default();
        let wait = std::time::Duration::from_millis(deadline.saturating_sub(now).max(250) as u64);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.unified_sidebar.deadline_booked != Some(deadline) {
                    return;
                }
                this.gx_store.unified_sidebar.deadline_booked = None;
                this.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
            });
        })
        .detach();
    }

    /// The crew count's click: folds or unfolds a coordinator's threads (`older` false), or lists
    /// or tucks its older threads (`older` true), saved per client (CDXC:Sidebar 2026-11-09 in
    /// gx-core unified_sidebar/tree.rs), and builds the list again.
    pub(crate) fn gx_store_toggle_unified_coordinator(
        &mut self,
        key: &str,
        older: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = &mut self.gx_store.unified_sidebar;
        if older {
            host.saved.toggle_older_threads(key);
        } else {
            host.saved.toggle_coordinator(key);
        }
        host.persist();
        self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
    }

    /// The Unified reveal of a session the web build opens by its key: now when a list holds its row,
    /// else once one does.
    #[cfg_attr(not(ghostex_web), allow(dead_code))]
    pub(crate) fn gx_store_unified_reveal_session(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let unified = self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_sidebar;
        let host = &mut self.gx_store.unified_sidebar;
        match host.row_for_session_key(session) {
            Some(row_id) => {
                host.pending_reveal_key = None;
                self.gx_store_unified_reveal(&row_id, cx);
            }
            // Kept only while the Unified sidebar is on.
            None => host.pending_reveal_key = unified.then(|| session.clone()),
        }
    }

    /// The Unified half of a reveal: opens what folds the card away so the renderer draws it and
    /// scrolls to it (CDXC:Sidebar 2026-11-05 in gx-core `unified_sidebar/reveal.rs`). Nothing
    /// while the Project sidebar is selected, whose own reveal plan does this for its headings.
    pub(crate) fn gx_store_unified_reveal(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(snapshot) = self.native_sidebar.unified.clone() else {
            // Kept for the first build only while the Unified sidebar is on; the Project sidebar
            // reveals its own rows.
            if self
                .gx_store
                .sidebar_list
                .last_inputs
                .settings
                .unified_sidebar
            {
                self.gx_store.unified_sidebar.pending_reveal = Some(sidebar_session_id.to_string());
            }
            return;
        };
        self.gx_store.unified_sidebar.pending_reveal = None;
        let now_ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or_default();
        // A new row to watch starts from the coordinators it has now; the same row keeps what the
        // newest build saw.
        let store = &self.gx_store;
        if store
            .unified_sidebar
            .reveal_watch
            .as_ref()
            .is_none_or(|watch| watch.row_id != sidebar_session_id)
        {
            let ancestors = store.unified_sidebar.ancestry(
                &store.core,
                &store.sidebar_list.last_inputs,
                sidebar_session_id,
                now_ms,
            );
            self.gx_store.unified_sidebar.reveal_watch = Some(RevealWatch {
                row_id: sidebar_session_id.to_string(),
                ancestors,
                changed: false,
            });
        }
        // The older thread an earlier reveal listed goes back behind its row once another row is
        // revealed.
        let host = &mut self.gx_store.unified_sidebar;
        let dropped_target = host
            .reveal_target
            .take_if(|target| target.as_str() != sidebar_session_id)
            .is_some();
        let grouped = self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_group_by_project;
        // Rebuilt when a reveal unfolds a coordinator, lists its older threads or lifts the
        // filter, or an earlier reveal's older thread goes back: the snapshot's threads come from
        // the build.
        let mut rebuild = dropped_target;
        let spot = match drawn_reveal_spot(&snapshot, sidebar_session_id) {
            Some(spot) => spot,
            None => {
                let store = &self.gx_store;
                let Some(hidden) = store.unified_sidebar.hidden_reveal(
                    &store.core,
                    &store.sidebar_list.last_inputs,
                    sidebar_session_id,
                    now_ms,
                ) else {
                    // Its computer's rows have not arrived yet: try again once they have.
                    let host = &mut self.gx_store.unified_sidebar;
                    if host.session_key_for_row(sidebar_session_id).is_none() {
                        host.pending_reveal = Some(sidebar_session_id.to_string());
                    }
                    if rebuild {
                        self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
                    }
                    return;
                };
                let host = &mut self.gx_store.unified_sidebar;
                if let Some(filter) = hidden.filter {
                    host.filter = filter;
                    host.pending_project_clear();
                    rebuild = true;
                }
                if hidden.older {
                    host.reveal_target = Some(sidebar_session_id.to_string());
                    rebuild = true;
                }
                rebuild |= !hidden.spot.ancestors.is_empty();
                hidden.spot
            }
        };
        let host = &mut self.gx_store.unified_sidebar;
        let opened = open_unified_reveal(&mut host.saved, &spot, grouped);
        if opened || rebuild {
            host.persist();
        }
        if rebuild {
            self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
        } else if opened {
            cx.notify();
        }
    }

    /// A filter row's project menu (`UnifiedSidebarHost::project_menu`).
    pub(crate) fn gx_store_unified_project_menu(&mut self, group_id: &str) -> Option<Value> {
        let menu_host = self.gx_store_menu_host();
        let now_ms = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or_default();
        self.gx_store
            .unified_sidebar
            .project_menu(&self.gx_store.core, &menu_host, group_id, now_ms)
            .map(Value::Array)
    }

    /// A project picked in Quick Access › Projects: while the Unified sidebar is on, its filter
    /// shows that project's repository alone, on every machine. The Project sidebar ignores it.
    ///
    /// CDXC:Sidebar 2026-10-30 DECISION:
    /// User: "when i click on a project here and unified sidebar is active then we should filter by that project in the unified sidebar". Opening or reopening a project from Quick Access › Projects (click or Enter) sets the Unified filter to that project's repository, as picking it in the filter menu would, and replaces any machine or Space pick so every machine's copy shows; the pick still opens the project as before. A reopened project is matched once it is back in the list.
    pub(crate) fn gx_store_unified_filter_to_project(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_sidebar
        {
            return;
        }
        self.gx_store.unified_sidebar.pending_project = Some(project_id.to_string());
        self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
    }

    /// The Unified header's filter menu and its shelves. Returns whether the command was one.
    pub(crate) fn gx_store_run_unified_sidebar_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command["type"].as_str() {
            Some("unifiedFilter") => {
                self.gx_store.unified_sidebar.pending_project = None;
                let filter = &mut self.gx_store.unified_sidebar.filter;
                match (command["kind"].as_str(), command["key"].as_str()) {
                    // All sessions clears every chip, the text with them.
                    (Some("clear"), _) => *filter = UnifiedFilter::default(),
                    // The dropdown's Search sessions row, and the text chip's X with no text.
                    (Some("text"), _) => {
                        filter.query = command["text"].as_str().unwrap_or_default().to_string();
                    }
                    (Some(kind), Some(key)) => {
                        let set = match kind {
                            "repo" => &mut filter.repos,
                            "machine" => &mut filter.machines,
                            "space" => &mut filter.spaces,
                            _ => return true,
                        };
                        if !set.remove(key) {
                            set.insert(key.to_string());
                        }
                    }
                    _ => return true,
                }
                self.gx_store.unified_sidebar.persist();
                self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
                true
            }
            Some("unifiedRemoveProject") => {
                if let Some(key) = command["key"].as_str() {
                    let key = key.to_string();
                    self.gx_store_unified_remove_project(&key, cx);
                }
                true
            }
            // CDXC:Sidebar 2026-10-31 DECISION: User (issue #359): Collapse All folds the groups the Unified sidebar draws, not the hidden Project list. It folds them all, or opens them all when every one is already folded.
            Some("sidebarAction")
                if command["action"] == ghostex_gx_core::UNIFIED_TOGGLE_GROUPS_ACTION =>
            {
                let keys: Vec<String> = self
                    .native_sidebar
                    .unified
                    .as_ref()
                    .map(|unified| {
                        unified
                            .groups
                            .iter()
                            .map(|group| group.repo_key.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let host = &mut self.gx_store.unified_sidebar;
                let all_folded = keys
                    .iter()
                    .all(|key| host.saved.folded_groups.contains(key));
                for key in keys {
                    if all_folded {
                        host.saved.folded_groups.remove(&key);
                    } else {
                        host.saved.folded_groups.insert(key);
                    }
                }
                host.persist();
                cx.notify();
                true
            }
            Some("unifiedHoverExpand") => {
                let host = &mut self.gx_store.unified_sidebar;
                host.saved.strip_expanded = !host.saved.strip_expanded;
                host.persist();
                cx.notify();
                true
            }
            Some("unifiedNewSessionInRepo") => {
                if let Some(key) = command["key"].as_str() {
                    let key = key.to_string();
                    self.open_new_session_in_repo(&key, cx);
                }
                true
            }
            Some("unifiedNewSession") => {
                if let (Some(machine_id), Some(project_id)) =
                    (command["machineId"].as_str(), command["projectId"].as_str())
                {
                    let (machine_id, project_id) = (machine_id.to_string(), project_id.to_string());
                    self.open_new_session_in(&machine_id, &project_id, cx);
                }
                true
            }
            _ => false,
        }
    }
}

impl GhostexGpuiApp {
    /// A filter row's Remove button, on its second click: takes the repository's projects off the
    /// list through Close Project, on every machine that has them.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User: "i don't understand how the user can remove a project that was added before, i think in the filter i want to be able to click on a button on the right side twice to remove a project". A filter row is a repository, so its Remove sends each of its projects (machines and worktrees alike) the message a project's own Close Project sends: each goes to its machine's Recent Projects, where Quick Access can reopen it, and its sessions keep their records and go on running out of sight. A pick of that repository leaves the filter, so reopening the project doesn't bring back a narrowed list. If the active session is among them, it moves to an awake session outside the repository.
    fn gx_store_unified_remove_project(&mut self, key: &str, cx: &mut gpui::Context<Self>) {
        let host = &mut self.gx_store.unified_sidebar;
        let mut group_ids = host
            .all_repos
            .iter()
            .find(|option| option.key == key)
            .map(|option| option.project_group_ids.clone())
            .unwrap_or_default();
        if host.filter.repos.remove(key) {
            host.persist();
        }
        let active_group = self.gx_store.core.focus().active_group.clone();
        // The active project first: its close moves the focus before anything else leaves.
        group_ids.sort_by_key(|group_id| {
            !ghostex_gx_core::close_project_group_is_active(active_group.as_ref(), group_id)
        });
        for group_id in &group_ids {
            let mut message = serde_json::json!({
                "type": "closeWorkspaceProjectForGroup",
                "groupId": group_id,
            });
            if ghostex_gx_core::close_project_group_is_active(active_group.as_ref(), group_id) {
                message["successorSessionId"] = Value::from(
                    self.gx_store
                        .sidebar_list
                        .close_projects_successor_session_id(group_id, &group_ids)
                        .unwrap_or_default(),
                );
            }
            self.dispatch_native_sidebar_ui(
                serde_json::json!({"type": "command", "message": message}),
                cx,
            );
        }
        self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
    }

    /// A project group's header was dropped on another's. `after`: below it.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION: User (Q850, "drag"): the order is saved in the `unifiedProjectOrder` setting, by repository, so the machines and the phone read the same order; the whole order is written, so repositories the filter keeps off screen keep their place. The Project sidebar's drag code (native_sidebar/drag.rs, session_drop.rs) is not involved: this list has no collections and no per-machine order to keep.
    pub(in crate::app) fn gx_store_unified_move_group(
        &mut self,
        moved: &str,
        target: &str,
        after: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = &mut self.gx_store.unified_sidebar;
        let saved_order = &self
            .gx_store
            .sidebar_list
            .last_inputs
            .settings
            .unified_project_order;
        let current = host.order_override.as_ref().unwrap_or(saved_order);
        let order = move_unified_project(current, &host.all_repos, moved, target, after);
        host.order_override = Some(order.clone());
        self.gx_store_save_unified_project_order(order, cx);
        self.gx_store_unified_sidebar_changed(&ChangeSummary::default(), cx);
    }

    fn gx_store_save_unified_project_order(
        &mut self,
        order: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(not(ghostex_web))]
        self.handle_gpui_app_modal_update_settings_patch_message(
            &serde_json::json!({
                "type": "updateSettingsPatch",
                "source": "sidebar:unifiedProjectOrder",
                "patch": { "unifiedProjectOrder": order },
            }),
            cx,
        );
        #[cfg(ghostex_web)]
        let _ = (order, cx);
    }
}

/// The session's sidebar row with its place in the Unified tree in place of the Project
/// sidebar's (gx-core `UnifiedRow::drawn_lines`), and, on a row with threads, the key its fold
/// control toggles (`coordinatorThreads.unifiedKey`, read by native_sidebar/threads.rs).
fn with_unified_nesting(
    session: &Arc<NativeSidebarSession>,
    row: &UnifiedRow,
    nesting: &RowNesting,
) -> Arc<NativeSidebarSession> {
    let mut drawn = (**session).clone();
    for key in [
        "threadDepth",
        "threadLast",
        "threadPassingLines",
        "coordinatorThreads",
    ] {
        drawn.details.remove(key);
    }
    super::sidebar_snapshot::insert_coordinator_nesting(
        &mut drawn.details,
        &row.session.row,
        nesting,
    );
    if let (Some(crew), Some(threads)) = (
        row.crew.as_ref(),
        drawn
            .details
            .get_mut("coordinatorThreads")
            .and_then(Value::as_object_mut),
    ) {
        threads.insert("unifiedKey".to_string(), Value::from(crew.key.clone()));
    }
    Arc::new(drawn)
}

/// The project whose menu each repository's filter row opens: the main checkout rather than a
/// worktree, on this computer when it has one there.
fn repo_menu_groups(
    repos: &[UnifiedFilterOption],
    groups: &HashMap<(MachineId, String), Arc<NativeSidebarGroup>>,
) -> HashMap<String, String> {
    repos
        .iter()
        .filter_map(|repo| {
            let group = repo
                .project_group_ids
                .iter()
                .filter_map(|group_id| {
                    let machine =
                        ghostex_gx_core::ProjectKey::parse_sidebar_group_id(group_id)?.machine;
                    groups
                        .get(&(machine.clone(), group_id.clone()))
                        .map(|group| (machine, group))
                })
                .min_by_key(|(machine, group)| {
                    let worktree = group
                        .project_context
                        .as_ref()
                        .is_some_and(|project| !project["worktree"].is_null());
                    (!machine.is_local(), worktree)
                })?
                .1;
            Some((repo.key.clone(), group.group_id.clone()))
        })
        .collect()
}

/// The project's own icon, as its Project sidebar header draws it: the one set for it, else the
/// one found in its folder.
fn project_icon(project_context: Option<&Value>) -> Option<Arc<gpui::Image>> {
    project_context
        .and_then(|project| {
            project
                .get("iconDataUrl")
                .or_else(|| project.get("discoveredIconDataUrl"))
        })
        .and_then(Value::as_str)
        .and_then(crate::app::native_sidebar::images::sidebar_image)
}
