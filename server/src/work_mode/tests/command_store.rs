//! What this computer keeps about each workspace's ticket command: setting it runs `info` and
//! pins its id, credentials are stored under that id, an id the command changes to waits for a
//! Switch, the id format never changes under saved links, and clearing or deleting retires the id
//! without ever failing on a bound.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::paths::{get_gxserver_paths, GxserverPaths};
use crate::storage::{initialize_gxserver_storage, open_gxserver_database, GxserverDatabase};

use super::*;

mod published;
mod stored_values;

/// The store's set of a typed path, as these tests read it: its result, whose errors already left
/// through its mask (`WorkSourceChange`).
fn set_work_source(
    db: &rusqlite::Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    typed_path: &str,
    switch: bool,
    references: References,
) -> Result<WorkspaceCommand, WorkSourceError> {
    crate::work_mode::set_work_source(
        db,
        paths,
        workspace_id,
        SourcePath::Typed(typed_path),
        switch,
        references,
    )
    .result
}

/// The store's check with the path the workspace has, as these tests read it.
fn check_work_source(
    db: &rusqlite::Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<WorkspaceCommand, WorkSourceError> {
    crate::work_mode::set_work_source(
        db,
        paths,
        workspace_id,
        SourcePath::Stored,
        false,
        &no_references,
    )
    .result
}

/// The store's clear, as these tests read it. It reads the stored values' mask from the folders
/// the test database keeps beside it.
fn clear_work_source(
    db: &StoreDb,
    workspace_id: &str,
    references: References,
) -> Result<(), WorkSourceError> {
    crate::work_mode::clear_work_source(db, &db.paths, workspace_id, references).result
}

/// A test's database and the folders its credentials file is in.
struct StoreDb {
    database: GxserverDatabase,
    paths: GxserverPaths,
}

impl std::ops::Deref for StoreDb {
    type Target = GxserverDatabase;

    fn deref(&self) -> &GxserverDatabase {
        &self.database
    }
}

struct Store {
    /// Each name's workspace, created in the workspaces document on first use. Generations are
    /// process-wide, and these ids are new for every test.
    workspaces: RefCell<HashMap<String, String>>,
    _temp: tempfile::TempDir,
    paths: GxserverPaths,
    db: StoreDb,
    program: PathBuf,
}

impl Store {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().join("gx")));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let program = temp.path().join("fixture-source");
        std::fs::write(&program, include_str!("command_source.sh")).expect("write fixture");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self {
            workspaces: RefCell::new(HashMap::new()),
            _temp: temp,
            db: StoreDb {
                database: db,
                paths: paths.clone(),
            },
            paths,
            program,
        }
    }

    fn ws(&self, name: &str) -> String {
        if let Some(id) = self.workspaces.borrow().get(name) {
            return id.clone();
        }
        let current = crate::workspaces::read_sidebar_workspaces(&self.db).expect("workspaces");
        let mut params = Map::new();
        params.insert("name".to_string(), json!(name));
        let (id, next) = crate::workspaces::create_workspace_in(&current, &params).expect("create");
        crate::workspaces::write_sidebar_workspaces(&self.db, &next).expect("write");
        self.workspaces
            .borrow_mut()
            .insert(name.to_string(), id.clone());
        id
    }

    /// The mode the fixture answers in before its `mode` credential is set.
    fn answer(&self, mode: &str) {
        for source in [None, Some("fixture"), Some("renamed")] {
            let dir = command_work_dir(&self.paths, source).expect("work dir");
            std::fs::write(dir.join("mode.txt"), mode).expect("write mode");
        }
    }

    fn set(&self, workspace: &str, switch: bool) -> Result<WorkspaceCommand, WorkSourceError> {
        set_work_source(
            &self.db,
            &self.paths,
            &self.ws(workspace),
            &self.program.display().to_string(),
            switch,
            &no_references,
        )
    }

    fn sources(&self) -> WorkSources {
        read_work_sources(&self.db).expect("read")
    }

    fn credentials(&self) -> Map<String, Value> {
        read_credentials(&self.paths)
    }
}

fn no_references() -> Result<BTreeSet<String>, crate::domain::DomainStateError> {
    Ok(BTreeSet::new())
}

fn refusal(result: Result<WorkspaceCommand, WorkSourceError>) -> String {
    match result {
        Err(WorkSourceError::Refused(text)) => text,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn setting_a_command_pins_its_id_and_keeps_its_info() {
    let store = Store::new();
    store.answer("good");
    let command = store.set("ws-a", false).expect("pinned");
    assert_eq!(command.source_id, "fixture");
    assert_eq!(command.info.name, "Fixture");
    assert_eq!(command.info.id_keys, ["ACM", "OPS"]);
    assert!(command.resolved_path.is_absolute());
    assert_eq!(store.sources().workspaces[&store.ws("ws-a")], command);
    // The record reads back exactly, icon included.
    assert!(matches!(command.info.icon, Some(Ok(_))));
}

#[test]
fn a_damaged_command_store_blocks_changes_without_losing_other_pins() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("first pin");
    store.set("ws-b", false).expect("second pin");
    let intact = stored_row(&store);
    let mut bad_command: Value = serde_json::from_str(&intact).expect("document");
    bad_command["workspaces"][store.ws("ws-a")]
        .as_object_mut()
        .expect("command")
        .remove("sourceId");
    let mut bad_retired: Value = serde_json::from_str(&intact).expect("document");
    bad_retired["retired"] = json!([{ "id": "old-without-format" }]);
    for damaged in [
        "{".to_string(),
        "[]".to_string(),
        bad_command.to_string(),
        bad_retired.to_string(),
    ] {
        store
            .db
            .execute(
                "UPDATE metadata SET value = ?1 WHERE key = 'workTicketSources'",
                [&damaged],
            )
            .expect("damage row");
        assert!(read_work_sources(&store.db).is_err());
        assert!(matches!(
            store.set("ws-b", false),
            Err(WorkSourceError::Storage(_))
        ));
        assert!(matches!(
            clear_work_source(&store.db, &store.ws("ws-b"), &no_references),
            Err(WorkSourceError::Storage(_))
        ));
        assert!(matches!(
            delete_workspace_work_source(
                &store.db,
                &store.paths,
                &store.ws("ws-b"),
                &no_references
            ),
            Err(WorkSourceError::Storage(_))
        ));
        assert_eq!(
            stored_row(&store),
            damaged,
            "the damaged row was not replaced"
        );
    }
    store
        .db
        .execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'workTicketSources'",
            [&intact],
        )
        .expect("restore row");
    assert_eq!(store.sources().workspaces.len(), 2);
}

