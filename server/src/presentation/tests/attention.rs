//! What a session in attention publishes: its kind, the start of the stint it ended and the
//! Notifications row of its event, through the real activity endpoint, feed store, full snapshot
//! and session delta.

use rusqlite::Connection;
use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;
use crate::notification_feed::store::{
    clear_notification_feed, dismiss_notification, insert_notification_feed_row,
    mark_notification_read, notification_exists, NewNotificationFeedRow,
    NOTIFICATION_FEED_KIND_FINISHED, NOTIFICATION_FEED_KIND_NEEDS_INPUT,
};

const SERVER: &str = "attention-host";
/// 2026-06-07T06:47:25.000Z.
const T0: i64 = 1_780_814_845_000;

fn iso(ms: i64) -> String {
    crate::session_status::iso_from_ms(ms)
}

fn create_session(
    db: &Connection,
    lifecycle_state: &str,
    runtime_settings: Value,
) -> (String, String) {
    let repository = DomainRepository::new(db, SERVER);
    let project = repository
        .create_project(
            json!({ "name": "Attention", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let provider_state = if lifecycle_state == "running" {
        "exists"
    } else {
        "missing"
    };
    let session = repository
        .create_session(
            json!({
                "projectId": project_id,
                "kind": "agent",
                "agentId": "codex",
                "title": "Attention session",
                "lifecycleState": lifecycle_state,
                "providerState": { "lifecycleState": provider_state },
                "runtimeSettings": runtime_settings,
                // A stopped row stays in the presentation only while it is kept, e.g. pinned.
                "isPinned": lifecycle_state == "stopped",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    (
        project_id,
        session["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string(),
    )
}

/// `/api/updateAgentActivity`, the endpoint hooks, titles and acknowledgements go through.
fn post_activity(db: &Connection, project_id: &str, session_id: &str, params: Value) -> Value {
    let mut params = params.as_object().cloned().expect("activity params");
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    crate::agents::dispatch_agent_endpoint(
        &DomainRepository::new(db, SERVER),
        db,
        &std::env::temp_dir(),
        "/api/updateAgentActivity",
        &params,
        None,
    )
    .expect("activity update")
    .result
}

fn work(db: &Connection, project_id: &str, session_id: &str, at: i64) {
    post_activity(
        db,
        project_id,
        session_id,
        json!({ "activity": "working", "nowMs": at }),
    );
}

/// A Stop hook: the turn finished. Returns the stored attention event id.
fn stop(db: &Connection, project_id: &str, session_id: &str, at: i64) -> String {
    let result = post_activity(
        db,
        project_id,
        session_id,
        json!({ "activity": "attention", "attentionSource": "turnComplete", "nowMs": at }),
    );
    result["activity"]["attentionEventId"]
        .as_str()
        .expect("stored attention event id")
        .to_string()
}

fn acknowledge(db: &Connection, project_id: &str, session_id: &str, at: i64) {
    post_activity(
        db,
        project_id,
        session_id,
        json!({ "event": "acknowledge", "nowMs": at }),
    );
}

/// Replaces (or with `None` removes) the session's stored chat prompt, as a hook or an answer does.
fn set_stored_prompt(db: &Connection, project_id: &str, session_id: &str, prompt: Option<String>) {
    let repository = DomainRepository::new(db, SERVER);
    let session = repository
        .get_session(project_id, session_id)
        .expect("get session")
        .expect("session row");
    let mut runtime_settings = session["runtimeSettings"].clone();
    match prompt {
        Some(prompt) => runtime_settings["agentActivity"]["sessionChatPrompt"] = json!(prompt),
        None => {
            runtime_settings["agentActivity"]
                .as_object_mut()
                .expect("agent activity")
                .remove("sessionChatPrompt");
        }
    }
    repository
        .update_session(
            json!({
                "projectId": project_id,
                "sessionId": session_id,
                "runtimeSettings": runtime_settings,
            })
            .as_object()
            .expect("update params"),
        )
        .expect("update session");
}

fn question_prompt() -> String {
    json!({
        "kind": "question",
        "questions": [{ "question": "Which one?", "options": [{ "label": "A" }] }],
    })
    .to_string()
}

fn approval_prompt() -> String {
    json!({ "kind": "approval", "tool": "Bash", "summary": "rm -rf build" }).to_string()
}

/// A Notifications row as the feed producer writes it on entering attention.
fn write_row(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    attention_event_id: Option<&str>,
) -> String {
    insert_notification_feed_row(
        db,
        NewNotificationFeedRow {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            kind: NOTIFICATION_FEED_KIND_FINISHED,
            title: "Attention session".to_string(),
            subtitle: "Attention".to_string(),
            body: "Finished a turn".to_string(),
            agent_name: None,
            attention_event_id: attention_event_id.map(str::to_string),
        },
    )
    .expect("feed row")["id"]
        .as_str()
        .expect("row id")
        .to_string()
}

fn set_row_created_at(db: &Connection, id: &str, created_at: &str) {
    db.execute(
        "UPDATE notification_feed SET createdAt = ?1 WHERE id = ?2",
        rusqlite::params![created_at, id],
    )
    .expect("set createdAt");
}

fn snapshot_session(db: &Connection, project_id: &str, session_id: &str) -> Value {
    let sessions = DomainRepository::new(db, SERVER)
        .list_sessions(None)
        .expect("sessions");
    let snapshot =
        read_presentation_snapshot(db, SERVER, None, false, false, sessions).expect("snapshot");
    assert_eq!(
        snapshot["capabilities"]["attentionNotificationId"],
        json!(true)
    );
    snapshot["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|session| {
            session["projectId"] == json!(project_id) && session["sessionId"] == json!(session_id)
        })
        .cloned()
        .expect("presented session")
}

fn delta_session(db: &Connection, project_id: &str, session_id: &str) -> Value {
    let repository = DomainRepository::new(db, SERVER);
    let delta = build_presentation_session_delta(db, &repository, project_id, session_id)
        .expect("session delta");
    assert_eq!(delta["type"], json!("sessionPresentationChanged"));
    delta["session"].clone()
}

/// The session as the full snapshot publishes it, after checking the session delta publishes the
/// same attention and working stint.
fn presented(db: &Connection, project_id: &str, session_id: &str) -> Value {
    let full = snapshot_session(db, project_id, session_id);
    let delta = delta_session(db, project_id, session_id);
    assert_eq!(
        full.get("attention"),
        delta.get("attention"),
        "the snapshot and the delta publish the same attention"
    );
    assert_eq!(full.get("workingStartedAt"), delta.get("workingStartedAt"));
    full
}

fn notification_id(session: &Value) -> Option<&str> {
    session["attention"]["notificationId"].as_str()
}

#[test]
fn finished_attention_keeps_its_stint_start() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    work(&db, &project_id, &session_id, T0 + 3_000);
    let working = presented(&db, &project_id, &session_id);
    assert_eq!(working["activity"], json!("working"));
    assert_eq!(
        working["workingStartedAt"],
        json!(iso(T0)),
        "a repeated working update keeps the stint's start"
    );
    assert!(working.get("attention").is_none());

    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);
    let finished = presented(&db, &project_id, &session_id);
    assert_eq!(finished["activity"], json!("attention"));
    assert!(
        finished.get("workingStartedAt").is_none(),
        "the top-level field keeps meaning a stint that works now"
    );
    assert_eq!(
        finished["attention"],
        json!({
            "acknowledged": false,
            "alerts": true,
            "enteredAt": iso(T0 + 30_000),
            "eventId": event_id,
            "kind": "finished",
            "workingStartedAt": iso(T0),
        }),
        "no Notifications row exists yet, so no notificationId"
    );
}

#[test]
fn acknowledge_removes_nested_attention_fields() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);
    write_row(&db, &project_id, &session_id, Some(&event_id));
    assert!(notification_id(&presented(&db, &project_id, &session_id)).is_some());

    acknowledge(&db, &project_id, &session_id, T0 + 40_000);
    let acknowledged = presented(&db, &project_id, &session_id);
    assert_eq!(acknowledged["activity"], json!("idle"));
    assert!(
        acknowledged.get("attention").is_none(),
        "acknowledging removes the attention with its kind, start and row link"
    );
    assert!(acknowledged.get("workingStartedAt").is_none());
}

#[test]
fn new_stint_replaces_the_previous_start() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let first_event = stop(&db, &project_id, &session_id, T0 + 30_000);
    acknowledge(&db, &project_id, &session_id, T0 + 40_000);

    let t1 = T0 + 60_000;
    work(&db, &project_id, &session_id, t1);
    work(&db, &project_id, &session_id, t1 + 5_000);
    assert_eq!(
        presented(&db, &project_id, &session_id)["workingStartedAt"],
        json!(iso(t1))
    );
    let second_event = stop(&db, &project_id, &session_id, t1 + 30_000);
    assert_ne!(first_event, second_event);
    let finished = presented(&db, &project_id, &session_id);
    assert_eq!(finished["attention"]["workingStartedAt"], json!(iso(t1)));
    assert_eq!(finished["attention"]["eventId"], json!(second_event));
}

#[test]
fn attention_kind_prefers_unanswered_inputs() {
    let (_temp, db) = open_test_database();
    let kind_after_stop = |runtime_settings: Value, prompt: Option<String>| -> (Value, Value) {
        let (project_id, session_id) = create_session(&db, "running", runtime_settings);
        work(&db, &project_id, &session_id, T0);
        if prompt.is_some() {
            set_stored_prompt(&db, &project_id, &session_id, prompt);
        }
        stop(&db, &project_id, &session_id, T0 + 30_000);
        let session = presented(&db, &project_id, &session_id);
        assert_eq!(session["activity"], json!("attention"));
        (session["attention"]["kind"].clone(), session)
    };

    let (kind, _) = kind_after_stop(json!({}), None);
    assert_eq!(kind, json!("finished"), "nothing waits on the user");

    let (kind, session) = kind_after_stop(json!({ "sessionChatAsyncQuestionIds": ["q1"] }), None);
    assert_eq!(session["pendingQuestionCount"], json!(1));
    assert_eq!(kind, json!("needsInput"), "an unanswered async question");

    let (kind, session) =
        kind_after_stop(json!({ "sessionChatOpenQuestions": { "count": 2 } }), None);
    assert_eq!(session["pendingQuestionCount"], json!(2));
    assert_eq!(kind, json!("needsInput"), "an open question form");

    let (kind, session) = kind_after_stop(json!({}), Some(approval_prompt()));
    assert_eq!(session["awaitingApproval"], json!(true));
    assert_eq!(kind, json!("needsInput"), "a stored approval card");

    let (kind, session) = kind_after_stop(json!({}), Some(question_prompt()));
    assert!(session.get("awaitingApproval").is_none());
    assert_eq!(session["pendingQuestionCount"], json!(0));
    assert_eq!(kind, json!("needsInput"), "a stored question card");

    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let (card_id, card_prompt) =
        crate::connectors::hold_waiting_connector_card(&project_id, &session_id);
    set_stored_prompt(&db, &project_id, &session_id, Some(card_prompt));
    stop(&db, &project_id, &session_id, T0 + 30_000);
    let session = presented(&db, &project_id, &session_id);
    crate::connectors::release_waiting_connector_card(&card_id);
    assert_eq!(
        session["attention"]["eventId"],
        json!(format!("connectorcard:{card_id}"))
    );
    assert_eq!(
        session["attention"]["kind"],
        json!("needsInput"),
        "a waiting connector card"
    );

    // A sleeping or stopped row keeps its stored attention. Its unanswered questions still keep
    // the turn from being finished, although its published `pendingQuestionCount` stays 0.
    let retained = |lifecycle_state: &str, extra: Value, prompt: Option<String>| {
        let mut agent_activity = json!({
            "activity": "attention",
            "agentName": "codex",
            "attentionEventId": "attn_retained",
            "attentionSource": "turnComplete",
            "isAcknowledged": false,
            "lastChangedAt": iso(T0 + 30_000),
            "workingStartedAt": iso(T0),
        });
        if let Some(prompt) = prompt {
            agent_activity["sessionChatPrompt"] = json!(prompt);
        }
        let mut runtime_settings = extra;
        runtime_settings["agentActivity"] = agent_activity;
        let (project_id, session_id) = create_session(&db, lifecycle_state, runtime_settings);
        let session = presented(&db, &project_id, &session_id);
        assert_eq!(session["lifecycleState"], json!(lifecycle_state));
        assert_eq!(session["activity"], json!("attention"));
        assert_eq!(session["pendingQuestionCount"], json!(0));
        session["attention"]["kind"].clone()
    };
    for lifecycle_state in ["sleeping", "stopped"] {
        assert_eq!(
            retained(lifecycle_state, json!({}), None),
            json!("finished"),
            "{lifecycle_state}: nothing waits"
        );
        assert_eq!(
            retained(
                lifecycle_state,
                json!({ "sessionChatAsyncQuestionIds": ["q1"] }),
                None
            ),
            json!("needsInput"),
            "{lifecycle_state}: an unanswered async question"
        );
        assert_eq!(
            retained(
                lifecycle_state,
                json!({ "sessionChatOpenQuestions": { "count": 2 } }),
                None
            ),
            json!("needsInput"),
            "{lifecycle_state}: an open question form"
        );
        assert_eq!(
            retained(lifecycle_state, json!({}), Some(question_prompt())),
            json!("needsInput"),
            "{lifecycle_state}: a stored question card"
        );
    }
}

#[test]
fn acknowledged_stint_start_reaches_no_later_attention() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    stop(&db, &project_id, &session_id, T0 + 30_000);
    assert_eq!(
        presented(&db, &project_id, &session_id)["attention"]["workingStartedAt"],
        json!(iso(T0)),
        "a finished stint keeps its start"
    );

    // Work, finish, acknowledge, then a bell with no working stint of its own.
    acknowledge(&db, &project_id, &session_id, T0 + 40_000);
    post_activity(
        &db,
        &project_id,
        &session_id,
        json!({ "event": "bell", "nowMs": T0 + 50_000 }),
    );
    let bell = presented(&db, &project_id, &session_id);
    assert_eq!(bell["activity"], json!("attention"));
    assert!(
        bell["attention"].get("workingStartedAt").is_none(),
        "the acknowledged stint's start does not reach the bell"
    );

    // Acknowledge, then a Stop with no working stint of its own.
    acknowledge(&db, &project_id, &session_id, T0 + 60_000);
    stop(&db, &project_id, &session_id, T0 + 70_000);
    let stopped = presented(&db, &project_id, &session_id);
    assert_eq!(stopped["attention"]["kind"], json!("finished"));
    assert!(stopped["attention"].get("workingStartedAt").is_none());

    // A new stint sets the start again.
    acknowledge(&db, &project_id, &session_id, T0 + 80_000);
    work(&db, &project_id, &session_id, T0 + 90_000);
    stop(&db, &project_id, &session_id, T0 + 120_000);
    assert_eq!(
        presented(&db, &project_id, &session_id)["attention"]["workingStartedAt"],
        json!(iso(T0 + 90_000))
    );
}

