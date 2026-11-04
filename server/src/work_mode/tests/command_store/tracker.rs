//! A ticket command as its workspace's tracker, as a project row shows it to clients that know
//! only Linear and GitHub. Each test pins ids of its own.

use crate::domain::DomainRepository;

use super::cards::{pin, project_id, project_in};
use super::*;

/// The project's tracker, remembered as the card pass does (`refresh.rs`).
fn remember_tracker(store: &Store, project: &Value) -> WorkSourceId {
    let workspaces = crate::workspaces::read_sidebar_workspaces(&store.db).expect("workspaces");
    let projects = DomainRepository::new(&store.db, "test")
        .list_projects()
        .expect("projects");
    let tracker = project_work_tracker(&store.paths, &workspaces, project, &projects);
    remember_project_work_tracker(project_id(project), tracker);
    tracker
}

/// With the command as the tracker, a row has no `workTracker` and no `workLinear`, even with a
/// Linear key set, and its `workSource` says it is the tracker.
#[test]
fn a_command_tracker_hides_legacy_tracker_fields() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "row-tracker", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    remember_project_linear_key(project_id(&project), Some(1));
    remember_tracker(&store, &project);
    let row = crate::presentation::project_presentation_project(&project);
    assert!(row.get("workTracker").is_some(), "{row}");
    assert_eq!(row["workLinear"], true);
    assert_eq!(row["workSource"]["isTracker"], false);

    pick_command_tracker(&store.db, &store.paths, &workspace_id, "row-tracker").expect("picked");
    assert_eq!(remember_tracker(&store, &project).as_wire(), "row-tracker");
    let row = crate::presentation::project_presentation_project(&project);
    assert_eq!(row["workMode"], true);
    assert!(row.get("workTracker").is_none(), "{row}");
    assert!(row.get("workLinear").is_none(), "{row}");
    assert_eq!(row["workSource"]["isTracker"], true);
    remember_project_linear_key(project_id(&project), None);
}

/// The record and another write saved as one refuse to run inside a caller's transaction, whose
/// rollback would leave the copy in memory ahead of the database; nothing is written.
#[test]
fn a_record_and_tracker_save_refuses_an_outer_transaction() {
    let store = Store::new();
    pin(&store, "ws-a", "row-outer", "good");
    let sources = read_work_sources(&store.db).expect("record");
    store.db.execute_batch("BEGIN").expect("begun");
    let refused = save_work_sources_with(&store.db, &WorkSources::default(), |_| Ok(()));
    store.db.execute_batch("ROLLBACK").expect("rolled back");
    assert_eq!(
        refused.expect_err("refused").message,
        "Ghostex refused a ticket command change made inside another change, and changed nothing."
    );
    assert_eq!(read_work_sources(&store.db).expect("record"), sources);
    assert!(workspace_command(&store.ws("ws-a")).is_some());
}

/// Sets the workspace's stored tracker, as `/api/setWorkTracker` does for Linear or GitHub.
fn store_tracker(store: &Store, workspace_id: &str, tracker: &str) {
    let current = crate::workspaces::read_sidebar_workspaces(&store.db).expect("workspaces");
    let next = crate::workspaces::update_workspace_in(
        &current,
        workspace_id,
        json!({ "tracker": tracker }).as_object().unwrap(),
    )
    .expect("patched");
    crate::workspaces::write_sidebar_workspaces(&store.db, &next).expect("written");
}

/// What a session on branch `218-fix` links, of GitHub and of the number-format command `id`.
fn bare_number_links(project: &Value, session: &Value, id: &str) -> (Vec<String>, Vec<String>) {
    let targets = work_targets(project, session);
    (
        targets.ids_of(WorkSourceId::Github),
        targets.ids_of(super::published::command_source(id)),
    )
}

/// A branch's bare number is the tracker's ticket when the tracker uses numbers, otherwise
/// GitHub's issue; a number-format command that isn't the tracker gets one only by hand.
#[test]
fn the_tracker_decides_what_a_bare_number_names() {
    let store = Store::new();
    let sources = super::published::record_pinning(&store, "ws-a", "row-numbers", IdFormat::Number);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws("ws-a");
    let project = project_in(&store, &workspace_id, json!({}));
    set_branch(project["path"].as_str().unwrap(), "218-fix");
    let on_branch = session(project_id(&project), "S1", json!({}));
    let numbers = (vec!["218".to_string()], Vec::new());
    let the_commands = (Vec::new(), vec!["218".to_string()]);

    // GitHub, the tracker of a workspace that never picked one and has no Linear key.
    assert_eq!(remember_tracker(&store, &project), WorkSourceId::Github);
    assert_eq!(
        bare_number_links(&project, &on_branch, "row-numbers"),
        numbers
    );

    pick_command_tracker(&store.db, &store.paths, &workspace_id, "row-numbers").expect("picked");
    assert_eq!(remember_tracker(&store, &project).as_wire(), "row-numbers");
    assert_eq!(
        bare_number_links(&project, &on_branch, "row-numbers"),
        the_commands
    );

    // Linear reads no bare numbers, so they stay GitHub's.
    store_tracker(&store, &workspace_id, "linear");
    assert_eq!(remember_tracker(&store, &project), WorkSourceId::Linear);
    assert_eq!(
        bare_number_links(&project, &on_branch, "row-numbers"),
        numbers
    );

    // Set by hand, the command's ticket links whatever the tracker.
    let by_hand = session(
        project_id(&project),
        "S2",
        json!({ "tickets": { "row-numbers": ["218"] } }),
    );
    assert_eq!(
        work_targets(&project, &by_hand).ids_of(super::published::command_source("row-numbers")),
        ["218"]
    );

    // A cleared command whose ticket kept its title reads no bare number either: it isn't the
    // tracker.
    save_ticket_titles(
        &store.db,
        &workspace_id,
        "row-numbers",
        &[("218".to_string(), "Fix it".to_string())],
    )
    .expect("titled");
    store_tracker(&store, &workspace_id, "github");
    clear_work_source(&store.db, &workspace_id, &no_references).expect("cleared");
    remember_tracker(&store, &project);
    let linked: Vec<String> = work_targets(&project, &on_branch)
        .tickets
        .iter()
        .map(|linked| format!("{}:{}", linked.ticket.source.as_wire(), linked.ticket.id))
        .collect();
    assert_eq!(linked, ["github:218"]);
}

