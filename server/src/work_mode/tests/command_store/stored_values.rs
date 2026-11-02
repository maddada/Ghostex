//! What a workspace keeps stays out of what its command's record keeps as it is: a path or an
//! `info` that holds a stored credential is refused before it is run or kept, and a check runs
//! the path the workspace has when it starts, never one read before.

use std::path::Path;

use super::*;

/// A workspace whose command answers `GOOD_INFO` and has `token` stored, secret.
fn pinned_with_token(store: &Store, name: &str, token: &str) -> String {
    let workspace_id = store.ws(name);
    let good = script(
        store,
        "good-info",
        &format!("cat <<'EOF'\n{GOOD_INFO}\nEOF"),
    );
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &good,
        false,
        &no_references,
    )
    .expect("pinned");
    save(store, &workspace_id, json!({ "token": token })).expect("stored");
    workspace_id
}

#[test]
fn a_path_holding_a_stored_credential_is_refused_before_it_runs() {
    let store = Store::new();
    let workspace_id = pinned_with_token(&store, "ws-a", "abc123");
    let kept = store.sources().workspaces[&workspace_id].path.clone();
    // A program in a folder named after the token, which leaves a mark when it runs.
    let folder = store._temp.path().join("abc123");
    std::fs::create_dir_all(&folder).expect("folder");
    let ran = store._temp.path().join("ran");
    let program = folder.join("tracker");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ntouch '{}'\ncat <<'EOF'\n{GOOD_INFO}\nEOF\n",
            ran.display()
        ),
    )
    .expect("write");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let program = program.display().to_string();

    let set = |path: &str| {
        set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            path,
            false,
            &no_references,
        )
    };
    assert_eq!(refusal(set(&program)), PATH_SPELLS_CREDENTIAL);
    // A path the resolver would refuse gets the same words, never the resolver's quote of it.
    assert_eq!(refusal(set("bin/abc123")), PATH_SPELLS_CREDENTIAL);
    assert_eq!(store.sources().workspaces[&workspace_id].path, kept);
    // Kept after a clear, the token still keeps the path out.
    clear_work_source(&store.db, &store.paths, &workspace_id, &no_references).expect("cleared");
    assert_eq!(refusal(set(&program)), PATH_SPELLS_CREDENTIAL);
    assert!(!ran.exists(), "the program never ran");
    assert!(!store.sources().workspaces.contains_key(&workspace_id));
}

/// Secret or not, a stored value of 3 or more characters keeps a path out; a shorter one doesn't.
#[test]
fn a_resolved_path_holding_a_stored_credential_is_refused() {
    let stored = StoredValues {
        mask: CommandEnv::default(),
        values: vec!["abc123".to_string(), "ab".to_string()],
    };
    let refused = |typed: &str, resolved: Option<&str>| match refuse_path_spelling(
        &stored,
        typed,
        resolved.map(Path::new),
    ) {
        Err(WorkSourceError::Refused(text)) => Some(text),
        Ok(()) => None,
        Err(other) => panic!("{other:?}"),
    };
    // `~/tracker` or a bare name can resolve into a folder that holds it.
    assert_eq!(
        refused("~/tracker", Some("/home/abc123/tracker")).as_deref(),
        Some(PATH_SPELLS_CREDENTIAL)
    );
    assert_eq!(
        refused("/opt/abc123/tracker", None).as_deref(),
        Some(PATH_SPELLS_CREDENTIAL)
    );
    assert_eq!(refused("~/tracker", Some("/home/ana/tracker")), None);
    assert_eq!(refused("/opt/ab/tracker", Some("/opt/ab/tracker")), None);
}

