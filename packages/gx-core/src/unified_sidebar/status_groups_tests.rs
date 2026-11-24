//! Which group a session's own state puts it in (`session_status_group`), for the attentions the
//! groups split by kind: a plain wait for input and a finished turn whose handover needs the user
//! stand in Needs you, a finished turn without such a handover in Finished (not yet read). The
//! phone's port groups the same cases the same way in its own tests; the shared Unified fixture
//! (fixture_tests.rs) cannot carry them, since its cases build the list with grouping off and its
//! expected answers hold no group.

use ghostex_gx_protocol::presentation::AttentionKind;

use crate::sidebar_view::status_tags::StatusGroup;
use crate::sidebar_view::SessionRow;

use super::session_status_group;

fn attention(kind: Option<AttentionKind>) -> SessionRow {
    SessionRow {
        activity: "attention".to_string(),
        attention_kind: kind,
        ..SessionRow::default()
    }
}

#[test]
fn a_plain_wait_for_input_stands_in_needs_you() {
    let waits = attention(Some(AttentionKind::NeedsInput));
    assert_eq!(session_status_group(&waits), StatusGroup::NeedsYou);
    // An older computer sends no kind: it counts as a wait, not as a finished turn.
    assert_eq!(
        session_status_group(&attention(None)),
        StatusGroup::NeedsYou
    );
    let mut working_in_background = waits;
    working_in_background.has_background_work = true;
    assert_eq!(
        session_status_group(&working_in_background),
        StatusGroup::NeedsYou
    );
}

#[test]
fn a_finished_turn_stands_in_needs_you_only_when_its_handover_needs_the_user() {
    let mut finished = attention(Some(AttentionKind::Finished));
    assert_eq!(session_status_group(&finished), StatusGroup::Finished);
    finished.attention_handover_needs_you = true;
    assert_eq!(session_status_group(&finished), StatusGroup::NeedsYou);
    finished.has_background_work = true;
    assert_eq!(session_status_group(&finished), StatusGroup::NeedsYou);
}

#[test]
fn an_approval_a_question_and_a_failure_still_come_before_a_wait() {
    let mut row = attention(Some(AttentionKind::NeedsInput));
    row.program_status_failed = true;
    assert_eq!(session_status_group(&row), StatusGroup::Error);
    row.awaiting_approval = true;
    assert_eq!(session_status_group(&row), StatusGroup::NeedsYou);
}
