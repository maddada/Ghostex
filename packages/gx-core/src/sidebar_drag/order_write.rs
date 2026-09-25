//! What an order message DOES: the document edit, or the daemon's own session order.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! `syncSessionOrder` reaches two entirely different writers and the group id is what chooses
//! between them. A USER-MADE group (`gpui-wsg:`) edits the workspace session groups document,
//! which is client storage with a debounced write-through and the pending-push guard; a PROJECT
//! group calls `/api/updateSessionOrder` and rewrites the local `sidebarOrder` of that project's
//! rows so the list moves before the daemon answers. Porting one and not the other would leave a
//! drag inside a project silently doing nothing, and the two are one function on the TypeScript
//! side only because the `case` arm asks the id first, so they are one function here too.
//!
//! Nothing in this file decides a refusal from a rule: every early return below is one the
//! TypeScript had, at the same place and for the same id. It was ported from
//! `gxserver-runtime/workspace-groups-sync.ts` (`syncWorkspaceSubgroupSessionOrder`,
//! `moveSessionToWorkspaceGroup`, `createWorkspaceGroupFromSession`),
//! `gxserver-runtime/sessions-and-focus.ts` (`syncSessionOrder`) and
//! `packages/shared/gxserver-presentation-cache.ts` (`reorderPresentationProjectSessions`), all
//! deleted; see git history.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_drag.rs.

use serde_json::{json, Value};

use crate::keys::{parse_workspace_subgroup_id, ProjectKey, SessionKey};
use crate::workspace_groups::WorkspaceGroupsDocument;

use super::super::sidebar_actions::ToastLevel;

/// The three message types this file answers.
pub const ORDER_WRITE_MESSAGE_TYPES: &[&str] = &[
    "syncSessionOrder",
    "moveSessionToGroup",
    "createGroupFromSession",
];

/// One thing the host does for an order message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderWrite {
    /// The workspace session groups document after the edit. It goes through
    /// `WorkspaceGroupsSync::edit`, which writes client storage and books the debounced push, and
    /// into the store's own side state so the list redraws in this frame.
    EditDocument { document: WorkspaceGroupsDocument },
    /// `/api/updateSessionOrder` plus the local `sidebarOrder` rewrite that makes the rows move
    /// before it answers. The ids are RAW session ids of one project, in their new order.
    SessionOrderCall {
        project: ProjectKey,
        session_ids: Vec<String>,
    },
    /// `activeProjectId` and `activeGroupId` after a group is created from a session.
    ActivateSubgroup {
        project: ProjectKey,
        group_id: String,
    },
    /// `postSidebarActionToast('info', 'Group limit reached for this project.')`.
    Toast { level: ToastLevel, title: String },
    /// `updateRemoteWorkspaceGroups`: `/api/updateWorkspaceSessionGroups` with `{ state }` down one
    /// remote machine's tunnel (project_order_write.rs).
    RemoteProjectOrder { machine_id: String, state: Value },
}

impl OrderWrite {
    /// The write as the parity gate compares it. The document is compared whole, because it is the
    /// thing a stale echo would undo.
    pub fn to_json(&self) -> Value {
        match self {
            Self::EditDocument { document } => {
                json!({ "write": "editDocument", "document": document.to_json() })
            }
            Self::SessionOrderCall {
                project,
                session_ids,
            } => json!({
                "write": "sessionOrderCall",
                "rpc": {
                    "path": "/api/updateSessionOrder",
                    "params": { "projectId": project.project_id, "sessionIds": session_ids },
                },
            }),
            // Named the way the TypeScript names them, because these two fields ARE
            // `activeProjectId` and `activeGroupId` and the gate compares them by name.
            // The runtime's `activeProjectId` is this computer's projects only, so a remote
            // group moves its `activeGroupId` alone.
            Self::ActivateSubgroup { project, group_id } => {
                let mut value = json!({
                    "write": "activateSubgroup",
                    "activeGroupId": crate::keys::encode_workspace_subgroup_id(project, group_id),
                });
                if project.machine.is_local() {
                    value["activeProjectId"] = json!(project.project_id);
                }
                value
            }
            Self::Toast { level, title } => {
                json!({ "write": "toast", "level": level.as_str(), "title": title })
            }
            Self::RemoteProjectOrder { machine_id, state } => json!({
                "write": "remoteProjectOrder",
                "remoteMachineId": machine_id,
                "rpc": { "path": "/api/updateWorkspaceSessionGroups", "params": { "state": state } },
            }),
        }
    }
}

