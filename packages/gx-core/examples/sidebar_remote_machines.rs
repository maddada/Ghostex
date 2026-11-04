//! Checks the rules that only exist once a second machine's presentation is in the store, none of
//! which a recording can exercise: every recording this port has was made on one machine.
//!
//! Usage: `cargo run --example sidebar_remote_machines`
//!
//! What it asks, in the order the sidebar asks it:
//!
//! 1. The ids. A remote group, its storage id, its rows and their routing ids have to be spelled
//!    exactly as `createGpuiRemotePresentationSidebarGroups` and `projectNativeSidebarGroup` spell
//!    them, because the collapse state on disk is keyed by them and a row id that disagrees with
//!    the renderer's is a row nothing can click.
//! 2. The machine tabs and their badges, which are counted over a machine the list is not built
//!    for.
//! 3. `supported` and `canAddProject`, which are the two gates that decide whether the renderer
//!    draws this list at all and whether the empty state offers Add Project.
//! 4. The empty state's inventory test, which spans machines: a user whose only projects are on a
//!    remote machine must not be shown first-run copy.
//! 5. Staleness: a machine whose stream dropped keeps drawing its rows, faded.
//! 6. The cross-machine reveal, which is declared difference 5 of M4b and is what this closes.
//! 7. The three places a remote row differs from a local one: the worktree count on its header,
//!    the state line in its tooltip, and the two host-timer capabilities it has to opt into.
//!
//! This is tooling, not a test suite; it prints what it found and fails the process on a
//! difference.

use std::process::ExitCode;

