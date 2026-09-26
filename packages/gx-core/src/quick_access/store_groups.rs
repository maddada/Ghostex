//! The sidebar's groups and sessions as Quick Access reads them, from the store's own sidebar
//! model rather than from the old runtime's zustand store.
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! The runtime fed Quick Access its whole group projection (`sessionIdsByGroup`): every machine,
//! the Chats group, hidden projects and user-made groups, before any tag or Space filter. The
//! sidebar model builds exactly those groups for the machine it is asked about
//! ([`SidebarViewModel::built_groups`]), so a host builds one per machine and hands the rows here.
//! A user-made group carries no project, as the runtime's `spliceWorkspaceSubgroups` cleared it.

use super::data::{QuickAccessSession, QuickAccessStoreGroup};
use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_view::SidebarViewModel;

/// One machine's groups, in the model's order.
pub fn quick_access_store_groups(
    model: &SidebarViewModel,
    machine: &MachineId,
) -> Vec<QuickAccessStoreGroup> {
    model
        .built_groups()
        .into_iter()
        .map(|(core, rows)| QuickAccessStoreGroup {
            group_id: core.group_id.clone(),
            title: core.title.clone(),
            editor_project_id: core.project_context.as_ref().map(|project| {
                ProjectKey {
                    machine: machine.clone(),
                    project_id: project.project_id.clone(),
                }
                .to_workspace_project_id()
            }),
            remote_machine_id: core
                .remote_machine
                .as_ref()
                .map(|remote| remote.machine_id.clone()),
            remote_project_id: core
                .remote_machine
                .as_ref()
                .and_then(|remote| remote.project_id.clone()),
            sessions: rows
                .into_iter()
                .map(|row| QuickAccessSession {
                    session_id: row.sidebar_session_id.clone(),
                    last_interaction_at: row.last_interaction_at.clone(),
                    alias: row.alias.clone(),
                    display_title: row.menu_facts.raw_display_title.clone(),
                    primary_title: row.menu_facts.primary_title.clone(),
                    terminal_title: row.menu_facts.terminal_title.clone(),
                    detail: row.menu_facts.detail.clone(),
                    session_number: None,
                    session_tag: row.session_tag.clone(),
                    is_favorite: row.is_favorite,
                    favicon_data_url: row.favicon_data_url.clone(),
                    agent_icon: row.agent_icon.clone(),
                    session_kind: row.session_kind.clone(),
                    lifecycle_state: Some(row.lifecycle_state.clone()),
                    agent_session_id: row.menu_facts.agent_session_id.clone(),
                    session_routing_id: row.menu_facts.session_routing_id.clone(),
                })
                .collect(),
        })
        .collect()
}
