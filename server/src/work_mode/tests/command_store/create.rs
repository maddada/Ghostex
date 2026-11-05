//! Creating a ticket with a workspace's ticket command: once, never retried, one at a time per
//! workspace and command, never started late, refused for a while after a timeout, and masked.
//! The tests share the ids `create-a` and `create-b`: the process interns at most 256 ids, and
//! every map a create keeps is keyed by the workspace, which each test makes its own.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::cards::{
    calls, exact_read, log_lines, pin, project_id, project_in, store_values, token_of, HANDSHAKE,
};
use super::details::{open, wait_for_calls, Released};
use super::published::command_source;
use super::start::wait_for_background_reads;
use super::*;

/// A test's own time limit for `create`, past which `create-slow` still sleeps.
const SHORT: Duration = Duration::from_millis(300);

fn create(
    paths: &GxserverPaths,
    project: &Value,
    workspace_id: &str,
    id: &str,
    timeout: Duration,
) -> Result<CreateOutcome, String> {
    create_with(
        &WorkCredentials::new(paths),
        project,
        workspace_id,
        id,
        timeout,
        Instant::now() + CREATE_WRITER_WAIT,
    )
}

/// `create` with the credentials it reads and the moment it must start by.
fn create_with(
    credentials: &WorkCredentials,
    project: &Value,
    workspace_id: &str,
    id: &str,
    timeout: Duration,
    admit_by: Instant,
) -> Result<CreateOutcome, String> {
    let WorkSourceId::Command(source) = command_source(id) else {
        unreachable!("a command id")
    };
    let extra = serde_json::Map::new();
    source.source().create_ticket(
        &TicketScope {
            project_id: project_id(project),
            effective_workspace_id: Some(workspace_id),
            credentials: Some(credentials),
            ..TicketScope::default()
        },
        &TicketDraft {
            title: "  Try it  ",
            description: Some("Body"),
            assign_to_me: true,
            extra: &extra,
        },
        &CommandProject {
            id: project_id(project).to_string(),
            name: "Demo".to_string(),
            path: None,
            repo: Some("acme/app".to_string()),
        },
        timeout,
        admit_by,
    )
}

fn create_in_background(
    store: &Store,
    project: &Value,
    workspace_id: &str,
    id: &str,
) -> mpsc::Receiver<Result<CreateOutcome, String>> {
    let (answered, answer) = mpsc::channel();
    let (paths, project, workspace_id, id) = (
        store.paths.clone(),
        project.clone(),
        workspace_id.to_string(),
        id.to_string(),
    );
    std::thread::spawn(move || {
        let _ = answered.send(create(&paths, &project, &workspace_id, &id, SHORT * 20));
    });
    answer
}

fn created(outcome: Result<CreateOutcome, String>) -> CommandCreated {
    match outcome {
        Ok(CreateOutcome::Created(created)) => created,
        other => panic!("not created: {other:?}"),
    }
}

/// The ticket the command made, from its draft and the project it was asked from; nothing of a
/// source's own pickers reaches it.
#[test]
fn creating_a_command_ticket_returns_its_id_url_and_branch() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-ok");
    let project = project_in(&store, &workspace_id, json!({}));
    let made = created(create(
        &store.paths,
        &project,
        &workspace_id,
        "create-a",
        SHORT * 20,
    ));
    assert_eq!(made.id, "ACM-41");
    assert_eq!(made.url.as_deref(), Some("https://tracker.example/ACM-41"));
    assert_eq!(made.branch_name.as_deref(), Some("dev/acm-41"));
    assert_eq!(calls(&store, "create-a"), ["create"]);
    let dir = command_work_dir(&store.paths, Some("create-a")).expect("work dir");
    let input: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("stdin.txt")).expect("input"))
            .expect("json");
    assert_eq!(
        input,
        json!({
            "title": "Try it",
            "description": "Body",
            "assignToMe": true,
            "project": { "id": project_id(&project), "name": "Demo", "path": null, "repo": "acme/app" },
        })
    );
}