/// Everything one order message does, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderWritePlan {
    pub writes: Vec<OrderWrite>,
    /// Why the plan is empty, for the record line. Never compared.
    pub refusal: Option<&'static str>,
}

impl OrderWritePlan {
    /// A plan with one document edit, which is what the project order write also produces.
    pub(super) fn edit(document: WorkspaceGroupsDocument) -> Self {
        Self {
            writes: vec![OrderWrite::EditDocument { document }],
            refusal: None,
        }
    }

    pub(super) fn refused(reason: &'static str) -> Self {
        Self {
            writes: Vec::new(),
            refusal: Some(reason),
        }
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.writes.iter().map(OrderWrite::to_json).collect())
    }
}

/// Whether this message is one this file answers, without building anything.
pub fn owns_order_write_message(message: &Value) -> bool {
    message
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| ORDER_WRITE_MESSAGE_TYPES.contains(&kind))
}

/// What an order message does to the document the sync holds.
///
/// `None` means the payload is malformed (a missing id or a type this file does not answer).
///
/// CDXC:Sessions 2026-09-25 WHY:
/// Remote rows and remote groups are answered here too. A remote project's user-made groups live in
/// THIS computer's document under `remote:<machine>:project:<id>`, and the store reads them there
/// (`PresentationStore::user_groups_of_project`), so the edit is the same edit. The reason they used
/// to go to the old runtime was its half of `createGroupFromSession`: it makes the new group its
/// `activeGroupId` without making the project active, and its remote tab list and the next click's
/// `keepView` are read from that id (`RuntimeActiveGroup`). The runtime's own `focusGroup` with the
/// user-made group's id does exactly that, so the host sends it for a remote `ActivateSubgroup`
/// (gx_store/sidebar_drag.rs) and nothing about a remote row needs the runtime's handler any more.
/// Supersedes the 2026-09-21 note that said no message could set that group.
pub fn plan_order_write(
    document: &WorkspaceGroupsDocument,
    message: &Value,
) -> Option<OrderWritePlan> {
    match message.get("type").and_then(Value::as_str)? {
        "syncSessionOrder" => plan_sync_session_order(document, message),
        "moveSessionToGroup" => plan_move_session_to_group(document, message),
        "createGroupFromSession" => plan_create_group_from_session(document, message),
        _ => None,
    }
}

/// The `case 'syncSessionOrder'` arm, which asks the group id which writer it means.
fn plan_sync_session_order(
    document: &WorkspaceGroupsDocument,
    message: &Value,
) -> Option<OrderWritePlan> {
    let group_id = message.get("groupId")?.as_str()?;
    let session_ids: Vec<&str> = message
        .get("sessionIds")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if let Some((project, subgroup_id)) = parse_workspace_subgroup_id(group_id) {
        // `parseGxserverPresentationProjectSessionId` then `reference.projectId === projectId`: an
        // id of another project is dropped rather than carried into the group. That parser reads
        // THIS computer's session ids only, so a remote group's order names no session and the
        // edit keeps the group's order, still writing the document as the TypeScript did.
        let raw_ids: Vec<String> = session_ids
            .iter()
            .filter_map(|id| SessionKey::parse_sidebar_session_id(id))
            .filter(|key| key.machine.is_local() && key.project_key() == project)
            .map(|key| key.session_id)
            .collect();
        return Some(
            match document.sync_subgroup_session_order(
                &project.to_workspace_project_id(),
                &subgroup_id,
                &raw_ids,
            ) {
                // The one identity return this path has: the group is gone, so nothing is written.
                None => OrderWritePlan::refused("subgroupMissing"),
                Some(document) => OrderWritePlan {
                    writes: vec![OrderWrite::EditDocument { document }],
                    refusal: None,
                },
            },
        );
    }
    // `parseGxserverPresentationProjectGroupId`: a project group, and nothing else. The machine's
    // Chats group and a remote group both fall out here with no write, which is what the
    // TypeScript's `if (!projectId) return` does.
    let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
        return Some(OrderWritePlan::refused("notAProjectGroup"));
    };
    // `syncSessionOrder` keeps no order for a remote project's own group.
    if !project.machine.is_local() {
        return Some(OrderWritePlan::refused("remoteProjectGroup"));
    }
    let raw_ids: Vec<String> = session_ids
        .iter()
        .filter_map(|id| SessionKey::parse_sidebar_session_id(id))
        .filter(|key| key.project_key() == project)
        .map(|key| key.session_id)
        .collect();
    // `if (gxserverSessionIds.length === 0) return`: an order with nothing of this project in it
    // is not sent, so a group of browser rows alone calls nothing.
    if raw_ids.is_empty() {
        return Some(OrderWritePlan::refused("noSessionsOfThisProject"));
    }
    Some(OrderWritePlan {
        writes: vec![OrderWrite::SessionOrderCall {
            project,
            session_ids: raw_ids,
        }],
        refusal: None,
    })
}

