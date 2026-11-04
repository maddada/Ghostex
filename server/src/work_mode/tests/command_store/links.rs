//! A session's hand-set tickets of a ticket command (`workLinks.tickets`), as
//! `checked_ticket_links` checks them and the save writes them: what the workspace stores is read strictly,
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
    let locks = hold_ticket_link_locks()?;
    let request = checked_ticket_links(
        &locks,
        request.as_object().unwrap(),
        || Ok(workspace_id.to_string()),
        db,
        paths,
    )?;
    let links = merge_work_links(None, &request)?;
    write_work_links(db, project_id, session_id, &links)?;
    Ok(links)
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

/// A session of a placed project in `workspace_id`, whose checkout is on `branch`.
fn session_on_branch(store: &Store, workspace_id: &str, branch: &str) -> (String, String) {
    let project = super::cards::project_in(store, workspace_id, json!({}));
    set_branch(project["path"].as_str().unwrap(), branch);
    let session = DomainRepository::new(&store.db, "test")
        .create_session(
            json!({ "projectId": project["projectId"], "kind": "agent", "agentId": "codex", "title": "S", "lifecycleState": "running" })
                .as_object()
                .unwrap(),
            false,
        )
        .expect("session");
    (
        project["projectId"].as_str().unwrap().to_string(),
        session["sessionId"].as_str().unwrap().to_string(),
    )
}

/// Saves `request` on the session, as `/api/setSessionWorkLinks` does.
fn save(
    db: &rusqlite::Connection,
    paths: &GxserverPaths,
    (project_id, session_id): &(String, String),
    request: Value,
) -> Result<SavedWorkLinks, crate::domain::DomainStateError> {
    let mut request = request.as_object().unwrap().clone();
    request.insert("projectId".to_string(), json!(project_id));
    request.insert("sessionId".to_string(), json!(session_id));
    save_session_work_links(
        &DomainRepository::new(db, "test"),
        db,
        paths,
        project_id,
        session_id,
        &request,
    )
}

/// An added ticket joins the ones the branch names, as the Work page's Link to (`source` +
/// `ticketId`) or `addTickets` sends it; its undo puts the command back to automatic, and adding
/// one typed as it is already linked changes nothing.
#[test]
fn linking_a_command_ticket_keeps_the_branch_ones() {
    let store = Store::new();
    let workspace_id = super::cards::pin(&store, "ws-a", "links-add", "good");
    let session = session_on_branch(&store, &workspace_id, "leandro/acm-12-fix");
    for add in [
        json!({ "addTickets": { "links-add": ["acm-14"] } }),
        json!({ "source": "links-add", "ticketId": "ACM-14" }),
    ] {
        let (links, undo) = save(&store.db, &store.paths, &session, add.clone()).expect("added");
        assert_eq!(
            links["tickets"],
            json!({ "links-add": ["ACM-12", "ACM-14"] }),
            "{add}"
        );
        let undo = undo.expect("an undo");
        assert_eq!(
            Value::Object(undo.clone()),
            json!({ "tickets": { "links-add": null } })
        );
        let (links, _) =
            save(&store.db, &store.paths, &session, Value::Object(undo)).expect("undone");
        assert!(links.get("tickets").is_none(), "{add}: {links:?}");
    }
    let (links, undo) = save(
        &store.db,
        &store.paths,
        &session,
        json!({ "addTickets": { "links-add": ["ACM-12"] } }),
    )
    .expect("already linked");
    assert!(links.get("tickets").is_none(), "{links:?}");
    assert_eq!(undo, Some(Map::new()));
}