/// While the command is set, the runner gives it the token and refuses an answer that holds it
/// before parsing it (`AnswerGuard::structural`). After a clear the token is kept but no longer
/// given to the command; the runner still masks with it (`CommandEnv::masking_too`) and refuses
/// that answer the same way.
#[test]
fn an_info_holding_a_stored_credential_never_reaches_the_record() {
    let cases = [
        (
            "id",
            "abc123",
            r#"{ "id": "x-abc123", "name": "X", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "token", "label": "API token" } ] }"#,
        ),
        (
            "ID keys",
            "ABC123",
            r#"{ "id": "fixture", "name": "X", "ids": { "format": "key-number", "keys": ["ABC123"] }, "credentials": [ { "name": "token", "label": "API token" } ] }"#,
        ),
        (
            "credential names",
            "abc123",
            r#"{ "id": "fixture", "name": "X", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "token", "label": "API token" }, { "name": "tok_abc123", "label": "Other" } ] }"#,
        ),
    ];
    for (what, token, answer) in cases {
        let store = Store::new();
        let workspace_id = pinned_with_token(&store, "ws-a", token);
        let echo = script(&store, "names-info", &format!("cat <<'EOF'\n{answer}\nEOF"));
        let set = || {
            set_work_source(
                &store.db,
                &store.paths,
                &workspace_id,
                &echo,
                true,
                &no_references,
            )
        };
        let refused_by_the_runner = |phase: &str| match set() {
            Err(WorkSourceError::Failed(failure)) => {
                assert!(
                    failure.message.contains("contains a credential"),
                    "{what}, {phase}: {failure:?}"
                );
                assert!(
                    !failure.message.contains(token),
                    "{what}, {phase}: {failure:?}"
                );
            }
            other => panic!("{what}, {phase}: not refused by the runner: {other:?}"),
        };
        refused_by_the_runner("while set");
        assert_eq!(
            store.sources().workspaces[&workspace_id].info.name,
            "Fixture"
        );

        clear_work_source(&store.db, &store.paths, &workspace_id, &no_references).expect("cleared");
        refused_by_the_runner("after a clear");
        assert!(
            !store.sources().workspaces.contains_key(&workspace_id),
            "{what}"
        );
    }
}

#[test]
fn a_check_runs_the_path_the_workspace_has_now() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    assert_eq!(
        refusal(check_work_source(&store.db, &store.paths, &workspace_id)),
        "This workspace has no ticket command to check."
    );
    store.set("ws-a", false).expect("pinned");
    // Another client sets another path; a check after it runs that one, never the first.
    let other = script(
        &store,
        "other-source",
        &format!("cat <<'EOF'\n{GOOD_INFO}\nEOF"),
    );
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &other,
        false,
        &no_references,
    )
    .expect("moved");
    let checked = check_work_source(&store.db, &store.paths, &workspace_id).expect("checked");
    assert_eq!(checked.path, other);
    assert_eq!(store.sources().workspaces[&workspace_id].path, other);
}

/// The check reads the path and the generation together under the writer lock, then runs `info`
/// without it. A path set while `info` runs bumps the generation, so the check commits nothing.
#[test]
fn a_path_set_while_a_check_runs_is_kept() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    // Answers once `go` exists, after marking that it runs, so the other set lands in between.
    let go = store._temp.path().join("go");
    let marker = running_marker(&store);
    let gated = script(
        &store,
        "gated-info",
        &format!(
            "touch '{}'\nwhile [ ! -e '{}' ]; do sleep 0.05; done\ncat <<'EOF'\n{GOOD_INFO}\nEOF",
            marker.display(),
            go.display()
        ),
    );
    std::fs::write(&go, "").expect("open the gate");
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &gated,
        false,
        &no_references,
    )
    .expect("pinned");
    std::fs::remove_file(&go).expect("close the gate");
    std::fs::remove_file(&marker).expect("reset the marker");
    let other = script(
        &store,
        "other-source",
        &format!("cat <<'EOF'\n{GOOD_INFO}\nEOF"),
    );
    let checked = std::thread::scope(|scope| {
        let paths = store.paths.clone();
        let checked_id = workspace_id.clone();
        let check = scope.spawn(move || {
            let db = open_gxserver_database(&paths).expect("db");
            check_work_source(&db, &paths, &checked_id)
        });
        wait_until_running(&marker);
        set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &other,
            false,
            &no_references,
        )
        .expect("set meanwhile");
        std::fs::write(&go, "").expect("let the check answer");
        check.join().expect("check thread")
    });
    assert!(
        refusal(checked).contains("changed while it was being checked"),
        "the check commits nothing"
    );
    assert_eq!(store.sources().workspaces[&workspace_id].path, other);
}

