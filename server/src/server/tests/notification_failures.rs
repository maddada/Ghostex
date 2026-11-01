//! Failed rows end to end: accepted hook events through `/api/ingestAgentHookEvent`, a program's
//! OSC 7501 reports through the program-status ingest, and the feed's existing rules (one live row
//! per session, retention, deferral, account rows, coordinator acknowledgement) with them, all on
//! a temporary state database.

use super::*;
use crate::notification_feed::store::{
    account_notification_session_id, defer_session_notification, dismiss_notification,
    insert_notification_feed_row, read_notification_feed_state, NewNotificationFeedRow,
    ACCOUNT_NOTIFICATION_PROJECT_ID, NOTIFICATION_FEED_KIND_RESET_EXPIRING,
    NOTIFICATION_FEED_RETENTION_LIMIT,
};
use crate::notification_feed::NOTIFICATION_FEED_FAILED_BODY;

struct Fixture {
    state: Arc<AppState>,
    project_id: String,
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let db = open_gxserver_database(&state.paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project_id = repository
            .create_project(
                json!({ "name": "Failures", "path": temp.path() })
                    .as_object()
                    .expect("params"),
            )
            .expect("project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        Self {
            state,
            project_id,
            _temp: temp,
        }
    }

    fn session(&self, agent: &str, title: &str, runtime: Value) -> String {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        let repository = DomainRepository::new(&db, self.state.metadata.server_id.as_str());
        let mut runtime = runtime.as_object().cloned().unwrap_or_default();
        runtime
            .entry("agentActivity".to_string())
            .or_insert(json!({ "activity": "working" }));
        runtime.insert("agentName".to_string(), json!(agent));
        repository
            .create_session(
                json!({
                    "projectId": self.project_id, "kind": "agent", "agentId": agent,
                    "title": title, "lifecycleState": "running", "runtimeSettings": runtime,
                })
                .as_object()
                .expect("params"),
                false,
            )
            .expect("session")["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string()
    }

    async fn hook(&self, session_id: &str, params: Value) -> Value {
        let mut params = params.as_object().cloned().expect("params");
        params.insert("projectId".to_string(), json!(self.project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        let routed = route_http(
            self.state.clone(),
            rpc_request(
                "/api/ingestAgentHookEvent",
                &self.state.auth_token,
                json!({ "params": params }),
            ),
            "request-failure-hook".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
        response_json(routed.response).await
    }

    fn feed(&self) -> Vec<Value> {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        read_notification_feed_state(&db).expect("feed")["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn rows(&self, session_id: &str, kind: &str) -> Vec<Value> {
        self.feed()
            .into_iter()
            .filter(|row| row["sessionId"] == session_id && row["kind"] == kind)
            .collect()
    }

    fn program_status(&self, session_id: &str, state: &str, seq: u64) {
        crate::program_status::ingest_program_status_line(
            &self.state,
            &self.project_id,
            session_id,
            crate::program_status::ProgramStatusLine::Snapshot {
                reported: true,
                submits: None,
                records: vec![crate::program_status::ProgramStatusRecord {
                    state: state.to_string(),
                    msg: Some("Request failed: 500 from the provider".to_string()),
                    seq,
                    ..Default::default()
                }],
            },
        );
    }
}

#[tokio::test]
async fn a_hook_that_names_a_final_error_writes_one_failed_row_with_the_fixed_body() {
    let fixture = Fixture::new();
    let session_id = fixture.session("claude", "Build the parser", json!({}));
    let mut events = fixture.state.event_hub.subscribe();
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "StopFailure", "rawEventName": "StopFailure",
                    "stopError": "authentication_failed" }),
        )
        .await;

    let rows = fixture.rows(&session_id, "failed");
    assert_eq!(rows.len(), 1, "{:?}", fixture.feed());
    assert_eq!(rows[0]["body"], NOTIFICATION_FEED_FAILED_BODY);
    assert_eq!(rows[0]["title"], "Build the parser");
    assert_eq!(rows[0]["subtitle"], "Failures");
    assert_eq!(rows[0]["read"], false);
    let mut announced = false;
    while let Ok(event) = events.try_recv() {
        announced |= event["type"] == "notificationFeedChanged";
    }
    assert!(announced, "a new failed row is announced on the event hub");
}

#[tokio::test]
async fn a_claude_server_error_is_left_to_recovery_while_recovery_retries_it() {
    let fixture = Fixture::new();
    let retried = fixture.session("claude", "Retried", json!({}));
    let failure =
        json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "server_error" });
    fixture.hook(&retried, failure.clone()).await;
    // Claude Code that names no error is left to recovery as well.
    fixture
        .hook(
            &retried,
            json!({ "agentName": "claude", "eventName": "StopFailure" }),
        )
        .await;
    assert!(fixture.rows(&retried, "failed").is_empty());

