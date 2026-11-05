//! A ticket command's tickets on a session's card. The card pass plans exact reads and queues
//! them; a worker runs each with what it prepared under the store's writer lock (the command's
//! path, its credentials and the mask of everything the workspace stores), holds that lock until
//! the command starts, and keeps the answer only while the setup and the project's placement it
//! was planned against still hold. A failure that would only repeat backs off until the setup
//! changes, and a ticket the command can't be asked about stays on the card with its last title:
//! after a restart, a clear, a Switch or a team join too. Each test pins ids of its own: the
//! record's copy, the caches and the titles are process-wide.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::domain::DomainRepository;

use super::published::{command_source, record_pinning};
use super::*;

/// How long a handshake waits before a test fails instead of hanging.
pub(super) const HANDSHAKE: Duration = Duration::from_secs(20);

/// The stored `token` of `id`'s fixture, which no test output may show.
pub(super) fn token_of(id: &str) -> String {
    format!("tok-{id}-secret")
}

/// Pins the fixture in `workspace` as `id`, a key-number source, with its `token` and the `mode`
/// it answers in stored. Returns the workspace's id.
pub(super) fn pin(store: &Store, workspace: &str, id: &str, mode: &str) -> String {
    let sources = record_pinning(store, workspace, id, IdFormat::KeyNumber);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws(workspace);
    store_values(&store.paths, &workspace_id, id, &token_of(id), mode);
    workspace_id
}

pub(super) fn store_values(
    paths: &GxserverPaths,
    workspace_id: &str,
    id: &str,
    token: &str,
    mode: &str,
) {
    mutate_credentials(paths, |credentials| {
        let workspaces = credentials
            .entry("workspaceSourceCredentials".to_string())
            .or_insert_with(|| json!({}));
        workspaces[workspace_id][id] = json!({ "token": token, "mode": mode });
    })
    .expect("stored");
}

/// A project in `workspace_id`, then every project placed in its workspace.
pub(super) fn project_in(store: &Store, workspace_id: &str, extra: Value) -> Value {
    let path = store
        .paths
        .app_data_dir
        .join(format!("project-{workspace_id}"));
    std::fs::create_dir_all(&path).expect("project folder");
    let mut params = json!({
        "name": "App",
        "path": path,
        "launchSettings": { "workspaceId": workspace_id },
    });
    for (key, value) in extra.as_object().into_iter().flatten() {
        params[key] = value.clone();
    }
    let repository = DomainRepository::new(&store.db, "test");
    let project = repository
        .create_project(params.as_object().unwrap())
        .expect("project");
    place(store);
    project
}

fn place(store: &Store) {
    let repository = DomainRepository::new(&store.db, "test");
    replace_project_workspaces(&store.paths, &repository, &store.db).expect("placed");
}

