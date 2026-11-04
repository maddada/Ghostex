//! Fewer command reads: a card's ticket is read again after `FOUND_TTL`, sooner only when its own
//! answer reports pull requests, and the projection finds a workspace's titled tickets without
//! walking every title.

use std::time::{Duration, Instant};

use super::cards::{card_pass, card_read, pin, plan_of, project_in, session_on_acm_12, wait_until};
use super::*;

/// A card pass reads `id`'s ticket for `project`'s session, which has no PR of its own.
fn read_once(store: &Store, workspace: &str, id: &str, mode: &str) -> JobPlan {
    let workspace_id = pin(store, workspace, id, mode);
    let project = project_in(store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    card_pass(store, &project, std::slice::from_ref(&session), id);
    wait_until("the exact read", || {
        !matches!(card_read(&project, id, "ACM-12"), TicketRead::Unread)
    });
    plan_of(&workspace_id, id)
}

/// A session with no PR of its own doesn't make its command's ticket read every 45 s: only a
/// ticket that reports pull requests is read that often.
#[test]
fn only_a_ticket_with_pull_requests_is_read_again_sooner() {
    let store = Store::new();
    let plain = read_once(&store, "ws-a", "reads-plain", "title-kept");
    let now = Instant::now();
    assert!(!command_ticket_due(
        &plain,
        "ACM-12",
        now + Duration::from_secs(60)
    ));
    assert!(command_ticket_due(
        &plain,
        "ACM-12",
        now + Duration::from_secs(121)
    ));
    let linked = read_once(&store, "ws-b", "reads-linked", "good");
    let now = Instant::now();
    assert!(!command_ticket_due(
        &linked,
        "ACM-12",
        now + Duration::from_secs(30)
    ));
    assert!(command_ticket_due(
        &linked,
        "ACM-12",
        now + Duration::from_secs(46)
    ));
}

/// The workspace's titled tickets come from an index: asking for them looks only at its own,
/// however many titles other workspaces keep.
#[test]
fn a_workspaces_titled_tickets_come_without_reading_the_others() {
    let store = Store::new();
    let mine = store.ws("ws-mine");
    let others = store.ws("ws-others");
    let many: Vec<(String, String)> = (0..300)
        .map(|n| (format!("ACM-{n}"), format!("Title {n}")))
        .collect();
    save_ticket_titles(&store.db, &others, "reads-index", &many).expect("saved");
    save_ticket_titles(
        &store.db,
        &mine,
        "reads-index",
        &[
            ("ACM-1".to_string(), "One".to_string()),
            ("ACM-2".to_string(), "Two".to_string()),
        ],
    )
    .expect("saved");
    let before = titles_looked_at();
    let titled = titled_tickets(&mine);
    assert_eq!(titles_looked_at() - before, 2);
    assert_eq!(titled["reads-index"], ["ACM-1", "ACM-2"]);
    forget_workspace_ticket_titles(&mine);
    assert!(titled_tickets(&mine).is_empty());
    assert_eq!(titled_tickets(&others)["reads-index"].len(), 300);
}
