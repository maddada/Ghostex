//! Which project a create names.
//!
//! The sidebar names a create by a GROUP id, in one of four shapes: a project's own group on this
//! computer (`combined-project:<id>`), a project's group on a remote machine
//! (`remote:<machine>:group:<id>`), a user-made group inside either
//! (`gpui-wsg:<workspace project id>:<group>`), or the Chats collection, which names no project.
//! These are the resolutions `session-create.ts` and `workspace-groups-sync.ts` made, kept as one
//! function each so a create and a browser open cannot disagree about the same id.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/create/ (the host), packages/gx-core/src/keys.rs.

use crate::keys::{parse_workspace_subgroup_id, MachineId, ProjectKey};

/// Where a terminal create lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateTarget {
    /// A project on this computer, or none at all (the Chats collection, or nothing active), and
    /// the user-made group the new session joins.
    Local {
        project_id: Option<String>,
        subgroup: Option<String>,
    },
    /// A project on a remote machine, and the user-made group the new session joins.
    Remote {
        project: ProjectKey,
        subgroup: Option<String>,
    },
}

/// `createSession(groupId = this.activeGroupId)`: the group the message named, else the active
/// group, else the active project.
///
/// A blank group id falls through to the active project exactly as the TypeScript's
/// `groupId ? ... : this.activeProjectId` did, and a group id that names no project (the Chats
/// collection, an id of another shape) is a create with no project at all, which gxserver answers
/// by putting the terminal where it puts a projectless one.
pub fn terminal_create_target(
    group_id: Option<&str>,
    active_project_id: Option<&str>,
) -> CreateTarget {
    let Some(group_id) = group_id.filter(|group_id| !group_id.is_empty()) else {
        return CreateTarget::Local {
            project_id: active_project_id
                .filter(|project_id| !project_id.is_empty())
                .map(str::to_string),
            subgroup: None,
        };
    };
    if let Some((project, subgroup)) = parse_workspace_subgroup_id(group_id) {
        return match project.machine {
            MachineId::Local => CreateTarget::Local {
                project_id: Some(project.project_id),
                subgroup: Some(subgroup),
            },
            MachineId::Remote(_) => CreateTarget::Remote {
                project,
                subgroup: Some(subgroup),
            },
        };
    }
    match ProjectKey::parse_sidebar_group_id(group_id) {
        Some(project) if !project.machine.is_local() => CreateTarget::Remote {
            project,
            subgroup: None,
        },
        Some(project) => CreateTarget::Local {
            project_id: Some(project.project_id),
            subgroup: None,
        },
        None => CreateTarget::Local {
            project_id: None,
            subgroup: None,
        },
    }
}

/// `resolveWorkspaceGroupProjectId`: the project a group belongs to, whichever machine it is on.
/// `None` for the Chats collection and for anything that is not a group id.
pub fn group_project(group_id: &str) -> Option<ProjectKey> {
    if let Some((project, _)) = parse_workspace_subgroup_id(group_id) {
        return Some(project);
    }
    ProjectKey::parse_sidebar_group_id(group_id)
}

/// `gpuiProjectNameFromPath(path)`: the last path segment, Windows separators read as slashes for
/// a drive or UNC path; `Project` for a path with no segment.
pub fn project_name_from_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let windows = (bytes.len() >= 3
        && bytes[0].is_ascii_lowercase()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/'))
        || lower.starts_with("\\\\")
        || lower.starts_with("//");
    let normalized = match windows {
        true => path.replace('\\', "/"),
        false => path.to_string(),
    };
    normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
        .map(str::to_string)
        .unwrap_or_else(|| "Project".to_string())
}

/// `normalizeGpuiProjectPath(value)`: trimmed, trailing slashes removed; `None` for a blank path.
pub fn normalize_project_path(path: &str) -> Option<String> {
    let trimmed = crate::sidebar_view::text::js_trim(path);
    (!trimmed.is_empty()).then(|| trimmed.trim_end_matches('/').to_string())
}
