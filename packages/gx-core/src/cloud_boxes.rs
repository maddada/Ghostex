//! Cloud Boxes as every client sees them: home gxserver's box registry, the hidden machine each
//! reachable box is, and the rule that lists a box's sessions inside its home project.
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1015 "in project with badge"): cloud sessions sit in their project with a ☁ badge whose
//! dot shows the box state; box machines never get a machine tab; the filter bar has a Cloud
//! filter. So a box is a hidden machine (`cloudbox:<boxId>`, its own gxserver feeding the store
//! like a remote computer's), and the sidebar of THIS computer draws that machine's sessions
//! inside the home project the registry names, never on a tab of their own.
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1013): "On sending a message there. Not on opening the session." A box that is not
//! reachable lists the sessions the registry cached for it ([`cached_box_snapshot`]), so opening
//! one reads what was last seen and calls nothing at the provider; only a send resumes it
//! ([`CloudBoxes::box_to_wake_for_send`]).
//!
//! SEE-ALSO: packages/gx-protocol/src/cloud_box/ (wire types), server/src/cloud_boxes/ (the
//! registry and `connectInfo`), apps/desktop/src/app/gx_store/cloud_boxes.rs (the host that
//! connects box machines), sidebar_view/model.rs (`box_rows_of_project`).

use std::collections::BTreeMap;

use ghostex_gx_protocol::cloud_box::{
    cloud_box_id_from_machine_id, cloud_box_machine_id, CloudBox, CloudBoxState,
    CloudBoxesChangedFrame, CloudProviderId,
};
use ghostex_gx_protocol::PresentationSnapshot;
use serde_json::{json, Value};

use crate::keys::{MachineId, SessionKey};
use crate::presentation_store::PresentationStore;

/// The sidebar filter id of "Cloud Box" in the Sort & Filter menu's filter list. It rides in the
/// tag filters (`selected_tag_filters`) so the filter bar, Clear filter and the reveal rules treat
/// it like any other ticked filter; a row matches it when it runs on a box.
pub const CLOUD_BOX_FILTER: &str = "cloudBox";

/// Home gxserver's box registry as this client last read it (`list`, then `cloudBoxesChanged`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CloudBoxes {
    boxes: BTreeMap<String, CloudBox>,
    /// A `list` answer has arrived at least once.
    listed: bool,
}

