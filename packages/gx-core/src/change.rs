//! What an apply call changed, so a UI can repaint narrowly.

use serde::{Deserialize, Serialize};

use crate::keys::{MachineId, ProjectKey, SessionKey};

/// Why an input changed nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum IgnoredReason {
    /// A delta arrived before any snapshot for that machine. Expected: the subscribe handler can
    /// enqueue repair deltas ahead of the snapshot.
    NotLoaded,
    /// The frame's revision is at or below the held revision. Expected, not an error.
    StaleRevision { held: i64, received: i64 },
    /// The frame names another daemon than the loaded snapshot; the host must resubscribe.
    ServerChanged,
    /// An external focus update older than the newest local intent.
    OlderThanLocalIntent {
        local_stamp: u64,
        observed_stamp: u64,
    },
    /// The intent named something the store does not hold.
    UnknownTarget,
    /// The frame carries another `protocolVersion` than this client speaks.
    ProtocolMismatch { received: u64 },
    /// A frame type the core has no state for yet (chat frames until the chat milestone, renderer
    /// commands, lifecycle notices).
    NotOwnedYet,
}

/// Which side-state documents a call replaced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideStateChanges {
    pub workspace_groups: bool,
    pub project_collections: bool,
    pub spaces: bool,
    pub custom_session_tags: bool,
}

impl SideStateChanges {
    pub fn any(&self) -> bool {
        self.workspace_groups || self.project_collections || self.spaces || self.custom_session_tags
    }
}

/// The result of one apply call. Lists hold each key once, in the order the changes were found.
///
/// "Changed" means the effective value (server state with local overlays applied) may differ
/// from before. A removed key is only in the `removed` list.
///
/// Build one with `ChangeSummary::default()`; fold several with [`ChangeSummary::merge`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ChangeSummary {
    /// Set when the input was dropped; every other field is then empty.
    pub ignored: Option<IgnoredReason>,
    /// A machine went from not loaded to loaded (or back): everything about it must be redrawn.
    pub machines_reloaded: Vec<MachineId>,
    pub projects_changed: Vec<ProjectKey>,
    pub projects_removed: Vec<ProjectKey>,
    pub sessions_changed: Vec<SessionKey>,
    pub sessions_removed: Vec<SessionKey>,
    /// The order of projects on these machines changed.
    pub project_order_changed: Vec<MachineId>,
    /// The order or membership of sessions in these projects changed (tab strips, sidebar rows).
    pub session_order_changed: Vec<ProjectKey>,
    /// The connection state of these machines changed.
    pub connection_changed: Vec<MachineId>,
    pub side_state: SideStateChanges,
    /// Active project, active group, focused session, or the visible set changed.
    pub focus_changed: bool,
    /// The host-reported displayed set changed.
    pub displayed_changed: bool,
    /// Home gxserver's Cloud Boxes registry moved (`cloud_boxes.rs`): a box's state, name or
    /// cached sessions, which the merged box rows and their badges read.
    pub cloud_boxes_changed: bool,
}

impl ChangeSummary {
    pub fn ignored(reason: IgnoredReason) -> Self {
        Self {
            ignored: Some(reason),
            ..Self::default()
        }
    }

    /// True when nothing a UI draws changed.
    pub fn is_empty(&self) -> bool {
        self.machines_reloaded.is_empty()
            && self.projects_changed.is_empty()
            && self.projects_removed.is_empty()
            && self.sessions_changed.is_empty()
            && self.sessions_removed.is_empty()
            && self.project_order_changed.is_empty()
            && self.session_order_changed.is_empty()
            && self.connection_changed.is_empty()
            && !self.side_state.any()
            && !self.focus_changed
            && !self.displayed_changed
            && !self.cloud_boxes_changed
    }

    pub(crate) fn note_project_changed(&mut self, key: ProjectKey) {
        push_unique(&mut self.projects_changed, key);
    }

    pub(crate) fn note_project_removed(&mut self, key: ProjectKey) {
        push_unique(&mut self.projects_removed, key);
    }

    pub(crate) fn note_session_changed(&mut self, key: SessionKey) {
        push_unique(&mut self.sessions_changed, key);
    }

    pub(crate) fn note_session_removed(&mut self, key: SessionKey) {
        push_unique(&mut self.sessions_removed, key);
    }

    pub(crate) fn note_session_order_changed(&mut self, key: ProjectKey) {
        push_unique(&mut self.session_order_changed, key);
    }

    pub(crate) fn note_project_order_changed(&mut self, machine: MachineId) {
        push_unique(&mut self.project_order_changed, machine);
    }

    pub(crate) fn note_connection_changed(&mut self, machine: MachineId) {
        push_unique(&mut self.connection_changed, machine);
    }

    /// True when the ordered keys of some group's tab list may have changed (as opposed to the
    /// content of a row, which `sessions_changed` reports).
    ///
    /// It is conservative, never the other way round: every change of a tab list's keys sets it,
    /// and a few inputs set it without moving a key (a machine reload with identical rows, a
    /// legacy group delta). A project title change, a group rename, and a row content change do
    /// not set it.
    pub fn tab_lists_changed(&self) -> bool {
        !self.machines_reloaded.is_empty()
            || !self.projects_removed.is_empty()
            || !self.project_order_changed.is_empty()
            || !self.session_order_changed.is_empty()
    }

    /// Folds `other` into `self`, so a host can coalesce a burst of events into one repaint. `other`
    /// must be the later of the two: its verdict on a key (changed or removed) replaces the earlier one.
    pub fn merge(&mut self, other: ChangeSummary) {
        let ignored = self.ignored.take().or(other.ignored);
        for machine in other.machines_reloaded {
            push_unique(&mut self.machines_reloaded, machine);
        }
        // `other` happened after `self`, so its verdict on a key replaces the earlier one.
        for key in other.projects_changed {
            self.projects_removed.retain(|removed| *removed != key);
            self.note_project_changed(key);
        }
        for key in other.projects_removed {
            self.projects_changed.retain(|changed| *changed != key);
            self.note_project_removed(key);
        }
        for key in other.sessions_changed {
            self.sessions_removed.retain(|removed| *removed != key);
            self.note_session_changed(key);
        }
        for key in other.sessions_removed {
            self.sessions_changed.retain(|changed| *changed != key);
            self.note_session_removed(key);
        }
        for machine in other.connection_changed {
            self.note_connection_changed(machine);
        }
        for machine in other.project_order_changed {
            self.note_project_order_changed(machine);
        }
        for key in other.session_order_changed {
            self.note_session_order_changed(key);
        }
        self.side_state.workspace_groups |= other.side_state.workspace_groups;
        self.side_state.project_collections |= other.side_state.project_collections;
        self.side_state.spaces |= other.side_state.spaces;
        self.side_state.custom_session_tags |= other.side_state.custom_session_tags;
        self.focus_changed |= other.focus_changed;
        self.displayed_changed |= other.displayed_changed;
        self.cloud_boxes_changed |= other.cloud_boxes_changed;
        // A reason survives only while the combined result is still "nothing changed".
        self.ignored = if self.is_empty() { ignored } else { None };
    }
}

fn push_unique<T: PartialEq>(list: &mut Vec<T>, value: T) {
    if !list.contains(&value) {
        list.push(value);
    }
}
