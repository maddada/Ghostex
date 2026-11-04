//! Session notes is a built-in extension (`sessionNotesHidden`, off by default): while it is off a
//! row offers no Note, neither in its menu nor as a hover button, and the note itself stays saved.

use serde_json::{json, Value};

use super::group::MenuGroup;
use super::item::MenuItem;
use super::session::{session_hover_actions, session_menu, SessionMenuInput};
use super::AgentChoicePlace;
use crate::sidebar_view::tags::TagCatalog;
use crate::sidebar_view::view::{SessionMenuFacts, SessionRow};
use crate::SidebarSettings;

fn row() -> SessionRow {
    SessionRow {
        sidebar_session_id: "P1:S1".to_string(),
        activity: "idle".to_string(),
        lifecycle_state: "running".to_string(),
        menu_facts: SessionMenuFacts {
            agent_name: Some("claude".to_string()),
            agent_session_id: Some("conversation-1".to_string()),
            ..SessionMenuFacts::default()
        },
        session_note: Some("Check the migration next".to_string()),
        ..SessionRow::default()
    }
}

fn group() -> MenuGroup<'static> {
    MenuGroup {
        group_id: "P1",
        storage_id: "P1",
        title: "Project",
        is_remote: false,
        remote_machine_name: None,
        is_stale: false,
        can_create_session_group: false,
        can_focus_mode: false,
        project: None,
        can_remove_project: false,
        git_remote_origin_url: None,
        server_id: None,
        agent_place: AgentChoicePlace::default(),
    }
}

fn settings(saved: Value) -> SidebarSettings {
    SidebarSettings::from_settings_json(&saved, Default::default())
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

fn menu_has_note(settings: &SidebarSettings) -> bool {
    let row = row();
    let group = group();
    let catalog = TagCatalog::default();
    let input = SessionMenuInput {
        row: &row,
        group: &group,
        settings,
        catalog: &catalog,
        below: &[],
        has_inactive_threads: false,
        now_ms: 0,
        group_moves: None,
    };
    labels(&session_menu(&input))
        .iter()
        .any(|label| label == "Note")
}

fn hover_has_note(settings: &SidebarSettings) -> bool {
    let row = row();
    let group = group();
    let catalog = TagCatalog::default();
    let input = SessionMenuInput {
        row: &row,
        group: &group,
        settings,
        catalog: &catalog,
        below: &[],
        has_inactive_threads: false,
        now_ms: 0,
        group_moves: None,
    };
    let actions = session_hover_actions(&input);
    actions
        .hover_before
        .iter()
        .chain(&actions.hover_after)
        .any(|item| item.label.as_deref() == Some("Note"))
}

#[test]
fn the_menu_offers_no_note_by_default() {
    assert!(!settings(json!({})).session_notes_enabled);
    assert!(!menu_has_note(&settings(json!({}))));
    assert!(!menu_has_note(&settings(
        json!({ "sessionNotesHidden": true })
    )));
}

#[test]
fn the_menu_offers_note_while_session_notes_is_on() {
    assert!(menu_has_note(&settings(
        json!({ "sessionNotesHidden": false })
    )));
}

#[test]
fn a_note_hover_button_shows_only_while_session_notes_is_on() {
    let hover = json!([{ "id": "note", "enabled": true }, { "id": "close", "enabled": true }]);
    assert!(hover_has_note(&settings(json!({
        "sessionCardHoverButtons": hover,
        "sessionNotesHidden": false,
    }))));
    assert!(!hover_has_note(&settings(json!({
        "sessionCardHoverButtons": hover,
        "sessionNotesHidden": true,
    }))));
    // Turned off, the saved strip still lists the note button for when it comes back.
    let off = settings(json!({ "sessionCardHoverButtons": hover, "sessionNotesHidden": true }));
    assert_eq!(off.session_card_hover_buttons, hover);
}