#[test]
fn a_missing_or_empty_command_store_reads_as_empty_and_every_written_one_reads_back() {
    let store = Store::new();
    assert_eq!(store.sources(), WorkSources::default(), "no row yet");
    write_work_sources(&store.db, &WorkSources::default()).expect("write empty");
    assert_eq!(store.sources(), WorkSources::default(), "an empty row");
    // A refused icon is kept as its reason; retired ids keep their format, name and icon.
    store.answer("bad-icon");
    let refused_icon = store.set("ws-a", false).expect("pinned");
    assert!(matches!(refused_icon.info.icon, Some(Err(_))));
    let mut written = store.sources();
    assert_eq!(written.workspaces[&store.ws("ws-a")], refused_icon);
    written.retired = vec![
        retired(0),
        RetiredSource {
            format: IdFormat::KeyNumber,
            icon: Some("<svg/>".to_string()),
            ..retired(1)
        },
    ];
    write_work_sources(&store.db, &written).expect("write back");
    assert_eq!(store.sources(), written);
}

#[test]
fn a_command_that_now_calls_itself_otherwise_waits_for_a_switch() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    store.answer("info-renamed");
    let refused = refusal(store.set("ws-a", false));
    assert!(
        refused.contains("now calls itself \"renamed\""),
        "{refused}"
    );
    // `ghostex issues source` adds its `--switch` hint after this ending.
    assert!(refused.ends_with(SWITCH_OFFER), "{refused}");
    let kept = &store.sources().workspaces[&store.ws("ws-a")];
    assert_eq!(kept.source_id, "fixture");
    assert_eq!(kept.offered_id.as_deref(), Some("renamed"));

    let switched = store.set("ws-a", true).expect("switched");
    assert_eq!(switched.source_id, "renamed");
    let sources = store.sources();
    assert_eq!(sources.workspaces[&store.ws("ws-a")].offered_id, None);
    assert_eq!(
        sources
            .retired
            .iter()
            .map(|retired| retired.id.as_str())
            .collect::<Vec<_>>(),
        ["fixture"]
    );
}

#[test]
fn a_change_of_id_format_is_refused_here_and_in_another_workspace() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    store.answer("info-number");
    let here = refusal(store.set("ws-a", false));
    assert!(here.contains("sends number ids now"), "{here}");
    assert_eq!(
        store.sources().workspaces[&store.ws("ws-a")].info.id_format,
        IdFormat::KeyNumber
    );
    let elsewhere = refusal(store.set("ws-b", false));
    assert!(elsewhere.contains("key-number ids"), "{elsewhere}");
    // A retired id keeps its format too.
    clear_work_source(&store.db, &store.ws("ws-a"), &no_references).expect("cleared");
    assert!(refusal(store.set("ws-b", false)).contains("sends number ids now"));
}

#[test]
fn clearing_keeps_the_credentials_and_retires_the_id() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let values = json!({ "mode": "good", "token": "  tok-store-1  " });
    let CredentialsSaved {
        command, failure, ..
    } = set_work_source_credentials(
        &store.db,
        &store.paths,
        &store.ws("ws-a"),
        values.as_object().unwrap(),
    )
    .expect("stored");
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(command.last_error, None);
    let stored = store.credentials();
    assert_eq!(
        stored["workspaceSourceCredentials"][&store.ws("ws-a")]["fixture"],
        json!({ "mode": "good", "token": "tok-store-1" })
    );
    clear_work_source(&store.db, &store.ws("ws-a"), &no_references).expect("cleared");
    assert!(store.sources().workspaces.is_empty());
    assert_eq!(store.sources().retired[0].id, "fixture");
    assert_eq!(
        store.credentials(),
        stored,
        "clearing keeps the credentials"
    );
}

#[test]
fn a_refused_path_masks_credentials_kept_after_clearing() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    let secret = "tok-path-903";
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": secret }).as_object().expect("values"),
    )
    .expect("stored");
    let path = format!("bin/{secret}");
    for cleared in [false, true] {
        if cleared {
            clear_work_source(&store.db, &workspace_id, &no_references).expect("cleared");
        }
        let refused = refusal(set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &path,
            false,
            &no_references,
        ));
        assert!(refused.contains("bin/••••"), "{refused}");
        assert!(!refused.contains(secret), "{refused}");
    }
}

#[test]
fn only_declared_single_line_credentials_are_stored() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let refused = |values: Value| match set_work_source_credentials(
        &store.db,
        &store.paths,
        &store.ws("ws-a"),
        values.as_object().unwrap(),
    ) {
        Err(WorkSourceError::Refused(text)) => text,
        other => panic!("not refused: {other:?}"),
    };
    assert!(refused(json!({ "gone": "x" })).contains("isn't a credential this command asks for"));
    assert!(refused(json!({ "token": "two\nlines" })).contains("single line"));
    assert!(refused(json!({ "token": "x".repeat(4097) })).contains("longer than 4 KB"));
    assert!(refused(json!({ "token": 7 })).contains("must be text or null"));
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
    assert!(matches!(
        set_work_source_credentials(&store.db, &store.paths, &store.ws("ws-none"), &Map::new()),
        Err(WorkSourceError::Refused(text)) if text == "Set the ticket command first."
    ));
}

#[test]
fn a_secret_of_one_or_two_characters_is_refused_when_it_is_saved() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    let save = |values: Value| {
        set_work_source_credentials(
            &store.db,
            &store.paths,
            &workspace_id,
            values.as_object().unwrap(),
        )
    };
    let too_short = "API token: a value this short can't be kept secret; a secret needs at least 3 characters. If it isn't secret, the command's info should mark it \"secret\": false.";
    // Counted after the trim the store applies.
    for value in ["7", "42", "  42  ", "ж€"] {
        match save(json!({ "token": value })) {
            Err(WorkSourceError::Refused(text)) => assert_eq!(text, too_short, "{value:?}"),
            other => panic!("{value:?} not refused: {other:?}"),
        }
    }
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
    // Characters no id or path here holds, so nothing else refuses it.
    save(json!({ "token": " ж€✓ " })).expect("3 characters are kept");
    // Blank clears the field, as `null` does.
    save(json!({ "token": "  " })).expect("cleared");
    assert!(stored_value(&store, &workspace_id, "token").is_none());
    // `mode` is declared not secret, so a single character is fine. `§` is no fixture mode, so
    // the check after it fails; its stdout isn't JSON and declares nothing, so `mode` stays not
    // secret.
    let saved = save(json!({ "mode": "§" })).expect("not secret");
    assert!(saved.failure.is_some(), "the check failed");
    assert!(!is_secret(&saved.command, "mode"));
    save(json!({ "mode": "¶" })).expect("still not secret");
    // A failing check that declares `mode` secret makes it secret: a short value is refused then.
    let saved = save(json!({ "mode": "mode-secret-invalid" })).expect("stored");
    assert!(saved.failure.is_some(), "the check failed");
    assert!(is_secret(&saved.command, "mode"));
    match save(json!({ "mode": "¶" })) {
        Err(WorkSourceError::Refused(text)) => {
            assert!(text.starts_with("Mode: a value this short"), "{text}")
        }
        other => panic!("not refused once secret: {other:?}"),
    }
    assert_eq!(
        stored_value(&store, &workspace_id, "mode").as_deref(),
        Some("mode-secret-invalid")
    );
    // The label is masked with a proposed value that can be masked, here `mode`'s, and the
    // sentence stays the same.
    match save(json!({ "mode": "API", "token": "A" })) {
        Err(WorkSourceError::Refused(text)) => assert_eq!(
            text,
            too_short.replacen("API token", "•••• token", 1),
            "{text}"
        ),
        other => panic!("not refused: {other:?}"),
    }
}

