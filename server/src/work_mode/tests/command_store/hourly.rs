//! The hourly `info` check: the card pass queues it for a workspace whose accepted `info` is an
//! hour old, and a worker checks the command again as `ghostex issues source check` does, holding
//! the store's writer lock until `info` starts. It refuses, removes and words things as that check
//! does, writes the record only when something changed, and isn't tried again for an hour when it
//! changed nothing. Each test pins ids of its own.

use std::sync::mpsc;
use std::time::Duration;

use super::cards::{
    calls, card_pass, log_lines, pin, plan_of, project_id, project_in, release, store_values,
    token_of, HANDSHAKE,
};
use super::published::{command_source, record_pinning};
use super::*;

/// A time an hour and more ago, when a test pins the command.
const LONG_AGO: &str = "2026-01-01T00:00:00.000Z";

/// Pins the fixture in `workspace` as `id`, last checked long ago, answering its hourly check as
/// `variant` of the `good` info. Returns the workspace's id.
fn pin_due(store: &Store, workspace: &str, id: &str, variant: &str) -> String {
    let mut sources = record_pinning(store, workspace, id, IdFormat::KeyNumber);
    let workspace_id = store.ws(workspace);
    let command = sources.workspaces.get_mut(&workspace_id).expect("pinned");
    command.checked_at = LONG_AGO.to_string();
    save_work_sources(&store.db, &sources).expect("pinned");
    store_values(
        &store.paths,
        &workspace_id,
        id,
        &token_of(id),
        &format!("hourly {id} {variant}"),
    );
    workspace_id
}

fn store_value(store: &Store, workspace_id: &str, id: &str, name: &str, value: &str) {
    mutate_credentials(&store.paths, |credentials| {
        let workspaces = credentials
            .entry("workspaceSourceCredentials".to_string())
            .or_insert_with(|| json!({}));
        workspaces[workspace_id][id][name] = json!(value);
    })
    .expect("stored");
}

fn stored(store: &Store, workspace_id: &str, id: &str, name: &str) -> Option<String> {
    Value::Object(store.credentials())["workspaceSourceCredentials"][workspace_id][id]
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn record(store: &Store, workspace_id: &str) -> WorkspaceCommand {
    read_work_sources(&store.db)
        .expect("record")
        .workspaces
        .remove(workspace_id)
        .expect("pinned")
}

fn check(store: &Store, workspace_id: &str, id: &str) {
    run_info_check(&InfoCheck {
        plan: plan_of(workspace_id, id),
        paths: store.paths.clone(),
    });
}

#[test]
fn an_hourly_check_that_finds_the_same_command_keeps_the_cards_reads() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-same", "same");
    let before = record(&store, &workspace_id);
    let generation = work_source_generation(&workspace_id);
    check(&store, &workspace_id, "hourly-same");
    let after = record(&store, &workspace_id);
    assert_ne!(after.checked_at, LONG_AGO);
    assert_eq!(
        after,
        WorkspaceCommand {
            checked_at: after.checked_at.clone(),
            ..before
        }
    );
    assert_eq!(work_source_generation(&workspace_id), generation);
}

#[test]
fn an_hourly_check_keeps_what_the_command_now_answers() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-renamed", "renamed");
    let generation = work_source_generation(&workspace_id);
    check(&store, &workspace_id, "hourly-renamed");
    assert_eq!(record(&store, &workspace_id).info.name, "Fixture Two");
    assert_ne!(work_source_generation(&workspace_id), generation);
}

/// Another id format is refused as a check refuses it: the record keeps its accepted `info`, and
/// one line in fixed words says so.
#[test]
fn an_hourly_check_refuses_another_id_format() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-format", "number");
    let generation = work_source_generation(&workspace_id);
    check(&store, &workspace_id, "hourly-format");
    let kept = record(&store, &workspace_id);
    assert_eq!(kept.info.id_format, IdFormat::KeyNumber);
    assert_eq!(kept.checked_at, LONG_AGO);
    assert_eq!(work_source_generation(&workspace_id), generation);
    let lines = log_lines(&store, "work_source_check_refused");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].contains("number"), "{lines:?}");
}

