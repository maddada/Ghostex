use super::*;
use crate::{
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};
use serde_json::{json, Map};

const NOW: &str = "2026-07-29T12:00:00.000Z";

fn now_ms() -> i64 {
    parse_iso_ms(NOW).expect("now")
}

fn ago(days: f64) -> String {
    iso_from_ms(now_ms() - (days * DAY_MS as f64) as i64)
}

fn ahead(hours: f64) -> String {
    iso_from_ms(now_ms() + (hours * 60.0 * 60.0 * 1_000.0) as i64)
}

struct TestDb {
    _temp: tempfile::TempDir,
    db: crate::storage::GxserverDatabase,
}

fn test_db() -> TestDb {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    TestDb { _temp: temp, db }
}

fn repository(handle: &TestDb) -> DomainRepository<'_> {
    DomainRepository::new(&handle.db, "S1")
}

fn params(entries: Value) -> Map<String, Value> {
    entries.as_object().cloned().expect("params object")
}

/// A running session whose meaningful-activity clock reads `activity_at`.
fn create_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    activity: &str,
    activity_at: &str,
) -> (String, String) {
    let mut agent_activity = params(json!({
        "activity": activity,
        "agentName": "codex",
        "lastChangedAt": activity_at,
        "lastMeaningfulActivityAt": activity_at,
        "updatedAt": activity_at,
    }));
    if activity == "working" {
        // An explicit working stint never expires on read, which is what a
        // real agent-hook working session looks like.
        agent_activity.insert("workingSource".to_string(), json!("explicit"));
        agent_activity.insert("workingStartedAt".to_string(), json!(activity_at));
        agent_activity.insert("hasSeenWorking".to_string(), json!(true));
    }
    let session = repository
        .create_session(
            &params(json!({
                "projectId": project_id,
                "kind": "agent",
                "agentId": "codex",
                "title": "Lifecycle session",
                "lifecycleState": "running",
                "lastActiveAt": activity_at,
                "providerState": { "lifecycleState": "exists" },
                "runtimeSettings": { "agentActivity": Value::Object(agent_activity) },
            })),
            false,
        )
        .expect("create session");
    (
        session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("projectId")
            .to_string(),
        session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("sessionId")
            .to_string(),
    )
}

fn create_project(repository: &DomainRepository<'_>) -> String {
    let project = repository
        .create_project(&params(
            json!({ "name": "Lifecycle", "path": std::env::temp_dir() }),
        ))
        .expect("create project");
    project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("projectId")
        .to_string()
}

fn lifecycle_of(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> SessionLifecycleFields {
    let session = repository
        .get_session(project_id, session_id)
        .expect("get session")
        .expect("session exists");
    SessionLifecycleFields::from_session(&session)
}

fn sweep_options(auto_settle_after_days: Option<f64>) -> SessionLifecycleSweepOptions {
    SessionLifecycleSweepOptions {
        auto_settle_on_finished_pull_request: auto_settle_on_finished_pull_request(
            auto_settle_after_days,
        ),
        auto_settle_after_days,
        max_mutations: SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS,
        now_iso: NOW.to_string(),
    }
}

/// Resolves every session's pull request to `disposition`, which is enough
/// for the guard tests: they only ever have one candidate session.
fn every_pull_request(
    disposition: PullRequestDisposition,
) -> impl Fn(&Value) -> PullRequestDisposition {
    move |_session: &Value| disposition
}

/// Resolves the pull request of one session id, and nothing else.
fn pull_request_of(
    session_id: &str,
    disposition: PullRequestDisposition,
) -> impl Fn(&Value) -> PullRequestDisposition + '_ {
    move |session: &Value| {
        if session.get("sessionId").and_then(Value::as_str) == Some(session_id) {
            disposition
        } else {
            PullRequestDisposition::Unknown
        }
    }
}

#[test]
fn old_state_rows_hydrate_without_lifecycle_fields() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));

    let session = repository
        .get_session(&project_id, &session_id)
        .expect("get session")
        .expect("session exists");
    for key in [
        "settledAt",
        "settledOverride",
        "settledOverrideAt",
        "snoozedAt",
        "snoozedUntil",
    ] {
        assert!(session.get(key).is_none(), "{key} should be absent");
    }
    assert_eq!(
        SessionLifecycleFields::from_session(&session),
        SessionLifecycleFields::default()
    );
}

