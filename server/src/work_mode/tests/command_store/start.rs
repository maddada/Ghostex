//! Start work on a ticket command's ticket: the branch it cuts, from one read of the ticket at the
//! workspace's generation now (a fresh card read, else `detail` in a process slot free now), and
//! every way it refuses. The tests share the ids `start-a` and `start-b` (and `start-number` for
//! the number format): the process interns at most 256 ids, and every cache, hook and flight is
//! keyed by the workspace, which each test makes its own.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::cards::{
    calls, exact_read, pin, plan_of, project_id, project_in, store_values, token_of, HANDSHAKE,
};
use super::details::{open, wait_for_calls, Released};
use super::published::{command_source, record_pinning};
use super::*;

const NEEDS_A_READ: &str = "Ghostex needs to read the ticket to start work on it.";

/// Start's branch for `typed` (the command reads it as `ACM-12`), with the project's own scope.
fn start_typed(
    store: &Store,
    project: &Value,
    workspace_id: &str,
    id: &str,
    typed: &str,
    canonical: &str,
) -> Result<String, String> {
    let credentials = WorkCredentials::new(&store.paths);
    let WorkSourceId::Command(source) = command_source(id) else {
        unreachable!("a command id")
    };
    source.source().start_branch(
        &TicketScope {
            project_id: project_id(project),
            effective_workspace_id: Some(workspace_id),
            credentials: Some(&credentials),
            ..TicketScope::default()
        },
        typed,
        canonical,
    )
}

fn start(store: &Store, project: &Value, workspace_id: &str, id: &str) -> Result<String, String> {
    start_typed(store, project, workspace_id, id, "ACM-12", "ACM-12")
}

/// Waits until the background reads of `ACM-1` to `ACM-3` a test queued for `id` in
/// `workspace_id` have ended: one still polling for its release in a test's deleted folder would
/// hold its worker until its time limit, and the next tests' background reads would wait for it.
pub(super) fn wait_for_background_reads(workspace_id: &str, id: &str) {
    let plan = plan_of(workspace_id, id);
    let deadline = Instant::now() + HANDSHAKE;
    while ["ACM-1", "ACM-2", "ACM-3"]
        .iter()
        .any(|ticket| cached_command_ticket(&plan, ticket).is_none())
    {
        assert!(
            Instant::now() < deadline,
            "the background reads never ended"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The tickets a branch names back to the command, as a session's card reads them.
fn named_by(workspace_id: &str, id: &str, branch: &str) -> Vec<String> {
    work_source(command_source(id)).tickets_in_branch(
        &TicketScope {
            effective_workspace_id: Some(workspace_id),
            ..TicketScope::default()
        },
        branch,
    )
}

/// The ticket's own branch, read once: a second Start finds the fresh read and asks nothing.
#[test]
fn start_uses_the_tickets_own_branch_from_one_read() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "start-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    assert_eq!(
        start(&store, &project, &workspace_id, "start-a"),
        Ok("leandro/acm-12-spec".to_string())
    );
    assert_eq!(
        start(&store, &project, &workspace_id, "start-a"),
        Ok("leandro/acm-12-spec".to_string())
    );
    assert_eq!(calls(&store, "start-a"), ["detail"]);
}

/// A ticket with no branch of its own gets `<id>-<slug of its title>`, or its id alone, with no
/// user prefix, and the card reads the ticket back from it, for both id formats.
#[test]
fn a_ticket_without_a_branch_gets_one_that_names_it() {
    let _slots = holding_slots();
    let store = Store::new();
    let id = "start-a";
    for (mode, branch) in [
        ("start-fallback", "acm-12-fix-the-login"),
        ("start-untitled", "acm-12"),
    ] {
        let workspace_id = pin(&store, &format!("ws-{mode}"), id, mode);
        let project = project_in(&store, &workspace_id, json!({}));
        assert_eq!(
            start(&store, &project, &workspace_id, id),
            Ok(branch.to_string())
        );
        assert_eq!(named_by(&workspace_id, id, branch), ["ACM-12"], "{branch}");
    }
    let sources = record_pinning(&store, "ws-number", "start-number", IdFormat::Number);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws("ws-number");
    store_values(
        &store.paths,
        &workspace_id,
        "start-number",
        &token_of("start-number"),
        "start-number",
    );
    let project = project_in(&store, &workspace_id, json!({}));
    let branch = start_typed(
        &store,
        &project,
        &workspace_id,
        "start-number",
        "218",
        "218",
    );
    assert_eq!(branch, Ok("218-arabic-plan-cards".to_string()));
    assert_eq!(
        named_by(&workspace_id, "start-number", "218-arabic-plan-cards"),
        ["218"]
    );
}

/// A branch name holding the token is dropped where the answer is read, and the title's slug is
/// used.
#[test]
fn a_branch_holding_a_secret_is_never_used() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "start-a", "start-token-branch");
    let project = project_in(&store, &workspace_id, json!({}));
    let branch = start(&store, &project, &workspace_id, "start-a").expect("branch");
    assert_eq!(branch, "acm-12-fix-the-login");
    assert!(!branch.contains(&token_of("start-a")));
}

