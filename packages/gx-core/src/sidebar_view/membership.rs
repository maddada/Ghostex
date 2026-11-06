//! Which sessions belong to which sidebar group: a project's own list and its user-made groups.
//!
//! SEE-ALSO: packages/shared/gxserver-presentation-sidebar-projection.ts (deleted 2026-10-01)
//! (`createGxserverPresentationSessionsByProjectFromGroups`). `spliceWorkspaceSubgroups` was ported
//! from the deleted `gxserver-runtime/sidebar-groups.ts` (see git history).

use std::collections::BTreeSet;

use crate::keys::{encode_workspace_subgroup_id, MachineId, ProjectKey};
use crate::presentation_store::{MachinePresentation, PresentationStore};

use super::text::js_trim;

/// One user-made session group of a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubgroupMembers {
    pub(crate) sidebar_group_id: String,
    pub(crate) title: String,
    pub(crate) session_ids: Vec<String>,
}

/// What one project contributes to the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectMembers {
    /// The project's own rows, in the daemon's order, without the ones a user-made group took.
    pub(crate) session_ids: Vec<String>,
    pub(crate) subgroups: Vec<SubgroupMembers>,
}

/// The user-made groups of a project, as the local machine's document holds them.
pub(crate) fn subgroups_of_project(
    store: &PresentationStore,
    project: &ProjectKey,
) -> Vec<(String, String, Vec<String>)> {
    store
        .user_groups_of_project(project)
        .iter()
        .filter(|group| !group.group_id.is_empty())
        .map(|group| {
            let title = if js_trim(&group.title).is_empty() {
                group.group_id.clone()
            } else {
                group.title.clone()
            };
            let session_ids = group
                .session_ids
                .iter()
                .filter(|session_id| !session_id.is_empty())
                .cloned()
                .collect();
            (group.group_id.clone(), title, session_ids)
        })
        .collect()
}

/// The rows of one project, and the user-made groups that took some of them.
pub(crate) fn project_members(
    store: &PresentationStore,
    machine: &MachinePresentation,
    machine_id: &MachineId,
    project_id: &str,
) -> ProjectMembers {
    let Some(loaded) = machine.loaded() else {
        return ProjectMembers::default();
    };
    let project = ProjectKey {
        machine: machine_id.clone(),
        project_id: project_id.to_string(),
    };
    let mut members = ProjectMembers::default();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    for (group_id, title, session_ids) in subgroups_of_project(store, &project) {
        // Every member of a user-made group, whether or not its row still exists, is kept out of
        // the project's own list, as `collectWorkspaceSubgroupSessionKeys` does.
        claimed.extend(session_ids.iter().cloned());
        members.subgroups.push(SubgroupMembers {
            sidebar_group_id: encode_workspace_subgroup_id(&project, &group_id),
            title,
            // The project's own list test too: a closed pinned/favorite/tagged session stays in the snapshot.
            session_ids: session_ids
                .into_iter()
                .filter(|session_id| {
                    machine
                        .effective_session(project_id, session_id)
                        .is_some_and(|session| session.listed_in_sidebar())
                })
                .collect(),
        });
    }
    for group in loaded
        .groups()
        .iter()
        .filter(|group| group.project_id == project_id)
    {
        for session_id in &group.session_ids {
            let listed = machine
                .effective_session(project_id, session_id)
                .is_some_and(|session| session.listed_in_sidebar());
            if listed && !claimed.contains(session_id) {
                members.session_ids.push(session_id.clone());
            }
        }
    }
    members
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::core::Core;

    fn session(id: &str, lifecycle: &str, visible: bool) -> serde_json::Value {
        json!({
            "sessionId": id,
            "projectId": "scratch",
            "groupId": "scratch:active",
            "kind": "agent",
            "surface": "workspace",
            "title": id,
            "activity": "idle",
            "lifecycleState": lifecycle,
            "isPinned": true,
            "sortKey": format!("1:{id}"),
            "visibleInSidebarByDefault": visible,
            "createdAt": "2026-11-05T01:00:00.000Z",
            "updatedAt": "2026-11-05T01:00:00.000Z",
        })
    }

    #[test]
    fn a_closed_pinned_session_leaves_its_user_made_group() {
        let project = ProjectKey::local("scratch".to_string());
        let snapshot = json!({
            "type": "presentationSnapshot",
            "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": "local",
            "revision": 1,
            "snapshot": {
                "revision": 1,
                "generatedAt": "2026-11-05T01:00:00.000Z",
                "projects": [{
                    "projectId": "scratch",
                    "title": "Scratch",
                    "path": "/Users/me/scratch",
                    "pathState": "available",
                    "groupIds": ["scratch:active"],
                    "sortKey": "1:scratch",
                    "createdAt": "2026-11-05T01:00:00.000Z",
                    "updatedAt": "2026-11-05T01:00:00.000Z",
                }],
                "groups": [{
                    "projectId": "scratch",
                    "groupId": "scratch:active",
                    "sessionIds": ["live", "closed"],
                }],
                "workspaceGroups": {
                    "projects": {
                        project.to_workspace_project_id(): {
                            "groups": [{"groupId": "g1", "title": "Folder", "sessionIds": ["live", "closed"]}],
                        },
                    },
                },
                // gxserver keeps a closed pinned session in the snapshot, not visible by default.
                "sessions": [session("live", "running", true), session("closed", "stopped", false)],
            }
        });
        let mut core = Core::new();
        core.handle_raw_frame(MachineId::Local, &snapshot.to_string(), 0)
            .expect("the frame parses");
        let store = core.presentation();
        let machine = store.machine(&MachineId::Local).expect("loaded");

        let members = project_members(store, machine, &MachineId::Local, "scratch");

        assert_eq!(members.subgroups.len(), 1);
        assert_eq!(members.subgroups[0].session_ids, vec!["live".to_string()]);
        assert!(
            members.session_ids.is_empty(),
            "group members stay out of the project's own list"
        );
    }
}