/// A field the accepted info has as not secret, with no value yet, that a failing `info`
/// declares secret: it is secret from then on, so a short value is refused, and a value saved
/// later never comes back, even after a check whose stdout declares nothing.
#[test]
fn a_field_a_failed_check_declares_secret_is_secret_before_it_has_a_value() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let phase = store._temp.path().join("phase");
    let program = script(
        &store,
        "pin-source",
        &format!(
            r#"case "$(cat '{}' 2>/dev/null)" in
declare) printf '{{ "credentials": [ {{ "name": "pin", "label": "PIN" }} ] }}\n'; exit 1 ;;
garbage) echo "not an answer"; exit 0 ;;
esac
printf '{{ "id": "fixture", "name": "Fixture", "ids": {{ "format": "number" }}, "credentials": [ {{ "name": "pin", "label": "PIN", "secret": false }} ] }}\n'"#,
            phase.display()
        ),
    );
    let check = || {
        set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &program,
            false,
            &no_references,
        )
    };
    check().expect("pinned");
    assert!(!is_secret(&stored_command(&store, &workspace_id), "pin"));
    std::fs::write(&phase, "declare").expect("phase");
    assert!(matches!(check(), Err(WorkSourceError::Failed(_))));
    assert!(is_secret(&stored_command(&store, &workspace_id), "pin"));
    let save = |value: &str| {
        set_work_source_credentials(
            &store.db,
            &store.paths,
            &workspace_id,
            json!({ "pin": value }).as_object().unwrap(),
        )
    };
    match save("7") {
        Err(WorkSourceError::Refused(text)) => {
            assert!(text.starts_with("PIN: a value this short"), "{text}")
        }
        other => panic!("not refused: {other:?}"),
    }
    std::fs::write(&phase, "garbage").expect("phase");
    let saved = save("ж742").expect("stored");
    assert!(saved.failure.is_some(), "the check failed");
    assert!(matches!(check(), Err(WorkSourceError::Failed(_))));
    let command = stored_command(&store, &workspace_id);
    assert!(is_secret(&command, "pin"));
    let summary = work_source_summary(&store.paths, &workspace_id, Some(&command), false);
    assert!(!summary.to_string().contains("ж742"), "{summary}");
    assert_eq!(summary["command"]["credentials"][0]["set"], json!(true));
    assert!(
        summary["command"]["credentials"][0].get("value").is_none(),
        "{summary}"
    );
}

/// A failing `info` that declares a field named like a stored secret, a name the accepted info
/// doesn't have: the name is not kept, and neither the failure's nor the error's `Debug` prints
/// it.
#[test]
fn a_declared_name_the_accepted_info_doesnt_have_is_dropped_and_never_printed() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let failing = store._temp.path().join("failing");
    let program = script(
        &store,
        "named-source",
        &format!(
            r#"if [ -f '{}' ]; then
printf '{{ "credentials": [ {{ "name": "abcsecret", "label": "Odd" }}, {{ "name": "token", "label": "API token" }} ] }}\n'
exit 1
fi
printf '%s\n'"#,
            failing.display()
        )
        .replace("%s", GOOD_INFO),
    );
    let check = || {
        set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &program,
            false,
            &no_references,
        )
    };
    check().expect("pinned");
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "abcsecret" }).as_object().unwrap(),
    )
    .expect("stored");
    std::fs::write(&failing, "").expect("fail");
    let error = check().expect_err("the check failed");
    let printed = format!("{error:?}");
    assert!(!printed.contains("abcsecret"), "{printed}");
    let WorkSourceError::Failed(failure) = error else {
        panic!("not a failed check: {printed}");
    };
    assert_eq!(failure.declared_secret, ["token"]);
    let printed = format!("{failure:?}");
    assert!(!printed.contains("abcsecret"), "{printed}");
    // Its `Debug` prints no name, whatever the set holds.
    let named = CheckFailure {
        declared_secret: vec!["abcsecret".to_string()],
        ..failure
    };
    let printed = format!("{named:?}");
    assert!(!printed.contains("abcsecret"), "{printed}");
}

fn save(
    store: &Store,
    workspace_id: &str,
    values: Value,
) -> Result<CredentialsSaved, WorkSourceError> {
    set_work_source_credentials(
        &store.db,
        &store.paths,
        workspace_id,
        values.as_object().unwrap(),
    )
}

fn join_team(store: &Store, workspace_id: &str) {
    change_work_source_config(workspace_id, || {
        mutate_credentials(&store.paths, |credentials| {
            let teams = credentials
                .entry("teamSync".to_string())
                .or_insert_with(|| json!({}));
            teams[workspace_id] =
                json!({ "deploymentUrl": "https://team.example", "memberToken": "member" });
        })
    })
    .expect("joined");
}

/// A team workspace can't set a command or store its credentials. Every refusal is masked with
/// what the workspace stored before it, and a credential save's with what it proposes too, both
/// read before the team check: here `team`, stored in one workspace and proposed in the other.
#[test]
fn a_team_workspace_is_refused_and_the_refusal_masks_stored_and_proposed_values() {
    let store = Store::new();
    store.answer("good");
    let masked = "A workspace connected to a •••• uses its ••••'s tracker; ticket commands are for this computer's workspaces.";
    let stored = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    save(&store, &stored, json!({ "token": "team" })).expect("stored");
    let proposed = store.ws("ws-b");
    store.set("ws-b", false).expect("pinned");
    for workspace_id in [&stored, &proposed] {
        join_team(&store, workspace_id);
    }
    assert_eq!(refusal(store.set("ws-a", false)), masked);
    assert_eq!(refusal(store.set("ws-b", false)), TEAM_REFUSAL);
    match save(&store, &stored, json!({ "token": "tok-new-5" })) {
        Err(WorkSourceError::Refused(text)) => assert_eq!(text, masked),
        other => panic!("not refused: {other:?}"),
    }
    match save(&store, &proposed, json!({ "token": "team" })) {
        Err(WorkSourceError::Refused(text)) => assert_eq!(text, masked),
        other => panic!("not refused: {other:?}"),
    }
    assert_eq!(
        stored_value(&store, &stored, "token").as_deref(),
        Some("team")
    );
    assert!(stored_value(&store, &proposed, "token").is_none());
}

/// The mask is read before the save changes anything: a storage error that quotes a stored
/// secret the save is removing still masks it.
#[test]
fn a_storage_error_masks_a_stored_secret_the_save_removes() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    save(&store, &workspace_id, json!({ "token": "tok-gone-2" })).expect("stored");
    store
        .db
        .execute_batch(
            "CREATE TRIGGER refuse_sources BEFORE UPDATE ON metadata \
             WHEN NEW.key = 'workTicketSources' BEGIN SELECT RAISE(ABORT, 'stopped by a trigger at tok-gone-2'); END;",
        )
        .expect("trigger");
    match save(&store, &workspace_id, json!({ "token": null })) {
        Err(WorkSourceError::Storage(error)) => assert_eq!(
            error.message,
            "SQLite ticket sources error: stopped by a trigger at ••••"
        ),
        other => panic!("not a storage error: {other:?}"),
    }
}

