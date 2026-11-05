//! Checks the Space rules the drawn list cannot answer on its own: that a reveal reaches a row in
//! another Space, that the follow-active rule does too, that a hidden collection does not move the
//! section to a Space the user was not in, and that a worktree follows its parent project.
//!
//! Usage: `cargo run --example sidebar_reveal_spaces`
//!
//! It needs no recording, and that is the point: a recording made on a machine with no Spaces runs
//! every reveal with the Space branch inert, so it passes whether the branch works or not. The two
//! cases here are the ones the drawn list cannot answer on its own.
//!
//! 1. The row is in a Space the section is not filtered by. The drawn list does not hold it at
//!    all, so the plan has to come from a list built with nothing filtering, and it must say which
//!    Space to move to. `reveal.ts` reads `state.groupOrder`, the unfiltered inventory, for the
//!    same reason.
//! 2. The row's project is inside a collection the user hid, with Show Hidden off, and the Space
//!    claims that collection rather than the project. The collection is dropped from the drawn
//!    list while its projects are still drawn, so a plan that read the collection from there would
//!    answer the Space question on the wrong branch and move the section for no reason.
//!
//! It also covers `project_slot_plan` (cmd+1..9), because the fixture it needs is exactly this
//! one: several local projects drawn in a known order, one of them collapsed. Building a tenth
//! presentation for six assertions would have been the more expensive answer.
//!
//! 3. Seven Spaces and three collections, the shape of a real sidebar rather than the smallest one
//!    that exercises a branch: the follow-active rule on a row in another Space, a worktree whose
//!    Space is its parent project's, two Spaces claiming one project so the order tie-break has a
//!    choice, and a section sitting on the built-in Other view.
//!
//! 4. Group working sessions on: a working row that is not the focused one sits under the
//!    project's folded Working heading, and a reveal opens that heading.
//!
//! This is tooling, not a test suite; it prints what it found and fails the process on a
//! difference.

use std::process::ExitCode;

