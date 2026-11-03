//! A ticket command's tickets in a session's links list (`PresentationSession.links.items`),
//! which every client's card and Links list draws: kind and key prefix are the command's id, with
//! its title and state, and `Unavailable` with the last title when the command can't be asked.

use crate::session_links::{
    presentation_session_links, session_links_read_snapshot, SessionLinkKind, SessionLinkRef,
};

use super::cards::{exact_read, pin, project_in, session_on_acm_12};
use super::*;

/// The session's links list entry for `key`.
fn link_item(project: &Value, session: &Value, key: &str) -> Value {
    let links = presentation_session_links(project, session).expect("the session has links");
    links["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["key"] == key)
        .cloned()
        .unwrap_or_else(|| panic!("no {key} among the links: {links}"))
}

#[test]
fn a_command_ticket_is_in_the_session_links_with_its_kind_and_title() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "links-list", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "links-list",
        &["ACM-12"],
    ));

    assert_published(
        &link_item(&project, &session, "links-list:ACM-12"),
        json!({
            "key": "links-list:ACM-12",
            "kind": "links-list",
            "label": "ACM-12",
            "source": "branch",
            "sourceLabel": "From branch",
            "title": "Spec the contract",
            "state": "active",
            "stateName": "In progress",
            "stateGroup": "progress",
            "url": "https://tracker.example/ACM-12",
        }),
    );
    // Its key names it everywhere a client sends one back (remove, pin, restore).
    let link = SessionLinkRef::from_key("links-list:acm-12").expect("a command ticket key");
    assert_eq!(link.key(), "links-list:ACM-12");
    assert!(matches!(link.kind(), SessionLinkKind::Command(_)));
    assert_eq!(link.kind().as_wire(), "links-list");
    // Its title stays in the command's own table, never in `sessionLinks.read`.
    let links = presentation_session_links(&project, &session).unwrap();
    assert!(!session_links_read_snapshot(&links).contains_key("links-list:ACM-12"));

    // The program moved away: the ticket stays, unavailable, with its last title.
    let moved = store.program.with_extension("moved");
    std::fs::rename(&store.program, &moved).expect("move program");
    let credentials = WorkCredentials::new(&store.paths);
    let projects: HashMap<&str, &Value> =
        HashMap::from([(project["projectId"].as_str().unwrap(), &project)]);
    work_source(command_source_of("links-list")).refresh_tickets(&TicketPass {
        paths: &store.paths,
        credentials: &credentials,
        projects: &projects,
        sessions: std::slice::from_ref(&session),
    });
    assert_published(
        &link_item(&project, &session, "links-list:ACM-12"),
        json!({
            "key": "links-list:ACM-12",
            "kind": "links-list",
            "label": "ACM-12",
            "source": "branch",
            "sourceLabel": "From branch",
            "title": "Spec the contract",
            "state": "unavailable",
            "stateName": "Unavailable",
        }),
    );
    std::fs::rename(&moved, &store.program).expect("move program back");
}

#[test]
fn a_removed_command_ticket_leaves_the_links_and_lists_under_removed() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "links-removed", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let mut session = session_on_acm_12(&project);
    session["runtimeSettings"]["sessionLinks"] = json!({ "removed": ["links-removed:ACM-12"] });

    let links = presentation_session_links(&project, &session).expect("a removed link");
    assert_eq!(links["items"], json!([]));
    assert_eq!(links["removed"][0]["key"], "links-removed:ACM-12");
    assert_eq!(links["removed"][0]["kind"], "links-removed");
}

fn command_source_of(id: &str) -> WorkSourceId {
    WorkSourceId::Command(CommandSourceId::known(id).expect("interned"))
}
