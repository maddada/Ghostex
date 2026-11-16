//! Saving a computer this one paired with through Easy Connect: the Connect a computer window's
//! own pairing, and Connect both ways, where gxserver hands over the computer that asked
//! (`addRemoteMachine`, server/src/remote_access/connect_back.rs).
//!
//! CDXC:RemotePairing 2026-11-14 WHY: only this app writes the saved computers (`remoteMachines`
//! in its settings file; gxserver's settings routes refuse the key), so the computer a pairing
//! approved is added here, by its Easy Connect address: a computer saved before at the same
//! address is updated in place, keeping its id, icon, password and switches, instead of a second
//! tile for it.
use crate::*;
use serde_json::{Value, json};

/// The saved computer to add or update.
#[derive(Clone, Debug)]
pub(crate) struct EasyConnectMachine {
    pub(crate) name: String,
    pub(crate) easy_connect_address: String,
    pub(crate) ssh_user: String,
    pub(crate) ssh_port: u16,
}

fn new_remote_machine_id() -> String {
    format!(
        "remote-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

/// The saved computers with `machine` added, or the one at its address updated; and its id.
pub(crate) fn remote_machines_with_easy_connect_machine(
    saved: &[Value],
    machine: &EasyConnectMachine,
) -> (Vec<Value>, String) {
    let mut machines = saved.to_vec();
    let existing = machines.iter().position(|candidate| {
        candidate.get("transport").and_then(Value::as_str) == Some("easyConnect")
            && candidate.get("easyConnectAddress").and_then(Value::as_str)
                == Some(machine.easy_connect_address.as_str())
    });
    match existing {
        Some(index) => {
            let entry = &mut machines[index];
            entry["name"] = json!(machine.name);
            entry["sshUser"] = json!(machine.ssh_user);
            entry["sshPort"] = json!(machine.ssh_port);
            entry["disabled"] = json!(false);
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            (machines, id)
        }
        None => {
            let id = new_remote_machine_id();
            machines.push(json!({
                "id": id,
                "name": machine.name,
                "transport": "easyConnect",
                "easyConnectAddress": machine.easy_connect_address,
                "sshHost": "",
                "sshUser": machine.ssh_user,
                "sshPort": machine.ssh_port,
            }));
            (machines, id)
        }
    }
}

impl GhostexGpuiApp {
    /// Adds `machine` to the saved computers (or updates the one at its address) and returns its
    /// id; the reconnect ladder notices the new one and connects it
    /// (`remote_reconnect_sync_with_settings`).
    pub(crate) fn save_easy_connect_machine(
        &mut self,
        machine: &EasyConnectMachine,
        cx: &mut gpui::Context<Self>,
    ) -> String {
        let saved: Vec<Value> = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("remoteMachines")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (machines, id) = remote_machines_with_easy_connect_machine(&saved, machine);
        let machines =
            ghostex_gx_core::normalize_remote_machine_settings(Some(&Value::Array(machines)));
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({
                "patch": { "remoteMachines": machines },
                "source": "settings:remoteMachines",
                "type": "updateSettingsPatch",
            }),
            cx,
        );
        id
    }
}