/// A create that times out ran once, says it may have made the ticket, and keeps every create of
/// that workspace and command refused: after a check of the command and after a credential
/// change too, which say nothing about whether it made one.
#[test]
fn a_timed_out_create_runs_once_and_blocks_creates_until_the_block_expires() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-slow");
    let project = project_in(&store, &workspace_id, json!({}));
    let started = Instant::now();
    let outcome = create(&store.paths, &project, &workspace_id, "create-a", SHORT);
    assert!(started.elapsed() < Duration::from_secs(10));
    let Ok(CreateOutcome::Failed {
        error,
        may_have_been_created: true,
        ..
    }) = outcome
    else {
        panic!("not a timeout: {outcome:?}");
    };
    assert!(
        error.contains("The ticket may have been created, so check Fixture before trying again."),
        "{error}"
    );
    assert_eq!(calls(&store, "create-a"), ["create"]);
    let blocked = Err(
        "Fixture's create a moment ago may have made the ticket; check Fixture before creating again."
            .to_string(),
    );
    let again = || {
        create(&store.paths, &project, &workspace_id, "create-a", SHORT)
            .map(|outcome| format!("{outcome:?}"))
    };
    assert_eq!(again(), blocked);
    // The command answers again, under this test's id, and a check runs it.
    store_values(
        &store.paths,
        &workspace_id,
        "create-a",
        &token_of("create-a"),
        "hourly create-a",
    );
    check_work_source(&store.db, &store.paths, &workspace_id).expect("checked");
    assert_eq!(again(), blocked);
    change_work_source_config(&workspace_id, || {
        store_values(
            &store.paths,
            &workspace_id,
            "create-a",
            "tok-create-new",
            "hourly create-a",
        );
        Ok::<_, crate::domain::DomainStateError>(())
    })
    .expect("changed")
    .expect("stored");
    assert_eq!(again(), blocked);
    assert_eq!(calls(&store, "create-a")[..2], ["create", "info"]);
    assert_eq!(
        calls(&store, "create-a")
            .iter()
            .filter(|call| *call == "create")
            .count(),
        1
    );
}

/// A create that ran and then failed in a way its answer can't explain (an unreadable answer, one
/// past its cap, an exit without an error object) may have made the ticket, and says so; one that
/// answered an error object says only what it said. Each that may have made it blocks the next
/// create, as a timeout does.
#[test]
fn a_create_that_ran_and_failed_without_saying_why_may_have_made_the_ticket() {
    let _slots = holding_slots();
    for (mode, may) in [
        ("create-unreadable", true),
        ("create-too-large", true),
        ("create-exited", true),
        ("create-reported", false),
    ] {
        let store = Store::new();
        let workspace_id = pin(&store, "ws-a", "create-a", mode);
        let project = project_in(&store, &workspace_id, json!({}));
        let outcome = create(
            &store.paths,
            &project,
            &workspace_id,
            "create-a",
            SHORT * 20,
        );
        let Ok(CreateOutcome::Failed {
            error,
            may_have_been_created,
            ..
        }) = outcome
        else {
            panic!("{mode}: not a failed create: {outcome:?}");
        };
        assert_eq!(may_have_been_created, may, "{mode}: {error}");
        assert_eq!(
            error.contains(
                "The ticket may have been created, so check Fixture before trying again."
            ),
            may,
            "{mode}: {error}"
        );
        let again = create(
            &store.paths,
            &project,
            &workspace_id,
            "create-a",
            SHORT * 20,
        );
        assert_eq!(
            again.is_err(),
            may,
            "{mode}: blocked only when it may have made the ticket: {again:?}"
        );
        let creates = calls(&store, "create-a")
            .iter()
            .filter(|call| *call == "create")
            .count();
        assert_eq!(creates, if may { 1 } else { 2 }, "{mode}");
    }
}

/// A second create while one runs is refused at once and runs nothing; it never shares the
/// first one's ticket.
#[test]
fn a_second_create_while_one_runs_is_refused() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-wait");
    let project = project_in(&store, &workspace_id, json!({}));
    let released = Released(&store, "create-a");
    let first = create_in_background(&store, &project, &workspace_id, "create-a");
    wait_for_calls(&store, "create-a", "create", 1);
    let started = Instant::now();
    assert_eq!(
        create(&store.paths, &project, &workspace_id, "create-a", SHORT).map(|_| ()),
        Err("Fixture is still creating a ticket; wait for it to answer.".to_string())
    );
    assert!(started.elapsed() < Duration::from_millis(500));
    drop(released);
    assert_eq!(
        created(first.recv_timeout(HANDSHAKE).expect("answered")).id,
        "ACM-41"
    );
    assert_eq!(calls(&store, "create-a"), ["create"]);
}