/// Credentials kept after the command was cleared are in the mask, though no record declares
/// their fields any more.
#[test]
fn a_refusal_after_a_clear_masks_the_values_it_kept() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    save(&store, &workspace_id, json!({ "token": "command" })).expect("stored");
    clear_work_source(&store.db, &workspace_id, &no_references).expect("cleared");
    match save(&store, &workspace_id, json!({ "token": "tok-new-6" })) {
        Err(WorkSourceError::Refused(text)) => assert_eq!(text, "Set the ticket •••• first."),
        other => panic!("not refused: {other:?}"),
    }
}

/// A save whose mask can't be read refuses with a fixed sentence and stores nothing, instead of
/// going on with an empty mask.
#[test]
fn a_save_that_cant_read_the_stored_values_is_refused() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    save(&store, &workspace_id, json!({ "token": "tok-old-4" })).expect("stored");
    let file = store.paths.auth_dir.join("work-mode-credentials.json");
    std::fs::remove_file(&file).expect("remove");
    std::fs::create_dir(&file).expect("folder");
    match save(&store, &workspace_id, json!({ "token": "unread" })) {
        // Ghostex's own words around the platform's reason.
        Err(WorkSourceError::Refused(text)) => assert!(
            text.starts_with("Ghostex couldn't read the credentials already stored, so it changed nothing: work-mode-credentials.json can't be read (")
                && text.ends_with("); fix it or remove it, and try again"),
            "{text}"
        ),
        other => panic!("not refused: {other:?}"),
    }
    assert!(file.is_dir(), "nothing was written over it");
}

/// A command whose 40-character name and label hold `ж€✓`; once `phase` says `declare-mode`,
/// its `info` fails and declares `mode` secret.
fn capped_names_source(store: &Store, name: &str, label: &str) -> String {
    let phase = store._temp.path().join("phase");
    script(
        store,
        "capped-source",
        &format!(
            r#"if [ "$(cat '{}' 2>/dev/null)" = declare-mode ]; then
printf '{{ "credentials": [ {{ "name": "mode", "label": "Mode" }} ] }}\n'; exit 1
fi
printf '{{ "id": "fixture", "name": "{name}", "ids": {{ "format": "number" }}, "credentials": [ {{ "name": "token", "label": "{label}" }}, {{ "name": "mode", "label": "Mode", "secret": false }} ] }}\n'"#,
            phase.display()
        ),
    )
}

/// Masking can lengthen the name past the 40 characters `parse_info` reads back (`••••` over a
/// 3-character value): the record is still read, whether the record was masked when the secret
/// was saved or the next `info` was masked with it. A label at its cap is cut where it is read.
#[test]
fn a_name_at_its_cap_stays_readable_once_a_secret_in_it_is_masked() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let name = format!("{:x<40}", "Tracker ж€✓ ");
    let label = format!("{:y<40}", "Token ж€✓ ");
    let program = capped_names_source(&store, &name, &label);
    let set = || {
        set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &program,
            false,
            &no_references,
        )
    };
    assert_eq!(set().expect("pinned").info.name, name);
    let saved = save(&store, &workspace_id, json!({ "token": "ж€✓" })).expect("stored");
    assert!(saved.failure.is_none(), "{:?}", saved.failure);
    for _ in 0..2 {
        let command = stored_command(&store, &workspace_id);
        assert!(
            command.info.name.chars().count() <= 40,
            "{}",
            command.info.name
        );
        assert!(
            command.info.name.starts_with("Tracker •••• x"),
            "{}",
            command.info.name
        );
        assert!(!command.info.credentials[0].label.contains("ж€✓"));
        set().expect("checked again");
    }
}

/// A short value stored as not secret, which a failed check then declares secret, is removed: it
/// can't be kept secret. The failure says so first, so does `lastError`, and the record's name
/// holding it is kept as it is, since it is no credential any more.
#[test]
fn a_failed_check_that_marks_a_short_value_secret_removes_it() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let program = capped_names_source(&store, "Tracker ё", "Token");
    let set = || {
        set_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &program,
            false,
            &no_references,
        )
    };
    set().expect("pinned");
    save(&store, &workspace_id, json!({ "mode": "ё" })).expect("not secret");
    std::fs::write(store._temp.path().join("phase"), "declare-mode").expect("phase");
    let Err(WorkSourceError::Failed(failure)) = set() else {
        panic!("not a failed check");
    };
    assert!(
        failure
            .message
            .starts_with(&format!("{SHORT_SECRET_REMOVED} ")),
        "{}",
        failure.message
    );
    let command = stored_command(&store, &workspace_id);
    assert!(is_secret(&command, "mode"));
    assert_eq!(command.last_error, Some(failure.message));
    assert_eq!(command.info.name, "Tracker ё");
    assert!(!Value::Object(store.credentials()).to_string().contains('ё'));
}

/// A credentials file that is there but can't be read is never written over: a credential save,
/// a Linear key save and a workspace deletion each answer with an error and leave it byte for byte as it was. A
/// missing file is still created.
#[test]
fn a_damaged_credentials_file_is_never_written_over() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    let file = store.paths.auth_dir.join("work-mode-credentials.json");
    std::fs::create_dir_all(&store.paths.auth_dir).expect("auth dir");
    for damaged in ["{ \"linearApiKey\": ", "[1, 2]"] {
        std::fs::write(&file, damaged).expect("damage");
        match save(&store, &workspace_id, json!({ "token": "tok-dmg-1" })) {
            Err(WorkSourceError::Refused(text)) => {
                assert!(
                    text.contains("work-mode-credentials.json can't be read"),
                    "{text}"
                )
            }
            other => panic!("not refused: {other:?}"),
        }
        let linear = store_linear_api_key(&store.paths, LinearKeyScope::Shared, Some("lin_api_k"))
            .expect_err("a damaged file");
        assert!(
            linear.to_string().contains("fix it or remove it"),
            "{linear}"
        );
        // A deletion can't tell whether the file holds the workspace's credentials. It runs in a
        // transaction, as the route runs it, which the error rolls back.
        let transaction = store.db.unchecked_transaction().expect("transaction");
        assert!(delete_workspace_work_source(
            &store.db,
            &store.paths,
            &workspace_id,
            &no_references
        )
        .is_err());
        drop(transaction);
        assert!(store.sources().workspaces.contains_key(&workspace_id));
        assert_eq!(std::fs::read_to_string(&file).expect("read"), damaged);
    }
    std::fs::remove_file(&file).expect("remove");
    store_linear_api_key(&store.paths, LinearKeyScope::Shared, Some("lin_api_k"))
        .expect("a missing file is created");
    assert!(store.credentials().contains_key("linearApiKey"));
}

