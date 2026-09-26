//! The drawn list, with the desktop's names (`view`, `last_inputs`, `note_changes`, `mark_dirty`) so the desktop's executor files read and move it unchanged. The desktop's `sidebar_list.rs` also paces installs, carries timer deadlines and keeps the old runtime's facts in step; the page rebuilds on every change and on the sidebar clock.
use ghostex_gx_core::{
    ChangeSummary, Core, MenuHost, SidebarInputs, SidebarMenus, SidebarUiState, SidebarView,
    SidebarViewModel,
};
use serde_json::Value;

use super::sidebar_snapshot::{SnapshotCache, SnapshotInput, snapshot_from_view};
use crate::app::native_sidebar::model::NativeSidebarSnapshot;

#[derive(Default)]
pub(crate) struct SidebarList {
    model: SidebarViewModel,
    /// The inputs the last update read, as on the desktop.
    pub(crate) last_inputs: SidebarInputs,
    changes: ChangeSummary,
    snapshot_cache: SnapshotCache,
    /// The wake booked for the next moment a row or its label reads differently (`sidebar_clock.rs`).
    pub(crate) deadline_booked: Option<u64>,
}

impl SidebarList {
    pub(crate) fn view(&self) -> &SidebarView {
        self.model.view()
    }

    #[allow(dead_code)]
    pub(crate) fn model(&self) -> &SidebarViewModel {
        &self.model
    }

    pub(crate) fn close_project_successor_session_id(&self, group_id: &str) -> Option<String> {
        self.model.close_project_successor_session_id(group_id)
    }

    /// The next host time the list reads differently: a row moving on its own (a snooze ending) or a label changing (a countdown, a relative time).
    pub(crate) fn next_deadline_ms(&self, now_ms: u64) -> Option<u64> {
        let label = self
            .model
            .view()
            .groups
            .iter()
            .flat_map(|group| group.core.sessions.iter())
            .filter_map(|session| session.row.next_label_deadline(now_ms, true))
            .map(|deadline| deadline.at_ms())
            .min();
        match (self.model.next_deadline_ms(), label) {
            (Some(row), Some(label)) => Some(row.min(label)),
            (row, label) => row.or(label),
        }
    }

    pub(crate) fn note_changes(&mut self, changes: &ChangeSummary) {
        self.changes.merge(changes.clone());
    }

    /// The page rebuilds on every update, so there is nothing to mark.
    pub(crate) fn mark_dirty(&mut self) {}

    pub(crate) fn update(
        &mut self,
        core: &Core,
        ui: SidebarUiState,
        menu_host: &MenuHost,
        hud: &std::sync::Arc<Value>,
        now_ms: u64,
    ) -> NativeSidebarSnapshot {
        self.last_inputs.ui = ui;
        let changes = std::mem::take(&mut self.changes);
        self.model.update(core, &self.last_inputs, &changes, now_ms);
        let menus = SidebarMenus::new(
            core,
            self.model.view(),
            &self.last_inputs,
            menu_host,
            now_ms,
        );
        snapshot_from_view(
            self.model.view(),
            &SnapshotInput {
                menus: &menus,
                hud,
                rename_request: None,
                reveal_request: None,
                search_shortcut: None,
                commands_shortcut: None,
                settings: &self.last_inputs.settings,
                hidden_items: &self.last_inputs.ui.hidden_items,
                host: menu_host,
                collapsed_groups: &self.last_inputs.ui.collapse.collapsed_groups,
                show_hidden: self.last_inputs.ui.show_hidden,
                selected_tag_filters: &self.last_inputs.ui.selected_tag_filters,
                now_ms,
            },
            &mut self.snapshot_cache,
        )
    }
}
