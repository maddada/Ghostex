//! A session on a remote machine carries its links to every place a local one does: the store's
//! session (what the work area header and the links card read), the sidebar row (its line, badge
//! and hover card) and the row id the links card and its actions parse back.
//!
//! The fixture is shaped like what a connected Windows machine's gxserver sends: one session with
//! an open PR whose checks fail and a merged one, and one whose only link is merged.

use ghostex_gx_core::{
    Core, MachineId, MachineTabInput, SessionKey, SessionLinkTone, SessionLinks,
    SessionLinksDisplay, SidebarInputs, SidebarViewModel, MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const MACHINE: &str = "remote-win1";

fn pull_request(number: u64, state: &str, checks: Option<&str>) -> Value {
    let finished = state != "open";
    let mut link = json!({
        "key": format!("pr:owner/repo#{number}"),
        "kind": "pullRequest",
        "label": format!("#{number}"),
        "source": "branch",
        "sourceLabel": "From branch",
        "number": number,
        "state": state,
        "title": format!("PR {number}"),
        "url": format!("https://github.com/owner/repo/pull/{number}"),
        "finished": finished,
    });
    if let Some(checks) = checks {
        link["checks"] = json!(checks);
    }
    link
}

fn session(session_id: &str, links: Value) -> Value {
    json!({
        "sessionId": session_id,
        "projectId": "R1",
        "groupId": "R1:active",
        "kind": "agent",
        "surface": "workspace",
        "zmxName": format!("S90-{session_id}"),
        "sortKey": format!("000000000000:0:1:{session_id}"),
        "visibleInSidebarByDefault": true,
        "alias": format!("Session {session_id}"),
        "activity": "idle",
        "pendingQuestionCount": 0,
        "lifecycleState": "running",
        "lastInteractionAt": "2026-10-27T01:18:43.055Z",
        "createdAt": "2026-10-27T01:00:00.000Z",
        "updatedAt": "2026-10-27T01:18:43.055Z",
        "links": links,
    })
}

fn remote_core() -> Core {
    let open = json!({
        "items": [pull_request(6647, "open", Some("failing")), pull_request(6600, "merged", None)],
        "suggested": [],
    });
    let merged = json!({ "items": [pull_request(151, "merged", None)], "suggested": [] });
    let frame = json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": "windows",
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-10-27T00:00:00.000Z",
            "projects": [{
                "projectId": "R1",
                "title": "R1",
                "path": "C:\\dev\\R1",
                "pathState": "available",
                "groupIds": ["R1:active"],
                "sortKey": "1:R1",
                "createdAt": "2026-06-29T13:10:42.091Z",
                "updatedAt": "2026-10-27T01:18:43.055Z",
            }],
            "groups": [{
                "groupId": "R1:active",
                "projectId": "R1",
                "title": "Active",
                "sessionIds": ["RS1", "RS2"],
                "sortKey": "1:R1:active",
            }],
            "sessions": [session("RS1", open), session("RS2", merged)],
        }
    });
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &empty_local().to_string(), NOW_MS)
        .expect("the local frame parses");
    core.handle_raw_frame(
        MachineId::Remote(MACHINE.to_string()),
        &frame.to_string(),
        NOW_MS,
    )
    .expect("the remote frame parses");
    core
}

fn empty_local() -> Value {
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": "local",
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-10-27T00:00:00.000Z",
            "projects": [],
            "groups": [],
            "sessions": [],
        }
    })
}

fn remote_inputs() -> SidebarInputs {
    let mut inputs = SidebarInputs::default();
    inputs.host.machines = ["local", MACHINE]
        .into_iter()
        .map(|machine_id| MachineTabInput {
            machine_id: machine_id.to_string(),
            label: machine_id.to_string(),
            state: MACHINE_STATE_CONNECTED.to_string(),
            message: None,
            fed: true,
            this_computer: false,
        })
        .collect();
    inputs.ui.selected_machine_id = MACHINE.to_string();
    inputs
}

/// What the work area header and the links card read: the store's session by its remote key.
#[test]
fn a_remote_sessions_links_reach_the_store_session_the_header_reads() {
    let core = remote_core();
    let key = SessionKey::remote(MACHINE, "R1", "RS1");
    let session = core
        .presentation()
        .session(&key)
        .expect("the remote session is in the store");
    let links = SessionLinks::from_presentation(session.links.as_ref().expect("it has links"));
    assert_eq!(
        links
            .items
            .iter()
            .map(|link| link.label.as_str())
            .collect::<Vec<_>>(),
        vec!["#6647", "#6600"]
    );
    // The same ids on this computer are not that session.
    assert!(core
        .presentation()
        .session(&SessionKey::local("R1", "RS1"))
        .is_none());
}

/// The sidebar row of a remote session carries its links, and its row id parses back to the
/// remote key the links card and its pin, remove and undo actions send down the machine's tunnel.
#[test]
fn a_remote_sessions_row_shows_its_links_and_names_its_machine() {
    let core = remote_core();
    let view = SidebarViewModel::build_from_scratch(&core, &remote_inputs(), NOW_MS);
    let rows: Vec<_> = view
        .groups
        .iter()
        .flat_map(|group| group.core.sessions.iter().map(|session| &session.row))
        .collect();
    let open = rows
        .iter()
        .find(|row| row.sidebar_session_id == format!("remote:{MACHINE}:session:R1:RS1"))
        .expect("the remote row is drawn");
    let links = open.links.as_ref().expect("the remote row has its links");
    assert_eq!(links.items.len(), 2);
    let badge = links.row(SessionLinksDisplay::Badge, false);
    assert!(badge.hover_card);
    assert_eq!(
        badge.badge.map(|badge| badge.tone),
        Some(SessionLinkTone::Closed)
    );
    assert_eq!(
        SessionKey::parse_sidebar_session_id(&open.sidebar_session_id),
        Some(SessionKey::remote(MACHINE, "R1", "RS1"))
    );

    // Its only link merged: the badge is grey.
    let merged = rows
        .iter()
        .find(|row| row.sidebar_session_id == format!("remote:{MACHINE}:session:R1:RS2"))
        .expect("the second remote row is drawn");
    let badge = merged
        .links
        .as_ref()
        .expect("it has its link")
        .row(SessionLinksDisplay::default(), false)
        .badge
        .expect("Badge, the default, draws a badge");
    assert_eq!((badge.count, badge.tone), (1, SessionLinkTone::Draft));
}
