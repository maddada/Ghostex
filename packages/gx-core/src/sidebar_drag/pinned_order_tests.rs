//! Regression tests for pinned rows drawn together from several projects
//! (CDXC:Sidebar 2026-11-07 WHY in pinned_order.rs).

use serde_json::{json, Value};

use super::plan_pinned_reorder;
use crate::change::ChangeSummary;
use crate::core::Core;
use crate::keys::{MachineId, ProjectKey, SessionKey};
use crate::sidebar_drag::plan_session_drop;
use crate::sidebar_view::{PinnedSessionOrder, SectionId, SidebarInputs, SidebarViewModel};

fn project(id: &str, extra: Value) -> Value {
    let mut project = json!({
        "projectId": id,
        "title": format!("Title {id}"),
        "path": format!("/Users/me/{id}"),
        "pathState": "available",
        "groupIds": [],
        "sortKey": format!("1:{id}"),
        "createdAt": "2026-10-27T01:00:00.000Z",
        "updatedAt": "2026-10-27T01:00:00.000Z",
    });
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    project
}

fn session(project: &str, id: &str, order: i64, last_active: &str) -> Value {
    json!({
        "sessionId": id,
        "projectId": project,
        "groupId": format!("{project}:active"),
        "kind": "agent",
        "surface": "workspace",
        "zmxName": format!("S90-{project}-{id}"),
        "title": id,
        "activity": "idle",
        "lifecycleState": "running",
        "agentId": "claude",
        "agentSessionId": format!("conversation-{id}"),
        "sortKey": format!("{order:0>12}:0:0:{id}"),
        "sidebarOrder": order,
        "isPinned": true,
        "visibleInSidebarByDefault": true,
        "createdAt": "2026-10-27T01:00:00.000Z",
        "updatedAt": last_active,
        "lastActiveAt": last_active,
        "lastInteractionAt": last_active,
    })
}

/// `web` with two pinned sessions and its worktree `web-wt` with one, which the Project sidebar
/// lists in web's Pinned (Worktree sessions show in their project, on by default).
fn seeded() -> Core {
    let snapshot = json!({
        "type": "presentationSnapshot",
        "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": "local",
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-10-27T01:00:00.000Z",
            "projects": [
                project("web", json!({})),
                project("web-wt", json!({
                    "worktree": {
                        "parentProjectId": "web",
                        "parentProjectName": "Title web",
                        "parentProjectPath": "/Users/me/web",
                        "name": "wt",
                        "branch": "feature/wt",
                    },
                })),
            ],
            "groups": [],
            "sessions": [
                session("web", "a", 1000, "2026-10-27T01:00:00.000Z"),
                session("web", "b", 2000, "2026-10-27T03:00:00.000Z"),
                session("web-wt", "w", 1500, "2026-10-27T02:00:00.000Z"),
            ],
        }
    });
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &snapshot.to_string(), 0)
        .expect("the frame parses");
    core
}

fn id(project: &str, session: &str) -> String {
    SessionKey {
        machine: MachineId::Local,
        project_id: project.to_string(),
        session_id: session.to_string(),
    }
    .to_sidebar_session_id()
}

fn model(core: &Core, inputs: &SidebarInputs) -> SidebarViewModel {
    let mut model = SidebarViewModel::new();
    model.update(core, inputs, &ChangeSummary::default(), 0);
    model
}

fn drawn_pinned(model: &SidebarViewModel) -> Vec<String> {
    let group_id = ProjectKey::local("web".to_string()).to_sidebar_group_id();
    let group = model
        .view()
        .groups
        .iter()
        .find(|group| group.core.group_id == group_id)
        .expect("web is drawn");
    group
        .core
        .sections
        .iter()
        .find(|section| section.id == SectionId::Pinned)
        .expect("web draws Pinned")
        .session_ids
        .clone()
}

fn drop(moved: &str, target: &str, position: &str) -> Value {
    json!({
        "type": "moveSession",
        "sessionId": moved,
        "groupId": ProjectKey::local("web".to_string()).to_sidebar_group_id(),
        "targetSessionId": target,
        "position": position,
    })
}

#[test]
fn a_folded_worktree_row_interleaves_with_its_parents_pinned_rows_by_their_order() {
    let core = seeded();
    let inputs = SidebarInputs::default();
    assert_eq!(
        drawn_pinned(&model(&core, &inputs)),
        vec![id("web", "a"), id("web-wt", "w"), id("web", "b")]
    );
}

#[test]
fn a_folded_worktree_row_can_be_dragged_among_its_parents_pinned_rows() {
    // Refused before (`worktreeInItsProject`, then `pinnedAcrossGroups`): nothing moved.
    let core = seeded();
    let inputs = SidebarInputs::default();
    let model = model(&core, &inputs);
    let command = drop(&id("web-wt", "w"), &id("web", "a"), "before");
    let planned = plan_session_drop(&core, &inputs, &model, &command, 0).expect("a reorder");
    let landing = planned.landing.expect("a drop line");
    assert_eq!(landing.section, "pinned");
    assert_eq!(landing.before_session_id, Some(id("web", "a")));
    assert_eq!(planned.command["pinnedReorder"], json!(true));
    assert_eq!(
        planned.messages,
        vec![
            json!({"type": "setSessionPinned", "sessionId": id("web-wt", "w"), "pinned": true, "sidebarOrder": 1000}),
            json!({"type": "setSessionPinned", "sessionId": id("web", "a"), "pinned": true, "sidebarOrder": 2000}),
            json!({"type": "setSessionPinned", "sessionId": id("web", "b"), "pinned": true, "sidebarOrder": 3000}),
        ]
    );
}

#[test]
fn a_parents_pinned_row_can_be_dropped_after_a_folded_worktree_row() {
    // Refused before (`hoveredPinnedRowNotInGroup`).
    let core = seeded();
    let inputs = SidebarInputs::default();
    let model = model(&core, &inputs);
    let command = drop(&id("web", "b"), &id("web-wt", "w"), "before");
    let planned = plan_session_drop(&core, &inputs, &model, &command, 0).expect("a reorder");
    assert_eq!(
        planned.landing.expect("a drop line").after_session_id,
        Some(id("web", "a"))
    );
}

#[test]
fn most_recently_updated_orders_pinned_rows_and_refuses_the_drag() {
    let core = seeded();
    let mut inputs = SidebarInputs::default();
    inputs.settings.pinned_session_order = PinnedSessionOrder::Recent;
    let model = model(&core, &inputs);
    assert_eq!(
        drawn_pinned(&model),
        vec![id("web", "b"), id("web-wt", "w"), id("web", "a")]
    );
    let command = drop(&id("web", "a"), &id("web", "b"), "before");
    assert_eq!(plan_session_drop(&core, &inputs, &model, &command, 0), None);
}

#[test]
fn a_drop_where_the_row_already_is_moves_nothing() {
    let rows = vec![("a".to_string(), Some(1000)), ("b".to_string(), Some(2000))];
    assert_eq!(plan_pinned_reorder(&rows, "a", "b", false), None);
    let moved = plan_pinned_reorder(&rows, "b", "a", false).expect("a move");
    assert_eq!(moved.order, vec!["b".to_string(), "a".to_string()]);
    assert_eq!(moved.messages.len(), 2);
}
