//! Applying daemon input: snapshots, deltas, side-state frames, and domain project rows.

use std::collections::BTreeMap;

use ghostex_gx_protocol::{
    PresentationDelta, PresentationSnapshot, WorkspaceProjectGroups, WorkspaceSessionGroupsState,
};
use serde_json::Value;

use super::loaded::{sort_groups, LoadedPresentation};
use super::reducers::{remove_project, remove_session, upsert_project, upsert_session};
use super::settle::{diff_loaded, settle_overlays_after_snapshot};
use super::store::{PresentationState, PresentationStore, SideStateUpdate};
use crate::change::{ChangeSummary, IgnoredReason};
use crate::connection::{ConnectionPhase, ConnectionUpdate};
use crate::keys::{MachineId, ProjectKey};

/// Where a snapshot came from, which decides whether it may be older than what is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotOrigin {
    /// The answer to a subscribe on the event stream. Authoritative: the daemon sends a full
    /// snapshot whenever the quoted revision does not match, whether the client is behind or
    /// ahead, so it always replaces what is held.
    Stream,
    /// An HTTP `readPresentationSnapshot` result. It can race the stream, so one that is older
    /// than the held revision is dropped instead of rolling newer deltas back.
    Read,
}

impl PresentationStore {
    /// Replaces a machine's presentation with a snapshot.
    ///
    /// Side-state documents are adopted only when the snapshot carries them: an older daemon that
    /// omits one must not wipe what a change frame delivered. Overlays survive, except those the
    /// snapshot shows the daemon has caught up with.
    pub fn apply_snapshot(
        &mut self,
        machine: &MachineId,
        server_id: &str,
        mut snapshot: PresentationSnapshot,
        origin: SnapshotOrigin,
    ) -> ChangeSummary {
        let entry = self.machine_mut(machine);
        if let (SnapshotOrigin::Read, PresentationState::Loaded(held)) = (origin, &entry.state) {
            if snapshot.revision < held.revision {
                return ChangeSummary::ignored(IgnoredReason::StaleRevision {
                    held: held.revision,
                    received: snapshot.revision,
                });
            }
        }

        let mut summary = ChangeSummary::default();
        let side = &mut entry.side;
        if let Some(groups) = snapshot.workspace_groups.take() {
            note_workspace_groups_change(
                machine,
                side.workspace_groups.as_ref(),
                &groups,
                &mut summary,
            );
            side.workspace_groups = Some(groups);
        }
        if let Some(collections) = snapshot.sidebar_project_collections.take() {
            summary.side_state.project_collections =
                side.project_collections.as_ref() != Some(&collections);
            side.project_collections = Some(collections);
        }
        // A workspaces change re-filters the same Spaces row and project list, so it raises the
        // Spaces change flag instead of a flag of its own.
        if let Some(workspaces) = snapshot.sidebar_workspaces.take() {
            summary.side_state.spaces |= side.workspaces.as_ref() != Some(&workspaces);
            side.workspaces = Some(workspaces);
        } else if snapshot
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| !capabilities.workspaces)
        {
            // CDXC:Workspaces 2026-10-09 WHY:
            // A daemon that says it has no workspaces (the Workspaces built-in extension turned off) drops the document a change frame delivered earlier, so the tile and the window's workspace filter go away with it. A daemon too old to send capabilities keeps it.
            summary.side_state.spaces |= side.workspaces.is_some();
            side.workspaces = None;
        }
        if let Some(spaces) = snapshot.sidebar_spaces.take() {
            summary.side_state.spaces = side.spaces.as_ref() != Some(&spaces);
            side.spaces = Some(spaces);
        }
        if let Some(enabled) = snapshot.sidebar_spaces_enabled.take() {
            summary.side_state.spaces |= side.spaces_enabled != Some(enabled);
            side.spaces_enabled = Some(enabled);
        }
        if let Some(tags) = snapshot.custom_session_tags.take() {
            summary.side_state.custom_session_tags =
                side.custom_session_tags.as_ref() != Some(&tags);
            side.custom_session_tags = Some(tags);
        }
        // gxserver sends the panels only while a sidebar extension is on, so a snapshot without
        // them means there are none.
        let panels = snapshot
            .extension_panels
            .take()
            .filter(|panels| !panels.0.is_empty());
        summary.side_state.extension_panels = side.extension_panels != panels;
        side.extension_panels = panels;