/// A row acknowledged by an earlier build is stored idle and acknowledged with its old start
/// still set. After the database is reopened, a bell, a Stop or a program's attention with no
/// working stint of their own publish no start.
#[test]
fn stored_acknowledged_start_reaches_no_later_attention() {
    let (temp, db) = open_test_database();
    let legacy_activity = json!({
        "activity": "idle",
        "agentName": "codex",
        "hasSeenWorking": true,
        "isAcknowledged": true,
        "lastChangedAt": iso(T0 + 40_000),
        "lastMeaningfulActivityAt": iso(T0 + 30_000),
        "workingStartedAt": iso(T0),
    });
    let sessions = ["bell", "stop", "program"].map(|route| {
        let (project_id, session_id) = create_session(
            &db,
            "running",
            json!({ "agentActivity": legacy_activity.clone() }),
        );
        (route, project_id, session_id)
    });
    drop(db);
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let db = crate::storage::open_gxserver_database(&paths).expect("reopen db");

    for (route, project_id, session_id) in &sessions {
        let stored = DomainRepository::new(&db, SERVER)
            .get_session(project_id, session_id)
            .expect("get session")
            .expect("session row");
        assert_eq!(
            stored["runtimeSettings"]["agentActivity"]["workingStartedAt"],
            json!(iso(T0)),
            "{route}: the reopened row still stores the old start"
        );
        let params = match *route {
            "bell" => json!({ "event": "bell", "nowMs": T0 + 90_000 }),
            "stop" => json!({
                "activity": "attention",
                "attentionSource": "turnComplete",
                "nowMs": T0 + 90_000,
            }),
            _ => json!({
                "programStatusActivity": "attention",
                "attentionSource": "turnComplete",
                "nowMs": T0 + 90_000,
            }),
        };
        let result = post_activity(&db, project_id, session_id, params);
        assert_eq!(result["enteredAttention"], json!(true), "{route}");
        let session = presented(&db, project_id, session_id);
        assert_eq!(session["activity"], json!("attention"), "{route}");
        assert_eq!(
            session["attention"]["enteredAt"],
            json!(iso(T0 + 90_000)),
            "{route}"
        );
        assert!(
            session["attention"].get("workingStartedAt").is_none(),
            "{route}: the acknowledged stint's start does not reach the attention"
        );
    }
}

