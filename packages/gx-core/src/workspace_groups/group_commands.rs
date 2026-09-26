//! A user-made group's own commands: New Group (a project's menu), Rename (the inline editor) and
//! Close Group (a group's menu, after the native confirm).
//!
//! CDXC:Sessions 2026-09-25 WHY:
//! These three were the last edits of the workspace session groups document the old runtime made
//! itself (`createWorkspaceGroup`, `renameWorkspaceGroup`, `closeWorkspaceGroup`) and handed over
//! with `persistWorkspaceGroups`. The planner is the TypeScript's, early return for early return;
//! the host writes the document through the one writer (gx_store/workspace_groups.rs), closes the
//! members through the store's own close, and dispatches a `focusGroup` sidebar command for the
//! group the command makes active (performed in Rust by gx_store/focus_perform.rs since the
//! QuickJS runtime was deleted on 2026-09-25; `focusGroup` with a user-made group's id is exactly the `activeProjectId` / `activeGroupId` /
//! HUD refresh / publish tail these handlers ended with).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/workspace_groups/group_commands.rs (the host). The
//! TypeScript handlers were in the deleted `gxserver-runtime/workspace-groups-sync.ts`.

use serde_json::Value;

use crate::focus::ActiveGroup;
use crate::keys::{
    encode_workspace_subgroup_id, parse_workspace_subgroup_id, ProjectKey, SessionKey,
};
use crate::session_create::group_project;

use super::document::WorkspaceGroupsDocument;
use super::edits::WORKSPACE_SESSION_GROUP_MAX_COUNT;

/// The command types this file answers.
const GROUP_COMMAND_MESSAGE_TYPES: &[&str] = &["createGroup", "renameGroup", "closeGroup"];

/// What the host does for one group command, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupCommandPlan {
    /// New Group: write the document, then make the new group active.
    Create {
        document: WorkspaceGroupsDocument,
        activate_group_id: String,
    },
    /// `postSidebarActionToast('info', 'Group limit reached for this project.')`, nothing written.
    LimitReached,
    /// Rename: write the document.
    Rename { document: WorkspaceGroupsDocument },
    /// Close Group: close every member (their sidebar session ids, in the group's order), then write
    /// the document without the group, then, when the closed group was the active one, make the
    /// project's own group active.
    Close {
        member_session_ids: Vec<String>,
        document: WorkspaceGroupsDocument,
        activate_project_group: Option<ProjectKey>,
    },
    /// The TypeScript's early return: nothing happens.
    Nothing,
}

/// Whether this message is one this file answers, without building anything.
pub fn owns_group_command(message: &Value) -> bool {
    message
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| GROUP_COMMAND_MESSAGE_TYPES.contains(&kind))
}

/// Plans one group command against the held document and the store's active project and group.
/// `None` is a message this file does not answer.
pub fn plan_group_command(
    document: &WorkspaceGroupsDocument,
    active_project: Option<&ProjectKey>,
    active_group: Option<&ActiveGroup>,
    message: &Value,
) -> Option<GroupCommandPlan> {
    let group_id = message.get("groupId").and_then(Value::as_str);
    Some(match message.get("type").and_then(Value::as_str)? {
        "createGroup" => plan_create(document, active_project, group_id),
        "renameGroup" => {
            let title = message.get("title").and_then(Value::as_str).unwrap_or("");
            plan_rename(document, group_id, title)
        }
        "closeGroup" => plan_close(document, active_group, group_id),
        _ => return None,
    })
}

/// `createWorkspaceGroup(groupId)`: the group's project, else the active project.
fn plan_create(
    document: &WorkspaceGroupsDocument,
    active_project: Option<&ProjectKey>,
    group_id: Option<&str>,
) -> GroupCommandPlan {
    // `this.activeProjectId` is this computer's raw project id only; a remote project is never it.
    let project = group_id
        .filter(|group_id| !group_id.is_empty())
        .and_then(group_project)
        .or_else(|| {
            active_project
                .filter(|project| project.machine.is_local())
                .cloned()
        });
    let Some(project) = project else {
        return GroupCommandPlan::Nothing;
    };
    // The same test `create_subgroup` makes, spelled here so the limit is a plan and not a `None`
    // a reader has to know the meaning of.
    let held = document
        .projects
        .get(&project.to_workspace_project_id())
        .map_or(0, |groups| groups.groups.len());
    if held + 1 >= WORKSPACE_SESSION_GROUP_MAX_COUNT {
        return GroupCommandPlan::LimitReached;
    }
    match document.create_subgroup(&project.to_workspace_project_id(), None) {
        Some((new_group_id, document)) => GroupCommandPlan::Create {
            document,
            activate_group_id: encode_workspace_subgroup_id(&project, &new_group_id),
        },
        None => GroupCommandPlan::LimitReached,
    }
}

/// `renameWorkspaceGroup(groupId, title)`: user-made groups only.
fn plan_rename(
    document: &WorkspaceGroupsDocument,
    group_id: Option<&str>,
    title: &str,
) -> GroupCommandPlan {
    let Some((project, subgroup)) = group_id.and_then(parse_workspace_subgroup_id) else {
        return GroupCommandPlan::Nothing;
    };
    match document.rename_subgroup(&project.to_workspace_project_id(), &subgroup, title) {
        Some(document) => GroupCommandPlan::Rename { document },
        None => GroupCommandPlan::Nothing,
    }
}

/// `closeWorkspaceGroup(groupId)`: user-made groups only. The document is written even when the
/// group was already gone, because `persistWorkspaceGroups` ran unconditionally there.
fn plan_close(
    document: &WorkspaceGroupsDocument,
    active_group: Option<&ActiveGroup>,
    group_id: Option<&str>,
) -> GroupCommandPlan {
    let Some((project, subgroup)) = group_id.and_then(parse_workspace_subgroup_id) else {
        return GroupCommandPlan::Nothing;
    };
    let workspace_project_id = project.to_workspace_project_id();
    let member_session_ids = document
        .subgroup_members(&workspace_project_id, &subgroup)
        .into_iter()
        .map(|session_id| {
            let key = match &project.machine {
                crate::keys::MachineId::Local => {
                    SessionKey::local(project.project_id.clone(), session_id)
                }
                crate::keys::MachineId::Remote(machine_id) => {
                    SessionKey::remote(machine_id.clone(), project.project_id.clone(), session_id)
                }
            };
            key.to_sidebar_session_id()
        })
        .collect();
    let next = document
        .remove_subgroup(&workspace_project_id, &subgroup)
        .unwrap_or_else(|| document.clone());
    let was_active = matches!(
        active_group,
        Some(ActiveGroup::Subgroup { project: active_project, group_id: active_group_id })
            if *active_project == project && *active_group_id == subgroup
    );
    GroupCommandPlan::Close {
        member_session_ids,
        document: next,
        activate_project_group: was_active.then_some(project),
    }
}