/// A clear's errors leave through the stored values too: here the references read fails with
/// text that quotes the token.
#[test]
fn a_clear_error_never_shows_a_stored_value() {
    let store = Store::new();
    let workspace_id = pinned_with_token(&store, "ws-a", "abc123");
    let failed = clear_work_source(&store.db, &store.paths, &workspace_id, &|| {
        Err(crate::domain::DomainStateError::bad_request(
            "sessions unreadable near abc123",
        ))
    });
    match failed {
        Err(WorkSourceError::Storage(error)) => {
            assert_eq!(error.message, "sessions unreadable near ••••")
        }
        other => panic!("{other:?}"),
    }
    assert!(store.sources().workspaces.contains_key(&workspace_id));
}

/// A command whose `info` declares `team` and `board`, both not secret, set in a new workspace.
fn pinned_with_open_fields(store: &Store, name: &str) -> (String, String) {
    let workspace_id = store.ws(name);
    let open = r#"{ "id": "fixture", "name": "Fixture", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "team", "label": "Team", "secret": false }, { "name": "board", "label": "Board", "secret": false } ] }"#;
    let program = script(store, "open-info", &format!("cat <<'EOF'\n{open}\nEOF"));
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    (workspace_id, program)
}

/// A program in `folder` (made under the test's folder) that answers `info`, leaving a mark when
/// it runs.
fn program_in(store: &Store, folder: &str, info: &str) -> (String, std::path::PathBuf) {
    let folder = store._temp.path().join(folder);
    std::fs::create_dir_all(&folder).expect("folder");
    let ran = folder.join("ran");
    let program = folder.join("tracker");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ntouch '{}'\ncat <<'EOF'\n{info}\nEOF\n",
            ran.display()
        ),
    )
    .expect("write");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    (program.display().to_string(), ran)
}

/// A value stored while its field is declared not secret keeps a path out as a secret one does,
/// before the path is run: an `info` there that declares it secret never runs, and the record
/// keeps its path and its declarations.
#[test]
fn a_path_holding_a_stored_value_that_isnt_secret_is_refused_before_it_runs() {
    let store = Store::new();
    let (workspace_id, first) = pinned_with_open_fields(&store, "ws-a");
    save(&store, &workspace_id, json!({ "team": "abc123" })).expect("stored, not secret");
    let (program, ran) = program_in(&store, "abc123", GOOD_INFO);

    let refused = refusal(set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    ));
    assert_eq!(refused, PATH_SPELLS_CREDENTIAL);
    assert!(!ran.exists(), "the program never ran");
    let kept = &store.sources().workspaces[&workspace_id];
    assert_eq!(kept.path, first);
    assert!(kept.info.credentials.iter().all(|field| !field.secret));
}

/// The other order: with the path set first, the value is refused when saved, secret or not.
#[test]
fn a_value_that_isnt_secret_is_refused_when_the_path_already_holds_it() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let open = r#"{ "id": "fixture", "name": "Fixture", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "team", "label": "Team", "secret": false } ] }"#;
    let (program, _) = program_in(&store, "abc123", open);
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    match save(&store, &workspace_id, json!({ "team": "abc123" })) {
        Err(WorkSourceError::Refused(text)) => assert_eq!(
            text,
            "team can't be saved: its value also appears in the command's path, which Ghostex can't hide."
        ),
        other => panic!("not refused: {other:?}"),
    }
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
}

