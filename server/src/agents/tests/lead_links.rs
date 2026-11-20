//! Lead links (agents/lead_links): the rules the Opus review of #678 found broken.

use rusqlite::Connection;

use crate::agents::lead_links::{
    link_checked, read_lead_link, set_link_observed_working, settle_quiet, settle_waiting,
};
use crate::coordinators::SessionKey;

fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!(
        "../../storage/migrations/0041_coordinators.sql"
    ))
    .unwrap();
    db.execute_batch(include_str!("../../storage/migrations/0084_lead_links.sql"))
        .unwrap();
    db
}

fn key(session_id: &str) -> SessionKey {
    ("P1".to_string(), session_id.to_string())
}

#[test]
fn a_session_cannot_take_a_session_linked_to_another_lead_and_a_move_starts_afresh() {
    let db = db();
    let (worker, lead_a, lead_b) = (key("G1"), key("GA"), key("GB"));
    // Lead A starts it with a task, then hears it waiting and going quiet.
    link_checked(&db, true, &worker, &lead_a, true).unwrap();
    set_link_observed_working(&db, &worker, true).unwrap();
    settle_waiting(&db, &worker, "prompt:1").unwrap();
    settle_quiet(&db, &worker, 42).unwrap();

    // Agent B can't take it, and nothing about A's link changes.
    let refused = link_checked(&db, true, &worker, &lead_b, false).unwrap_err();
    assert!(
        refused.message.contains("only the user can move it"),
        "{}",
        refused.message
    );
    let kept = read_lead_link(&db, "P1", "G1").unwrap().unwrap();
    assert_eq!(kept.lead_key(), lead_a);
    assert!(kept.owed_since.is_some());

    // The user may move it; nothing owed to or reported to A carries over to B.
    assert_eq!(
        link_checked(&db, false, &worker, &lead_b, false).unwrap(),
        Some(lead_a)
    );
    let moved = read_lead_link(&db, "P1", "G1").unwrap().unwrap();
    assert_eq!(moved.lead_key(), lead_b);
    assert_eq!(moved.owed_since, None);
    assert!(!moved.observed_working);
    assert_eq!(moved.reported_prompt_key, None);
    assert_eq!(moved.reported_quiet_since, None);
    assert_eq!(moved.reported_at, None);

    // Linking it again to the same lead keeps what is owed.
    link_checked(&db, true, &worker, &lead_b, true).unwrap();
    let owed = read_lead_link(&db, "P1", "G1").unwrap().unwrap().owed_since;
    assert!(owed.is_some());
    link_checked(&db, true, &worker, &lead_b, false).unwrap();
    assert_eq!(
        read_lead_link(&db, "P1", "G1").unwrap().unwrap().owed_since,
        owed
    );
}