    // Stop retrying: nothing will retry this one, so the run is over now.
    let stopped = fixture.session(
        "claude",
        "Stopped retrying",
        json!({ "accountRecoverySuppressed": true }),
    );
    fixture.hook(&stopped, failure).await;
    assert_eq!(fixture.rows(&stopped, "failed").len(), 1);
}

#[tokio::test]
async fn aborts_finished_turns_and_other_agents_events_never_create_a_failed_row() {
    let fixture = Fixture::new();
    let cursor = fixture.session("cursor", "Cursor", json!({}));
    fixture
        .hook(
            &cursor,
            json!({ "agentName": "cursor", "eventName": "stop", "stopStatus": "aborted" }),
        )
        .await;
    let omp = fixture.session("omp", "OMP", json!({}));
    fixture
        .hook(
            &omp,
            json!({ "agentName": "omp", "eventName": "Interrupt" }),
        )
        .await;
    let claude = fixture.session("claude", "Claude", json!({}));
    fixture
        .hook(
            &claude,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    fixture
        .hook(
            &claude,
            json!({ "agentName": "claude", "eventName": "SubagentStop", "stopError": "unknown" }),
        )
        .await;
    // Another agent's failure posted to this session is rejected before it counts.
    let codex = fixture.session("codex", "Codex", json!({}));
    let rejected = fixture
        .hook(
            &codex,
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "billing_error" }),
        )
        .await;
    assert_eq!(
        rejected["result"]["reason"],
        json!("agent-hook-agent-mismatch")
    );
    assert!(
        fixture.feed().iter().all(|row| row["kind"] != "failed"),
        "{:?}",
        fixture.feed()
    );
    // The finished Claude turn still notifies as before.
    assert_eq!(fixture.rows(&claude, "finished").len(), 1);
}

#[tokio::test]
async fn hook_driven_thread_questions_approvals_and_failures_reach_the_feed() {
    let fixture = Fixture::new();
    let coordinator = fixture.session("claude", "Coordinator", json!({}));
    let thread = fixture.session("claude", "Thread", json!({}));
    {
        let db = open_gxserver_database(&fixture.state.paths).expect("db");
        crate::coordinators::link_thread(
            &db,
            &fixture.project_id,
            &thread,
            &fixture.project_id,
            &coordinator,
            "task",
        )
        .expect("link");
    }
    fixture
        .hook(
            &thread,
            json!({ "agentName": "claude", "eventName": "PermissionRequest", "toolName": "Bash",
                    "toolInput": { "command": "rm -rf build" } }),
        )
        .await;
    assert_eq!(fixture.rows(&thread, "needsInput").len(), 1);
    fixture
        .hook(
            &thread,
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "invalid_request" }),
        )
        .await;
    assert_eq!(fixture.rows(&thread, "failed").len(), 1);
}