/// A connector card puts a session in attention without a stint of its own: over a stored
/// acknowledged row it publishes no start, over a working session it publishes the working start.
#[test]
fn connector_attention_takes_only_a_working_start() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(
        &db,
        "running",
        json!({
            "agentActivity": {
                "activity": "idle",
                "agentName": "codex",
                "hasSeenWorking": true,
                "isAcknowledged": true,
                "lastChangedAt": iso(T0 + 40_000),
                "workingStartedAt": iso(T0),
            },
        }),
    );
    let (card_id, card_prompt) =
        crate::connectors::hold_waiting_connector_card(&project_id, &session_id);
    set_stored_prompt(&db, &project_id, &session_id, Some(card_prompt));
    let acknowledged = presented(&db, &project_id, &session_id);
    crate::connectors::release_waiting_connector_card(&card_id);
    assert_eq!(
        acknowledged["attention"]["eventId"],
        json!(format!("connectorcard:{card_id}"))
    );
    assert!(
        acknowledged["attention"].get("workingStartedAt").is_none(),
        "the acknowledged stint's start does not reach the card attention"
    );

    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let (card_id, card_prompt) =
        crate::connectors::hold_waiting_connector_card(&project_id, &session_id);
    set_stored_prompt(&db, &project_id, &session_id, Some(card_prompt));
    let working = presented(&db, &project_id, &session_id);
    crate::connectors::release_waiting_connector_card(&card_id);
    assert_eq!(working["activity"], json!("attention"));
    assert_eq!(working["attention"]["workingStartedAt"], json!(iso(T0)));
    assert_eq!(working["workingStartedAt"], json!(iso(T0)));
}

