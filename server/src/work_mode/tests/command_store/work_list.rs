//! A ticket command's open tickets on the Work list of a workspace whose tracker it is: read by a
//! background worker, so the feeds' refresh only queues it; owed (`command_lists_pending`) until a
//! worker has read it, unless the command can't run; and its rows scoped by the workspace they
//! were read in. Each test pins ids of its own.

use std::sync::Arc;
use std::time::Instant;

use super::cards::{pin, project_id, project_in, release, wait_until};
use super::published::command_source;
use super::*;

/// `project`, with `sessions`, as the Work list reads it in `workspace_id` with `id` its tracker.
fn tracked(
    store: &Store,
    project: &Value,
    sessions: Vec<Value>,
    workspace_id: &str,
    id: &str,
) -> WorkProjectInput {
    WorkProjectInput {
        project_id: project_id(project).to_string(),
        name: project["name"].as_str().unwrap_or("project").to_string(),
        path: project["path"].as_str().map(str::to_string),
        project: project.clone(),
        sessions,
        credentials: Arc::new(WorkCredentials::new(&store.paths)),
        repo: None,
        workspace_id: workspace_id.to_string(),
        tracker: command_source(id),
    }
}

/// Lets `id`'s `wait-release` fixture answer when dropped, so a test that fails early never leaves
/// a list holding a process slot.
struct Released<'s>(&'s Store, &'static str);

impl Drop for Released<'_> {
    fn drop(&mut self) {
        release(self.0, self.1);
    }
}

/// Refreshes the feeds, then waits until a worker has read every open list they owe.
fn read_lists(plan: &WorkFeedPlan) {
    refresh_work_feeds(plan, false);
    wait_until("the open lists", || !command_lists_pending(plan));
}

fn rows_of<'l>(list: &'l Value, id: &str) -> Vec<&'l Value> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["source"] == id)
        .collect()
}

#[test]
fn command_tickets_reach_the_work_list() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-reach", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let projects = [tracked(
        &store,
        &project,
        Vec::new(),
        &workspace_id,
        "wl-reach",
    )];
    let plan = work_feed_plan(&projects);
    // Owed before any worker has read it, whatever queueing it answers.
    assert!(command_lists_pending(&plan));
    read_lists(&plan);
    let list = build_work_list(&projects, &plan);
    let rows = rows_of(&list, "wl-reach");
    assert_eq!(rows.len(), 1, "{list}");
    let key = source_key(&workspace_id, "wl-reach");
    assert_eq!(rows[0]["key"], format!("wl-reach:{key}:ACM-12"));
    assert_eq!(rows[0]["sourceKey"], key);
    assert_eq!(rows[0]["id"], "ACM-12");
    assert_eq!(rows[0]["title"], "Spec the contract");
    assert_eq!(rows[0]["status"]["group"], "progress");
    assert_eq!(rows[0]["canStart"], false);
    assert_eq!(list["errors"], json!([]));
}

/// The feeds' refresh queues the list and returns while the command still runs: the shared
/// refresh lock never waits for a command.
#[test]
fn a_slow_list_never_holds_the_feed_lock() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-slow", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let projects = [tracked(
        &store,
        &project,
        Vec::new(),
        &workspace_id,
        "wl-slow",
    )];
    let plan = work_feed_plan(&projects);
    let released = Released(&store, "wl-slow");
    let started = Instant::now();
    let refresh = std::thread::spawn({
        let plan = plan.clone();
        move || refresh_work_feeds(&plan, false)
    });
    wait_until("the list's command to start", || {
        super::cards::calls(&store, "wl-slow").contains(&"list".to_string())
    });
    wait_until("the refresh to return", || refresh.is_finished());
    assert!(command_lists_pending(&plan), "owed while it runs");
    drop(released);
    wait_until("the open list", || !command_lists_pending(&plan));
    assert!(started.elapsed() < super::cards::HANDSHAKE);
    assert_eq!(
        rows_of(&build_work_list(&projects, &plan), "wl-slow").len(),
        1
    );
}

/// A command that can't run (here, a required credential unset) owes no list, so the page stops
/// asking, and the page's errors say what to set.
#[test]
fn a_command_missing_a_required_credential_never_keeps_refreshing() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-unset", "good");
    mutate_credentials(&store.paths, |credentials| {
        credentials["workspaceSourceCredentials"][&workspace_id]["wl-unset"]
            .as_object_mut()
            .unwrap()
            .remove("token");
    })
    .expect("unset");
    let project = project_in(&store, &workspace_id, json!({}));
    let projects = [tracked(
        &store,
        &project,
        Vec::new(),
        &workspace_id,
        "wl-unset",
    )];
    let plan = work_feed_plan(&projects);
    assert!(!command_lists_pending(&plan));
    let list = build_work_list(&projects, &plan);
    assert_eq!(
        list["errors"],
        json!(["Set Fixture's API token first: `ghostex issues source credential token`."])
    );
    assert!(rows_of(&list, "wl-unset").is_empty());
}