#[tokio::test]
async fn a_usage_limit_is_never_a_failure_even_when_nothing_retries_it() {
    let fixture = Fixture::new();
    let session_id = fixture.session(
        "claude",
        "Limited",
        json!({ "accountRecoverySuppressed": true }),
    );
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "rate_limit" }),
        )
        .await;
    assert!(fixture.rows(&session_id, "failed").is_empty());
}

#[tokio::test]
async fn switching_recovery_off_while_an_error_waits_announces_it_once() {
    let fixture = Fixture::new();
    let session_id = fixture.session(
        "claude",
        "Waiting",
        json!({ "accountRecovery": {
            "status": "waiting", "trigger": "streamError", "attempt": 2,
            "reason": "Claude Code hit a temporary service error",
        }}),
    );
    let off = json!({ "enabled": false, "atLimit": "switch", "priority": "soonestReset", "retryErrors": false });
    for _ in 0..2 {
        let routed = route_http(
            fixture.state.clone(),
            rpc_request(
                "/api/agentAccounts",
                &fixture.state.auth_token,
                json!({ "params": {
                    "operation": "sessionPolicy", "projectId": fixture.project_id,
                    "sessionId": session_id, "policy": off,
                }}),
            ),
            "request-session-policy".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
    }
    assert_eq!(
        fixture.rows(&session_id, "failed").len(),
        1,
        "{:?}",
        fixture.feed()
    );
}

#[tokio::test]
async fn viewing_a_coordinator_reads_its_finished_threads_and_leaves_a_failed_one_unread() {
    let fixture = Fixture::new();
    let coordinator = fixture.session("claude", "Coordinator", json!({}));
    let finished = fixture.session("claude", "Finished thread", json!({}));
    let failed = fixture.session("claude", "Failed thread", json!({}));
    {
        let db = open_gxserver_database(&fixture.state.paths).expect("db");
        for thread in [&finished, &failed] {
            crate::coordinators::link_thread(
                &db,
                &fixture.project_id,
                thread,
                &fixture.project_id,
                &coordinator,
                "task",
            )
            .expect("link");
        }
    }
    fixture
        .hook(
            &finished,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    fixture
        .hook(
            &failed,
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "unknown" }),
        )
        .await;
    let state = fixture.state.clone();
    let params = json!({ "projectId": fixture.project_id, "sessionId": coordinator })
        .as_object()
        .cloned()
        .expect("params");
    tokio::task::spawn_blocking(move || {
        crate::coordinators::acknowledge_coordinator_threads(&state, &params)
    })
    .await
    .expect("join")
    .expect("acknowledge");

    assert_eq!(fixture.rows(&finished, "finished")[0]["read"], true);
    // Only a finished turn is what viewing the coordinator clears; a failure is not one.
    assert_eq!(fixture.rows(&failed, "failed")[0]["read"], false);
}

#[tokio::test]
async fn a_program_error_creates_a_failed_row_and_keeps_the_finished_activity() {
    let fixture = Fixture::new();
    let session_id = fixture.session("codex", "Program", json!({}));
    fixture.program_status(&session_id, "working", 1);
    fixture.program_status(&session_id, "error", 2);

    assert_eq!(
        fixture.rows(&session_id, "failed").len(),
        1,
        "{:?}",
        fixture.feed()
    );
    assert!(fixture.rows(&session_id, "finished").is_empty());
    assert_eq!(
        fixture.rows(&session_id, "failed")[0]["body"],
        NOTIFICATION_FEED_FAILED_BODY
    );
    let db = open_gxserver_database(&fixture.state.paths).expect("db");
    let session = DomainRepository::new(&db, fixture.state.metadata.server_id.as_str())
        .get_session(&fixture.project_id, &session_id)
        .expect("read")
        .expect("session");
    let activity = &session["runtimeSettings"]["agentActivity"];
    assert_eq!(activity["activity"], "attention");
    assert_eq!(
        activity["attentionSource"],
        crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE
    );
}

