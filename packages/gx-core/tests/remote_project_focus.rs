//! A click on a browser page's notification brings a remote project on screen with
//! `Intent::FocusProject` (`show_browser_tab_of_notification` in the desktop app), not with the
//! remote attach a click on the project's header does. These pin what that relies on: the remote
//! project becomes the active one without a session being chosen, both while its computer streams
//! and while only its last-seen copy is held (where the header attach finds no live session and
//! stops at a toast), and the project of a computer the store has nothing for is not refused.

use ghostex_gx_core::protocol::PresentationSnapshot;
use ghostex_gx_core::{Core, Event, Intent, MachineId, ProjectKey};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const MACHINE: &str = "remote-mac1";

fn snapshot() -> Value {
    json!({
        "revision": 1,
        "generatedAt": "2026-11-02T00:00:00.000Z",
        "projects": [{
            "projectId": "R1",
            "title": "R1",
            "path": "/Users/me/dev/R1",
            "pathState": "available",
            "groupIds": ["R1:active"],
            "sortKey": "1:R1",
            "createdAt": "2026-10-01T00:00:00.000Z",
            "updatedAt": "2026-11-02T00:00:00.000Z",
        }],
        "groups": [{
            "groupId": "R1:active",
            "projectId": "R1",
            "title": "Active",
            "sessionIds": ["RS1"],
            "sortKey": "1:R1:active",
        }],
        "sessions": [{
            "sessionId": "RS1",
            "projectId": "R1",
            "groupId": "R1:active",
            "kind": "agent",
            "surface": "workspace",
            "zmxName": "S90-RS1",
            "sortKey": "000000000000:0:1:RS1",
            "visibleInSidebarByDefault": true,
            "alias": "Session RS1",
            "activity": "idle",
            "pendingQuestionCount": 0,
            "lifecycleState": "running",
            "createdAt": "2026-11-02T00:00:00.000Z",
            "updatedAt": "2026-11-02T00:00:00.000Z",
        }],
    })
}

fn frame(server_id: &str, snapshot: Value) -> String {
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "revision": 1,
        "snapshot": snapshot,
    })
    .to_string()
}

fn local_core() -> Core {
    let empty = json!({
        "revision": 1,
        "generatedAt": "2026-11-02T00:00:00.000Z",
        "projects": [],
        "groups": [],
        "sessions": [],
    });
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &frame("local", empty), NOW_MS)
        .expect("the local frame parses");
    core
}

fn remote() -> MachineId {
    MachineId::Remote(MACHINE.to_string())
}

/// The notification click's focus: the project on screen, no session chosen.
fn focus_remote_project(core: &mut Core, project_id: &str) {
    core.handle(
        Event::Intent(Intent::FocusProject {
            project: ProjectKey::remote(MACHINE, project_id),
        }),
        NOW_MS + 1_000,
    );
}

fn assert_remote_project_on_screen(core: &Core) {
    let focus = core.focus();
    assert_eq!(
        focus.active_project,
        Some(ProjectKey::remote(MACHINE, "R1"))
    );
    assert_eq!(focus.focused_session, None);
}

#[test]
fn a_connected_remote_project_comes_on_screen_without_choosing_a_session() {
    let mut core = local_core();
    core.handle_raw_frame(remote(), &frame("remote", snapshot()), NOW_MS)
        .expect("the remote frame parses");
    assert!(core.presentation().loaded_live(&remote()).is_some());
    focus_remote_project(&mut core, "R1");
    assert_remote_project_on_screen(&core);
}

/// The offline case the header attach cannot serve: `loaded_live` is `None`, so its candidate
/// list is empty and it only toasts; the focus still puts the project on screen, and its terminals
/// reattach on the machine's connected edge.
#[test]
fn an_offline_remote_project_held_as_last_seen_still_comes_on_screen() {
    let mut core = local_core();
    let snapshot: PresentationSnapshot =
        serde_json::from_value(snapshot()).expect("the snapshot parses");
    core.seed_last_seen_presentation(&remote(), snapshot);
    assert!(core.presentation().loaded(&remote()).is_some());
    assert!(core.presentation().loaded_live(&remote()).is_none());
    focus_remote_project(&mut core, "R1");
    assert_remote_project_on_screen(&core);
}

#[test]
fn a_remote_project_of_a_machine_not_loaded_is_not_refused() {
    let mut core = local_core();
    focus_remote_project(&mut core, "R1");
    assert_remote_project_on_screen(&core);
}
