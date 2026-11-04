use super::{discover, discover_before_listing, read_zcode_conversations};
use crate::{
    domain::DomainRepository,
    paths::get_gxserver_paths,
    presentation::list_previous_sessions,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::{fs, path::Path};

/// ZCode's CLI store with one interactive conversation in `cwd`, closed the way a ZCode that quits
/// cleanly leaves it: WAL mode, with no `-wal` or `-shm` file beside it.
fn write_closed_zcode_store(zcode_home: &Path, cwd: &Path) -> std::path::PathBuf {
    let db_path = zcode_home.join("cli").join("db").join("db.sqlite");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let store = Connection::open(&db_path).unwrap();
    store
        .execute_batch(
            "PRAGMA journal_mode = WAL; \
             CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT NOT NULL, title TEXT, \
             time_updated INTEGER NOT NULL, task_type TEXT, parent_id TEXT, time_archived INTEGER);",
        )
        .unwrap();
    store
        .execute(
            "INSERT INTO session (id, directory, title, time_updated, task_type) \
             VALUES ('sess_one', ?1, 'Fix the build', 1793297367000, 'interactive')",
            [cwd.to_str().unwrap()],
        )
        .unwrap();
    store
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    drop(store);
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{suffix}", db_path.display()));
    }
    db_path
}

#[test]
fn a_cleanly_closed_zcode_store_is_read_without_changing_it() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let zcode_home = temp.path().join(".zcode");
    let db_path = write_closed_zcode_store(&zcode_home, &work);
    let before = fs::read(&db_path).unwrap();

    let conversations = read_zcode_conversations(&zcode_home).expect("the store reads");

    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].id, "sess_one");
    assert_eq!(conversations[0].title, "Fix the build");
    assert_eq!(fs::read(&db_path).unwrap(), before);
}

#[test]
fn the_closed_list_answers_when_an_outside_store_cannot_be_read() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).unwrap();
    let db = open_gxserver_database(&paths).unwrap();
    let work = temp.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Ghostex", "path": work })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    let session = repository
        .create_session(
            json!({
                "agentId": "codex",
                "kind": "agent",
                "lifecycleState": "stopped",
                "projectId": project["projectId"],
                "providerState": {
                    "lifecycleState": "missing",
                    "probedAt": "2026-10-29T12:00:00.000Z",
                    "provider": "zmx",
                },
                "runtimeSettings": { "titleSource": "terminal-auto" },
                "title": "Closed today",
            })
            .as_object()
            .unwrap(),
            false,
        )
        .unwrap();
    let zcode_db = temp.path().join(".zcode").join("cli").join("db");
    fs::create_dir_all(&zcode_db).unwrap();
    fs::write(zcode_db.join("db.sqlite"), "not a database").unwrap();
    assert!(discover(&db, "S7k", &paths, true).is_err());

    // What `/api/listPreviousSessions` runs.
    discover_before_listing(&db, "S7k", &paths, true);
    let mut params = Map::new();
    params.insert("projectId".to_string(), project["projectId"].clone());
    let result = list_previous_sessions(&db, "S7k", &params).unwrap();

    let results = result["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["sessionId"], session["sessionId"]);
    assert_eq!(result["projects"][0]["projectId"], project["projectId"]);
    assert_eq!(result["projects"][0]["name"], Value::from("Ghostex"));
}
