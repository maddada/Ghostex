//! A session row dragged onto another project: a session cannot change project (it is known
//! everywhere by `projectId:sessionId`), so that drop hands it over to a new session there.
//!
//! CDXC:Sidebar 2026-10-31 DECISION:
//! User: Sam on Discord starts chats outside a project and makes the project later, and "I can handover when you drag... after a message to confirm". User (Q865): dragging stays a Project sidebar feature; the Unified sidebar reaches the same thing through Handoff / Export in a card's menu. This check runs BEFORE `plan_session_move`, whose refusals (`pinnedAcrossGroups`, `targetOfAnotherProject`) would otherwise answer first and a pinned row could never hand over. A drop inside the row's own project is not this file's and keeps every rule it had.
//! Only a session that has a conversation hands over: a draft has none, a terminal has no agent, and a coordinator and its threads belong to their tree. A row on another computer is not picked up in this sidebar at all (`can_drag` in native_sidebar/sessions.rs), and a project on another computer is not on screen here, so both stay out; Handoff / Export covers them.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/session_drop.rs (the caller),
//! packages/gx-core/src/session_handoff.rs (the steps and the draft),
//! apps/desktop/src/app/native_sidebar/drag.rs (the highlight and the confirm).

use serde_json::Value;

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, ProjectKey, SessionKey};
use crate::selectors::QUICK_AUTOMATIONS_PROJECT_ID;
use crate::sidebar_view::SidebarInputs;

use super::inventory::group_by_id;

/// Set on a `moveSession` from the empty space under the last block, which only ever means "the
/// last of my own project's groups": a release there never starts a handoff.
pub const SESSION_DROP_ZONE_LIST_END: &str = "listEnd";

/// A drop that would hand a session over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionHandoffDrop {
    pub session: SessionKey,
    pub target: ProjectKey,
    pub target_title: String,
    /// The user-made group of the target project the new session joins.
    pub subgroup: Option<String>,
    /// The header that lights up: the project's, or that group's.
    pub highlight_group_id: String,
}

/// What the check says about a drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionHandoffCheck {
    /// Not a drop across projects: the ordinary move rules answer it.
    NotAcross,
    /// Across projects, but this row cannot hand over, so nothing lights up and nothing happens.
    Refused(&'static str),
    Handoff(SessionHandoffDrop),
}

