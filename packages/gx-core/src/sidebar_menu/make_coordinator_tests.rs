//! Make Coordinator sits in a session's Advanced submenu on this computer's rows and on a
//! connected computer's rows alike (CDXC:Coordinators 2026-11-07 in session.rs).

use serde_json::json;

use super::group::MenuGroup;
use super::item::MenuItem;
use super::session::{session_menu, SessionMenuInput};
use super::AgentChoicePlace;
use crate::sidebar_view::tags::TagCatalog;
use crate::sidebar_view::view::{SessionMenuFacts, SessionRow};
use crate::SidebarSettings;

fn row(agent: &str) -> SessionRow {
    SessionRow {
        sidebar_session_id: "P1:S1".to_string(),
        activity: "idle".to_string(),
        lifecycle_state: "running".to_string(),
        menu_facts: SessionMenuFacts {
            agent_name: Some(agent.to_string()),
            agent_session_id: Some("conversation-1".to_string()),
            ..SessionMenuFacts::default()
        },
        ..SessionRow::default()
    }
}

fn group(is_remote: bool) -> MenuGroup<'static> {
    MenuGroup {
        group_id: "P1",
        storage_id: "P1",
        title: "Project",
        is_remote,
        remote_machine_name: is_remote.then_some("Mac mini"),
        is_stale: false,
        can_create_session_group: false,
        can_focus_mode: false,
        project: None,
        can_remove_project: false,
        git_remote_origin_url: None,
        server_id: is_remote.then(|| "S9".to_string()),
        agent_place: AgentChoicePlace::default(),
    }
}

fn labels(items: &[MenuItem]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        out.extend(item.label.clone());
        if let Some(children) = &item.children {
            out.extend(labels(children));
        }
    }
    out
}

fn offers_make_coordinator(row: &SessionRow, is_remote: bool) -> bool {
    let group = group(is_remote);
    let settings = SidebarSettings::from_settings_json(&json!({}), Default::default());
    let catalog = TagCatalog::default();
    let input = SessionMenuInput {
        row,
        group: &group,
        settings: &settings,
        catalog: &catalog,
        below: &[],
        has_inactive_threads: false,
        now_ms: 0,
        group_moves: None,
    };
    labels(&session_menu(&input))
        .iter()
        .any(|label| label == "Make Coordinator")
}

#[test]
fn a_local_claude_session_offers_make_coordinator() {
    assert!(offers_make_coordinator(&row("claude"), false));
}

#[test]
fn a_remote_computers_claude_session_offers_make_coordinator() {
    assert!(offers_make_coordinator(&row("claude"), true));
}

#[test]
fn a_remote_coordinator_is_not_offered_it_again() {
    let mut coordinator = row("claude");
    coordinator.is_coordinator = true;
    assert!(!offers_make_coordinator(&coordinator, true));
}
