//! The sidebar HUD: the one document of launcher agents, Actions, Recent Projects, appearance
//! settings and view-scope options that the sidebar, its menus and the view scopes read.
//!
//! The host gathers the sources (its settings file, gxserver's `/api/readSidebarHud` and
//! `/api/listRecentProjects`, each remote machine's HUD, and the remote machines' client-parked
//! projects) and calls [`compose_sidebar_hud`]; everything that comes from the presentation (the
//! Spaces, the projects the sidebar lists, the active project) is read from the store.
//!
//! **The HUD's contract is what Rust reads.** These fields are produced, each as
//! the app runtime's `createGpuiSidebarHudState` (deleted with QuickJS; see git history)
//! built it and the sidebar store normalized it: `activeProjectId`, `activeProjectSpaceRefs`,
//! `activeSessionsSortMode`, `agentManagerZoomPercent`, `agents`, `commandsByProject`,
//! `createSessionOnSidebarDoubleClick`, `debuggingMode`, `globalCommands`, `projectViewProjects`,
//! `projectViewSpaces`, `recentProjects`, `renameSessionOnDoubleClick`, `settings` (see
//! `settings.rs`) and `theme`. The runtime's other fields (the active project's `commands`, git,
//! command-session indicators, focus title, slot labels) had no Rust reader and are not produced; a reader that needs one adds it
//! here.
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! The HUD used to reach Rust on the runtime's facts channel (`CDXC:Sidebar 2026-09-21 DECISION`,
//! which said the channel dies with QuickJS). The app runtime port (family F2) builds it here so
//! the desktop and the web build compose one HUD, and the channel's `hud` kind is gone.

mod actions;
mod recent;
mod scopes;
pub mod settings;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::core::Core;
use crate::keys::MachineId;

/// What the host supplies. Every field is what it last read; `None` for a read that has not
/// answered yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudSources {
    /// The saved settings object, as the settings file holds it.
    pub settings: Value,
    pub debugging_mode: bool,
    pub show_beta_features: bool,
    /// This computer's `/api/readSidebarHud` answer.
    pub sidebar_hud: Option<Value>,
    /// Each remote machine's `/api/readSidebarHud` answer, by machine id.
    pub remote_sidebar_huds: BTreeMap<String, Value>,
    /// This computer's `/api/listRecentProjects` rows.
    pub recent_projects: Vec<Value>,
    /// The projects parked on each remote machine by this client, in the stored order.
    pub remote_recent_projects: Vec<(String, Vec<Value>)>,
    /// The project the workspace shows: a raw id for this computer's, a machine-scoped one for a
    /// remote machine's.
    pub active_project_id: Option<String>,
}

/// The saved remote machines as `(id, name)`, in their saved order.
fn remote_machines(settings: &Value) -> Vec<(String, String)> {
    crate::sidebar_actions::normalize_remote_machine_settings(settings.get("remoteMachines"))
        .into_iter()
        .filter_map(|machine| {
            Some((
                machine.get("id")?.as_str()?.to_string(),
                machine.get("name")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

/// The projects each machine keeps parked, which the sidebar does not list.
fn parked_projects(recent_projects: &Value) -> BTreeMap<MachineId, BTreeSet<String>> {
    let mut parked: BTreeMap<MachineId, BTreeSet<String>> = BTreeMap::new();
    for row in recent_projects
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(project_id) = row.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        match row.get("remoteMachineId").and_then(Value::as_str) {
            None => {
                parked
                    .entry(MachineId::Local)
                    .or_default()
                    .insert(project_id.to_string());
            }
            Some(machine_id) => {
                if let Some(key) = crate::keys::ProjectKey::parse_workspace_project_id(project_id)
                    .filter(|key| key.machine.remote_id() == Some(machine_id))
                {
                    parked
                        .entry(key.machine)
                        .or_default()
                        .insert(key.project_id);
                }
            }
        }
    }
    parked
}

/// The HUD document.
pub fn compose_sidebar_hud(core: &Core, sources: &HudSources) -> Value {
    let store = core.presentation();
    let empty = Map::new();
    let raw_settings = sources.settings.as_object().unwrap_or(&empty);
    let settings = settings::hud_settings(
        &sources.settings,
        sources.debugging_mode,
        sources.show_beta_features,
    );
    let theme_setting = settings::sidebar_theme_setting(raw_settings);
    let theme = settings::resolved_dark_theme(theme_setting).to_string();
    let machines = remote_machines(&sources.settings);
    let machine_names: BTreeMap<String, String> = machines.iter().cloned().collect();
    let recent_projects = recent::recent_projects(
        store,
        &sources.recent_projects,
        &sources.remote_recent_projects,
        &machine_names,
        &theme,
    );
    let parked = parked_projects(&recent_projects);
    let sidebar_hud = sources.sidebar_hud.as_ref();
    // A remote machine's Actions count only while its stream is live: the runtime dropped a
    // machine's HUD with its presentation when the machine went away.
    let remote_huds: BTreeMap<String, Value> = sources
        .remote_sidebar_huds
        .iter()
        .filter(|(machine_id, _)| {
            store
                .loaded_live(&MachineId::Remote((*machine_id).clone()))
                .is_some()
        })
        .map(|(machine_id, hud)| (machine_id.clone(), hud.clone()))
        .collect();

    let mut hud = Map::new();
    if let Some(active) = &sources.active_project_id {
        hud.insert("activeProjectId".into(), Value::from(active.as_str()));
    }
    hud.insert(
        "activeProjectSpaceRefs".into(),
        scopes::active_project_space_refs(store, sources.active_project_id.as_deref()),
    );
    hud.insert("activeSessionsSortMode".into(), Value::from("lastActivity"));
    hud.insert(
        "agentManagerZoomPercent".into(),
        settings::agent_manager_zoom_percent(raw_settings),
    );
    hud.insert("agents".into(), actions::agents(sidebar_hud));
    if let Some(commands_by_project) = actions::commands_by_project(sidebar_hud, &remote_huds) {
        hud.insert("commandsByProject".into(), commands_by_project);
    }
    hud.insert(
        "createSessionOnSidebarDoubleClick".into(),
        settings["createSessionOnSidebarDoubleClick"].clone(),
    );
    hud.insert("debuggingMode".into(), Value::Bool(sources.debugging_mode));
    hud.insert(
        "globalCommands".into(),
        actions::global_commands(sidebar_hud),
    );
    hud.insert(
        "projectViewProjects".into(),
        scopes::project_view_projects(store, &machines, &parked),
    );
    hud.insert(
        "projectViewSpaces".into(),
        scopes::project_view_spaces(store, &machine_names),
    );
    hud.insert("recentProjects".into(), recent_projects);
    hud.insert(
        "renameSessionOnDoubleClick".into(),
        settings["renameSessionOnDoubleClick"].clone(),
    );
    hud.insert("settings".into(), settings);
    hud.insert("theme".into(), Value::from(theme));
    Value::Object(hud)
}