/// Answers a `moveSession` drop before any move rule does.
pub fn check_session_handoff(
    core: &Core,
    inputs: &SidebarInputs,
    command: &Value,
) -> SessionHandoffCheck {
    use SessionHandoffCheck::{Handoff, NotAcross, Refused};
    let text = |key: &str| command.get(key).and_then(Value::as_str);
    if text("type") != Some("moveSession") || text("zone") == Some(SESSION_DROP_ZONE_LIST_END) {
        return NotAcross;
    }
    let (Some(session_id), Some(group_id)) = (text("sessionId"), text("groupId")) else {
        return NotAcross;
    };
    let Some(target) = group_by_id(core, inputs, group_id).and_then(|group| group.project) else {
        return NotAcross;
    };
    let Some(session) = SessionKey::parse_sidebar_session_id(session_id) else {
        // A browser row has no session key.
        return Refused("notASession");
    };
    if session.project_key() == target {
        return NotAcross;
    }
    if !session.machine.is_local() || session.machine != target.machine {
        return Refused("otherComputer");
    }
    let store = core.presentation();
    let Some(row) = store.session(&session) else {
        return Refused("sessionNotListed");
    };
    let has_agent = row
        .agent_id
        .as_deref()
        .is_some_and(|agent| !agent.trim().is_empty());
    if !has_agent {
        return Refused("terminal");
    }
    if row.is_draft {
        return Refused("draft");
    }
    if row.coordinator_role.as_deref() == Some("coordinator") {
        return Refused("coordinator");
    }
    if row.coordinator_session_id.is_some() {
        return Refused("thread");
    }
    let Some(project) = store
        .loaded(&target.machine)
        .and_then(|loaded| loaded.project(&target.project_id))
    else {
        return Refused("targetNotListed");
    };
    if project.bot_profile.is_some() || target.project_id == QUICK_AUTOMATIONS_PROJECT_ID {
        return Refused("targetTakesNoHandoff");
    }
    let target_title = project.title.clone();
    Handoff(SessionHandoffDrop {
        subgroup: parse_workspace_subgroup_id(group_id).map(|(_, subgroup)| subgroup),
        highlight_group_id: group_id.to_string(),
        session,
        target,
        target_title,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::keys::MachineId;
    use crate::sidebar_drag::{plan_session_drop, plan_session_move};
    use crate::sidebar_view::SidebarViewModel;

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

    fn session(project: &str, id: &str, extra: Value) -> Value {
        let mut session = json!({
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
            "sortKey": format!("1:{id}"),
            "visibleInSidebarByDefault": true,
            "createdAt": "2026-10-27T01:00:00.000Z",
            "updatedAt": "2026-10-27T01:00:00.000Z",
        });
        session
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        session
    }

    /// Two projects: Scratch holds one session of each kind, and acme is new and empty.
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
                    project("scratch", json!({})),
                    project("acme", json!({})),
                    project("bot", json!({"botProfile": "helper"})),
                ],
                "groups": [],
                "workspaceGroups": {
                    "projects": {
                        ProjectKey::local("acme".to_string()).to_workspace_project_id(): {
                            "groups": [{"groupId": "g1", "title": "Marketing", "sessionIds": []}],
                        },
                    },
                },
                "sessions": [
                    session("scratch", "plain", json!({})),
                    session("scratch", "pinned", json!({"isPinned": true})),
                    session("scratch", "draft", json!({"isDraft": true})),
                    session("scratch", "terminal", json!({"kind": "terminal", "agentId": null})),
                    session("scratch", "lead", json!({"coordinatorRole": "coordinator"})),
                    session("scratch", "thread", json!({
                        "coordinatorProjectId": "scratch",
                        "coordinatorSessionId": "lead",
                    })),
                ],
            }
        });
        let mut core = Core::new();
        core.handle_raw_frame(MachineId::Local, &snapshot.to_string(), 0)
            .expect("the frame parses");
        core
    }

    fn sidebar_id(project: &str, session: &str) -> String {
        SessionKey {
            machine: MachineId::Local,
            project_id: project.to_string(),
            session_id: session.to_string(),
        }
        .to_sidebar_session_id()
    }

    fn group(project: &str) -> String {
        ProjectKey::local(project.to_string()).to_sidebar_group_id()
    }

    fn drop_on(session_id: &str, group_id: &str) -> Value {
        json!({
            "type": "moveSession",
            "sessionId": sidebar_id("scratch", session_id),
            "groupId": group_id,
            "position": "before",
        })
    }

    fn check(core: &Core, command: &Value) -> SessionHandoffCheck {
        check_session_handoff(core, &SidebarInputs::default(), command)
    }

    #[test]
    fn a_session_dropped_on_another_project_hands_over_even_to_an_empty_one() {
        let core = seeded();
        let SessionHandoffCheck::Handoff(drop) = check(&core, &drop_on("plain", &group("acme")))
        else {
            panic!("a handoff");
        };
        assert_eq!(drop.target, ProjectKey::local("acme".to_string()));
        assert_eq!(drop.target_title, "Title acme");
        assert_eq!(drop.highlight_group_id, group("acme"));
        assert_eq!(drop.subgroup, None);
    }

    #[test]
    fn the_check_answers_before_the_move_rules_refuse() {
        let core = seeded();
        let inputs = SidebarInputs::default();
        // The move rules alone refuse both of these drops.
        for (session, reason) in [
            ("plain", "targetOfAnotherProject"),
            ("pinned", "pinnedAcrossGroups"),
        ] {
            let command = drop_on(session, &group("acme"));
            let plan = plan_session_move(&core, &inputs, &command).expect("answered");
            assert_eq!(plan.refusal, Some(reason));
            assert!(plan.messages.is_empty());
            // The drop itself is a handoff, with no move message and no drop line.
            let drop = plan_session_drop(&core, &inputs, &SidebarViewModel::new(), &command, 0)
                .expect("a handoff, not a refusal");
            assert!(drop.handoff.is_some(), "{session}");
            assert!(drop.messages.is_empty());
            assert_eq!(drop.landing, None);
        }
    }

    #[test]
    fn a_new_session_joins_the_session_group_it_was_dropped_on() {
        let core = seeded();
        let subgroup = format!(
            "gpui-wsg:{}:g1",
            ProjectKey::local("acme".to_string()).to_workspace_project_id()
        );
        let SessionHandoffCheck::Handoff(drop) = check(&core, &drop_on("plain", &subgroup)) else {
            panic!("a handoff");
        };
        assert_eq!(drop.target, ProjectKey::local("acme".to_string()));
        assert_eq!(drop.subgroup.as_deref(), Some("g1"));
        assert_eq!(drop.highlight_group_id, subgroup);
    }

    #[test]
    fn rows_with_no_conversation_to_carry_are_refused() {
        let core = seeded();
        let inputs = SidebarInputs::default();
        for (session, reason) in [
            ("draft", "draft"),
            ("terminal", "terminal"),
            ("lead", "coordinator"),
            ("thread", "thread"),
        ] {
            let command = drop_on(session, &group("acme"));
            assert_eq!(check(&core, &command), SessionHandoffCheck::Refused(reason));
            // Refused means nothing lights up and a release does nothing.
            assert_eq!(
                plan_session_drop(&core, &inputs, &SidebarViewModel::new(), &command, 0),
                None,
                "{session}"
            );
        }
    }

    #[test]
    fn a_browser_row_is_refused() {
        let core = seeded();
        let command = json!({
            "type": "moveSession",
            "sessionId": "gpui-browser:scratch:tab",
            "groupId": group("acme"),
            "position": "before",
        });
        assert_eq!(
            check(&core, &command),
            SessionHandoffCheck::Refused("notASession")
        );
    }

    #[test]
    fn a_bot_project_takes_no_handoff() {
        let core = seeded();
        assert_eq!(
            check(&core, &drop_on("plain", &group("bot"))),
            SessionHandoffCheck::Refused("targetTakesNoHandoff")
        );
    }

    #[test]
    fn a_drop_inside_its_own_project_is_left_to_the_move_rules() {
        let core = seeded();
        assert_eq!(
            check(&core, &drop_on("plain", &group("scratch"))),
            SessionHandoffCheck::NotAcross
        );
        let section = json!({
            "type": "moveSessionToSection",
            "sessionId": sidebar_id("scratch", "plain"),
            "groupId": group("acme"),
            "from": "sessions",
            "section": "pinned",
        });
        assert_eq!(check(&core, &section), SessionHandoffCheck::NotAcross);
    }

    #[test]
    fn the_empty_space_under_the_list_never_starts_a_handoff() {
        let core = seeded();
        let inputs = SidebarInputs::default();
        let mut command = drop_on("plain", &group("acme"));
        command["zone"] = json!(SESSION_DROP_ZONE_LIST_END);
        assert_eq!(check(&core, &command), SessionHandoffCheck::NotAcross);
        assert_eq!(
            plan_session_drop(&core, &inputs, &SidebarViewModel::new(), &command, 0),
            None
        );
    }

    #[test]
    fn a_section_heading_of_its_own_project_starts_nothing() {
        let core = seeded();
        let inputs = SidebarInputs::default();
        let mut command = drop_on("plain", &group("scratch"));
        command["handoffOnly"] = json!(true);
        assert_eq!(
            plan_session_drop(&core, &inputs, &SidebarViewModel::new(), &command, 0),
            None
        );
    }
}
