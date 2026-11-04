use std::collections::BTreeMap;
use std::sync::Arc;

use crate::keys::{MachineId, SessionKey};
use crate::sidebar_view::{
    GroupCore, GroupSummary, GroupView, ProjectContextView, SessionRow, SessionTiming, SessionView,
    SidebarView, Space, SpacesState, WorktreeView,
};

use crate::sidebar_view::view::SpaceProject;

use super::*;

fn row(machine: &MachineId, project: &str, id: &str) -> SessionRow {
    SessionRow {
        sidebar_session_id: match machine.remote_id() {
            None => id.to_string(),
            Some(remote) => format!("{remote}:{project}:{id}"),
        },
        key: Some(SessionKey {
            machine: machine.clone(),
            project_id: project.to_string(),
            session_id: id.to_string(),
        }),
        activity: "idle".to_string(),
        ..SessionRow::default()
    }
}

fn timed(mut row: SessionRow, created: i64, last: i64) -> SessionRow {
    row.timing = SessionTiming {
        created_ms: Some(created),
        last_interaction_ms: Some(last),
        ..SessionTiming::default()
    };
    row
}

fn view_of(row: SessionRow) -> SessionView {
    SessionView {
        row: Arc::new(row),
        is_focused: false,
        is_visible: false,
        is_multi_selected: false,
        nesting: Default::default(),
    }
}