use ghostex_gx_core::{
    project_slot_plan, reveal_plan, space_for_focused_row, Core, Event, MachineId, SectionId,
    SidebarInputs, SidebarView, SidebarViewModel,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const SPACE_A: &str = "space-a";
const SPACE_B: &str = "space-b";
const COLLECTION: &str = "project-collection-1";

fn main() -> ExitCode {
    let failures = std::cell::Cell::new(0usize);
    let check = |label: &str, ok: bool, detail: String| {
        println!("{:<56} {}", label, if ok { "ok" } else { "DIFFERS" });
        if !ok {
            failures.set(failures.get() + 1);
            println!("  {detail}");
        }
    };

    // Case 1: two Spaces, one project each, the section filtered by the first.
    // No collection holds either project here: a project inside one follows the collection, which
    // is the second case rather than this one.
    let core = core_with(spaces_by_project(), &[]);
    let mut inputs = base_inputs();
    inputs
        .ui
        .collapse
        .selected_space_by_section
        .insert("local".to_string(), SPACE_A.to_string());
    let view = SidebarViewModel::build_from_scratch(&core, &inputs, NOW_MS);
    if std::env::var("DEBUG_ROWS").is_ok() {
        for group in &view.groups {
            println!(
                "group {} storage {} collection {:?} rows {:?}",
                group.core.group_id,
                group.core.storage_id,
                group.collection_id,
                group
                    .core
                    .sessions
                    .iter()
                    .map(|session| session.row.sidebar_session_id.clone())
                    .collect::<Vec<_>>()
            );
        }
    }
    check(
        "the drawn list holds only the selected Space's project",
        drawn_groups(&view) == vec!["combined-project:P1".to_string()],
        format!("{:?}", drawn_groups(&view)),
    );
    let plan = reveal_plan(&core, &inputs, &view, "combined-session:P2:S2", NOW_MS);
    check(
        "a reveal finds a row in another Space at all",
        plan.is_some(),
        "reveal_plan returned None: the row was looked for in the filtered list".to_string(),
    );
    check(
        "the reveal names the group holding it",
        plan.as_ref().map(|plan| plan.group_id.as_str()) == Some("combined-project:P2"),
        format!("{:?}", plan.as_ref().map(|plan| &plan.group_id)),
    );
    check(
        "the reveal moves the section to that Space",
        plan.as_ref().and_then(|plan| plan.select_space.as_deref()) == Some(SPACE_B),
        format!(
            "{:?}",
            plan.as_ref().and_then(|plan| plan.select_space.clone())
        ),
    );
    // The row the section already shows needs no move.
    let same = reveal_plan(&core, &inputs, &view, "combined-session:P1:S1", NOW_MS);
    check(
        "a reveal inside the selected Space moves nothing",
        same.as_ref()
            .is_some_and(|plan| plan.select_space.is_none()),
        format!("{:?}", same.as_ref().map(|plan| plan.select_space.clone())),
    );

    // Case 2: the second project is in a collection the first Space claims, and that collection is
    // hidden. The project is still drawn, and the section must stay where it is.
    let core = core_with(spaces_by_collection(), &["P2"]);
    let mut inputs = base_inputs();
    inputs
        .ui
        .collapse
        .selected_space_by_section
        .insert("local".to_string(), SPACE_A.to_string());
    inputs
        .ui
        .hidden_items
        .collection_keys
        .push(format!("local:{COLLECTION}"));
    let view = SidebarViewModel::build_from_scratch(&core, &inputs, NOW_MS);
    check(
        "a hidden collection is dropped while its projects are drawn",
        view.collections.is_empty()
            && drawn_groups(&view).contains(&"combined-project:P2".to_string()),
        format!(
            "collections {:?} groups {:?}",
            view.collections.len(),
            drawn_groups(&view)
        ),
    );
    let plan = reveal_plan(&core, &inputs, &view, "combined-session:P2:S2", NOW_MS);
    check(
        "a project in a hidden collection does not move the Space",
        plan.as_ref()
            .is_some_and(|plan| plan.select_space.is_none()),
        format!(
            "{:?}",
            plan.as_ref().and_then(|plan| plan.select_space.clone())
        ),
    );
    check(
        "and its hidden collection is still the one to unhide",
        plan.as_ref().is_some_and(|plan| plan.show_hidden),
        format!("{:?}", plan.as_ref().map(|plan| plan.show_hidden)),
    );

    seven_spaces(&check);
    two_machines(&check);
    working_section(&check);

    if failures.get() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A sidebar the size of a real one: seven Spaces, three collections, a worktree, and a project
/// two Spaces both claim.
fn seven_spaces(check: &dyn Fn(&str, bool, String)) {
    let core = seven_space_core();
    let selected = |space_id: &str| {
        let mut inputs = base_inputs();
        inputs.settings.sidebar_space_follow_active_session = true;
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .insert("local".to_string(), space_id.to_string());
        inputs
    };
    let inputs = selected("s1");
    let view = SidebarViewModel::build_from_scratch(&core, &inputs, NOW_MS);
    check(
        "seven Spaces: the section draws only its own",
        drawn_groups(&view) == vec!["combined-project:P1".to_string()],
        format!("{:?}", drawn_groups(&view)),
    );

    // The rule this whole function exists for: the focused row is in another Space.
    //
    // `space_for_focused_row` answers two things since M5 piece 7c: the Space the row belongs to
    // (which is remembered whatever the setting says) and whether the section MOVES to it. These
    // assertions are about the move, so they read `follow` and the Space together; the Space alone
    // is asserted below.
    let resolved = |inputs: &SidebarInputs, view: &SidebarView, session: &str| {
        space_for_focused_row(&core, inputs, view, session, NOW_MS)
    };
    let follow = |inputs: &SidebarInputs, view: &SidebarView, session: &str| {
        resolved(inputs, view, session)
            .filter(|resolved| resolved.follow)
            .map(|resolved| resolved.space_id)
    };
    check(
        "follow-active moves to the focused row's Space",
        follow(&inputs, &view, "combined-session:P4:S4").as_deref() == Some("s4"),
        format!("{:?}", follow(&inputs, &view, "combined-session:P4:S4")),
    );
    check(
        "follow-active moves to a Space at the far end of the order",
        follow(&inputs, &view, "combined-session:P7:S7").as_deref() == Some("s7"),
        format!("{:?}", follow(&inputs, &view, "combined-session:P7:S7")),
    );
    check(
        "follow-active leaves a row the section already shows alone",
        follow(&inputs, &view, "combined-session:P1:S1").is_none(),
        format!("{:?}", follow(&inputs, &view, "combined-session:P1:S1")),
    );
    let mut off = inputs.clone();
    off.settings.sidebar_space_follow_active_session = false;
    check(
        "follow-active does nothing with the setting off",
        follow(&off, &view, "combined-session:P4:S4").is_none(),
        format!("{:?}", follow(&off, &view, "combined-session:P4:S4")),
    );

    // A worktree belongs to the Space that claims the project it was cut from.
    check(
        "a worktree follows its parent project's Space",
        follow(&inputs, &view, "combined-session:PW:SW").as_deref() == Some("s3"),
        format!("{:?}", follow(&inputs, &view, "combined-session:PW:SW")),
    );

    // Two Spaces name P5; the earlier one in `order` is the one that claims it.
    check(
        "the earlier Space wins a project two of them claim",
        follow(&inputs, &view, "combined-session:P5:S5").as_deref() == Some("s5"),
        format!("{:?}", follow(&inputs, &view, "combined-session:P5:S5")),
    );

    // A project no Space claims lives in the built-in Other view.
    let other = selected("other");
    let other_view = SidebarViewModel::build_from_scratch(&core, &other, NOW_MS);
    check(
        "Other draws the project no Space claims",
        drawn_groups(&other_view) == vec!["combined-project:P8".to_string()],
        format!("{:?}", drawn_groups(&other_view)),
    );
    check(
        "follow-active moves off Other into a claimed row's Space",
        follow(&other, &other_view, "combined-session:P1:S1").as_deref() == Some("s1"),
        format!(
            "{:?}",
            follow(&other, &other_view, "combined-session:P1:S1")
        ),
    );

    // The memory is written whatever the follow setting says, and under the row's own Space: with
    // the setting off nothing moves and the row is still remembered where it belongs.
    check(
        "the Space memory answers with the setting off",
        resolved(&off, &view, "combined-session:P4:S4")
            .is_some_and(|resolved| resolved.space_id == "s4" && !resolved.follow),
        format!("{:?}", resolved(&off, &view, "combined-session:P4:S4")),
    );
    check(
        "a row the section already shows is still remembered, under the shown Space",
        resolved(&inputs, &view, "combined-session:P1:S1")
            .is_some_and(|resolved| resolved.space_id == "s1" && !resolved.follow),
        format!("{:?}", resolved(&inputs, &view, "combined-session:P1:S1")),
    );
    check(
        "the memory is keyed by the section the tab is on",
        resolved(&inputs, &view, "combined-session:P4:S4")
            .is_some_and(|resolved| resolved.section_key == "local"),
        format!("{:?}", resolved(&inputs, &view, "combined-session:P4:S4")),
    );

    // The project slot hotkeys, on the same nine-project fixture rather than a tenth one built for
    // six assertions. `project_slot_plan` reads the DRAWN list, so it needs a list with several
    // projects in a known order, which this one is once Spaces are off.
    let mut all = base_inputs();
    all.settings.sidebar_spaces_enabled = false;
    all.ui
        .collapse
        .collapsed_groups
        .insert("combined-project:P2".to_string());
    let all_view = SidebarViewModel::build_from_scratch(&core, &all, NOW_MS);
    let slot = |inputs: &SidebarInputs, view: &SidebarView, number: u32| {
        project_slot_plan(view, &inputs.ui, &inputs.settings, number)
    };
    let drawn = drawn_groups(&all_view);
    check(
        "slot hotkey: the nth drawn project, in the drawn order",
        slot(&all, &all_view, 2).map(|plan| plan.group_id) == drawn.get(1).cloned(),
        format!("{:?} of {drawn:?}", slot(&all, &all_view, 2)),
    );
    // Asked for every slot rather than for one: `|| drawn.len() >= 9` would have made this pass
    // whatever the answer on a fixture with nine projects, which is the gate-that-cannot-fail shape
    // this port keeps hitting.
    check(
        "slot hotkey: a slot names a project exactly while one is drawn there",
        (1..=9u32).all(|number| {
            slot(&all, &all_view, number).is_some() == (number as usize <= drawn.len())
        }),
        format!(
            "{:?} of {} drawn",
            (1..=9u32)
                .map(|number| slot(&all, &all_view, number).is_some())
                .collect::<Vec<_>>(),
            drawn.len()
        ),
    );
    check(
        "slot hotkey: 0 and 10 are refused",
        slot(&all, &all_view, 0).is_none() && slot(&all, &all_view, 10).is_none(),
        format!(
            "{:?} {:?}",
            slot(&all, &all_view, 0),
            slot(&all, &all_view, 10)
        ),
    );
    let collapsed_slot = drawn
        .iter()
        .position(|group_id| group_id == "combined-project:P2")
        .map(|index| index as u32 + 1)
        .unwrap_or_default();
    check(
        "slot hotkey: an expanded project is left expanded",
        slot(&all, &all_view, 1).is_some_and(|plan| !plan.was_collapsed && !plan.expand_group),
        format!("{:?}", slot(&all, &all_view, 1)),
    );
    check(
        "slot hotkey: a collapsed project is expanded",
        slot(&all, &all_view, collapsed_slot)
            .is_some_and(|plan| plan.was_collapsed && plan.expand_group),
        format!("{:?}", slot(&all, &all_view, collapsed_slot)),
    );
    let mut no_expand = all.clone();
    no_expand.settings.expand_collapsed_projects_on_jump = false;
    check(
        "slot hotkey: with the jump setting off the project stays collapsed",
        slot(&no_expand, &all_view, collapsed_slot)
            .is_some_and(|plan| plan.was_collapsed && !plan.expand_group),
        format!("{:?}", slot(&no_expand, &all_view, collapsed_slot)),
    );

    // And a reveal, on the same presentation, answers the same way.
    let plan = reveal_plan(&core, &inputs, &view, "combined-session:P7:S7", NOW_MS);
    check(
        "a reveal across seven Spaces names the Space and the group",
        plan.as_ref().and_then(|plan| plan.select_space.as_deref()) == Some("s7")
            && plan.as_ref().map(|plan| plan.group_id.as_str()) == Some("combined-project:P7"),
        format!(
            "{:?}",
            plan.as_ref()
                .map(|plan| (plan.select_space.clone(), plan.group_id.clone()))
        ),
    );
    let worktree_plan = reveal_plan(&core, &inputs, &view, "combined-session:PW:SW", NOW_MS);
    check(
        "a reveal of a worktree names its parent's Space",
        worktree_plan
            .as_ref()
            .and_then(|plan| plan.select_space.as_deref())
            == Some("s3"),
        format!(
            "{:?}",
            worktree_plan.as_ref().map(|plan| plan.select_space.clone())
        ),
    );
}

/// Eight projects, one of them a worktree of the third, in seven Spaces and three collections.
fn seven_space_core() -> Core {
    let projects = ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "PW"];
    let spaces = json!({
        "order": ["s1", "s2", "s3", "s4", "s5", "s6", "s7"],
        "spaces": {
            "s1": space("s1", "One", [], ["P1"]),
            "s2": space("s2", "Two", ["c1"], []),
            "s3": space("s3", "Three", [], ["P3"]),
            "s4": space("s4", "Four", [], ["P4"]),
            // Both name P5; `sanitizeSidebarSpacesState` gives it to the earlier one.
            "s5": space("s5", "Five", [], ["P5"]),
            "s6": space("s6", "Six", ["c2"], ["P5", "P6"]),
            "s7": space("s7", "Seven", [], ["P7"]),
        }
    });
    let frame = json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": "reveal-spaces",
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-09-20T00:00:00.000Z",
            "projects": projects
                .iter()
                .map(|project_id| project(project_id, project_id))
                .collect::<Vec<_>>(),
            "groups": projects.iter().map(|project_id| group(project_id)).collect::<Vec<_>>(),
            "sessions": projects
                .iter()
                .map(|project_id| session(project_id, &format!("S{}", &project_id[1..])))
                .collect::<Vec<_>>(),
            "sidebarSpaces": spaces,
            "sidebarProjectCollections": {
                "order": ["c1", "c2", "c3"],
                "collections": {
                    "c1": collection("c1", "Held", ["P2"]),
                    "c2": collection("c2", "Six", ["P6"]),
                    "c3": collection("c3", "Empty", []),
                },
            },
        }
    });
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &frame.to_string(), NOW_MS)
        .expect("the frame parses");
    // The worktree metadata rides on the domain rows, not on the presentation.
    core.handle(
        Event::DomainProjectsRead {
            machine: MachineId::Local,
            projects: vec![json!({
                "projectId": "PW",
                "name": "PW",
                "path": "/tmp/PW",
                "worktree": {
                    "branch": "feature",
                    "name": "PW",
                    "parentProjectId": "P3",
                    "parentProjectName": "P3",
                    "parentProjectPath": "/tmp/P3",
                },
            })],
        },
        NOW_MS,
    );
    core
}