/// Every value the workspace stores, secret or not, kept ones included: an id holding one, as
/// typed or as the command reads it, is refused before anything runs; a branch name, and then the
/// title's slug, holding one gives way to the id.
#[test]
fn a_start_never_uses_a_stored_value() {
    let _slots = holding_slots();
    let store = Store::new();
    let id = "start-a";
    for (mode, typed) in [
        // The typed id holds the not-secret `mode`, and the id as read doesn't.
        ("acm", "acm-12"),
        // The id as read holds it, and the typed one doesn't.
        ("ACM", "acm-12"),
    ] {
        let workspace_id = pin(&store, &format!("ws-{mode}"), id, mode);
        let project = project_in(&store, &workspace_id, json!({}));
        assert_eq!(
            start_typed(&store, &project, &workspace_id, id, typed, "ACM-12"),
            Err(START_ID_SPELLS_CREDENTIAL.to_string()),
            "{mode}"
        );
        assert!(calls(&store, id).is_empty(), "{mode}");
    }
    // A value kept for a command the workspace no longer pins counts too.
    let workspace_id = pin(&store, "ws-kept", "start-a", "good");
    mutate_credentials(&store.paths, |credentials| {
        credentials["workspaceSourceCredentials"][workspace_id.as_str()]["start-elsewhere"] =
            json!({ "org": "acm" });
    })
    .expect("kept");
    let project = project_in(&store, &workspace_id, json!({}));
    assert_eq!(
        start_typed(
            &store,
            &project,
            &workspace_id,
            "start-a",
            "acm-12",
            "ACM-12"
        ),
        Err(START_ID_SPELLS_CREDENTIAL.to_string())
    );
    assert!(calls(&store, "start-a").is_empty());
    // `start-stored` is the mode's own value: its branch name holds it, and so would
    // `acm-12-fix-start-stored-login`.
    let workspace_id = pin(&store, "ws-stored", "start-a", "start-stored");
    let project = project_in(&store, &workspace_id, json!({}));
    assert_eq!(
        start(&store, &project, &workspace_id, "start-a"),
        Ok("acm-12".to_string())
    );
}