/// An ID key holding a value the workspace stores is refused, as a check refuses it, and the
/// record keeps its keys.
#[test]
fn an_hourly_check_refuses_a_key_holding_a_stored_value() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-keys", "keys-zzq");
    store_value(&store, &workspace_id, "hourly-keys", "team", "ZZQ");
    check(&store, &workspace_id, "hourly-keys");
    assert_eq!(record(&store, &workspace_id).info.id_keys, ["ACM"]);
    // The runner refuses it before the store sees it, as for a check someone asks for.
    let lines = log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].contains("ZZQ"), "{lines:?}");
}

/// A field the answer newly marks secret has its short saved value removed, and the record says
/// why, as a check does.
#[test]
fn an_hourly_check_removes_a_short_value_it_makes_secret() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-pin", "pin-secret");
    store_value(&store, &workspace_id, "hourly-pin", "pin", "42");
    check(&store, &workspace_id, "hourly-pin");
    assert_eq!(stored(&store, &workspace_id, "hourly-pin", "pin"), None);
    assert_eq!(
        stored(&store, &workspace_id, "hourly-pin", "token").as_deref(),
        Some(token_of("hourly-pin").as_str())
    );
    let kept = record(&store, &workspace_id);
    assert_eq!(kept.last_error.as_deref(), Some(SHORT_SECRET_REMOVED));
    assert!(kept
        .info
        .credentials
        .iter()
        .any(|field| field.name == "pin" && field.secret));
}

/// A failing check is logged once, without the command's words, and the card pass doesn't ask
/// again within the hour.
#[test]
fn a_failing_hourly_check_waits_an_hour_before_the_next() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-fail", "fail");
    let plan = plan_of(&workspace_id, "hourly-fail");
    assert!(info_check_wanted(&plan, LONG_AGO));
    check(&store, &workspace_id, "hourly-fail");
    let lines = log_lines(&store, "work_source_failed");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(r#""verb":"info""#), "{lines:?}");
    assert!(!lines[0].contains("tracker is down"), "{lines:?}");
    assert_eq!(record(&store, &workspace_id).checked_at, LONG_AGO);
    assert!(!info_check_wanted(
        &plan_of(&workspace_id, "hourly-fail"),
        LONG_AGO
    ));
    // The same failure again changes nothing the cards read.
    let generation = work_source_generation(&workspace_id);
    check(&store, &workspace_id, "hourly-fail");
    assert_eq!(work_source_generation(&workspace_id), generation);
}

/// A check that can't open the database asks nothing, and isn't queued again for an hour either.
#[test]
fn an_hourly_check_without_its_database_waits_an_hour_before_the_next() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-no-db", "good");
    assert!(info_check_wanted(
        &plan_of(&workspace_id, "hourly-no-db"),
        LONG_AGO
    ));
    let mut paths = store.paths.clone();
    paths.state_db_file = store.paths.app_data_dir.join("not-a-database");
    std::fs::create_dir_all(&paths.state_db_file).expect("a folder where the database would be");
    run_info_check(&InfoCheck {
        plan: plan_of(&workspace_id, "hourly-no-db"),
        paths,
    });
    assert!(calls(&store, "hourly-no-db").is_empty());
    assert!(!info_check_wanted(
        &plan_of(&workspace_id, "hourly-no-db"),
        LONG_AGO
    ));
}

/// A check that can't read the ticket command record (while the card pass still plans from its
/// copy in memory) asks nothing, logs once, and isn't queued again for an hour either.
#[test]
fn an_hourly_check_that_cant_read_the_record_waits_an_hour_before_the_next() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-unread", "good");
    store
        .db
        .execute(
            "UPDATE metadata SET value = 'not a record' WHERE key = 'workTicketSources'",
            [],
        )
        .expect("damaged");
    assert!(info_check_wanted(
        &plan_of(&workspace_id, "hourly-unread"),
        LONG_AGO
    ));
    check(&store, &workspace_id, "hourly-unread");
    assert!(calls(&store, "hourly-unread").is_empty());
    assert_eq!(log_lines(&store, "work_source_check_refused").len(), 1);
    assert!(!info_check_wanted(
        &plan_of(&workspace_id, "hourly-unread"),
        LONG_AGO
    ));
}