fn collection(
    collection_id: &str,
    title: &str,
    projects: impl IntoIterator<Item = &'static str>,
) -> Value {
    json!({
        "collectionId": collection_id,
        "title": title,
        "color": "#3aa675",
        "projectIds": projects.into_iter().collect::<Vec<_>>(),
    })
}

/// Two machines, because the section the per-Space session memory and the Space follow are keyed by
/// is the ROW's machine and not the machine tab.
///
/// The single-machine fixture above cannot tell the two apart: `section_key` is `"local"` whichever
/// rule is in force, which is exactly what the first cut of the assertion locked in. With the tab
/// on the remote machine, a focused LOCAL row has to answer for the local section, and the tab must
/// not move; a reveal of the same row moves the tab, which is the one difference between them.
fn two_machines(check: &dyn Fn(&str, bool, String)) {
    let core = two_machine_core();
    // The tab is on the remote machine; the focused row is on this computer.
    let mut remote_tab = base_inputs();
    remote_tab.settings.sidebar_space_follow_active_session = true;
    remote_tab.ui.selected_machine_id = "m1".to_string();
    let remote_view = SidebarViewModel::build_from_scratch(&core, &remote_tab, NOW_MS);
    let local_row = "combined-session:P1:PS1";
    let answer = space_for_focused_row(&core, &remote_tab, &remote_view, local_row, NOW_MS);
    check(
        "two machines: a local row answers for the local section from a remote tab",
        answer
            .as_ref()
            .is_some_and(|resolved| resolved.section_key == "local" && resolved.space_id == "s1"),
        format!("{answer:?}"),
    );
    check(
        "two machines: and the follow still applies, without moving the tab",
        answer.as_ref().is_some_and(|resolved| resolved.follow),
        format!("{answer:?}"),
    );
    // The mirror image: the tab is local and the focused row is on the remote machine, which is
    // where the remote section's own Space document decides the answer.
    let mut local_tab = base_inputs();
    local_tab.settings.sidebar_space_follow_active_session = true;
    let local_view = SidebarViewModel::build_from_scratch(&core, &local_tab, NOW_MS);
    let remote_row = "remote:m1:session:R1:RS1";
    let remote_answer = space_for_focused_row(&core, &local_tab, &local_view, remote_row, NOW_MS);
    check(
        "two machines: a remote row answers for its own section and its own Space",
        remote_answer.as_ref().is_some_and(|resolved| {
            resolved.section_key == "remote:m1" && resolved.space_id == "r1"
        }),
        format!("{remote_answer:?}"),
    );
    // A reveal of the same row is the one that moves the tab.
    let revealed = reveal_plan(&core, &local_tab, &local_view, remote_row, NOW_MS);
    check(
        "two machines: a reveal of that row moves the tab, a focus change does not",
        revealed
            .as_ref()
            .is_some_and(|plan| plan.select_machine.as_deref() == Some("m1")),
        format!(
            "{:?}",
            revealed.as_ref().map(|plan| plan.select_machine.clone())
        ),
    );
}