fn move_project(store: &Store, project: &Value, workspace_id: &str) {
    DomainRepository::new(&store.db, "test")
        .update_project(
            json!({ "projectId": project_id(project), "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("moved");
    place(store);
}

pub(super) fn project_id(project: &Value) -> &str {
    project["projectId"].as_str().unwrap()
}

/// A session of `project` whose branch names `ACM-12`, in a project Issues Workflow is on for (a
/// Linear key applies to it).
pub(super) fn session_on_acm_12(project: &Value) -> Value {
    set_branch(project["path"].as_str().unwrap(), "acm-12-spec");
    remember_project_linear_key(project_id(project), Some(1));
    session(project_id(project), "S1", json!({}))
}

pub(super) fn plan_of(workspace_id: &str, id: &str) -> JobPlan {
    let WorkSourceId::Command(source) = command_source(id) else {
        unreachable!("a command id")
    };
    JobPlan {
        workspace_id: workspace_id.to_string(),
        source,
        generation: work_source_generation(workspace_id),
    }
}

pub(super) fn exact_read(store: &Store, workspace_id: &str, id: &str, ids: &[&str]) -> ExactRead {
    ExactRead {
        plan: plan_of(workspace_id, id),
        ids: ids.iter().map(|id| id.to_string()).collect(),
        projects: Vec::new(),
        paths: store.paths.clone(),
    }
}

/// The reads a card pass over `project`'s `sessions` plans for `id`, without queueing them.
fn planned_reads(store: &Store, project: &Value, sessions: &[Value], id: &str) -> Vec<ExactRead> {
    let WorkSourceId::Command(source) = command_source(id) else {
        unreachable!("a command id")
    };
    let credentials = WorkCredentials::new(&store.paths);
    let projects: HashMap<&str, &Value> = HashMap::from([(project_id(project), project)]);
    source.source().exact_reads_for(&TicketPass {
        paths: &store.paths,
        credentials: &credentials,
        projects: &projects,
        sessions,
    })
}

/// A card pass: queues what it plans for the background workers.
pub(super) fn card_pass(store: &Store, project: &Value, sessions: &[Value], id: &str) {
    let credentials = WorkCredentials::new(&store.paths);
    let projects: HashMap<&str, &Value> = HashMap::from([(project_id(project), project)]);
    work_source(command_source(id)).refresh_tickets(&TicketPass {
        paths: &store.paths,
        credentials: &credentials,
        projects: &projects,
        sessions,
    });
}

/// The command's own read of `ticket`, as the projection's scope (only the project) asks it.
pub(super) fn card_read(project: &Value, id: &str, ticket: &str) -> TicketRead {
    work_source(command_source(id)).cached_ticket(
        &TicketScope {
            project_id: project_id(project),
            ..TicketScope::default()
        },
        ticket,
    )
}

/// What the session's card publishes for its tickets.
fn card_tickets(project: &Value, session: &Value) -> Value {
    presentation_session_work(project, session).expect("Issues Workflow is on")["tickets"].clone()
}

/// The card's link for `key`.
fn card_link(project: &Value, session: &Value, key: &str) -> Value {
    card_tickets(project, session)
        .as_array()
        .into_iter()
        .flatten()
        .find(|link| link["key"] == key)
        .cloned()
        .unwrap_or_else(|| panic!("no {key} on the card: {}", card_tickets(project, session)))
}

/// `id`'s fixture ticket as the card shows it while unavailable, with its last title.
fn unavailable(id: &str) -> Value {
    json!({
        "key": format!("{id}:ACM-12"),
        "kind": id,
        "label": "ACM-12",
        "title": "Spec the contract",
        "state": "unavailable",
        "stateName": "Unavailable",
        "source": "branch",
        "sourceLabel": "From branch",
    })
}

/// The verbs the fixture ran as `id`, in order.
pub(super) fn calls(store: &Store, id: &str) -> Vec<String> {
    let dir = command_work_dir(&store.paths, Some(id)).expect("work dir");
    std::fs::read_to_string(dir.join("calls.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Lets a `wait-release` fixture running as `id` answer.
pub(super) fn release(store: &Store, id: &str) {
    let dir = command_work_dir(&store.paths, Some(id)).expect("work dir");
    std::fs::write(dir.join("release"), "").expect("release");
}

pub(super) fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + HANDSHAKE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn title_rows(store: &Store, workspace_id: &str) -> Vec<(String, String)> {
    let mut statement = store
        .db
        .prepare("SELECT ticketId, title FROM work_ticket_titles WHERE workspaceId = ?1")
        .expect("prepare");
    statement
        .query_map([workspace_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows")
}

pub(super) fn log_lines(store: &Store, event: &str) -> Vec<String> {
    std::fs::read_to_string(&store.paths.log_file)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(event))
        .map(str::to_string)
        .collect()
}

/// Runs `change` on a thread of its own; the receiver answers once it is done.
fn in_background(change: impl FnOnce() + Send + 'static) -> mpsc::Receiver<()> {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        change();
        let _ = done.send(());
    });
    finished
}

#[test]
fn a_branch_names_a_command_ticket_on_the_card() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-branch", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    assert!(matches!(
        card_read(&project, "card-branch", "ACM-12"),
        TicketRead::Unread
    ));

    card_pass(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-branch",
    );
    wait_until("the exact read", || {
        !matches!(
            card_read(&project, "card-branch", "ACM-12"),
            TicketRead::Unread
        )
    });

    assert_published(
        &card_tickets(&project, &session),
        json!([{
            "key": "card-branch:ACM-12",
            "kind": "card-branch",
            "label": "ACM-12",
            "title": "Spec the contract",
            "state": "active",
            "stateName": "In progress",
            "stateGroup": "progress",
            "url": "https://tracker.example/ACM-12",
            "source": "branch",
            "sourceLabel": "From branch",
        }]),
    );
    let dir = command_work_dir(&store.paths, Some("card-branch")).expect("work dir");
    assert_eq!(
        serde_json::from_str::<Value>(&std::fs::read_to_string(dir.join("stdin.txt")).unwrap())
            .unwrap(),
        json!({ "projects": [], "ids": ["ACM-12"] })
    );
    // Still fresh: the next pass plans nothing.
    assert!(planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-branch"
    )
    .is_empty());
    assert_eq!(calls(&store, "card-branch"), ["list"]);
}

#[test]
fn a_ticket_the_command_cant_be_asked_about_keeps_its_last_title() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-title", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-title",
        &["ACM-12"],
    ));
    assert_eq!(
        card_link(&project, &session, "card-title:ACM-12")["title"],
        "Spec the contract"
    );
    assert_eq!(
        title_rows(&store, &workspace_id),
        [("ACM-12".to_string(), "Spec the contract".to_string())]
    );

    // The program moved away: the next pass finds it can't run, and plans nothing.
    let moved = store.program.with_extension("moved");
    std::fs::rename(&store.program, &moved).expect("move program");
    assert!(planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-title"
    )
    .is_empty());
    assert_published(
        &card_link(&project, &session, "card-title:ACM-12"),
        unavailable("card-title"),
    );

    // After a restart, the title comes back from its table.
    forget_workspace_ticket_titles(&workspace_id);
    assert_eq!(
        card_link(&project, &session, "card-title:ACM-12").get("title"),
        None
    );
    load_ticket_titles(&store.db).expect("loaded");
    assert_published(
        &card_link(&project, &session, "card-title:ACM-12"),
        unavailable("card-title"),
    );
    assert_eq!(calls(&store, "card-title"), ["list"]);
}

#[test]
fn a_cleared_command_keeps_its_tickets_on_the_card() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-cleared", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-cleared",
        &["ACM-12"],
    ));
    clear_work_source(&store.db, &workspace_id, &no_references).expect("cleared");
    assert_published(
        &card_tickets(&project, &session),
        json!([unavailable("card-cleared")]),
    );
}