/// Start runs nothing when every process slot is taken: the background workers hold theirs with
/// reads that wait, and a details page the one they leave free.
#[test]
fn a_start_with_no_free_slot_is_refused_at_once() {
    let _slots = holding_slots();
    let store = Store::new();
    let busy = pin(&store, "ws-busy", "start-a", "wait-release");
    let _released = Released(&store, "start-a");
    for ticket in ["ACM-1", "ACM-2", "ACM-3"] {
        assert_eq!(
            queue_exact_read(exact_read(&store, &busy, "start-a", &[ticket])),
            QueueOutcome::Queued
        );
    }
    wait_for_calls(&store, "start-a", "list", BACKGROUND_WORKERS);
    let opening = {
        let (paths, project) = (store.paths.clone(), project_in(&store, &busy, json!({})));
        let workspace_id = busy.clone();
        std::thread::spawn(move || open(&paths, &project, &workspace_id, "start-a"))
    };
    wait_for_calls(&store, "start-a", "detail", 1);
    let workspace_id = pin(&store, "ws-a", "start-b", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let started = Instant::now();
    assert_eq!(
        start(&store, &project, &workspace_id, "start-b"),
        Err(format!(
            "Fixture is busy answering another request; try again in a moment. {NEEDS_A_READ}"
        ))
    );
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(calls(&store, "start-b").is_empty());
    drop(_released);
    let _ = opening.join();
    wait_for_background_reads(&busy, "start-a");
}

/// Each reason the command can't be read refuses Start in words that say why, through the
/// workspace's mask.
#[test]
fn each_reason_the_ticket_cant_be_read_refuses_start() {
    let _slots = holding_slots();
    let store = Store::new();
    let id = "start-a";
    let refusal = |case: &str, prepare: &dyn Fn(&str)| {
        let workspace_id = pin(&store, &format!("ws-{case}"), id, "good");
        let project = project_in(&store, &workspace_id, json!({}));
        prepare(&workspace_id);
        let refused = start(&store, &project, &workspace_id, id).expect_err(case);
        assert!(refused.ends_with(NEEDS_A_READ), "{refused}");
        assert!(!refused.contains(&token_of(id)), "{refused}");
        refused
    };
    let cleared = refusal("cleared", &|workspace_id| {
        clear_work_source(&store.db, workspace_id, &no_references).expect("cleared");
    });
    assert_eq!(
        cleared,
        format!("Fixture isn't this workspace's ticket command. {NEEDS_A_READ}")
    );
    let team = refusal("team", &|workspace_id| join_team(&store, workspace_id));
    assert_eq!(team, format!("{TEAM_REFUSAL} {NEEDS_A_READ}"));
    let moved = store.program.with_extension("moved");
    let gone = refusal("gone", &|_| {
        std::fs::rename(&store.program, &moved).expect("moved away");
    });
    std::fs::rename(&moved, &store.program).expect("moved back");
    assert!(gone.contains("its program is gone or can't run"), "{gone}");
    let unset = refusal("unset", &|workspace_id| {
        store_values(&store.paths, workspace_id, "start-a", "", "good");
    });
    assert!(
        unset.starts_with("Set Fixture's API token first"),
        "{unset}"
    );
    let failed = refusal("failed", &|workspace_id| {
        let token = token_of("start-a");
        store_values(&store.paths, workspace_id, "start-a", &token, "error-json");
    });
    assert!(failed.starts_with("Fixture: The token "), "{failed}");
    let missing = refusal("missing", &|workspace_id| {
        let token = token_of("start-a");
        store_values(
            &store.paths,
            workspace_id,
            "start-a",
            &token,
            "start-not-found",
        );
    });
    assert_eq!(missing, format!("Fixture: no such ticket. {NEEDS_A_READ}"));
}

/// A person's Start asks again even while the details page keeps a failure, and its answer
/// replaces that failure.
#[test]
fn a_kept_failure_never_stops_a_start() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "start-a", "start-flaky");
    let project = project_in(&store, &workspace_id, json!({}));
    assert!(open(&store.paths, &project, &workspace_id, "start-a").is_err());
    let dir = command_work_dir(&store.paths, Some("start-a")).expect("work dir");
    std::fs::write(dir.join("recovered"), "").expect("recovered");
    assert!(
        open(&store.paths, &project, &workspace_id, "start-a").is_err(),
        "the details page keeps its failure"
    );
    assert_eq!(
        start(&store, &project, &workspace_id, "start-a"),
        Ok("leandro/acm-12-spec".to_string())
    );
    assert!(open(&store.paths, &project, &workspace_id, "start-a").is_ok());
    assert_eq!(calls(&store, "start-a"), ["detail", "detail"]);
}