/// This computer with one Space and one project, and a remote machine with its own.
fn two_machine_core() -> Core {
    let mut core = Core::new();
    let frame = |server_id: &str, projects: &[&str], spaces: Value| {
        json!({
            "type": "presentationSnapshot",
            "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": server_id,
            "revision": 1,
            "snapshot": {
                "revision": 1,
                "generatedAt": "2026-09-20T00:00:00.000Z",
                "projects": projects.iter().map(|id| project(id, id)).collect::<Vec<_>>(),
                "groups": projects.iter().map(|id| group(id)).collect::<Vec<_>>(),
                "sessions": projects
                    .iter()
                    .map(|id| session(id, &format!("{}S{}", &id[..1], &id[1..])))
                    .collect::<Vec<_>>(),
                "sidebarSpaces": spaces,
            }
        })
    };
    core.handle_raw_frame(
        MachineId::Local,
        &frame(
            "local",
            &["P1"],
            json!({"order": ["s1"], "spaces": {"s1": space("s1", "One", [], ["P1"])}}),
        )
        .to_string(),
        NOW_MS,
    )
    .expect("the local frame parses");
    core.handle_raw_frame(
        MachineId::Remote("m1".to_string()),
        &frame(
            "m1",
            &["R1"],
            json!({"order": ["r1"], "spaces": {"r1": space("r1", "Remote", [], ["R1"])}}),
        )
        .to_string(),
        NOW_MS,
    )
    .expect("the remote frame parses");
    core
}