use ghostex_gx_core::{
    reveal_plan, ConnectionUpdate, Core, Event, MachineId, MachineTabInput, SidebarInputs,
    SidebarView, SidebarViewModel, MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const MACHINE: &str = "remote-ab12";

fn main() -> ExitCode {
    let failures = std::cell::Cell::new(0usize);
    let check = |label: &str, ok: bool, detail: String| {
        println!("{:<62} {}", label, if ok { "ok" } else { "DIFFERS" });
        if !ok {
            failures.set(failures.get() + 1);
            println!("  {detail}");
        }
    };

    ids(&check);
    machine_tabs(&check);
    gates(&check);
    inventory(&check);
    staleness(&check);
    cross_machine_reveal(&check);
    row_differences(&check);

    if failures.get() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every id a remote group and its rows carry, against the strings the TypeScript builds.
fn ids(check: &dyn Fn(&str, bool, String)) {
    let core = two_machine_core();
    let view = remote_view(&core, remote_inputs());
    let group = view
        .groups
        .iter()
        .find(|group| group.core.group_id == format!("remote:{MACHINE}:group:R1"));
    check(
        "a remote project group is remote:<machine>:group:<project>",
        group.is_some(),
        format!("{:?}", drawn_groups(&view)),
    );
    let Some(group) = group else {
        return;
    };
    check(
        "its storage id carries the machine prefix twice",
        group.core.storage_id == format!("remote:{MACHINE}:remote:{MACHINE}:project:R1"),
        group.core.storage_id.clone(),
    );
    check(
        "it names its machine and the raw project id",
        group.core.remote_machine.as_ref().map(|remote| {
            (
                remote.machine_id.as_str(),
                remote.machine_name.as_str(),
                remote.project_id.as_deref(),
            )
        }) == Some((MACHINE, "Kubuntu VM", Some("R1"))),
        format!("{:?}", group.core.remote_machine),
    );
    let row = group.core.sessions.first().map(|session| &session.row);
    check(
        "a remote row is remote:<machine>:session:<project>:<session>",
        row.map(|row| row.sidebar_session_id.as_str())
            == Some(format!("remote:{MACHINE}:session:R1:RS1").as_str()),
        format!("{:?}", row.map(|row| row.sidebar_session_id.clone())),
    );
    check(
        "its routing id puts the machine in front",
        row.and_then(|row| row.menu_facts.session_routing_id.as_deref())
            == Some(format!("{MACHINE}:R1:RS1").as_str()),
        format!(
            "{:?}",
            row.and_then(|row| row.menu_facts.session_routing_id.clone())
        ),
    );
    // The app's browser tabs name a remote project by its machine-scoped id.
    let mut with_tab = remote_inputs();
    with_tab.host.browser_tabs = vec![ghostex_gx_core::BrowserTabInput {
        project_id: format!("remote:{MACHINE}:project:R1"),
        tab_id: "tab-1".to_string(),
        title: "Docs".to_string(),
        ..ghostex_gx_core::BrowserTabInput::default()
    }];
    let view = remote_view(&core, with_tab);
    let rows = view
        .groups
        .iter()
        .find(|group| group.core.group_id == format!("remote:{MACHINE}:group:R1"))
        .map(|group| {
            group
                .core
                .sessions
                .iter()
                .map(|session| session.row.sidebar_session_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    check(
        "a browser tab of a remote project splices in by its scoped id",
        rows.iter().any(|id| {
            *id == format!(
                "gpui-browser:{}:tab-1",
                ghostex_gx_core::encode_uri_component(&format!("remote:{MACHINE}:project:R1"))
            )
        }),
        format!("{rows:?}"),
    );
    // And a tab named by the RAW project id belongs to no machine's list.
    let mut wrong_tab = remote_inputs();
    wrong_tab.host.browser_tabs = vec![ghostex_gx_core::BrowserTabInput {
        project_id: "R1".to_string(),
        tab_id: "tab-2".to_string(),
        title: "Docs".to_string(),
        ..ghostex_gx_core::BrowserTabInput::default()
    }];
    let view = remote_view(&core, wrong_tab);
    check(
        "a tab named by the raw project id is not spliced in",
        !view
            .groups
            .iter()
            .flat_map(|group| group.core.sessions.iter())
            .any(|session| session.row.sidebar_session_id.contains("tab-2")),
        "a raw-id tab reached a remote group".to_string(),
    );
}

/// The tabs and their badges, one of which is counted for a machine the list is not built for.
fn machine_tabs(check: &dyn Fn(&str, bool, String)) {
    let core = two_machine_core();
    let view = remote_view(&core, local_inputs());
    check(
        "the tabs are the host's list, in its order",
        view.machines
            .iter()
            .map(|machine| machine.id.as_str())
            .collect::<Vec<_>>()
            == vec!["local", MACHINE],
        format!("{:?}", view.machines),
    );
    check(
        "the local tab counts the local machine while local is selected",
        view.machines
            .first()
            .map(|machine| (machine.working_count, machine.attention_count))
            == Some((1, 0)),
        format!("{:?}", view.machines.first()),
    );
    check(
        "the remote tab counts a machine this list was not built for",
        view.machines
            .get(1)
            .map(|machine| (machine.working_count, machine.attention_count))
            == Some((2, 1)),
        format!("{:?}", view.machines.get(1)),
    );
    check(
        "it carries the host's state word and message",
        view.machines
            .get(1)
            .map(|machine| (machine.state.as_str(), machine.message.as_deref()))
            == Some((MACHINE_STATE_CONNECTED, None)),
        format!("{:?}", view.machines.get(1)),
    );
    // The same numbers with the remote machine selected, where they come off the built groups.
    let view = remote_view(&core, remote_inputs());
    check(
        "selecting the machine gives its tab the same counts",
        view.machines
            .get(1)
            .map(|machine| (machine.working_count, machine.attention_count))
            == Some((2, 1)),
        format!("{:?}", view.machines.get(1)),
    );
    check(
        "and the local tab keeps its own",
        view.machines
            .first()
            .map(|machine| (machine.working_count, machine.attention_count))
            == Some((1, 0)),
        format!("{:?}", view.machines.first()),
    );
    // A project the machine parked is out of its list and out of its count.
    // The RAW daemon id, which is what the host has to parse out of the machine-scoped id the
    // Recent Projects row publishes. Asserting the raw id here is what the example is for: it is
    // the contract both readers of this set compare against.
    let mut parked = remote_inputs();
    parked
        .host
        .remote_recent_project_ids
        .entry(MACHINE.to_string())
        .or_default()
        .insert("R1".to_string());
    let view = remote_view(&core, parked);
    check(
        "a project the remote machine parked leaves its list",
        !drawn_groups(&view).contains(&format!("remote:{MACHINE}:group:R1")),
        format!("{:?}", drawn_groups(&view)),
    );
    check(
        "and leaves its badge",
        view.machines
            .get(1)
            .map(|machine| (machine.working_count, machine.attention_count))
            == Some((1, 1)),
        format!("{:?}", view.machines.get(1)),
    );
    // The other half of that contract, and the one a host gets wrong: the MACHINE-SCOPED id a
    // Recent Projects row publishes hides nothing here. This set is raw daemon ids, because that
    // is what every reader of it compares against, so a host that mirrors the published id
    // unchanged silently stops hiding parked projects. Asserting only the raw case let exactly
    // that ship (review R1).
    let mut scoped = remote_inputs();
    scoped
        .host
        .remote_recent_project_ids
        .entry(MACHINE.to_string())
        .or_default()
        .insert(format!("remote:{MACHINE}:project:R1"));
    let view = remote_view(&core, scoped);
    check(
        "the machine-scoped id hides nothing: this set is raw ids",
        drawn_groups(&view).contains(&format!("remote:{MACHINE}:group:R1")),
        format!("{:?}", drawn_groups(&view)),
    );
}

/// `supported` and `canAddProject`: the two gates around a remote tab.
fn gates(check: &dyn Fn(&str, bool, String)) {
    let core = two_machine_core();
    let view = remote_view(&core, remote_inputs());
    check(
        "a machine the host feeds is supported",
        view.supported,
        "supported was false for a fed machine".to_string(),
    );
    check(
        "a connected machine offers Add Project",
        view.empty_state.can_add_project,
        format!("{:?}", view.empty_state),
    );
    let mut unfed = remote_inputs();
    unfed.host.machines[1].fed = false;
    let view = remote_view(&core, unfed);
    check(
        "a machine the host does not feed is not supported",
        !view.supported,
        "supported was true for a machine with no client".to_string(),
    );
    let mut disconnected = remote_inputs();
    disconnected.host.machines[1].state = "disconnected".to_string();
    let view = remote_view(&core, disconnected);
    check(
        "a disconnected machine offers no Add Project and says nothing",
        !view.empty_state.can_add_project
            && !view.empty_state.loading
            && !view.empty_state.error
            && view.empty_state.copy.is_empty(),
        format!("{:?}", view.empty_state),
    );
    check(
        "the local tab always offers Add Project",
        remote_view(&core, local_inputs())
            .empty_state
            .can_add_project,
        "the local empty state refused Add Project".to_string(),
    );
}

/// The empty state's inventory test spans machines.
fn inventory(check: &dyn Fn(&str, bool, String)) {
    // A store with rows on the remote machine only, and nothing local at all.
    let mut core = Core::new();
    core.handle_raw_frame(
        MachineId::Remote(MACHINE.to_string()),
        &snapshot(&["R1"], "remote").to_string(),
        NOW_MS,
    )
    .expect("the frame parses");
    core.handle_raw_frame(
        MachineId::Local,
        &snapshot(&[], "local").to_string(),
        NOW_MS,
    )
    .expect("the frame parses");
    let view = remote_view(&core, local_inputs());
    check(
        "a user whose only projects are remote is not shown first-run copy",
        view.empty_state.copy == "No projects",
        format!("{:?}", view.empty_state),
    );
    // Nothing anywhere is the real first run.
    let mut empty = Core::new();
    empty
        .handle_raw_frame(
            MachineId::Local,
            &snapshot(&[], "local").to_string(),
            NOW_MS,
        )
        .expect("the frame parses");
    let view = remote_view(&empty, local_inputs());
    check(
        "with no project on any machine it is the first-run copy",
        view.empty_state.copy == "No projects added yet.",
        format!("{:?}", view.empty_state),
    );
}

/// A machine whose stream dropped keeps its rows, faded.
fn staleness(check: &dyn Fn(&str, bool, String)) {
    let mut core = two_machine_core();
    let view = remote_view(&core, remote_inputs());
    check(
        "a live machine's groups are not stale",
        view.groups.iter().all(|group| !group.core.is_stale),
        "a live group was marked stale".to_string(),
    );
    core.handle(
        Event::Connection {
            machine: MachineId::Remote(MACHINE.to_string()),
            update: ConnectionUpdate::Lost {
                error: Some("the tunnel dropped".to_string()),
            },
        },
        NOW_MS,
    );
    let view = remote_view(&core, remote_inputs());
    check(
        "a dropped stream keeps the rows",
        drawn_groups(&view).contains(&format!("remote:{MACHINE}:group:R1")),
        format!("{:?}", drawn_groups(&view)),
    );
    check(
        "and marks every group of that machine stale",
        view.groups.iter().all(|group| group.core.is_stale),
        format!(
            "{:?}",
            view.groups
                .iter()
                .map(|group| (group.core.group_id.clone(), group.core.is_stale))
                .collect::<Vec<_>>()
        ),
    );
    let local = remote_view(&core, local_inputs());
    check(
        "this computer's groups are never stale",
        local.groups.iter().all(|group| !group.core.is_stale),
        "a local group was marked stale".to_string(),
    );
    // A reconnect attempt emits `Connecting` before the failure emits `Lost`, so a machine that
    // cannot be reached walks through both once per rung of the ladder. The fade must not follow
    // it: it is "the rows are held and the stream is not live", not "the last update was Lost".
    core.handle(
        Event::Connection {
            machine: MachineId::Remote(MACHINE.to_string()),
            update: ConnectionUpdate::Connecting { attempt: 2 },
        },
        NOW_MS,
    );
    let view = remote_view(&core, remote_inputs());
    check(
        "a reconnect attempt does not un-fade the rows",
        view.groups.iter().all(|group| group.core.is_stale),
        format!(
            "{:?}",
            view.groups
                .iter()
                .map(|group| (group.core.group_id.clone(), group.core.is_stale))
                .collect::<Vec<_>>()
        ),
    );
    core.handle(
        Event::Connection {
            machine: MachineId::Remote(MACHINE.to_string()),
            update: ConnectionUpdate::Live,
        },
        NOW_MS,
    );
    let view = remote_view(&core, remote_inputs());
    check(
        "and a live stream un-fades them",
        view.groups.iter().all(|group| !group.core.is_stale),
        format!(
            "{:?}",
            view.groups
                .iter()
                .map(|group| (group.core.group_id.clone(), group.core.is_stale))
                .collect::<Vec<_>>()
        ),
    );
}

/// The cross-machine reveal, which M4b declared as doing nothing.
fn cross_machine_reveal(check: &dyn Fn(&str, bool, String)) {
    let core = two_machine_core();
    let inputs = local_inputs();
    let view = remote_view(&core, inputs.clone());
    let plan = reveal_plan(
        &core,
        &inputs,
        &view,
        &format!("remote:{MACHINE}:session:R1:RS1"),
        NOW_MS,
    );
    check(
        "a reveal of a remote row moves the machine tab",
        plan.as_ref()
            .and_then(|plan| plan.select_machine.as_deref())
            == Some(MACHINE),
        format!(
            "{:?}",
            plan.as_ref().map(|plan| plan.select_machine.clone())
        ),
    );
    check(
        "and names the remote group and its storage id",
        plan.as_ref()
            .map(|plan| (plan.group_id.as_str(), plan.storage_id.as_str()))
            == Some((
                format!("remote:{MACHINE}:group:R1").as_str(),
                format!("remote:{MACHINE}:remote:{MACHINE}:project:R1").as_str(),
            )),
        format!(
            "{:?}",
            plan.as_ref()
                .map(|plan| (plan.group_id.clone(), plan.storage_id.clone()))
        ),
    );
    // A collapsed remote group is read under the remote storage id, not the local one.
    let mut collapsed = local_inputs();
    collapsed
        .ui
        .collapse
        .collapsed_groups
        .insert(format!("remote:{MACHINE}:group:R1"));
    let plan = reveal_plan(
        &core,
        &collapsed,
        &view,
        &format!("remote:{MACHINE}:session:R1:RS1"),
        NOW_MS,
    );
    check(
        "a collapsed remote group is expanded by the reveal",
        plan.as_ref().is_some_and(|plan| plan.collapsed_group),
        format!("{:?}", plan.as_ref().map(|plan| plan.collapsed_group)),
    );
    // A row on a machine the store does not hold is not revealed at all.
    let plan = reveal_plan(
        &core,
        &inputs,
        &view,
        "remote:remote-gone:session:R1:RS1",
        NOW_MS,
    );
    check(
        "a row on a machine the store does not hold reveals nothing",
        plan.is_none(),
        format!("{:?}", plan.map(|plan| plan.select_machine)),
    );
    // A local row still reveals without moving the tab.
    let plan = reveal_plan(&core, &inputs, &view, "combined-session:P1:PS1", NOW_MS);
    check(
        "a local reveal still moves no machine tab",
        plan.as_ref()
            .is_some_and(|plan| plan.select_machine.is_none()),
        format!(
            "{:?}",
            plan.as_ref().map(|plan| plan.select_machine.clone())
        ),
    );
}

/// The three per-row facts a remote machine's row differs in.
fn row_differences(check: &dyn Fn(&str, bool, String)) {
    let core = worktree_core();
    let remote = remote_view(&core, remote_inputs());
    let local = remote_view(&core, local_inputs());
    let tooltip_of = |view: &SidebarView, group_id: &str| -> Option<String> {
        view.groups
            .iter()
            .find(|group| group.core.group_id == group_id)
            .and_then(|group| group.core.sessions.first())
            .map(|session| session.row.title_tooltip.clone())
    };
    let remote_tooltip = tooltip_of(&remote, &format!("remote:{MACHINE}:group:R1"));
    let local_tooltip = tooltip_of(&local, "combined-project:P1");
    check(
        "a remote row's tooltip always carries the state line",
        remote_tooltip
            .as_deref()
            .is_some_and(|tooltip| tooltip.contains("Sleeping")),
        format!("{remote_tooltip:?}"),
    );
    check(
        "a local row's does not",
        local_tooltip
            .as_deref()
            .is_some_and(|tooltip| !tooltip.contains("Sleeping")),
        format!("{local_tooltip:?}"),
    );
    let worktrees_in = |view: &SidebarView, group_id: &str| -> Option<String> {
        view.groups
            .iter()
            .find(|group| group.core.group_id == group_id)
            .and_then(|group| group.core.title_tooltip.clone())
    };
    check(
        "a local parent project counts its worktree",
        worktrees_in(&local, "combined-project:P1")
            .is_some_and(|tooltip| tooltip.contains("1 worktree")),
        format!("{:?}", worktrees_in(&local, "combined-project:P1")),
    );
    check(
        "a remote parent project counts none, exactly as the projection does",
        worktrees_in(&remote, &format!("remote:{MACHINE}:group:R1"))
            .is_some_and(|tooltip| tooltip.contains("0 worktrees")),
        format!(
            "{:?}",
            worktrees_in(&remote, &format!("remote:{MACHINE}:group:R1"))
        ),
    );
    let facts_of = |view: &SidebarView, group_id: &str, index: usize| {
        view.groups
            .iter()
            .find(|group| group.core.group_id == group_id)
            .and_then(|group| group.core.sessions.get(index))
            .map(|session| {
                (
                    session.row.menu_facts.can_schedule_delayed_send,
                    session.row.menu_facts.can_toggle_close_after_done,
                )
            })
    };
    check(
        "a remote terminal row opts into the host timers",
        facts_of(&remote, &format!("remote:{MACHINE}:group:R1"), 0) == Some((true, true)),
        format!(
            "{:?}",
            facts_of(&remote, &format!("remote:{MACHINE}:group:R1"), 0)
        ),
    );
    check(
        "a remote row of another kind does not",
        facts_of(&remote, &format!("remote:{MACHINE}:group:R2"), 0) == Some((false, false)),
        format!(
            "{:?}",
            facts_of(&remote, &format!("remote:{MACHINE}:group:R2"), 0)
        ),
    );
    check(
        "a local row always does",
        facts_of(&local, "combined-project:P1", 0) == Some((true, true)),
        format!("{:?}", facts_of(&local, "combined-project:P1", 0)),
    );
}

fn drawn_groups(view: &SidebarView) -> Vec<String> {
    view.groups
        .iter()
        .map(|group| group.core.group_id.clone())
        .collect()
}

fn remote_view(core: &Core, inputs: SidebarInputs) -> SidebarView {
    SidebarViewModel::build_from_scratch(core, &inputs, NOW_MS)
}

fn machines() -> Vec<MachineTabInput> {
    vec![
        MachineTabInput {
            machine_id: "local".to_string(),
            label: "Local".to_string(),
            state: MACHINE_STATE_CONNECTED.to_string(),
            message: None,
            fed: true,
            this_computer: false,
        },
        MachineTabInput {
            machine_id: MACHINE.to_string(),
            label: "Kubuntu VM".to_string(),
            state: MACHINE_STATE_CONNECTED.to_string(),
            message: None,
            fed: true,
            this_computer: false,
        },
    ]
}

fn local_inputs() -> SidebarInputs {
    let mut inputs = SidebarInputs::default();
    inputs.host.machines = machines();
    inputs
}

fn remote_inputs() -> SidebarInputs {
    let mut inputs = local_inputs();
    inputs.ui.selected_machine_id = MACHINE.to_string();
    inputs
}

/// One local project with a working session, and one remote machine with two projects: a terminal
/// row that is working and needing attention, and a row of another kind.
fn two_machine_core() -> Core {
    let mut core = Core::new();
    core.handle_raw_frame(
        MachineId::Local,
        &snapshot(&["P1"], "local").to_string(),
        NOW_MS,
    )
    .expect("the frame parses");
    core.handle_raw_frame(
        MachineId::Remote(MACHINE.to_string()),
        &snapshot(&["R1", "R2"], "remote").to_string(),
        NOW_MS,
    )
    .expect("the frame parses");
    core
}

/// The same two machines, with a worktree of the first project on each of them.
fn worktree_core() -> Core {
    let mut core = two_machine_core();
    for (machine, server_id, parent, worktree, projects) in [
        (MachineId::Local, "local", "P1", "PW", vec!["P1", "PW"]),
        (
            MachineId::Remote(MACHINE.to_string()),
            "remote",
            "R1",
            "RW",
            vec!["R1", "R2", "RW"],
        ),
    ] {
        core.handle(
            Event::DomainProjectsRead {
                machine: machine.clone(),
                projects: vec![json!({
                    "projectId": worktree,
                    "name": worktree,
                    "path": format!("/tmp/{worktree}"),
                    "worktree": {
                        "branch": "feature",
                        "name": worktree,
                        "parentProjectId": parent,
                        "parentProjectName": parent,
                        "parentProjectPath": format!("/tmp/{parent}"),
                    },
                })],
            },
            NOW_MS,
        );
        let _ = worktree;
        core.handle_raw_frame(
            machine,
            &snapshot_with(&projects, server_id, 2).to_string(),
            NOW_MS,
        )
        .expect("the frame parses");
    }
    core
}

fn snapshot(projects: &[&str], server_id: &str) -> Value {
    snapshot_with(projects, server_id, 1)
}

fn snapshot_with(projects: &[&str], server_id: &str, revision: i64) -> Value {
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "revision": revision,
        "snapshot": {
            "revision": revision,
            "generatedAt": "2026-09-20T00:00:00.000Z",
            "projects": projects.iter().map(|id| project(id)).collect::<Vec<_>>(),
            "groups": projects.iter().map(|id| group(id)).collect::<Vec<_>>(),
            "sessions": projects.iter().map(|id| session(id)).collect::<Vec<_>>(),
        }
    })
}

fn project(project_id: &str) -> Value {
    json!({
        "projectId": project_id,
        "title": project_id,
        "path": format!("/tmp/{project_id}"),
        "pathState": "available",
        "groupIds": [format!("{project_id}:active")],
        "sortKey": format!("1:{project_id}"),
        "createdAt": "2026-06-29T13:10:42.091Z",
        "updatedAt": "2026-09-15T01:18:43.055Z",
    })
}

fn group(project_id: &str) -> Value {
    json!({
        "groupId": format!("{project_id}:active"),
        "projectId": project_id,
        "title": "Active",
        "sessionIds": [format!("{}S1", &project_id[..1])],
        "sortKey": format!("1:{project_id}:active"),
    })
}

/// `R2`'s row is a browser session, which is the kind a remote machine withholds the two host
/// timers from; every other row is a sleeping terminal that is working.
fn session(project_id: &str) -> Value {
    let session_id = format!("{}S1", &project_id[..1]);
    let browser = project_id == "R2";
    json!({
        "sessionId": session_id,
        "projectId": project_id,
        "groupId": format!("{project_id}:active"),
        "kind": if browser { "browser" } else { "agent" },
        "surface": "workspace",
        "zmxName": format!("S90-{project_id}"),
        "sortKey": format!("000000000000:0:1:{session_id}"),
        "visibleInSidebarByDefault": true,
        "alias": format!("Session {session_id}"),
        "activity": "working",
        "pendingQuestionCount": if browser { 1 } else { 0 },
        "lifecycleState": "sleeping",
        "lastInteractionAt": "2026-09-15T01:18:43.055Z",
        "createdAt": "2026-09-15T01:00:00.000Z",
        "updatedAt": "2026-09-15T01:18:43.055Z",
    })
}
