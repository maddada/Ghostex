//! The Issues Workflow pass's deltas: what a project row or a session card published is kept only
//! once its delta went out, so one that fails is sent again on the next pass, and it holds back no
//! other row's or card's.
//!
//! Other tests' passes share what was published and drop the rows their own state lacks, so these
//! only assert what such a pass can't change: a pass sends each changed delta, and one that failed
//! is still due on the next.

use super::*;
use crate::server::work_mode_sync::{delta_hooks, run_work_mode_refresh_once};

fn store_a_linear_key(paths: &GxserverPaths, project_id: &str) {
    crate::work_mode::store_linear_api_key(
        paths,
        crate::work_mode::LinearKeyScope::Project(project_id),
        Some(&format!("lin_api_{project_id}")),
    )
    .expect("key stored");
}

/// How many deltas passes tried for a project or session id.
fn attempts(id: &str) -> usize {
    delta_hooks()
        .attempted
        .iter()
        .filter(|attempted| *attempted == id)
        .count()
}

/// Runs passes until one sends neither id a delta, so what each published is settled.
fn settle(state: &Arc<AppState>, ids: [&str; 2]) {
    let sent = || ids.iter().map(|id| attempts(id)).sum::<usize>();
    for _ in 0..5 {
        let before = sent();
        run_work_mode_refresh_once(state).expect("a pass");
        if sent() == before {
            return;
        }
    }
    panic!("the pass never settled");
}

/// Both ids are due, `failing` sent first. Its delta fails on each pass until `other`'s stops
/// changing (a row can change on two passes in a row, as its tracker follows its Linear key), and
/// the next pass sends it again.
fn fail_until_the_other_settles(state: &Arc<AppState>, failing: &str, other: &str) {
    let (failing_before, other_before) = (attempts(failing), attempts(other));
    let mut other_sent = other_before;
    for pass in 0..5 {
        delta_hooks().failing.insert(failing.to_string());
        let result = run_work_mode_refresh_once(state);
        if pass == 0 {
            assert_eq!(
                result.expect_err("the failed delta").message,
                "A test's delta failed."
            );
            assert_eq!(attempts(failing), failing_before + 1);
            assert_eq!(
                attempts(other),
                other_before + 1,
                "the other delta still went out"
            );
        }
        if pass > 0 && attempts(other) == other_sent {
            break;
        }
        other_sent = attempts(other);
    }
    delta_hooks().failing.remove(failing);
    let failed = attempts(failing);

    run_work_mode_refresh_once(state).expect("sent");
    assert_eq!(attempts(failing), failed + 1, "sent again on the next pass");
}

/// The ids in the order the repository lists them, which is the order the pass sends them in.
fn two_ids(listed: Vec<Value>, field: &str) -> [String; 2] {
    <[String; 2]>::try_from(
        listed
            .iter()
            .map(|row| row[field].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
    )
    .expect("two rows")
}

#[test]
fn a_project_delta_that_fails_is_sent_again_and_holds_back_no_other() {
    let _gh = crate::session_git_status::assume_gh_cli_signed_out_for_test();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().join("gx")));
    let state = test_app_state(paths.clone());
    let db = crate::storage::open_gxserver_database(&paths).expect("db");
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    for name in ["pass-first", "pass-second"] {
        repository
            .create_project(json!({ "name": name }).as_object().unwrap())
            .expect("project");
    }
    let [failing, other] = two_ids(repository.list_projects().expect("projects"), "projectId");
    settle(&state, [&failing, &other]);
    // Both get a Linear key, so both rows change.
    store_a_linear_key(&paths, &failing);
    store_a_linear_key(&paths, &other);

    fail_until_the_other_settles(&state, &failing, &other);
}

#[test]
fn a_session_delta_that_fails_is_sent_again_and_holds_back_no_other() {
    let _gh = crate::session_git_status::assume_gh_cli_signed_out_for_test();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().join("gx")));
    let state = test_app_state(paths.clone());
    let db = crate::storage::open_gxserver_database(&paths).expect("db");
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository
        .create_project(
            json!({ "name": "pass-sessions", "path": temp.path() })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap().to_string();
    // With a Linear key, Issues Workflow is on, so every pass reads the project's open sessions.
    store_a_linear_key(&paths, &project_id);
    for title in ["First", "Second"] {
        repository
            .create_session(
                json!({ "kind": "agent", "projectId": project_id, "title": title })
                    .as_object()
                    .unwrap(),
                true,
            )
            .expect("session");
    }
    let [failing, other] = two_ids(
        repository
            .list_sessions_excluding_stopped(Some(&project_id))
            .expect("sessions"),
        "sessionId",
    );

    // Neither card was published yet, so both are due.
    fail_until_the_other_settles(&state, &failing, &other);
}