/// CDXC:Sidebar 2026-11-05: Reveal Active Session for a working row with Group working sessions
/// on. The focused row stays out of Working (`held_out_of_working`), but a reveal can name any
/// row (a notification, a session link), and one under the folded Working heading must open it.
fn working_section(check: &dyn Fn(&str, bool, String)) {
    let mut busy = session("P2", "S2");
    busy["activity"] = json!("working");
    busy["lifecycleState"] = json!("running");
    let core = core_with_sessions(spaces_by_project(), &[], vec![session("P1", "S1"), busy]);
    let mut inputs = SidebarInputs::default();
    inputs.settings.group_working_sessions = true;
    let view = SidebarViewModel::build_from_scratch(&core, &inputs, NOW_MS);
    let plan = reveal_plan(&core, &inputs, &view, "combined-session:P2:S2", NOW_MS);
    check(
        "a working row's reveal opens its folded Working heading",
        plan.as_ref().and_then(|plan| plan.collapsed_section) == Some(SectionId::Working),
        format!("{:?}", plan.as_ref().map(|plan| plan.collapsed_section)),
    );
    inputs.settings.group_working_sessions = false;
    let view = SidebarViewModel::build_from_scratch(&core, &inputs, NOW_MS);
    let plan = reveal_plan(&core, &inputs, &view, "combined-session:P2:S2", NOW_MS);
    check(
        "with the setting off it is in Sessions, which is open",
        plan.as_ref()
            .is_some_and(|plan| plan.collapsed_section.is_none()),
        format!("{:?}", plan.as_ref().map(|plan| plan.collapsed_section)),
    );
}