#[test]
fn a_command_switched_from_keeps_its_tickets_on_the_card() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-switched", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-switched",
        &["ACM-12"],
    ));
    // The fixture's `info` now answers `card-switched-to`, which a Switch pins in its place. An id
    // of its own: the format the cards read for an id is id-wide, and other tests' stores pin the
    // shared `fixture` id too.
    store_values(
        &store.paths,
        &workspace_id,
        "card-switched",
        &token_of("card-switched"),
        "info-switched",
    );
    let switched = set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &store.program.display().to_string(),
        true,
        &no_references,
    )
    .expect("switched");
    assert_eq!(switched.source_id, "card-switched-to");
    let tickets = card_tickets(&project, &session);
    assert!(
        tickets
            .as_array()
            .unwrap()
            .contains(&unavailable("card-switched")),
        "{tickets}"
    );
    assert_eq!(
        card_link(&project, &session, "card-switched-to:ACM-12")["loading"],
        true
    );
}

#[test]
fn a_command_stopped_in_a_team_keeps_its_tickets_on_the_card_and_runs_nothing() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-team", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    run_exact_read(&exact_read(&store, &workspace_id, "card-team", &["ACM-12"]));
    let queued_before = exact_read(&store, &workspace_id, "card-team", &["ACM-12"]);
    join_team(&store, &workspace_id);
    place(&store);

    assert!(!work_sources_of(Some(&workspace_id)).contains(&command_source("card-team")));
    assert_published(
        &card_tickets(&project, &session),
        json!([unavailable("card-team")]),
    );
    assert!(planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-team"
    )
    .is_empty());
    let mut planned_now = queued_before;
    planned_now.plan.generation = work_source_generation(&workspace_id);
    assert!(prepare(&planned_now).is_none());
    run_exact_read(&planned_now);
    assert_eq!(calls(&store, "card-team"), ["list"]);
}

#[test]
fn a_failure_that_would_repeat_backs_off_until_the_setup_changes() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-auth", "error-json");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    let job = exact_read(&store, &workspace_id, "card-auth", &["ACM-12"]);
    run_exact_read(&job);
    run_exact_read(&job);
    assert_eq!(calls(&store, "card-auth"), ["list"]);
    assert_eq!(
        JobQueue::new(QUEUE_CAPACITY).push(exact_read(
            &store,
            &workspace_id,
            "card-auth",
            &["ACM-12"]
        )),
        QueueOutcome::BackingOff
    );
    // Never found: the card says so, with no title to show.
    assert_published(
        &card_link(&project, &session, "card-auth:ACM-12"),
        json!({
            "key": "card-auth:ACM-12",
            "kind": "card-auth",
            "label": "ACM-12",
            "state": "unavailable",
            "stateName": "Unavailable",
            "source": "branch",
            "sourceLabel": "From branch",
        }),
    );
    // One line, with what kind of failure it was and never the command's words or its token.
    let lines = log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert!(
        line.contains("ticket source authentication failed"),
        "{line}"
    );
    assert!(line.contains(r#""verb":"list""#), "{line}");
    assert!(!line.contains(&token_of("card-auth")), "{line}");
    assert!(!line.contains("rejected"), "{line}");

    // A credential change bumps the generation: the back-off no longer applies.
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "mode": "good" }).as_object().unwrap(),
    )
    .expect("saved");
    run_exact_read(&exact_read(&store, &workspace_id, "card-auth", &["ACM-12"]));
    let lists = calls(&store, "card-auth")
        .into_iter()
        .filter(|verb| verb == "list")
        .count();
    assert_eq!(lists, 2);
    assert!(matches!(
        card_read(&project, "card-auth", "ACM-12"),
        TicketRead::Found(_)
    ));
}

#[test]
fn a_failure_that_wont_repeat_waits_for_the_tickets_next_read() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-exit", "error-stderr");
    let job = exact_read(&store, &workspace_id, "card-exit", &["ACM-12"]);
    run_exact_read(&job);
    assert!(!backing_off(&job.plan, CommandVerb::ExactList));
    assert!(!command_ticket_due(&job.plan, "ACM-12", Instant::now()));
    assert!(command_ticket_due(
        &job.plan,
        "ACM-12",
        Instant::now() + Duration::from_secs(121)
    ));
    let lines = log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("ticket source command exited"),
        "{lines:?}"
    );
    assert!(!lines[0].contains("reach the server"), "{lines:?}");
}

#[test]
fn a_value_the_workspace_keeps_is_masked_wherever_the_answer_lands() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-kept", "title-kept");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    // Kept for another source the workspace had: never given to this command.
    mutate_credentials(&store.paths, |credentials| {
        let sources = credentials
            .get_mut("workspaceSourceCredentials")
            .expect("stored");
        sources[&workspace_id]["card-old-source"] = json!({ "token": "customer-secret" });
    })
    .expect("kept");
    run_exact_read(&exact_read(&store, &workspace_id, "card-kept", &["ACM-12"]));

    let link = card_link(&project, &session, "card-kept:ACM-12");
    assert_eq!(link["title"], "Renewal for ••••", "{link}");
    let TicketRead::Found(ticket) = card_read(&project, "card-kept", "ACM-12") else {
        panic!("found");
    };
    assert!(!format!("{ticket:?}").contains("customer-secret"));
    assert_eq!(
        title_rows(&store, &workspace_id),
        [("ACM-12".to_string(), "Renewal for ••••".to_string())]
    );
    let dir = command_work_dir(&store.paths, Some("card-kept")).expect("work dir");
    let env = std::fs::read_to_string(dir.join("env.txt")).expect("env");
    assert!(
        !env.contains("customer-secret"),
        "never passed to the command"
    );
}

