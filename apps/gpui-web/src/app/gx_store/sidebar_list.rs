//! The drawn list, with the desktop's names (`view`, `last_inputs`, `note_changes`, `mark_dirty`) so the desktop's executor files read and move it unchanged. The desktop's `sidebar_list.rs` also paces installs, carries timer deadlines and keeps the old runtime's facts in step; the page rebuilds on every change and on the sidebar clock.
use ghostex_gx_core::{
    ChangeSummary, Core, MenuHost, SidebarInputs, SidebarMenus, SidebarSettings, SidebarUiState,
    SidebarView, SidebarViewModel,
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
    /// The `?settings=` content hash the inputs were last built from.
    settings_hash: u64,
    /// What the newest update applied, which the Unified sidebar applies next (`unified_sidebar.rs`).
    pub(crate) last_changes: ChangeSummary,
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

    /// The same when `closing_too` are going with it.
    pub(crate) fn close_projects_successor_session_id(
        &self,
        group_id: &str,
        closing_too: &[String],
    ) -> Option<String> {
        self.model
            .close_projects_successor_session_id(group_id, closing_too)
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

    /// The page has no settings file and caches no settings read, so there is nothing to forget.
    pub(crate) fn forget_settings(&mut self) {}

    pub(crate) fn update(
        &mut self,
        core: &Core,
        ui: SidebarUiState,
        menu_host: &MenuHost,
        hud: &std::sync::Arc<Value>,
        now_ms: u64,
    ) -> NativeSidebarSnapshot {
        self.last_inputs.ui = ui;
        // `?settings=` is the page's only settings source (`lib.rs`); without it the rows keep the defaults.
        let saved = crate::shared_settings::shared_sidebar_settings_snapshot();
        if saved.revision() > 0 && saved.content_hash() != self.settings_hash {
            self.settings_hash = saved.content_hash();
            self.last_inputs.settings = SidebarSettings::from_settings_json(
                &Value::Object(saved.object().clone()),
                self.last_inputs.settings.sort_mode,
            );
        }
        // The page's own machine is the gxserver that serves it and the page has no settings file,
        // so this machine's Spaces switch is the one that daemon publishes (CDXC:Spaces 2026-10-06).
        self.last_inputs.settings.sidebar_spaces_enabled = core
            .presentation()
            .machine(&ghostex_gx_core::MachineId::Local)
            .and_then(|machine| machine.side_state().spaces_enabled)
            .unwrap_or(false);
        // Session notes the same way: the computer's handshake says whether it is on.
        self.last_inputs.settings.session_notes_enabled =
            crate::app::session_notes::session_notes_on();
        // How long this computer's Ghostex service has been away, which the empty list's waiting
        // copy reads, as the desktop's `note_machine_state` tracks it.
        let loaded = core
            .presentation()
            .machine(&ghostex_gx_core::MachineId::Local)
            .is_some_and(|machine| machine.loaded().is_some());
        let unavailable = &mut self.last_inputs.host.unavailable;
        if loaded {
            unavailable.since_ms = None;
            unavailable.observed_available = true;
        } else if unavailable.since_ms.is_none() {
            unavailable.since_ms = Some(now_ms);
        }
        let changes = std::mem::take(&mut self.changes);
        self.model.update(core, &self.last_inputs, &changes, now_ms);
        self.last_changes = changes;
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
