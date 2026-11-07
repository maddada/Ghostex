//! Issue #461: with Separate by machine off, the one-list sidebar merges a project only with the
//! same project on another computer. On one computer, folders that share a git remote (subfolders
//! of one monorepo, or two clones of it) stay separate projects, each in its own Space with its own
//! sessions, and a Space switch restores a session of that Space's own projects.

use ghostex_gx_core::protocol::SidebarSpacesState;
use ghostex_gx_core::{
    plan_space_switch_restore, Core, Event, Intent, MachineId, MachineTabInput, SessionKey,
    SideStateUpdate, SidebarInputs, SidebarView, SidebarViewModel, SpaceSwitchFocus,
    MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const REMOTE: &str = "remote-hp";
const ORIGIN: &str = "git@github.com:me/monorepo.git";

/// One project with one Active group: (id, path, repository root, session ids).
struct Project<'a> {
    id: &'a str,
    path: &'a str,
    root: &'a str,
    sessions: &'a [&'a str],
}

/// This computer: the reporter's monorepo, three of its subfolders, and a second clone of it.
const LOCAL_PROJECTS: [Project<'static>; 5] = [
    Project {
        id: "MONO",
        path: "/Users/me/dev/monorepo",
        root: "/Users/me/dev/monorepo",
        sessions: &["MONO1"],
    },
    Project {
        id: "PX",
        path: "/Users/me/dev/monorepo/product-x",
        root: "/Users/me/dev/monorepo",
        sessions: &["PX1", "PX2"],
    },
    Project {
        id: "TR",
        path: "/Users/me/dev/monorepo/trading",
        root: "/Users/me/dev/monorepo",
        sessions: &["TR1"],
    },
    Project {
        id: "SUB",
        path: "/Users/me/dev/monorepo/company/sub-co",
        root: "/Users/me/dev/monorepo",
        sessions: &["SUB1"],
    },
    Project {
        id: "CL",
        path: "/Users/me/dev/monorepo-2",
        root: "/Users/me/dev/monorepo-2",
        sessions: &["CL1"],
    },
];

/// The Space each local project is in.
const SPACES: [(&str, &str, &str); 5] = [
    ("space-a", "A", "MONO"),
    ("space-product", "Product", "PX"),
    ("space-trading", "Trading", "TR"),
    ("space-startup", "Startup", "SUB"),
    ("space-clone", "Clone", "CL"),
];

fn frame(server_id: &str, origin: &str, projects: &[Project<'_>]) -> String {
    let mut project_rows = Vec::new();
    let mut groups = Vec::new();
    let mut sessions = Vec::new();
    for (index, project) in projects.iter().enumerate() {
        let id = project.id;
        project_rows.push(json!({
            "projectId": id,
            "title": id,
            "path": project.path,
            "pathState": "available",
            "groupIds": [format!("{id}:active")],
            "sortKey": format!("{index}:{id}"),
            "gitRemoteOriginUrl": origin,
            "gitRepositoryRootPath": project.root,
            "createdAt": "2026-10-01T00:00:00.000Z",
            "updatedAt": "2026-11-07T00:00:00.000Z",
        }));
        groups.push(json!({
            "groupId": format!("{id}:active"),
            "projectId": id,
            "title": "Active",
            "sessionIds": project.sessions,
            "sortKey": format!("{index}:{id}:active"),
        }));
        for session_id in project.sessions {
            sessions.push(session(id, session_id));
        }
    }
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-11-07T00:00:00.000Z",
            "projects": project_rows,
            "groups": groups,
            "sessions": sessions,
        },
    })
    .to_string()
}

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
        "createdAt": "2026-11-07T00:00:00.000Z",
        "updatedAt": "2026-11-07T00:00:00.000Z",
    })
}

fn spaces_document() -> SidebarSpacesState {
    let spaces: serde_json::Map<String, Value> = SPACES
        .iter()
        .map(|(space_id, name, project_id)| {
            (
                space_id.to_string(),
                json!({ "spaceId": space_id, "name": name, "memberProjectIds": [project_id] }),
            )
        })
        .collect();
    serde_json::from_value(json!({
        "order": SPACES.iter().map(|space| space.0).collect::<Vec<_>>(),
        "spaces": spaces,
    }))
    .expect("a Spaces document")
}

/// This computer alone, as the reporter had it: one gxserver, its Spaces document applied.
fn one_computer() -> Core {
    let mut core = Core::new();
    core.handle_raw_frame(
        MachineId::Local,
        &frame("S9l", ORIGIN, &LOCAL_PROJECTS),
        NOW_MS,
    )
    .expect("the local frame parses");
    core.handle(
        Event::Intent(Intent::SetSideState {
            machine: MachineId::Local,
            update: Box::new(SideStateUpdate::Spaces(spaces_document())),
        }),
        NOW_MS,
    );
    core
}

fn tab(machine_id: &str) -> MachineTabInput {
    MachineTabInput {
        machine_id: machine_id.to_string(),
        label: machine_id.to_string(),
        short_name: String::new(),
        state: MACHINE_STATE_CONNECTED.to_string(),
        message: None,
        fed: true,
        this_computer: false,
    }
}

/// The Project sidebar's own list with Separate by machine off (the default): the merged list.
fn inputs(machines: &[&str], space_id: Option<&str>) -> SidebarInputs {
    let mut inputs = SidebarInputs::default();
    inputs.host.machines = machines.iter().map(|machine| tab(machine)).collect();
    inputs.host.merge_machines = true;
    inputs.ui.selected_machine_id = "local".to_string();
    if let Some(space_id) = space_id {
        inputs.settings.sidebar_spaces_enabled = true;
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .insert("local".to_string(), space_id.to_string());
    }
    inputs
}