/// `moveSessionToWorkspaceGroup`.
fn plan_move_session_to_group(
    document: &WorkspaceGroupsDocument,
    message: &Value,
) -> Option<OrderWritePlan> {
    let session = SessionKey::parse_sidebar_session_id(message.get("sessionId")?.as_str()?)?;
    let group_id = message.get("groupId")?.as_str()?;
    let target_index = message.get("targetIndex").and_then(Value::as_i64);
    let project = session.project_key();
    let workspace_project_id = project.to_workspace_project_id();
    let target_group = match parse_workspace_subgroup_id(group_id) {
        Some((group_project, subgroup_id)) => {
            // `if (subgroup.projectId !== reference.projectId) return`: a session cannot be dropped
            // into another project's group.
            if group_project != project {
                return Some(OrderWritePlan::refused("subgroupOfAnotherProject"));
            }
            Some(subgroup_id)
        }
        None => {
            let Some(group_project) = ProjectKey::parse_sidebar_group_id(group_id) else {
                return Some(OrderWritePlan::refused("notAProjectGroup"));
            };
            if group_project != project {
                return Some(OrderWritePlan::refused("projectGroupOfAnotherProject"));
            }
            // A drop on the project's own group takes the session OUT of whatever user-made group
            // holds it, which is what `targetGroupId: undefined` means.
            None
        }
    };
    Some(
        match document.move_session_to_subgroup(
            &workspace_project_id,
            &session.session_id,
            target_group.as_deref(),
            target_index,
        ) {
            None => OrderWritePlan::refused("subgroupMissing"),
            Some(document) => OrderWritePlan {
                writes: vec![OrderWrite::EditDocument { document }],
                refusal: None,
            },
        },
    )
}

/// `createWorkspaceGroupFromSession`: a new group holding the one session, then the project and
/// that group become active.
fn plan_create_group_from_session(
    document: &WorkspaceGroupsDocument,
    message: &Value,
) -> Option<OrderWritePlan> {
    let session = SessionKey::parse_sidebar_session_id(message.get("sessionId")?.as_str()?)?;
    let project = session.project_key();
    let Some((group_id, document)) = document.create_subgroup(
        &project.to_workspace_project_id(),
        Some(session.session_id.as_str()),
    ) else {
        // The limit is reached: a toast, and NOTHING is written. The activation does not happen
        // either, which is why the toast is the whole plan rather than a follow-up.
        return Some(OrderWritePlan {
            writes: vec![OrderWrite::Toast {
                level: ToastLevel::Info,
                title: "Group limit reached for this project.".to_string(),
            }],
            refusal: None,
        });
    };
    Some(OrderWritePlan {
        writes: vec![
            OrderWrite::EditDocument { document },
            OrderWrite::ActivateSubgroup { project, group_id },
        ],
        refusal: None,
    })
}