#[test]
fn a_credentials_file_that_cant_be_read_runs_nothing() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-damaged", "good");
    std::fs::write(
        store.paths.auth_dir.join("work-mode-credentials.json"),
        "{ damaged",
    )
    .expect("damage");
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-damaged",
        &["ACM-12"],
    ));
    assert!(calls(&store, "card-damaged").is_empty());
}

#[test]
fn a_project_moved_after_its_read_was_planned_runs_nothing() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-moved", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let session = session_on_acm_12(&project);
    let reads = planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-moved",
    );
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].projects, [project_id(&project)]);
    move_project(&store, &project, &store.ws("ws-b"));
    run_exact_read(&reads[0]);
    assert!(calls(&store, "card-moved").is_empty());
}

/// A project move waits for a job that checked the project's placement until its command
/// starts, and the job then keeps nothing of its answer.
#[test]
fn a_project_move_waits_for_the_command_to_start_and_the_job_drops_its_answer() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-move-race", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let other = store.ws("ws-b");
    let session = session_on_acm_12(&project);
    let job = planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-move-race",
    )
    .into_iter()
    .next()
    .expect("planned");
    let plan = job.plan.clone();
    let (reached, paused) = mpsc::channel();
    let (resume, release_pause) = mpsc::channel();
    hook_before_spawn(
        &workspace_id,
        BeforeSpawn::Pause {
            reached,
            release: release_pause,
        },
    );
    let worker = std::thread::spawn(move || run_exact_read(&job));
    paused.recv_timeout(HANDSHAKE).expect("prepared");

    let about_to_lock = notify_placement_writer(&store.paths, PlacementChange::Move);
    let moved = {
        let (paths, project_id, other) = (
            store.paths.clone(),
            project_id(&project).to_string(),
            other.clone(),
        );
        in_background(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            let repository = DomainRepository::new(&db, "test");
            place_projects_after(&paths, &repository, &db, || {
                repository
                    .update_project(
                        json!({ "projectId": project_id, "launchSettings": { "workspaceId": other } })
                            .as_object()
                            .unwrap(),
                    )
                    .map(|_| ())
            })
            .expect("moved");
        })
    };
    // Prepared, not started: the move reaches the lock and waits there.
    about_to_lock
        .recv_timeout(HANDSHAKE)
        .expect("the move reached the lock");
    assert!(moved.recv_timeout(Duration::from_millis(200)).is_err());
    assert_eq!(
        cached_project_workspace_id(project_id(&project)),
        Some(workspace_id.clone())
    );
    resume.send(()).expect("resume");
    moved
        .recv_timeout(HANDSHAKE)
        .expect("the move lands once the command started");
    release(&store, "card-move-race");
    worker.join().expect("the job ends");

    assert_eq!(
        cached_project_workspace_id(project_id(&project)),
        Some(other)
    );
    assert!(cached_command_ticket(&plan, "ACM-12").is_none());
    assert!(title_rows(&store, &workspace_id).is_empty());
}

/// Any change to a project's placement, here a plain repository update with no route around it,
/// waits for a job that checked the project's placement until its command starts: the repository
/// takes the placement lock itself (`hold_project_placement`). The job then keeps nothing.
#[test]
fn a_repository_placement_change_waits_for_the_command_to_start() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-repo-race", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let other = store.ws("ws-b");
    let session = session_on_acm_12(&project);
    let job = planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-repo-race",
    )
    .into_iter()
    .next()
    .expect("planned");
    let plan = job.plan.clone();
    let (reached, paused) = mpsc::channel();
    let (resume, release_pause) = mpsc::channel();
    hook_before_spawn(
        &workspace_id,
        BeforeSpawn::Pause {
            reached,
            release: release_pause,
        },
    );
    let worker = std::thread::spawn(move || run_exact_read(&job));
    paused.recv_timeout(HANDSHAKE).expect("prepared");

    let about_to_lock =
        crate::domain::repository::project::notify_project_placement(project_id(&project));
    let updated = {
        let (paths, project_id, other) = (
            store.paths.clone(),
            project_id(&project).to_string(),
            other.clone(),
        );
        in_background(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            DomainRepository::new(&db, "test")
                .update_project(
                    json!({ "projectId": project_id, "launchSettings": { "workspaceId": other } })
                        .as_object()
                        .unwrap(),
                )
                .expect("updated");
        })
    };
    about_to_lock
        .recv_timeout(HANDSHAKE)
        .expect("the update reached the placement lock");
    assert!(updated.recv_timeout(Duration::from_millis(200)).is_err());
    resume.send(()).expect("resume");
    updated
        .recv_timeout(HANDSHAKE)
        .expect("the update lands once the command started");
    release(&store, "card-repo-race");
    worker.join().expect("the job ends");
    assert!(cached_command_ticket(&plan, "ACM-12").is_none());
}

