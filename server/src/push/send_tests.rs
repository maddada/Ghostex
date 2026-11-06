//! The real worker, store, limiter and Expo client against a fake Expo on a loopback port, with
//! the clock under the test's control.

use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::{
    domain::DomainRepository,
    logging::GxserverLogger,
    paths::{get_gxserver_paths, GxserverPaths},
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

use super::{
    expo::ExpoClient,
    fake_expo::{Captured, FakeExpo, Reply, RECEIPTS_PATH},
    message::{AgentStartEvent, AlertEvent, AlertKind},
    runtime::{PushClock, PushRuntime, RECEIPT_DELAY_MS, RECEIPT_KEEP_MS},
    store::{PushKinds, PushRegistration, PushRegistrationStore, PushStoreData},
};

const T0: i64 = 1_800_000_000_000;
const TOKEN_P: &str = "ExponentPushToken[phonePtoken0001]";
const TOKEN_P2: &str = "ExponentPushToken[phonePtoken0002]";
const TOKEN_Q: &str = "ExpoPushToken[phone_Q-token-0003]";
const TOKEN_Q2: &str = "ExpoPushToken[phone_Q-token-0004]";
const ALL_TOKENS: [&str; 4] = [TOKEN_P, TOKEN_P2, TOKEN_Q, TOKEN_Q2];
const ALL_ON: (bool, bool, bool) = (true, true, true);

struct FakeClock(AtomicI64);

impl PushClock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Harness {
    temp: tempfile::TempDir,
    paths: GxserverPaths,
    clock: Arc<FakeClock>,
    fake: FakeExpo,
    origin: String,
    runtime: PushRuntime,
}

impl Harness {
    async fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let clock = Arc::new(FakeClock(AtomicI64::new(T0)));
        let fake = FakeExpo::start(clock.clone()).await;
        let origin = fake.origin.clone();
        let runtime = start_runtime(&paths, &clock, &origin);
        Self {
            temp,
            paths,
            clock,
            fake,
            origin,
            runtime,
        }
    }

    fn register(&self, machine_id: &str, token: &str, kinds: (bool, bool, bool)) {
        let store = PushRegistrationStore::open(&self.paths);
        let mut data = store.read().expect("read store");
        data.registrations
            .retain(|registration| registration.machine_id != machine_id);
        data.registrations.push(PushRegistration {
            token: token.to_string(),
            machine_id: machine_id.to_string(),
            kinds: PushKinds {
                finished: kinds.0,
                waiting: kinds.1,
                error: kinds.2,
            },
            device_id: None,
            registered_at: "2026-10-31T10:00:00.000Z".to_string(),
        });
        store.write(&data).expect("write store");
    }

    fn unregister(&self, machine_id: &str) {
        let store = PushRegistrationStore::open(&self.paths);
        let mut data = store.read().expect("read store");
        data.registrations
            .retain(|registration| registration.machine_id != machine_id);
        store.write(&data).expect("write store");
    }

    fn stored(&self) -> PushStoreData {
        PushRegistrationStore::open(&self.paths)
            .read()
            .expect("read store")
    }

    fn reply(&self, reply: Reply) {
        self.fake.reply(reply);
    }

    fn requests(&self) -> Vec<Captured> {
        self.fake.requests()
    }

    fn sends(&self) -> Vec<Captured> {
        self.fake.sends()
    }

    fn settle(&self) {
        let runtime = &self.runtime;
        let idle = tokio::task::block_in_place(|| runtime.wait_until_idle(Duration::from_secs(10)));
        assert!(idle, "the push worker did not settle");
    }

    /// Moves the clock to `T0 + offset` and lets the worker do what is due.
    fn at(&self, offset_ms: i64) {
        self.clock.0.store(T0 + offset_ms, Ordering::SeqCst);
        self.runtime.wake();
        self.settle();
    }

    fn alert(&self, kind: AlertKind, session_id: &str, notification_id: &str) {
        self.runtime.enqueue_alert(alert_event(
            kind,
            session_id,
            notification_id,
            self.clock.now_ms(),
        ));
        self.settle();
    }

    fn start(&self, session_id: &str) {
        self.runtime.enqueue_agent_start(AgentStartEvent {
            session_id: session_id.to_string(),
            occurred_at_ms: self.clock.now_ms(),
        });
        self.settle();
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.paths.log_file).unwrap_or_default()
    }
}

