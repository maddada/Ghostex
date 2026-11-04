//! A ticket command's ticket details, which someone waits for: asked of the workspace's command on
//! the caller's thread in the process slot the background leaves free, with the same checks as a
//! background read, once for every caller asking at the same time, and kept by the details cache
//! at the workspace's generation. Each test pins ids of its own.

use std::sync::mpsc;
use std::time::Instant;

use super::cards::{
    calls, card_read, exact_read, log_lines, pin, plan_of, project_id, project_in, release,
    store_values, token_of, HANDSHAKE,
};
use super::published::command_source;
use super::*;

/// Opens `ticket`'s details as the details page does, with the project's own scope.
fn open(
    paths: &GxserverPaths,
    project: &Value,
    workspace_id: &str,
    id: &str,
) -> Result<Value, String> {
    let credentials = WorkCredentials::new(paths);
    work_source(command_source(id)).read_detail(
        &TicketScope {
            project_id: project_id(project),
            effective_workspace_id: Some(workspace_id),
            credentials: Some(&credentials),
            ..TicketScope::default()
        },
        "ACM-12",
        false,
    )
}

/// Opens the details on a thread of its own; the receiver answers with what it read.
fn open_in_background(
    store: &Store,
    project: &Value,
    workspace_id: &str,
    id: &str,
) -> mpsc::Receiver<Result<Value, String>> {
    let (answered, answer) = mpsc::channel();
    let (paths, project, workspace_id, id) = (
        store.paths.clone(),
        project.clone(),
        workspace_id.to_string(),
        id.to_string(),
    );
    std::thread::spawn(move || {
        let _ = answered.send(open(&paths, &project, &workspace_id, &id));
    });
    answer
}

/// Lets `id`'s `wait-release` fixture answer when dropped, so a test that fails early never leaves
/// a call holding a process slot.
struct Released<'s>(&'s Store, &'static str);

impl Drop for Released<'_> {
    fn drop(&mut self) {
        release(self.0, self.1);
    }
}

/// Waits until `id`'s fixture has started `count` calls of `verb`.
fn wait_for_calls(store: &Store, id: &str, verb: &str, count: usize) {
    let deadline = Instant::now() + HANDSHAKE;
    while calls(store, id).iter().filter(|call| *call == verb).count() < count {
        assert!(Instant::now() < deadline, "{:?}", calls(store, id));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn a_ticket_opens_with_its_details_and_refreshes_its_card() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "detail-open", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let ticket = open(&store.paths, &project, &workspace_id, "detail-open").expect("details");
    assert_eq!(ticket["source"], json!("detail-open"));
    assert_eq!(ticket["id"], json!("ACM-12"));
    assert_eq!(ticket["title"], json!("Spec the contract"));
    assert_eq!(ticket["description"], json!("Body"));
    assert_eq!(ticket["state"]["group"], json!("progress"));
    assert_eq!(ticket["commentCount"], json!(1));
    assert_eq!(ticket["canStart"], json!(false));
    assert!(ticket.get("secretField").is_none(), "{ticket}");
    assert!(matches!(
        card_read(&project, "detail-open", "ACM-12"),
        TicketRead::Found(_)
    ));
    assert_eq!(
        last_ticket_title(&workspace_id, "detail-open", "ACM-12").as_deref(),
        Some("Spec the contract")
    );
    assert_eq!(calls(&store, "detail-open"), ["detail"]);
}

#[test]
fn a_second_reader_waits_for_the_first_ones_answer() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "detail-flight", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let released = Released(&store, "detail-flight");
    let first = open_in_background(&store, &project, &workspace_id, "detail-flight");
    wait_for_calls(&store, "detail-flight", "detail", 1);
    let second = open_in_background(&store, &project, &workspace_id, "detail-flight");
    let key = format!(
        "cmd:detail-flight:{workspace_id}:{}:ACM-12",
        work_source_generation(&workspace_id)
    );
    let deadline = Instant::now() + HANDSHAKE;
    while single_flight_waiting(&key) < 1 {
        assert!(Instant::now() < deadline, "the second reader never waited");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    drop(released);
    let first = first.recv_timeout(HANDSHAKE).expect("first").expect("read");
    let second = second
        .recv_timeout(HANDSHAKE)
        .expect("second")
        .expect("read");
    assert_eq!(first, second);
    assert_eq!(calls(&store, "detail-flight"), ["detail"]);
}

#[test]
fn the_details_are_kept_until_the_setup_changes() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "detail-cache", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    open(&store.paths, &project, &workspace_id, "detail-cache").expect("details");
    open(&store.paths, &project, &workspace_id, "detail-cache").expect("details");
    assert_eq!(calls(&store, "detail-cache"), ["detail"]);
    change_work_source_config(&workspace_id, || {
        store_values(
            &store.paths,
            &workspace_id,
            "detail-cache",
            "tok-detail-cache-new",
            "good",
        )
    })
    .expect("changed");
    open(&store.paths, &project, &workspace_id, "detail-cache").expect("details");
    assert_eq!(calls(&store, "detail-cache"), ["detail", "detail"]);
}

