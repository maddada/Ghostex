//! A machine tab's Hide Machine (`machineAction: hide`), which sets `disabled: true` on that
//! machine in Settings.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! Followed to its last function, the controller's `machineAction` arm posts
//! `{ type: 'updateSettingsPatch', baseRevision, source: 'settings:remoteMachines', patch:
//! { remoteMachines } }` to the runtime for EVERY action that is not `configure` (the menu sends
//! `hide`). The runtime's `saveSidebarSettingsPatch` forwards it untouched as
//! `postAppModalHostMessage({ type: 'sidebarCommand', message })`, and the bridge's
//! `sidebarCommand` arm hands it to `handle_gpui_app_modal_update_settings_patch_message`, which
//! merges `patch` over the saved settings and saves with `source`. That function is the end, and
//! the host calls it directly with this message.
//!
//! **`baseRevision` is not carried.** It is the sidebar store's revision, which nothing here holds,
//! and the end function never reads it: it merges over the settings as saved at that moment, which
//! is what the store reads too. Declared difference 42.
//!
//! **The list is normalized first** (`remote_machine_settings.rs`), because the TypeScript maps
//! over the normalized list its HUD holds and the merge writes whatever list arrives. The machine
//! is matched by id against that list, so an id normalization renumbered is not the one the tab
//! carried and nothing is hidden, on both sides.
//!
//! SEE-ALSO: the deleted sidebar page's `controller.ts` (the `machineAction` arm),
//! apps/desktop/sidebar/gxserver-runtime/app-shot-and-misc.ts (`saveSidebarSettingsPatch`, deleted
//! 2026-09-25 once nothing called it),
//! apps/desktop/src/app/remote_conn/settings_and_install_probe.rs,
//! apps/desktop/src/app/gx_store/sidebar_state_actions.rs. (The parity gate,
//! `tooling/gx-core/state-action-parity.ts`, was deleted with the TypeScript.)

use serde_json::{json, Value};

use super::plan::{ActionEffect, SidebarActionPlan};
use super::remote_machine_settings::normalize_remote_machine_settings;
use super::resolve::text_field;

/// `source` of the patch, `SettingsUpdateSource`'s remote-machines value.
pub const MACHINE_DISABLE_SETTINGS_SOURCE: &str = "settings:remoteMachines";

/// Whether this renderer command is a machine tab action the store answers here: every
/// `machineAction` that is not Configure, which is the TypeScript's `else`.
pub fn owns_machine_disable_command(command: &Value) -> bool {
    text_field(command, "type") == Some("machineAction")
        && text_field(command, "action") != Some("configure")
}

/// The patch, given the saved settings' `remoteMachines` value as it is stored.
pub fn plan_machine_disable(
    command: &Value,
    saved_remote_machines: Option<&Value>,
) -> Option<SidebarActionPlan> {
    if !owns_machine_disable_command(command) {
        return None;
    }
    // `machine.id === command.machineId`: a missing id matches nothing and the list is posted as
    // it is, which the TypeScript does too.
    let machine_id = command.get("machineId");
    let machines: Vec<Value> = normalize_remote_machine_settings(saved_remote_machines)
        .into_iter()
        .map(|mut machine| {
            if machine_id.is_some_and(|machine_id| machine.get("id") == Some(machine_id)) {
                machine["disabled"] = Value::Bool(true);
            }
            machine
        })
        .collect();
    Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
        message: json!({
            "type": "updateSettingsPatch",
            "source": MACHINE_DISABLE_SETTINGS_SOURCE,
            "patch": { "remoteMachines": machines },
        }),
    }))
}
