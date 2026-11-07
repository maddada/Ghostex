//! A coordinator's thread on another computer whose hooks never say its turn ended (issue #475):
//! two real gxservers, the thread's computer read over its peer gate.

use super::*;

use crate::server::coordinator_remote_runtime::{tick_at, RemoteReport, RemoteSupervisorMemory};

const MINUTE: i64 = 60_000;

struct Remote {
    _temp: tempfile::TempDir,
    _local_temp: tempfile::TempDir,
    local: Arc<AppState>,
    transcript: std::path::PathBuf,
}

/// The thread's computer (S5r, "headless") with one Claude session that its hooks left working,
/// its transcript holding `transcript`; and this computer (S6l) with a coordinator linked to it.
async fn remote_thread(transcript: &str) -> Remote {
    let remote_temp = tempfile::tempdir().expect("tempdir");
    let remote_paths = get_gxserver_paths(Some(remote_temp.path().to_path_buf()));
    std::fs::create_dir_all(remote_paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let remote = test_app_state_for(remote_paths.clone(), "S5r");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let remote_url = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("address").port()
    );
    let served = remote.clone();
    let app = axum::Router::new().fallback_service(tower::service_fn(move |request| {
        handle_http_request(served.clone(), request)
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let transcript_path = remote_temp.path().join("thread.jsonl");
    std::fs::write(&transcript_path, transcript).expect("write transcript");
    let db = crate::storage::open_gxserver_database(&remote_paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "S5r");
    let project = repository
        .create_project(
            json!({ "name": "Linux app", "path": remote_temp.path() })
                .as_object()
                .expect("params"),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().expect("id").to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "agentSessionId": "conv-1",
                    "agentSessionPath": transcript_path,
                    // No hook ever said the turn ended.
                    "agentActivity": { "activity": "working" },
                },
                "title": "Bisect the kernel",
            })
            .as_object()
            .expect("params"),
            true,
        )
        .expect("session");
    let session_id = session["sessionId"].as_str().expect("id").to_string();
    db.execute(
        "UPDATE sessions SET lifecycleState = 'running' WHERE sessionId = ?1",
        [&session_id],
    )
    .expect("mark running");
    drop(db);

    let granted = route_http(
        remote.clone(),
        rpc_request(
            "/api/issueComputerGrant",
            &remote.auth_token.clone(),
            json!({ "params": {"serverId": "S6l", "name": "Mac", "agentAccess": true, "rotate": false} }),
        ),
        "grant".to_string(),
    )
    .await;
    let token = response_json(granted.response).await["result"]["peerToken"]
        .as_str()
        .expect("peer token")
        .to_string();

    let local_temp = tempfile::tempdir().expect("tempdir");
    let local_paths = get_gxserver_paths(Some(local_temp.path().to_path_buf()));
    std::fs::create_dir_all(local_paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let local = test_app_state_for(local_paths.clone(), "S6l");
    let routed = route_http(
        local.clone(),
        rpc_request(
            "/api/registerConnectedComputer",
            &local.auth_token.clone(),
            json!({ "params": {"serverId": "S5r", "name": "Omarchy", "baseUrl": remote_url, "token": token, "owner": true} }),
        ),
        "register".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);

    let db = crate::storage::open_gxserver_database(&local_paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "S6l");
    let local_project = repository
        .create_project(
            json!({ "name": "Mac app", "path": local_temp.path() })
                .as_object()
                .expect("params"),
        )
        .expect("project");
    let local_project_id = local_project["projectId"].as_str().expect("id").to_string();
    let coordinator = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": local_project_id,
                "runtimeSettings": { "agentName": "claude", "coordinatorRole": "skill" },
                "title": "Coordinator",
            })
            .as_object()
            .expect("params"),
            true,
        )
        .expect("coordinator");
    let coordinator_id = coordinator["sessionId"].as_str().expect("id").to_string();
    db.execute(
        "UPDATE sessions SET lifecycleState = 'running' WHERE sessionId = ?1",
        [&coordinator_id],
    )
    .expect("mark running");
    crate::coordinators::insert_coordinator(&db, &local_project_id, &coordinator_id, "", "")
        .expect("coordinator");
    crate::coordinators::link_remote_thread(
        &db,
        &("S5r".to_string(), project_id, session_id),
        &(local_project_id, coordinator_id),
        "Bisect the kernel",
        "Bisect the kernel",
    )
    .expect("link");
    Remote {
        _temp: remote_temp,
        _local_temp: local_temp,
        local,
        transcript: transcript_path,
    }
}