#[test]
fn bell_attention_is_needs_input() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let result = post_activity(
        &db,
        &project_id,
        &session_id,
        json!({ "event": "bell", "nowMs": T0 + 30_000 }),
    );
    assert_eq!(result["enteredAttention"], json!(true));
    let session = presented(&db, &project_id, &session_id);
    assert_eq!(session["attention"]["kind"], json!("needsInput"));
    assert_eq!(
        session["attention"]["eventId"],
        result["activity"]["attentionEventId"]
    );
}

#[test]
fn stop_on_standing_prompt_remains_needs_input() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    set_stored_prompt(&db, &project_id, &session_id, Some(question_prompt()));
    let asked = post_activity(
        &db,
        &project_id,
        &session_id,
        json!({ "activity": "attention", "nowMs": T0 + 10_000 }),
    );
    let event_id = asked["activity"]["attentionEventId"].clone();
    assert_eq!(
        presented(&db, &project_id, &session_id)["attention"]["kind"],
        json!("needsInput")
    );

    let stopped = post_activity(
        &db,
        &project_id,
        &session_id,
        json!({ "activity": "attention", "attentionSource": "turnComplete", "nowMs": T0 + 30_000 }),
    );
    assert_eq!(stopped["enteredAttention"], json!(false));
    assert_eq!(
        stopped["activity"]["attentionSource"],
        json!("turnComplete")
    );
    let session = presented(&db, &project_id, &session_id);
    assert_eq!(
        session["attention"]["eventId"], event_id,
        "the same attention"
    );
    assert_eq!(
        session["attention"]["kind"],
        json!("needsInput"),
        "the prompt still waits on the user"
    );
}

