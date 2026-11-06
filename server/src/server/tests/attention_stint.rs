//! A program that reports its own status (OSC 7501) can finish a turn before the agent's Stop hook
//! for the same turn arrives. Both mark one attention: one Notifications row, and the stored
//! attention, the published attention and the row share one event id, so the published attention
//! links its row (`attention.notificationId`).

use super::*;
use crate::program_status::ProgramStatusRecord;

#[tokio::test]
async fn a_program_done_before_the_stop_hook_is_one_attention_linked_to_its_row() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let db = open_gxserver_database(&state.paths).expect("db");
    let server_id = state.metadata.server_id.clone();
    let repository = DomainRepository::new(&db, server_id.as_str());
    let project = repository
        .create_project(
            json!({ "name": "Stint", "path": temp.path() })
                .as_object()
                .expect("project params"),
        )
        .expect("project");
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "projectId": project_id, "kind": "agent", "agentId": "codex",
                "title": "Finish a turn", "lifecycleState": "running",
                "providerState": { "lifecycleState": "exists" },
                "runtimeSettings": { "agentName": "codex", "agentActivity": { "activity": "working" } },
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session");
    let session_id = session["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    // A watched terminal (its title observer runs), so its OSC 7501 records stay while the
    // session is published. The observer is a stand-in task; no zmx process starts.
    state.zmx_title_observers.lock().expect("observers").insert(
        crate::server::session_observer_key(&project_id, &session_id),
        crate::server::ZmxTitleObserverTask {
            handle: tokio::spawn(std::future::pending::<()>()),
            zmx_name: session["zmxName"].as_str().expect("zmxName").to_string(),
        },
    );
    let report = |records: Vec<ProgramStatusRecord>| {
        let line = json!({ "programStatus": { "reported": true, "records": records } });
        let parsed = crate::program_status::parse_program_status_line(&line.to_string())
            .expect("a program-status watch line");
        crate::program_status::ingest_program_status_line(&state, &project_id, &session_id, parsed);
    };
    let root = |seq: u64, state: &str| ProgramStatusRecord {
        state: state.to_string(),
        app: Some("codex".to_string()),
        seq,
        updated_at_ms: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };

    report(Vec::new());
    report(vec![root(1, "working")]);
    report(vec![root(2, "done")]);
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let stopped = route_http(
        state.clone(),
        rpc_request(
            "/api/ingestAgentHookEvent",
            &state.auth_token,
            json!({ "params": {
                "projectId": project_id, "sessionId": session_id, "agentName": "codex",
                "eventName": "Stop", "rawEventName": "Stop", "status": "idle",
            } }),
        ),
        "request-stop-after-done".to_string(),
    )
    .await;
    assert_eq!(stopped.response.status(), StatusCode::OK);

    let stored = repository
        .get_session(&project_id, &session_id)
        .expect("read session")
        .expect("session row");
    let activity = &stored["runtimeSettings"]["agentActivity"];
    assert_eq!(activity["activity"], json!("attention"));
    let stored_event = activity["attentionEventId"]
        .as_str()
        .expect("stored attention event id");
    let mut rows = db
        .prepare("SELECT id, attentionEventId FROM notification_feed WHERE sessionId = ?1")
        .expect("feed query");
    let rows: Vec<(String, Option<String>)> = rows
        .query_map(rusqlite::params![session_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("feed rows")
        .map(|row| row.expect("feed row"))
        .collect();
    assert_eq!(rows.len(), 1, "one turn, one row: {rows:?}");
    let (row_id, row_event) = &rows[0];
    assert_eq!(
        row_event.as_deref(),
        Some(stored_event),
        "the Stop hook keeps the event id the row was written with"
    );

    let sessions = repository.list_sessions(None).expect("sessions");
    let snapshot = crate::presentation::read_presentation_snapshot(
        &db,
        server_id.as_str(),
        None,
        false,
        false,
        sessions,
    )
    .expect("snapshot");
    let presented = snapshot["sessions"]
        .as_array()
        .expect("presented sessions")
        .iter()
        .find(|presented| presented["sessionId"] == json!(session_id))
        .expect("presented session");
    assert_eq!(presented["attention"]["eventId"], json!(stored_event));
    assert_eq!(presented["attention"]["kind"], json!("finished"));
    assert_eq!(presented["attention"]["notificationId"], json!(row_id));
    crate::program_status::forget_program_status(&project_id, &session_id);
}