/// With every process slot taken, a create is refused at once and runs nothing.
#[test]
fn a_create_with_no_free_slot_is_refused_at_once() {
    let _slots = holding_slots();
    let store = Store::new();
    let busy = pin(&store, "ws-busy", "create-b", "wait-release");
    let released = Released(&store, "create-b");
    for ticket in ["ACM-1", "ACM-2", "ACM-3"] {
        assert_eq!(
            queue_exact_read(exact_read(&store, &busy, "create-b", &[ticket])),
            QueueOutcome::Queued
        );
    }
    wait_for_calls(&store, "create-b", "list", BACKGROUND_WORKERS);
    let holding = {
        let (paths, project, workspace_id) = (
            store.paths.clone(),
            project_in(&store, &busy, json!({})),
            busy.clone(),
        );
        std::thread::spawn(move || open(&paths, &project, &workspace_id, "create-b"))
    };
    wait_for_calls(&store, "create-b", "detail", 1);
    let workspace_id = pin(&store, "ws-a", "create-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    // On its own thread: a create that waited for a slot would never answer here.
    let refused = create_in_background(&store, &project, &workspace_id, "create-a")
        .recv_timeout(Duration::from_millis(500))
        .expect("refused at once");
    assert_eq!(
        refused.map(|_| ()),
        Err(
            "Fixture is busy answering another request; nothing was created. Try again in a moment."
                .to_string()
        )
    );
    assert!(calls(&store, "create-a").is_empty());
    drop(released);
    let _ = holding.join();
    wait_for_background_reads(&busy, "create-b");
}

/// A create waits at most `CREATE_WRITER_WAIT` for the store's writer lock, then refuses and runs
/// nothing, so it can't start late and outlive its client.
#[test]
fn a_create_never_waits_past_two_seconds_for_the_writer_lock() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let writer = hold_work_source_writer().expect("writer");
    let answer = create_in_background(&store, &project, &workspace_id, "create-a");
    let started = Instant::now();
    let refused = answer.recv_timeout(HANDSHAKE).expect("answered");
    drop(writer);
    assert!(
        started.elapsed() >= CREATE_WRITER_WAIT - Duration::from_millis(200),
        "{:?}",
        started.elapsed()
    );
    assert!(started.elapsed() < CREATE_WRITER_WAIT * 2);
    assert_eq!(
        refused.map(|_| ()),
        Err(
            "Fixture is busy answering another request; nothing was created. Try again in a moment."
                .to_string()
        )
    );
    assert!(calls(&store, "create-a").is_empty());
}

/// A create needs a command that creates, pinned in the workspace outside a team, with every
/// required credential set; each refusal runs nothing.
#[test]
fn create_needs_can_create_a_pin_no_team_and_required_credentials() {
    let _slots = holding_slots();
    let store = Store::new();
    let id = "create-a";
    let refusal = |case: &str, prepare: &dyn Fn(&str)| {
        let workspace_id = pin(&store, &format!("ws-{case}"), id, "good");
        let project = project_in(&store, &workspace_id, json!({}));
        prepare(&workspace_id);
        let refused = create(&store.paths, &project, &workspace_id, id, SHORT)
            .map(|_| ())
            .expect_err(case);
        assert!(!refused.contains(&token_of(id)), "{refused}");
        refused
    };
    let before = calls(&store, id).len();
    assert_eq!(
        refusal("no-create", &|workspace_id| {
            let mut sources = read_work_sources(&store.db).expect("record");
            sources
                .workspaces
                .get_mut(workspace_id)
                .expect("pinned")
                .info
                .can_create = false;
            save_work_sources(&store.db, &sources).expect("saved");
        }),
        "Fixture doesn't create tickets."
    );
    assert!(refusal("cleared", &|workspace_id| {
        clear_work_source(&store.db, workspace_id, &no_references).expect("cleared");
    })
    .ends_with("isn't this workspace's ticket command."));
    assert_eq!(
        refusal("team", &|workspace_id| join_team(&store, workspace_id)),
        TEAM_REFUSAL
    );
    assert!(refusal("unset", &|workspace_id| {
        store_values(&store.paths, workspace_id, id, "", "good");
    })
    .starts_with("Set Fixture's API token first"));
    assert_eq!(calls(&store, id).len(), before);
}

