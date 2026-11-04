//! Push delivery end to end: a registration through the real route, agent events through
//! `/api/ingestAgentHookEvent` and the activity and program-status ingests, the feed rows they
//! commit, the push worker, and the HTTP bytes a fake Expo on a loopback port receives.

use super::*;
use crate::notification_feed::store::read_notification_feed_state;
use crate::push::{
    expo::ExpoClient,
    fake_expo::{Captured, FakeExpo},
    runtime::SystemClock,
    PushRuntime,
};
use std::time::Duration;

const TOKEN: &str = "ExponentPushToken[deliveryToken01]";

struct Fixture {
    state: Arc<AppState>,
    expo: FakeExpo,
    project_id: String,
    _temp: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let expo = FakeExpo::start(Arc::new(SystemClock)).await;
        let push = PushRuntime::start_with(
            paths.clone(),
            Arc::new(GxserverLogger::new(paths.clone())),
            Arc::new(SystemClock),
            ExpoClient::for_test_origin(expo.origin.clone()),
        );
        let state = test_app_state_with_push(paths, Arc::new(push));
        let db = open_gxserver_database(&state.paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project_id = repository
            .create_project(
                json!({ "name": "Delivery", "path": temp.path() })
                    .as_object()
                    .expect("params"),
            )
            .expect("project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        Self {
            state,
            expo,
            project_id,
            _temp: temp,
        }
    }