fn drawn_groups(view: &SidebarView) -> Vec<String> {
    view.groups
        .iter()
        .map(|group| group.core.group_id.clone())
        .collect()
}

fn base_inputs() -> SidebarInputs {
    let mut inputs = SidebarInputs::default();
    inputs.settings.sidebar_spaces_enabled = true;
    inputs
}

/// Each Space owns one project outright.
fn spaces_by_project() -> Value {
    json!({
        "order": [SPACE_A, SPACE_B],
        "spaces": {
            SPACE_A: space(SPACE_A, "One", [], ["P1"]),
            SPACE_B: space(SPACE_B, "Two", [], ["P2"]),
        }
    })
}

/// The first Space owns a collection, and the second project is in it.
fn spaces_by_collection() -> Value {
    json!({
        "order": [SPACE_A, SPACE_B],
        "spaces": {
            SPACE_A: space(SPACE_A, "One", [COLLECTION], ["P1"]),
            SPACE_B: space(SPACE_B, "Two", [], []),
        }
    })
}

fn space(
    space_id: &str,
    name: &str,
    collections: impl IntoIterator<Item = &'static str>,
    projects: impl IntoIterator<Item = &'static str>,
) -> Value {
    json!({
        "spaceId": space_id,
        "name": name,
        "color": "#4f5663",
        "icon": "stack",
        "memberCollectionIds": collections.into_iter().collect::<Vec<_>>(),
        "memberProjectIds": projects.into_iter().collect::<Vec<_>>(),
    })
}