#[test]
fn a_card_pass_queues_the_check_only_when_it_is_due() {
    let store = Store::new();
    let due = pin_due(&store, "ws-a", "hourly-due", "same");
    let project = project_in(&store, &due, json!({}));
    let fresh = pin(&store, "ws-b", "hourly-fresh", "good");
    let other = project_in(&store, &fresh, json!({}));
    let credentials = WorkCredentials::new(&store.paths);
    let planned = |id: &str, project: &Value| {
        let WorkSourceId::Command(source) = command_source(id) else {
            unreachable!("a command id")
        };
        let projects: HashMap<&str, &Value> = HashMap::from([(project_id(project), project)]);
        source
            .source()
            .info_checks_for(&TicketPass {
                paths: &store.paths,
                credentials: &credentials,
                projects: &projects,
                sessions: &[],
            })
            .len()
    };
    assert_eq!(planned("hourly-due", &project), 1);
    assert_eq!(planned("hourly-fresh", &other), 0);
    // A check planned for a command that isn't due by the time it runs asks nothing.
    check(&store, &fresh, "hourly-fresh");
    assert!(!calls(&store, "hourly-fresh").contains(&"info".to_string()));
    // The pass queues the due one for a worker.
    card_pass(&store, &project, &[], "hourly-due");
    let deadline = std::time::Instant::now() + HANDSHAKE;
    while record(&store, &due).checked_at == LONG_AGO {
        assert!(std::time::Instant::now() < deadline, "the check never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The check holds the store's writer lock from its plan until `info` starts: a credential
/// change waits for the start, then lands, and the check keeps nothing it read.
#[test]
fn a_change_waits_for_the_hourly_check_to_start_and_the_check_keeps_nothing() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-locked", "renamed");
    let (reached, paused) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    hook_before_spawn(&workspace_id, BeforeSpawn::Pause { reached, release });
    let checking = {
        let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            run_info_check(&InfoCheck {
                plan: plan_of(&workspace_id, "hourly-locked"),
                paths,
            })
        })
    };
    paused.recv_timeout(HANDSHAKE).expect("planned");
    assert!(work_source_writer_held());
    let (changed, change_done) = mpsc::channel();
    {
        let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            change_work_source_config(&workspace_id, || {
                mutate_credentials(&paths, |credentials| {
                    let workspaces = credentials
                        .entry("workspaceSourceCredentials".to_string())
                        .or_insert_with(|| json!({}));
                    workspaces[&workspace_id]["hourly-locked"]["token"] =
                        json!("tok-hourly-locked-new");
                })
                .expect("stored")
            })
            .expect("changed");
            let _ = changed.send(());
        });
    }
    // Planned, not started: the change waits.
    assert!(change_done
        .recv_timeout(Duration::from_millis(200))
        .is_err());
    resume.send(()).expect("resume");
    change_done.recv_timeout(HANDSHAKE).expect("changed");
    checking.join().expect("checked");
    unhook_before_spawn(&workspace_id);
    let kept = record(&store, &workspace_id);
    assert_eq!(kept.info.name, "Fixture");
    assert_eq!(kept.checked_at, LONG_AGO);
}

/// A job planned at an older generation runs nothing and keeps the current generation's check due.
#[test]
fn a_stale_hourly_check_leaves_the_current_one_due() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-stale", "fail");
    let stale = plan_of(&workspace_id, "hourly-stale");
    change_work_source_config(&workspace_id, || ()).expect("changed");
    run_info_check(&InfoCheck {
        plan: stale,
        paths: store.paths.clone(),
    });
    assert!(!calls(&store, "hourly-stale").contains(&"info".to_string()));
    assert!(info_check_wanted(
        &plan_of(&workspace_id, "hourly-stale"),
        LONG_AGO
    ));
}