    async fn register(&self, kinds: (bool, bool, bool)) {
        let routed = route_http(
            self.state.clone(),
            rpc_request(
                "/api/registerPushToken",
                &self.state.auth_token,
                json!({ "protocolVersion": 1, "params": {
                    "action": "register", "token": TOKEN, "machineId": "computer-P",
                    "kinds": { "finished": kinds.0, "waiting": kinds.1, "error": kinds.2 },
                } }),
            ),
            "request-register".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
    }

    fn session(&self, title: &str, activity: &str) -> String {
        self.session_with(title, activity, json!({}))
    }

    /// A session whose runtime settings also hold `extra` (for example an unanswered question).
    fn session_with(&self, title: &str, activity: &str, extra: Value) -> String {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        let repository = DomainRepository::new(&db, self.state.metadata.server_id.as_str());
        let mut runtime = extra.as_object().cloned().unwrap_or_default();
        runtime.insert("agentName".to_string(), json!("claude"));
        runtime.insert("agentActivity".to_string(), json!({ "activity": activity }));
        repository
            .create_session(
                json!({
                    "projectId": self.project_id, "kind": "agent", "agentId": "claude",
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

    async fn call(&self, endpoint: &str, session_id: &str, params: Value) -> Value {
        let mut params = params.as_object().cloned().expect("params");
        params.insert("projectId".to_string(), json!(self.project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        let routed = route_http(
            self.state.clone(),
            rpc_request(
                endpoint,
                &self.state.auth_token,
                json!({ "params": params }),
            ),
            "request-delivery".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
        let body = response_json(routed.response).await;
        self.settle();
        body
    }

    async fn hook(&self, session_id: &str, params: Value) -> Value {
        self.call("/api/ingestAgentHookEvent", session_id, params)
            .await
    }

    fn settle(&self) {
        let push = &self.state.push;
        assert!(
            tokio::task::block_in_place(|| push.wait_until_idle(Duration::from_secs(10))),
            "the push worker did not settle"
        );
    }

    fn rows(&self, session_id: &str, kind: &str) -> Vec<Value> {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        read_notification_feed_state(&db).expect("feed")["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row["sessionId"] == session_id && row["kind"] == kind)
            .collect()
    }

    fn sends(&self) -> Vec<Captured> {
        self.expo.sends()
    }
}

fn message(request: &Captured) -> &Value {
    &request.body.as_array().expect("message array")[0]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_finished_uses_committed_feed_id() {
    let fixture = Fixture::new().await;
    let session_id = fixture.session("Secret plan for the parser", "working");

    // No phone registered: the row is written, nothing goes out.
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    assert_eq!(fixture.rows(&session_id, "finished").len(), 1);
    assert!(fixture.sends().is_empty());

    fixture.register((true, true, true)).await;
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "UserPromptSubmit" }),
        )
        .await;
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    // The first turn's row was read when the new turn started; the feed lists the newest first.
    let rows = fixture.rows(&session_id, "finished");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["read"], false);
    let alerts: Vec<Captured> = fixture
        .sends()
        .into_iter()
        .filter(|request| message(request)["data"]["kind"] == "finished")
        .collect();
    assert_eq!(alerts.len(), 1, "{:?}", fixture.sends());
    let alert = message(&alerts[0]);
    assert_eq!(alert["to"], TOKEN);
    assert_eq!(alert["title"], "Claude · Delivery");
    assert_eq!(alert["body"], "Finished");
    assert_eq!(
        alert["data"],
        json!({
            "kind": "finished",
            "machineId": "computer-P",
            "sessionId": session_id,
            "notificationId": rows[0]["id"],
        })
    );
    let text = alerts[0].body.to_string();
    assert!(!text.contains("Secret plan"));
    assert!(!text.contains(rows[0]["body"].as_str().expect("row body")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_uses_the_phones_custom_launcher_name() {
    let fixture = Fixture::new().await;
    fixture.register((true, true, true)).await;
    let session_id = fixture.session("Private custom work", "working");
    {
        let db = open_gxserver_database(&fixture.state.paths).expect("db");
        let repository = DomainRepository::new(&db, fixture.state.metadata.server_id.as_str());
        // Launcher settings are machine-wide, even when another project stores them.
        repository
            .create_project(
                json!({
                    "name": "Launcher settings",
                    "path": fixture._temp.path().join("launchers"),
                    "customAgents": [{
                        "agentId": "custom-reviewer", "command": "claude",
                        "icon": "claude", "name": "My reviewer",
                    }],
                    "customAgentOrder": ["custom-reviewer"],
                })
                .as_object()
                .expect("project params"),
            )
            .expect("launcher project");
        repository
            .update_session(
                json!({
                    "projectId": fixture.project_id, "sessionId": session_id,
                    "agentId": "custom-reviewer", "launchSettings": { "icon": "claude" },
                })
                .as_object()
                .expect("session params"),
            )
            .expect("custom launcher");
    }
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    let sends = fixture.sends();
    assert_eq!(sends.len(), 1, "{sends:?}");
    // The phone's sessionFacts prefers the matching launcher name to the runtime family.
    assert_eq!(message(&sends[0])["title"], "My reviewer · Delivery");
    assert_eq!(message(&sends[0])["body"], "Finished");
    assert!(!sends[0].body.to_string().contains("Private custom work"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_known_question_goes_out_as_needs_input() {
    let fixture = Fixture::new().await;
    // Waiting on, Finished off: a turn that ended with a question the host already knows reaches
    // the phone as needs input, with the finished row's id.
    fixture.register((false, true, false)).await;
    let asking = fixture.session_with(
        "Asks before stopping",
        "working",
        json!({ "sessionChatAsyncQuestionIds": ["known-before-stop"] }),
    );
    fixture
        .hook(
            &asking,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    let rows = fixture.rows(&asking, "finished");
    assert_eq!(rows.len(), 1);
    let sends = fixture.sends();
    assert_eq!(sends.len(), 1, "{sends:?}");
    assert_eq!(message(&sends[0])["body"], "Has a question");
    assert_eq!(message(&sends[0])["data"]["kind"], "needsInput");
    assert_eq!(message(&sends[0])["data"]["notificationId"], rows[0]["id"]);

    // The inverse: Finished on, Waiting off. The same kind of turn sends nothing, a plain one does.
    fixture.register((true, false, false)).await;
    let asking_again = fixture.session_with(
        "Asks again",
        "working",
        json!({ "sessionChatAsyncQuestionIds": ["another-question"] }),
    );
    fixture
        .hook(
            &asking_again,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    assert_eq!(fixture.sends().len(), 1);
    let plain = fixture.session("Plain turn", "working");
    fixture
        .hook(
            &plain,
            json!({ "agentName": "claude", "eventName": "Stop" }),
        )
        .await;
    let sends = fixture.sends();
    assert_eq!(sends.len(), 2, "{sends:?}");
    assert_eq!(message(&sends[1])["data"]["kind"], "finished");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_excluded_events_remain_local() {
    let fixture = Fixture::new().await;
    fixture.register((true, true, true)).await;
    let session_id = fixture.session("Bell session", "idle");

    // A terminal bell and a custom notification stay on the computer.
    fixture
        .call(
            "/api/updateAgentActivity",
            &session_id,
            json!({ "activity": "attention", "event": "bell" }),
        )
        .await;
    assert_eq!(fixture.rows(&session_id, "bell").len(), 1);
    let routed = route_http(
        fixture.state.clone(),
        rpc_request(
            "/api/createNotification",
            &fixture.state.auth_token,
            json!({ "params": { "sessionId": session_id, "projectId": fixture.project_id, "title": "Deploy done" } }),
        ),
        "request-custom".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);
    fixture.settle();
    // A subagent's stop does not finish the session.
    let waiting = fixture.session("Lead", "working");
    fixture
        .hook(
            &waiting,
            json!({ "agentName": "claude", "eventName": "SubagentStop" }),
        )
        .await;
    assert!(fixture.sends().is_empty(), "{:?}", fixture.sends());

    // A permission request is a wait for the user and is pushed as needs input.
    fixture
        .hook(
            &waiting,
            json!({ "agentName": "claude", "eventName": "PermissionRequest", "toolName": "Bash",
                    "toolInput": { "command": "rm -rf build" } }),
        )
        .await;
    let rows = fixture.rows(&waiting, "needsInput");
    assert_eq!(rows.len(), 1);
    let sends = fixture.sends();
    let alerts: Vec<&Captured> = sends
        .iter()
        .filter(|request| message(request).get("title").is_some())
        .collect();
    assert_eq!(alerts.len(), 1, "{sends:?}");
    assert_eq!(message(alerts[0])["body"], "Waiting for you");
    assert_eq!(message(alerts[0])["data"]["notificationId"], rows[0]["id"]);
    assert!(!alerts[0].body.to_string().contains("rm -rf"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_failure_notifies_only_after_commit() {
    let fixture = Fixture::new().await;
    fixture.register((false, false, true)).await;

    // A hook's final error: the failed row and its push, with the row's id.
    let session_id = fixture.session("Failing run", "working");
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "authentication_failed" }),
        )
        .await;
    let rows = fixture.rows(&session_id, "failed");
    assert_eq!(rows.len(), 1);
    let sends = fixture.sends();
    assert_eq!(sends.len(), 1, "{sends:?}");
    assert_eq!(message(&sends[0])["body"], "Ended with an error");
    assert_eq!(message(&sends[0])["data"]["kind"], "failed");
    assert_eq!(message(&sends[0])["data"]["notificationId"], rows[0]["id"]);

    // A program's error report.
    let program = fixture.session("Program run", "working");
    crate::program_status::ingest_program_status_line(
        &fixture.state,
        &fixture.project_id,
        &program,
        crate::program_status::ProgramStatusLine::Snapshot {
            reported: true,
            submits: None,
            records: vec![crate::program_status::ProgramStatusRecord {
                state: "error".to_string(),
                msg: Some("Request failed".to_string()),
                seq: 1,
                ..Default::default()
            }],
        },
    );
    fixture.settle();
    let rows = fixture.rows(&program, "failed");
    assert_eq!(rows.len(), 1);
    let sends = fixture.sends();
    assert_eq!(sends.len(), 2, "{sends:?}");
    assert_eq!(message(&sends[1])["data"]["notificationId"], rows[0]["id"]);

    // A failed row the database refuses is rolled back and never pushed.
    {
        let db = open_gxserver_database(&fixture.state.paths).expect("db");
        db.execute_batch(
            "CREATE TRIGGER refuse_failed BEFORE INSERT ON notification_feed WHEN NEW.kind = 'failed' BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .expect("trigger");
    }
    let refused = fixture.session("Refused run", "working");
    fixture
        .hook(
            &refused,
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "authentication_failed" }),
        )
        .await;
    assert!(fixture.rows(&refused, "failed").is_empty());
    assert_eq!(fixture.sends().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_agent_start_is_silent_and_once_per_start() {
    let fixture = Fixture::new().await;
    // Start messages do not depend on the three switches.
    fixture.register((false, false, false)).await;
    let session_id = fixture.session("Starting", "idle");
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "UserPromptSubmit" }),
        )
        .await;
    let sends = fixture.sends();
    assert_eq!(sends.len(), 1, "{sends:?}");
    let start = message(&sends[0]);
    assert_eq!(
        start["data"],
        json!({ "kind": "agentStarted", "machineId": "computer-P", "sessionId": session_id })
    );
    assert_eq!(start["contentAvailable"], true);
    assert_eq!(start["priority"], "normal");
    assert!(
        start.get("title").is_none() && start.get("body").is_none() && start.get("sound").is_none()
    );

    // Still working: no second start message. A client's activity call never sends one.
    fixture
        .hook(
            &session_id,
            json!({ "agentName": "claude", "eventName": "PreToolUse", "toolName": "Bash" }),
        )
        .await;
    let other = fixture.session("Client set", "idle");
    fixture
        .call(
            "/api/updateAgentActivity",
            &other,
            json!({ "activity": "working" }),
        )
        .await;
    assert_eq!(fixture.sends().len(), 1, "{:?}", fixture.sends());
}
