//! The desktop half of the status indicators: one neutral sidebar view per machine, kept up to date
//! with the list's own changes, and the payload handed to the menu bar item whenever it moves.
//!
//! CDXC:StatusPet 2026-09-25 WHY:
//! The runtime rebuilt both payloads on every publish from its own projection and posted them on
//! two bridge functions (app runtime port ledger P022, P023). They are derived here from the store,
//! over the same groups in the same order, and given to the same receivers, which keep their
//! validation and their transition-based attention notifications.
//!
//! SEE-ALSO: packages/gx-core/src/indicators.rs, apps/desktop/src/app/workspace_reconcile/sidebar_payloads.rs.

use std::collections::BTreeMap;

use ghostex_gx_core::indicators::{
    indicator_candidates, indicator_machines, neutral_indicator_inputs, status_indicators_payload,
};
use ghostex_gx_core::{ChangeSummary, MachineId, SidebarInputs, SidebarViewModel};

use crate::GhostexGpuiApp;
use crate::app::gx_store::host::now_ms;

#[derive(Default)]
pub(crate) struct IndicatorsHost {
    models: BTreeMap<MachineId, SidebarViewModel>,
    posted_status: Option<String>,
}

impl GhostexGpuiApp {
    /// The list moved: bring each machine's neutral view up to date and hand over what changed.
    pub(in crate::app::gx_store) fn gx_store_indicators_changed(
        &mut self,
        changes: &ChangeSummary,
        list_inputs: &SidebarInputs,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:Settings 2026-10-29 DECISION (cleanup Q18): the Show Menu Bar Session Indicators
        // setting is gone; the macOS menu bar item is always on (remove it from the menu bar itself).
        let hide_menu_bar = false;
        // This computer, then every remote machine connected right now (user decision
        // 2026-11-03 in gx-core's `indicator_machines`): a disconnected machine's last-seen rows
        // are not counted.
        let machines: Vec<MachineId> = indicator_machines(self.gx_store.remote.tabs())
            .into_iter()
            .filter(|machine| {
                machine.is_local() || self.gx_store.core.presentation().loaded(machine).is_some()
            })
            .collect();
        let now = now_ms();
        let host = &mut self.gx_store.indicators;
        host.models.retain(|machine, _| machines.contains(machine));
        for machine in &machines {
            let inputs = neutral_indicator_inputs(list_inputs, machine);
            // Only the groups are read (`indicator_candidates`), so the list is never laid out.
            host.models
                .entry(machine.clone())
                .or_insert_with(SidebarViewModel::groups_only)
                .update(&self.gx_store.core, &inputs, changes, now);
        }
        let models: Vec<(MachineId, &SidebarViewModel)> = machines
            .iter()
            .filter_map(|machine| Some((machine.clone(), host.models.get(machine)?)))
            .collect();
        let candidates = indicator_candidates(&self.gx_store.core, &models);
        let status = status_indicators_payload(&candidates, hide_menu_bar).to_string();
        if self.gx_store.indicators.posted_status.as_deref() != Some(status.as_str()) {
            self.receive_sidebar_session_status_indicators_payload(&status, cx);
            self.gx_store.indicators.posted_status = Some(status);
        }
    }

    /// Settings moved without a list change (the menu bar visibility): rederive.
    pub(in crate::app::gx_store) fn gx_store_indicators_settings_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let inputs = self.gx_store.sidebar_list.last_inputs.clone();
        self.gx_store_indicators_changed(&ChangeSummary::default(), &inputs, cx);
    }
}