/// A placement change through the repository, whoever makes it, makes the projection forget where
/// the project and its worktrees were until the next replace: their cards read no ticket command
/// rather than the one of the workspace they left.
#[test]
fn a_placement_change_makes_the_projection_forget_the_workspace_left() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-forget", "good");
    let parent = project_in(&store, &workspace_id, json!({}));
    let worktree = project_in(
        &store,
        &workspace_id,
        json!({ "worktree": { "parentProjectId": project_id(&parent) } }),
    );
    let other = store.ws("ws-b");
    let session = session_on_acm_12(&parent);
    assert_eq!(
        work_targets(&parent, &session).ids_of(command_source("card-forget")),
        ["ACM-12"]
    );

    DomainRepository::new(&store.db, "test")
        .update_project(
            json!({ "projectId": project_id(&parent), "launchSettings": { "workspaceId": other } })
                .as_object()
                .unwrap(),
        )
        .expect("moved");
    assert_eq!(cached_project_workspace_id(project_id(&parent)), None);
    assert_eq!(cached_project_workspace_id(project_id(&worktree)), None);
    assert!(work_targets(&parent, &session)
        .ids_of(command_source("card-forget"))
        .is_empty());

    place(&store);
    assert_eq!(
        cached_project_workspace_id(project_id(&worktree)),
        Some(other)
    );
}

/// A replace of the map holds the placement lock from its reads through its publish, so a move
/// through the repository alone (no route's writer lock) waits for the publish, and the forget that
/// follows it is never undone by the replace's older snapshot.
#[test]
fn a_replace_never_brings_back_a_placement_a_move_made_the_projection_forget() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-replace-race", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let other = store.ws("ws-b");
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    pause_placement_replace(&store.paths, reached, release);
    let replacing = {
        let paths = store.paths.clone();
        std::thread::spawn(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            replace_project_workspaces(&paths, &DomainRepository::new(&db, "test"), &db)
        })
    };
    paused
        .recv_timeout(Duration::from_secs(20))
        .expect("the replace read placement");
    let about_to_lock =
        crate::domain::repository::project::notify_project_placement(project_id(&project));
    let (moved, move_done) = mpsc::channel();
    {
        let (paths, project_id) = (store.paths.clone(), project_id(&project).to_string());
        std::thread::spawn(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            let moving = DomainRepository::new(&db, "test").update_project(
                json!({ "projectId": project_id, "launchSettings": { "workspaceId": other } })
                    .as_object()
                    .unwrap(),
            );
            let _ = moved.send(moving.is_ok());
        });
    }
    about_to_lock
        .recv_timeout(Duration::from_secs(20))
        .expect("the move reached the placement lock");
    // Read, not published: the move waits.
    assert!(move_done.recv_timeout(Duration::from_millis(200)).is_err());
    resume.send(()).expect("resume");
    replacing.join().expect("replace thread").expect("replaced");
    assert!(move_done
        .recv_timeout(Duration::from_secs(20))
        .expect("the move lands after the publish"));
    assert_eq!(cached_project_workspace_id(project_id(&project)), None);
}

#[test]
fn a_title_that_cant_be_saved_is_read_again_on_the_next_pass() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-unsaved", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let job = exact_read(&store, &workspace_id, "card-unsaved", &["ACM-12"]);
    store
        .db
        .execute_batch("DROP TABLE work_ticket_titles")
        .expect("drop");
    run_exact_read(&job);
    assert!(matches!(
        card_read(&project, "card-unsaved", "ACM-12"),
        TicketRead::Found(_)
    ));
    assert_eq!(
        last_ticket_title(&workspace_id, "card-unsaved", "ACM-12"),
        None
    );
    assert!(command_ticket_due(&job.plan, "ACM-12", Instant::now()));
    let lines = log_lines(&store, "work_ticket_titles_unsaved");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].contains("Spec the contract"), "{lines:?}");
}

#[test]
fn a_found_ticket_stays_found_when_a_later_read_fails() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-later", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-later",
        &["ACM-12"],
    ));
    // The command answers without a word about this ticket.
    let plan = plan_of(&workspace_id, "card-later");
    remember_exact_read(&plan, &["ACM-12".to_string()], Some((&[], &[])));
    let TicketRead::Found(ticket) = card_read(&project, "card-later", "ACM-12") else {
        panic!("still found");
    };
    assert_eq!(ticket.title.as_deref(), Some("Spec the contract"));
}

#[test]
fn a_ticket_the_command_doesnt_have_is_not_found() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-missing", "not-found");
    let project = project_in(&store, &workspace_id, json!({}));
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-missing",
        &["ACM-12"],
    ));
    assert!(matches!(
        card_read(&project, "card-missing", "ACM-12"),
        TicketRead::NotFound
    ));
}

#[test]
fn two_workspaces_keep_their_own_tickets() {
    let store = Store::new();
    let found_in = pin(&store, "ws-a", "card-two", "good");
    let missing_in = pin(&store, "ws-b", "card-two", "not-found");
    let found_project = project_in(&store, &found_in, json!({}));
    let missing_project = project_in(&store, &missing_in, json!({}));
    run_exact_read(&exact_read(&store, &found_in, "card-two", &["ACM-12"]));
    run_exact_read(&exact_read(&store, &missing_in, "card-two", &["ACM-12"]));
    assert!(matches!(
        card_read(&found_project, "card-two", "ACM-12"),
        TicketRead::Found(_)
    ));
    assert!(matches!(
        card_read(&missing_project, "card-two", "ACM-12"),
        TicketRead::NotFound
    ));
}