#[tokio::test]
async fn a_reapplied_program_report_does_not_notify_twice_but_a_new_error_in_attention_does() {
    let fixture = Fixture::new();
    let session_id = fixture.session("codex", "Program", json!({}));
    fixture.program_status(&session_id, "working", 1);
    fixture.program_status(&session_id, "error", 2);
    fixture.program_status(&session_id, "error", 2);
    assert_eq!(fixture.rows(&session_id, "failed").len(), 1);

    // Read it, then block (attention) and fail again while that attention stands.
    {
        let db = open_gxserver_database(&fixture.state.paths).expect("db");
        crate::notification_feed::store::mark_all_notifications_read(&db).expect("read all");
    }
    fixture.program_status(&session_id, "blocked", 3);
    fixture.program_status(&session_id, "error", 4);
    let failed = fixture.rows(&session_id, "failed");
    assert_eq!(failed.len(), 2, "{:?}", fixture.feed());
    assert_eq!(failed.iter().filter(|row| row["read"] == false).count(), 1);
}

#[test]
fn failed_rows_keep_one_live_row_retention_deferral_and_account_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    crate::storage::initialize_gxserver_storage(&paths).expect("storage");
    let db = open_gxserver_database(&paths).expect("db");
    let repository = DomainRepository::new(&db, "S-retention");
    let project_id = repository
        .create_project(
            json!({ "name": "Retention", "path": temp.path() })
                .as_object()
                .expect("params"),
        )
        .expect("project")["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let session = |title: &str| {
        repository
            .create_session(
                json!({ "projectId": project_id, "kind": "agent", "agentId": "claude", "title": title })
                    .as_object()
                    .expect("params"),
                false,
            )
            .expect("session")["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string()
    };
    let row = |session_id: &str, kind: &'static str| NewNotificationFeedRow {
        project_id: project_id.clone(),
        session_id: session_id.to_string(),
        kind,
        title: "Session".to_string(),
        subtitle: "Retention".to_string(),
        body: NOTIFICATION_FEED_FAILED_BODY.to_string(),
        agent_name: None,
        attention_event_id: None,
    };
    let account_session = account_notification_session_id("acct-1");
    insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: ACCOUNT_NOTIFICATION_PROJECT_ID.to_string(),
            ..row(&account_session, NOTIFICATION_FEED_KIND_RESET_EXPIRING)
        },
    )
    .expect("account row");

    // One live row per session: a later finished turn supersedes the unread failed row.
    let first = session("First");
    insert_notification_feed_row(&db, row(&first, "failed")).expect("failed");
    insert_notification_feed_row(&db, row(&first, "finished")).expect("finished");
    let state = read_notification_feed_state(&db).expect("feed");
    let first_rows: Vec<&Value> = state["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["sessionId"] == first.as_str())
        .collect();
    assert_eq!(first_rows.len(), 1);
    assert_eq!(first_rows[0]["kind"], "finished");

    // Deferral and dismissal work on a failed row like on any other.
    let second = session("Second");
    let stored = insert_notification_feed_row(&db, row(&second, "failed")).expect("failed");
    assert_eq!(defer_session_notification(&db, &second).expect("defer"), 1);
    let state = read_notification_feed_state(&db).expect("feed");
    assert_ne!(
        state["nextUnreadId"], stored["id"],
        "a deferred row is jumped to last"
    );
    assert_eq!(
        dismiss_notification(&db, stored["id"].as_str().unwrap()).expect("dismiss"),
        1
    );

    // Retention keeps the newest rows, failed ones counted like any other.
    for index in 0..NOTIFICATION_FEED_RETENTION_LIMIT + 5 {
        let id = session(&format!("Bulk {index}"));
        insert_notification_feed_row(&db, row(&id, "failed")).expect("bulk");
    }
    let state = read_notification_feed_state(&db).expect("feed");
    assert_eq!(
        state["items"].as_array().unwrap().len(),
        NOTIFICATION_FEED_RETENTION_LIMIT
    );
    assert!(state["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["kind"] == "failed"));
}
