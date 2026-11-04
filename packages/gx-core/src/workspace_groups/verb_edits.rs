//! The session group edits the `ghostex` group verbs ask for (`ghostex create-session-group` and the
//! rest, through the `sessionGroups` renderer command), made on the document the app owns with the
//! same edits a sidebar gesture makes.
//!
//! CDXC:Sessions 2026-10-25 WHY:
//! The document is the client's and gxserver's copy is its write-through, so a CLI edit written to that copy was lost whenever the app had a push outstanding (a drag, a rename, or the prune after a grouped session closed): the app ignores echoes while it pushes and then writes its own document over them. The verbs therefore ask the app, which applies the edit to the document it is about to push. A session joins only a group of its own project.
//! SEE-ALSO: server/src/ghostex_cli/session_groups.rs, apps/desktop/src/app/gx_store/renderer_commands/perform.rs, server/src/workspace_groups.rs (the copy's limits).

use serde_json::{json, Value};

use super::document::WorkspaceGroupsDocument;
use crate::keys::{encode_workspace_subgroup_id, ProjectKey};
use crate::renderer_commands::RendererCommandError;

/// gxserver keeps at most this many members per group and this many projects in its copy
/// (`MAX_SESSION_IDS_PER_GROUP`, `MAX_PROJECT_ENTRIES` in server/src/workspace_groups.rs) and drops
/// the rest when it normalizes a push, so an edit past them is refused rather than lost.
const SESSION_GROUP_MAX_MEMBERS: usize = 1024;
const SESSION_GROUP_MAX_PROJECTS: usize = 512;

/// What one group verb asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionGroupOp {
    List,
    /// Whether the group exists and can take one more session, before a create that joins it.
    Check {
        group_id: String,
    },
    Create {
        title: Option<String>,
    },
    Rename {
        group_id: String,
        title: String,
    },
    /// CDXC:Sessions 2026-10-25 DECISION: Q569, on what deleting a group from the CLI does with its sessions. User: "close", and "they have to appear in history tho at the top pls so i can restore them". `delete-session-group` is the sidebar's Close Group: the app closes every member through its ordinary close and removes the group, and History lists closed sessions newest first, so the ones it can restore come first.
    Delete {
        group_id: String,
    },
    /// `group_id` `None` puts the session back in its project's own list.
    Move {
        session_id: String,
        group_id: Option<String>,
    },
}

/// A group verb and the project it acts in (`None` only for a list of every project).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGroupRequest {
    pub project_id: Option<String>,
    pub op: SessionGroupOp,
}

/// The document after the edit (`None` when nothing changed) and the group the verb made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGroupOutcome {
    pub document: Option<WorkspaceGroupsDocument>,
    pub group_id: Option<String>,
    /// A delete: the sidebar id (`gpui-wsg:…`) of the group the app closes with Close Group, which
    /// writes the document itself.
    pub close_group: Option<String>,
}

impl WorkspaceGroupsDocument {
    /// Applies one group verb. Validation of the project and the session against the store is the
    /// renderer command planner's (`renderer_commands/verbs.rs`).
    ///
    /// An edit that would change nothing is answered before the document is copied, so a repeated
    /// verb costs no copy of the whole document on the app's main thread.
    pub fn apply_session_group_request(
        &self,
        request: &SessionGroupRequest,
    ) -> Result<SessionGroupOutcome, RendererCommandError> {
        let unchanged = |group_id: Option<&String>| {
            Ok(SessionGroupOutcome {
                document: None,
                group_id: group_id.cloned(),
                close_group: None,
            })
        };
        let Some(project_id) = request.project_id.as_deref() else {
            return match request.op {
                SessionGroupOp::List => unchanged(None),
                _ => Err(RendererCommandError::NoMatchingProject),
            };
        };
        let groups = self
            .projects
            .get(project_id)
            .map(|project| project.groups.as_slice())
            .unwrap_or_default();
        let group = |group_id: &str| {
            groups
                .iter()
                .find(|group| group.group_id == group_id)
                .ok_or(RendererCommandError::NoMatchingSessionGroup)
        };
        let (next, group_id) = match &request.op {
            SessionGroupOp::List => return unchanged(None),
            SessionGroupOp::Check { group_id } => {
                if group(group_id)?.session_ids.len() >= SESSION_GROUP_MAX_MEMBERS {
                    return Err(RendererCommandError::SessionGroupFull);
                }
                return unchanged(Some(group_id));
            }
            SessionGroupOp::Create { title } => {
                if !self.projects.contains_key(project_id)
                    && self.projects.len() >= SESSION_GROUP_MAX_PROJECTS
                {
                    return Err(RendererCommandError::SessionGroupLimit);
                }
                let (group_id, created) = self
                    .create_subgroup_titled(project_id, None, title.as_deref())
                    .ok_or(RendererCommandError::SessionGroupLimit)?;
                (created, Some(group_id))
            }
            SessionGroupOp::Rename { group_id, title } => {
                if group(group_id)?.title == *title {
                    return unchanged(None);
                }
                let renamed = self
                    .rename_subgroup(project_id, group_id, title)
                    .ok_or(RendererCommandError::InvalidTitle)?;
                (renamed, None)
            }
            SessionGroupOp::Delete { group_id } => {
                group(group_id)?;
                return Ok(SessionGroupOutcome {
                    document: None,
                    group_id: Some(group_id.clone()),
                    close_group: Some(encode_workspace_subgroup_id(
                        &ProjectKey::local(project_id),
                        group_id,
                    )),
                });
            }
            SessionGroupOp::Move {
                session_id,
                group_id: Some(group_id),
            } => {
                let target = group(group_id)?;
                // Already there: it keeps its place in the group.
                if target.session_ids.contains(session_id) {
                    return unchanged(Some(group_id));
                }
                if target.session_ids.len() >= SESSION_GROUP_MAX_MEMBERS {
                    return Err(RendererCommandError::SessionGroupFull);
                }
                let moved = self
                    .move_session_to_subgroup(project_id, session_id, Some(group_id), None)
                    .ok_or(RendererCommandError::NoMatchingSessionGroup)?;
                (moved, Some(group_id.clone()))
            }
            SessionGroupOp::Move {
                session_id,
                group_id: None,
            } => {
                if !groups
                    .iter()
                    .any(|group| group.session_ids.contains(session_id))
                {
                    return unchanged(None);
                }
                let moved = self
                    .move_session_to_subgroup(project_id, session_id, None, None)
                    .ok_or(RendererCommandError::NoMatchingSessionGroup)?;
                (moved, None)
            }
        };
        Ok(SessionGroupOutcome {
            document: Some(next),
            group_id,
            close_group: None,
        })
    }

    /// The groups `ghostex session-groups` prints: every project's, or one project's, in order.
    /// Groups this app keeps for another computer's projects (`remote:<machine>:project:<id>`) are
    /// left out, since the verbs act only on this computer's projects.
    pub fn session_groups_json(&self, project_id: Option<&str>) -> Value {
        let groups: Vec<Value> = self
            .projects
            .iter()
            .filter(|(id, _)| {
                ProjectKey::parse_workspace_project_id(id)
                    .is_some_and(|project| project.machine.is_local())
            })
            .filter(|(id, _)| project_id.is_none_or(|wanted| wanted == id.as_str()))
            .flat_map(|(id, project)| {
                project.groups.iter().map(move |group| {
                    json!({
                        "projectId": id,
                        "groupId": group.group_id,
                        "title": group.title,
                        "sessionIds": group.session_ids,
                    })
                })
            })
            .collect();
        Value::Array(groups)
    }
}