fn group(project: &str, path: &str, origin: Option<&str>, sessions: Vec<SessionRow>) -> GroupView {
    GroupView {
        core: Arc::new(GroupCore {
            group_id: format!("project:{project}"),
            storage_id: project.to_string(),
            title: project.to_string(),
            title_tooltip: None,
            is_active: false,
            project_context: Some(ProjectContextView {
                project_id: project.to_string(),
                path: path.to_string(),
                git_remote_origin_url: origin.map(str::to_string),
                ..ProjectContextView::default()
            }),
            summary: GroupSummary::default(),
            collapsed: false,
            hover_actions_expanded: false,
            sections: Vec::new(),
            sessions: sessions.into_iter().map(view_of).collect(),
            remote_machine: None,
            is_stale: false,
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

fn settings() -> UnifiedSettings {
    UnifiedSettings {
        working_section: true,
        enable_session_parking: true,
    }
}

fn ids(rows: &[UnifiedRow]) -> Vec<String> {
    rows.iter()
        .map(|row| row.session.row.sidebar_session_id.clone())
        .collect()
}

const NOW: u64 = 1_000_000;

#[test]
fn repo_identity_reads_every_origin_form_as_one_repository() {
    let forms = [
        "git@github.com:Owner/Ghostex.git",
        "https://github.com/owner/ghostex",
        "https://user@GitHub.com/owner/ghostex.git/",
        "ssh://git@github.com:22/owner/ghostex.git",
    ];
    for form in forms {
        let identity = repo_identity(Some(form), "/Users/a/dev/whatever", None, "whatever");
        assert_eq!(identity.key, "github.com/owner/ghostex", "{form}");
    }
    assert_eq!(
        repo_identity(Some("git@github.com:Owner/Ghostex.git"), "/x", None, "x").label,
        "Ghostex"
    );
}

#[test]
fn repo_identity_falls_back_to_the_folder_and_a_worktree_to_its_parent() {
    let plain = repo_identity(None, "/Users/a/dev/hello_3d/", None, "Hello");
    assert_eq!(plain.key, "dir:hello_3d");
    assert_eq!(plain.label, "hello_3d");
    let worktree = repo_identity(
        None,
        "/Users/a/dev/hello_3d-feature",
        Some("/Users/a/dev/hello_3d"),
        "x",
    );
    assert_eq!(worktree.key, plain.key);
    let windows = repo_identity(None, "C:\\code\\Tool", None, "x");
    assert_eq!(windows.label, "Tool");
    assert_eq!(repo_identity(None, "", None, " Notes ").label, "Notes");
}

#[test]
fn a_filter_row_names_every_project_its_remove_takes_out() {
    let windows = MachineId::Remote("windows".to_string());
    let local = SidebarView {
        groups: vec![
            group(
                "P1",
                "/a/ghostex",
                Some("git@github.com:o/Ghostex.git"),
                vec![],
            ),
            group(
                "P2",
                "/a/other",
                Some("https://github.com/o/other.git"),
                vec![],
            ),
        ],
        ..SidebarView::default()
    };
    let remote = SidebarView {
        groups: vec![group(
            "P9",
            "/w/ghostex",
            Some("https://github.com/o/Ghostex"),
            vec![],
        )],
        ..SidebarView::default()
    };
    let inputs = [
        UnifiedMachineInput {
            machine: MachineId::Local,
            label: "Local".to_string(),
            view: &local,
            spaces: None,
        },
        UnifiedMachineInput {
            machine: windows.clone(),
            label: "Windows".to_string(),
            view: &remote,
            spaces: None,
        },
    ];
    let view = build_unified_sidebar(
        &inputs,
        UnifiedSettings::default(),
        &UnifiedFilter::default(),
        0,
    );
    let ghostex = view
        .options
        .repos
        .iter()
        .find(|option| option.label == "Ghostex")
        .unwrap();
    let group_of = |machine: MachineId, project: &str| {
        crate::keys::ProjectKey {
            machine,
            project_id: project.to_string(),
        }
        .to_sidebar_group_id()
    };
    let mut expected = vec![group_of(MachineId::Local, "P1"), group_of(windows, "P9")];
    expected.sort();
    assert_eq!(ghostex.project_group_ids, expected);
    // Machines and Spaces name none, so they get no Remove.
    assert!(view
        .options
        .machines
        .iter()
        .all(|option| option.project_group_ids.is_empty()));
}

#[test]
fn the_same_repository_on_two_machines_is_one_filter_choice() {
    let remote = MachineId::Remote("remote-1".to_string());
    let local_view = sidebar(vec![group(
        "p1",
        "/Users/me/dev/Ghostex",
        Some("git@github.com:o/Ghostex.git"),
        vec![row(&MachineId::Local, "p1", "a")],
    )]);
    let remote_view = sidebar(vec![group(
        "p9",
        "/home/me/src/gx",
        Some("https://github.com/o/ghostex"),
        vec![row(&remote, "p9", "b")],
    )]);
    let machines = [
        UnifiedMachineInput {
            machine: MachineId::Local,
            label: "Local".to_string(),
            view: &local_view,
            spaces: None,
        },
        UnifiedMachineInput {
            machine: remote.clone(),
            label: "hp-laptop".to_string(),
            view: &remote_view,
            spaces: None,
        },
    ];
    let view = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert_eq!(view.options.repos.len(), 1);
    assert_eq!(view.options.repos[0].label, "Ghostex");
    assert_eq!(
        view.options.repos[0].detail.as_deref(),
        Some("Local, hp-laptop")
    );
    assert_eq!(view.active.len(), 2);
    assert!(view.active.iter().all(|row| row.repo.label == "Ghostex"));
    let remote_row = view
        .active
        .iter()
        .find(|row| row.machine == remote)
        .unwrap();
    assert_eq!(remote_row.machine_label.as_deref(), Some("hp-laptop"));
    let local_row = view
        .active
        .iter()
        .find(|row| row.machine.is_local())
        .unwrap();
    assert_eq!(local_row.machine_label, None);
}

#[test]
fn rows_are_ordered_by_when_they_last_came_back_not_by_output_while_working() {
    let local = MachineId::Local;
    // Started long ago and still printing: its meaningful clock is the newest, its stint is old.
    let mut busy = timed(row(&local, "p", "busy"), 100, 9_000);
    busy.activity = "working".to_string();
    busy.timing.working_started_ms = Some(1_000);
    // A turn that finished at 5_000.
    let mut done = timed(row(&local, "p", "done"), 200, 5_000);
    done.activity = "attention".to_string();
    // Created last, never prompted.
    let fresh = timed(row(&local, "p", "fresh"), 7_000, 0);
    let old = timed(row(&local, "p", "old"), 50, 300);
    let view = sidebar(vec![group("p", "/p", None, vec![busy, done, fresh, old])]);
    let machines = [UnifiedMachineInput {
        machine: local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let unfolded = UnifiedSettings {
        working_section: false,
        ..settings()
    };
    let built = build_unified_sidebar(&machines, unfolded, &UnifiedFilter::default(), NOW);
    assert_eq!(ids(&built.active), ["fresh", "done", "busy", "old"]);
}

#[test]
fn shelves_follow_pin_snooze_park_and_working_section() {
    let local = MachineId::Local;
    let mut pinned = row(&local, "p", "pinned");
    pinned.is_pinned = true;
    let mut working = row(&local, "p", "working");
    working.activity = "working".to_string();
    let mut asking = row(&local, "p", "asking");
    asking.activity = "working".to_string();
    asking.pending_question_count = 1;
    let mut parked = row(&local, "p", "parked");
    parked.is_parked = true;
    let mut snoozed = row(&local, "p", "snoozed");
    snoozed.timing.snoozed_until_ms = Some(NOW as i64 + 60_000);
    let mut woke = row(&local, "p", "woke");
    woke.timing.snoozed_until_ms = Some(NOW as i64 - 1);
    let mut focused_working = view_of(row(&local, "p", "open"));
    Arc::make_mut(&mut focused_working.row).activity = "working".to_string();
    focused_working.is_focused = true;
    let mut project = group(
        "p",
        "/p",
        None,
        vec![pinned, working, asking, parked, snoozed, woke],
    );
    Arc::make_mut(&mut project.core)
        .sessions
        .push(focused_working);
    let view = sidebar(vec![project]);
    let machines = [UnifiedMachineInput {
        machine: local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert_eq!(ids(&built.pinned), ["pinned"]);
    // The open session folds too: opening a card never moves it.
    let mut working = ids(&built.working);
    working.sort();
    assert_eq!(working, ["open", "working"]);
    assert_eq!(ids(&built.snoozed), ["snoozed"]);
    assert_eq!(ids(&built.parked), ["parked"]);
    let mut active = ids(&built.active);
    active.sort();
    assert_eq!(active, ["asking", "woke"]);

    // Group working sessions off: nothing folds. Parking off: parked rows are ordinary rows.
    let plain = UnifiedSettings::default();
    let built = build_unified_sidebar(&machines, plain, &UnifiedFilter::default(), NOW);
    assert!(built.working.is_empty());
    assert!(built.parked.is_empty());
    assert_eq!(built.active.len(), 5);
}

#[test]
fn the_working_shelf_lists_the_last_started_first_and_parked_the_latest_active_first() {
    let local = MachineId::Local;
    let working = |id: &str, started: i64| {
        let mut row = row(&local, "p", id);
        row.activity = "working".to_string();
        row.timing.working_started_ms = Some(started);
        row
    };
    let parked = |id: &str, last: i64| {
        let mut row = timed(row(&local, "p", id), 0, last);
        row.is_parked = true;
        row
    };
    let view = sidebar(vec![group(
        "p",
        "/p",
        None,
        vec![
            working("w1", 10),
            working("w2", 30),
            working("w3", 20),
            parked("k1", 5),
            parked("k2", 50),
        ],
    )]);
    let machines = [UnifiedMachineInput {
        machine: local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert_eq!(ids(&built.working), ["w2", "w3", "w1"]);
    assert_eq!(ids(&built.parked), ["k2", "k1"]);
}

#[test]
fn the_filter_narrows_by_repository_machine_and_space_together() {
    let remote = MachineId::Remote("r".to_string());
    let local_view = sidebar(vec![
        group(
            "web",
            "/dev/web",
            Some("git@github.com:o/web.git"),
            vec![row(&MachineId::Local, "web", "w")],
        ),
        group(
            "api",
            "/dev/api",
            Some("git@github.com:o/api.git"),
            vec![row(&MachineId::Local, "api", "a")],
        ),
    ]);
    let remote_view = sidebar(vec![group(
        "web2",
        "/srv/web",
        Some("git@github.com:o/web.git"),
        vec![row(&remote, "web2", "rw")],
    )]);
    let mut spaces = BTreeMap::new();
    spaces.insert(
        "work".to_string(),
        Space {
            space_id: "work".to_string(),
            name: "Work".to_string(),
            member_project_ids: vec!["api".to_string()],
            ..Space::default()
        },
    );
    let local_spaces = SpacesState {
        order: vec!["work".to_string()],
        spaces,
    };
    let machines = [
        UnifiedMachineInput {
            machine: MachineId::Local,
            label: "Local".to_string(),
            view: &local_view,
            spaces: Some(local_spaces),
        },
        UnifiedMachineInput {
            machine: remote.clone(),
            label: "hp".to_string(),
            view: &remote_view,
            spaces: None,
        },
    ];
    let all = |filter: &UnifiedFilter| {
        let mut rows = ids(&build_unified_sidebar(&machines, settings(), filter, NOW).active);
        rows.sort();
        rows
    };
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert_eq!(built.total, 3);
    assert_eq!(
        built
            .options
            .spaces
            .iter()
            .map(|option| option.key.as_str())
            .collect::<Vec<_>>(),
        ["local|work", "local|other"]
    );

    let mut by_repo = UnifiedFilter::default();
    by_repo.repos.insert("github.com/o/web".to_string());
    assert_eq!(all(&by_repo), ["r:web2:rw", "w"]);

    let mut repo_and_machine = by_repo.clone();
    repo_and_machine.machines.insert("local".to_string());
    assert_eq!(all(&repo_and_machine), ["w"]);

    let mut by_space = UnifiedFilter::default();
    by_space
        .spaces
        .insert(space_filter_key(&MachineId::Local, "work"));
    assert_eq!(all(&by_space), ["a"]);

    let mut other = UnifiedFilter::default();
    other
        .spaces
        .insert(space_filter_key(&MachineId::Local, "other"));
    assert_eq!(all(&other), ["w"]);

    let filtered = build_unified_sidebar(&machines, settings(), &by_space, NOW);
    assert_eq!(filtered.total, 3, "the total counts rows before the filter");
}

#[test]
fn a_worktree_project_is_its_parents_repository() {
    let mut worktree = group(
        "wt",
        "/dev/app-feature",
        None,
        vec![row(&MachineId::Local, "wt", "x")],
    );
    if let Some(context) = Arc::make_mut(&mut worktree.core).project_context.as_mut() {
        context.worktree = Some(WorktreeView {
            branch: "feature".to_string(),
            name: "app-feature".to_string(),
            parent_project_id: "app".to_string(),
            parent_project_name: "app".to_string(),
            parent_project_path: "/dev/app".to_string(),
        });
    }
    let parent = group(
        "app",
        "/dev/app",
        None,
        vec![row(&MachineId::Local, "app", "y")],
    );
    let view = sidebar(vec![parent, worktree]);
    let machines = [UnifiedMachineInput {
        machine: MachineId::Local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert_eq!(built.options.repos.len(), 1);
    assert!(built.active.iter().all(|row| row.repo.label == "app"));
}

#[test]
fn status_reads_the_most_urgent_first() {
    let mut row = row(&MachineId::Local, "p", "s");
    assert_eq!(unified_status(&row), None);
    row.has_background_work = true;
    assert_eq!(unified_status(&row), Some(UnifiedStatus::Waiting));
    row.activity = "attention".to_string();
    assert_eq!(unified_status(&row), Some(UnifiedStatus::Done));
    row.activity = "working".to_string();
    assert_eq!(unified_status(&row), Some(UnifiedStatus::Working));
    row.pending_question_count = 2;
    assert_eq!(unified_status(&row), Some(UnifiedStatus::Question));
    row.awaiting_approval = true;
    assert_eq!(unified_status(&row), Some(UnifiedStatus::Approval));
}

#[test]
fn browser_rows_stay_out_of_the_unified_list() {
    let mut tab = row(&MachineId::Local, "p", "tab");
    tab.is_browser = true;
    let view = sidebar(vec![group("p", "/p", None, vec![tab])]);
    let machines = [UnifiedMachineInput {
        machine: MachineId::Local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert!(built.is_empty());
    assert_eq!(built.total, 0);
}

#[test]
fn two_repositories_with_one_name_say_which_is_which() {
    let view = sidebar(vec![
        group(
            "a",
            "/dev/Ghostex",
            Some("https://github.com/maddada/Ghostex.git"),
            vec![row(&MachineId::Local, "a", "x")],
        ),
        group(
            "b",
            "/dev/ghostex",
            Some("git@github.com:other/ghostex.git"),
            vec![row(&MachineId::Local, "b", "y")],
        ),
        group(
            "c",
            "/tmp/ghostex",
            None,
            vec![row(&MachineId::Local, "c", "z")],
        ),
        group(
            "d",
            "/dev/unique",
            Some("git@github.com:o/unique.git"),
            vec![row(&MachineId::Local, "d", "w")],
        ),
    ]);
    let machines = [UnifiedMachineInput {
        machine: MachineId::Local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    let mut details: Vec<(String, Option<String>)> = built
        .options
        .repos
        .iter()
        .map(|option| (option.label.clone(), option.detail.clone()))
        .collect();
    details.sort();
    assert_eq!(
        details,
        [
            ("Ghostex".to_string(), Some("maddada/ghostex".to_string())),
            ("ghostex".to_string(), Some("no remote".to_string())),
            ("ghostex".to_string(), Some("other/ghostex".to_string())),
            ("unique".to_string(), None),
        ]
    );
}

#[test]
fn drafts_are_never_sent_sessions_newest_first_and_leave_once_sent() {
    let local = MachineId::Local;
    let draft = |id: &str, created: i64, text: bool| {
        let mut row = timed(row(&local, "p", id), created, created);
        row.is_draft = true;
        row.has_composer_draft = text;
        row
    };
    let older = draft("older", 100, true);
    // Never sent and nothing typed: still a draft.
    let empty = draft("empty", 300, false);
    // Sent once: not a draft, whatever its chat box holds.
    let mut sent = timed(row(&local, "p", "sent"), 400, 400);
    sent.has_composer_draft = true;
    let mut pinned = draft("pinned", 500, true);
    pinned.is_pinned = true;
    let view = sidebar(vec![group(
        "p",
        "/p",
        None,
        vec![older, empty, sent, pinned],
    )]);
    let machines = [UnifiedMachineInput {
        machine: local.clone(),
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    assert_eq!(ids(&built.drafts), ["empty", "older"]);
    assert_eq!(ids(&built.pinned), ["pinned"]);
    assert_eq!(ids(&built.active), ["sent"]);
}

#[test]
fn a_cards_menu_comes_from_the_machine_list_that_holds_it() {
    use crate::{Core, MenuHost, SidebarInputs, SidebarMenus};
    let local = MachineId::Local;
    // The Project sidebar's list shows another Space, without this session.
    let selected_space = sidebar(vec![group("other", "/other", None, vec![])]);
    // The Unified sidebar's machine list holds every session.
    let every_space = sidebar(vec![group("p", "/p", None, vec![row(&local, "p", "far")])]);
    let core = Core::new();
    let inputs = SidebarInputs::default();
    let host = MenuHost::default();
    let classic = SidebarMenus::new(&core, &selected_space, &inputs, &host, NOW);
    assert!(classic.row_menu("far").is_none());
    let unified = SidebarMenus::new(&core, &every_space, &inputs, &host, NOW);
    let items = unified.row_menu("far").expect("the card's own menu");
    assert!(!items.is_empty());
}

#[test]
fn a_quick_access_project_pick_finds_its_repository() {
    let local = MachineId::Local;
    let view = sidebar(vec![group(
        "p1",
        "/Users/me/dev/Ghostex",
        Some("git@github.com:o/Ghostex.git"),
        vec![row(&local, "p1", "a")],
    )]);
    assert_eq!(
        repo_of_project(&[&view], "p1").map(|repo| repo.key),
        Some("github.com/o/ghostex".to_string())
    );
    assert!(repo_of_project(&[&view], "missing").is_none());
}

#[test]
fn the_working_section_holds_working_and_background_sessions_until_they_need_you() {
    let local = MachineId::Local;
    let mut working = row(&local, "p", "working");
    working.activity = "working".to_string();
    // Its turn ended, but a shell it started still runs.
    let mut monitoring = row(&local, "p", "monitoring");
    monitoring.has_background_work = true;
    // Finished while a background shell runs: it needs you.
    let mut finished = row(&local, "p", "finished");
    finished.activity = "attention".to_string();
    finished.has_background_work = true;
    let mut approval = row(&local, "p", "approval");
    approval.activity = "working".to_string();
    approval.awaiting_approval = true;
    let view = sidebar(vec![group(
        "p",
        "/p",
        None,
        vec![working, monitoring, finished, approval],
    )]);
    let machines = [UnifiedMachineInput {
        machine: local,
        label: "Local".to_string(),
        view: &view,
        spaces: None,
    }];
    let built = build_unified_sidebar(&machines, settings(), &UnifiedFilter::default(), NOW);
    let mut shelf = ids(&built.working);
    shelf.sort();
    assert_eq!(shelf, ["monitoring", "working"]);
    let mut list = ids(&built.active);
    list.sort();
    assert_eq!(list, ["approval", "finished"]);

    // Switched off: nothing folds.
    let off = UnifiedSettings {
        working_section: false,
        ..settings()
    };
    let built = build_unified_sidebar(&machines, off, &UnifiedFilter::default(), NOW);
    assert!(built.working.is_empty());
    assert_eq!(built.active.len(), 4);
}

#[test]
fn a_cards_snooze_button_gets_the_presets_without_a_snooze_hover_button() {
    use crate::{Core, HoverAction, MenuHost, SidebarInputs, SidebarMenus};
    let local = MachineId::Local;
    let view = sidebar(vec![group("p", "/p", None, vec![row(&local, "p", "s")])]);
    let core = Core::new();
    let mut inputs = SidebarInputs::default();
    // The user's hover strip has no Snooze button.
    inputs.settings.session_card_hover_buttons = serde_json::json!([]);
    let host = MenuHost::default();
    let menus = SidebarMenus::new(&core, &view, &inputs, &host, NOW);
    assert_eq!(
        menus
            .row_hover_submenu("s", HoverAction::Snooze)
            .map(|items| items.len()),
        Some(0)
    );
    let presets = menus
        .row_action_submenu("s", HoverAction::Snooze)
        .expect("the row is drawn");
    let labels: Vec<_> = presets
        .iter()
        .filter_map(|item| item.label.clone())
        .collect();
    assert_eq!(labels, ["1 Hour", "3 Hours", "Tomorrow", "Next Week"]);
}