#[test]
fn answered_prompt_uses_current_presentation_truth() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    set_stored_prompt(&db, &project_id, &session_id, Some(question_prompt()));
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);
    let row_id = insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            kind: NOTIFICATION_FEED_KIND_NEEDS_INPUT,
            title: "Attention session".to_string(),
            subtitle: "Attention".to_string(),
            body: "Waiting for your input".to_string(),
            agent_name: None,
            attention_event_id: Some(event_id.clone()),
        },
    )
    .expect("feed row")["id"]
        .as_str()
        .expect("row id")
        .to_string();
    assert_eq!(
        presented(&db, &project_id, &session_id)["attention"]["kind"],
        json!("needsInput")
    );

    set_stored_prompt(&db, &project_id, &session_id, None);
    let session = presented(&db, &project_id, &session_id);
    assert_eq!(session["attention"]["eventId"], json!(event_id));
    assert_eq!(
        session["attention"]["kind"],
        json!("finished"),
        "the answered question no longer waits, whatever the row said"
    );
    assert_eq!(notification_id(&session), Some(row_id.as_str()));
}

#[test]
fn attention_row_link_appears_after_insert() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        None,
        "no row before the producer writes one"
    );

    let row_id = write_row(&db, &project_id, &session_id, Some(&event_id));
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(row_id.as_str())
    );
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(row_id.as_str()),
        "publishing the same attention again finds the same row"
    );
    mark_notification_read(&db, &row_id).expect("mark read");
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(row_id.as_str()),
        "a read row still belongs to the attention"
    );
}