/// A stored value that isn't secret is kept out of what an accepted answer puts in the record as
/// it is: its id (an id offered for a Switch too), its ID keys and its credential names. The
/// answer is refused in fixed words and the record keeps what it had.
#[test]
fn an_answer_holding_a_stored_value_that_isnt_secret_is_refused() {
    let store = Store::new();
    let (workspace_id, first) = pinned_with_open_fields(&store, "ws-a");
    save(
        &store,
        &workspace_id,
        json!({ "team": "abc123", "board": "QRX" }),
    )
    .expect("stored, not secret");
    let fields = r#"{ "name": "team", "label": "Team", "secret": false }, { "name": "board", "label": "Board", "secret": false }"#;
    let answers = [
        (
            "id",
            format!(
                r#"{{ "id": "abc123-tracker", "name": "Fixture", "ids": {{ "format": "key-number", "keys": ["ACM"] }}, "credentials": [ {fields} ] }}"#
            ),
        ),
        (
            "keys",
            format!(
                r#"{{ "id": "fixture", "name": "Fixture", "ids": {{ "format": "key-number", "keys": ["QRX"] }}, "credentials": [ {fields} ] }}"#
            ),
        ),
        (
            "names",
            format!(
                r#"{{ "id": "fixture", "name": "Fixture", "ids": {{ "format": "key-number", "keys": ["ACM"] }}, "credentials": [ {fields}, {{ "name": "abc123_extra", "label": "Extra", "secret": false }} ] }}"#
            ),
        ),
    ];
    for (name, info) in &answers {
        let program = script(&store, name, &format!("cat <<'EOF'\n{info}\nEOF"));
        for switch in [false, true] {
            let refused = refusal(set_work_source(
                &store.db,
                &store.paths,
                &workspace_id,
                &program,
                switch,
                &no_references,
            ));
            assert_eq!(refused, ANSWER_SPELLS_CREDENTIAL, "{name}");
            let kept = &store.sources().workspaces[&workspace_id];
            assert_eq!(kept.path, first, "{name}");
            assert_eq!(kept.source_id, "fixture", "{name}");
            assert_eq!(kept.offered_id, None, "{name}");
            assert_eq!(kept.info.id_keys, vec!["ACM".to_string()], "{name}");
            assert_eq!(kept.info.credentials.len(), 2, "{name}");
        }
    }
}

/// A credential save's own check is held to the values it finds stored and to the ones it stores:
/// an answer whose id or ID keys hold one is dropped, and the values stay stored.
#[test]
fn a_credential_check_drops_an_answer_holding_a_stored_value() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let answer = store._temp.path().join("answer.json");
    let with = |id: &str, key: &str| {
        format!(
            r#"{{ "id": "{id}", "name": "Fixture", "ids": {{ "format": "key-number", "keys": ["{key}"] }}, "credentials": [ {{ "name": "team", "label": "Team", "secret": false }}, {{ "name": "board", "label": "Board", "secret": false }} ] }}"#
        )
    };
    std::fs::write(&answer, with("fixture", "ACM")).expect("answer");
    let program = script(&store, "answering", &format!("cat '{}'", answer.display()));
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    save(&store, &workspace_id, json!({ "team": "abc123" })).expect("stored, not secret");
    for (id, key, values) in [
        ("abc123-tracker", "ACM", json!({ "board": "first" })),
        ("fixture", "QRX", json!({ "board": "QRX" })),
    ] {
        std::fs::write(&answer, with(id, key)).expect("answer");
        let saved = save(&store, &workspace_id, values).expect("stored");
        assert!(saved.failure.is_none(), "{id}");
        assert_eq!(saved.command.source_id, "fixture");
        assert_eq!(saved.command.offered_id, None, "{id}");
        assert_eq!(saved.command.info.id_keys, vec!["ACM".to_string()]);
        assert_eq!(
            saved.command.last_error.as_deref(),
            Some(ANSWER_SPELLS_CREDENTIAL),
            "{id}"
        );
    }
    let stored = &store.credentials()["workspaceSourceCredentials"][&workspace_id]["fixture"];
    assert_eq!(stored, &json!({ "team": "abc123", "board": "QRX" }));
}