/// Two workspaces pinning the same id answer the same ticket id: two rows, each with its own
/// workspace's key, and a session joins only its own workspace's row.
#[test]
fn two_workspaces_with_the_same_command_keep_their_own_rows() {
    let store = Store::new();
    let first = pin(&store, "ws-a", "wl-two", "good");
    let second = pin(&store, "ws-b", "wl-two", "good");
    let first_project = project_in(&store, &first, json!({}));
    let second_project = project_in(&store, &second, json!({}));
    let linked = session(
        project_id(&second_project),
        "S2",
        json!({ "tickets": { "wl-two": ["ACM-12"] } }),
    );
    let projects = [
        tracked(&store, &first_project, Vec::new(), &first, "wl-two"),
        tracked(&store, &second_project, vec![linked], &second, "wl-two"),
    ];
    let plan = work_feed_plan(&projects);
    assert_eq!(plan.command_lists.len(), 2);
    read_lists(&plan);
    let list = build_work_list(&projects, &plan);
    let rows = rows_of(&list, "wl-two");
    assert_eq!(rows.len(), 2, "{list}");
    let row_of = |workspace_id: &str| {
        let key = source_key(workspace_id, "wl-two");
        *rows
            .iter()
            .find(|row| row["sourceKey"] == key)
            .unwrap_or_else(|| panic!("a row for {workspace_id}: {list}"))
    };
    assert_ne!(row_of(&first)["key"], row_of(&second)["key"]);
    assert_eq!(row_of(&first)["sessions"], json!([]));
    assert_eq!(row_of(&second)["sessions"][0]["sessionId"], "S2");
}

/// A listed key-number ticket that names no project of its own is worked in the project whose
/// sessions link its team's tickets (`team_of`: the key of its id).
#[test]
fn a_key_number_ticket_learns_its_project_from_a_linked_session() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-team", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let linked = session(
        project_id(&project),
        "S1",
        json!({ "tickets": { "wl-team": ["ACM-3"] } }),
    );
    let projects = [tracked(
        &store,
        &project,
        vec![linked],
        &workspace_id,
        "wl-team",
    )];
    let plan = work_feed_plan(&projects);
    read_lists(&plan);
    let list = build_work_list(&projects, &plan);
    let row = rows_of(&list, "wl-team")
        .into_iter()
        .find(|row| row["id"] == "ACM-12")
        .unwrap_or_else(|| panic!("ACM-12: {list}"));
    assert_eq!(row["projectId"], project_id(&project));
}

/// A failure that would only repeat backs off: the list is owed no more, the page's errors show
/// it without the credential it echoed, and it is logged once without the command's words.
#[test]
fn a_failing_list_backs_off_and_shows_its_failure_masked() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-fail", "error-json");
    let project = project_in(&store, &workspace_id, json!({}));
    let projects = [tracked(
        &store,
        &project,
        Vec::new(),
        &workspace_id,
        "wl-fail",
    )];
    let plan = work_feed_plan(&projects);
    read_lists(&plan);
    assert!(backing_off(
        &plan.command_lists[0].plan,
        CommandVerb::OpenList
    ));
    let list = build_work_list(&projects, &plan);
    let token = super::cards::token_of("wl-fail");
    let errors = list["errors"].to_string();
    assert!(errors.contains("Fixture"), "{errors}");
    assert!(!errors.contains(&token), "{errors}");
    assert!(rows_of(&list, "wl-fail").is_empty());
    // The list stops being owed inside the job's commit, and the job logs its failure after it.
    wait_until("the failure's log line", || {
        !super::cards::log_lines(&store, "work_source_failed").is_empty()
    });
    let lines = super::cards::log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(r#""verb":"list""#), "{lines:?}");
    assert!(!lines[0].contains(&token), "{lines:?}");
    assert!(!lines[0].contains("rejected"), "{lines:?}");
}

