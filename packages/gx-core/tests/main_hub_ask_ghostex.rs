//! CDXC:Chief 2026-11-19 (server/src/chief/home.rs): Ask Ghostex AI's chats live in the Ghostex
//! config folder's project, which is never drawn as a project of its own. While Main Hub is off
//! gxserver marks that project `isHome`, so it is Main Hub, listing only its own chats and drawn
//! only while it holds one; while Main Hub is on its chats are listed in the home project's Main
//! Hub, so turning it on moves nothing out of Main Hub.

use ghostex_gx_core::{
    Core, MachineId, MachineTabInput, SidebarInputs, SidebarView, SidebarViewModel,
    MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;

/// (id, path, sessions, isHome, isHelpHome)
type Project<'a> = (&'a str, &'a str, &'a [&'a str], bool, bool);

fn session(project_id: &str, session_id: &str) -> Value {
    json!({
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
        "activity": "idle",
        "pendingQuestionCount": 0,
        "lifecycleState": "running",
        "createdAt": "2026-11-19T00:00:00.000Z",
        "updatedAt": "2026-11-19T00:00:00.000Z",
    })
}

fn core_with(projects: &[Project<'_>]) -> Core {
    let mut rows = Vec::new();
    let mut groups = Vec::new();
    let mut sessions = Vec::new();
    for (index, (id, path, ids, is_home, is_help_home)) in projects.iter().enumerate() {
        let mut row = json!({
            "projectId": id,
            "title": id,
            "path": path,
            "pathState": "available",
            "groupIds": [format!("{id}:active")],
            "sortKey": format!("{index}:{id}"),
            "createdAt": "2026-10-01T00:00:00.000Z",
            "updatedAt": "2026-11-19T00:00:00.000Z",
        });
        if *is_home {
            row["isHome"] = json!(true);
        }
        if *is_help_home {
            row["isHelpHome"] = json!(true);
        }
        rows.push(row);
        groups.push(json!({
            "groupId": format!("{id}:active"),
            "projectId": id,
            "title": "Active",
            "sessionIds": ids,
            "sortKey": format!("{index}:{id}:active"),
        }));
        for session_id in ids.iter() {
            sessions.push(session(id, session_id));
        }
    }
    let frame = json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": "S9t",
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-11-19T00:00:00.000Z",
            "projects": rows,
            "groups": groups,
            "sessions": sessions,
        },
    })
    .to_string();
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &frame, NOW_MS)
        .expect("the local frame parses");
    core
}

fn view_of(core: &Core) -> SidebarView {
    let mut inputs = SidebarInputs::default();
    inputs.host.machines = vec![MachineTabInput {
        machine_id: "local".to_string(),
        label: "local".to_string(),
        short_name: String::new(),
        state: MACHINE_STATE_CONNECTED.to_string(),
        message: None,
        fed: true,
        this_computer: false,
    }];
    inputs.host.merge_machines = true;
    inputs.ui.selected_machine_id = "local".to_string();
    SidebarViewModel::build_from_scratch(core, &inputs, NOW_MS)
}

/// Every drawn group as (project id, its session ids sorted).
fn entries(view: &SidebarView) -> Vec<(String, Vec<String>)> {
    view.groups
        .iter()
        .map(|group| {
            let project_id = group
                .core
                .project_context
                .as_ref()
                .map(|project| project.project_id.clone())
                .unwrap_or_default();
            let mut sessions: Vec<String> = group
                .core
                .sessions
                .iter()
                .filter_map(|row| row.row.key.as_ref())
                .map(|key| key.session_id.clone())
                .collect();
            sessions.sort();
            (project_id, sessions)
        })
        .collect()
}

fn entry(project_id: &str, sessions: &[&str]) -> (String, Vec<String>) {
    (
        project_id.to_string(),
        sessions.iter().map(|session| session.to_string()).collect(),
    )
}

/// The project ids whose group carries Main Hub's header.
fn home_projects(view: &SidebarView) -> Vec<String> {
    view.home_headers
        .iter()
        .filter_map(|home| {
            view.groups
                .iter()
                .find(|group| group.core.group_id == home.group_id)
                .and_then(|group| group.core.project_context.as_ref())
                .map(|project| project.project_id.clone())
        })
        .collect()
}

#[test]
fn with_main_hub_off_an_ask_ghostex_chat_shows_under_main_hub_alone() {
    let core = core_with(&[
        ("HOME", "/Users/me", &["H1"], false, false),
        ("HELP", "/Users/me/.config/ghostex", &["ASK1"], true, true),
        ("P", "/Users/me/dev/p", &["P1"], false, false),
    ]);
    let view = view_of(&core);
    assert_eq!(home_projects(&view), vec!["HELP".to_string()]);
    let entries = entries(&view);
    assert!(entries.contains(&entry("HELP", &["ASK1"])));
    // The home folder's own sessions stay in their ordinary project.
    assert!(entries.contains(&entry("HOME", &["H1"])));
    assert!(entries.contains(&entry("P", &["P1"])));
}

#[test]
fn with_main_hub_off_and_no_ask_ghostex_chat_there_is_no_main_hub() {
    let core = core_with(&[
        ("HOME", "/Users/me", &["H1"], false, false),
        ("HELP", "/Users/me/.config/ghostex", &[], true, true),
        ("P", "/Users/me/dev/p", &["P1"], false, false),
    ]);
    let view = view_of(&core);
    assert!(view.home_headers.is_empty());
    let mut entries = entries(&view);
    entries.sort();
    assert_eq!(entries, vec![entry("HOME", &["H1"]), entry("P", &["P1"])]);
}

#[test]
fn with_main_hub_on_ask_ghostex_chats_are_listed_in_main_hub() {
    let core = core_with(&[
        ("HOME", "/Users/me", &["H1"], true, false),
        ("HELP", "/Users/me/.config/ghostex", &["ASK1"], false, true),
        ("P", "/Users/me/dev/p", &["P1"], false, false),
    ]);
    let view = view_of(&core);
    assert_eq!(home_projects(&view), vec!["HOME".to_string()]);
    let mut entries = entries(&view);
    entries.sort();
    assert_eq!(
        entries,
        vec![entry("HOME", &["ASK1", "H1"]), entry("P", &["P1"])]
    );
}