#[test]
fn settle_rejects_working_and_attention_sessions() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    for activity in ["working", "attention"] {
        let (project_id, session_id) = create_session(&repository, &project_id, activity, NOW);
        let error =
            settle_session(&repository, &project_id, &session_id, NOW).expect_err("blocked settle");
        assert_eq!(error.code, "badRequest");
        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id),
            SessionLifecycleFields::default()
        );
    }
}

#[test]
fn snooze_allows_attention_and_working_sessions() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    let (project_id, attention_id) = create_session(&repository, &project_id, "attention", NOW);
    let outcome = snooze_session(&repository, &project_id, &attention_id, &ahead(1.0), NOW)
        .expect("attention sessions are snoozable");
    assert!(outcome.changed);

    let (project_id, working_id) = create_session(&repository, &project_id, "working", NOW);
    let outcome = snooze_session(&repository, &project_id, &working_id, &ahead(1.0), NOW)
        .expect("working sessions are snoozable");
    assert!(outcome.changed);
    assert_eq!(
        lifecycle_of(&repository, &project_id, &working_id).snoozed_until,
        Some(ahead(1.0))
    );
}

#[test]
fn snooze_rejects_wake_times_that_are_not_in_the_future() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", NOW);

    for wake_at in [ago(1.0), NOW.to_string(), "not-a-timestamp".to_string()] {
        let error = snooze_session(&repository, &project_id, &session_id, &wake_at, NOW)
            .expect_err("rejected wake time");
        assert_eq!(error.code, "badRequest");
    }
    assert_eq!(
        lifecycle_of(&repository, &project_id, &session_id),
        SessionLifecycleFields::default()
    );
}

#[test]
fn settle_and_unsettle_are_idempotent_and_carry_override_semantics() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));

    let settled = settle_session(&repository, &project_id, &session_id, NOW).expect("settle");
    assert!(settled.changed);
    let after_settle = lifecycle_of(&repository, &project_id, &session_id);
    assert_eq!(after_settle.settled_override.as_deref(), Some("settled"));
    assert_eq!(after_settle.settled_at.as_deref(), Some(NOW));

    let later = "2026-07-29T13:00:00.000Z";
    let repeat = settle_session(&repository, &project_id, &session_id, later).expect("settle");
    assert!(!repeat.changed, "duplicate settle must be a no-op");
    assert_eq!(
        lifecycle_of(&repository, &project_id, &session_id).settled_at,
        after_settle.settled_at
    );

    let unsettled =
        unsettle_session(&repository, &project_id, &session_id, later).expect("unsettle");
    assert!(unsettled.changed);
    let after_unsettle = lifecycle_of(&repository, &project_id, &session_id);
    assert_eq!(after_unsettle.settled_override.as_deref(), Some("active"));
    assert_eq!(after_unsettle.settled_at, None);
    assert_eq!(after_unsettle.settled_override_at.as_deref(), Some(later));

    let repeat = unsettle_session(&repository, &project_id, &session_id, NOW).expect("unsettle");
    assert!(!repeat.changed, "duplicate unsettle must be a no-op");
}

#[test]
fn snooze_is_idempotent_and_unsnooze_clears_both_fields() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
    let wake_at = ahead(3.0);

    let snoozed =
        snooze_session(&repository, &project_id, &session_id, &wake_at, NOW).expect("snooze");
    assert!(snoozed.changed);
    let after_snooze = lifecycle_of(&repository, &project_id, &session_id);
    assert_eq!(
        after_snooze.snoozed_until.as_deref(),
        Some(wake_at.as_str())
    );
    assert_eq!(after_snooze.snoozed_at.as_deref(), Some(NOW));

    let later = "2026-07-29T12:30:00.000Z";
    let repeat =
        snooze_session(&repository, &project_id, &session_id, &wake_at, later).expect("snooze");
    assert!(!repeat.changed, "same wake time must be a no-op");
    assert_eq!(
        lifecycle_of(&repository, &project_id, &session_id).snoozed_at,
        after_snooze.snoozed_at
    );

    let rescheduled =
        snooze_session(&repository, &project_id, &session_id, &ahead(9.0), later).expect("snooze");
    assert!(rescheduled.changed, "a new wake time is a real change");
    assert_eq!(
        lifecycle_of(&repository, &project_id, &session_id).snoozed_at,
        Some(later.to_string())
    );

    let woken = unsnooze_session(&repository, &project_id, &session_id, later).expect("unsnooze");
    assert!(woken.changed);
    let after_wake = lifecycle_of(&repository, &project_id, &session_id);
    assert_eq!(after_wake.snoozed_until, None);
    assert_eq!(after_wake.snoozed_at, None);

    let repeat = unsnooze_session(&repository, &project_id, &session_id, later).expect("unsnooze");
    assert!(!repeat.changed, "waking an awake session must be a no-op");
}