#[test]
fn a_job_planned_before_a_change_never_runs() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-stale", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let job = exact_read(&store, &workspace_id, "card-stale", &["ACM-12"]);
    change_work_source_config(&workspace_id, || {}).expect("changed");
    run_exact_read(&job);
    assert!(calls(&store, "card-stale").is_empty());
    assert!(matches!(
        card_read(&project, "card-stale", "ACM-12"),
        TicketRead::Unread
    ));
}

/// The store's writer lock is held from the job's check until its command starts: a change waits
/// for the start, lands while the command runs, and the job keeps nothing of its answer.
#[test]
fn a_change_waits_for_the_command_to_start_and_the_job_drops_its_answer() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-locked", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let job = exact_read(&store, &workspace_id, "card-locked", &["ACM-12"]);
    let plan = job.plan.clone();
    let (reached, paused) = mpsc::channel();
    let (resume, release_pause) = mpsc::channel();
    hook_before_spawn(
        &workspace_id,
        BeforeSpawn::Pause {
            reached,
            release: release_pause,
        },
    );
    let worker = std::thread::spawn(move || run_exact_read(&job));
    paused.recv_timeout(HANDSHAKE).expect("prepared");

    // Prepared, not started: the lock is held, so the change waits.
    assert!(work_source_writer_held());
    let changed = {
        let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
        in_background(move || {
            change_work_source_config(&workspace_id, || {
                store_values(&paths, &workspace_id, "card-locked", "tok-new", "good");
            })
            .expect("changed")
        })
    };
    assert!(calls(&store, "card-locked").is_empty());
    assert!(changed.try_recv().is_err());
    resume.send(()).expect("resume");

    // Started: the lock is free while the command runs, so the change lands before it answers.
    changed
        .recv_timeout(HANDSHAKE)
        .expect("the change lands while the command runs");
    release(&store, "card-locked");
    worker.join().expect("the job ends");

    let dir = command_work_dir(&store.paths, Some("card-locked")).expect("work dir");
    let env = std::fs::read_to_string(dir.join("env.txt")).expect("env");
    assert!(env.contains(&format!(
        "GHOSTEX_CREDENTIAL_TOKEN={}",
        token_of("card-locked")
    )));
    assert!(cached_command_ticket(&plan, "ACM-12").is_none());
    assert!(matches!(
        card_read(&project, "card-locked", "ACM-12"),
        TicketRead::Unread
    ));
    assert!(title_rows(&store, &workspace_id).is_empty());
}

#[test]
fn the_queue_holds_each_job_once_and_at_most_its_capacity() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-queue", "good");
    let queue = JobQueue::new(2);
    let job = |ids: &[&str]| exact_read(&store, &workspace_id, "card-queue", ids);
    assert_eq!(queue.push(job(&["ACM-1", "ACM-2"])), QueueOutcome::Queued);
    assert_eq!(
        queue.push(job(&["ACM-2", "ACM-1"])),
        QueueOutcome::Duplicate
    );
    assert_eq!(queue.push(job(&["ACM-3"])), QueueOutcome::Queued);
    assert_eq!(queue.push(job(&["ACM-4"])), QueueOutcome::Full);
    assert_eq!(queue.len(), 2);
}

#[test]
fn a_job_that_panics_frees_its_key_and_its_worker_goes_on() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-panic", "good");
    let queue: &'static JobQueue = Box::leak(Box::new(JobQueue::new(4)));
    let (ran, finished) = mpsc::channel::<Vec<String>>();
    std::thread::spawn(move || {
        drain(queue, move |job| {
            let CommandJob::ExactRead(job) = job else {
                return;
            };
            if job.ids == ["ACM-1"] {
                // Unwinds like a panic, without the panic hook's report.
                std::panic::resume_unwind(Box::new("a job panics"));
            }
            let _ = ran.send(job.ids.clone());
        })
    });
    let job = |ids: &[&str]| exact_read(&store, &workspace_id, "card-panic", ids);
    assert_eq!(queue.push(job(&["ACM-1"])), QueueOutcome::Queued);
    assert_eq!(queue.push(job(&["ACM-2"])), QueueOutcome::Queued);
    assert_eq!(finished.recv_timeout(HANDSHAKE).expect("ran"), ["ACM-2"]);
    assert_eq!(queue.push(job(&["ACM-1"])), QueueOutcome::Queued);
}

