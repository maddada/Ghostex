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

#[test]
fn owed_is_honoured_only_for_a_session_just_created_with_no_reply() {
    use crate::agents::lead_links::honours_owed;
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-20T08:00:30.000Z")
        .unwrap()
        .timestamp_millis();
    let fresh = serde_json::json!({ "createdAt": "2026-11-20T08:00:00.000Z" });
    let old = serde_json::json!({ "createdAt": "2026-11-20T07:00:00.000Z" });
    // `agents create --lead --task`: created moments ago, nothing said yet.
    assert!(honours_owed(&fresh, now, false));
    // An existing session (or one that already replied) can't be made to owe the lead its user's
    // next turn.
    assert!(!honours_owed(&old, now, false));
    assert!(!honours_owed(&fresh, now, true));
    assert!(!honours_owed(&serde_json::json!({}), now, false));
}

#[test]
fn the_report_is_the_reply_to_the_leads_own_message_never_a_later_user_turn() {
    use crate::agents::lead_links::{TurnRow, lead_turn_reply};
    use crate::coordinators::{MessageSender, agent_message};
    let lead_ref = "S1:P1:GA";
    let from = |reply_to: &str, body: &str| {
        agent_message(
            &MessageSender {
                agent_name: "Codex".into(),
                title: "Lead".into(),
                session_id: "GA".into(),
                agent_id: "codex".into(),
                agent_session_id: "x".into(),
                global_ref: reply_to.into(),
            },
            body,
        )
    };
    let row = |user: bool, text: &str, at: i64| TurnRow {
        user,
        text: text.to_string(),
        at: Some(at),
    };
    let owed = 10_000;
    let lead_message = from(lead_ref, "Check the signing setup.");
    let mut rows = vec![
        row(true, "an older message of the user's", 1_000),
        row(false, "older reply", 2_000),
        row(true, &lead_message, 10_500),
    ];
    // The lead's turn has not answered yet.
    assert_eq!(lead_turn_reply(&rows, lead_ref, owed), None);
    rows.push(row(false, "Working on it.", 11_000));
    rows.push(row(false, "Signing is set up.", 12_000));
    assert_eq!(
        lead_turn_reply(&rows, lead_ref, owed).as_deref(),
        Some("Signing is set up.")
    );
    // The user then has their own turn there (the report to the lead is being retried): the lead
    // still gets the reply to its own message, never the user's turn.
    rows.push(row(true, "Also rename the branch.", 13_000));
    rows.push(row(false, "Renamed it.", 14_000));
    assert_eq!(
        lead_turn_reply(&rows, lead_ref, owed).as_deref(),
        Some("Signing is set up.")
    );
    // Another session's message, or the user's own, is not the lead's.
    let other = vec![
        row(true, &from("S1:P1:GB", "Do something."), 10_500),
        row(false, "Done.", 11_000),
    ];
    assert_eq!(lead_turn_reply(&other, lead_ref, owed), None);
    let user_only = vec![
        row(true, "Do something.", 10_500),
        row(false, "Done.", 11_000),
    ];
    assert_eq!(lead_turn_reply(&user_only, lead_ref, owed), None);
    // A lead message from before the turn it owes (an earlier, reported one) does not count.
    let stale = vec![row(true, &lead_message, 1_000), row(false, "Old.", 1_500)];
    assert_eq!(lead_turn_reply(&stale, lead_ref, 100_000), None);
}

#[test]
fn a_long_lead_turn_is_found_and_notices_stop_once_the_user_types() {
    use crate::agents::lead_links::{LeadTurn, TurnRow, lead_turn};
    use crate::coordinators::{MessageSender, agent_message};
    let lead_ref = "S1:P1:GA";
    let lead_message = agent_message(
        &MessageSender {
            agent_name: "Claude".into(),
            title: "Lead".into(),
            session_id: "GA".into(),
            agent_id: "claude".into(),
            agent_session_id: "x".into(),
            global_ref: lead_ref.into(),
        },
        "Refactor the settings page.",
    );
    let row = |user: bool, text: String, at: i64| TurnRow {
        user,
        text,
        at: Some(at),
    };
    // The lead's message, then a turn of 500 rows of the agent's own (the watcher reads back in
    // pages until it reaches the message, lead_runtime.rs `lead_turn_rows`).
    let mut rows = vec![row(true, lead_message, 10_000)];
    rows.extend((0..500).map(|step| row(false, format!("step {step}"), 11_000 + step)));
    assert_eq!(
        lead_turn(&rows, lead_ref, &[], 10_000),
        Some(LeadTurn {
            reply: Some("step 499".to_string()),
            user_after: false,
        })
    );
    // The user types there afterwards: the lead's turn is over, so a waiting or quiet notice now
    // would be about the user's own turn and is not sent.
    rows.push(row(true, "Now something of mine.".to_string(), 20_000));
    let turn = lead_turn(&rows, lead_ref, &[], 10_000).unwrap();
    assert!(turn.user_after);
    assert_eq!(turn.reply.as_deref(), Some("step 499"));
}