#[test]
fn snoozing_a_settled_session_leaves_the_settle_alone() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));

    settle_session(&repository, &project_id, &session_id, NOW).expect("settle");
    snooze_session(&repository, &project_id, &session_id, &ahead(2.0), NOW).expect("snooze");

    let lifecycle = lifecycle_of(&repository, &project_id, &session_id);
    assert_eq!(lifecycle.settled_override.as_deref(), Some("settled"));
    assert_eq!(lifecycle.settled_at.as_deref(), Some(NOW));
    assert_eq!(
        lifecycle.snoozed_until.as_deref(),
        Some(ahead(2.0).as_str())
    );
}

#[test]
fn generic_session_updates_can_neither_set_nor_lose_lifecycle_state() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
    settle_session(&repository, &project_id, &session_id, NOW).expect("settle");

    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert("title".to_string(), json!("Renamed"));
    update.insert("settledOverride".to_string(), json!("active"));
    update.insert("snoozedUntil".to_string(), json!(ahead(4.0)));
    let updated = repository.update_session(&update).expect("update session");

    assert_eq!(updated.get("title"), Some(&json!("Renamed")));
    let lifecycle = lifecycle_of(&repository, &project_id, &session_id);
    assert_eq!(
        lifecycle.settled_override.as_deref(),
        Some("settled"),
        "a generic update must not smuggle a lifecycle change past the guards"
    );
    assert_eq!(lifecycle.settled_at.as_deref(), Some(NOW));
    assert_eq!(lifecycle.snoozed_until, None);
}

#[test]
fn sweep_auto_settles_only_stale_idle_unpinned_sessions() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    let (project_id, stale_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
    let (project_id, fresh_id) = create_session(&repository, &project_id, "idle", &ago(1.0));
    let (project_id, working_id) = create_session(&repository, &project_id, "working", &ago(5.0));
    let (project_id, attention_id) =
        create_session(&repository, &project_id, "attention", &ago(5.0));
    let (project_id, snoozed_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
    snooze_session(&repository, &project_id, &snoozed_id, &ahead(4.0), NOW).expect("snooze");
    let (project_id, pinned_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
    unsettle_session(&repository, &project_id, &pinned_id, NOW).expect("unsettle");

    let outcome = run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &ignore_pull_requests,
    )
    .expect("sweep");
    assert_eq!(
        outcome.changed,
        vec![(project_id.clone(), stale_id.clone())],
        "only the stale idle session may auto-settle"
    );

    let settled = lifecycle_of(&repository, &project_id, &stale_id);
    assert_eq!(settled.settled_override.as_deref(), Some("settled"));
    assert_eq!(
        settled.settled_at, None,
        "an inactivity settle is not an explicit stamp"
    );
    for session_id in [&fresh_id, &working_id, &attention_id] {
        assert_eq!(
            lifecycle_of(&repository, &project_id, session_id).settled_override,
            None
        );
    }
    assert_eq!(
        lifecycle_of(&repository, &project_id, &snoozed_id).settled_override,
        None,
        "snoozed is not settled"
    );
    assert_eq!(
        lifecycle_of(&repository, &project_id, &pinned_id)
            .settled_override
            .as_deref(),
        Some("active"),
        "a manual un-settle pin suppresses auto-settle"
    );
}

#[test]
fn sweep_does_not_auto_settle_when_the_window_is_disabled() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(90.0));

    let outcome =
        run_session_lifecycle_sweep(&repository, &sweep_options(None), &ignore_pull_requests)
            .expect("sweep");
    assert!(outcome.changed.is_empty());
    assert_eq!(
        lifecycle_of(&repository, &project_id, &session_id).settled_override,
        None
    );
}