/// Jobs that unwind while they hold a slot and the store's writer lock, as many as there are
/// slots: each frees both, so a job after them runs and a change after them lands.
#[test]
fn a_job_that_panics_holding_a_slot_and_the_lock_frees_both() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-held", "good");
    let queue: &'static JobQueue = Box::leak(Box::new(JobQueue::new(QUEUE_CAPACITY)));
    /// Says which job ended, when it returns or unwinds.
    struct Ended(mpsc::Sender<Vec<String>>, Vec<String>);
    impl Drop for Ended {
        fn drop(&mut self) {
            let _ = self.0.send(self.1.clone());
        }
    }
    let (done, finished) = mpsc::channel::<Vec<String>>();
    std::thread::spawn(move || {
        drain(queue, move |job| {
            let CommandJob::ExactRead(job) = job else {
                return;
            };
            let _ended = Ended(done.clone(), job.ids.clone());
            run_exact_read(job);
        })
    });
    hook_before_spawn(&workspace_id, BeforeSpawn::Panic);
    for n in 0..PROCESS_SLOTS {
        let id = format!("ACM-{n}");
        assert_eq!(
            queue.push(exact_read(&store, &workspace_id, "card-held", &[&id])),
            QueueOutcome::Queued
        );
    }
    for _ in 0..PROCESS_SLOTS {
        finished
            .recv_timeout(HANDSHAKE)
            .expect("an unwinding job ended");
    }
    unhook_before_spawn(&workspace_id);
    let job = exact_read(&store, &workspace_id, "card-held", &["ACM-12"]);
    assert_eq!(queue.push(job), QueueOutcome::Queued);
    assert_eq!(
        finished.recv_timeout(HANDSHAKE).expect("the job ran"),
        ["ACM-12"]
    );
    assert!(matches!(
        cached_command_ticket(&plan_of(&workspace_id, "card-held"), "ACM-12"),
        Some(CachedTicket::Found(_))
    ));
    assert!(calls(&store, "card-held") == ["list"]);
    in_background(|| change_work_source_config("card-held-unrelated", || {}).expect("changed"))
        .recv_timeout(HANDSHAKE)
        .expect("the lock is free");
}

#[test]
fn the_background_keeps_one_slot_for_a_call_someone_waits_for() {
    let slots = Slots::new(PROCESS_SLOTS);
    let background: Vec<_> = (0..BACKGROUND_WORKERS)
        .map(|_| slots.try_take(SlotUse::Background).expect("free"))
        .collect();
    assert!(slots.try_take(SlotUse::Background).is_none());
    let awaited = slots.try_take(SlotUse::Awaited).expect("the reserved slot");
    assert!(slots.try_take(SlotUse::Awaited).is_none());
    drop(awaited);
    drop(background);
    assert!(slots.try_take(SlotUse::Background).is_some());
}

#[test]
fn a_prepared_run_prints_no_credential() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-debug", "good");
    let prepared = prepare(&exact_read(
        &store,
        &workspace_id,
        "card-debug",
        &["ACM-12"],
    ))
    .expect("the plan holds");
    let printed = format!("{prepared:?}");
    drop(prepared);
    assert!(!printed.contains(&token_of("card-debug")), "{printed}");
    assert!(printed.contains("card-debug"), "{printed}");
}

#[test]
fn each_project_is_placed_in_its_workspace_and_a_worktree_in_its_parents() {
    let store = Store::new();
    let first = store.ws("ws-first");
    let second = store.ws("ws-second");
    let parent = project_in(&store, &first, json!({}));
    let worktree = project_in(
        &store,
        &second,
        json!({ "worktree": { "parentProjectId": project_id(&parent) } }),
    );
    assert_eq!(
        cached_project_workspace_id(project_id(&parent)),
        Some(first.clone())
    );
    assert_eq!(
        cached_project_workspace_id(project_id(&worktree)),
        Some(first)
    );

    move_project(&store, &parent, &second);
    assert_eq!(
        cached_project_workspace_id(project_id(&worktree)),
        Some(second)
    );

    DomainRepository::new(&store.db, "test")
        .remove_project(project_id(&parent))
        .expect("removed");
    place(&store);
    assert_eq!(cached_project_workspace_id(project_id(&parent)), None);
}

#[test]
fn last_titles_keep_the_newest_2000() {
    let store = Store::new();
    let workspace_id = store.ws("ws-titles");
    let titles: Vec<(String, String)> = (0..MAX_TICKET_TITLES)
        .map(|n| (format!("ACM-{n}"), format!("Ticket {n}")))
        .collect();
    save_ticket_titles(&store.db, &workspace_id, "titles-cap", &titles).expect("saved");
    std::thread::sleep(Duration::from_millis(5));
    save_ticket_titles(
        &store.db,
        &workspace_id,
        "titles-cap",
        &[("ACM-new".to_string(), "Newest".to_string())],
    )
    .expect("saved");
    let rows = title_rows(&store, &workspace_id);
    assert_eq!(rows.len(), MAX_TICKET_TITLES);
    assert!(!rows.iter().any(|(id, _)| id == "ACM-0"));
    assert_eq!(
        last_ticket_title(&workspace_id, "titles-cap", "ACM-0"),
        None
    );
    // Nor among the workspace's titled tickets.
    assert!(!titled_tickets(&workspace_id)["titles-cap"].contains(&"ACM-0".to_string()));
    assert_eq!(
        last_ticket_title(&workspace_id, "titles-cap", "ACM-new").as_deref(),
        Some("Newest")
    );
}

#[test]
fn an_unchanged_title_writes_nothing_and_a_changed_one_its_own_row() {
    let store = Store::new();
    let workspace_id = store.ws("ws-titles");
    let updated_at = |id: &str| -> String {
        store
            .db
            .query_row(
                "SELECT updatedAt FROM work_ticket_titles WHERE workspaceId = ?1 AND ticketId = ?2",
                [workspace_id.as_str(), id],
                |row| row.get(0),
            )
            .expect("row")
    };
    let save = |titles: &[(&str, &str)]| {
        let titles: Vec<(String, String)> = titles
            .iter()
            .map(|(id, title)| (id.to_string(), title.to_string()))
            .collect();
        save_ticket_titles(&store.db, &workspace_id, "titles-rows", &titles).expect("saved");
    };
    save(&[("ACM-1", "One"), ("ACM-2", "Two")]);
    let (first, second) = (updated_at("ACM-1"), updated_at("ACM-2"));
    std::thread::sleep(Duration::from_millis(5));
    save(&[("ACM-1", "One"), ("ACM-2", "Two, renamed")]);
    assert_eq!(updated_at("ACM-1"), first);
    assert_ne!(updated_at("ACM-2"), second);
    assert_eq!(
        last_ticket_title(&workspace_id, "titles-rows", "ACM-2").as_deref(),
        Some("Two, renamed")
    );
}

