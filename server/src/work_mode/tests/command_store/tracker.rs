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
