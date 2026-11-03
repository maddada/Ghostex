//! A coordinator on this computer whose thread runs on a connected computer: the thread's row,
//! taken from that computer's own sessions, is drawn under the coordinator with the state the
//! coordinator's computer reports, and keeps its key on the other computer, so clicking it opens
//! the session there.

use ghostex_gx_core::{
    Core, MachineId, MachineTabInput, SessionKey, SidebarInputs, SidebarViewModel,
    MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const MACHINE: &str = "remote-omarchy";

fn project(project_id: &str, sessions: &[&str]) -> (Value, Value) {
    (
        json!({
            "projectId": project_id,
            "title": project_id,
            "path": format!("/home/me/{project_id}"),
            "pathState": "available",
            "groupIds": [format!("{project_id}:active")],
            "sortKey": format!("1:{project_id}"),
            "createdAt": "2026-10-01T00:00:00.000Z",
            "updatedAt": "2026-11-03T00:00:00.000Z",
        }),
        json!({
            "groupId": format!("{project_id}:active"),
            "projectId": project_id,
            "title": "Active",
            "sessionIds": sessions,
            "sortKey": format!("1:{project_id}:active"),
        }),
    )
}

fn session(project_id: &str, session_id: &str, extra: Value) -> Value {
    let mut session = json!({
        "sessionId": session_id,
        "projectId": project_id,
        "groupId": format!("{project_id}:active"),
        "kind": "agent",
        "surface": "workspace",
        "zmxName": format!("S-{session_id}"),
        "sortKey": format!("000000000000:0:1:{session_id}"),
        "visibleInSidebarByDefault": true,
        "alias": format!("Session {session_id}"),
        "title": format!("Session {session_id}"),
        "activity": "working",
        "pendingQuestionCount": 0,
        "lifecycleState": "running",
        "createdAt": "2026-11-03T00:00:00.000Z",
        "updatedAt": "2026-11-03T00:00:00.000Z",
    });
    if let (Some(object), Some(extra)) = (session.as_object_mut(), extra.as_object()) {
        object.extend(extra.clone());
    }
    session
}

fn frame(server_id: &str, projects: Vec<(Value, Value)>, sessions: Vec<Value>) -> String {
    let (projects, groups): (Vec<_>, Vec<_>) = projects.into_iter().unzip();
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-11-03T00:00:00.000Z",
            "projects": projects,
            "groups": groups,
            "sessions": sessions,
        },
    })
    .to_string()
}

fn core() -> Core {
    let mut core = Core::new();
    let coordinator = session(
        "L1",
        "LC",
        json!({
            "coordinatorRole": "coordinator",
            "coordinatorRemoteThreads": [
                { "serverId": "S4k", "projectId": "R1", "sessionId": "RT", "threadState": "waiting" },
                // A thread on a computer this app is not connected to has no row to draw.
                { "serverId": "S0z", "projectId": "R9", "sessionId": "RX", "threadState": "working" },
            ],
        }),
    );
    core.handle_raw_frame(
        MachineId::Local,
        &frame("S9l", vec![project("L1", &["LC"])], vec![coordinator]),
        NOW_MS,
    )
    .expect("the local frame parses");
    core.handle_raw_frame(
        MachineId::Remote(MACHINE.to_string()),
        &frame(
            "S4k",
            vec![project("R1", &["RT", "RO"])],
            vec![
                session("R1", "RT", json!({})),
                session("R1", "RO", json!({})),
            ],
        ),
        NOW_MS,
    )
    .expect("the remote frame parses");
    core
}

fn inputs() -> SidebarInputs {
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
    inputs.ui.selected_machine_id = "local".to_string();
    inputs
}

#[test]
fn a_thread_on_another_computer_is_drawn_under_its_coordinator_here() {
    let core = core();
    let view = SidebarViewModel::build_from_scratch(&core, &inputs(), NOW_MS);
    let group = view
        .groups
        .iter()
        .find(|group| group.core.sessions.iter().any(|row| row.row.is_coordinator))
        .expect("the coordinator's group is drawn");
    let rows: Vec<_> = group.core.sessions.iter().collect();
    let coordinator = rows
        .iter()
        .position(|row| row.row.is_coordinator)
        .expect("the coordinator row");
    let thread = rows
        .iter()
        .position(|row| row.row.key == Some(SessionKey::remote(MACHINE, "R1", "RT")))
        .expect("the remote thread is drawn in the coordinator's group");
    assert_eq!(thread, coordinator + 1, "it follows its coordinator");
    let thread = &rows[thread].row;
    assert_eq!(
        thread.coordinator_parent,
        Some(SessionKey::local("L1", "LC"))
    );
    assert_eq!(thread.thread_state.as_deref(), Some("waiting"));
    // Only the linked session joins; the computer's other session stays in its own tab.
    assert!(!rows
        .iter()
        .any(|row| row.row.key == Some(SessionKey::remote(MACHINE, "R1", "RO"))));
    assert_eq!(rows.len(), 2);
}