#[test]
fn snapshot_and_delta_link_the_same_attention_row() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);

    // Another session's or another project's row with the same event id is not this one. Rows of
    // the same session id are marked read, so the next write of that session id keeps them.
    write_row(&db, &project_id, "G-other", Some(&event_id));
    let other_project = write_row(&db, "P-other", &session_id, Some(&event_id));
    mark_notification_read(&db, &other_project).expect("mark read");
    // A row of this session for another event is not this attention's.
    let other_event = write_row(&db, &project_id, &session_id, Some("attn_other"));
    mark_notification_read(&db, &other_event).expect("mark read");
    assert!(notification_exists(&db, &other_project).expect("exists"));
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        None
    );

    let first = write_row(&db, &project_id, &session_id, Some(&event_id));
    mark_notification_read(&db, &first).expect("mark read");
    let second = write_row(&db, &project_id, &session_id, Some(&event_id));
    set_row_created_at(&db, &first, "2026-06-07T06:48:00.000Z");
    set_row_created_at(&db, &second, "2026-06-07T06:48:00.000Z");
    let tie_winner = first.clone().max(second.clone());
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(tie_winner.as_str()),
        "rows of one event tie-break by id, newest first"
    );
    set_row_created_at(&db, &first, "2026-06-07T06:49:00.000Z");
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(first.as_str()),
        "the newest row of the event wins"
    );
}

#[test]
fn attention_row_link_disappears_after_removal() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);

    let row_id = write_row(&db, &project_id, &session_id, Some(&event_id));
    dismiss_notification(&db, &row_id).expect("dismiss");
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        None
    );

    write_row(&db, &project_id, &session_id, Some(&event_id));
    clear_notification_feed(&db).expect("clear");
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        None
    );

    let row_id = write_row(&db, &project_id, &session_id, Some(&event_id));
    set_row_created_at(&db, &row_id, "2026-06-07T06:48:00.000Z");
    for index in 0..40 {
        write_row(
            &db,
            &project_id,
            &format!("G-newer-{index}"),
            Some("attn_newer"),
        );
    }
    assert!(!notification_exists(&db, &row_id).expect("exists"));
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        None,
        "a pruned row is gone"
    );

    // The next attention never takes the earlier row: it has its own event.
    let row_id = write_row(&db, &project_id, &session_id, Some(&event_id));
    mark_notification_read(&db, &row_id).expect("mark read");
    acknowledge(&db, &project_id, &session_id, T0 + 40_000);
    work(&db, &project_id, &session_id, T0 + 60_000);
    let next_event = stop(&db, &project_id, &session_id, T0 + 90_000);
    assert_ne!(next_event, event_id);
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        None
    );
    let next_row = write_row(&db, &project_id, &session_id, Some(&next_event));
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(next_row.as_str())
    );
}