/// A store holding two projects, one group and one session each, with the given Spaces document
/// and a collection holding whichever projects are named.
fn core_with(spaces: Value, collection_projects: &[&str]) -> Core {
    core_with_sessions(
        spaces,
        collection_projects,
        vec![session("P1", "S1"), session("P2", "S2")],
    )
}

fn core_with_sessions(spaces: Value, collection_projects: &[&str], sessions: Vec<Value>) -> Core {
    let frame = json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": "reveal-spaces",
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-09-20T00:00:00.000Z",
            "projects": [project("P1", "One"), project("P2", "Two")],
            "groups": [group("P1"), group("P2")],
            "sessions": sessions,
            "sidebarSpaces": spaces,
            "sidebarProjectCollections": {
                "collections": {
                    COLLECTION: {
                        "collectionId": COLLECTION,
                        "title": "Held",
                        "color": "#3aa675",
                        "projectIds": collection_projects,
                    }
                },
                "order": [COLLECTION],
            },
        }
    });
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &frame.to_string(), NOW_MS)
        .expect("the frame parses");
    core
}

fn project(project_id: &str, title: &str) -> Value {
    json!({
        "projectId": project_id,
        "title": title,
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
        "sessionIds": [format!("S{}", &project_id[1..])],
        "sortKey": format!("1:{project_id}:active"),
    })
}

fn session(project_id: &str, session_id: &str) -> Value {
    json!({
        "sessionId": session_id,
        "projectId": project_id,
        "groupId": format!("{project_id}:active"),
        "kind": "agent",
        "surface": "workspace",
        "zmxName": format!("S90-{project_id}-{session_id}"),
        "sortKey": format!("000000000000:0:1:{session_id}"),
        "visibleInSidebarByDefault": true,
        "alias": format!("Session {session_id}"),
        "activity": "idle",
        "lifecycleState": "sleeping",
        "lastInteractionAt": "2026-09-15T01:18:43.055Z",
        "createdAt": "2026-09-15T01:00:00.000Z",
        "updatedAt": "2026-09-15T01:18:43.055Z",
    })
}