/// A file whose root is an object but which holds a container a change writes into that isn't
/// one is never written over: one case per container and depth, each change answering with an
/// error that names the container's level, never a key read from the file. A key Ghostex doesn't
/// know stays as it is.
#[test]
fn a_credentials_file_with_a_misshapen_container_is_never_written_over() {
    let store = Store::new();
    store.answer("good");
    let workspace_id = store.ws("ws-a");
    store.set("ws-a", false).expect("pinned");
    let file = store.paths.auth_dir.join("work-mode-credentials.json");
    std::fs::create_dir_all(&store.paths.auth_dir).expect("auth dir");
    let linear = |scope| store_linear_api_key(&store.paths, scope, Some("lin_api_k"));
    let cases: [(String, &str, Box<dyn Fn() -> String>); 5] = [
        (
            r#"{"projectLinearApiKeys":[{"P1":"lin_api_p1"}]}"#.to_string(),
            "projectLinearApiKeys",
            Box::new(|| {
                linear(LinearKeyScope::Project("P2"))
                    .expect_err("misshapen")
                    .to_string()
            }),
        ),
        (
            r#"{"workspaceLinearApiKeys":"lin_api_w"}"#.to_string(),
            "workspaceLinearApiKeys",
            Box::new(|| {
                linear(LinearKeyScope::Workspace("ws-b"))
                    .expect_err("misshapen")
                    .to_string()
            }),
        ),
        (
            r#"{"workspaceSourceCredentials":[1]}"#.to_string(),
            "workspaceSourceCredentials",
            Box::new(|| {
                refused(save(
                    &store,
                    &workspace_id,
                    json!({ "token": "tok-shape-1" }),
                ))
            }),
        ),
        (
            r#"{"workspaceSourceCredentials":{"tok-key-leak-7":"fixture"}}"#.to_string(),
            "workspaceSourceCredentials has a workspace that isn't a JSON object",
            Box::new(|| {
                refused(save(
                    &store,
                    &workspace_id,
                    json!({ "token": "tok-shape-1" }),
                ))
            }),
        ),
        (
            format!(
                r#"{{"workspaceSourceCredentials":{{"{workspace_id}":{{"tok-src-leak-8":["tok-old"]}}}}}}"#
            ),
            "workspaceSourceCredentials has a source that isn't a JSON object",
            Box::new(|| {
                refused(save(
                    &store,
                    &workspace_id,
                    json!({ "token": "tok-shape-1" }),
                ))
            }),
        ),
    ];
    for (damaged, named, change) in cases {
        std::fs::write(&file, &damaged).expect("damage");
        let error = change();
        assert!(error.contains("can't be read"), "{error}");
        assert!(error.contains(named), "{named}: {error}");
        // A key read from a damaged file can be any text, a credential among them.
        assert!(!error.contains("leak"), "{error}");
        assert_eq!(std::fs::read_to_string(&file).expect("read"), damaged);
    }
    std::fs::write(&file, r#"{"laterKey":[1,2]}"#).expect("unknown key");
    linear(LinearKeyScope::Shared).expect("an unknown key is no container Ghostex writes");
    assert_eq!(store.credentials()["laterKey"], json!([1, 2]));
}

fn refused(result: Result<CredentialsSaved, WorkSourceError>) -> String {
    match result {
        Err(WorkSourceError::Refused(text)) => text,
        other => panic!("not refused: {other:?}"),
    }
}

fn is_secret(command: &WorkspaceCommand, name: &str) -> bool {
    command
        .info
        .credentials
        .iter()
        .any(|field| field.name == name && field.secret)
}

fn stored_value(store: &Store, workspace_id: &str, name: &str) -> Option<String> {
    Value::Object(store.credentials())["workspaceSourceCredentials"][workspace_id]["fixture"]
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[test]
fn deleting_a_workspace_removes_its_command_and_credentials() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    store.set("ws-b", false).expect("pinned");
    for workspace in ["ws-a", "ws-b"] {
        set_work_source_credentials(
            &store.db,
            &store.paths,
            &store.ws(workspace),
            json!({ "token": "tok-delete" }).as_object().unwrap(),
        )
        .expect("stored");
    }
    delete_workspace_work_source(&store.db, &store.paths, &store.ws("ws-a"), &no_references)
        .expect("deleted")
        .commit();
    let sources = store.sources();
    assert!(!sources.workspaces.contains_key(&store.ws("ws-a")));
    assert!(sources.retired.is_empty(), "ws-b still pins it");
    let credentials = store.credentials();
    assert!(credentials["workspaceSourceCredentials"]
        .get(&store.ws("ws-a"))
        .is_none());
    assert!(credentials["workspaceSourceCredentials"]
        .get(&store.ws("ws-b"))
        .is_some());
    delete_workspace_work_source(&store.db, &store.paths, &store.ws("ws-b"), &no_references)
        .expect("deleted")
        .commit();
    assert_eq!(store.sources().retired[0].id, "fixture");
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
}

fn retired(index: usize) -> RetiredSource {
    RetiredSource {
        id: format!("old-{index}"),
        format: IdFormat::Number,
        name: format!("Old {index}"),
        icon: None,
        retired_at: format!("2026-10-{:02}T00:00:00Z", 1 + index % 28),
    }
}

#[test]
fn retiring_never_fails_past_the_pruning_target_and_keeps_referenced_ids() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let mut sources = store.sources();
    sources.retired = (0..64).map(retired).collect();
    write_work_sources(&store.db, &sources).expect("seed");
    // Every id is still named by a saved link, the one being retired too.
    let referenced: BTreeSet<String> = (0..64)
        .map(|index| format!("old-{index}"))
        .chain(["fixture".to_string()])
        .collect();
    delete_workspace_work_source(&store.db, &store.paths, &store.ws("ws-a"), &|| {
        Ok(referenced.clone())
    })
    .expect("a delete never fails on a bound")
    .commit();
    let sources = store.sources();
    assert_eq!(sources.retired.len(), 65);
    assert_eq!(sources.retired.last().unwrap().id, "fixture");

    // With nothing referring to them, the oldest go until 64 are left.
    let mut sources = store.sources();
    sources.retired.push(retired(99));
    write_work_sources(&store.db, &sources).expect("seed");
    store.set("ws-b", false).expect("pinned again");
    let mut kept: BTreeSet<String> = BTreeSet::new();
    kept.insert("old-63".to_string());
    clear_work_source(&store.db, &store.ws("ws-b"), &|| Ok(kept.clone())).expect("cleared");
    let sources = store.sources();
    assert_eq!(sources.retired.len(), 64);
    assert!(sources.retired.iter().any(|retired| retired.id == "old-63"));
    assert!(!sources.retired.iter().any(|retired| retired.id == "old-0"));
}