impl CloudBoxes {
    pub fn listed(&self) -> bool {
        self.listed
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    pub fn get(&self, box_id: &str) -> Option<&CloudBox> {
        self.boxes.get(box_id)
    }

    /// Every box the registry holds that is not deleted.
    pub fn iter(&self) -> impl Iterator<Item = &CloudBox> {
        self.boxes
            .values()
            .filter(|cloud_box| !cloud_box.state.is_gone())
    }

    /// The box a `cloudbox:<boxId>` machine is.
    pub fn of_machine(&self, machine: &MachineId) -> Option<&CloudBox> {
        self.get(box_id_of_machine(machine)?)
    }

    /// The boxes whose gxserver a client should be connected to right now.
    pub fn reachable(&self) -> impl Iterator<Item = &CloudBox> {
        self.iter()
            .filter(|cloud_box| cloud_box.state.is_reachable())
    }

    /// The box a send to this session must resume first (Q1013), when the session runs on a box
    /// that is paused or stopped.
    pub fn box_to_wake_for_send(&self, session: &SessionKey) -> Option<&CloudBox> {
        self.of_machine(&session.machine)
            .filter(|cloud_box| cloud_box.state.wakes_on_send())
    }

    /// Replaces the whole registry with a `list` answer. Returns whether anything moved.
    pub(crate) fn replace_all(&mut self, boxes: Vec<CloudBox>) -> bool {
        let next: BTreeMap<String, CloudBox> = boxes
            .into_iter()
            .filter(|cloud_box| !cloud_box.box_id.is_empty())
            .map(|cloud_box| (cloud_box.box_id.clone(), cloud_box))
            .collect();
        let moved = !self.listed || next != self.boxes;
        self.boxes = next;
        self.listed = true;
        moved
    }

    /// Applies one `cloudBoxesChanged` frame. Returns whether a row moved, and whether the frame
    /// asked for the list to be read again (no row, `scope` naming `boxes`).
    pub(crate) fn apply_frame(&mut self, frame: &CloudBoxesChangedFrame) -> (bool, bool) {
        let Some(cloud_box) = frame.cloud_box.as_ref() else {
            let refetch =
                frame.scope.is_empty() || frame.scope.iter().any(|scope| scope == "boxes");
            return (false, refetch);
        };
        if cloud_box.box_id.is_empty() {
            return (false, false);
        }
        if cloud_box.state.is_gone() {
            return (self.boxes.remove(&cloud_box.box_id).is_some(), false);
        }
        let moved = self.boxes.get(&cloud_box.box_id) != Some(cloud_box);
        if moved {
            self.boxes
                .insert(cloud_box.box_id.clone(), cloud_box.clone());
        }
        (moved, false)
    }
}

/// The box id of a box machine, `None` for this computer and for a saved remote machine.
pub fn box_id_of_machine(machine: &MachineId) -> Option<&str> {
    cloud_box_id_from_machine_id(machine.remote_id()?)
}

/// A box's hidden machine.
pub fn cloud_box_machine(box_id: &str) -> MachineId {
    MachineId::Remote(cloud_box_machine_id(box_id))
}

/// Whether the machine is a box's hidden machine.
pub fn is_cloud_box_machine(machine: &MachineId) -> bool {
    box_id_of_machine(machine).is_some()
}

/// The machine tab a machine's rows are drawn on: a box's sessions sit in their home project on
/// this computer's tab; every other machine has its own.
pub fn sidebar_tab_machine(machine: &MachineId) -> MachineId {
    if is_cloud_box_machine(machine) {
        MachineId::Local
    } else {
        machine.clone()
    }
}

/// The sessions of every box whose home project is `home_project_id`, in each box's own order,
/// boxes in registry order. Only listed rows; a box machine the store does not hold yet adds
/// nothing.
pub(crate) fn box_session_keys(
    store: &PresentationStore,
    boxes: &CloudBoxes,
    home_project_id: &str,
) -> Vec<SessionKey> {
    let mut keys = Vec::new();
    for cloud_box in boxes
        .iter()
        .filter(|cloud_box| cloud_box.home_project_id.as_deref() == Some(home_project_id))
    {
        let machine = cloud_box_machine(&cloud_box.box_id);
        let Some(entry) = store.machine(&machine) else {
            continue;
        };
        let Some(loaded) = entry.loaded() else {
            continue;
        };
        for group in loaded.groups() {
            for session_id in &group.session_ids {
                let listed = entry
                    .effective_session(&group.project_id, session_id)
                    .is_some_and(|session| session.listed_in_sidebar());
                if listed {
                    keys.push(SessionKey {
                        machine: machine.clone(),
                        project_id: group.project_id.clone(),
                        session_id: session_id.clone(),
                    });
                }
            }
        }
    }
    keys
}

/// What a row's ☁ badge draws, from the registry: the box's name, provider and state (the dot).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionCloudBoxBadge {
    pub box_id: String,
    pub box_name: String,
    pub provider: CloudProviderId,
    pub state: CloudBoxState,
    /// The state line ("Pauses in 6 min", the failure), for the badge's tooltip.
    pub state_detail: Option<String>,
}

impl SessionCloudBoxBadge {
    /// The badge a box's own gxserver published on the row, before the registry's newer word.
    pub fn from_presentation(
        cloud_box: &ghostex_gx_protocol::cloud_box::PresentationCloudBox,
    ) -> Self {
        Self {
            box_id: cloud_box.box_id.clone(),
            box_name: cloud_box.box_name.clone(),
            provider: cloud_box.provider.clone(),
            state: cloud_box.state.clone(),
            state_detail: None,
        }
    }

    /// A home draft's badge once its first message started a box: that box's name and state from
    /// the registry, so a failed start reads as Failed instead of still waiting.
    ///
    /// CDXC:CloudBoxes 2026-11-02 WHY: the draft's own row (`runtimeSettings.cloudBox`) stays
    /// `pending` until the box session replaces it, so after a failed start its badge kept saying
    /// the box starts with the first message. The draft records the box it started (`boxId`), and
    /// the registry knows how that went.
    /// SEE-ALSO: apps/mobile/app/src/cloudBoxes/boxSessions.ts (`withBoxSessions`, the phone's twin).
    pub fn with_started_box(self, boxes: &CloudBoxes) -> Self {
        if self.state != CloudBoxState::Pending || self.box_id.is_empty() {
            return self;
        }
        boxes.get(&self.box_id).map_or(self, Self::from_box)
    }