/// Two workspaces with clones of the same repo pin the same id with different accepted keys, and
/// both list `ACM-12`; the repo's PRs are listed under the first workspace's project. A PR folds
/// only into a ticket of that workspace: PR 10, which a session there links with `ACM-12`, into its
/// row; PR 7, which only the second workspace's `ACM-12` names, into none, and that ticket, which
/// names no project or repo of its own, stays in the second workspace's project; PR 8, whose branch names
/// `ACM-12`, into none, since only the second workspace's command has the `ACM` key; PR 9, which a
/// session of the second workspace links with `ACM-12`, into none either.
#[test]
fn a_pull_request_folds_only_into_its_own_workspaces_command_ticket() {
    let store = Store::new();
    // Its list names no PR (`title-kept`).
    let first = pin(&store, "ws-a", "wl-pr", "title-kept");
    // Its ticket names PR 7, and no project or repo (`pr-only`).
    let second = pin(&store, "ws-b", "wl-pr", "pr-only");
    let mut sources = read_work_sources(&store.db).expect("record");
    sources
        .workspaces
        .get_mut(&first)
        .expect("pinned")
        .info
        .id_keys = vec!["OPS".to_string()];
    save_work_sources(&store.db, &sources).expect("keys");
    let first_project = project_in(&store, &first, json!({}));
    let second_project = project_in(&store, &second, json!({}));
    let links = |project: &Value, session_id: &str, number: u64| {
        session(
            project_id(project),
            session_id,
            json!({ "pullRequest": number, "tickets": { "wl-pr": ["ACM-12"] } }),
        )
    };
    let in_repo = |mut input: WorkProjectInput| {
        input.repo = Some("acme/app".to_string());
        input
    };
    let projects = [
        in_repo(tracked(
            &store,
            &first_project,
            vec![links(&first_project, "S1", 10)],
            &first,
            "wl-pr",
        )),
        in_repo(tracked(
            &store,
            &second_project,
            vec![links(&second_project, "S2", 9)],
            &second,
            "wl-pr",
        )),
    ];
    let plan = work_feed_plan(&projects);
    read_lists(&plan);
    let pull_request = |number, head| {
        list_pull_request(
            number,
            "acme/app",
            PullRequestState::Open,
            head,
            "",
            "2026-10-20T09:00:00.000Z",
        )
    };
    let feed = GithubFeed {
        repo: Some("acme/app".to_string()),
        pull_requests: vec![
            pull_request(7, "fix-checkout"),
            pull_request(8, "acm-12-fix"),
            pull_request(9, "misc"),
            pull_request(10, "ship-it"),
        ],
        issues: Vec::new(),
    };
    seed_work_feeds(
        &[],
        &[(first_project["path"].as_str().unwrap(), feed)],
        Some(VIEWER_LOGIN),
    );
    let list = build_work_list(&projects, &plan);
    let row_of = |workspace_id: &str| {
        let key = source_key(workspace_id, "wl-pr");
        rows_of(&list, "wl-pr")
            .into_iter()
            .find(|row| row["sourceKey"] == key)
            .unwrap_or_else(|| panic!("a row for {workspace_id}: {list}"))
    };
    assert_eq!(row_of(&first)["pullRequest"]["number"], 10, "{list}");
    assert!(row_of(&second).get("pullRequest").is_none(), "{list}");
    assert_eq!(
        row_of(&second)["projectId"],
        project_id(&second_project),
        "{list}"
    );
    let pull_request_rows: Vec<&Value> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == "pullRequest")
        .map(|item| &item["id"])
        .collect();
    assert_eq!(
        pull_request_rows,
        [&json!("#7"), &json!("#8"), &json!("#9")],
        "{list}"
    );
}

/// A list the queue turns away as full is still owed, so the page keeps asking; it stops being owed
/// only once a worker has read it. A queue of its own with no room stands in for a full one: the
/// shared queue is process-wide, and filling it would turn away other tests' jobs.
#[test]
fn a_list_the_full_queue_turns_away_stays_owed() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-full", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let projects = [tracked(
        &store,
        &project,
        Vec::new(),
        &workspace_id,
        "wl-full",
    )];
    let plan = work_feed_plan(&projects);
    let full = JobQueue::new(0);
    assert_eq!(full.push(plan.command_lists[0].clone()), QueueOutcome::Full);
    assert!(command_lists_pending(&plan));
    // Once there is room, a worker reads it.
    run_open_list(&plan.command_lists[0]);
    assert!(!command_lists_pending(&plan));
    assert_eq!(
        rows_of(&build_work_list(&projects, &plan), "wl-full").len(),
        1
    );
}