#[test]
fn deleting_a_workspace_deletes_its_titles() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-deleted", "good");
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-deleted",
        &["ACM-12"],
    ));
    assert_eq!(title_rows(&store, &workspace_id).len(), 1);
    delete_workspace_work_source(&store.db, &store.paths, &workspace_id, &no_references)
        .expect("deleting")
        .commit();
    assert!(title_rows(&store, &workspace_id).is_empty());
    assert_eq!(
        last_ticket_title(&workspace_id, "card-deleted", "ACM-12"),
        None
    );
}

#[test]
fn a_hand_set_command_ticket_replaces_the_branchs_and_stays_after_a_clear() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-hand", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    set_branch(project["path"].as_str().unwrap(), "acm-7-other");
    remember_project_linear_key(project_id(&project), Some(1));
    let session = session(
        project_id(&project),
        "S1",
        json!({ "tickets": { "card-hand": ["ACM-12"] } }),
    );
    let reads = planned_reads(
        &store,
        &project,
        std::slice::from_ref(&session),
        "card-hand",
    );
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].ids, ["ACM-12"]);
    run_exact_read(&reads[0]);
    let tickets = card_tickets(&project, &session);
    assert_eq!(tickets.as_array().map(Vec::len), Some(1), "{tickets}");
    assert_eq!(tickets[0]["key"], "card-hand:ACM-12");
    assert_eq!(tickets[0]["source"], "user");
    assert_eq!(tickets[0]["title"], "Spec the contract");

    clear_work_source(&store.db, &workspace_id, &no_references).expect("cleared");
    let tickets = card_tickets(&project, &session);
    assert_eq!(tickets.as_array().map(Vec::len), Some(1), "{tickets}");
    assert_eq!(tickets[0]["state"], "unavailable");
    assert_eq!(tickets[0]["source"], "user");
    assert_eq!(tickets[0]["title"], "Spec the contract");
}

#[test]
fn a_ticket_command_turns_issues_workflow_on_and_shows_on_the_project_row() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-row", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    assert!(issues_workflow_parts(&project).command);
    assert!(project_work_mode(&project));
    let source = presentation_work_source(&project).expect("a work source");
    assert_eq!(source["id"], "card-row");
    assert_eq!(source["name"], "Fixture");
    assert_eq!(source["isTracker"], false);
    // The row carries no icon (CDXC:WorkMode 2026-11-03 in work_mode/project.rs).
    assert_eq!(
        source.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["id", "name", "isTracker"]
    );

    // A project in a workspace without one has neither.
    let bare = project_in(&store, &store.ws("ws-bare"), json!({}));
    assert!(!issues_workflow_parts(&bare).command);
    assert!(presentation_work_source(&bare).is_none());

    // In a team workspace the command is stopped.
    join_team(&store, &workspace_id);
    place(&store);
    assert!(!issues_workflow_parts(&project).command);
    assert!(presentation_work_source(&project).is_none());
}

/// A cleared command that was the only part of Issues Workflow for its project turns Issues
/// Workflow off there, so the card shows no work fields at all, its tickets included. With a
/// Linear key or `gh` on, they stay as unavailable (`a_cleared_command_keeps_its_tickets_on_the_card`).
#[test]
fn a_cleared_command_that_was_issues_workflows_only_part_leaves_the_cards() {
    let _gh = crate::session_git_status::assume_gh_cli_signed_out_for_test();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-only-part", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    set_branch(project["path"].as_str().unwrap(), "acm-12-spec");
    let session = session(project_id(&project), "S1", json!({}));
    run_exact_read(&exact_read(
        &store,
        &workspace_id,
        "card-only-part",
        &["ACM-12"],
    ));
    assert_eq!(
        card_link(&project, &session, "card-only-part:ACM-12")["title"],
        "Spec the contract"
    );

    clear_work_source(&store.db, &workspace_id, &no_references).expect("cleared");
    assert!(!project_work_mode(&project));
    assert_eq!(presentation_session_work(&project, &session), None);
}

/// A project moved into a team workspace by any repository write (a re-added path repaired into a
/// worktree, for one) never shows the workspace it left's command on its row, not even before the
/// next card pass: the projection forgets it at once, and a team workspace shows none.
#[test]
fn a_project_moved_into_a_team_workspace_shows_no_work_source() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "card-to-team", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let team = store.ws("ws-team");
    join_team(&store, &team);
    assert!(presentation_work_source(&project).is_some());

    DomainRepository::new(&store.db, "test")
        .update_project(
            json!({ "projectId": project_id(&project), "launchSettings": { "workspaceId": team } })
                .as_object()
                .unwrap(),
        )
        .expect("moved");
    assert!(presentation_work_source(&project).is_none());
    assert!(!issues_workflow_parts(&project).command);

    place(&store);
    assert!(presentation_work_source(&project).is_none());
}