/// An added ticket is checked as a set one is: only the command the session's workspace pins,
/// outside a team and not retired, and never an id holding a stored value. Nothing is written.
#[test]
fn an_added_command_ticket_is_checked_like_a_set_one() {
    let store = Store::new();
    let workspace_id = pin_number_source(&store, "links-add-checked");
    let session = session_on_branch(&store, &workspace_id, "main");
    let elsewhere = session_on_branch(&store, &store.ws("ws-b"), "main");
    let add = |ids: Value| json!({ "addTickets": { "links-add-checked": ids } });
    let refused = |session: &(String, String), request: Value| {
        save(&store.db, &store.paths, session, request)
            .expect_err("refused")
            .message
    };
    assert_eq!(
        refused(&session, add(json!(["#123"]))),
        TICKET_LINK_SPELLS_CREDENTIAL
    );
    assert_eq!(
        refused(
            &session,
            json!({ "addTickets": { "links-never-pinned": ["45"] } })
        ),
        TICKET_LINKS_REFUSED
    );
    assert_eq!(
        refused(&elsewhere, add(json!(["45"]))),
        TICKET_LINKS_REFUSED
    );
    join_team(&store.paths, &workspace_id);
    assert_eq!(refused(&session, add(json!(["45"]))), TICKET_LINKS_REFUSED);
    leave_team(&store);
    let references = || {
        let repository = DomainRepository::new(&store.db, "test");
        let workspaces = crate::workspaces::read_sidebar_workspaces(&store.db)?;
        referenced_source_ids(&repository, &workspaces)
    };
    crate::work_mode::clear_work_source(&store.db, &store.paths, &workspace_id, &references)
        .result
        .expect("cleared");
    assert_eq!(refused(&session, add(json!(["45"]))), TICKET_LINKS_REFUSED);
    for (project_id, session_id) in [&session, &elsewhere] {
        let row = DomainRepository::new(&store.db, "test")
            .get_session(project_id, session_id)
            .expect("read")
            .expect("session");
        assert!(row["runtimeSettings"].get("workLinks").is_none(), "{row}");
    }
}

/// Connects `workspace_id` to a team, as `/api/joinTeam` saves it.
fn join_team(paths: &GxserverPaths, workspace_id: &str) {
    mutate_credentials(paths, |credentials| {
        credentials.insert(
            "teamSync".to_string(),
            json!({ workspace_id: { "deploymentUrl": "https://team.example", "memberToken": "member-token" } }),
        );
    })
    .expect("joined");
}

fn leave_team(store: &Store) {
    mutate_credentials(&store.paths, |credentials| {
        credentials.remove("teamSync");
    })
    .expect("left");
}

/// A team join and a clear each wait while an added ticket is checked and written, so the add is
/// written while its command still applies, and the next add after them is refused.
#[test]
fn a_team_join_or_clear_waits_for_a_ticket_being_added() {
    for change in ["team", "clear"] {
        let store = Store::new();
        let id = if change == "team" {
            "links-add-team"
        } else {
            "links-add-clear"
        };
        let workspace_id = pin_number_source(&store, id);
        let session = session_on_branch(&store, &workspace_id, "main");
        let (reached, paused) = mpsc::channel();
        let (resume, release) = mpsc::channel();
        hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
        let adding = {
            let (paths, session) = (store.paths.clone(), session.clone());
            let request = json!({ "addTickets": { id: ["45"] } });
            std::thread::spawn(move || {
                let db = crate::storage::open_gxserver_database(&paths).expect("db");
                save(&db, &paths, &session, request)
            })
        };
        paused
            .recv_timeout(Duration::from_secs(20))
            .expect("checked");
        let (changed, change_done) = mpsc::channel();
        {
            let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
            std::thread::spawn(move || {
                let done = if change == "team" {
                    // As saving a team connection writes it (`store_team_connection`).
                    crate::work_mode::change_work_source_config(&workspace_id, || {
                        join_team(&paths, &workspace_id)
                    })
                    .is_ok()
                } else {
                    let db = crate::storage::open_gxserver_database(&paths).expect("db");
                    let references = || {
                        let repository = DomainRepository::new(&db, "test");
                        let workspaces = crate::workspaces::read_sidebar_workspaces(&db)?;
                        referenced_source_ids(&repository, &workspaces)
                    };
                    crate::work_mode::clear_work_source(&db, &paths, &workspace_id, &references)
                        .result
                        .is_ok()
                };
                let _ = changed.send(done);
            });
        }
        // Checked, not written: the change waits.
        assert!(
            change_done
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "{change}"
        );
        resume.send(()).expect("resume");
        let (links, _) = adding.join().expect("added").expect("written");
        assert_eq!(links["tickets"], json!({ id: ["45"] }), "{change}");
        assert!(
            change_done
                .recv_timeout(Duration::from_secs(20))
                .expect("the change lands after the write"),
            "{change}"
        );
        let refused = save(
            &store.db,
            &store.paths,
            &session,
            json!({ "addTickets": { id: ["46"] } }),
        )
        .expect_err("refused after the change");
        assert_eq!(refused.message, TICKET_LINKS_REFUSED, "{change}");
    }
}