#[test]
fn only_a_new_id_can_hit_the_bound() {
    let store = Store::new();
    let mut sources = WorkSources::default();
    sources.retired = (0..MAX_SOURCE_IDS).map(retired).collect();
    write_work_sources(&store.db, &sources).expect("seed");
    store.answer("good");
    assert!(refusal(store.set("ws-a", false)).contains("too many ticket source ids"));
    // An id the record already knows is no new one.
    let mut sources = store.sources();
    sources.retired[0] = RetiredSource {
        id: "fixture".to_string(),
        format: IdFormat::KeyNumber,
        ..retired(0)
    };
    write_work_sources(&store.db, &sources).expect("seed");
    let command = store.set("ws-a", false).expect("a known id pins");
    assert_eq!(command.source_id, "fixture");
    assert!(!store
        .sources()
        .retired
        .iter()
        .any(|retired| retired.id == "fixture"));
}

#[test]
fn every_change_bumps_the_workspace_generation() {
    let store = Store::new();
    store.answer("good");
    let before = work_source_generation(&store.ws("ws-gen"));
    store.set("ws-gen", false).expect("pinned");
    let pinned = work_source_generation(&store.ws("ws-gen"));
    assert!(pinned > before);
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &store.ws("ws-gen"),
        json!({ "token": "tok-gen" }).as_object().unwrap(),
    )
    .expect("stored");
    let stored = work_source_generation(&store.ws("ws-gen"));
    assert!(stored > pinned);
    change_work_source_config(&store.ws("ws-gen"), || {});
    assert_eq!(work_source_generation(&store.ws("ws-gen")), stored + 1);
}

#[test]
fn a_typed_path_resolves_to_an_absolute_one() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    std::fs::write(bin.join("fixture-source"), "#!/bin/sh\n").expect("write");
    let search = std::ffi::OsString::from(format!("/nowhere:{}", bin.display()));
    let resolve = |typed: &str| resolve_command_path_in(typed, &home, &search);
    assert_eq!(resolve("~/bin/source"), Ok(home.join("bin/source")));
    assert_eq!(resolve(" ~ "), Ok(home.clone()));
    assert_eq!(resolve("/opt/source"), Ok(PathBuf::from("/opt/source")));
    assert_eq!(resolve("fixture-source"), Ok(bin.join("fixture-source")));
    assert!(resolve("missing-source").unwrap_err().contains("on PATH"));
    assert!(resolve("bin/source").unwrap_err().contains("is relative"));
    assert!(resolve("~ana/source").unwrap_err().contains("use ~/"));
    assert!(resolve("  ").is_err());
}

/// A one-off `info` program in the test's folder.
fn script(store: &Store, name: &str, body: &str) -> String {
    let program = store._temp.path().join(name);
    std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).expect("write script");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    program.display().to_string()
}

const GOOD_INFO: &str = r#"{ "id": "fixture", "name": "Fixture", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "token", "label": "API token" } ] }"#;

/// The marker a slow program writes just before it sleeps, so a test knows `info` is running.
fn running_marker(store: &Store) -> PathBuf {
    store._temp.path().join("info-running")
}

/// Waits until the program wrote its marker: `info` is running from then on.
fn wait_until_running(marker: &std::path::Path) {
    let started = std::time::Instant::now();
    while !marker.exists() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "info never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A program that writes its marker, sleeps, then answers `GOOD_INFO`.
fn slow_good_info(store: &Store) -> String {
    let marker = running_marker(store);
    script(
        store,
        "slow-info",
        &format!(
            "touch '{}'\nsleep 1\ncat <<'EOF'\n{GOOD_INFO}\nEOF",
            marker.display()
        ),
    )
}

/// Sets `program` for the workspace on another thread while `meanwhile` runs on this one, once
/// `info` is running, and returns what the set answered.
fn set_while(
    store: &Store,
    workspace_id: &str,
    program: &str,
    meanwhile: impl FnOnce(),
) -> Result<WorkspaceCommand, WorkSourceError> {
    std::thread::scope(|scope| {
        let paths = store.paths.clone();
        let set = scope.spawn(move || {
            let db = open_gxserver_database(&paths).expect("db");
            set_work_source(&db, &paths, workspace_id, program, false, &no_references)
        });
        wait_until_running(&running_marker(store));
        meanwhile();
        set.join().expect("set thread")
    })
}

#[test]
fn a_team_join_while_info_runs_commits_nothing() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let program = slow_good_info(&store);
    let result = set_while(&store, &workspace_id, &program, || {
        change_work_source_config(&workspace_id, || {
            mutate_credentials(&store.paths, |credentials| {
                credentials.insert(
                    "teamSync".to_string(),
                    json!({ &workspace_id: { "deploymentUrl": "https://team.example", "memberToken": "member" } }),
                );
            })
        })
        .expect("joined");
    });
    assert!(
        matches!(result, Err(WorkSourceError::Refused(_))),
        "{result:?}"
    );
    assert!(!store.sources().workspaces.contains_key(&workspace_id));
}

#[test]
fn a_workspace_deleted_while_info_runs_commits_nothing() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let program = slow_good_info(&store);
    let result = set_while(&store, &workspace_id, &program, || {
        delete_workspace_work_source(&store.db, &store.paths, &workspace_id, &no_references)
            .expect("cleaned up")
            .commit();
        let current = crate::workspaces::read_sidebar_workspaces(&store.db).expect("read");
        let next = crate::workspaces::delete_workspace_in(&current, &workspace_id).expect("delete");
        crate::workspaces::write_sidebar_workspaces(&store.db, &next).expect("write");
    });
    assert!(
        matches!(result, Err(WorkSourceError::Refused(_))),
        "{result:?}"
    );
    assert!(!store.sources().workspaces.contains_key(&workspace_id));
}

#[test]
fn a_field_a_failed_check_declared_secret_is_never_shown_back() {
    let store = Store::new();
    // Not secret in the accepted info; declared secret, in an answer the contract refuses, once
    // the token is set.
    let program = script(
        &store,
        "flip-info",
        r#"if [ -n "$GHOSTEX_CREDENTIAL_TOKEN" ]; then
  printf '{ "id": "fixture", "name": "Fixture", "credentials": [ { "name": "token", "label": "API token" } ] }\n'
else
  printf '{ "id": "fixture", "name": "Fixture", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "API token", "secret": false } ] }\n'
fi"#,
    );
    let workspace_id = store.ws("ws-a");
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    let CredentialsSaved {
        command, failure, ..
    } = set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "tok-open-7104" }).as_object().unwrap(),
    )
    .expect("stored");
    assert!(failure.is_some(), "the check failed");
    assert!(command.info.credentials[0].secret, "tightened");
    let summary = work_source_summary(&store.paths, &workspace_id, Some(&command), false);
    assert!(!summary.to_string().contains("tok-open-7104"), "{summary}");
    assert_eq!(summary["command"]["credentials"][0]["set"], json!(true));
    assert!(
        summary["command"]["credentials"][0].get("value").is_none(),
        "{summary}"
    );
}

