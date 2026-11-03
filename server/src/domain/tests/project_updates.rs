//! `update_project` against concurrent writers: an update that read the row before a move never
//! writes the old workspace back, and a placement change inside a transaction needs the placement
//! lock taken before the transaction.

use std::sync::mpsc;
use std::time::Duration;

use crate::domain::repository::project::{
    hold_project_placement, pause_project_update, PLACEMENT_CHANGE_IN_TRANSACTION,
    PROJECT_CHANGED_DURING_UPDATE,
};
use serde_json::json;

use super::*;

fn project_in(repository: &DomainRepository<'_>, workspace_id: &str) -> String {
    let project = repository
        .create_project(
            json!({ "name": "App", "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    value_str(&project, "projectId").to_string()
}

fn update(
    repository: &DomainRepository<'_>,
    params: serde_json::Value,
) -> DomainResult<serde_json::Value> {
    repository.update_project(params.as_object().expect("update params"))
}

fn move_to(repository: &DomainRepository<'_>, project_id: &str, workspace_id: &str) {
    update(
        repository,
        json!({ "projectId": project_id, "launchSettings": { "workspaceId": workspace_id } }),
    )
    .expect("moved");
}

/// Renames the project on another thread, which pauses after it read the row (`pause_project_update`).
fn rename_paused(
    temp: &tempfile::TempDir,
    project_id: &str,
) -> (
    std::thread::JoinHandle<DomainResult<serde_json::Value>>,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    pause_project_update(project_id, reached, release);
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let project_id = project_id.to_string();
    let renaming = std::thread::spawn(move || {
        let db = crate::storage::open_gxserver_database(&paths).expect("db");
        let repository = DomainRepository::new(&db, "S7k");
        update(
            &repository,
            json!({ "projectId": project_id, "name": "Renamed" }),
        )
    });
    (renaming, paused, resume)
}

#[test]
fn an_update_that_read_the_row_before_a_move_keeps_the_move_and_its_own_change() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project_id = project_in(&repository, "ws-a");
    let (renaming, paused, resume) = rename_paused(&temp, &project_id);
    paused
        .recv_timeout(Duration::from_secs(20))
        .expect("the rename read the row");
    move_to(&repository, &project_id, "ws-b");
    resume.send(()).expect("resume");
    let renamed = renaming.join().expect("rename thread").expect("renamed");
    let stored = repository
        .get_project(&project_id)
        .expect("read")
        .expect("project");
    assert_eq!(stored["launchSettings"]["workspaceId"], json!("ws-b"));
    assert_eq!(stored["name"], json!("Renamed"));
    assert_eq!(renamed["launchSettings"]["workspaceId"], json!("ws-b"));
}

/// Renames the project while, at each of its 3 attempts, this thread moves the project between
/// the rename's read and its write; with `remove_last`, the third change removes it instead. The
/// rename's answer and the row it leaves.
fn rename_raced_on_every_attempt(
    remove_last: bool,
) -> (DomainResult<serde_json::Value>, Option<serde_json::Value>) {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project_id = project_in(&repository, "ws-0");
    let (renaming, mut paused, mut resume) = rename_paused(&temp, &project_id);
    for attempt in 1..=3 {
        paused
            .recv_timeout(Duration::from_secs(20))
            .expect("the rename read the row");
        if remove_last && attempt == 3 {
            repository.remove_project(&project_id).expect("removed");
        } else {
            move_to(&repository, &project_id, &format!("ws-{attempt}"));
        }
        // Paused again on its next attempt, if it makes one (set after the change, which would take it).
        let (reached, next_paused) = mpsc::channel();
        let (next_resume, release) = mpsc::channel();
        pause_project_update(&project_id, reached, release);
        resume.send(()).expect("resume");
        (paused, resume) = (next_paused, next_resume);
    }
    let answer = renaming.join().expect("rename thread");
    (answer, repository.get_project(&project_id).expect("read"))
}

#[test]
fn an_update_the_project_keeps_changing_under_gives_up_in_fixed_words() {
    let (answer, stored) = rename_raced_on_every_attempt(false);
    let refused = answer.expect_err("gave up");
    assert_eq!(refused.code, "conflict");
    assert_eq!(refused.message, PROJECT_CHANGED_DURING_UPDATE);
    let stored = stored.expect("project");
    assert_eq!(stored["launchSettings"]["workspaceId"], json!("ws-3"));
    assert_eq!(stored["name"], json!("App"));
}

#[test]
fn an_update_whose_project_is_removed_during_its_last_attempt_answers_not_found() {
    let (answer, stored) = rename_raced_on_every_attempt(true);
    assert_eq!(answer.expect_err("not found").code, "notFound");
    assert!(stored.is_none());
}

#[test]
fn a_placement_change_inside_a_transaction_needs_the_placement_lock_first() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project_id = project_in(&repository, "ws-a");

    let transaction = db.unchecked_transaction().expect("transaction");
    let inside = DomainRepository::new(&transaction, "S7k");
    update(
        &inside,
        json!({ "projectId": project_id, "name": "Renamed" }),
    )
    .expect("an update that doesn't move it is written");
    let refused = update(
        &inside,
        json!({ "projectId": project_id, "launchSettings": { "workspaceId": "ws-b" } }),
    )
    .expect_err("refused");
    assert_eq!(refused.message, PLACEMENT_CHANGE_IN_TRANSACTION);
    drop(transaction);

    let placement = hold_project_placement();
    let transaction = db.unchecked_transaction().expect("transaction");
    move_to(
        &DomainRepository::new(&transaction, "S7k"),
        &project_id,
        "ws-b",
    );
    transaction.commit().expect("committed");
    drop(placement);
    let stored = repository
        .get_project(&project_id)
        .expect("read")
        .expect("project");
    assert_eq!(stored["launchSettings"]["workspaceId"], json!("ws-b"));
}