#[test]
fn sweep_clears_any_override_once_activity_outruns_its_stamp() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    // Settled a day ago, then worked an hour ago.
    let (project_id, settled_id) = create_session(&repository, &project_id, "idle", &ago(0.04));
    settle_session(&repository, &project_id, &settled_id, &ago(1.0)).expect("settle");
    // Pinned active a day ago, then worked an hour ago.
    let (project_id, pinned_id) = create_session(&repository, &project_id, "idle", &ago(0.04));
    unsettle_session(&repository, &project_id, &pinned_id, &ago(1.0)).expect("unsettle");
    // Settled after its last activity: the settle still stands.
    let (project_id, stable_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
    settle_session(&repository, &project_id, &stable_id, &ago(1.0)).expect("settle");

    run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &ignore_pull_requests,
    )
    .expect("sweep");

    assert_eq!(
        lifecycle_of(&repository, &project_id, &settled_id),
        SessionLifecycleFields::default()
    );
    assert_eq!(
        lifecycle_of(&repository, &project_id, &pinned_id),
        SessionLifecycleFields::default()
    );
    let stable = lifecycle_of(&repository, &project_id, &stable_id);
    assert_eq!(stable.settled_override.as_deref(), Some("settled"));
    assert_eq!(stable.settled_at.as_deref(), Some(ago(1.0).as_str()));
}

#[test]
fn sweep_clears_spent_snooze_state_at_the_wake_time() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    let (project_id, just_woke_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
    let (project_id, long_woke_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
    let (project_id, still_snoozed_id) =
        create_session(&repository, &project_id, "idle", &ago(0.1));

    let base = iso_from_ms(now_ms() - 10 * DAY_MS);
    snooze_session(&repository, &project_id, &just_woke_id, &ago(0.2), &base).expect("snooze");
    snooze_session(&repository, &project_id, &long_woke_id, &ago(3.0), &base).expect("snooze");
    snooze_session(
        &repository,
        &project_id,
        &still_snoozed_id,
        &ahead(6.0),
        &base,
    )
    .expect("snooze");

    run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &ignore_pull_requests,
    )
    .expect("sweep");

    assert_eq!(
        lifecycle_of(&repository, &project_id, &just_woke_id).snoozed_until,
        None,
        "a session past its wake time is cleared on the next sweep so clients get a delta"
    );
    assert_eq!(
        lifecycle_of(&repository, &project_id, &long_woke_id).snoozed_until,
        None
    );
    assert_eq!(
        lifecycle_of(&repository, &project_id, &still_snoozed_id).snoozed_until,
        Some(ahead(6.0))
    );
}

#[test]
fn sweep_never_clears_a_snooze_because_a_session_wants_attention() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
    snooze_session(&repository, &project_id, &session_id, &ahead(5.0), NOW).expect("snooze");

    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        json!({
            "agentActivity": {
                "activity": "attention",
                "agentName": "codex",
                "updatedAt": NOW,
            },
        }),
    );
    repository.update_session(&update).expect("update session");

    run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &ignore_pull_requests,
    )
    .expect("sweep");

    assert_eq!(
        lifecycle_of(&repository, &project_id, &session_id).snoozed_until,
        Some(ahead(5.0)),
        "the client surfaces a raised hand; the server keeps the return ticket"
    );
}

#[test]
fn auto_settle_window_reads_the_shared_sidebar_settings_file() {
    assert_eq!(
        normalize_auto_settle_after_days(None),
        Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS)
    );
    assert_eq!(normalize_auto_settle_after_days(Some(&json!(null))), None);
    assert_eq!(normalize_auto_settle_after_days(Some(&json!(0))), None);
    assert_eq!(normalize_auto_settle_after_days(Some(&json!(-2))), None);
    assert_eq!(normalize_auto_settle_after_days(Some(&json!(7))), Some(7.0));
    assert_eq!(
        normalize_auto_settle_after_days(Some(&json!("nonsense"))),
        Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS)
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    assert_eq!(
        read_sweep_auto_settle_after_days(&paths),
        None,
        "a machine with no settings file is a V1 machine and never auto-settles"
    );

    let settings_dir = paths.app_config_dir.clone();
    std::fs::create_dir_all(&settings_dir).expect("settings dir");
    let settings_file = settings_dir.join("native-sidebar-settings.json");
    std::fs::write(
        &settings_file,
        json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 10 }).to_string(),
    )
    .expect("settings file");
    assert_eq!(read_sweep_auto_settle_after_days(&paths), Some(10.0));

    std::fs::write(
        &settings_file,
        json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 10 }).to_string(),
    )
    .expect("settings file");
    assert_eq!(
        read_sweep_auto_settle_after_days(&paths),
        None,
        "switching back to V1 stops the automatic pass without touching the window setting"
    );
}