/// The Work page's form (`source` + `ticketId`) reaches the check as typed: `acm-14` holds the
/// stored `acm` and is refused, though the command reads it as `ACM-14`, which doesn't; and a
/// source nobody pins gets the check's fixed words.
#[test]
fn the_work_pages_ticket_is_checked_as_typed() {
    let store = Store::new();
    let workspace_id = super::cards::pin(&store, "ws-a", "links-typed", "good");
    mutate_credentials(&store.paths, |credentials| {
        credentials["workspaceSourceCredentials"][&workspace_id]["links-typed"]["token"] =
            json!("acm");
    })
    .expect("stored");
    let session = session_on_branch(&store, &workspace_id, "main");
    let refused = |request: Value| {
        save(&store.db, &store.paths, &session, request)
            .expect_err("refused")
            .message
    };
    assert_eq!(
        refused(json!({ "source": "links-typed", "ticketId": "acm-14" })),
        TICKET_LINK_SPELLS_CREDENTIAL
    );
    // Even beside the same ticket already linked: the typed form is still checked.
    save(
        &store.db,
        &store.paths,
        &session,
        json!({ "tickets": { "links-typed": ["ACM-14"] } }),
    )
    .expect("linked");
    assert_eq!(
        refused(json!({ "addTickets": { "links-typed": ["acm-14"] } })),
        TICKET_LINK_SPELLS_CREDENTIAL
    );
    assert_eq!(
        refused(json!({ "source": "links-nobody-pins", "ticketId": "ACM-14" })),
        TICKET_LINKS_REFUSED
    );
    let (links, _) = save(
        &store.db,
        &store.paths,
        &session,
        json!({ "source": "links-typed", "ticketId": "ACM-15" }),
    )
    .expect("linked");
    assert_eq!(
        links["tickets"],
        json!({ "links-typed": ["ACM-14", "ACM-15"] })
    );
}

/// Two adds on one session at once: the second reads the links the first wrote, so both tickets
/// stay linked, and each undo puts back what that add found.
#[test]
fn two_adds_at_once_keep_both_tickets() {
    let store = Store::new();
    let workspace_id = pin_number_source(&store, "links-two-adds");
    let session = session_on_branch(&store, &workspace_id, "main");
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let add = |number: &str, wanted: Option<mpsc::Sender<()>>| {
        let (paths, session) = (store.paths.clone(), session.clone());
        let request = json!({ "addTickets": { "links-two-adds": [number] } });
        std::thread::spawn(move || {
            if let Some(wanted) = wanted {
                notify_ticket_link_locks_wanted(wanted);
            }
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            save(&db, &paths, &session, request)
        })
    };
    let first = add("45", None);
    paused
        .recv_timeout(Duration::from_secs(20))
        .expect("the first add checked");
    let (wanted, waiting) = mpsc::channel();
    let second = add("46", Some(wanted));
    // The second add now waits for the locks the first holds; whatever it read before them, it read.
    waiting
        .recv_timeout(Duration::from_secs(20))
        .expect("the second add reached the locks");
    resume.send(()).expect("resume");
    let (_, first_undo) = first.join().expect("first").expect("written");
    let (links, second_undo) = second.join().expect("second").expect("written");
    assert_eq!(links["tickets"], json!({ "links-two-adds": ["45", "46"] }));
    assert_eq!(
        first_undo.map(Value::Object),
        Some(json!({ "tickets": { "links-two-adds": null } }))
    );
    assert_eq!(
        second_undo.map(Value::Object),
        Some(json!({ "tickets": { "links-two-adds": ["45"] } }))
    );
}

/// A ticket linked before a credential that its id contains was stored is checked again when it
/// is added again, as typed, and refused: the add never answers with the links as they were.
#[test]
fn a_linked_ticket_added_again_is_checked_against_what_is_stored_now() {
    let store = Store::new();
    let workspace_id = super::cards::pin(&store, "ws-a", "links-again", "good");
    let session = session_on_branch(&store, &workspace_id, "main");
    let (links, _) = save(
        &store.db,
        &store.paths,
        &session,
        json!({ "addTickets": { "links-again": ["ACM-14"] } }),
    )
    .expect("linked");
    assert_eq!(links["tickets"], json!({ "links-again": ["ACM-14"] }));
    mutate_credentials(&store.paths, |credentials| {
        credentials["workspaceSourceCredentials"][&workspace_id]["links-again"]["token"] =
            json!("M-14");
    })
    .expect("stored");
    for again in [
        json!({ "addTickets": { "links-again": ["ACM-14"] } }),
        json!({ "source": "links-again", "ticketId": "ACM-14" }),
    ] {
        let refused = save(&store.db, &store.paths, &session, again.clone()).expect_err("refused");
        assert_eq!(refused.message, TICKET_LINK_SPELLS_CREDENTIAL, "{again}");
    }
}