/// The background workers hold every slot they may take, each with a read that waits; the details
/// still run, in the slot they leave free.
#[test]
fn a_detail_runs_while_the_background_holds_its_slots() {
    let store = Store::new();
    let busy = pin(&store, "ws-busy", "detail-busy", "wait-release");
    let released = Released(&store, "detail-busy");
    for ticket in ["ACM-1", "ACM-2", "ACM-3"] {
        assert_eq!(
            queue_exact_read(exact_read(&store, &busy, "detail-busy", &[ticket])),
            QueueOutcome::Queued
        );
    }
    wait_for_calls(&store, "detail-busy", "list", BACKGROUND_WORKERS);
    let workspace_id = pin(&store, "ws-a", "detail-slot", "good");
    let project = project_in(&store, &workspace_id, json!({}));
    let answer = open_in_background(&store, &project, &workspace_id, "detail-slot");
    let answer = answer.recv_timeout(HANDSHAKE);
    drop(released);
    // The background reads end before the test's folder goes: one still polling for its release
    // in a deleted folder would hold its slot until its time limit.
    let plan = plan_of(&busy, "detail-busy");
    let deadline = Instant::now() + HANDSHAKE;
    while ["ACM-1", "ACM-2", "ACM-3"]
        .iter()
        .any(|ticket| cached_command_ticket(&plan, ticket).is_none())
    {
        assert!(
            Instant::now() < deadline,
            "the background reads never ended"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    answer.expect("the details ran").expect("read");
}

/// The setup changes while the command answers: the answer is dropped, and nothing it read is
/// kept.
#[test]
fn details_read_before_a_change_keep_nothing() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "detail-changed", "wait-release");
    let project = project_in(&store, &workspace_id, json!({}));
    let released = Released(&store, "detail-changed");
    let answer = open_in_background(&store, &project, &workspace_id, "detail-changed");
    wait_for_calls(&store, "detail-changed", "detail", 1);
    change_work_source_config(&workspace_id, || {
        store_values(
            &store.paths,
            &workspace_id,
            "detail-changed",
            "tok-detail-changed-new",
            "wait-release",
        )
    })
    .expect("changed");
    drop(released);
    assert_eq!(
        answer.recv_timeout(HANDSHAKE).expect("answered"),
        Err(DETAIL_CHANGED_MEANWHILE.to_string())
    );
    assert_eq!(
        last_ticket_title(&workspace_id, "detail-changed", "ACM-12"),
        None
    );
    assert!(matches!(
        card_read(&project, "detail-changed", "ACM-12"),
        TicketRead::Unread
    ));
}