/// A team join waits while Start's `detail` is about to start, holding the store's writer lock;
/// the read then answers after the join, so Start keeps nothing of it and refuses.
#[test]
fn a_team_join_waits_for_starts_detail_to_spawn() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "start-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let starting = {
        let (paths, project, workspace_id) =
            (store.paths.clone(), project.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            let credentials = WorkCredentials::new(&paths);
            let WorkSourceId::Command(source) = command_source("start-a") else {
                unreachable!("a command id")
            };
            source.source().start_branch(
                &TicketScope {
                    project_id: project_id(&project),
                    effective_workspace_id: Some(&workspace_id),
                    credentials: Some(&credentials),
                    ..TicketScope::default()
                },
                "ACM-12",
                "ACM-12",
            )
        })
    };
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
        starting.join().expect("started"),
        Err(format!("{DETAIL_CHANGED_MEANWHILE} {NEEDS_A_READ}"))
    );
    assert!(cached_command_ticket(&plan_of(&workspace_id, "start-a"), "ACM-12").is_none());
}

/// Start on a ticket with a fresh card read, on a thread of its own; the receiver answers with
/// the branch.
fn start_in_background(
    store: &Store,
    project: &Value,
    workspace_id: &str,
    id: &str,
) -> mpsc::Receiver<Result<String, String>> {
    let (answered, answer) = mpsc::channel();
    let (paths, project, workspace_id, id) = (
        store.paths.clone(),
        project.clone(),
        workspace_id.to_string(),
        id.to_string(),
    );
    std::thread::spawn(move || {
        let credentials = WorkCredentials::new(&paths);
        let WorkSourceId::Command(source) = command_source(&id) else {
            unreachable!("a command id")
        };
        let _ = answered.send(source.source().start_branch(
            &TicketScope {
                project_id: project_id(&project),
                effective_workspace_id: Some(&workspace_id),
                credentials: Some(&credentials),
                ..TicketScope::default()
            },
            "ACM-12",
            "ACM-12",
        ));
    });
    answer
}

/// A ticket the cards read a moment ago is used as is, but only while the setup it was read with
/// still holds when the branch is picked: a credential change in between refuses Start.
#[test]
fn a_change_after_a_fresh_read_refuses_start() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "start-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    assert_eq!(
        start(&store, &project, &workspace_id, "start-a"),
        Ok("leandro/acm-12-spec".to_string())
    );
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let answer = start_in_background(&store, &project, &workspace_id, "start-a");
    paused.recv_timeout(HANDSHAKE).expect("read");
    // `spec` is in the branch the read names.
    change_work_source_config(&workspace_id, || {
        store_values(
            &store.paths,
            &workspace_id,
            "start-a",
            &token_of("start-a"),
            "spec",
        );
        Ok::<_, crate::domain::DomainStateError>(())
    })
    .expect("changed")
    .expect("stored");
    resume.send(()).expect("resume");
    assert_eq!(
        answer.recv_timeout(HANDSHAKE).expect("answered"),
        Err(format!("{DETAIL_CHANGED_MEANWHILE} {NEEDS_A_READ}"))
    );
    assert_eq!(calls(&store, "start-a"), ["detail"]);
}