/// A created ticket whose id holds a value the workspace stores is made, so the words say so,
/// and the id isn't shown or kept. A kept value no record declares counts as secret, so the
/// answer's own guard refuses that id. Either way the ticket was made, so a create right after it
/// is refused and runs nothing.
#[test]
fn a_created_id_holding_a_stored_value_says_it_was_created() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "good");
    mutate_credentials(&store.paths, |credentials| {
        credentials["workspaceSourceCredentials"][workspace_id.as_str()]["create-elsewhere"] =
            json!({ "org": "ACM" });
    })
    .expect("kept");
    let project = project_in(&store, &workspace_id, json!({}));
    let refused = match create(
        &store.paths,
        &project,
        &workspace_id,
        "create-a",
        SHORT * 20,
    ) {
        Err(refused) => refused,
        Ok(outcome) => panic!("not refused: {outcome:?}"),
    };
    assert_eq!(refused, format!("Fixture {CREATED_ID_SPELLS_CREDENTIAL}"));
    assert!(!refused.contains("ACM-41"));
    assert_eq!(calls(&store, "create-a"), ["create"]);
    // The ticket was made, so creating again right away is refused and runs nothing.
    let blocked = Err(
        "Fixture's create a moment ago may have made the ticket; check Fixture before creating again."
            .to_string(),
    );
    assert_eq!(
        create(
            &store.paths,
            &project,
            &workspace_id,
            "create-a",
            SHORT * 20
        )
        .map(|_| ()),
        blocked
    );
    assert_eq!(calls(&store, "create-a"), ["create"]);
    // `mode` isn't secret, so only the check against every stored value finds it in the id.
    let not_secret = pin(&store, "ws-b", "create-b", "ACM-4");
    let not_secret_project = project_in(&store, &not_secret, json!({}));
    assert_eq!(
        create(
            &store.paths,
            &not_secret_project,
            &not_secret,
            "create-b",
            SHORT * 20
        )
        .map(|_| ()),
        Err(format!("Fixture {CREATED_ID_SPELLS_CREDENTIAL}"))
    );
    assert_eq!(calls(&store, "create-b"), ["create"]);
    assert_eq!(
        create(
            &store.paths,
            &not_secret_project,
            &not_secret,
            "create-b",
            SHORT * 20
        )
        .map(|_| ()),
        blocked
    );
    assert_eq!(calls(&store, "create-b"), ["create"]);
}

/// Neither an answer nor a failure shows the token, and the failure's log line holds none of the
/// command's words.
#[test]
fn a_create_answer_never_shows_a_stored_value() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-token");
    let project = project_in(&store, &workspace_id, json!({}));
    let token = token_of("create-a");
    let made = created(create(
        &store.paths,
        &project,
        &workspace_id,
        "create-a",
        SHORT * 20,
    ));
    assert!(!format!("{made:?}").contains(&token), "{made:?}");
    assert_eq!(made.url, None);
    assert_eq!(made.branch_name, None);
    // `spec` isn't secret, so only the check against every stored value drops the branch.
    let not_secret = pin(&store, "ws-c", "create-b", "spec");
    let not_secret_project = project_in(&store, &not_secret, json!({}));
    let made = created(create(
        &store.paths,
        &not_secret_project,
        &not_secret,
        "create-b",
        SHORT * 20,
    ));
    assert_eq!((made.id.as_str(), made.branch_name), ("ACM-41", None));
    let failing = pin(&store, "ws-b", "create-a", "error-token");
    let failing_project = project_in(&store, &failing, json!({}));
    let failed = create(
        &store.paths,
        &failing_project,
        &failing,
        "create-a",
        SHORT * 20,
    );
    let Ok(CreateOutcome::Failed {
        error,
        may_have_been_created: false,
        ..
    }) = &failed
    else {
        panic!("not a failure: {failed:?}");
    };
    assert!(!error.contains(&token), "{error}");
    let lines = log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(r#""verb":"create""#), "{lines:?}");
    assert!(!lines[0].contains(&token), "{lines:?}");
    assert!(!lines[0].contains("rejected"), "{lines:?}");
}

/// The project row says its workspace's command creates only while its `info` says so and the
/// card pass found it can run.
#[test]
fn the_project_row_says_whether_its_command_creates() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let can_create =
        || crate::work_mode::presentation_work_source(&project).expect("row")["canCreate"].clone();
    assert_eq!(can_create(), json!(false), "not checked yet");
    let generation = work_source_generation(&workspace_id);
    remember_command_readiness(&workspace_id, generation, true);
    assert_eq!(can_create(), json!(true));
    remember_command_readiness(&workspace_id, generation, false);
    assert_eq!(can_create(), json!(false));
    remember_command_readiness(&workspace_id, generation, true);
    let mut sources = read_work_sources(&store.db).expect("record");
    sources
        .workspaces
        .get_mut(&workspace_id)
        .expect("pinned")
        .info
        .can_create = false;
    save_work_sources(&store.db, &sources).expect("saved");
    assert_eq!(can_create(), json!(false));
}

