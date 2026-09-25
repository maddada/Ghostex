//! The Rust store (`ghostex-gx-core`) running in the browser. `host.rs` is this build's host: it owns the core, pumps the daemon's frames into it and installs the list. The symlinked files are the desktop's own executor files (`sidebar_snapshot.rs`, `sidebar_actions.rs`, `sidebar_modals.rs`, `terminal_lifecycle/`), which call gxserver only through `gx_rpc`; `sidebar_list.rs` and `diagnostics.rs` give them the desktop's names for the page's list and its (absent) record lines. The desktop files that drive its pane workspace are left out on purpose.
// The desktop's executor files are compiled here for the paths a page takes; their remote-machine, board and pane paths stay unused.
#![allow(dead_code, unused_imports)]
mod client_document;
mod collection_menu;
mod create;
mod custom_tags_sync;
mod diagnostics;
/// Items of desktop store files the page does not compile whole (`extracted-items.txt`).
#[allow(dead_code, unused_imports)]
mod gx_store_lifted {
    use crate::app::helpers::*;
    use crate::app::model::*;
    use crate::*;
    use ghostex_gx_core::PreferredInterfaceSettings;
    include!(concat!(env!("OUT_DIR"), "/gx_store_lifted.rs"));
}
pub(crate) mod git;
pub(crate) mod host;
mod hud;
mod rpc;
mod rpc_types;
mod runtime_facts;
mod primary_launcher;
mod project_docs;
mod quick_access_data;
/// The desktop's readers of the HUD's agent and Saved Action lists for the menus (`sidebar_menus.rs`).
#[allow(dead_code)]
mod menu_host_lifted {
    use ghostex_gx_core::{HeaderCommand, LauncherAgent};
    use serde_json::Value;
    use std::collections::BTreeMap;
    include!(concat!(env!("OUT_DIR"), "/menu_host_lifted.rs"));

    pub(crate) fn launcher_agents(value: &Value) -> Vec<LauncherAgent> {
        agents(value)
    }

    pub(crate) fn header_commands(value: &Value) -> Vec<HeaderCommand> {
        commands(value)
    }

    pub(crate) fn header_commands_by_project(value: &Value) -> BTreeMap<String, Vec<HeaderCommand>> {
        commands_by_project(value)
    }
}
mod sidebar_actions;
mod sidebar_bulk;
mod sidebar_clock;
mod sidebar_close_project;
mod sidebar_drag;
mod sidebar_drop_queue;
mod sidebar_flags;
mod sidebar_lifecycle;
mod sidebar_list;
mod sidebar_modals;
mod sidebar_open;
mod sidebar_reload;
mod sidebar_snooze;
mod sidebar_state_actions;
mod sidebar_ui_storage;
mod sidebar_snapshot;
pub(crate) mod terminal_lifecycle;
mod web_commands;
mod workspace_groups;
pub(crate) mod web_transport;

pub(crate) use host::GxStoreHost;
#[allow(unused_imports)]
pub(crate) use primary_launcher::read_primary_agent_launcher_id;
#[allow(unused_imports)]
pub(crate) use rpc::{gx_rpc, gx_rpc_with_timeout};
#[allow(unused_imports)]
pub(crate) use rpc_types::GxRpcError;
#[allow(unused_imports)]
pub(crate) use sidebar_ui_storage::{read_preference_value, write_client_document_value};