/// Starts that join a read of the same ticket already running hold no process slot while they
/// wait, so a Start on another ticket still finds one.
#[test]
fn starts_joining_a_running_read_hold_no_slot() {
    let _slots = holding_slots();
    let store = Store::new();
    let workspace_id = pin(&store, "ws-held", "start-a", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let released = Released(&store, "start-a");
    let opening = {
        let (paths, project, workspace_id) =
            (store.paths.clone(), project.clone(), workspace_id.clone());
        std::thread::spawn(move || open(&paths, &project, &workspace_id, "start-a"))
    };
    wait_for_calls(&store, "start-a", "detail", 1);
    let joining: Vec<_> = (0..3)
        .map(|_| start_in_background(&store, &project, &workspace_id, "start-a"))
        .collect();
    let key = DetailRead {
        plan: plan_of(&workspace_id, "start-a"),
        id: "ACM-12".to_string(),
        project: [project_id(&project).to_string()],
        paths: store.paths.clone(),
    }
    .cache_key();
    let deadline = Instant::now() + HANDSHAKE;
    while single_flight_waiting(&key) < 3 {
        assert!(Instant::now() < deadline, "the Starts never joined");
        std::thread::sleep(Duration::from_millis(10));
    }
    let other = pin(&store, "ws-free", "start-b", "good");
    let other_project = project_in(&store, &other, json!({}));
    assert_eq!(
        start(&store, &other_project, &other, "start-b"),
        Ok("leandro/acm-12-spec".to_string())
    );
    drop(released);
    for answer in joining {
        assert_eq!(
            answer.recv_timeout(HANDSHAKE).expect("answered"),
            Ok("leandro/acm-12-spec".to_string())
        );
    }
    opening.join().expect("opened").expect("details");
    assert_eq!(calls(&store, "start-a"), ["detail"]);
}

/// A Start that finds no read to join and no slot free answers "busy" without starting a shared
/// read, so details reads coming meanwhile never get that answer: they share one call that waits
/// for a slot, and show the ticket once one frees.
#[test]
fn a_starts_busy_answer_never_reaches_a_details_read() {
    let _slots = holding_slots();
    let store = Store::new();
    let busy = pin(&store, "ws-busy", "start-b", "wait-release");
    let released = Released(&store, "start-b");
    for ticket in ["ACM-1", "ACM-2", "ACM-3"] {
        assert_eq!(
            queue_exact_read(exact_read(&store, &busy, "start-b", &[ticket])),
            QueueOutcome::Queued
        );
    }
    wait_for_calls(&store, "start-b", "list", BACKGROUND_WORKERS);
    let holding = {
        let (paths, project, workspace_id) = (
            store.paths.clone(),
            project_in(&store, &busy, json!({})),
            busy.clone(),
        );
        std::thread::spawn(move || open(&paths, &project, &workspace_id, "start-b"))
    };
    wait_for_calls(&store, "start-b", "detail", 1);
    let workspace_id = pin(&store, "ws-a", "start-a", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let starting = start_in_background(&store, &project, &workspace_id, "start-a");
    paused
        .recv_timeout(HANDSHAKE)
        .expect("no read to join and no slot free");
    let opening: Vec<_> = (0..2)
        .map(|_| {
            let (paths, project, workspace_id) =
                (store.paths.clone(), project.clone(), workspace_id.clone());
            std::thread::spawn(move || open(&paths, &project, &workspace_id, "start-a"))
        })
        .collect();
    let key = DetailRead {
        plan: plan_of(&workspace_id, "start-a"),
        id: "ACM-12".to_string(),
        project: [project_id(&project).to_string()],
        paths: store.paths.clone(),
    }
    .cache_key();
    let deadline = Instant::now() + HANDSHAKE;
    while single_flight_waiting(&key) < 1 {
        assert!(
            Instant::now() < deadline,
            "the details reads never shared a call"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    resume.send(()).expect("resume");
    assert_eq!(
        starting.recv_timeout(HANDSHAKE).expect("answered"),
        Err(format!(
            "Fixture is busy answering another request; try again in a moment. {NEEDS_A_READ}"
        ))
    );
    drop(released);
    for reader in opening {
        let details = reader.join().expect("opened").expect("details");
        assert_eq!(details["title"], json!("Spec the contract"));
    }
    assert!(kept_detail(&key).is_some());
    assert_eq!(calls(&store, "start-a"), ["detail"]);
    holding.join().expect("held").expect("details");
    wait_for_background_reads(&busy, "start-b");
}
