//! The rules for a large setup (issue #359): the strip, grouping, the saved order, Hide, the search
//! field, remembered state and where a new session starts.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::json;

use crate::keys::{MachineId, SessionKey};
use crate::sidebar_menu::{hover_strip, unified_default_hover_buttons, HoverAction};
use crate::sidebar_view::view::SpaceProject;
use crate::sidebar_view::{
    GroupCore, GroupSummary, GroupView, ProjectContextView, SessionMenuFacts, SessionRow,
    SessionTiming, SessionView, SidebarInputs, SidebarView,
};

use super::*;

const NOW: u64 = 1_000_000;

fn row(machine: &MachineId, project: &str, id: &str, last: i64) -> SessionRow {
    SessionRow {
        sidebar_session_id: id.to_string(),
        display_title: format!("title of {id}"),
        key: Some(SessionKey {
            machine: machine.clone(),
            project_id: project.to_string(),
            session_id: id.to_string(),
        }),
        activity: "idle".to_string(),
        timing: SessionTiming {
            created_ms: Some(1),
            last_interaction_ms: Some(last),
            ..SessionTiming::default()
        },
        ..SessionRow::default()
    }
}

fn group(project: &str, origin: &str, sessions: Vec<SessionRow>) -> GroupView {
    GroupView {
        core: Arc::new(GroupCore {
            group_id: format!("project:{project}"),
            storage_id: project.to_string(),
            title: project.to_string(),
            title_tooltip: None,
            is_active: false,
            project_context: Some(ProjectContextView {
                project_id: project.to_string(),
                path: format!("/{project}"),
                git_remote_origin_url: Some(origin.to_string()),
                ..ProjectContextView::default()
            }),
            summary: GroupSummary::default(),
            collapsed: false,
            hover_actions_expanded: false,
            sections: Vec::new(),
            sessions: sessions
                .into_iter()
                .map(|row| SessionView {
                    row: Arc::new(row),
                    is_focused: false,
                    is_visible: false,
                    is_multi_selected: false,
                    nesting: Default::default(),
                })
                .collect(),
            remote_machine: None,
            is_stale: false,
            machine_names: Vec::new(),
            action_target: None,
            folded_worktrees: Vec::new(),
        }),
        collection_color: None,
        collection_id: None,
        space_project: Some(SpaceProject {
            project_id: project.to_string(),
            parent_project_id: None,
            collection_id: None,
        }),
        holds_active_session_group: false,
    }
}

fn sidebar(groups: Vec<GroupView>) -> SidebarView {
    SidebarView {
        groups,
        ..SidebarView::default()
    }
}

const SETTINGS: UnifiedSettings = UnifiedSettings {
    working_section: true,
    enable_session_parking: true,
    pinned_session_order: crate::sidebar_view::PinnedSessionOrder::Manual,
};

fn studio() -> MachineId {
    MachineId::Remote("studio".to_string())
}

/// `acme` on two machines with `billing` and `zebra` here.
fn two_machines() -> (SidebarView, SidebarView) {
    let local = sidebar(vec![
        group(
            "zebra",
            "https://github.com/o/zebra",
            vec![row(&MachineId::Local, "zebra", "z1", 900)],
        ),
        group(
            "acme",
            "https://github.com/o/acme",
            vec![
                row(&MachineId::Local, "acme", "a1", 100),
                {
                    let mut pinned = row(&MachineId::Local, "acme", "a-pin", 50);
                    pinned.is_pinned = true;
                    pinned
                },
                {
                    let mut working = row(&MachineId::Local, "acme", "a-work", 500);
                    working.activity = "working".to_string();
                    working
                },
                {
                    let mut parked = row(&MachineId::Local, "acme", "a-park", 400);
                    parked.is_parked = true;
                    parked
                },
                {
                    let mut draft = row(&MachineId::Local, "acme", "a-draft", 600);
                    draft.is_draft = true;
                    draft
                },
            ],
        ),
        group(
            "billing",
            "https://github.com/o/billing",
            vec![row(&MachineId::Local, "billing", "b1", 300)],
        ),
    ]);
    let remote = sidebar(vec![group(
        "racme",
        "git@github.com:o/acme.git",
        vec![{
            let mut snoozed = row(&studio(), "racme", "r1", 700);
            snoozed.timing.snoozed_until_ms = Some(NOW as i64 + 10_000);
            snoozed
        }],
    )]);
    (local, remote)
}