/*
CDXC:StateSync 2026-07-29:
The gate the Sidebar V2 data passes (git status, `origin` remote, project
icons) read once per pass. It answers from the SAME settings file and the
SAME `is_sidebar_v2_selected` rule as the auto-settle window above, so a
machine can never be V2 for one of them and V1 for the other, and a flip is
picked up by the next pass rather than the next daemon start.
*/
#[test]
fn the_sidebar_v2_data_gate_reads_the_shared_settings_file_each_time() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    assert!(
        !read_sidebar_v2_selected(&paths),
        "a machine with no settings file is a V1 machine and probes nothing"
    );

    let settings_dir = paths.app_config_dir.clone();
    std::fs::create_dir_all(&settings_dir).expect("settings dir");
    let settings_file = settings_dir.join("native-sidebar-settings.json");

    std::fs::write(&settings_file, "{ not json").expect("settings file");
    assert!(
        !read_sidebar_v2_selected(&paths),
        "an unreadable settings file is V1, never a reason to probe anyway"
    );

    for (settings, expected) in [
        (json!({}), false),
        (json!({ "sidebarVersion": "v1" }), false),
        (json!({ "sidebarVersion": "v3" }), false),
        (json!({ "sidebarVersion": "v2" }), true),
        (json!({ "sidebarVersion": " V2 " }), true),
    ] {
        std::fs::write(&settings_file, settings.to_string()).expect("settings file");
        assert_eq!(
            read_sidebar_v2_selected(&paths),
            expected,
            "settings {settings} must gate the data passes to {expected}"
        );
        assert_eq!(
            read_sweep_auto_settle_after_days(&paths).is_some(),
            expected,
            "the data gate and the auto-settle gate must never disagree"
        );
    }
}

#[test]
fn auto_settle_window_is_gated_on_the_sidebar_version() {
    assert!(!is_sidebar_v2_selected(None));
    assert!(!is_sidebar_v2_selected(Some(&json!("v1"))));
    assert!(!is_sidebar_v2_selected(Some(&json!("v3"))));
    assert!(!is_sidebar_v2_selected(Some(&json!(2))));
    assert!(is_sidebar_v2_selected(Some(&json!("v2"))));
    assert!(is_sidebar_v2_selected(Some(&json!(" v2 "))));

    assert_eq!(resolve_sweep_auto_settle_after_days(None), None);
    assert_eq!(
        resolve_sweep_auto_settle_after_days(Some(&json!({ "sidebarAutoSettleAfterDays": 7 }))),
        None,
        "a missing sidebarVersion means V1"
    );
    assert_eq!(
        resolve_sweep_auto_settle_after_days(Some(
            &json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 7 })
        )),
        None
    );
    assert_eq!(
        resolve_sweep_auto_settle_after_days(Some(
            &json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 7 })
        )),
        Some(7.0)
    );
    assert_eq!(
        resolve_sweep_auto_settle_after_days(Some(&json!({ "sidebarVersion": "v2" }))),
        Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS),
        "V2 without an explicit window uses the default"
    );
    assert_eq!(
        resolve_sweep_auto_settle_after_days(Some(
            &json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": null })
        )),
        None,
        "an explicit null still disables auto-settle inside V2"
    );
}

