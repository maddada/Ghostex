//! Chief's row (CDXC:Chief in src/sidebar_view/chief.rs): the session a computer flags `isChief`
//! leaves its project's list and is drawn on its own under the Spaces bar, in every Space; with
//! computers combined every computer's Chief is listed, this computer's first; with Separate by
//! machine on, a tab shows only its own computer's.

use ghostex_gx_core::protocol::SidebarSpacesState;
use ghostex_gx_core::{
    Core, Event, Intent, MachineId, MachineTabInput, SideStateUpdate, SidebarInputs, SidebarView,
    SidebarViewModel, MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const REMOTE: &str = "remote-hp";

fn session(project_id: &str, session_id: &str, chief: bool) -> Value {
    let mut session = json!({
        "sessionId": session_id,
        "projectId": project_id,
        "groupId": format!("{project_id}:active"),
        "kind": "agent",
        "surface": "workspace",
        "zmxName": format!("S-{session_id}"),
        "sortKey": format!("000000000000:0:1:{session_id}"),
        "visibleInSidebarByDefault": true,
        "alias": session_id,
        "title": session_id,
        "activity": "idle",
        "pendingQuestionCount": 0,
        "lifecycleState": "running",
        "createdAt": "2026-11-07T00:00:00.000Z",
        "updatedAt": "2026-11-07T00:00:00.000Z",
    });
    if chief {
        session["isChief"] = json!(true);
        session["coordinatorRole"] = json!("coordinator");
    }
    session
}

/// A computer with the Ghostex config project (`GX`: Chief and a Help chat) and one project.
fn frame(server_id: &str, chief_id: &str) -> String {
    let projects = [("GX", vec![chief_id, "HELP"]), ("APP", vec!["A1"])];
    let mut project_rows = Vec::new();
    let mut groups = Vec::new();
    let mut sessions = Vec::new();
    for (index, (id, ids)) in projects.iter().enumerate() {
        project_rows.push(json!({
            "projectId": id, "title": id, "path": format!("/{server_id}/{id}"),
            "pathState": "available", "groupIds": [format!("{id}:active")],
            "sortKey": format!("{index}:{id}"),
            "createdAt": "2026-10-01T00:00:00.000Z", "updatedAt": "2026-11-07T00:00:00.000Z",
        }));
        groups.push(json!({
            "groupId": format!("{id}:active"), "projectId": id, "title": "Active",
            "sessionIds": ids, "sortKey": format!("{index}:{id}:active"),
        }));
        for session_id in ids {
            sessions.push(session(id, session_id, *session_id == chief_id));
        }
    }
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "revision": 1,
        "snapshot": {
            "revision": 1, "generatedAt": "2026-11-07T00:00:00.000Z",
            "projects": project_rows, "groups": groups, "sessions": sessions,
        },
    })
    .to_string()
}

fn two_computers() -> Core {
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &frame("S9l", "CHIEF"), NOW_MS)
        .expect("local frame");
    core.handle_raw_frame(
        MachineId::Remote(REMOTE.to_string()),
        &frame("S2r", "CHIEF2"),
        NOW_MS,
    )
    .expect("remote frame");
    // A Space that holds only APP, so the config project is not in it.
    let spaces: SidebarSpacesState = serde_json::from_value(json!({
        "order": ["work"],
        "spaces": { "work": { "spaceId": "work", "name": "Work", "memberProjectIds": ["APP"] } },
    }))
    .expect("spaces");
    core.handle(
        Event::Intent(Intent::SetSideState {
            machine: MachineId::Local,
            update: Box::new(SideStateUpdate::Spaces(spaces)),
        }),
        NOW_MS,
    );
    core
}

fn inputs(merge: bool, selected: &str, space: Option<&str>) -> SidebarInputs {
    let mut inputs = SidebarInputs::default();
    inputs.host.machines = ["local", REMOTE]
        .iter()
        .map(|id| MachineTabInput {
            machine_id: id.to_string(),
            label: if *id == "local" {
                "MacBook Pro"
            } else {
                "Hetzner"
            }
            .to_string(),
            short_name: String::new(),
            state: MACHINE_STATE_CONNECTED.to_string(),
            message: None,
            fed: true,
            this_computer: false,
        })
        .collect();
    inputs.host.merge_machines = merge;
    inputs.ui.selected_machine_id = selected.to_string();
    if let Some(space) = space {
        inputs.settings.sidebar_spaces_enabled = true;
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .insert(selected.to_string(), space.to_string());
    }
    inputs
}

fn listed_session_ids(view: &SidebarView) -> Vec<String> {
    view.groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .filter_map(|row| row.row.key.as_ref().map(|key| key.session_id.clone()))
        .collect()
}

fn chiefs(view: &SidebarView) -> Vec<(String, String, bool)> {
    view.chiefs
        .iter()
        .map(|chief| {
            (
                chief.machine_label.clone(),
                chief.row.key.as_ref().expect("key").session_id.clone(),
                chief.is_local,
            )
        })
        .collect()
}

#[test]
fn chief_leaves_its_project_list_and_gets_its_own_row() {
    let core = two_computers();
    let view = SidebarViewModel::build_from_scratch(&core, &inputs(false, "local", None), NOW_MS);
    let listed = listed_session_ids(&view);
    assert!(!listed.contains(&"CHIEF".to_string()), "{listed:?}");
    assert!(listed.contains(&"HELP".to_string()), "{listed:?}");
    assert_eq!(
        chiefs(&view),
        vec![("MacBook Pro".to_string(), "CHIEF".to_string(), true)]
    );
}

#[test]
fn chief_stays_when_a_space_without_its_project_is_selected() {
    let core = two_computers();
    let view =
        SidebarViewModel::build_from_scratch(&core, &inputs(false, "local", Some("work")), NOW_MS);
    assert!(listed_session_ids(&view)
        .iter()
        .all(|id| id != "HELP" && id != "CHIEF"));
    assert_eq!(chiefs(&view).len(), 1);
}

#[test]
fn computers_combined_list_this_computers_chief_first_then_the_others() {
    let core = two_computers();
    let view = SidebarViewModel::build_from_scratch(&core, &inputs(true, "local", None), NOW_MS);
    assert_eq!(
        chiefs(&view),
        vec![
            ("MacBook Pro".to_string(), "CHIEF".to_string(), true),
            ("Hetzner".to_string(), "CHIEF2".to_string(), false),
        ]
    );
    assert!(!listed_session_ids(&view)
        .iter()
        .any(|id| id == "CHIEF" || id == "CHIEF2"));
}

#[test]
fn separate_by_machine_shows_each_tabs_own_chief() {
    let core = two_computers();
    let view = SidebarViewModel::build_from_scratch(&core, &inputs(false, REMOTE, None), NOW_MS);
    assert_eq!(
        chiefs(&view),
        vec![("Hetzner".to_string(), "CHIEF2".to_string(), false)]
    );
}