/// The other places a branch's bare number is read follow the same rule: the tickets a PR's head
/// branch names on its details page, and the worktree Start reuses for a GitHub issue.
#[test]
fn a_prs_branch_and_starts_worktree_follow_the_trackers_bare_numbers() {
    let store = Store::new();
    let sources = super::published::record_pinning(&store, "ws-a", "pr-numbers", IdFormat::Number);
    save_work_sources(&store.db, &sources).expect("pinned");
    let command = super::published::command_source("pr-numbers");
    let known_keys = [
        (WorkSourceId::Linear, Vec::new()),
        (WorkSourceId::Github, Vec::new()),
        (command, Vec::new()),
    ];
    let named = |tracker| {
        pull_request_branch_tickets(&json!({ "headBranch": "218-fix" }), &known_keys, tracker)
            .into_iter()
            .map(|ticket| match ticket {
                WorkItemRef::Ticket(ticket) => format!("{}:{}", ticket.source.as_wire(), ticket.id),
                WorkItemRef::PullRequest(_) => "a PR".to_string(),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(named(WorkSourceId::Github), ["github:218"]);
    assert_eq!(named(WorkSourceId::Linear), ["github:218"]);
    assert_eq!(named(command), ["pr-numbers:218"]);

    let issue = WorkTicket::from_params(
        json!({ "source": "github", "ticketId": 218 })
            .as_object()
            .unwrap(),
    )
    .expect("an issue");
    let unlinked = WorkTargets::default();
    assert!(issue.is_linked(&unlinked, "218-fix", WorkSourceId::Github));
    assert!(issue.is_linked(&unlinked, "218-fix", WorkSourceId::Linear));
    assert!(!issue.is_linked(&unlinked, "218-fix", command));
}

/// Start reads the tracker from the workspace state, not the card pass's copy: right after a
/// number-format command becomes the tracker, before a pass has run, Start for GitHub issue 218
/// doesn't reuse a `218-fix` worktree, whose number is now the command's ticket.
#[test]
fn start_reads_a_tracker_picked_since_the_last_pass() {
    let store = Store::new();
    let sources =
        super::published::record_pinning(&store, "ws-a", "start-numbers", IdFormat::Number);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws("ws-a");
    let project = project_in(&store, &workspace_id, json!({}));
    let worktree = store.paths.app_data_dir.join("worktree-218");
    std::fs::create_dir_all(&worktree).expect("worktree");
    // Its git status is cached too, as the background poll leaves it.
    set_branch(worktree.to_str().unwrap(), "218-fix");
    let repository = DomainRepository::new(&store.db, "test");
    repository
        .create_session(
            json!({
                "projectId": project_id(&project),
                "kind": "agent",
                "agentId": "codex",
                "title": "S",
                "lifecycleState": "running",
                "cwd": worktree,
                "runtimeSettings": { "worktreeSession": { "branch": "218-fix", "path": worktree } },
            })
            .as_object()
            .unwrap(),
            false,
        )
        .expect("session");
    let issue = WorkTicket::from_params(
        json!({ "source": "github", "ticketId": 218 })
            .as_object()
            .unwrap(),
    )
    .expect("an issue");
    let reused = || {
        existing_ticket_worktree(&store.db, &store.paths, &repository, &project, &issue, None)
            .expect("sessions")
    };

    assert_eq!(remember_tracker(&store, &project), WorkSourceId::Github);
    assert_eq!(reused(), Some(worktree.to_string_lossy().to_string()));

    pick_command_tracker(&store.db, &store.paths, &workspace_id, "start-numbers").expect("picked");
    // No pass has run since: its copy still says GitHub.
    assert_eq!(
        cached_project_work_tracker(project_id(&project)),
        WorkSourceId::Github
    );
    assert_eq!(reused(), None);
}