async fn pass(
    remote: &Remote,
    memory: &Arc<Mutex<RemoteSupervisorMemory>>,
    now: i64,
) -> Vec<String> {
    let (state, memory) = (remote.local.clone(), memory.clone());
    tokio::task::spawn_blocking(move || {
        let reports = tick_at(&state, &memory, now);
        // Nothing is delivered here: the next pass sees the same thread again.
        memory.lock().unwrap().in_flight.clear();
        reports
            .iter()
            .map(|report| match &report.kind {
                RemoteReport::Finished => "finished".to_string(),
                RemoteReport::Waiting => "waiting".to_string(),
                RemoteReport::Closed => "closed".to_string(),
                RemoteReport::Stalled { due } => format!("stalled:{}", due.round),
                RemoteReport::TurnEnded { due } => format!("turn-ended:{}", due.round),
            })
            .collect()
    })
    .await
    .expect("pass")
}

const PROMPT: &str = "{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2026-11-06T10:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"Bisect the kernel\"}}\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remote_thread_whose_transcript_ended_its_turn_is_reported_finished_without_hooks() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let transcript = format!(
        "{PROMPT}{}",
        "{\"type\":\"assistant\",\"uuid\":\"a1\",\"timestamp\":\"2026-11-06T10:05:00.000Z\",\"message\":{\"id\":\"m1\",\"role\":\"assistant\",\"stop_reason\":\"end_turn\",\"content\":[{\"type\":\"text\",\"text\":\"Found it: commit abc broke the driver.\"}]}}\n"
    );
    let remote = remote_thread(&transcript).await;
    let memory = Arc::new(Mutex::new(RemoteSupervisorMemory::default()));
    let start = 1_000 * MINUTE;
    // Its hooks say working: no report yet, and its clock starts.
    assert!(pass(&remote, &memory, start).await.is_empty());
    assert!(pass(&remote, &memory, start + 7 * MINUTE).await.is_empty());
    // A round without output, and the transcript's turn ended: a finished turn, not a stall.
    assert_eq!(
        pass(&remote, &memory, start + 8 * MINUTE + 1).await,
        vec!["turn-ended:1".to_string()]
    );
    // Once that report went out, its hooks still saying working no longer keeps it working.
    {
        let mut memory = memory.lock().unwrap();
        let watch = memory.stalls.values_mut().next().expect("watch");
        let due = watch.due(start + 8 * MINUTE + 1).expect("due");
        watch.mark_reported(due.quiet_since, due.round);
        watch.mark_finished(due.rows);
    }
    {
        // What a delivered finished report records.
        let db = crate::storage::open_gxserver_database(&remote.local.paths).expect("db");
        let key = crate::coordinators::list_remote_threads(&db).expect("threads")[0].key();
        crate::coordinators::record_remote_thread_report(&db, &key, None, Some("m1"), "Found it")
            .expect("report");
    }
    assert!(pass(&remote, &memory, start + 60 * MINUTE).await.is_empty());
    let db = crate::storage::open_gxserver_database(&remote.local.paths).expect("db");
    let thread = crate::coordinators::list_remote_threads(&db).expect("threads")[0].clone();
    assert_eq!(thread.state(), "finished");
    crate::connected_computers::tests::reset();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remote_thread_quiet_mid_turn_is_reported_not_moving_until_it_writes_again() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let transcript = format!(
        "{PROMPT}{}",
        "{\"type\":\"assistant\",\"uuid\":\"a1\",\"timestamp\":\"2026-11-06T10:01:00.000Z\",\"message\":{\"id\":\"m1\",\"role\":\"assistant\",\"stop_reason\":\"tool_use\",\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Bash\",\"input\":{\"command\":\"make test\"}}]}}\n"
    );
    let remote = remote_thread(&transcript).await;
    let memory = Arc::new(Mutex::new(RemoteSupervisorMemory::default()));
    let start = 1_000 * MINUTE;
    assert!(pass(&remote, &memory, start).await.is_empty());
    assert_eq!(
        pass(&remote, &memory, start + 8 * MINUTE + 1).await,
        vec!["stalled:1".to_string()]
    );
    // Not delivered, so the same round is due again on the next pass.
    assert_eq!(
        pass(&remote, &memory, start + 8 * MINUTE + 31_000).await,
        vec!["stalled:1".to_string()]
    );
    // It writes a row: its clock restarts and nothing is due.
    let mut grown = transcript.clone();
    grown.push_str("{\"type\":\"user\",\"uuid\":\"r1\",\"timestamp\":\"2026-11-06T10:20:00.000Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\",\"content\":\"ok\"}]}}\n");
    std::fs::write(&remote.transcript, grown).expect("grow transcript");
    // (30 seconds after the last read, when the next one is due.)
    assert!(pass(&remote, &memory, start + 9 * MINUTE + 5_000)
        .await
        .is_empty());
    assert!(pass(&remote, &memory, start + 16 * MINUTE).await.is_empty());
    crate::connected_computers::tests::reset();
}