/// `list` is given the Work list's projects, so a list is kept, and queued, per set of them: one
/// project's answer never stands in for two's, and two's job isn't taken for one's.
#[test]
fn an_open_list_is_kept_and_queued_per_set_of_projects() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "wl-subset", "good");
    let first = project_in(&store, &workspace_id, json!({}));
    let other_path = store.paths.app_data_dir.join("project-subset-second");
    std::fs::create_dir_all(&other_path).expect("project folder");
    let second = project_in(&store, &workspace_id, json!({ "path": other_path }));
    let input = |project: &Value| tracked(&store, project, Vec::new(), &workspace_id, "wl-subset");
    let one = work_feed_plan(&[input(&first)]);
    let both = work_feed_plan(&[input(&second), input(&first)]);

    let queue = JobQueue::new(2);
    assert_eq!(
        queue.push(one.command_lists[0].clone()),
        QueueOutcome::Queued
    );
    assert_eq!(
        queue.push(both.command_lists[0].clone()),
        QueueOutcome::Queued
    );
    // The same projects in another order are the same list.
    let reordered = work_feed_plan(&[input(&first), input(&second)]);
    assert_eq!(
        queue.push(reordered.command_lists[0].clone()),
        QueueOutcome::Duplicate
    );

    run_open_list(&one.command_lists[0]);
    assert!(!command_lists_pending(&one));
    assert!(
        command_lists_pending(&both),
        "one project's answer isn't two's"
    );
    run_open_list(&both.command_lists[0]);
    assert!(!command_lists_pending(&both));
}

/// A ticket's details are read from the row of the requesting project's workspace: two workspaces
/// pinning the same id answer the same ticket id, each in its own row.
#[test]
fn a_command_tickets_details_use_their_own_workspaces_row() {
    let store = Store::new();
    // Its tickets name no PR, so the details read nothing from GitHub.
    let first = pin(&store, "ws-a", "wl-details", "title-kept");
    let second = pin(&store, "ws-b", "wl-details", "title-kept");
    let first_project = project_in(&store, &first, json!({}));
    let second_project = project_in(&store, &second, json!({}));
    let projects = [
        tracked(&store, &first_project, Vec::new(), &first, "wl-details"),
        tracked(&store, &second_project, Vec::new(), &second, "wl-details"),
    ];
    let plan = work_feed_plan(&projects);
    read_lists(&plan);
    let ticket = WorkItemRef::Ticket(TicketRef::new(
        super::published::command_source("wl-details"),
        "ACM-12",
    ));
    for (workspace_id, project) in [(&first, &first_project), (&second, &second_project)] {
        let read = read_work_item(
            &store.paths,
            &projects,
            &plan,
            Some(project_id(project)),
            &ticket,
            false,
        );
        assert_eq!(
            read["item"]["sourceKey"],
            source_key(workspace_id, "wl-details"),
            "{read}"
        );
    }
}

/// A PR's details take the ticket command tickets of the sessions in the requesting project's
/// workspace only, since another workspace's command can answer the same ids; the other sources'
/// tickets come from every session that links the PR, as before.
#[test]
fn a_prs_details_take_command_tickets_from_their_own_workspace() {
    let store = Store::new();
    let first = pin(&store, "ws-a", "wl-pr-details", "good");
    let second = pin(&store, "ws-b", "wl-pr-details", "good");
    let first_project = project_in(&store, &first, json!({}));
    let second_project = project_in(&store, &second, json!({}));
    let links = |project: &Value, session_id: &str, links: Value| {
        let mut input = tracked(
            &store,
            project,
            vec![session(project_id(project), session_id, links)],
            if session_id == "S1" { &first } else { &second },
            "wl-pr-details",
        );
        input.repo = Some("acme/app".to_string());
        input
    };
    let projects = [
        links(
            &first_project,
            "S1",
            json!({ "pullRequest": 9, "tickets": { "wl-pr-details": ["ACM-12"] } }),
        ),
        links(
            &second_project,
            "S2",
            json!({ "pullRequest": 9, "githubIssues": [5], "tickets": { "wl-pr-details": ["ACM-34"] } }),
        ),
    ];
    let named =
        pull_request_tickets_known_locally("9", Some("acme/app"), Some(&first), None, &projects)
            .into_iter()
            .map(|ticket| match ticket {
                WorkItemRef::Ticket(ticket) => format!("{}:{}", ticket.source.as_wire(), ticket.id),
                WorkItemRef::PullRequest(_) => "a PR".to_string(),
            })
            .collect::<Vec<_>>();
    assert_eq!(named, ["wl-pr-details:ACM-12", "github:5"]);
}