fn build(local: &SidebarView, remote: &SidebarView, filter: &UnifiedFilter) -> UnifiedSidebarView {
    build_unified_sidebar(
        &[
            UnifiedMachineInput {
                machine: MachineId::Local,
                label: "Local".to_string(),
                view: local,
                spaces: None,
            },
            UnifiedMachineInput {
                machine: studio(),
                label: "Studio".to_string(),
                view: remote,
                spaces: None,
            },
        ],
        SETTINGS,
        filter,
        NOW,
    )
}

fn ids(rows: &[UnifiedRow]) -> Vec<&str> {
    rows.iter()
        .map(|row| row.session.row.sidebar_session_id.as_str())
        .collect()
}

#[test]
fn grouping_puts_every_machines_sessions_of_a_repository_under_one_header_with_its_shelves() {
    let (local, remote) = two_machines();
    let view = build(&local, &remote, &UnifiedFilter::default());
    let groups = group_unified_sidebar(&view, &[]);
    // A to Z until the user drags, one header per repository.
    let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
    assert_eq!(labels, ["acme", "billing", "zebra"]);
    let acme = &groups[0];
    assert_eq!(acme.repo_key, "github.com/o/acme");
    assert_eq!(ids(&acme.pinned), ["a-pin"]);
    assert_eq!(ids(&acme.active), ["a1"]);
    // The shelves are the project's own, and the other machine's snoozed row joins them.
    assert_eq!(ids(&acme.working), ["a-work"]);
    assert_eq!(ids(&acme.snoozed), ["r1"]);
    assert_eq!(ids(&acme.parked), ["a-park"]);
    assert_eq!(acme.count(), 5);
    // Drafts stay above every group.
    assert_eq!(ids(&view.drafts), ["a-draft"]);
    assert!(groups
        .iter()
        .all(|group| group.count() > 0 && !ids(&group.active).contains(&"a-draft")));
}

#[test]
fn a_project_the_filter_empties_draws_no_header() {
    let (local, remote) = two_machines();
    let filter = UnifiedFilter {
        query: "title of z1".to_string(),
        ..UnifiedFilter::default()
    };
    let view = build(&local, &remote, &filter);
    let groups = group_unified_sidebar(&view, &[]);
    let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
    assert_eq!(labels, ["zebra"]);
}

#[test]
fn the_saved_order_comes_first_and_a_new_repository_lands_after_it_a_to_z() {
    let (local, remote) = two_machines();
    let view = build(&local, &remote, &UnifiedFilter::default());
    let order = vec!["github.com/o/zebra".to_string()];
    let groups = group_unified_sidebar(&view, &order);
    let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
    assert_eq!(labels, ["zebra", "acme", "billing"]);
}

#[test]
fn the_order_follows_a_repository_across_machines() {
    // The saved key is the repository's, so a machine dropping out keeps its header's place, and
    // that repository on another machine sits there too.
    let (local, remote) = two_machines();
    let order = vec![
        "github.com/o/billing".to_string(),
        "github.com/o/acme".to_string(),
        "github.com/o/zebra".to_string(),
    ];
    let both = build(&local, &remote, &UnifiedFilter::default());
    let labels = |view: &UnifiedSidebarView| -> Vec<String> {
        group_unified_sidebar(view, &order)
            .into_iter()
            .map(|group| group.label)
            .collect()
    };
    assert_eq!(labels(&both), ["billing", "acme", "zebra"]);
    // Only the other machine has acme now; acme keeps its place.
    let only_remote = build_unified_sidebar(
        &[UnifiedMachineInput {
            machine: studio(),
            label: "Studio".to_string(),
            view: &remote,
            spaces: None,
        }],
        SETTINGS,
        &UnifiedFilter::default(),
        NOW,
    );
    assert_eq!(labels(&only_remote), ["acme"]);
}