/// A stale check that also finds the record unreadable records no try: the generation a change
/// moved the workspace to (a team join, a credential change) stays due once the record reads again.
#[test]
fn a_stale_check_of_an_unreadable_record_leaves_the_current_one_due() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-stale-unread", "good");
    let stale = plan_of(&workspace_id, "hourly-stale-unread");
    change_work_source_config(&workspace_id, || ()).expect("changed");
    store
        .db
        .execute(
            "UPDATE metadata SET value = 'not a record' WHERE key = 'workTicketSources'",
            [],
        )
        .expect("damaged");
    run_info_check(&InfoCheck {
        plan: stale,
        paths: store.paths.clone(),
    });
    assert!(calls(&store, "hourly-stale-unread").is_empty());
    assert!(info_check_wanted(
        &plan_of(&workspace_id, "hourly-stale-unread"),
        LONG_AGO
    ));
}

/// A change that lands after an hourly check kept its result, before its try is recorded, leaves
/// its newer generation due: the try is at the generation the check left, read while the writer
/// its result was kept under was still held.
#[test]
fn a_change_after_the_check_kept_its_result_stays_due() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-left", "good");
    let (reached_start, started) = mpsc::channel();
    let (release_start, start) = mpsc::channel();
    hook_before_spawn(
        &workspace_id,
        BeforeSpawn::Pause {
            reached: reached_start,
            release: start,
        },
    );
    let job = InfoCheck {
        plan: plan_of(&workspace_id, "hourly-left"),
        paths: store.paths.clone(),
    };
    let check = std::thread::spawn(move || run_info_check(&job));
    started.recv_timeout(HANDSHAKE).expect("the check started");
    // The next pause: after the result is kept, before its try is recorded.
    let (reached_kept, kept) = mpsc::channel();
    let (release_kept, record) = mpsc::channel();
    hook_before_spawn(
        &workspace_id,
        BeforeSpawn::Pause {
            reached: reached_kept,
            release: record,
        },
    );
    release_start.send(()).expect("start");
    kept.recv_timeout(HANDSHAKE).expect("the result was kept");
    change_work_source_config(&workspace_id, || ()).expect("changed");
    release_kept.send(()).expect("record");
    check.join().expect("the check ended");
    assert!(info_check_wanted(
        &plan_of(&workspace_id, "hourly-left"),
        LONG_AGO
    ));
}

/// A name an hourly check accepts changes what the project row publishes, so the card pass sends
/// the row again.
#[test]
fn an_hourly_rename_republishes_the_project_row() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-row-name", "renamed");
    let project = project_in(&store, &workspace_id, json!({}));
    let before = published_project_work(project_id(&project), &project);
    check(&store, &workspace_id, "hourly-row-name");
    let after = published_project_work(project_id(&project), &project);
    assert_ne!(before, after);
    assert_eq!(before.parts, after.parts);
}

/// The setup changes while the check's `info` runs: its answer is dropped, and the new
/// generation is still due, not held back for an hour by a check that never ran at it.
#[test]
fn a_check_the_setup_changed_under_leaves_the_new_generation_due() {
    let store = Store::new();
    let workspace_id = pin_due(&store, "ws-a", "hourly-moved-on", "wait");
    let checking = {
        let (paths, workspace_id) = (store.paths.clone(), workspace_id.clone());
        std::thread::spawn(move || {
            run_info_check(&InfoCheck {
                plan: plan_of(&workspace_id, "hourly-moved-on"),
                paths,
            })
        })
    };
    let deadline = std::time::Instant::now() + HANDSHAKE;
    while !calls(&store, "hourly-moved-on").contains(&"info".to_string()) {
        assert!(
            std::time::Instant::now() < deadline,
            "the check never started"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    change_work_source_config(&workspace_id, || {
        store_value(
            &store,
            &workspace_id,
            "hourly-moved-on",
            "token",
            "tok-hourly-new",
        );
    })
    .expect("changed");
    release(&store, "hourly-moved-on");
    checking.join().expect("checked");
    assert_eq!(record(&store, &workspace_id).checked_at, LONG_AGO);
    assert!(info_check_wanted(
        &plan_of(&workspace_id, "hourly-moved-on"),
        LONG_AGO
    ));
    assert!(log_lines(&store, "work_source_check_refused").is_empty());
}