#[test]
fn a_value_that_appears_in_what_cant_be_masked_is_refused() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    for (value, what) in [
        ("fixture", "source id"),
        ("ACM", "ID keys"),
        (workspace_id.as_str(), "workspace id"),
        ("fixture-sour", "path"),
        ("mode", "credential names"),
        ("token", "credential names"),
    ] {
        match set_work_source_credentials(
            &store.db,
            &store.paths,
            &workspace_id,
            json!({ "token": value }).as_object().unwrap(),
        ) {
            Err(WorkSourceError::Refused(text)) => {
                assert!(text.contains(what), "{value}: {text}");
                assert!(!text.contains(value), "{text}");
            }
            other => panic!("{value}: not refused: {other:?}"),
        }
    }
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
}

#[test]
fn a_new_secret_is_masked_in_what_the_record_already_keeps() {
    let store = Store::new();
    let program = script(
        &store,
        "named-info",
        r#"printf '{ "id": "fixture", "name": "Fixture s3cr3t-name", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "API token" } ] }\n'"#,
    );
    let workspace_id = store.ws("ws-a");
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    // The check that follows the credential fails, so nothing replaces the record's info.
    std::fs::remove_file(&program).expect("remove");
    let CredentialsSaved {
        command, failure, ..
    } = set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "s3cr3t-name" }).as_object().unwrap(),
    )
    .expect("stored");
    assert!(failure.is_some());
    let row: String = store
        .db
        .query_row(
            "SELECT value FROM metadata WHERE key = 'workTicketSources'",
            [],
            |row| row.get(0),
        )
        .expect("row");
    assert!(!row.contains("s3cr3t-name"), "{row}");
    assert_eq!(command.info.name, "Fixture ••••");
    let summary = work_source_summary(&store.paths, &workspace_id, Some(&command), false);
    assert!(!summary.to_string().contains("s3cr3t-name"), "{summary}");
}

#[test]
fn a_check_clears_what_an_earlier_one_left_behind() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    store.answer("info-renamed");
    refusal(store.set("ws-a", false));
    assert!(store.sources().workspaces[&workspace_id]
        .offered_id
        .is_some());
    // An accepted check with the pinned id clears the offer.
    store.answer("good");
    let CredentialsSaved { command, .. } = set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "tok-clear-1" }).as_object().unwrap(),
    )
    .expect("stored");
    assert_eq!((command.offered_id, command.last_error), (None, None));
    // So does a failed one.
    store.answer("info-renamed");
    refusal(store.set("ws-a", false));
    store.answer("error-silent");
    let CredentialsSaved {
        command, failure, ..
    } = set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "tok-clear-2" }).as_object().unwrap(),
    )
    .expect("stored");
    assert!(failure.is_some());
    assert_eq!(command.offered_id, None);
}

#[test]
fn a_failed_reference_read_is_an_error_not_no_references() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    let failed = clear_work_source(&store.db, &workspace_id, &|| {
        Err(crate::domain::DomainStateError::bad_request("no sessions"))
    });
    assert!(
        matches!(failed, Err(WorkSourceError::Storage(_))),
        "{failed:?}"
    );
    assert!(
        store.sources().workspaces.contains_key(&workspace_id),
        "nothing changed"
    );
}

#[test]
fn the_summary_masks_every_text_it_returns() {
    let store = Store::new();
    store.answer("good");
    let command = store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    // A secret that the record keeps as it is: inside the time it was last checked.
    let secret = command.checked_at[..7].to_string();
    let CredentialsSaved { command, .. } = set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": secret }).as_object().unwrap(),
    )
    .expect("stored");
    let summary = work_source_summary(&store.paths, &workspace_id, Some(&command), false);
    assert!(!summary.to_string().contains(&secret), "{summary}");
    assert!(summary["command"]["checkedAt"]
        .as_str()
        .unwrap()
        .starts_with("••••"));
}

#[test]
fn a_team_join_while_a_credential_check_runs_keeps_its_answer_out() {
    let store = Store::new();
    let keyed = r#"
  sleep 1
  printf '{ "id": "fixture", "name": "Fixture after", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "token", "label": "API token" } ] }\n'
else
  printf '{ "id": "fixture", "name": "Fixture", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "token", "label": "API token" } ] }\n'
fi"#;
    let marker = running_marker(&store);
    let program = script(
        &store,
        "slow-when-keyed",
        &format!(
            "if [ -n \"$GHOSTEX_CREDENTIAL_TOKEN\" ]; then\n  touch '{}'{keyed}",
            marker.display()
        ),
    );
    let workspace_id = store.ws("ws-a");
    set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    let result = std::thread::scope(|scope| {
        let paths = store.paths.clone();
        let workspace = workspace_id.clone();
        let stored = scope.spawn(move || {
            let db = open_gxserver_database(&paths).expect("db");
            set_work_source_credentials(
                &db,
                &paths,
                &workspace,
                json!({ "token": "tok-team-race" }).as_object().unwrap(),
            )
        });
        wait_until_running(&marker);
        change_work_source_config(&workspace_id, || {
            mutate_credentials(&store.paths, |credentials| {
                credentials.insert(
                    "teamSync".to_string(),
                    json!({ &workspace_id: { "deploymentUrl": "https://team.example", "memberToken": "member" } }),
                );
            })
        })
        .expect("joined");
        stored.join().expect("thread")
    });
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        store.sources().workspaces[&workspace_id].info.name,
        "Fixture"
    );
}

/// The raw `workTicketSources` row.
fn stored_command(store: &Store, workspace_id: &str) -> WorkspaceCommand {
    read_work_sources(&store.db)
        .expect("read")
        .workspaces
        .remove(workspace_id)
        .expect("a command")
}

fn stored_row(store: &Store) -> String {
    store
        .db
        .query_row(
            "SELECT value FROM metadata WHERE key = 'workTicketSources'",
            [],
            |row| row.get(0),
        )
        .expect("row")
}

/// An `info` whose name holds the value of `mode`, a field it declares not secret.
const OPEN_MODE_INFO: &str = r#"printf '{ "id": "fixture", "name": "Fixture vis-val-77", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false } ] }\n'"#;

/// Pins `OPEN_MODE_INFO` and stores `mode`, which then shows as it is (it isn't secret).
fn pin_with_open_mode(store: &Store, workspace_id: &str) -> String {
    let program = script(store, "open-mode", OPEN_MODE_INFO);
    set_work_source(
        &store.db,
        &store.paths,
        workspace_id,
        &program,
        false,
        &no_references,
    )
    .expect("pinned");
    set_work_source_credentials(
        &store.db,
        &store.paths,
        workspace_id,
        json!({ "mode": "vis-val-77" }).as_object().unwrap(),
    )
    .expect("stored");
    assert!(
        stored_row(store).contains("vis-val-77"),
        "not secret yet: kept as it is"
    );
    program
}