/// Every drawn entry as (its project's id, the sessions listed in it as `machine/project/session`).
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
            let sessions = group
                .core
                .sessions
                .iter()
                .filter_map(|row| row.row.key.as_ref())
                .map(|key| {
                    let machine = match &key.machine {
                        MachineId::Local => "local".to_string(),
                        MachineId::Remote(machine) => machine.clone(),
                    };
                    format!("{machine}/{}/{}", key.project_id, key.session_id)
                })
                .collect();
            (project_id, sessions)
        })
        .collect()
}

fn local(project_id: &str, sessions: &[&str]) -> (String, Vec<String>) {
    (
        project_id.to_string(),
        sessions
            .iter()
            .map(|session| format!("local/{project_id}/{session}"))
            .collect(),
    )
}

#[test]
fn folders_of_one_repository_on_one_computer_stay_separate_projects() {
    let core = one_computer();
    let view = SidebarViewModel::build_from_scratch(&core, &inputs(&["local"], None), NOW_MS);
    assert_eq!(
        entries(&view),
        vec![
            local("MONO", &["MONO1"]),
            local("PX", &["PX1", "PX2"]),
            local("TR", &["TR1"]),
            local("SUB", &["SUB1"]),
            // A second clone of the same repository is a different folder too.
            local("CL", &["CL1"]),
        ]
    );
    // None of them is "on several computers".
    assert!(view
        .groups
        .iter()
        .all(|group| group.core.machine_names.is_empty()));
}

#[test]
fn each_space_draws_only_its_own_projects_sessions() {
    let core = one_computer();
    for (space_id, _, project_id) in SPACES {
        let view = SidebarViewModel::build_from_scratch(
            &core,
            &inputs(&["local"], Some(space_id)),
            NOW_MS,
        );
        let project = LOCAL_PROJECTS
            .iter()
            .find(|project| project.id == project_id)
            .expect("the Space's project");
        assert_eq!(
            entries(&view),
            vec![local(project_id, project.sessions)],
            "Space {space_id}"
        );
    }
}

#[test]
fn switching_to_a_space_restores_a_session_of_that_space() {
    let core = one_computer();
    let view = SidebarViewModel::build_from_scratch(
        &core,
        &inputs(&["local"], Some("space-trading")),
        NOW_MS,
    );
    // The memory the reporter found: two sessions of the Product Space's project on top.
    let remembered = [
        SessionKey::local("PX", "PX2").to_sidebar_session_id(),
        SessionKey::local("PX", "PX1").to_sidebar_session_id(),
        SessionKey::local("TR", "TR1").to_sidebar_session_id(),
    ];
    assert_eq!(
        plan_space_switch_restore(&view, &remembered),
        Some(SpaceSwitchFocus::Session {
            sidebar_session_id: SessionKey::local("TR", "TR1").to_sidebar_session_id(),
        })
    );
    // With only other Spaces' sessions remembered it opens on its own first row.
    assert_eq!(
        plan_space_switch_restore(&view, &remembered[..2]),
        Some(SpaceSwitchFocus::Session {
            sidebar_session_id: SessionKey::local("TR", "TR1").to_sidebar_session_id(),
        })
    );
}

/// The merge still does its job: the same folder of the repository on another computer joins this
/// computer's entry for it, and only that one.
#[test]
fn the_same_project_on_another_computer_still_merges_into_its_own_entry() {
    let mut core = one_computer();
    let remote_projects = [
        Project {
            id: "R-MONO",
            path: "/home/me/monorepo",
            root: "/home/me/monorepo",
            sessions: &["RM1"],
        },
        Project {
            id: "R-TR",
            path: "/home/me/monorepo/trading",
            root: "/home/me/monorepo",
            sessions: &["RT1"],
        },
        // Two clones there too: the second joins this computer's second clone.
        Project {
            id: "R-CL",
            path: "/home/me/monorepo-copy",
            root: "/home/me/monorepo-copy",
            sessions: &["RC1"],
        },
        // A folder this computer does not have is its own entry.
        Project {
            id: "R-HEALTH",
            path: "/home/me/monorepo/health",
            root: "/home/me/monorepo",
            sessions: &["RH1"],
        },
    ];
    // The same repository, spelled the https way on that computer.
    core.handle_raw_frame(
        MachineId::Remote(REMOTE.to_string()),
        &frame("S4k", "https://github.com/me/monorepo", &remote_projects),
        NOW_MS,
    )
    .expect("the remote frame parses");
    let view =
        SidebarViewModel::build_from_scratch(&core, &inputs(&["local", REMOTE], None), NOW_MS);
    let remote = |project: &str, session: &str| format!("{REMOTE}/{project}/{session}");
    assert_eq!(
        entries(&view),
        vec![
            (
                "MONO".to_string(),
                vec!["local/MONO/MONO1".to_string(), remote("R-MONO", "RM1")]
            ),
            local("PX", &["PX1", "PX2"]),
            (
                "TR".to_string(),
                vec!["local/TR/TR1".to_string(), remote("R-TR", "RT1")]
            ),
            local("SUB", &["SUB1"]),
            (
                "CL".to_string(),
                vec!["local/CL/CL1".to_string(), remote("R-CL", "RC1")]
            ),
            ("R-HEALTH".to_string(), vec![remote("R-HEALTH", "RH1")]),
        ]
    );
    let names = |project_id: &str| -> Vec<String> {
        view.groups
            .iter()
            .find(|group| {
                group
                    .core
                    .project_context
                    .as_ref()
                    .is_some_and(|project| project.project_id == project_id)
            })
            .map(|group| {
                group
                    .core
                    .machine_names
                    .iter()
                    .map(|chip| chip.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(names("TR"), vec!["local".to_string(), REMOTE.to_string()]);
    assert!(names("PX").is_empty());
}
