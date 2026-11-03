//! A set whose path is another program than the pinned one's: that program is asked without the
//! pinned command's credentials and in no source's folder, and its answer changes nothing about
//! what is secret unless it answers with the pinned id. Its failure, or an answer with another id,
//! leaves the pinned record and the stored values as they were.

use super::*;

/// A program that records its environment beside itself and answers `info`.
fn recording(store: &Store, name: &str, answer: &str) -> String {
    let env = store._temp.path().join(format!("{name}.env"));
    script(
        store,
        name,
        &format!("env >'{}'\nprintf '%s\\n' '{answer}'", env.display()),
    )
}

fn recorded_env(store: &Store, name: &str) -> String {
    std::fs::read_to_string(store._temp.path().join(format!("{name}.env"))).expect("env")
}

/// The fixture pinned as `fixture`, with `token` and a `mode` too short to keep secret stored.
fn pinned_with_values(store: &Store) -> String {
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "tok-other-program-1", "mode": "zq" })
            .as_object()
            .unwrap(),
    )
    .expect("stored");
    workspace_id
}

#[test]
fn another_program_is_asked_without_the_pinned_commands_credentials() {
    let store = Store::new();
    let workspace_id = pinned_with_values(&store);
    let moved = recording(
        &store,
        "moved-source",
        r#"{ "id": "fixture", "name": "Fixture", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false }, { "name": "token", "label": "API token", "required": true } ] }"#,
    );
    let pinned = set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &moved,
        false,
        &no_references,
    )
    .expect("the same id, so it is pinned at its new path");
    assert_eq!(pinned.source_id, "fixture");
    let env = recorded_env(&store, "moved-source");
    assert!(!env.contains("tok-other-program-1"), "{env}");
    assert!(!env.contains("GHOSTEX_CREDENTIAL_"), "{env}");
    assert!(!env.contains("GHOSTEX_SOURCE_ID"), "{env}");
    // In the folder for a command not pinned yet, not the pinned id's.
    assert!(
        env.lines()
            .any(|line| line.starts_with("PWD=") && line.ends_with("/work-sources")),
        "{env}"
    );

    // Pinned there now, its own check gets the credentials.
    check_work_source(&store.db, &store.paths, &workspace_id).expect("checked");
    assert!(recorded_env(&store, "moved-source").contains("tok-other-program-1"));
}

#[test]
fn another_programs_answer_with_another_id_changes_no_secret_and_nothing_in_the_record() {
    let store = Store::new();
    let workspace_id = pinned_with_values(&store);
    let before = stored_row(&store);
    let other = recording(
        &store,
        "other-source",
        r#"{ "id": "other", "name": "Other", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "mode", "label": "Mode" } ] }"#,
    );
    let refused = set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &other,
        false,
        &no_references,
    );
    match refused {
        Err(WorkSourceError::Refused(text)) => assert_eq!(
            text,
            format!(
                "That command calls itself \"other\"; this workspace's links use \"fixture\". {SWITCH_OFFER}"
            )
        ),
        other => panic!("not refused: {other:?}"),
    }
    assert_eq!(stored_row(&store), before);
    assert!(!is_secret(&stored_command(&store, &workspace_id), "mode"));
    let stored = read_workspace_source_credentials(&store.paths, &workspace_id).expect("read");
    assert_eq!(stored["fixture"]["mode"], json!("zq"));
}

#[test]
fn another_programs_failure_changes_no_secret_and_nothing_in_the_record() {
    let store = Store::new();
    let workspace_id = pinned_with_values(&store);
    let before = stored_row(&store);
    let failing = script(
        &store,
        "failing-source",
        r#"printf '{ "error": { "code": "down", "message": "no" }, "credentials": [ { "name": "mode", "label": "Mode" } ] }\n'
exit 1"#,
    );
    let failed = set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &failing,
        false,
        &no_references,
    );
    assert!(
        matches!(failed, Err(WorkSourceError::Failed(_))),
        "{failed:?}"
    );
    assert_eq!(stored_row(&store), before);
    let stored = read_workspace_source_credentials(&store.paths, &workspace_id).expect("read");
    assert_eq!(stored["fixture"]["mode"], json!("zq"));
}
