//! The desktop half of the status indicators: one neutral sidebar view per machine, kept up to date
//! with the list's own changes, and the two payloads handed to the menu bar item and the pet
//! whenever they move.
//!
//! CDXC:StatusPet 2026-09-25 WHY:
//! The runtime rebuilt both payloads on every publish from its own projection and posted them on
//! two bridge functions (app runtime port ledger P022, P023). They are derived here from the store,
//! over the same groups in the same order, and given to the same receivers, which keep their
//! validation and their transition-based attention notifications.
//!
//! SEE-ALSO: packages/gx-core/src/indicators.rs, apps/desktop/src/app/workspace_reconcile.rs.

use std::collections::BTreeMap;

use ghostex_gx_core::indicators::{
    indicator_candidates, neutral_indicator_inputs, pet_overlay_payload, status_indicators_payload,
};
use ghostex_gx_core::{ChangeSummary, MachineId, SidebarInputs, SidebarViewModel};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::gx_store::host::now_ms;

#[derive(Default)]
pub(crate) struct IndicatorsHost {
    models: BTreeMap<MachineId, SidebarViewModel>,
    posted_status: Option<String>,
    posted_pet: Option<String>,
}

impl GhostexGpuiApp {
    /// The list moved: bring each machine's neutral view up to date and hand over what changed.
    pub(in crate::app::gx_store) fn gx_store_indicators_changed(
        &mut self,
        changes: &ChangeSummary,
        list_inputs: &SidebarInputs,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings = self.gx_store_hud_settings_value();
        let hide_menu_bar =
            settings.get("hideMenuBarSessionStatusIndicators") == Some(&Value::Bool(true));
        let pet_enabled = settings.get("petOverlayEnabled") == Some(&Value::Bool(true));
        let pet_id = settings
            .get("selectedPetId")
            .and_then(Value::as_str)
            .map(str::to_string);
        // This computer, then every saved remote machine the store holds rows for, live or last
        // seen, in the order Settings keeps them.
        let mut machines = vec![MachineId::Local];
        machines.extend(
            ghostex_gx_core::normalize_remote_machine_settings(settings.get("remoteMachines"))
                .iter()
                .filter_map(|machine| machine.get("id")?.as_str().map(str::to_string))
                .map(MachineId::Remote)
                .filter(|machine| self.gx_store.core.presentation().loaded(machine).is_some()),
        );
        let now = now_ms();
        let host = &mut self.gx_store.indicators;
        host.models.retain(|machine, _| machines.contains(machine));
        for machine in &machines {
            let inputs = neutral_indicator_inputs(list_inputs, machine);
            host.models.entry(machine.clone()).or_default().update(
                &self.gx_store.core,
                &inputs,
                changes,
                now,
            );
        }
        let models: Vec<(MachineId, &SidebarViewModel)> = machines
            .iter()
            .filter_map(|machine| Some((machine.clone(), host.models.get(machine)?)))
            .collect();
        let candidates = indicator_candidates(&self.gx_store.core, &models);
        let status = status_indicators_payload(&candidates, hide_menu_bar).to_string();
        let pet = pet_overlay_payload(&candidates, pet_enabled, pet_id.as_deref()).to_string();
        if self.gx_store.indicators.posted_status.as_deref() != Some(status.as_str()) {
            self.receive_sidebar_session_status_indicators_payload(&status, cx);
            self.gx_store.indicators.posted_status = Some(status);
        }
        if self.gx_store.indicators.posted_pet.as_deref() != Some(pet.as_str()) {
            self.receive_sidebar_pet_overlay_state_payload(&pet, cx);
            self.gx_store.indicators.posted_pet = Some(pet);
        }
    }

    /// Settings moved without a list change (the menu bar visibility, the pet): rederive.
    pub(in crate::app::gx_store) fn gx_store_indicators_settings_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let inputs = self.gx_store.sidebar_list.last_inputs.clone();
        self.gx_store_indicators_changed(&ChangeSummary::default(), &inputs, cx);
    }
}