#[test]
fn dragging_a_header_saves_the_whole_order_so_hidden_projects_keep_their_place() {
    let option = |key: &str| UnifiedFilterOption {
        key: key.to_string(),
        label: key.to_string(),
        detail: None,
        ..UnifiedFilterOption::default()
    };
    let options = [option("a"), option("b"), option("c"), option("d")];
    // `gone` is a saved repository no machine lists right now, `b` is filtered off screen.
    let order: Vec<String> = ["c", "gone", "b", "a"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(
        move_unified_project(&order, &options, "a", "c", false),
        ["a", "c", "gone", "b", "d"]
    );
    assert_eq!(
        move_unified_project(&order, &options, "c", "a", true),
        ["gone", "b", "a", "c", "d"]
    );
    // Dropping on itself only writes the order out.
    assert_eq!(
        move_unified_project(&order, &options, "a", "a", false),
        ["c", "gone", "b", "a", "d"]
    );
    // A repository the order has not met can be dragged too.
    assert_eq!(
        move_unified_project(&[], &options, "d", "a", false),
        ["d", "a", "b", "c"]
    );
}

#[test]
fn the_search_field_matches_title_project_branch_and_machine_on_every_shelf() {
    let (mut local, remote) = two_machines();
    let core = Arc::make_mut(&mut local.groups[2].core);
    let mut branched = (*core.sessions[0].row).clone();
    branched.menu_facts = SessionMenuFacts {
        branch: Some("feature/Invoices".to_string()),
        ..SessionMenuFacts::default()
    };
    core.sessions[0].row = Arc::new(branched);
    let search = |query: &str| {
        let filter = UnifiedFilter {
            query: query.to_string(),
            ..UnifiedFilter::default()
        };
        let view = build(&local, &remote, &filter);
        let mut found: BTreeSet<String> = BTreeSet::new();
        for rows in [
            &view.drafts,
            &view.pinned,
            &view.active,
            &view.working,
            &view.snoozed,
            &view.parked,
        ] {
            found.extend(ids(rows).into_iter().map(str::to_string));
        }
        found.into_iter().collect::<Vec<_>>()
    };
    // A title.
    assert_eq!(search("of Z1"), ["z1"]);
    // A project, on both machines and every shelf.
    assert_eq!(
        search("ACME"),
        ["a-draft", "a-park", "a-pin", "a-work", "a1", "r1"]
    );
    // A branch.
    assert_eq!(search("invoices"), ["b1"]);
    // A machine's name, with another word.
    assert_eq!(search("studio acme"), ["r1"]);
    assert!(search("acme nothing-like-this").is_empty());
    // Spaces between words only.
    assert_eq!(search("   ").len(), 8);
}

#[test]
fn a_search_is_not_a_pick_of_the_filter_menu() {
    let filter = UnifiedFilter {
        query: "x".to_string(),
        ..UnifiedFilter::default()
    };
    assert!(filter.is_empty());
    assert!(!filter.keeps_everything());
    assert!(UnifiedFilter::default().keeps_everything());
}

#[test]
fn a_hidden_project_and_collection_leave_the_list_and_the_filter() {
    let (local, remote) = two_machines();
    let mut hidden = local.clone();
    hidden.hidden_collection_group_ids = ["project:billing".to_string()].into();
    let view = build(&hidden, &remote, &UnifiedFilter::default());
    assert!(view
        .options
        .repos
        .iter()
        .all(|option| option.label != "billing"));
    assert!(!ids(&view.active).contains(&"b1"));
}

#[test]
fn the_unified_lane_takes_the_hide_list_and_its_own_strip_and_nothing_else_of_the_project_list() {
    let mut inputs = SidebarInputs::default();
    inputs.ui.hidden_items.group_ids = vec!["project:x".to_string()];
    inputs.ui.show_hidden = true;
    inputs.ui.selected_tag_filters = vec!["blocked".to_string()];
    inputs.settings.session_card_hover_buttons = json!([{ "id": "rename", "enabled": true }]);
    let lane = unified_lane_inputs(&inputs, &studio());
    assert_eq!(lane.ui.hidden_items.group_ids, ["project:x"]);
    assert!(lane.ui.show_hidden);
    // The tag filter is the Project sidebar's.
    assert!(lane.ui.selected_tag_filters.is_empty());
    assert!(lane.ui.show_all_projects);
    assert_eq!(lane.ui.selected_machine_id, "studio");
    // No Unified strip saved: the default, not the Project sidebar's.
    assert_eq!(
        lane.settings.session_card_hover_buttons,
        unified_default_hover_buttons()
    );
    // Its own, once saved; and the Project sidebar's inputs never move.
    inputs.settings.unified_card_hover_buttons = json!([{ "id": "tag", "enabled": true }]);
    let lane = unified_lane_inputs(&inputs, &studio());
    assert_eq!(
        hover_strip(&lane.settings.session_card_hover_buttons).after,
        [HoverAction::Tag]
    );
    assert_eq!(
        inputs.settings.session_card_hover_buttons,
        json!([{ "id": "rename", "enabled": true }])
    );
}

fn choice_machines() -> Vec<UnifiedChoiceMachineInput> {
    let project = |id: &str, origin: &str, parent: Option<&str>| ProjectChoiceInput {
        project_id: id.to_string(),
        title: id.to_string(),
        path: format!("/{id}"),
        git_remote_origin_url: Some(origin.to_string()),
        worktree_parent_path: parent.map(str::to_string),
        has_sessions: id == "acme",
        last_activity_ms: (id == "acme").then_some(500),
    };
    vec![
        UnifiedChoiceMachineInput {
            machine: MachineId::Local,
            label: "Local".to_string(),
            connected: true,
            projects: vec![
                project("acme-wt", "https://github.com/o/acme", Some("/acme")),
                project("acme", "https://github.com/o/acme", None),
                project("zebra", "https://github.com/o/zebra", None),
            ],
        },
        UnifiedChoiceMachineInput {
            machine: studio(),
            label: "Studio".to_string(),
            connected: false,
            projects: vec![
                project("racme", "git@github.com:o/acme.git", None),
                project("rbill-wt", "git@github.com:o/bill.git", Some("/bill")),
            ],
        },
    ]
}

#[test]
fn a_repository_on_several_machines_is_one_row_with_a_chip_per_machine() {
    let choices = unified_project_choices(&choice_machines(), &BTreeMap::new(), &[], "local");
    let labels: Vec<&str> = choices.iter().map(|choice| choice.label.as_str()).collect();
    assert_eq!(labels, ["acme", "bill", "zebra"]);
    let acme = &choices[0];
    let chips: Vec<(&str, &str, bool)> = acme
        .machines
        .iter()
        .map(|chip| {
            (
                chip.machine_id.as_str(),
                chip.project_id.as_str(),
                chip.connected,
            )
        })
        .collect();
    // Each machine's main checkout, though the worktree was listed first; the unreachable machine
    // keeps its chip, dimmed by the renderer.
    assert_eq!(chips, [("local", "acme", true), ("studio", "racme", false)]);
    // A worktree stands in when a machine has nothing else of the repository.
    assert_eq!(choices[1].machines[0].project_id, "rbill-wt");
}

#[test]
fn a_row_starts_on_the_machine_last_used_then_the_current_then_this_computer() {
    let mut machines = choice_machines();
    machines[1].connected = true;
    let start_of =
        |machines: &[UnifiedChoiceMachineInput], last: &BTreeMap<String, String>, current: &str| {
            let choices = unified_project_choices(machines, last, &[], current);
            choices[0]
                .selected_machine()
                .map(|chip| chip.machine_id.clone())
                .unwrap()
        };
    let mut last = BTreeMap::new();
    assert_eq!(start_of(&machines, &last, "local"), "local");
    assert_eq!(start_of(&machines, &last, "studio"), "studio");
    assert_eq!(start_of(&machines, &last, "elsewhere"), "local");
    last.insert("github.com/o/acme".to_string(), "studio".to_string());
    assert_eq!(start_of(&machines, &last, "local"), "studio");
    // A last machine that no longer has it falls through.
    last.insert("github.com/o/acme".to_string(), "gone".to_string());
    assert_eq!(start_of(&machines, &last, "local"), "local");
    // A machine that isn't connected is skipped while another one has the project.
    machines[1].connected = false;
    last.insert("github.com/o/acme".to_string(), "studio".to_string());
    assert_eq!(start_of(&machines, &last, "studio"), "local");
    // Only one machine has it: that one, connected or not, wherever the draft is.
    let choices = unified_project_choices(&machines, &BTreeMap::new(), &[], "local");
    assert_eq!(choices[1].selected_machine().unwrap().machine_id, "studio");
}

#[test]
fn arrows_move_along_the_chips_and_stop_at_the_ends() {
    let mut machines = choice_machines();
    machines[1].connected = true;
    let choices = unified_project_choices(&machines, &BTreeMap::new(), &[], "local");
    let mut acme = choices[0].clone();
    assert_eq!(move_project_choice_machine(&acme, false), 0);
    assert_eq!(move_project_choice_machine(&acme, true), 1);
    acme.selected = 1;
    assert_eq!(move_project_choice_machine(&acme, true), 1);
    assert_eq!(move_project_choice_machine(&acme, false), 0);
    // A machine that isn't connected is stepped over.
    acme.machines[1].connected = false;
    acme.selected = 0;
    assert_eq!(move_project_choice_machine(&acme, true), 0);
}

#[test]
fn recent_projects_come_first_then_the_rest_a_to_z() {
    // Session activity first (acme), then the picker's own starts (zebra); a start that is no
    // longer listed is skipped.
    let recent = ["github.com/o/zebra".to_string(), "gone/repo".to_string()];
    let choices = unified_project_choices(&choice_machines(), &BTreeMap::new(), &recent, "local");
    let rows: Vec<(&str, bool, bool)> = choices
        .iter()
        .map(|choice| (choice.label.as_str(), choice.recent, choice.has_sessions))
        .collect();
    assert_eq!(
        rows,
        [
            ("acme", true, true),
            ("zebra", true, false),
            ("bill", false, false)
        ]
    );
}

#[test]
fn recent_holds_the_newest_active_projects_up_to_its_cap() {
    let machine = |projects: Vec<ProjectChoiceInput>| UnifiedChoiceMachineInput {
        machine: MachineId::Local,
        label: "Local".to_string(),
        connected: true,
        projects,
    };
    let project = |id: &str, at: Option<i64>| ProjectChoiceInput {
        project_id: id.to_string(),
        title: id.to_string(),
        path: format!("/{id}"),
        git_remote_origin_url: Some(format!("https://github.com/o/{id}")),
        worktree_parent_path: None,
        has_sessions: at.is_some(),
        last_activity_ms: at,
    };
    let projects = vec![
        project("a", Some(1)),
        project("b", Some(7)),
        project("c", Some(3)),
        project("d", Some(9)),
        project("e", Some(5)),
        project("f", Some(2)),
        project("g", None),
    ];
    let choices = unified_project_choices(&[machine(projects)], &BTreeMap::new(), &[], "local");
    let labels: Vec<(&str, bool)> = choices
        .iter()
        .map(|choice| (choice.label.as_str(), choice.recent))
        .collect();
    assert_eq!(
        labels,
        [
            ("d", true),
            ("b", true),
            ("e", true),
            ("c", true),
            ("f", true),
            ("a", false),
            ("g", false)
        ]
    );
}

#[test]
fn a_started_session_moves_its_repository_to_the_top_of_recent_and_remembers_the_machine() {
    let mut saved = UnifiedSavedState::default();
    for repo in ["a", "b", "c", "d", "e", "f"] {
        saved.remember_new_session(repo, "local");
    }
    saved.remember_new_session("c", "studio");
    assert_eq!(saved.recent_repos, ["c", "f", "e", "d", "b"]);
    assert_eq!(saved.last_machines["c"], "studio");
}

#[test]
fn a_new_session_outside_the_filter_joins_only_the_kinds_that_are_picked() {
    let filter = UnifiedFilter {
        repos: ["r1".to_string()].into(),
        query: "typed".to_string(),
        ..UnifiedFilter::default()
    };
    let widened = widen_filter_for_new_session(&filter, "r2", "studio", Some("studio|s"));
    assert_eq!(widened.repos, ["r1".to_string(), "r2".to_string()].into());
    assert!(widened.machines.is_empty());
    assert!(widened.spaces.is_empty());
    assert_eq!(widened.query, "typed");
    let nothing = widen_filter_for_new_session(&UnifiedFilter::default(), "r2", "local", None);
    assert!(nothing.keeps_everything());
}

#[test]
fn the_project_search_matches_names_and_machines() {
    let choices = unified_project_choices(&choice_machines(), &BTreeMap::new(), &[], "local");
    let found = |query: &str| -> Vec<String> {
        filter_project_choices(&choices, query)
            .into_iter()
            .map(|choice| choice.label)
            .collect()
    };
    assert_eq!(found("ZEB"), ["zebra"]);
    assert_eq!(found("studio"), ["acme", "bill"]);
    assert_eq!(found("acme stu"), ["acme"]);
    assert_eq!(found("").len(), 3);
}

#[test]
fn new_session_aims_at_the_filtered_repository_without_activating_anything() {
    let choices = unified_project_choices(&choice_machines(), &BTreeMap::new(), &[], "local");
    let filter = |repo: &[&str]| UnifiedFilter {
        repos: repo.iter().map(|key| key.to_string()).collect(),
        ..UnifiedFilter::default()
    };
    let at = |repo: &str, machine: &str, project: &str| {
        Some(UnifiedNewSessionTarget {
            repo_key: repo.to_string(),
            machine_id: machine.to_string(),
            project_id: project.to_string(),
        })
    };
    let active = |machine_id, project_id| {
        let repo_key = match project_id {
            "zebra" => "github.com/o/zebra",
            _ => "github.com/o/acme",
        };
        Some(ActiveProject {
            repo_key,
            machine_id,
            project_id,
        })
    };
    // Nothing or several filtered: the open project, on its machine.
    assert_eq!(
        unified_new_session_target(&filter(&[]), &choices, active("local", "zebra")),
        at("github.com/o/zebra", "local", "zebra")
    );
    assert_eq!(
        unified_new_session_target(
            &filter(&["github.com/o/acme", "github.com/o/zebra"]),
            &choices,
            active("studio", "racme")
        ),
        at("github.com/o/acme", "studio", "racme")
    );
    // Already in that repository, on any machine: stay there.
    assert_eq!(
        unified_new_session_target(
            &filter(&["github.com/o/acme"]),
            &choices,
            active("studio", "racme")
        ),
        at("github.com/o/acme", "studio", "racme")
    );
    // Elsewhere: the row's own start.
    assert_eq!(
        unified_new_session_target(
            &filter(&["github.com/o/acme"]),
            &choices,
            active("local", "zebra")
        ),
        at("github.com/o/acme", "local", "acme")
    );
    // A repository only another machine has.
    assert_eq!(
        unified_new_session_target(&filter(&["github.com/o/bill"]), &choices, None),
        at("github.com/o/bill", "studio", "rbill-wt")
    );
    // A worktree you have open is "here", though the row points at the main checkout.
    assert_eq!(
        unified_new_session_target(&filter(&[]), &choices, active("local", "acme-wt")),
        at("github.com/o/acme", "local", "acme-wt")
    );
    // Nothing open and nothing filtered: the list's first project.
    assert_eq!(
        unified_new_session_target(&filter(&[]), &choices, None),
        at("github.com/o/acme", "local", "acme")
    );
}

#[test]
fn the_filter_shelves_folds_and_chips_survive_a_restart_but_the_search_does_not() {
    let mut saved = UnifiedSavedState::default();
    saved.set_filter(&UnifiedFilter {
        repos: ["github.com/o/acme".to_string()].into(),
        machines: ["studio".to_string()].into(),
        spaces: ["local|box".to_string()].into(),
        query: "typed".to_string(),
    });
    saved.toggled_shelves = [
        "Working".to_string(),
        "github.com/o/acme|Parked".to_string(),
    ]
    .into();
    saved.folded_groups = ["github.com/o/zebra".to_string()].into();
    saved.strip_expanded = true;
    saved
        .last_machines
        .insert("github.com/o/acme".to_string(), "studio".to_string());
    let restored = UnifiedSavedState::parse(Some(&saved.to_json()));
    assert_eq!(restored, saved);
    let filter = restored.filter();
    assert_eq!(filter.machines, saved.machines);
    assert_eq!(filter.spaces, saved.spaces);
    assert_eq!(filter.query, "");
}

#[test]
fn a_restored_pick_of_something_gone_narrows_nothing_but_stays_saved() {
    let (local, remote) = two_machines();
    let view = build(&local, &remote, &UnifiedFilter::default());
    let saved = UnifiedFilter {
        repos: [
            "github.com/o/gone".to_string(),
            "github.com/o/acme".to_string(),
        ]
        .into(),
        machines: ["away".to_string()].into(),
        ..UnifiedFilter::default()
    };
    let effective = saved.within(&view.options);
    assert_eq!(effective.repos, ["github.com/o/acme".to_string()].into());
    assert!(effective.machines.is_empty());
    assert_eq!(saved.repos.len(), 2);
}

#[test]
fn nothing_saved_or_something_unreadable_starts_over() {
    assert_eq!(UnifiedSavedState::parse(None), UnifiedSavedState::default());
    assert_eq!(
        UnifiedSavedState::parse(Some("not json")),
        UnifiedSavedState::default()
    );
    // A newer build's extra field does not lose the rest.
    let restored = UnifiedSavedState::parse(Some(r#"{"stripExpanded":true,"future":1}"#));
    assert!(restored.strip_expanded);
}
