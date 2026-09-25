//! The inputs the Rust sidebar list is built from besides the store.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! Three kinds of value meet here and the file keeps them apart on purpose. The sidebar's own
//! state (collapse, Space, filters, hidden items, selection) is the Rust store's, owned since
//! M4b. The stored collections, the machine tabs and the unavailable clock are this app's own
//! facts. The rest are taken from the RUNTIME's one-way facts channel (`runtime_facts.rs`),
//! because their real source has not moved into Rust yet: the Recent Projects come from the
//! daemon's sidebar HUD, the git numbers from the old runtime's background probe, and the Close
//! After Done and Delayed Send timers from the runtime that owns them. Each of those is handed to
//! M5 with the HUD and the session lifecycle; until then they are mirrored here and nowhere else,
//! so there is one list of what is still borrowed. This used to read the old projection's newest
//! publish; the channel replaced it in M4d part 2 step 3.

use ghostex_gx_core::{
    MachineTabInput, SessionSortMode, SidebarHostInputs, SidebarInputs, SidebarSettings,
    SidebarUiState, UnavailableState,
};
use serde_json::Value;

use super::runtime_facts::SidebarRuntimeFacts;

/// The one sort mode any desktop HUD can have.
///
/// CDXC:Sessions 2026-09-21 WHY:
/// Both HUD builders pin `activeSessionsSortMode: 'lastActivity'` (gx-core `compose_sidebar_hud` in
/// hud/mod.rs and the hydrate in
/// app/helpers/sidebar/settings_messages_and_width.rs) and no desktop handler answers
/// `setActiveSessionsSortMode`, so Manual Sorting ends in a no-op (gx-core `sidebar_actions/sort.rs`).
/// Reading it off the HUD document per update was a walk that could only ever produce this.
pub(super) const DESKTOP_SORT_MODE: SessionSortMode = SessionSortMode::LastActivity;

/// What the assembled inputs were built from, so an update that changes none of it does no work.
#[derive(Default)]
pub(super) struct InputsCache {
    /// Identity of the sidebar's own state: bumped by every intent and by the restore.
    ui_generation: u64,
    /// The channel generations the mirrored values were taken from.
    hud_generation: Option<u64>,
    rows_generation: Option<u64>,
    /// The machine tabs as the last refresh saw them.
    machines: Option<Vec<MachineTabInput>>,
    stored_collections: Option<Option<Value>>,
    settings: Option<SidebarSettings>,
    unavailable: Option<UnavailableState>,
}

/// Brings the assembled inputs up to date in place. Each part is rebuilt only when its own source
/// moved, so a pump that changed one session does not re-read the whole HUD.
#[allow(clippy::too_many_arguments)]
pub(super) fn refresh_inputs(
    inputs: &mut SidebarInputs,
    cache: &mut InputsCache,
    ui: &SidebarUiState,
    ui_generation: u64,
    settings: SidebarSettings,
    facts: &SidebarRuntimeFacts,
    stored_project_collections: &Option<Value>,
    unavailable: UnavailableState,
    machines: &[MachineTabInput],
) {
    if cache.ui_generation != ui_generation || cache.settings.is_none() {
        cache.ui_generation = ui_generation;
        inputs.ui = ui.clone();
    }
    if cache.settings.as_ref() != Some(&settings) {
        cache.settings = Some(settings.clone());
        inputs.settings = settings;
    }
    if cache.stored_collections.as_ref() != Some(stored_project_collections) {
        cache.stored_collections = Some(stored_project_collections.clone());
        inputs.host.stored_project_collections = stored_project_collections.clone();
    }
    if cache.unavailable != Some(unavailable) {
        cache.unavailable = Some(unavailable);
        inputs.host.unavailable = unavailable;
    }
    if cache.machines.as_deref() != Some(machines) {
        cache.machines = Some(machines.to_vec());
        inputs.host.machines = machines.to_vec();
    }
    if cache.hud_generation != Some(facts.hud_generation) {
        cache.hud_generation = Some(facts.hud_generation);
        refresh_recent_projects(&mut inputs.host, facts.hud.as_deref());
    }
    if cache.rows_generation != Some(facts.rows_generation) {
        cache.rows_generation = Some(facts.rows_generation);
        refresh_row_facts(&mut inputs.host, facts);
    }
}

/// The parked projects, from the HUD the runtime posts.
fn refresh_recent_projects(host: &mut SidebarHostInputs, hud: Option<&Value>) {
    let recent_projects = hud
        .and_then(|hud| hud.get("recentProjects"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // The HUD hides a parked project by its own id; a remote machine's entry carries a
    // machine-scoped one and belongs to that machine's section.
    host.recent_project_ids = recent_projects
        .iter()
        .filter(|project| project.get("remoteMachineId").is_none())
        .filter_map(|project| project.get("projectId").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    // A remote machine's parked projects are kept per machine: a project id is unique per daemon
    // only, so one flat set would hide a local project whose id a remote machine also handed out.
    //
    // CDXC:RemoteMachines 2026-09-20 WHY:
    // The `projectId` of a remote Recent Project is the MACHINE-SCOPED id
    // (`createGpuiRemotePresentationProjectId`, helpers/recent-projects.ts), and the raw daemon id
    // the store keys its rows by is nowhere else in the row, so it has to be parsed back out.
    // Inserting the id as published put strings of the form `remote:<m>:project:<raw>` into a set
    // that both readers compare against RAW ids (`build_project_meta`, `machine_tab_summary`), so
    // the set matched nothing and every project the user had parked on a remote machine came back
    // as a sidebar group the moment that machine connected.
    host.remote_recent_project_ids.clear();
    for project in recent_projects {
        let Some(machine_id) = project.get("remoteMachineId").and_then(Value::as_str) else {
            continue;
        };
        let parsed = project
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(ghostex_gx_core::ProjectKey::parse_workspace_project_id);
        // An entry whose id does not name this machine is not this machine's to hide.
        let Some(project) = parsed.filter(|key| key.machine.remote_id() == Some(machine_id)) else {
            continue;
        };
        host.remote_recent_project_ids
            .entry(machine_id.to_string())
            .or_default()
            .insert(project.project_id);
    }
    host.recent_project_count = recent_projects.len();
}

/// The per-row facts this app computes: a project's git numbers and this app's own Delayed Sends.
/// A row's Close After Done comes from gxserver's presentation (gx-core `CloseAfterDoneInput::from_session`).
///
/// CDXC:Sidebar 2026-09-21 WHY:
/// The two maps are taken whole rather than filtered to the drawn rows. The channel keys them
/// the way the view model looks them up (a project id, a sidebar session id), every reader asks
/// per row it draws, and the channel's sets are supersets of the publish's, which only ever held
/// the rows that survived the machine filter, the Space and the tag filters. A Delayed Send the
/// channel does not carry is the DAEMON's, and the view model takes that one off the presentation
/// itself (gx-core `rows::delayed_send`).
fn refresh_row_facts(host: &mut SidebarHostInputs, facts: &SidebarRuntimeFacts) {
    host.project_diff_stats = facts
        .project_diff_stats
        .iter()
        .map(|(project_id, stats)| (project_id.clone(), *stats))
        .collect();
    host.local_delayed_sends = facts
        .delayed_sends
        .iter()
        .map(|(session_id, delayed)| (session_id.clone(), delayed.clone()))
        .collect();
}
