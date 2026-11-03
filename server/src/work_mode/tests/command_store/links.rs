//! A session's hand-set tickets of a ticket command (`workLinks.tickets`), as
//! `with_checked_ticket_links` checks and writes them: what the workspace stores is read strictly,
//! no id may hold a stored value, and the check and the write run under the store's writer lock,
//! so no clear lands between them.

use std::sync::mpsc;
use std::time::Duration;

use crate::domain::DomainRepository;

use super::published::record_pinning;
use super::*;

/// A session row of a project in `workspace_id`.
fn session_in(store: &Store, workspace_id: &str) -> (String, String) {
    let repository = DomainRepository::new(&store.db, "test");
    let path = store
        .paths
        .app_data_dir
        .join(format!("links-{workspace_id}"));
    std::fs::create_dir_all(&path).expect("folder");
    let project = repository
        .create_project(
            json!({ "name": "App", "path": path, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = repository
        .create_session(
            json!({ "projectId": project_id, "kind": "agent", "agentId": "codex", "title": "S", "lifecycleState": "running" })
                .as_object()
                .unwrap(),
            false,
        )
        .expect("session");
    (
        project_id,
        session["sessionId"].as_str().unwrap().to_string(),
    )
}

/// Checks and writes `tickets` for the session, as `/api/setSessionWorkLinks` does.
fn link(
    db: &rusqlite::Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    (project_id, session_id): &(String, String),
    tickets: Value,
) -> Result<Map<String, Value>, crate::domain::DomainStateError> {
    let request = json!({ "tickets": tickets });
    with_checked_ticket_links(
        request.as_object().unwrap(),
        || Ok(workspace_id.to_string()),
        db,
        paths,
        |request| {
            let links = merge_work_links(None, request)?;
            write_work_links(db, project_id, session_id, &links)?;
            Ok(links)
        },
    )
}

fn pin_number_source(store: &Store, id: &str) -> String {
    let sources = record_pinning(store, "ws-a", id, IdFormat::Number);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws("ws-a");
    mutate_credentials(&store.paths, |credentials| {
        let workspaces = credentials
            .entry("workspaceSourceCredentials".to_string())
            .or_insert_with(|| json!({}));
        workspaces[&workspace_id][id] = json!({ "token": "123" });
    })
    .expect("stored");
    workspace_id
}

#[test]
fn a_ticket_id_holding_a_stored_value_is_never_linked() {
    let store = Store::new();
    let workspace_id = pin_number_source(&store, "links-num");
    let session = session_in(&store, &workspace_id);
    for typed in ["123", "#123", "41234"] {
        let refused = link(
            &store.db,
            &store.paths,
            &workspace_id,
            &session,
            json!({ "links-num": [typed] }),
        )
        .expect_err(typed);
        assert_eq!(refused.message, TICKET_LINK_SPELLS_CREDENTIAL, "{typed}");
    }
    let linked = link(
        &store.db,
        &store.paths,
        &workspace_id,
        &session,
        json!({ "links-num": ["#45"] }),
    )
    .expect("linked");
    assert_eq!(linked["tickets"], json!({ "links-num": ["45"] }));
}

#[test]
fn a_credentials_file_that_cant_be_read_links_nothing() {
    let store = Store::new();
    let workspace_id = pin_number_source(&store, "links-damaged");
    let session = session_in(&store, &workspace_id);
    std::fs::write(
        store.paths.auth_dir.join("work-mode-credentials.json"),
        "{ damaged",
    )
    .expect("damage");
    assert!(link(
        &store.db,
        &store.paths,
        &workspace_id,
        &session,
        json!({ "links-damaged": ["45"] }),
    )
    .is_err());
    let session_row = DomainRepository::new(&store.db, "test")
        .get_session(&session.0, &session.1)
        .expect("read")
        .expect("session");
    assert!(session_row["runtimeSettings"].get("workLinks").is_none());
}

/// A clear waits while a link is checked and written, so the link it checked for is still the
/// workspace's command when it is written, and the clear then keeps the linked id.
#[test]
fn a_clear_waits_for_a_link_being_checked_and_written() {
    let store = Store::new();
    let workspace_id = pin_number_source(&store, "links-locked");
    let session = session_in(&store, &workspace_id);
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let linking = {
        let (paths, workspace_id, session) =
            (store.paths.clone(), workspace_id.clone(), session.clone());
        std::thread::spawn(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            link(
                &db,
                &paths,
                &workspace_id,
                &session,
                json!({ "links-locked": ["45"] }),
            )
        })
    };
    paused
        .recv_timeout(Duration::from_secs(20))
        .expect("checked");
    assert!(work_source_writer_held());
    let (cleared, clear_done) = mpsc::channel();
    {
        let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            let references = || {
                let repository = DomainRepository::new(&db, "test");
                let workspaces = crate::workspaces::read_sidebar_workspaces(&db)?;
                referenced_source_ids(&repository, &workspaces)
            };
            let _ = cleared.send(
                crate::work_mode::clear_work_source(&db, &paths, &workspace_id, &references)
                    .result
                    .is_ok(),
            );
        });
    }
    // Checked, not written: the clear waits.
    assert!(clear_done.recv_timeout(Duration::from_millis(200)).is_err());
    resume.send(()).expect("resume");
    linking.join().expect("linked").expect("written");
    assert!(clear_done
        .recv_timeout(Duration::from_secs(20))
        .expect("the clear lands after the write"));
    // The link was written while the command was still pinned; the clear then retired its id.
    let sources = read_work_sources(&store.db).expect("record");
    assert!(!sources.workspaces.contains_key(&workspace_id));
    assert!(sources
        .retired
        .iter()
        .any(|retired| retired.id == "links-locked"));
}

/// A move of the session's project, written straight through the repository (no route's writer
/// lock), waits while a link is checked and written: the link is written while the project is still
/// in the workspace whose stored values it was checked against.
#[test]
fn a_project_move_waits_for_a_link_being_checked_and_written() {
    let store = Store::new();
    let workspace_id = pin_number_source(&store, "links-moved");
    let session = session_in(&store, &workspace_id);
    let other_workspace = store.ws("ws-b");
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let linking = {
        let (paths, workspace_id, session) =
            (store.paths.clone(), workspace_id.clone(), session.clone());
        std::thread::spawn(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            link(
                &db,
                &paths,
                &workspace_id,
                &session,
                json!({ "links-moved": ["45"] }),
            )
        })
    };
    paused
        .recv_timeout(Duration::from_secs(20))
        .expect("checked");
    let about_to_lock = crate::domain::repository::project::notify_project_placement(&session.0);
    let (moved, move_done) = mpsc::channel();
    {
        let (paths, project_id) = (store.paths.clone(), session.0.clone());
        std::thread::spawn(move || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            let moving = DomainRepository::new(&db, "test").update_project(
                json!({ "projectId": project_id, "launchSettings": { "workspaceId": other_workspace } })
                    .as_object()
                    .unwrap(),
            );
            let _ = moved.send(moving.is_ok());
        });
    }
    about_to_lock
        .recv_timeout(Duration::from_secs(20))
        .expect("the move reached the placement lock");
    // Checked, not written: the move waits.
    assert!(move_done.recv_timeout(Duration::from_millis(200)).is_err());
    resume.send(()).expect("resume");
    linking.join().expect("linked").expect("written");
    assert!(move_done
        .recv_timeout(Duration::from_secs(20))
        .expect("the move lands after the write"));
}