/// The version gate reaches the sweep itself: a V1 machine keeps every stale
/// row in its inbox, a V2 machine settles it, and spent-snooze collection
/// grooms both so a user who flips back to V1 does not strand snooze state.
#[test]
fn sweep_auto_settles_only_for_v2_while_snooze_gc_runs_for_both_versions() {
    for settings in [
        json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 3 }),
        json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 3 }),
    ] {
        let is_v2 = settings.get("sidebarVersion") == Some(&json!("v2"));
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        let (project_id, stale_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
        let (project_id, long_woke_id) =
            create_session(&repository, &project_id, "idle", &ago(0.1));
        let base = iso_from_ms(now_ms() - 10 * DAY_MS);
        snooze_session(&repository, &project_id, &long_woke_id, &ago(3.0), &base).expect("snooze");

        let options = sweep_options(resolve_sweep_auto_settle_after_days(Some(&settings)));
        run_session_lifecycle_sweep(&repository, &options, &ignore_pull_requests).expect("sweep");

        assert_eq!(
            lifecycle_of(&repository, &project_id, &stale_id)
                .settled_override
                .as_deref(),
            if is_v2 { Some("settled") } else { None },
            "auto-settle must follow sidebarVersion (v2: {is_v2})"
        );
        assert_eq!(
            lifecycle_of(&repository, &project_id, &long_woke_id).snoozed_until,
            None,
            "spent snooze state is collected regardless of sidebarVersion (v2: {is_v2})"
        );
    }
}

/*
CDXC:Git 2026-07-29-00:00:
The pull-request trigger settles finished work the moment the forge says it
is finished — no inactivity wait — while an open (or draft, which resolves to
open) pull request and an unknown one leave the row exactly where it is.
*/
#[test]
fn sweep_settles_a_finished_pull_request_immediately_without_waiting_out_the_window() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    // All three worked minutes ago, far inside the three-day window.
    let (project_id, finished_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
    let (project_id, open_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
    let (project_id, unknown_id) = create_session(&repository, &project_id, "idle", &ago(0.01));

    let finished_ref = finished_id.clone();
    let open_ref = open_id.clone();
    let outcome = run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &move |session: &Value| match session.get("sessionId").and_then(Value::as_str) {
            Some(session_id) if session_id == finished_ref => PullRequestDisposition::Finished,
            Some(session_id) if session_id == open_ref => PullRequestDisposition::Open,
            _ => PullRequestDisposition::Unknown,
        },
    )
    .expect("sweep");

    assert_eq!(
        outcome.changed,
        vec![(project_id.clone(), finished_id.clone())],
        "only the merged/closed pull request settles"
    );
    let settled = lifecycle_of(&repository, &project_id, &finished_id);
    assert_eq!(settled.settled_override.as_deref(), Some("settled"));
    assert_eq!(
        settled.settled_at, None,
        "an automatic settle is not an explicit stamp, whatever triggered it"
    );
    assert_eq!(settled.settled_override_at.as_deref(), Some(NOW));
    for session_id in [&open_id, &unknown_id] {
        assert_eq!(
            lifecycle_of(&repository, &project_id, session_id).settled_override,
            None
        );
    }
}

#[test]
fn a_finished_pull_request_still_passes_through_every_settle_guard() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);

    let (project_id, working_id) = create_session(&repository, &project_id, "working", &ago(0.01));
    let (project_id, attention_id) =
        create_session(&repository, &project_id, "attention", &ago(0.01));
    let (project_id, snoozed_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
    snooze_session(&repository, &project_id, &snoozed_id, &ahead(4.0), NOW).expect("snooze");
    let (project_id, pinned_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
    unsettle_session(&repository, &project_id, &pinned_id, NOW).expect("unsettle");

    let outcome = run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &every_pull_request(PullRequestDisposition::Finished),
    )
    .expect("sweep");
    assert!(
        outcome.changed.is_empty(),
        "a merged pull request never parks work that is in motion, blocked on the user, snoozed, or pinned active"
    );

    for session_id in [&working_id, &attention_id, &snoozed_id] {
        assert_eq!(
            lifecycle_of(&repository, &project_id, session_id).settled_override,
            None
        );
    }
    assert_eq!(
        lifecycle_of(&repository, &project_id, &pinned_id)
            .settled_override
            .as_deref(),
        Some("active"),
        "the active pin survives until real activity clears it (rule 2)"
    );
}

