//! Every computer's short name for the sidebar's pills, by machine id, from the saved settings
//! (CDXC:RemoteMachines 2026-11-06 in gx-core machine_short_names.rs). The machine tabs carry it
//! (remote_clients.rs) and the Unified sidebar reads it (unified_sidebar.rs).

/// This computer's own name (host_name.rs).
pub(crate) fn local_computer_name() -> String {
    crate::host_name::local_computer_name()
}

/// Every computer's short name by machine id (`local` for this one), from the saved settings
/// (gx-core `short_names_by_machine_id`).
pub(crate) fn machine_short_names_by_id() -> std::collections::HashMap<String, String> {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    ghostex_gx_core::machine_short_names::short_names_by_machine_id(
        &local_computer_name(),
        settings.object(),
    )
}
