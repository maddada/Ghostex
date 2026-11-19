//! Server-owned sidebar documents that ride the presentation snapshot and have their own change
//! frames: user-made session groups, project collections, Spaces, and the custom tag catalog.
//!
//! Each document is normalized by gxserver and always travels whole: a change frame replaces the
//! client's copy. `order` is the authoritative ordering; the maps are keyed by id. Member ids may
//! be soft references to things that no longer exist, so clients tolerate ids they cannot resolve.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSessionGroup {
    pub group_id: String,
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub session_ids: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProjectGroups {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub groups: Vec<WorkspaceSessionGroup>,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub next_group_number: Option<u64>,
}

/// User-made session groups per project, plus the manual project order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSessionGroupsState {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub project_order: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub projects: BTreeMap<String, WorkspaceProjectGroups>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarProjectCollection {
    pub collection_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub color: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub project_ids: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub title: String,
}

/// Project collections (folders). A project id appears in at most one collection; empty
/// collections are dropped by the server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarProjectCollectionsState {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub collections: BTreeMap<String, SidebarProjectCollection>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub next_collection_number: u64,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub order: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSpace {
    pub space_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    /// Lowercase `#rrggbb`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub color: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub icon: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub member_collection_ids: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub member_project_ids: Vec<String>,
    /// The workspace this Space belongs to; absent = the default workspace (and on an older
    /// daemon). gxserver keeps a stored id when a client writes the Space without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// Spaces: saved sidebar filters. An empty Space is valid and kept.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSpacesState {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub order: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub spaces: BTreeMap<String, SidebarSpace>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSessionTag {
    pub tag_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    /// Lowercase `#rrggbb`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub color: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub icon: String,
}

/// The custom tag catalog. A tag removed here is cleared from every session in the same write.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSessionTagsState {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub order: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub tags: BTreeMap<String, CustomSessionTag>,
}

/// One workspace: a company (kind `work`) or Personal, with its own projects, Spaces, Linear key,
/// Claude account and browser sign-ins.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarWorkspace {
    pub workspace_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    /// Lowercase `#rrggbb`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub color: String,
    /// One character drawn on the workspace tile.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub letter: String,
    /// `work` or `personal`; sets the work-mode default of the workspace's projects.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub kind: String,
    /// The saved Claude account (`/api/agentAccounts` id) this workspace's agents launch with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_account_id: Option<String>,
    /// `linear` or `github`: the primary tracker picked on this computer (server/src/work_mode/
    /// tracker.rs). Absent = never picked; a team's own choice is not in this document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker: Option<String>,
}

impl SidebarWorkspace {
    pub fn is_work(&self) -> bool {
        self.kind == "work"
    }
}

/// Workspaces. A project or Space with no `workspaceId` belongs to `defaultWorkspaceId`, which
/// always exists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarWorkspacesState {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub default_workspace_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub order: Vec<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub workspaces: BTreeMap<String, SidebarWorkspace>,
    /// A remote machine's sidebar tab (by this computer's settings id for it) → the workspace it
    /// shows in here. A machine not listed shows in the default workspace.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub machine_workspaces: BTreeMap<String, String>,
    /// A remote machine (same id as `machine_workspaces`) → its projects placed apart from its tab
    /// → the workspace each shows in here (gxserver `assign_remote_project_workspace_in`).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub remote_project_workspaces: BTreeMap<String, BTreeMap<String, String>>,
}

impl SidebarWorkspacesState {
    /// The workspace a remote machine's tab shows in on this computer.
    pub fn machine_workspace(&self, machine_id: &str) -> &str {
        self.resolve(self.machine_workspaces.get(machine_id).map(String::as_str))
    }

    /// The workspace a remote machine's project shows in on this computer: its own placement, else
    /// its machine tab's.
    pub fn remote_project_workspace(&self, machine_id: &str, project_id: &str) -> &str {
        match self
            .remote_project_workspaces
            .get(machine_id)
            .and_then(|projects| projects.get(project_id))
        {
            Some(id) if self.workspaces.contains_key(id) => id,
            _ => self.machine_workspace(machine_id),
        }
    }

    /// Whether a remote machine has anything to show in `workspace_id`: its tab is placed there,
    /// or one of its projects is.
    pub fn machine_shows_in(&self, machine_id: &str, workspace_id: &str) -> bool {
        self.machine_workspace(machine_id) == workspace_id
            || self
                .remote_project_workspaces
                .get(machine_id)
                .is_some_and(|projects| projects.values().any(|id| id == workspace_id))
    }

    /// The default workspace's id (`personal` when the document came without one).
    pub fn default_id(&self) -> &str {
        if self.default_workspace_id.is_empty() {
            "personal"
        } else {
            &self.default_workspace_id
        }
    }

    /// Workspaces in display order.
    pub fn ordered(&self) -> impl Iterator<Item = &SidebarWorkspace> {
        self.order.iter().filter_map(|id| self.workspaces.get(id))
    }

    /// `id` when it names a workspace, else the default workspace's id.
    pub fn resolve<'a>(&'a self, id: Option<&'a str>) -> &'a str {
        match id {
            Some(id) if self.workspaces.contains_key(id) => id,
            _ => self.default_id(),
        }
    }
}