        let mut next = LoadedPresentation::from_snapshot(server_id.to_string(), snapshot);
        match &entry.state {
            PresentationState::NotLoaded => summary.machines_reloaded.push(machine.clone()),
            PresentationState::Loaded(previous) => {
                // A read does not know the daemon identity; keep the one the stream gave.
                if next.server_id.is_empty() {
                    next.server_id = previous.server_id.clone();
                }
                // The held revision becomes the snapshot's even when that is lower (a stream reply
                // from a daemon whose database was restored): keeping the higher number would make
                // every later delta look stale.
                diff_loaded(machine, previous, &next, &mut summary);
            }
        }
        if origin == SnapshotOrigin::Stream {
            entry.resubscribe_requested = false;
        }
        entry.state = PresentationState::Loaded(Box::new(next));
        settle_overlays_after_snapshot(machine, entry, &mut summary);
        summary
    }

    /// Seeds a machine the host has NOT connected to in this run from its last-seen snapshot, so
    /// its sessions are on screen, faded, until the machine connects.
    ///
    /// CDXC:RemoteMachines 2026-09-21 DECISION:
    /// Asked "today, when a remote machine is offline, the old sidebar still shows its sessions
    /// from the last time it connected, greyed out; the Rust sidebar cannot do that yet, so an
    /// offline machine shows nothing until it connects. Do you want that last-seen view kept?",
    /// the user answered "yes pls". So the copy the old sidebar persisted is read back and held
    /// here, and this is what makes it "hold these rows, not live": `last_seen` is set, so no
    /// revisioned frame may be measured against them, the daemon cannot confirm them as current,
    /// and the host must not quote their revision on a subscribe. `is_stale` in the view model
    /// already draws a loaded machine whose stream is not live as faded, so nothing about the
    /// drawing changes.
    ///
    /// Refused for a machine that is ALREADY loaded: the seed is a cold start, and a machine the
    /// host has streamed to in this run holds something newer than any stored copy.
    pub fn seed_last_seen(
        &mut self,
        machine: &MachineId,
        snapshot: PresentationSnapshot,
    ) -> ChangeSummary {
        let entry = self.machine_mut(machine);
        if matches!(entry.state, PresentationState::Loaded(_)) {
            return ChangeSummary::ignored(IgnoredReason::StaleRevision {
                held: 0,
                received: snapshot.revision,
            });
        }
        let mut summary = self.apply_snapshot(machine, "", snapshot, SnapshotOrigin::Stream);
        if let PresentationState::Loaded(loaded) = &mut self.machine_mut(machine).state {
            loaded.last_seen = true;
        }
        // A seeded machine has never been subscribed to, so nothing may resume from its revision.
        summary.machines_reloaded.retain(|held| held != machine);
        summary.machines_reloaded.push(machine.clone());
        summary
    }

    /// The daemon confirmed that the quoted `lastRevision` is current: nothing to apply.
    ///
    /// Returns `NotLoaded` when the store holds nothing for the machine, which means the host
    /// quoted a revision the store never had; the caller must resubscribe without `lastRevision`.
    pub fn apply_snapshot_current(
        &mut self,
        machine: &MachineId,
        server_id: &str,
        revision: i64,
    ) -> ChangeSummary {
        match &mut self.machine_mut(machine).state {
            PresentationState::NotLoaded => ChangeSummary::ignored(IgnoredReason::NotLoaded),
            // Rows held from a previous run were never told to this client, so nothing can be
            // "current" with them. Answered as `NotLoaded` rather than as a stale revision,
            // because that is the outcome the caller already handles by resubscribing without
            // `lastRevision`, which is exactly what these rows need.
            PresentationState::Loaded(loaded) if loaded.last_seen => {
                ChangeSummary::ignored(IgnoredReason::NotLoaded)
            }
            PresentationState::Loaded(loaded) => {
                // "Current" from another daemon says nothing about the rows held from this one.
                if !server_id.is_empty()
                    && !loaded.server_id.is_empty()
                    && server_id != loaded.server_id
                {
                    return ChangeSummary::ignored(IgnoredReason::ServerChanged);
                }
                if loaded.server_id.is_empty() {
                    loaded.server_id = server_id.to_string();
                }
                loaded.revision = loaded.revision.max(revision);
                ChangeSummary::default()
            }
        }
    }

    /// Applies one delta.
    ///
    /// Revision rules: a delta before the first snapshot, or at or below the held revision, is
    /// dropped without complaint (the subscribe handler itself can enqueue such deltas right
    /// before the snapshot). A gap is never loss, because side-channel frames consume revisions
    /// from the same counter; loss is signalled only by the socket closing. Sessions and projects
    /// are whole-object replacements, never merges, because present-only keys clear by absence.
    pub fn apply_delta(
        &mut self,
        machine: &MachineId,
        server_id: &str,
        revision: i64,
        delta: PresentationDelta,
    ) -> ChangeSummary {
        let entry = self.machine_mut(machine);
        let loaded = match &mut entry.state {
            PresentationState::NotLoaded => {
                return ChangeSummary::ignored(IgnoredReason::NotLoaded)
            }
            PresentationState::Loaded(loaded) => loaded,
        };
        if let Some(reason) = frame_rejection(loaded, server_id, revision) {
            return ChangeSummary::ignored(reason);
        }
        loaded.revision = revision;

        let mut summary = ChangeSummary::default();
        match delta {
            PresentationDelta::SessionPresentationChanged { session } => {
                upsert_session(machine, loaded, &mut entry.overlays, *session, &mut summary);
            }
            PresentationDelta::SessionRemoved {
                project_id,
                session_id,
            } => {
                remove_session(
                    machine,
                    loaded,
                    &mut entry.overlays,
                    &project_id,
                    &session_id,
                    &mut summary,
                );
            }
            PresentationDelta::ProjectAdded {
                project,
                domain_project,
            }
            | PresentationDelta::ProjectUpdated {
                project,
                domain_project,
            } => {
                if let Some(domain_project) = domain_project {
                    entry
                        .domain_projects
                        .insert(project.project_id.clone(), domain_project);
                }
                upsert_project(machine, loaded, *project, &mut summary);
            }
            PresentationDelta::ProjectRemoved { project_id } => {
                entry.domain_projects.remove(&project_id);
                remove_project(
                    machine,
                    loaded,
                    &mut entry.overlays,
                    &project_id,
                    &mut summary,
                );
            }
            PresentationDelta::GroupUpserted { group } => {
                let project = ProjectKey {
                    machine: machine.clone(),
                    project_id: group.project_id.clone(),
                };
                match loaded
                    .groups
                    .iter_mut()
                    .find(|existing| existing.group_id == group.group_id)
                {
                    Some(existing) => *existing = group,
                    None => loaded.groups.push(group),
                }
                sort_groups(&mut loaded.groups);
                summary.note_session_order_changed(project);
            }
            PresentationDelta::GroupRemoved {
                project_id,
                group_id,
            } => {
                loaded.groups.retain(|group| group.group_id != group_id);
                let doomed: Vec<String> = loaded
                    .project_sessions(&project_id)
                    .map(|sessions| {
                        sessions
                            .values()
                            .filter(|session| session.group_id == group_id)
                            .map(|session| session.session_id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                for session_id in doomed {
                    remove_session(
                        machine,
                        loaded,
                        &mut entry.overlays,
                        &project_id,
                        &session_id,
                        &mut summary,
                    );
                }
                summary.note_session_order_changed(ProjectKey {
                    machine: machine.clone(),
                    project_id,
                });
            }
            // An unknown delta type advances the revision and changes nothing.
            PresentationDelta::Unknown { .. } => {}
        }
        summary
    }

    /// Replaces one side-state document.
    ///
    /// CDXC:StateSync 2026-09-19 WHY:
    /// Side-channel frames allocate from the presentation revision counter, so one at or below the held revision is already contained in the held snapshot and is dropped, and an applied one advances the held revision. That keeps `lastRevision` current across side-state changes, which the TypeScript runtime did not do (it then always got a full snapshot on the next reconnect). A frame without a revision (a daemon of another version) is applied. Before the first snapshot the document is kept, because an older daemon's snapshot may not carry it.
    pub fn apply_side_state(
        &mut self,
        machine: &MachineId,
        server_id: &str,
        revision: Option<i64>,
        update: SideStateUpdate,
    ) -> ChangeSummary {
        let entry = self.machine_mut(machine);
        if let (Some(revision), PresentationState::Loaded(loaded)) = (revision, &mut entry.state) {
            if let Some(reason) = frame_rejection(loaded, server_id, revision) {
                return ChangeSummary::ignored(reason);
            }
            loaded.revision = revision;
        }
        let mut summary = ChangeSummary::default();
        let side = &mut entry.side;
        match update {
            SideStateUpdate::WorkspaceGroups(state) => {
                note_workspace_groups_change(
                    machine,
                    side.workspace_groups.as_ref(),
                    &state,
                    &mut summary,
                );
                side.workspace_groups = Some(state);
            }
            SideStateUpdate::ProjectCollections(state) => {
                summary.side_state.project_collections =
                    side.project_collections.as_ref() != Some(&state);
                side.project_collections = Some(state);
            }
            SideStateUpdate::Spaces(state) => {
                summary.side_state.spaces = side.spaces.as_ref() != Some(&state);
                side.spaces = Some(state);
            }
            SideStateUpdate::CustomSessionTags(state) => {
                summary.side_state.custom_session_tags =
                    side.custom_session_tags.as_ref() != Some(&state);
                side.custom_session_tags = Some(state);
            }
            SideStateUpdate::Workspaces(state) => {
                summary.side_state.spaces = side.workspaces.as_ref() != Some(&state);
                side.workspaces = Some(state);
            }
            SideStateUpdate::ExtensionPanels(state) => {
                let state = (!state.0.is_empty()).then_some(state);
                summary.side_state.extension_panels = side.extension_panels != state;
                side.extension_panels = state;
            }
        }
        summary
    }

    /// Records the daemon's own Spaces switch from a Spaces frame the store already accepted.
    /// Reports the change as a Spaces change, which is what every reader of it rebuilds on.
    pub fn apply_spaces_enabled(&mut self, machine: &MachineId, enabled: bool) -> bool {
        let side = &mut self.machine_mut(machine).side;
        let changed = side.spaces_enabled != Some(enabled);
        side.spaces_enabled = Some(enabled);
        changed
    }

    /// Notes a frame that carries a revision but no state the store holds (for example
    /// `globalSidebarCommandsChanged`), so the held revision stays current.
    pub fn note_revision(&mut self, machine: &MachineId, server_id: &str, revision: i64) {
        if let PresentationState::Loaded(loaded) = &mut self.machine_mut(machine).state {
            if frame_rejection(loaded, server_id, revision).is_none() {
                loaded.revision = revision;
            }
        }
    }

    /// Replaces the full list of domain project rows (from `listProjects`).
    pub fn set_domain_projects(
        &mut self,
        machine: &MachineId,
        projects: Vec<Value>,
    ) -> ChangeSummary {
        let entry = self.machine_mut(machine);
        let mut next = BTreeMap::new();
        for project in projects {
            if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
                next.insert(project_id.to_string(), project);
            }
        }
        let changed: Vec<String> = next
            .keys()
            .chain(entry.domain_projects.keys())
            .filter(|project_id| next.get(*project_id) != entry.domain_projects.get(*project_id))
            .cloned()
            .collect();
        entry.domain_projects = next;

        let mut summary = ChangeSummary::default();
        for project_id in changed {
            summary.note_project_changed(ProjectKey {
                machine: machine.clone(),
                project_id,
            });
        }
        summary
    }

    /// Records what the socket client reports about a machine's event stream.
    pub fn apply_connection(
        &mut self,
        machine: &MachineId,
        update: ConnectionUpdate,
        now_ms: u64,
    ) -> ChangeSummary {
        let entry = self.machine_mut(machine);
        let mut next = entry.connection.clone();
        match update {
            ConnectionUpdate::Connecting { attempt } => {
                next.phase = ConnectionPhase::Connecting;
                next.attempt = attempt;
            }
            ConnectionUpdate::Live => {
                next.phase = ConnectionPhase::Live;
                next.attempt = 0;
                next.last_error = None;
            }
            ConnectionUpdate::Lost { error } => {
                next.phase = if entry.is_loaded() {
                    ConnectionPhase::Stale
                } else {
                    ConnectionPhase::Disconnected
                };
                next.last_error = error;
            }
        }
        // A new socket starts a new subscribe, so an earlier resubscribe request is settled.
        entry.resubscribe_requested = false;
        let mut summary = ChangeSummary::default();
        if next.phase != entry.connection.phase
            || next.last_error != entry.connection.last_error
            || next.attempt != entry.connection.attempt
        {
            if next.phase != entry.connection.phase {
                next.changed_at_ms = now_ms;
            }
            entry.connection = next;
            summary.note_connection_changed(machine.clone());
        }
        summary
    }

    /// Marks that the core asked the host to resubscribe. Returns `false` when a request is
    /// already outstanding, so one daemon change yields one request, not one per ignored frame.
    pub fn begin_resubscribe(&mut self, machine: &MachineId) -> bool {
        let entry = self.machine_mut(machine);
        !std::mem::replace(&mut entry.resubscribe_requested, true)
    }

    /// Drops a machine's daemon state (a removed or disconnected machine). Overlays go with it.
    pub fn unload_machine(&mut self, machine: &MachineId) -> ChangeSummary {
        let mut summary = ChangeSummary::default();
        if let Some(entry) = self.machines.remove(machine) {
            if entry.is_loaded() {
                summary.machines_reloaded.push(machine.clone());
            }
        }
        summary
    }
}

/// Why a revisioned frame must not be applied to `loaded`, if any.
pub(super) fn frame_rejection(
    loaded: &LoadedPresentation,
    server_id: &str,
    revision: i64,
) -> Option<IgnoredReason> {
    // Rows held from a previous run are not a revision this client has been told, so no revisioned
    // frame may be measured against them or applied on top of them. Only a full stream snapshot
    // replaces them (`last_seen` on `LoadedPresentation`).
    if loaded.last_seen {
        return Some(IgnoredReason::StaleRevision {
            held: loaded.revision,
            received: revision,
        });
    }
    if !server_id.is_empty() && !loaded.server_id.is_empty() && server_id != loaded.server_id {
        return Some(IgnoredReason::ServerChanged);
    }
    (revision <= loaded.revision).then_some(IgnoredReason::StaleRevision {
        held: loaded.revision,
        received: revision,
    })
}

/// Reports what a new workspace-groups document moves.
///
/// User-made groups take sessions out of their project's own tab list, so a document that changes
/// a project's groups changes that project's tab lists even though no session row changed. Only
/// the local machine's document holds user-made groups (keyed by workspace project id, local and
/// remote); every machine's document orders that machine's projects.
fn note_workspace_groups_change(
    machine: &MachineId,
    previous: Option<&WorkspaceSessionGroupsState>,
    next: &WorkspaceSessionGroupsState,
    summary: &mut ChangeSummary,
) {
    if previous == Some(next) {
        return;
    }
    summary.side_state.workspace_groups = true;
    if previous.map(|state| &state.project_order) != Some(&next.project_order) {
        summary.note_project_order_changed(machine.clone());
    }
    if !machine.is_local() {
        return;
    }
    let empty = BTreeMap::new();
    let before = previous.map_or(&empty, |state| &state.projects);
    for workspace_project_id in before.keys().chain(next.projects.keys()) {
        // Membership and order only: renaming a user-made group moves no tab.
        if group_members(before, workspace_project_id)
            != group_members(&next.projects, workspace_project_id)
        {
            if let Some(project) = ProjectKey::parse_workspace_project_id(workspace_project_id) {
                summary.note_session_order_changed(project);
            }
        }
    }
}

/// The `(group id, member ids)` list of one project in a workspace-groups document.
fn group_members<'a>(
    projects: &'a BTreeMap<String, WorkspaceProjectGroups>,
    workspace_project_id: &str,
) -> Option<Vec<(&'a String, &'a Vec<String>)>> {
    projects.get(workspace_project_id).map(|entry| {
        entry
            .groups
            .iter()
            .map(|group| (&group.group_id, &group.session_ids))
            .collect()
    })
}