    pub fn from_box(cloud_box: &CloudBox) -> Self {
        Self {
            box_id: cloud_box.box_id.clone(),
            box_name: cloud_box.name.clone(),
            provider: cloud_box.provider.clone(),
            state: cloud_box.state.clone(),
            state_detail: cloud_box.state_detail.clone(),
        }
    }

    /// The badge's hover text: "box-c31 on Instabox · Paused · sending wakes it".
    pub fn tooltip(&self) -> String {
        let name = if self.box_name.trim().is_empty() {
            "Cloud box"
        } else {
            self.box_name.trim()
        };
        let state = cloud_box_state_label(&self.state);
        let mut text = format!("{name} on {} · {state}", self.provider.label());
        if self.state.wakes_on_send() {
            text.push_str(" · sending wakes it");
        }
        if let Some(detail) = self
            .state_detail
            .as_deref()
            .map(str::trim)
            .filter(|detail| !detail.is_empty())
        {
            text.push_str(" · ");
            text.push_str(detail);
        }
        text
    }
}

/// The word a user reads for a box state.
pub fn cloud_box_state_label(state: &CloudBoxState) -> &str {
    match state {
        CloudBoxState::Pending => "Not started",
        CloudBoxState::Creating => "Creating",
        CloudBoxState::Booting => "Starting",
        CloudBoxState::Running => "Running",
        CloudBoxState::Idle => "Idle",
        CloudBoxState::Paused => "Paused",
        CloudBoxState::Stopped => "Stopped",
        CloudBoxState::Deleting => "Deleting",
        CloudBoxState::Deleted => "Deleted",
        CloudBoxState::Failed => "Failed",
        CloudBoxState::Other(value) => value.as_str(),
    }
}

/// A presentation snapshot of the sessions the registry cached for a box that is not reachable,
/// so its rows are listed (faded, through the last-seen seed) without waking it. `None` when it
/// cached nothing.
///
/// The rows are sleeping and idle: whatever ran there is paused or ended, and a send is what
/// brings it back.
pub fn cached_box_snapshot(cloud_box: &CloudBox) -> Option<PresentationSnapshot> {
    let sessions: Vec<_> = cloud_box
        .sessions
        .iter()
        .filter(|row| !row.archived && !row.project_id.is_empty() && !row.session_id.is_empty())
        .collect();
    if sessions.is_empty() {
        return None;
    }
    let mut projects: Vec<String> = Vec::new();
    for row in &sessions {
        if !projects.contains(&row.project_id) {
            projects.push(row.project_id.clone());
        }
    }
    let presentation_box = json!({
        "boxId": cloud_box.box_id,
        "provider": cloud_box.provider,
        "state": cloud_box.state,
        "boxName": cloud_box.name,
        "homeProjectId": cloud_box.home_project_id,
    });
    let session_rows: Vec<Value> = sessions
        .iter()
        .map(|row| {
            let mut session = json!({
                "projectId": row.project_id,
                "sessionId": row.session_id,
                "groupId": row.project_id,
                "kind": "agent",
                "surface": "workspace",
                "visibleInSidebarByDefault": true,
                "lifecycleState": "sleeping",
                "activity": "idle",
                "title": row.title,
                "updatedAt": row.last_active_at.clone().unwrap_or_default(),
                "createdAt": row.last_active_at.clone().unwrap_or_default(),
                "cloudBox": presentation_box,
            });
            if let Some(agent_id) = &row.agent_id {
                session["agentId"] = json!(agent_id);
                session["agentIcon"] = json!(agent_id);
            }
            if let Some(agent_session_id) = &row.agent_session_id {
                session["agentSessionId"] = json!(agent_session_id);
            }
            session
        })
        .collect();
    let snapshot = json!({
        "revision": 0,
        "generatedAt": cloud_box.stopped_at.clone().or_else(|| cloud_box.last_active_at.clone()).unwrap_or_default(),
        "projects": projects.iter().map(|project_id| json!({
            "projectId": project_id,
            "title": cloud_box.name,
            "groupIds": [project_id],
        })).collect::<Vec<_>>(),
        "groups": projects.iter().map(|project_id| json!({
            "groupId": project_id,
            "projectId": project_id,
            "sessionIds": sessions
                .iter()
                .filter(|row| &row.project_id == project_id)
                .map(|row| row.session_id.clone())
                .collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "sessions": session_rows,
    });
    serde_json::from_value(snapshot).ok()
}
