//! What Quick Access reads from the store: every machine's groups and sessions, the HUD, the Action
//! run states and the custom tag catalogs.
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! The runtime's Quick Access read the zustand `sidebarStore`, which the runtime's publish filled
//! with its whole group projection. That projection is the store's own sidebar model now, so the
//! groups are built here from the store, one model per machine, with the rows before any tag,
//! Space or hidden filter (gx-core `quick_access_store_groups`). The HUD is the one gx_store/hud/
//! composes into `runtime_facts`, where every reader takes it.

use ghostex_gx_core::{
    ChangeSummary, HotkeyPlatformWire, MachineId, QuickAccessData, QuickAccessOpenTarget,
    QuickAccessRunState, QuickAccessStoreGroup, SidebarViewModel, quick_access_store_groups,
};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_visible_open_targets_from_current_settings;
use crate::app::model::GpuiSidebarCommandRunState;

impl GhostexGpuiApp {
    /// The store as Quick Access reads it, at `now_ms`.
    pub(crate) fn gx_store_quick_access_data(&self, now_ms: u64) -> QuickAccessData {
        let store = &self.gx_store;
        let presentation = store.core.presentation();
        let mut machines: Vec<MachineId> = vec![MachineId::Local];
        machines.extend(
            presentation
                .machines()
                .map(|(machine, _)| machine.clone())
                .filter(|machine| !machine.is_local()),
        );
        let mut groups: Vec<QuickAccessStoreGroup> = Vec::new();
        for machine in &machines {
            let mut inputs = store.sidebar_list.last_inputs.clone();
            inputs.ui.selected_machine_id = match machine.remote_id() {
                Some(id) => id.to_string(),
                None => ghostex_gx_core::LOCAL_MACHINE_ID.to_string(),
            };
            let mut model = SidebarViewModel::new();
            model.update(&store.core, &inputs, &ChangeSummary::default(), now_ms);
            groups.extend(quick_access_store_groups(&model, machine));
        }
        let local_custom_tags = presentation
            .machine(&MachineId::Local)
            .and_then(|entry| entry.side_state().custom_session_tags.clone());
        let remote_custom_tags = presentation
            .machines()
            .filter(|(machine, _)| !machine.is_local())
            .filter_map(|(_, entry)| entry.side_state().custom_session_tags.clone())
            .collect();
        let command_run_states = self
            .sidebar_command_run_feedback_states
            .iter()
            .filter_map(|(command_id, state)| {
                let status = state.status?;
                Some((
                    command_id.clone(),
                    QuickAccessRunState {
                        status: match status {
                            GpuiSidebarCommandRunState::Error => "error",
                            GpuiSidebarCommandRunState::Running => "running",
                            GpuiSidebarCommandRunState::Success => "success",
                        }
                        .to_string(),
                        active_run_ids: state.active_run_ids.clone(),
                    },
                ))
            })
            .collect();
        // The Commands tab's "Open In" rows: every visible target except Finder, which has a row of
        // its own (Open File/Folder Location).
        let open_targets = gpui_visible_open_targets_from_current_settings()
            .into_iter()
            .filter(|target| target.id != "finder")
            .map(|target| QuickAccessOpenTarget {
                custom: target.id.starts_with("custom:"),
                id: target.id,
                label: target.label,
            })
            .collect();
        QuickAccessData {
            hud: store
                .runtime_facts
                .hud()
                .map(|hud| hud.as_ref().clone())
                .unwrap_or(Value::Null),
            groups,
            command_run_states,
            local_custom_tags,
            remote_custom_tags,
            open_targets,
            platform: if cfg!(target_os = "macos") {
                HotkeyPlatformWire::Mac
            } else if cfg!(target_os = "windows") {
                HotkeyPlatformWire::Windows
            } else {
                HotkeyPlatformWire::Linux
            },
        }
    }
}