#[test]
fn a_failed_detail_is_logged_once_without_the_commands_words() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "detail-failed", "error-token");
    let project = project_in(&store, &workspace_id, json!({}));
    let refused = open(&store.paths, &project, &workspace_id, "detail-failed").expect_err("failed");
    let token = token_of("detail-failed");
    assert!(!refused.contains(&token), "{refused}");
    let lines = log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(r#""verb":"detail""#), "{lines:?}");
    assert!(!lines[0].contains(&token), "{lines:?}");
    assert!(!lines[0].contains("rejected"), "{lines:?}");
}

#[test]
fn a_token_the_details_echo_is_masked() {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", "detail-masked", "detail-token");
    let project = project_in(&store, &workspace_id, json!({}));
    let ticket = open(&store.paths, &project, &workspace_id, "detail-masked").expect("details");
    let token = token_of("detail-masked");
    assert!(!ticket.to_string().contains(&token), "{ticket}");
    assert!(ticket["description"]
        .as_str()
        .is_some_and(|description| description.starts_with("Use ")));
}

/// What two projects of one workspace read of the same ticket when the second joins the first one's
/// call, through `id`'s fixture answering in `mode`, and then moves to another workspace: the
/// first one's answer, the second one's, and the second one's again from the cache.
fn two_readers_and_the_second_moves(
    id: &'static str,
    mode: &str,
) -> (
    Result<Value, String>,
    Result<Value, String>,
    Result<Value, String>,
) {
    let store = Store::new();
    let workspace_id = pin(&store, "ws-a", id, mode);
    let first_project = project_in(&store, &workspace_id, json!({}));
    let second_path = store
        .paths
        .app_data_dir
        .join(format!("project-{id}-second"));
    std::fs::create_dir_all(&second_path).expect("project folder");
    let second_project = project_in(&store, &workspace_id, json!({ "path": second_path }));
    let other = store.ws("ws-b");
    let released = Released(&store, id);
    let first = open_in_background(&store, &first_project, &workspace_id, id);
    wait_for_calls(&store, id, "detail", 1);
    let second = open_in_background(&store, &second_project, &workspace_id, id);
    let key = format!(
        "cmd:{id}:{workspace_id}:{}:ACM-12",
        work_source_generation(&workspace_id)
    );
    let deadline = Instant::now() + HANDSHAKE;
    while single_flight_waiting(&key) < 1 {
        assert!(Instant::now() < deadline, "the second reader never waited");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    crate::domain::DomainRepository::new(&store.db, "test")
        .update_project(
            json!({ "projectId": project_id(&second_project), "launchSettings": { "workspaceId": other } })
                .as_object()
                .unwrap(),
        )
        .expect("moved");
    drop(released);
    let first = first.recv_timeout(HANDSHAKE).expect("first");
    let second = second.recv_timeout(HANDSHAKE).expect("second");
    let again = open(&store.paths, &second_project, &workspace_id, id);
    assert_eq!(calls(&store, id), ["detail"]);
    (first, second, again)
}

/// The second project gets no ticket of the workspace it left, from the shared call or from the
/// cache afterwards, while the first does.
#[test]
fn a_reader_whose_project_left_the_workspace_gets_no_ticket() {
    let (first, second, again) = two_readers_and_the_second_moves("detail-moved", "wait-release");
    first.expect("the first project is still there");
    assert_eq!(second, Err(DETAIL_CHANGED_MEANWHILE.to_string()));
    assert_eq!(again, Err(DETAIL_CHANGED_MEANWHILE.to_string()));
}

/// The same for a failure: the second project doesn't get the command's words about the workspace
/// it left, from the shared call or from the cache.
#[test]
fn a_reader_whose_project_left_the_workspace_gets_no_failure_of_it() {
    let (first, second, again) =
        two_readers_and_the_second_moves("detail-moved-failed", "fail-after-release");
    let failure = first.expect_err("the command failed");
    assert_ne!(failure, DETAIL_CHANGED_MEANWHILE);
    assert_eq!(second, Err(DETAIL_CHANGED_MEANWHILE.to_string()));
    assert_eq!(again, Err(DETAIL_CHANGED_MEANWHILE.to_string()));
}