/// A card answered after the delta projected its attention but before the row lookup: the
/// projected card attention must not take the stored event's row. Runs the steps of
/// `build_presentation_session_delta` with the card released between them.
#[test]
fn card_released_after_projection_borrows_no_row() {
    let (_temp, db) = open_test_database();
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);
    let row_id = write_row(&db, &project_id, &session_id, Some(&event_id));
    let (card_id, card_prompt) =
        crate::connectors::hold_waiting_connector_card(&project_id, &session_id);
    set_stored_prompt(&db, &project_id, &session_id, Some(card_prompt));

    let repository = DomainRepository::new(&db, SERVER);
    let project = repository
        .get_project_without_session_history(&project_id)
        .expect("get project")
        .expect("project row");
    let raw = repository
        .get_session(&project_id, &session_id)
        .expect("get session")
        .expect("session row");
    let mut projected =
        project_presentation_session(&project, &default_group_id(&project_id), &raw, &now_iso());
    crate::connectors::release_waiting_connector_card(&card_id);
    insert_attention_notification_id(
        &mut projected,
        &db,
        stored_attention_event_id(&raw).as_deref(),
    )
    .expect("enrich");
    assert_eq!(
        projected["attention"]["eventId"],
        json!(format!("connectorcard:{card_id}"))
    );
    assert_eq!(
        notification_id(&projected),
        None,
        "the card attention keeps no row of the stored event"
    );

    // The next publication sees no waiting card: the stored event is the attention again.
    let session = presented(&db, &project_id, &session_id);
    assert_eq!(session["attention"]["eventId"], json!(event_id));
    assert_eq!(notification_id(&session), Some(row_id.as_str()));
}

#[test]
fn attention_link_never_uses_legacy_or_connector_event_id() {
    let (_temp, db) = open_test_database();

    // A row stored before attention event ids existed: its published eventId is lastChangedAt.
    let entered_at = iso(T0 + 30_000);
    let (project_id, session_id) = create_session(
        &db,
        "running",
        json!({
            "agentActivity": {
                "activity": "attention",
                "agentName": "codex",
                "isAcknowledged": false,
                "lastChangedAt": entered_at,
            },
        }),
    );
    // Read rows, so the next write of this session does not replace them.
    let legacy_row = write_row(&db, &project_id, &session_id, Some(&entered_at));
    mark_notification_read(&db, &legacy_row).expect("mark read");
    write_row(&db, &project_id, &session_id, None);
    assert!(notification_exists(&db, &legacy_row).expect("exists"));
    let legacy = presented(&db, &project_id, &session_id);
    assert_eq!(legacy["attention"]["eventId"], json!(entered_at));
    assert_eq!(notification_id(&legacy), None);

    // A waiting connector card owns the attention; the stored event's row is not its row.
    let (project_id, session_id) = create_session(&db, "running", json!({}));
    work(&db, &project_id, &session_id, T0);
    let event_id = stop(&db, &project_id, &session_id, T0 + 30_000);
    let row_id = write_row(&db, &project_id, &session_id, Some(&event_id));
    assert_eq!(
        notification_id(&presented(&db, &project_id, &session_id)),
        Some(row_id.as_str())
    );
    let (card_id, card_prompt) =
        crate::connectors::hold_waiting_connector_card(&project_id, &session_id);
    set_stored_prompt(&db, &project_id, &session_id, Some(card_prompt.clone()));
    let card_event_id = format!("connectorcard:{card_id}");
    mark_notification_read(&db, &row_id).expect("mark read");
    write_row(&db, &project_id, &session_id, Some(&card_event_id));
    assert!(notification_exists(&db, &row_id).expect("exists"));
    let card = presented(&db, &project_id, &session_id);
    assert_eq!(card["attention"]["eventId"], json!(card_event_id));
    assert_eq!(notification_id(&card), None);

    // After an acknowledgement the session keeps its last attention id while a card puts it in
    // attention again: still no row of that earlier attention.
    acknowledge(&db, &project_id, &session_id, T0 + 40_000);
    set_stored_prompt(&db, &project_id, &session_id, Some(card_prompt));
    let acknowledged_card = presented(&db, &project_id, &session_id);
    assert_eq!(acknowledged_card["activity"], json!("attention"));
    assert_eq!(
        acknowledged_card["attention"]["eventId"],
        json!(card_event_id)
    );
    assert_eq!(notification_id(&acknowledged_card), None);

    crate::connectors::release_waiting_connector_card(&card_id);
    assert!(
        presented(&db, &project_id, &session_id)
            .get("attention")
            .is_none(),
        "with the card gone the acknowledged session is idle"
    );
}

