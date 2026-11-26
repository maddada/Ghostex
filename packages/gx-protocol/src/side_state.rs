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
    /// Hidden from the Spaces row until Show hidden is on; absent = shown (and on an older daemon,
    /// which drops the field). gxserver keeps a stored `true` when a client writes the Space
    /// without the field.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
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
    /// One character, kept for phones and desktops older than the workspace icons.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub letter: String,
    /// The icon picked in Settings > Workspaces, one of [`WORKSPACE_ICONS`]; empty = the kind's
    /// default ([`SidebarWorkspace::icon_id`]).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub icon: String,
    /// `work` or `personal`; sets the work-mode default of the workspace's projects.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub kind: String,
    /// The first saved Claude account (`/api/agentAccounts` id) this workspace's agents launch
    /// with; kept in sync with `claude_account_ids` for older clients and phones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_account_id: Option<String>,
    /// The saved Claude accounts (`/api/agentAccounts` ids) this workspace's agents may launch
    /// with; empty means follow Settings > Accounts > Account for new sessions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_account_ids: Vec<String>,
    /// The remote computer id where this workspace's instructions, Claude accounts and claude.ai
    /// connectors live; absent means this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_machine_id: Option<String>,
    /// Cached display name of `home_machine_id`, for the read-only note on other computers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_machine_name: Option<String>,
    /// `linear` or `github`: the primary tracker picked on this computer (server/src/work_mode/
    /// tracker.rs). Absent = never picked; a team's own choice is not in this document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker: Option<String>,
    /// The other workspaces whose Spaces this workspace's windows (and the phone's list of it)
    /// show too, mixed in with its own in the Spaces row in the one stored Spaces order (Settings >
    /// Workspaces > Show Work's Spaces). Empty: only its own Spaces, as before.
    ///
    /// CDXC:Workspaces 2026-11-26 DECISION:
    /// User: "i want the option to merge the spaces from workspaces to my main workspaces so i dont
    /// need to switch between the work workspace and regular workspace ... put a separator between
    /// the work workspace spaces and the regular ones ... when i hover on spaces we need to show
    /// in this case which workspace they're part of". From the mockup they picked a hairline (Q5011
    /// A), no label with the hover card naming the workspace (Q5012 A), the switch on the viewing
    /// workspace's section in Settings > Workspaces, per workspace (Q5013 A), reorder only inside
    /// the other workspace's group and Hide Here for one Space (Q5014 A + B). Off by default.
    ///
    /// CDXC:Workspaces 2026-11-26 DECISION:
    /// User, the same day: "if i pick show work's spaces please hide the toggle between
    /// workspaces, not needed anymore. also please allow me to swipe to the work space and back to
    /// personal spaces regularly. also when i show a work workspace in my personal workspace allow
    /// me to place it anywhere, remove the line between the spaces pls". So, while Spaces are on
    /// and this list names a workspace: no workspace tile in the Spaces row (desktop
    /// native_sidebar/workspace_tile.rs), no hairline, the shown Spaces in the stored `order`
    /// (no grouping by workspace, gx-core `window_spaces`), Other last, a Space of either workspace
    /// dragged anywhere (gx-core `plan_move_space`), and swipes and Go to Space walk that one row.
    /// Supersedes the hairline (Q5011 A) and reorder only inside a group (Q5014 A).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub show_spaces_from: Vec<String>,
    /// Spaces of the workspaces in `show_spaces_from` hidden from this workspace's row only (a
    /// Space's Hide Here); the Space's own `hidden` (Hide Space) still hides it everywhere.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_space_ids: Vec<String>,
}

/// The icons a workspace can wear (Settings > Workspaces > Icon), in the picker's order. None of
/// them is a Space icon (the desktop's `SPACE_EDITOR_ICONS`), and the sidebar draws them on a
/// filled disc in the workspace's colour, where a Space is an outline glyph on a hairline tile.
///
/// CDXC:Workspaces 2026-11-20 DECISION:
/// User picked "F. The first tile of the Spaces row" (Q4296): "each workspace gets its own ICON (not just a colour dot), and those icons must look different from the Spaces icons (e.g. a distinct shape/frame or a different icon set, picked per workspace in Settings)". Both: a set of its own (places and roles, no developer glyphs) and a shape of its own (the glyph on a disc in the workspace's colour). The ids are Tabler icon names, the desktop's `assets/titlebar/<id>.svg` and the phone's `COMMAND_ICONS`.
pub const WORKSPACE_ICONS: [&str; 12] = [
    "home",
    "briefcase",
    "building",
    "user",
    "users",
    "coffee",
    "camera",
    "music",
    "palette",
    "plane",
    "leaf",
    "moon",
];

impl SidebarWorkspace {
    pub fn is_work(&self) -> bool {
        self.kind == "work"
    }

    /// The icon to draw: the one picked when it is still one of [`WORKSPACE_ICONS`], else
    /// `briefcase` for a work workspace and `user` for a personal one.
    ///
    /// CDXC:Workspaces 2026-11-21 DECISION:
    /// User: "I don't like this icon for the home, for personal and work. Just make it like a person icon for personal and the bag icon for work". A personal workspace's default is the person (`user`) instead of the house; a work one keeps the bag. A picked icon still wins.
    pub fn icon_id(&self) -> &str {
        match WORKSPACE_ICONS.iter().find(|icon| **icon == self.icon) {
            Some(icon) => icon,
            None if self.is_work() => "briefcase",
            None => "user",
        }
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
    /// or one of its projects is. `has_project` says whether the machine still has a project, so a
    /// placement left by a project removed there does not keep its tab in an empty workspace.
    pub fn machine_shows_in(
        &self,
        machine_id: &str,
        workspace_id: &str,
        has_project: impl Fn(&str) -> bool,
    ) -> bool {
        self.machine_workspace(machine_id) == workspace_id
            || self
                .remote_project_workspaces
                .get(machine_id)
                .is_some_and(|projects| {
                    projects
                        .iter()
                        .any(|(project_id, id)| id == workspace_id && has_project(project_id))
                })
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

    /// The other workspaces whose Spaces `workspace_id` shows too (its `show_spaces_from`), in
    /// display order, leaving out itself and ids that name no workspace.
    pub fn joined_workspace_ids(&self, workspace_id: &str) -> Vec<&str> {
        let Some(workspace) = self.workspaces.get(workspace_id) else {
            return Vec::new();
        };
        self.order
            .iter()
            .map(String::as_str)
            .filter(|id| {
                *id != workspace_id
                    && self.workspaces.contains_key(*id)
                    && workspace.show_spaces_from.iter().any(|shown| shown == id)
            })
            .collect()
    }

    /// `id` when it names a workspace, else the default workspace's id.
    pub fn resolve<'a>(&'a self, id: Option<&'a str>) -> &'a str {
        match id {
            Some(id) if self.workspaces.contains_key(id) => id,
            _ => self.default_id(),
        }
    }
}
