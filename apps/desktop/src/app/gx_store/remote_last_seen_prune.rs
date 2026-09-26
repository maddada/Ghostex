//! A remote machine removed from Settings loses its stored last-seen copy.
//!
//! The old runtime's writer did this on every remote publish (`captureRemoteLastSeenPresentations`
//! queued a removal for every stored machine Settings no longer lists). That writer is gone, so the
//! removal runs here, at launch and after every Settings change, which is when a machine can stop
//! being saved. A disabled machine is still saved and keeps its copy.

use std::collections::HashSet;

use ghostex_gx_core::decode_uri_component;

use super::records_storage::{remove_record, scan_record_raw};
use super::remote_last_seen::{MACHINE_KEY_PREFIX, STORE};
use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_remote_machine_id_from_value;
use crate::shared_settings;

impl GhostexGpuiApp {
    pub(crate) fn gx_store_prune_remote_last_seen(&mut self) {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        // `if (this.runtimeSettings?.settings === undefined) return`: no Settings read yet is not
        // "no machines saved".
        let Some(machines) = settings
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        let saved: HashSet<String> = machines
            .iter()
            .filter_map(gpui_remote_machine_id_from_value)
            .collect();
        let now_ms = super::host::now_ms() as i64;
        let Ok(rows) = scan_record_raw(STORE, MACHINE_KEY_PREFIX, now_ms) else {
            return;
        };
        for (key, _) in rows {
            let Some(machine_id) = key
                .strip_prefix(MACHINE_KEY_PREFIX)
                .and_then(decode_uri_component)
            else {
                continue;
            };
            if !saved.contains(&machine_id) {
                let _ = remove_record(&key);
            }
        }
    }
}