/// The existing question count, approval flag and Auto Sleep gate read the same values with the
/// attention additions in place.
#[test]
fn presentation_extensions_do_not_change_question_or_sleep_rules() {
    let (_temp, db) = open_test_database();
    let settings = crate::session_auto_sleep::AutoSleepSettings {
        idle_minutes: 1,
        favorite_sessions: true,
        require_resume_reference: false,
    };
    let far_future_ms = T0 + 365 * 24 * 60 * 60 * 1_000;
    let presented_after =
        |runtime_settings: Value, prompt: Option<String>, steps: &dyn Fn(&str, &str)| -> Value {
            let (project_id, session_id) = create_session(&db, "running", runtime_settings);
            work(&db, &project_id, &session_id, T0);
            if prompt.is_some() {
                set_stored_prompt(&db, &project_id, &session_id, prompt);
            }
            steps(&project_id, &session_id);
            presented(&db, &project_id, &session_id)
        };
    let working = |_: &str, _: &str| {};
    let stopped = |project_id: &str, session_id: &str| {
        stop(&db, project_id, session_id, T0 + 30_000);
    };
    let acknowledged = |project_id: &str, session_id: &str| {
        stop(&db, project_id, session_id, T0 + 30_000);
        acknowledge(&db, project_id, session_id, T0 + 40_000);
    };

    let card_only = presented_after(json!({}), Some(question_prompt()), &stopped);
    assert_eq!(card_only["pendingQuestionCount"], json!(0));
    assert!(card_only.get("awaitingApproval").is_none());

    let async_question = presented_after(
        json!({ "sessionChatAsyncQuestionIds": ["q1"] }),
        None,
        &working,
    );
    assert_eq!(async_question["activity"], json!("working"));
    assert_eq!(async_question["pendingQuestionCount"], json!(1));

    let open_form = presented_after(
        json!({ "sessionChatOpenQuestions": { "count": 2 } }),
        None,
        &stopped,
    );
    assert_eq!(open_form["pendingQuestionCount"], json!(2));

    let approval = presented_after(json!({}), Some(approval_prompt()), &stopped);
    assert_eq!(approval["awaitingApproval"], json!(true));
    assert_eq!(approval["pendingQuestionCount"], json!(0));

    let idle = presented_after(json!({}), None, &acknowledged);
    assert_eq!(idle["activity"], json!("idle"));
    assert_eq!(idle["pendingQuestionCount"], json!(0));
    assert!(
        crate::session_auto_sleep::should_sleep(&idle, settings, far_future_ms, None),
        "an idle acknowledged session with nothing waiting may sleep"
    );

    let idle_with_question = presented_after(
        json!({ "sessionChatAsyncQuestionIds": ["q1"] }),
        None,
        &acknowledged,
    );
    assert_eq!(idle_with_question["pendingQuestionCount"], json!(1));
    assert!(
        !crate::session_auto_sleep::should_sleep(
            &idle_with_question,
            settings,
            far_future_ms,
            None
        ),
        "an unanswered question still keeps the session awake"
    );
}