/// A credential save's own check masks with what the workspace kept too: here `token`, kept for
/// the old id after a Switch, which the new command isn't given and prints anyway.
#[test]
fn a_credential_check_masks_a_value_kept_for_another_id() {
    let store = Store::new();
    let workspace_id = pinned_with_token(&store, "ws-a", "abc123");
    let failing = store._temp.path().join("now-failing");
    let other = script(
        &store,
        "other-id",
        &format!(
            "if [ -e '{}' ]; then printf '{{ \"error\": {{ \"code\": \"down\", \"message\": \"cannot reach abc123\" }} }}\\n'; exit 1; fi\ncat <<'EOF'\n{{ \"id\": \"other\", \"name\": \"Other\", \"ids\": {{ \"format\": \"key-number\", \"keys\": [\"ACM\"] }}, \"credentials\": [ {{ \"name\": \"mode\", \"label\": \"Mode\", \"secret\": false }} ] }}\nEOF",
            failing.display()
        ),
    );
    let switched = set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &other,
        true,
        &no_references,
    )
    .expect("switched");
    assert_eq!(switched.source_id, "other");
    std::fs::write(&failing, "").expect("fail from now on");
    let saved = save(&store, &workspace_id, json!({ "mode": "plain" })).expect("stored");
    let failure = saved.failure.expect("the check failed");
    assert_eq!(failure.message, "other: cannot reach ••••");
}

#[test]
fn a_value_only_masked_is_never_passed_to_the_command() {
    let kept = CommandEnv {
        credentials: vec![CommandCredential {
            name: "token".to_string(),
            value: "abc123".to_string(),
            secret: true,
        }],
        ..CommandEnv::default()
    };
    let env = CommandEnv::new(Some("fixture"), "ws-a", None, &[], None).masking_too(&kept);
    assert!(env
        .vars(|_| None)
        .iter()
        .all(|(_, value)| !value.to_string_lossy().contains("abc123")));
    assert_eq!(env.shown("cannot reach abc123"), "cannot reach ••••");
    assert!(env.holds_secrets());
}

/// A value declared not secret shorter than 3 characters is saved even where the workspace id
/// (`ws-` and 32 hex digits) holds it: a one-digit team number is saved in any workspace.
#[test]
fn a_short_value_that_isnt_secret_is_saved_when_the_workspace_id_holds_it() {
    let store = Store::new();
    let (workspace_id, _) = pinned_with_open_fields(&store, "ws-a");
    assert!(
        workspace_id.starts_with("ws-") && workspace_id.len() == 35,
        "{workspace_id}"
    );
    for value in [&workspace_id[3..4], &workspace_id[3..5]] {
        save(&store, &workspace_id, json!({ "team": value })).expect("saved");
        assert_eq!(
            store.credentials()["workspaceSourceCredentials"][&workspace_id]["fixture"]["team"],
            json!(value)
        );
    }
}

/// From 3 characters on, a value declared not secret is refused where the workspace id or the path
/// holds it, as a secret one is.
#[test]
fn a_value_that_isnt_secret_is_refused_when_the_workspace_id_or_the_path_holds_it() {
    let store = Store::new();
    let (workspace_id, program) = pinned_with_open_fields(&store, "ws-a");
    let in_path = &program[program.len() - 3..];
    for (value, what) in [
        (&workspace_id[3..6], "workspace id"),
        (workspace_id.as_str(), "workspace id"),
        (in_path, "path"),
    ] {
        match save(&store, &workspace_id, json!({ "team": value })) {
            Err(WorkSourceError::Refused(text)) => assert_eq!(
                text,
                format!("team can't be saved: its value also appears in the command's {what}, which Ghostex can't hide.")
            ),
            other => panic!("{value}: not refused: {other:?}"),
        }
    }
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
}