/*
The activity-reset half of the pin's semantics. An "active" pin means "keep
this in my inbox" only until real activity outruns its stamp; after that the
pin is spent and the ordinary rules apply again, in the same pass — exactly
how the inactivity trigger already behaves when a long-dormant pin is reset
by activity that is itself older than the window.
*/
#[test]
fn a_pin_outrun_by_activity_stops_protecting_a_finished_pull_request() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    // Pinned a day ago, then worked an hour ago: the pin is spent.
    let (project_id, spent_pin_id) = create_session(&repository, &project_id, "idle", &ago(0.04));
    unsettle_session(&repository, &project_id, &spent_pin_id, &ago(1.0)).expect("unsettle");
    // Pinned after its last activity: the pin still stands.
    let (project_id, live_pin_id) = create_session(&repository, &project_id, "idle", &ago(1.0));
    unsettle_session(&repository, &project_id, &live_pin_id, &ago(0.04)).expect("unsettle");

    run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &every_pull_request(PullRequestDisposition::Finished),
    )
    .expect("sweep");

    let spent = lifecycle_of(&repository, &project_id, &spent_pin_id);
    assert_eq!(
        spent.settled_override.as_deref(),
        Some("settled"),
        "the reset clears the pin and the merged pull request then settles the row"
    );
    assert_eq!(spent.settled_at, None);
    assert_eq!(
        lifecycle_of(&repository, &project_id, &live_pin_id)
            .settled_override
            .as_deref(),
        Some("active"),
        "a live pin still outranks the pull request"
    );
}

/// The pull-request trigger rides the same switch as the inactivity window:
/// V1 machines and users who turned auto-settle off never see it.
#[test]
fn pull_request_auto_settle_follows_the_inactivity_window_switch() {
    assert!(!auto_settle_on_finished_pull_request(None));
    assert!(auto_settle_on_finished_pull_request(Some(
        DEFAULT_AUTO_SETTLE_AFTER_DAYS
    )));

    for (settings, expected) in [
        (
            json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 3 }),
            None,
        ),
        (json!({ "sidebarVersion": "v2" }), Some("settled")),
        (
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": null }),
            None,
        ),
    ] {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.01));

        let options = sweep_options(resolve_sweep_auto_settle_after_days(Some(&settings)));
        run_session_lifecycle_sweep(
            &repository,
            &options,
            &every_pull_request(PullRequestDisposition::Finished),
        )
        .expect("sweep");

        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id)
                .settled_override
                .as_deref(),
            expected,
            "settings {settings} must decide the pull-request trigger too"
        );
    }
}

/// The resolver is only consulted for machines the trigger is enabled on, so
/// a V1 daemon never pays for a git-status lookup it would ignore.
#[test]
fn the_pull_request_resolver_is_skipped_when_the_trigger_is_off() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    create_session(&repository, &project_id, "idle", &ago(0.01));

    let calls = std::cell::Cell::new(0_usize);
    let options = sweep_options(None);
    run_session_lifecycle_sweep(&repository, &options, &|_session: &Value| {
        calls.set(calls.get() + 1);
        PullRequestDisposition::Finished
    })
    .expect("sweep");
    assert_eq!(calls.get(), 0);

    let options = sweep_options(Some(3.0));
    run_session_lifecycle_sweep(&repository, &options, &|_session: &Value| {
        calls.set(calls.get() + 1);
        PullRequestDisposition::Unknown
    })
    .expect("sweep");
    assert_eq!(calls.get(), 1);
}

/// One session id, so `pull_request_of` earns its keep next to
/// `every_pull_request`.
#[test]
fn only_the_session_whose_pull_request_finished_is_settled() {
    let handle = test_db();
    let repository = repository(&handle);
    let project_id = create_project(&repository);
    let (project_id, settled_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
    let (project_id, untouched_id) = create_session(&repository, &project_id, "idle", &ago(0.01));

    run_session_lifecycle_sweep(
        &repository,
        &sweep_options(Some(3.0)),
        &pull_request_of(&settled_id, PullRequestDisposition::Finished),
    )
    .expect("sweep");

    assert_eq!(
        lifecycle_of(&repository, &project_id, &settled_id)
            .settled_override
            .as_deref(),
        Some("settled")
    );
    assert_eq!(
        lifecycle_of(&repository, &project_id, &untouched_id).settled_override,
        None
    );
}