fn start_runtime(paths: &GxserverPaths, clock: &Arc<FakeClock>, origin: &str) -> PushRuntime {
    PushRuntime::start_with(
        paths.clone(),
        Arc::new(GxserverLogger::new(paths.clone())),
        clock.clone(),
        ExpoClient::for_test_origin(origin.to_string()),
    )
}

fn alert_event(
    kind: AlertKind,
    session_id: &str,
    notification_id: &str,
    occurred_at_ms: i64,
) -> AlertEvent {
    AlertEvent {
        kind,
        project_id: "p-push".to_string(),
        session_id: session_id.to_string(),
        notification_id: notification_id.to_string(),
        agent_name: "OMP".to_string(),
        project_name: "demo-app".to_string(),
        occurred_at_ms,
    }
}

fn message(request: &Captured) -> &Value {
    let messages = request.body.as_array().expect("message array");
    assert_eq!(messages.len(), 1);
    &messages[0]
}

fn recipients(requests: &[Captured]) -> Vec<String> {
    requests
        .iter()
        .map(|request| message(request)["to"].as_str().expect("to").to_string())
        .collect()
}

fn assert_no_token_outside_store(root: &Path, store: &Path) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(&path).expect("read dir") {
                pending.push(entry.expect("entry").path());
            }
        } else if path != store {
            let text = String::from_utf8_lossy(&fs::read(&path).expect("read")).into_owned();
            for token in ALL_TOKENS {
                assert!(!text.contains(token), "token found in {}", path.display());
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_finished_alert_carries_only_event_and_names() {
    let h = Harness::new().await;

    // No phone registered: nothing is sent.
    h.alert(AlertKind::Finished, "s0", "nf-0");
    assert!(h.requests().is_empty());

    h.register("computer-P", TOKEN_P, (true, false, true));
    h.register("computer-Q", TOKEN_Q, (false, true, false));
    h.alert(AlertKind::Finished, "s1", "nf-1");
    let sends = h.sends();
    assert_eq!(sends.len(), 1, "only the phone with Finished on");
    assert_eq!(
        sends[0].body,
        json!([{
            "to": TOKEN_P,
            "title": "OMP · demo-app",
            "body": "Finished",
            "data": {
                "kind": "finished",
                "machineId": "computer-P",
                "sessionId": "s1",
                "notificationId": "nf-1",
            },
            "contentAvailable": true,
            "sound": "default",
            "ttl": 3600,
            "priority": "high",
        }])
    );

    h.alert(AlertKind::NeedsInput, "s2", "nf-2");
    h.alert(AlertKind::Failed, "s3", "nf-3");
    let sends = h.sends();
    assert_eq!(sends.len(), 3);
    assert_eq!(message(&sends[1])["to"], json!(TOKEN_Q));
    assert_eq!(message(&sends[1])["body"], json!("Waiting for you"));
    assert_eq!(message(&sends[1])["data"]["kind"], json!("needsInput"));
    assert_eq!(message(&sends[2])["to"], json!(TOKEN_P));
    assert_eq!(message(&sends[2])["body"], json!("Ended with an error"));
    assert_eq!(message(&sends[2])["data"]["kind"], json!("failed"));

    // Long, multi-line names stay on one line within 120 characters and the whole message within 4096 bytes.
    let mut long = alert_event(AlertKind::Finished, "s4", "nf-4", h.clock.now_ms());
    long.agent_name = format!("agent\n\u{7}{}", "\u{1F600}".repeat(200));
    long.project_name = "\"quoted\" \\ project".repeat(20);
    h.runtime.enqueue_alert(long);
    h.settle();
    let sends = h.sends();
    let last = message(&sends[3]);
    let title = last["title"].as_str().expect("title");
    assert!(title.chars().count() <= 120);
    assert!(title.starts_with("agent "));
    assert!(!title.chars().any(char::is_control));
    assert!(last.to_string().len() <= 4096);

    // Every accepted ticket waits for its receipt in the private store.
    let stored = h.stored();
    assert_eq!(stored.receipts.len(), 4);
    assert!(stored
        .receipts
        .iter()
        .all(|receipt| receipt.due_at == T0 + RECEIPT_DELAY_MS));
    assert_no_token_outside_store(
        h.temp.path(),
        &h.paths.auth_dir.join("push-registrations.json"),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_needs_input_reads_the_open_question_card_at_each_send() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, (false, true, false));
    let db = open_gxserver_database(&h.paths).expect("open db");
    let repository = DomainRepository::new(&db, "");
    let project = repository
        .create_project(
            json!({ "name": "demo-app", "path": h.temp.path() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let prompt = crate::session_chat_interactive::derive_session_chat_prompt(
        Some("AskUserQuestion"),
        Some(&json!({
            "questions": [{
                "question": "private-question", "header": "Pick", "multiSelect": false,
                "options": [{ "label": "A", "description": "First" }, { "label": "B", "description": "Second" }],
            }],
        })),
        Some("PreToolUse"),
    )
    .expect("question card");
    let asking = json!({
        "agentActivity": {
            "activity": "working",
            "sessionChatPrompt": serde_json::to_string(&prompt).expect("stored card"),
        },
    });
    let session = repository
        .create_session(
            json!({
                "projectId": project["projectId"],
                "kind": "agent",
                "agentId": "claude",
                "title": "private-session-title",
                "lifecycleState": "running",
                "runtimeSettings": asking,
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let mut event = alert_event(
        AlertKind::NeedsInput,
        session["sessionId"].as_str().expect("sessionId"),
        "nf-question",
        h.clock.now_ms(),
    );
    event.project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    h.reply(Reply::new(503, "{}"));
    h.reply(Reply::new(503, "{}"));
    h.runtime.enqueue_alert(event);
    h.settle();
    assert_eq!(message(&h.sends()[0])["body"], json!("Has a question"));

    repository
        .update_session(
            json!({
                "projectId": project["projectId"],
                "sessionId": session["sessionId"],
                "runtimeSettings": { "agentActivity": { "activity": "idle" } },
            })
            .as_object()
            .expect("answer params"),
        )
        .expect("answer question");
    h.at(1_000);
    assert_eq!(message(&h.sends()[1])["body"], json!("Waiting for you"));

    repository
        .update_session(
            json!({
                "projectId": project["projectId"],
                "sessionId": session["sessionId"],
                "runtimeSettings": asking,
            })
            .as_object()
            .expect("question params"),
        )
        .expect("ask again");
    h.at(3_000);
    let sends = h.sends();
    assert_eq!(sends.len(), 3);
    assert_eq!(message(&sends[2])["body"], json!("Has a question"));
    for request in &sends {
        let alert = message(request);
        assert_eq!(alert["title"], json!("OMP · demo-app"));
        assert_eq!(alert["data"]["kind"], json!("needsInput"));
        assert_eq!(alert["data"]["notificationId"], json!("nf-question"));
        assert!(!alert.to_string().contains("private-"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_start_is_silent_and_does_not_consume_alert_budget() {
    let h = Harness::new().await;
    // A start message ignores the three category switches.
    h.register("computer-P", TOKEN_P, (false, false, false));
    h.start("s1");
    let sends = h.sends();
    assert_eq!(sends.len(), 1);
    assert_eq!(
        sends[0].body,
        json!([{
            "to": TOKEN_P,
            "data": { "kind": "agentStarted", "machineId": "computer-P", "sessionId": "s1" },
            "contentAvailable": true,
            "ttl": 60,
            "priority": "normal",
        }])
    );

    h.register("computer-P", TOKEN_P, ALL_ON);
    h.at(1_000);
    h.start("s2");
    h.at(2_000);
    h.start("s3");
    assert_eq!(h.sends().len(), 2, "the hour's start budget is two");

    // Alerts right after keep their own budget: eight in the minute.
    for index in 0..8 {
        h.at(3_000 + index);
        h.alert(
            AlertKind::Finished,
            &format!("a{index}"),
            &format!("nf-a{index}"),
        );
    }
    let sends = h.sends();
    assert_eq!(sends.len(), 10);
    assert!(sends[2..]
        .iter()
        .all(|request| message(request)["data"]["kind"] == json!("finished")));

    // An alert never gets a second, background-only copy.
    assert_eq!(
        sends
            .iter()
            .filter(|request| message(request)["data"]["kind"] == json!("agentStarted"))
            .count(),
        2
    );

    // An hour after the first start, the budget has room again.
    h.at(3_600_000);
    h.start("s4");
    assert_eq!(h.sends().len(), 11);
    assert_eq!(message(&h.sends()[10])["data"]["sessionId"], json!("s4"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_limits_boundaries_and_phone_fanout() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    h.register("computer-Q", TOKEN_Q, ALL_ON);

    // One alert per session and phone in 20 s.
    h.alert(AlertKind::Finished, "s1", "nf-1");
    h.at(5_000);
    h.alert(AlertKind::NeedsInput, "s1", "nf-2");
    h.at(19_999);
    h.alert(AlertKind::Finished, "s1", "nf-3");
    assert_eq!(recipients(&h.sends()), [TOKEN_P, TOKEN_Q]);
    h.at(20_000);
    h.alert(AlertKind::Finished, "s1", "nf-4");
    assert_eq!(h.sends().len(), 4);

    // Ten sessions in a minute: each phone gets eight, counting the two above.
    for index in 0..10 {
        h.at(21_000 + index * 1_000);
        h.alert(
            AlertKind::Finished,
            &format!("t{index}"),
            &format!("nf-t{index}"),
        );
    }
    let sends = h.sends();
    let per_phone = |token: &str| {
        sends
            .iter()
            .filter(|request| message(request)["to"] == json!(token))
            .count()
    };
    assert_eq!(per_phone(TOKEN_P), 8);
    assert_eq!(per_phone(TOKEN_Q), 8);

    // The first send (t = 0) leaves the window at exactly 60 s, not a millisecond before.
    h.at(59_999);
    h.alert(AlertKind::Finished, "u1", "nf-u1");
    assert_eq!(h.sends().len(), 16);
    h.at(60_000);
    h.alert(AlertKind::Finished, "u2", "nf-u2");
    assert_eq!(h.sends().len(), 18);

    // A phone that registers later has its own budget.
    h.register("computer-R", TOKEN_Q2, ALL_ON);
    h.at(60_001);
    h.alert(AlertKind::Finished, "u3", "nf-u3");
    assert_eq!(recipients(&h.sends()[18..]), [TOKEN_Q2]);

    // The budget survives a restart.
    h.runtime.stop(Duration::from_secs(1));
    let restarted = start_runtime(&h.paths, &h.clock, &h.origin);
    restarted.enqueue_alert(alert_event(AlertKind::Finished, "u4", "nf-u4", T0 + 60_001));
    tokio::task::block_in_place(|| restarted.wait_until_idle(Duration::from_secs(10)));
    assert_eq!(h.sends().len(), 20, "only R had room: one request");
    restarted.stop(Duration::from_secs(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_expiry_never_moves_on_retry() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);

    // Two retries, 1 s then 2 s apart, same feed row id, shrinking ttl.
    h.reply(Reply::new(503, "{}"));
    h.reply(Reply::new(503, "{}"));
    h.alert(AlertKind::Finished, "s1", "nf-1");
    h.at(999);
    assert_eq!(h.sends().len(), 1);
    h.at(1_000);
    h.at(2_999);
    assert_eq!(h.sends().len(), 2);
    h.at(3_000);
    let sends = h.sends();
    assert_eq!(sends.len(), 3);
    assert_eq!(
        sends
            .iter()
            .map(|request| request.at - T0)
            .collect::<Vec<_>>(),
        [0, 1_000, 3_000]
    );
    assert!(sends
        .iter()
        .all(|request| message(request)["data"]["notificationId"] == json!("nf-1")));
    assert_eq!(
        sends
            .iter()
            .map(|request| message(request)["ttl"].as_i64().expect("ttl"))
            .collect::<Vec<_>>(),
        [3600, 3599, 3597]
    );

    // No third retry.
    for _ in 0..3 {
        h.reply(Reply::new(500, "{}"));
    }
    h.at(30_000);
    h.alert(AlertKind::Finished, "s2", "nf-2");
    h.at(31_000);
    h.at(33_000);
    h.at(60_000);
    assert_eq!(h.sends().len(), 6);

    // Expo's Retry-After is obeyed.
    let mut limited = Reply::new(
        429,
        json!({ "errors": [{ "code": "TOO_MANY_REQUESTS" }] }).to_string(),
    );
    limited.headers.push(("retry-after", "30".to_string()));
    h.reply(limited);
    h.at(100_000);
    h.alert(AlertKind::Finished, "s3", "nf-3");
    h.at(101_000);
    assert_eq!(h.sends().len(), 7);
    h.at(130_000);
    assert_eq!(h.sends().len(), 8);

    // An event half a second from expiry is not retried past it.
    h.reply(Reply::new(503, "{}"));
    h.at(200_000);
    h.runtime.enqueue_alert(alert_event(
        AlertKind::Finished,
        "s4",
        "nf-4",
        T0 + 200_000 - 3_599_500,
    ));
    h.settle();
    assert_eq!(message(h.sends().last().unwrap())["ttl"], json!(1));
    h.at(205_000);
    assert_eq!(h.sends().len(), 9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_backward_clock_keeps_the_budget_spent() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    for index in 0..8 {
        h.at(index);
        h.alert(
            AlertKind::Finished,
            &format!("b{index}"),
            &format!("nf-b{index}"),
        );
    }
    assert_eq!(h.sends().len(), 8);

    // The clock is set back an hour: the eight sends still count, nothing catches up.
    h.at(-3_600_000);
    h.alert(AlertKind::Finished, "c1", "nf-c1");
    h.at(-3_600_000 + 59_000);
    h.alert(AlertKind::Finished, "c2", "nf-c2");
    assert_eq!(h.sends().len(), 8);

    // Once the window after those sends has passed on the new clock, alerts go out again.
    h.at(60_000);
    h.alert(AlertKind::Finished, "c3", "nf-c3");
    assert_eq!(h.sends().len(), 9);
    assert_eq!(message(&h.sends()[8])["data"]["sessionId"], json!("c3"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_rotation_and_unregistration_cancel_only_that_phone() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    h.register("computer-Q", TOKEN_Q, ALL_ON);
    h.register("computer-R", TOKEN_Q2, ALL_ON);
    for _ in 0..3 {
        h.reply(Reply::new(503, "{}"));
    }
    h.alert(AlertKind::Finished, "s1", "nf-1");
    assert_eq!(h.sends().len(), 3);

    // P rotates and Q unregisters while their retries wait; R's retry still goes out.
    h.register("computer-P", TOKEN_P2, ALL_ON);
    h.unregister("computer-Q");
    h.at(1_000);
    assert_eq!(recipients(&h.sends()[3..]), [TOKEN_Q2]);

    // A switched-off kind cancels a waiting retry too; R, whose switch stays on, still retries.
    h.reply(Reply::new(503, "{}"));
    h.reply(Reply::new(503, "{}"));
    h.at(30_000);
    h.alert(AlertKind::NeedsInput, "s2", "nf-2");
    let before = h.sends().len();
    assert_eq!(before, 6);
    h.register("computer-P", TOKEN_P2, (true, false, true));
    h.at(31_000);
    assert_eq!(recipients(&h.sends()[before..]), [TOKEN_Q2]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_receipts_remove_only_current_dead_token() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    h.register("computer-Q", TOKEN_Q, ALL_ON);
    h.alert(AlertKind::Finished, "s1", "nf-1");
    assert_eq!(h.sends().len(), 2);

    h.at(RECEIPT_DELAY_MS - 1);
    assert!(h
        .requests()
        .iter()
        .all(|request| request.path != RECEIPTS_PATH));

    // Fifteen minutes on: one receipts request for both tickets. P's token is dead, Q's
    // receipt is not ready yet.
    h.reply(Reply::new(
        200,
        json!({ "data": { "ticket-1": {
            "status": "error",
            "message": format!("\"{TOKEN_P}\" is not a registered push notification recipient"),
            "details": { "error": "DeviceNotRegistered" },
        } } })
        .to_string(),
    ));
    h.at(RECEIPT_DELAY_MS);
    let receipts: Vec<Captured> = h
        .requests()
        .into_iter()
        .filter(|request| request.path == RECEIPTS_PATH)
        .collect();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].body, json!({ "ids": ["ticket-1", "ticket-2"] }));
    let stored = h.stored();
    assert!(stored.registration("computer-P").is_none());
    assert!(stored.registration("computer-Q").is_some());
    assert_eq!(stored.receipts.len(), 1);

    // Q rotates before its receipt reports the old token dead: the new token stays.
    h.register("computer-Q", TOKEN_Q2, ALL_ON);
    h.reply(Reply::new(
        200,
        json!({ "data": { "ticket-2": { "status": "error", "details": { "error": "DeviceNotRegistered" } } } })
            .to_string(),
    ));
    // The receipt is checked again after a restart.
    h.runtime.stop(Duration::from_secs(1));
    let restarted = start_runtime(&h.paths, &h.clock, &h.origin);
    h.clock.0.store(T0 + 2 * RECEIPT_DELAY_MS, Ordering::SeqCst);
    restarted.wake();
    tokio::task::block_in_place(|| restarted.wait_until_idle(Duration::from_secs(10)));
    let stored = h.stored();
    assert_eq!(
        stored
            .registration("computer-Q")
            .expect("replacement kept")
            .token,
        TOKEN_Q2
    );
    assert!(stored.receipts.is_empty());

    // A dead token on the ticket itself is deleted at once.
    h.reply(Reply::new(
        200,
        json!({ "data": [{ "status": "error", "details": { "error": "DeviceNotRegistered" } }] })
            .to_string(),
    ));
    restarted.enqueue_alert(alert_event(
        AlertKind::Finished,
        "s2",
        "nf-2",
        T0 + 2 * RECEIPT_DELAY_MS,
    ));
    tokio::task::block_in_place(|| restarted.wait_until_idle(Duration::from_secs(10)));
    assert!(h.stored().registrations.is_empty());

    // A receipt Expo never reports is dropped after a day.
    restarted.stop(Duration::from_secs(1));
    h.register("computer-S", TOKEN_P2, ALL_ON);
    let restarted = start_runtime(&h.paths, &h.clock, &h.origin);
    restarted.enqueue_alert(alert_event(
        AlertKind::Finished,
        "s3",
        "nf-3",
        T0 + 2 * RECEIPT_DELAY_MS,
    ));
    tokio::task::block_in_place(|| restarted.wait_until_idle(Duration::from_secs(10)));
    assert_eq!(h.stored().receipts.len(), 1);
    let mut offset = 3 * RECEIPT_DELAY_MS;
    while offset <= 2 * RECEIPT_DELAY_MS + RECEIPT_KEEP_MS {
        h.clock.0.store(T0 + offset, Ordering::SeqCst);
        restarted.wake();
        tokio::task::block_in_place(|| restarted.wait_until_idle(Duration::from_secs(10)));
        offset += RECEIPT_DELAY_MS;
    }
    assert!(h.stored().receipts.is_empty());
    assert!(h.stored().registration("computer-S").is_some());
    restarted.stop(Duration::from_secs(1));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_receipts_pause_while_the_store_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    h.alert(AlertKind::Finished, "s1", "nf-1");
    assert_eq!(h.sends().len(), 1);
    let receipt_reads = || {
        h.requests()
            .iter()
            .filter(|request| request.path == RECEIPTS_PATH)
            .count()
    };

    // Expo has no receipt yet, and the folder refuses the store's new file: the receipt keeps
    // its old due time on disk, and the next reading still waits the receipt delay.
    let auth_dir = h.paths.auth_dir.clone();
    fs::set_permissions(&auth_dir, fs::Permissions::from_mode(0o500)).expect("chmod");
    h.at(RECEIPT_DELAY_MS);
    assert_eq!(receipt_reads(), 1);
    h.at(2 * RECEIPT_DELAY_MS - 1);
    assert_eq!(receipt_reads(), 1);
    h.at(2 * RECEIPT_DELAY_MS);
    assert_eq!(receipt_reads(), 2);
    fs::set_permissions(&auth_dir, fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(h.log().contains("pushStoreUnwritable"));
    assert_eq!(h.stored().receipts[0].due_at, T0 + RECEIPT_DELAY_MS);
    h.runtime.stop(Duration::from_secs(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_provider_errors_are_bounded_and_redacted() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    let quoted = format!("\"{TOKEN_P}\" SECRET-MESSAGE");
    let refusals = [
        Reply::new(400, json!({ "errors": [{ "code": "VALIDATION_ERROR", "message": quoted }] }).to_string()),
        Reply::new(401, json!({ "errors": [{ "code": "UNAUTHORIZED", "message": quoted }] }).to_string()),
        Reply::new(200, "not json at all"),
        Reply::new(200, json!({ "data": [] }).to_string()),
        Reply::new(
            200,
            json!({ "data": [{ "status": "error", "message": quoted, "details": { "error": "MessageTooBig" } }] })
                .to_string(),
        ),
        Reply::new(200, json!({ "data": [{ "status": "error", "details": { "error": format!("bad {TOKEN_P}") } }] }).to_string()),
    ];
    let count = refusals.len();
    for (index, reply) in refusals.into_iter().enumerate() {
        h.reply(reply);
        let offset = index as i64 * 30_000;
        h.at(offset);
        h.alert(
            AlertKind::Finished,
            &format!("s{index}"),
            &format!("nf-{index}"),
        );
        // Refused for good: no retry follows.
        h.at(offset + 10_000);
        assert_eq!(h.sends().len(), index + 1);
    }
    assert_eq!(h.sends().len(), count);
    assert!(h.stored().receipts.is_empty());
    assert!(h.stored().registration("computer-P").is_some());

    let log = h.log();
    for code in [
        "VALIDATION_ERROR",
        "UNAUTHORIZED",
        "malformedResponse",
        "MessageTooBig",
        "unknown",
    ] {
        assert!(log.contains(code), "{code} missing from {log}");
    }
    assert!(!log.contains("SECRET-MESSAGE"));
    assert_no_token_outside_store(
        h.temp.path(),
        &h.paths.auth_dir.join("push-registrations.json"),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn push_send_queue_pressure_and_shutdown_are_bounded() {
    let h = Harness::new().await;
    h.register("computer-P", TOKEN_P, ALL_ON);
    let mut stalled = Reply::new(200, "{}");
    stalled.delay = Duration::from_secs(20);
    h.reply(stalled);
    h.runtime
        .enqueue_alert(alert_event(AlertKind::Finished, "s0", "nf-0", T0));
    // Wait until the worker is inside the stalled request.
    let started = Instant::now();
    while h.sends().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // 64 alerts fit while the worker is busy; the rest are dropped with one warning.
    for index in 1..=70 {
        h.runtime.enqueue_alert(alert_event(
            AlertKind::Finished,
            &format!("s{index}"),
            &format!("nf-{index}"),
            T0,
        ));
    }
    h.runtime.enqueue_agent_start(AgentStartEvent {
        session_id: "s-start".to_string(),
        occurred_at_ms: T0,
    });
    assert_eq!(h.log().matches("pushQueueFull").count(), 1);

    let stopping = Instant::now();
    let finished = tokio::task::block_in_place(|| h.runtime.stop(Duration::from_secs(7)));
    let took = stopping.elapsed();
    assert!(finished, "the worker did not stop");
    assert!(took < Duration::from_secs(7), "stop took {took:?}");
    // The queued alerts were dropped, not sent.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(h.sends().len(), 1);
}