#[test]
fn a_value_that_turns_secret_never_stays_raw_in_the_record() {
    // A failed check that declares nothing (the program is gone) changes no flag: `mode` isn't
    // secret, so its value stays as it is.
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    let program = pin_with_open_mode(&store, &workspace_id);
    std::fs::remove_file(&program).expect("remove");
    let failed = set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &program,
        false,
        &no_references,
    );
    assert!(
        matches!(failed, Err(WorkSourceError::Failed(_))),
        "{failed:?}"
    );
    assert!(stored_row(&store).contains("vis-val-77"));
    assert!(!is_secret(&stored_command(&store, &workspace_id), "mode"));

    // A failed check that declares `mode` secret: it is secret from then on, and its value never
    // stays raw in the record.
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    pin_with_open_mode(&store, &workspace_id);
    let failing = script(
        &store,
        "failing",
        r#"printf '{ "credentials": [ { "name": "mode", "label": "Mode" } ] }\n'
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
    assert!(
        !stored_row(&store).contains("vis-val-77"),
        "{}",
        stored_row(&store)
    );
    assert!(is_secret(&stored_command(&store, &workspace_id), "mode"));

    // An answer with another id that declares `mode` secret, refused for a Switch.
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    pin_with_open_mode(&store, &workspace_id);
    let renamed = script(
        &store,
        "renamed",
        r#"printf '{ "id": "renamed", "name": "Renamed", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "mode", "label": "Mode" } ] }\n'"#,
    );
    refusal(set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &renamed,
        false,
        &no_references,
    ));
    assert!(
        !stored_row(&store).contains("vis-val-77"),
        "{}",
        stored_row(&store)
    );

    // The same id with another id format, which declares `mode` secret, refused as a format change.
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    pin_with_open_mode(&store, &workspace_id);
    let numbered = script(
        &store,
        "numbered",
        r#"printf '{ "id": "fixture", "name": "Fixture", "ids": { "format": "number" }, "credentials": [ { "name": "mode", "label": "Mode" } ] }\n'"#,
    );
    let refused = refusal(set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &numbered,
        false,
        &no_references,
    ));
    assert!(refused.contains("sends number ids now"), "{refused}");
    let row = stored_row(&store);
    assert!(!row.contains("vis-val-77"), "{row}");
    let command = store.sources().workspaces[&workspace_id].clone();
    assert!(command.info.credentials[0].secret, "mode is secret now");
    let summary = work_source_summary(&store.paths, &workspace_id, Some(&command), false);
    assert!(!summary.to_string().contains("vis-val-77"), "{summary}");
}

#[test]
fn a_rolled_back_deletion_leaves_the_command_and_its_credentials() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "tok-undo-5" }).as_object().unwrap(),
    )
    .expect("stored");
    let before = (store.sources(), store.credentials());
    let transaction = store.db.unchecked_transaction().expect("transaction");
    let deletion =
        delete_workspace_work_source(&transaction, &store.paths, &workspace_id, &no_references)
            .expect("deleted");
    assert!(!store
        .credentials()
        .contains_key("workspaceSourceCredentials"));
    // What follows fails: the transaction rolls back and the credentials are put back.
    drop(transaction);
    deletion.undo(&store.paths).expect("undone");
    assert_eq!((store.sources(), store.credentials()), before);
}

#[test]
fn a_deletion_holds_the_writer_until_it_is_committed_or_undone() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    // No change to the workspace's command or credentials can land while a deletion is open.
    let transaction = store.db.unchecked_transaction().expect("transaction");
    let deletion =
        delete_workspace_work_source(&transaction, &store.paths, &workspace_id, &no_references)
            .expect("deleted");
    assert!(work_source_writer_held());
    transaction.commit().expect("commit");
    deletion.commit();
    // Undone, it is released too.
    store.set("ws-b", false).expect("pinned");
    let other = store.ws("ws-b");
    let transaction = store.db.unchecked_transaction().expect("transaction");
    let deletion = delete_workspace_work_source(&transaction, &store.paths, &other, &no_references)
        .expect("deleted");
    assert!(work_source_writer_held());
    drop(transaction);
    deletion.undo(&store.paths).expect("undone");
}

#[test]
fn a_write_waiting_on_a_deletion_lands_after_its_undo() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    set_work_source_credentials(
        &store.db,
        &store.paths,
        &workspace_id,
        json!({ "token": "tok-before" }).as_object().unwrap(),
    )
    .expect("stored");
    let transaction = store.db.unchecked_transaction().expect("transaction");
    let deletion =
        delete_workspace_work_source(&transaction, &store.paths, &workspace_id, &no_references)
            .expect("deleted");
    std::thread::scope(|scope| {
        let paths = store.paths.clone();
        let workspace = workspace_id.clone();
        let (started, waiting) = std::sync::mpsc::channel();
        let writer = scope.spawn(move || {
            started.send(()).expect("signal");
            change_work_source_config(&workspace, || {
                mutate_credentials(&paths, |credentials| {
                    store_source_credentials(
                        credentials,
                        &workspace,
                        "fixture",
                        &[("token".to_string(), Some("tok-after".to_string()))],
                    )
                })
            })
        });
        waiting.recv().expect("started");
        // The writer waits on the store's lock; the deletion fails and is undone first.
        drop(transaction);
        deletion.undo(&store.paths).expect("undone");
        writer.join().expect("thread").expect("written");
    });
    assert_eq!(
        store.credentials()["workspaceSourceCredentials"][&workspace_id]["fixture"]["token"],
        json!("tok-after"),
        "the undo put back the old value before the waiting write, never over it"
    );
}

#[test]
fn deleting_a_workspace_without_command_credentials_never_writes_the_file() {
    let store = Store::new();
    let workspace_id = store.ws("ws-a");
    mutate_credentials(&store.paths, |credentials| {
        credentials.insert("linearApiKey".to_string(), json!("lin_api_kept"));
    })
    .expect("seed");
    std::fs::set_permissions(
        &store.paths.auth_dir,
        std::fs::Permissions::from_mode(0o500),
    )
    .expect("chmod");
    let deleted =
        delete_workspace_work_source(&store.db, &store.paths, &workspace_id, &no_references);
    std::fs::set_permissions(
        &store.paths.auth_dir,
        std::fs::Permissions::from_mode(0o700),
    )
    .expect("chmod");
    deleted
        .expect("nothing to remove, nothing written")
        .commit();
}

#[test]
fn a_failed_check_carries_what_the_log_line_needs_masked() {
    let store = Store::new();
    store.answer("good");
    store.set("ws-a", false).expect("pinned");
    let workspace_id = store.ws("ws-a");
    let missing = store
        ._temp
        .path()
        .join("missing-source")
        .display()
        .to_string();
    match set_work_source(
        &store.db,
        &store.paths,
        &workspace_id,
        &missing,
        false,
        &no_references,
    ) {
        Err(WorkSourceError::Failed(failure)) => {
            assert!(
                failure.message.contains("can't find"),
                "{}",
                failure.message
            );
            assert_eq!(failure.workspace_id, workspace_id);
            assert_eq!(failure.source_id.as_deref(), Some("fixture"));
            assert!(failure.duration < std::time::Duration::from_secs(10));
        }
        other => panic!("not a failed check: {other:?}"),
    }
}
