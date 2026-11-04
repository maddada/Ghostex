//! The sidebar's own state, the intents that move it, and which values a write still owes storage.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! Two things live here that a reader would expect to find with the list: the selection, which is
//! not persisted and so is not part of what storage holds, and the memory of which projects were
//! expanded before a Collapse All, which is per machine and per collection and would otherwise be
//! reconstructed from a list that has already moved. Everything here is applied synchronously and
//! owes its write afterwards; nothing waits on the host.

use std::collections::BTreeMap;

use super::intents::{SidebarUiIntent, SidebarUiOutcome, ToggleAllProjectsInput};
use crate::sidebar_view::{SidebarMode, SidebarUiState};

/// Which persisted values a change made stale. The host writes each one at most once per burst.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SidebarPersistSet {
    pub collapse: bool,
    pub machine_tab: bool,
    pub hidden_items: bool,
}

impl SidebarPersistSet {
    fn collapse() -> Self {
        Self {
            collapse: true,
            ..Self::default()
        }
    }

    fn machine_tab() -> Self {
        Self {
            machine_tab: true,
            ..Self::default()
        }
    }

    fn hidden_items() -> Self {
        Self {
            hidden_items: true,
            ..Self::default()
        }
    }

    pub fn is_empty(self) -> bool {
        !self.collapse && !self.machine_tab && !self.hidden_items
    }

    pub fn merge(&mut self, other: Self) {
        self.collapse |= other.collapse;
        self.machine_tab |= other.machine_tab;
        self.hidden_items |= other.hidden_items;
    }
}

/// The sidebar's own state and the writes it still owes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarUiStore {
    state: SidebarUiState,
    /// Per machine, the projects that were expanded when Collapse All ran, so Expand All puts
    /// exactly those back rather than every project the machine has.
    previous_expanded_groups: BTreeMap<String, Vec<String>>,
    pending: SidebarPersistSet,
}

