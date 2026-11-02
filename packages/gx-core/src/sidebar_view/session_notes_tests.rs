//! A row carries its session's note (the card's note dot and the `Note:` line of its tooltip) only
//! while the Session notes extension is on; the note itself is the daemon's and is never touched.

use ghostex_gx_protocol::PresentationSession;
use serde_json::json;

use super::rows::{session_row, RowContext};
use super::tags::TagCatalog;
use crate::keys::{MachineId, SessionKey};

fn session() -> PresentationSession {
    serde_json::from_value(json!({
        "sessionId": "S1",
        "projectId": "P1",
        "groupId": "P1:active",
        "kind": "agent",
        "surface": "workspace",
        "zmxName": "S90-P1-S1",
        "title": "Migration",
        "activity": "idle",
        "lifecycleState": "running",
        "agentSessionId": "conversation-1",
        "sessionNote": "Check the migration next",
    }))
    .expect("a presentation session")
}

fn row_with(session_notes: bool) -> crate::sidebar_view::view::SessionRow {
    let catalog = TagCatalog::default();
    let context = RowContext {
        catalog: &catalog,
        debugging_mode: false,
        always_show_state_tooltip: false,
        session_notes,
        side_chat_needs: &Default::default(),
    };
    let key = SessionKey {
        machine: MachineId::Local,
        project_id: "P1".to_string(),
        session_id: "S1".to_string(),
    };
    session_row(key, &session(), None, None, &context)
}

#[test]
fn a_row_has_no_note_dot_or_tooltip_line_while_session_notes_is_off() {
    let row = row_with(false);
    assert_eq!(row.session_note, None);
    assert!(
        !row.title_tooltip.contains("Note:"),
        "{}",
        row.title_tooltip
    );
}

#[test]
fn the_kept_note_comes_back_when_session_notes_is_turned_on() {
    // The same presentation session, off then on: nothing was deleted while it was off.
    assert_eq!(row_with(false).session_note, None);
    let row = row_with(true);
    assert_eq!(
        row.session_note.as_deref(),
        Some("Check the migration next")
    );
    assert!(
        row.title_tooltip.contains("Note: Check the migration next"),
        "{}",
        row.title_tooltip
    );
}