/// A create that can't start by its deadline, because its request waited for a worker past the
/// admission window, is refused and runs nothing.
#[test]
fn a_create_whose_request_waited_too_long_is_refused() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-ok");
    let project = project_in(&store, &workspace_id, json!({}));
    let arrived = Instant::now() - (CREATE_WRITER_WAIT + Duration::from_millis(100));
    assert_eq!(
        create_with(
            &WorkCredentials::new(&store.paths),
            &project,
            &workspace_id,
            "create-a",
            SHORT,
            arrived + CREATE_WRITER_WAIT,
        )
        .map(|_| ()),
        Err(
            "Fixture is busy answering another request; nothing was created. Try again in a moment."
                .to_string()
        )
    );
    assert!(calls(&store, "create-a").is_empty());
}

/// The store's writer lock is held from the plan check until the command starts: a team join
/// waits for the spawn. The ticket the command then makes is answered, the setup having moved.
#[test]
fn a_team_join_waits_for_a_creates_command_to_start() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-ok");
    let project = project_in(&store, &workspace_id, json!({}));
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let answer = create_in_background(&store, &project, &workspace_id, "create-a");
    paused.recv_timeout(HANDSHAKE).expect("about to spawn");
    let (joined, join_done) = mpsc::channel();
    {
        let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            let _ = joined.send(change_work_source_config(&workspace_id, || {
                mutate_credentials(&paths, |credentials| {
                    credentials.insert(
                        "teamSync".to_string(),
                        json!({ workspace_id.as_str(): { "deploymentUrl": "https://team.example", "memberToken": "member" } }),
                    );
                })
            }));
        });
    }
    assert!(
        join_done.recv_timeout(Duration::from_millis(300)).is_err(),
        "the join waits for the spawn"
    );
    resume.send(()).expect("resume");
    join_done
        .recv_timeout(HANDSHAKE)
        .expect("joined")
        .expect("changed")
        .expect("saved");
    assert_eq!(
        created(answer.recv_timeout(HANDSHAKE).expect("answered")).id,
        "ACM-41"
    );
    assert_eq!(calls(&store, "create-a"), ["create"]);
}

/// A credentials file the create can't read is refused in the store's own words, not as a
/// changed setup, and runs nothing.
#[test]
fn a_create_that_cant_read_the_stored_values_says_so() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-ok");
    let project = project_in(&store, &workspace_id, json!({}));
    // Read before the file goes, as a request reads it once.
    let credentials = WorkCredentials::new(&store.paths);
    assert!(credentials
        .source_fields(&workspace_id, "create-a")
        .is_some());
    let file = store.paths.auth_dir.join("work-mode-credentials.json");
    std::fs::remove_file(&file).expect("remove");
    std::fs::create_dir(&file).expect("folder");
    let refused = create_with(
        &credentials,
        &project,
        &workspace_id,
        "create-a",
        SHORT,
        Instant::now() + CREATE_WRITER_WAIT,
    )
    .map(|_| ())
    .expect_err("refused");
    assert!(
        refused.starts_with("Ghostex couldn't read the credentials already stored")
            && refused.ends_with("Nothing was created."),
        "{refused}"
    );
    assert!(calls(&store, "create-a").is_empty());
}

/// A create whose preparation runs past its deadline doesn't start its command: the deadline is
/// checked again right before the spawn, which a create that had time on entry can still miss.
#[test]
fn a_create_past_its_deadline_at_the_spawn_runs_nothing() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "create-a", "create-ok");
    let project = project_in(&store, &workspace_id, json!({}));
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let admit_by = Instant::now() + Duration::from_millis(500);
    let answer = {
        let (paths, project, workspace_id) =
            (store.paths.clone(), project.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            create_with(
                &WorkCredentials::new(&paths),
                &project,
                &workspace_id,
                "create-a",
                SHORT,
                admit_by,
            )
            .map(|_| ())
        })
    };
    paused.recv_timeout(HANDSHAKE).expect("prepared in time");
    std::thread::sleep(
        admit_by.saturating_duration_since(Instant::now()) + Duration::from_millis(50),
    );
    resume.send(()).expect("resume");
    assert_eq!(
        answer.join().expect("answered"),
        Err(
            "Fixture is busy answering another request; nothing was created. Try again in a moment."
                .to_string()
        )
    );
    assert!(calls(&store, "create-a").is_empty());
    // The slot and the writer lock were let go.
    assert!(hold_work_source_writer_within(Duration::from_millis(100)).is_some());
}