impl SidebarUiStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &SidebarUiState {
        &self.state
    }

    /// Seeds the state from client storage at startup. Owes no write: this is what storage holds.
    pub fn restore(&mut self, state: SidebarUiState) {
        self.state = state;
    }

    /// Drops rows the list no longer draws from the multi-selection, the way the projection prunes
    /// `selectedSessionIds` against the sessions it holds before it builds. Returns whether the
    /// selection moved. Never persisted: a selection does not survive a restart.
    pub fn retain_selected_sessions(&mut self, mut keep: impl FnMut(&str) -> bool) -> bool {
        let before = self.state.selected_session_ids.len();
        self.state
            .selected_session_ids
            .retain(|session_id| keep(session_id));
        before != self.state.selected_session_ids.len()
    }

    /// Drops ticked tag filters the Sort & Filter menu no longer offers, which is what the
    /// projection did on every build. Returns whether the list moved. Never persisted.
    pub fn retain_tag_filters(&mut self, offered: &[String]) -> bool {
        let before = self.state.selected_tag_filters.len();
        self.state
            .selected_tag_filters
            .retain(|tag| offered.iter().any(|offered| offered == tag));
        before != self.state.selected_tag_filters.len()
    }

    /// Forgets a shrunk machine notice once its machine connected, moved to any other state than
    /// the one shrunk (another failure, plain `disconnected`), or left the tabs, so the next
    /// failure opens the full card. A connect attempt in between keeps it, so a retry that fails the same way stays
    /// a pill. Returns whether anything was forgotten. Never persisted.
    pub fn retain_machine_notice_collapses(
        &mut self,
        machines: &[crate::sidebar_view::MachineTabInput],
    ) -> bool {
        let notices = &mut self.state.collapsed_machine_notices;
        let before = notices.len();
        notices.retain(|machine_id, state| {
            machines.iter().any(|machine| {
                machine.machine_id == *machine_id
                    && (machine.state == *state
                        || crate::sidebar_view::machine_state_is_busy(&machine.state)
                        // A shrunk "This is this computer" card stays shrunk while it still is.
                        || (machine.this_computer
                            && machine.is_connected()
                            && state == crate::sidebar_view::THIS_COMPUTER_NOTICE_STATE))
            })
        });
        before != notices.len()
    }

    /// The machine tab the sidebar draws.
    pub fn selected_machine_id(&self) -> &str {
        &self.state.selected_machine_id
    }

    /// Everything the host still has to write, taken once so a burst writes each value once.
    pub fn take_pending(&mut self) -> SidebarPersistSet {
        std::mem::take(&mut self.pending)
    }

    /// Whether a write is still owed, without taking it.
    pub fn pending(&self) -> SidebarPersistSet {
        self.pending
    }

    /// Owes these writes again. A host calls this when a write it had taken did not reach storage,
    /// so the next one carries the change rather than dropping it.
    pub fn mark_pending(&mut self, owed: SidebarPersistSet) {
        self.pending.merge(owed);
    }

    /// Applies one intent. The state moves at once; the write is owed afterwards.
    pub fn apply(&mut self, intent: SidebarUiIntent) -> SidebarUiOutcome {
        let outcome = self.apply_inner(intent);
        if outcome.changed {
            self.pending.merge(outcome.persist);
        }
        outcome
    }

    fn apply_inner(&mut self, intent: SidebarUiIntent) -> SidebarUiOutcome {
        match intent {
            SidebarUiIntent::ToggleGroupCollapsed { group_id } => {
                toggle(&mut self.state.collapse.collapsed_groups, group_id);
                changed(SidebarPersistSet::collapse())
            }
            SidebarUiIntent::ToggleHoverActions { storage_id } => {
                toggle(&mut self.state.collapse.expanded_hover_actions, storage_id);
                changed(SidebarPersistSet::collapse())
            }
            SidebarUiIntent::ToggleSection {
                storage_id,
                section,
            } => {
                let sections = self
                    .state
                    .collapse
                    .section_collapse
                    .entry(storage_id)
                    .or_default();
                let collapsed = sections.get(section);
                sections.set(section, !collapsed);
                changed(SidebarPersistSet::collapse())
            }
            SidebarUiIntent::ToggleCollectionCollapsed { storage_id } => {
                toggle(&mut self.state.collapse.collapsed_collections, storage_id);
                changed(SidebarPersistSet::collapse())
            }
            SidebarUiIntent::ToggleCoordinatorCollapsed { sidebar_session_id } => {
                toggle(
                    &mut self.state.collapse.collapsed_coordinators,
                    sidebar_session_id,
                );
                changed(SidebarPersistSet::collapse())
            }
            SidebarUiIntent::ToggleCoordinatorOlderThreads { sidebar_session_id } => {
                toggle(
                    &mut self.state.collapse.expanded_coordinator_older_threads,
                    sidebar_session_id,
                );
                changed(SidebarPersistSet::collapse())
            }
            SidebarUiIntent::SelectSpace { space_id } => {
                let section_key = self.state.section_key();
                // A Space shows projects, so picking one from Bots mode goes back to Projects.
                let left_bots = self.set_sidebar_mode(SidebarMode::Projects);
                let mut selected = self.set_section_space(section_key, space_id);
                selected.changed |= left_bots;
                selected
            }
            SidebarUiIntent::SetSidebarMode { mode } => {
                outcome(self.set_sidebar_mode(mode), SidebarPersistSet::collapse())
            }
            SidebarUiIntent::SetSectionSpace {
                section_key,
                space_id,
            } => self.set_section_space(section_key, space_id),
            SidebarUiIntent::SelectMachine { machine_id } => {
                let moved = self.state.selected_machine_id != machine_id;
                self.state.selected_machine_id = machine_id;
                outcome(moved, SidebarPersistSet::machine_tab())
            }
            SidebarUiIntent::SetMachineNoticeCollapsed { machine_id, state } => {
                let notices = &mut self.state.collapsed_machine_notices;
                let moved = match state {
                    Some(state) => notices.insert(machine_id, state.clone()) != Some(state),
                    None => notices.remove(&machine_id).is_some(),
                };
                outcome(moved, SidebarPersistSet::default())
            }
            SidebarUiIntent::ToggleTagFilter { tag } => {
                match self
                    .state
                    .selected_tag_filters
                    .iter()
                    .position(|held| *held == tag)
                {
                    Some(index) => {
                        self.state.selected_tag_filters.remove(index);
                    }
                    // The order the user ticked them in; the filter itself is a set, but the menu
                    // and the projection both keep the list as it was built.
                    None => self.state.selected_tag_filters.push(tag),
                }
                changed(SidebarPersistSet::default())
            }
            SidebarUiIntent::ToggleShowHidden => {
                self.state.show_hidden = !self.state.show_hidden;
                changed(SidebarPersistSet::default())
            }
            SidebarUiIntent::ToggleShowAllProjects => {
                self.state.show_all_projects = !self.state.show_all_projects;
                changed(SidebarPersistSet::default())
            }
            SidebarUiIntent::HideGroup { group_id } => {
                let moved = insert_unique(&mut self.state.hidden_items.group_ids, group_id);
                outcome(moved, SidebarPersistSet::hidden_items())
            }
            SidebarUiIntent::UnhideGroup { group_id } => {
                let moved = remove_entry(&mut self.state.hidden_items.group_ids, &group_id);
                outcome(moved, SidebarPersistSet::hidden_items())
            }
            SidebarUiIntent::HideCollection { storage_id } => {
                let moved = insert_unique(&mut self.state.hidden_items.collection_keys, storage_id);
                outcome(moved, SidebarPersistSet::hidden_items())
            }
            SidebarUiIntent::UnhideCollection { storage_id } => {
                let moved = remove_entry(&mut self.state.hidden_items.collection_keys, &storage_id);
                outcome(moved, SidebarPersistSet::hidden_items())
            }
            SidebarUiIntent::SetSelectedSessions { session_ids } => {
                let moved = self.state.selected_session_ids != session_ids;
                self.state.selected_session_ids = session_ids;
                outcome(moved, SidebarPersistSet::default())
            }
            SidebarUiIntent::ToggleAllProjects(input) => self.toggle_all_projects(input),
            SidebarUiIntent::RememberSpaceSession {
                section_key,
                space_id,
                sidebar_session_id,
            } => self.remember_space_session(section_key, space_id, sidebar_session_id),
            SidebarUiIntent::ForgetSectionSpace { section_key } => {
                let moved = self
                    .state
                    .collapse
                    .selected_space_by_section
                    .remove(&section_key)
                    .is_some();
                outcome(moved, SidebarPersistSet::collapse())
            }
            SidebarUiIntent::ExpandProjectForSlotJump { group_id } => {
                // DELETE, not toggle: `runNativeProjectSlotHotkey` removes the key, so a jump to a
                // project that is already expanded leaves it expanded.
                let moved = self.state.collapse.collapsed_groups.remove(&group_id);
                outcome(moved, SidebarPersistSet::collapse())
            }
        }
    }

    /// One section's chosen Space. `SelectSpace` is this against the section the tab is on and
    /// `SetSectionSpace` against a named one; they are one function because the follow and the
    /// renderer's command must not be able to disagree about what writing a Space means.
    fn set_section_space(&mut self, section_key: String, space_id: String) -> SidebarUiOutcome {
        let previous = self
            .state
            .collapse
            .selected_space_by_section
            .insert(section_key, space_id.clone());
        outcome(
            previous.as_deref() != Some(space_id.as_str()),
            SidebarPersistSet::collapse(),
        )
    }

    /// Returns whether the mode moved.
    fn set_sidebar_mode(&mut self, mode: SidebarMode) -> bool {
        let moved = self.state.collapse.sidebar_mode != mode;
        self.state.collapse.sidebar_mode = mode;
        moved
    }

    /// `rememberSidebarSpaceSession`: the row to the front of that Space's list, capped, with the
    /// list left exactly as it is when the row is already first.
    ///
    /// A section whose object ends up empty is not stored empty, because the reader drops an empty
    /// one and the two sides must agree on the object that is written.
    fn remember_space_session(
        &mut self,
        section_key: String,
        space_id: String,
        sidebar_session_id: String,
    ) -> SidebarUiOutcome {
        let by_space = self
            .state
            .collapse
            .recent_sessions_by_space
            .entry(section_key)
            .or_default();
        let current = by_space.entry(space_id).or_default();
        if current.first().map(String::as_str) == Some(sidebar_session_id.as_str()) {
            return outcome(false, SidebarPersistSet::collapse());
        }
        current.retain(|held| *held != sidebar_session_id);
        current.insert(0, sidebar_session_id);
        current.truncate(crate::sidebar_view::MAX_RECENT_SPACE_SESSION_IDS);
        changed(SidebarPersistSet::collapse())
    }

    /// `toggleProjects`: collapse every drawn project when any is expanded, else put back the ones
    /// that were expanded last time, falling back to every drawn project.
    fn toggle_all_projects(&mut self, input: ToggleAllProjectsInput) -> SidebarUiOutcome {
        let collapsed = &mut self.state.collapse.collapsed_groups;
        let expanded: Vec<String> = input
            .group_ids
            .iter()
            .filter(|group_id| !collapsed.contains(*group_id))
            .cloned()
            .collect();
        let mut moved = false;
        if expanded.is_empty() {
            let restore = self
                .previous_expanded_groups
                .get(&input.machine_id)
                .cloned()
                .unwrap_or(input.group_ids);
            for group_id in restore {
                moved |= collapsed.remove(&group_id);
            }
        } else {
            self.previous_expanded_groups
                .insert(input.machine_id, expanded);
            for group_id in input.group_ids {
                moved |= collapsed.insert(group_id);
            }
        }
        outcome(moved, SidebarPersistSet::collapse())
    }
}

fn toggle(set: &mut std::collections::BTreeSet<String>, key: String) {
    if !set.remove(&key) {
        set.insert(key);
    }
}

fn insert_unique(list: &mut Vec<String>, value: String) -> bool {
    if list.contains(&value) {
        return false;
    }
    list.push(value);
    true
}

fn remove_entry(list: &mut Vec<String>, value: &str) -> bool {
    let before = list.len();
    list.retain(|held| held != value);
    before != list.len()
}

fn changed(persist: SidebarPersistSet) -> SidebarUiOutcome {
    SidebarUiOutcome {
        changed: true,
        persist,
    }
}

fn outcome(changed: bool, persist: SidebarPersistSet) -> SidebarUiOutcome {
    SidebarUiOutcome { changed, persist }
}
