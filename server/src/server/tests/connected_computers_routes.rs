//! Two computers in one process: one served on a real port, the other reaching it through the
//! relay, and the token the first hands the second.

use super::*;

/// Serves `state` on a loopback port the way gxserver does, and returns its address.
async fn serve(state: Arc<AppState>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    let app = axum::Router::new().fallback_service(tower::service_fn(move |request| {
        handle_http_request(state.clone(), request)
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://127.0.0.1:{}", address.port())
}

fn request(method: Method, path: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(
            crate::constants::GXSERVER_PROTOCOL_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .header(header::CONTENT_TYPE, "application/json");
    builder
        .body(
            body.map(|body| Body::from(body.to_string()))
                .unwrap_or_default(),
        )
        .expect("request")
}

fn temp_state(server_id: &str) -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    (temp, test_app_state_for(paths, server_id))
}

async fn register(state: &Arc<AppState>, params: Value) -> Value {
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/registerConnectedComputer",
            &state.auth_token.clone(),
            json!({ "params": params }),
        ),
        "register".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);
    response_json(routed.response).await
}

async fn post(
    state: &Arc<AppState>,
    path: &str,
    token: &str,
    params: Value,
) -> (StatusCode, Value) {
    let routed = route_http(
        state.clone(),
        rpc_request(path, token, json!({ "params": params })),
        "test".to_string(),
    )
    .await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

/// The token `issuer` gives `holder_server_id`, as the desktop asks for it.
async fn grant(
    issuer: &Arc<AppState>,
    holder_server_id: &str,
    agent_access: bool,
    rotate: bool,
) -> Option<String> {
    let (status, body) = post(
        issuer,
        "/api/issueComputerGrant",
        &issuer.auth_token.clone(),
        json!({ "serverId": holder_server_id, "name": "Peer", "agentAccess": agent_access, "rotate": rotate }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["result"]["peerToken"].as_str().map(str::to_string)
}

/// Every `/api/...` path the endpoint table names.
fn every_endpoint_path() -> Vec<String> {
    let source = include_str!("../../protocol.rs");
    let table = &source[source.find("pub fn endpoint_for").expect("endpoint table")..];
    let table = &table[..table.find("\nfn descriptor(").expect("table end")];
    let mut paths: Vec<String> = table
        .split('"')
        .filter(|part| part.starts_with("/api/") && !part.contains(' '))
        .map(str::to_string)
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_reaches_a_connected_computer_and_its_switch_cuts_both_ways() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let (_remote_dir, remote) = temp_state("S5r");
    let (_local_dir, local) = temp_state("S6l");
    let remote_url = serve(remote.clone()).await;

    // Not connected yet: a plain refusal that names where to connect it.
    let refused = route_http(
        local.clone(),
        request(
            Method::GET,
            "/api/computers/S5r/relay/api/health/server",
            &local.auth_token,
            None,
        ),
        "before".to_string(),
    )
    .await;
    assert_eq!(refused.response.status(), StatusCode::NOT_FOUND);
    let body = response_json(refused.response).await;
    assert_eq!(body["error"], "computerNotConnected");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("Settings > Remote"));

    // The other computer gives this one a token of its own; this one never holds its full token.
    let token_for_local = grant(&remote, "S6l", true, false).await.expect("a token");
    let (status, registered) = post(
        &local,
        "/api/registerConnectedComputer",
        &local.auth_token.clone(),
        json!({ "serverId": "S5r", "name": "Omarchy", "baseUrl": remote_url, "token": token_for_local }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        registered["result"].get("peerToken").is_none(),
        "registering hands out nothing"
    );

    // The relay answers with the other computer's own health and lists its projects.
    let health = route_http(
        local.clone(),
        request(
            Method::GET,
            "/api/computers/S5r/relay/api/health/server",
            &local.auth_token,
            None,
        ),
        "health".to_string(),
    )
    .await;
    assert_eq!(health.response.status(), StatusCode::OK);
    assert_eq!(response_json(health.response).await["serverId"], "S5r");
    let (status, projects) = post(
        &local,
        "/api/computers/S5r/relay/api/listProjects",
        &local.auth_token.clone(),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{projects}");

    // The relay forwards only the allowlist, and never another relay (A to B to C).
    for forwarded in [
        "/api/computers/S7c/relay/api/sendSessionText",
        "/api/control/stop",
        "/api/registerConnectedComputer",
        "/api/enableSshAccess",
        "/api/remotePairingCode",
        "/api/installExtension",
    ] {
        let (status, body) = post(
            &local,
            &format!("/api/computers/S5r/relay{forwarded}"),
            &local.auth_token.clone(),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{forwarded}: {body}");
    }
    let (status, _) = post(
        &remote,
        "/api/listProjects",
        &remote.auth_token.clone(),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the other computer is still running"
    );

    // Only this computer's own token relays.
    let chained = route_http(
        local.clone(),
        request(
            Method::GET,
            "/api/computers/S5r/relay/api/health/server",
            &token_for_local,
            None,
        ),
        "chained".to_string(),
    )
    .await;
    assert_eq!(chained.response.status(), StatusCode::UNAUTHORIZED);

    // On the other computer, its token opens the allowlist and nothing else: every endpoint walked.
    let (status, _) = post(&remote, "/api/listProjects", &token_for_local, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let mut walked = 0;
    for path in every_endpoint_path() {
        if crate::connected_computers::is_peer_allowed(&path) {
            continue;
        }
        let Some(endpoint) = crate::protocol::endpoint_for(&path) else {
            continue;
        };
        if !endpoint.requires_auth || endpoint.transport != crate::protocol::Transport::Http {
            // Answered without any token on the main listener, as before; the way-back listener
            // below refuses them.
            continue;
        }
        let (status, body) = post(&remote, &path, &token_for_local, json!({})).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{path} answered a peer token: {body}"
        );
        assert_eq!(body["error"], "forbidden", "{path}");
        walked += 1;
    }
    assert!(walked > 250, "walked only {walked} endpoints");

    // The listing never shows a token or the address.
    let (_, listed) = post(
        &local,
        "/api/listConnectedComputers",
        &local.auth_token.clone(),
        json!({}),
    )
    .await;
    let listed = listed.to_string();
    assert!(listed.contains("Omarchy"));
    assert!(!listed.contains(&remote.auth_token));
    assert!(!listed.contains(&token_for_local));
    assert!(!listed.contains(&remote_url));

    // Agent access off: the relay refuses plainly, and the other computer drops the token.
    let (status, _) = post(
        &local,
        "/api/registerConnectedComputer",
        &local.auth_token.clone(),
        json!({ "serverId": "S5r", "name": "Omarchy", "agentAccess": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(
        &local,
        "/api/computers/S5r/relay/api/listProjects",
        &local.auth_token.clone(),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "agentAccessOff");
    assert!(body["message"].as_str().unwrap().contains("Omarchy"));
    assert_eq!(grant(&remote, "S6l", false, false).await, None);
    let (status, _) = post(&remote, "/api/listProjects", &token_for_local, json!({})).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a dropped token opens nothing"
    );

    // Disconnected: forgotten.
    let (status, _) = post(
        &local,
        "/api/unregisterConnectedComputer",
        &local.auth_token.clone(),
        json!({ "serverId": "S5r" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &local,
        "/api/computers/S5r/relay/api/listProjects",
        &local.auth_token.clone(),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    crate::connected_computers::tests::reset();
}

/// The way-back listener answers only the allowlist, only to a token this computer gave, and
/// none of the routes the main listener serves before sign-in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_way_back_listener_serves_only_the_allowlist_to_a_granted_token() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let (_dir, state) = temp_state("S6w");
    let token = grant(&state, "S5w", true, false).await.expect("a token");
    let call = |method: Method, path: &str, token: &str| {
        super::peer_http::handle_peer_http_request(
            state.clone(),
            request(method, path, token, Some(json!({ "params": {} }))),
        )
    };
    let ok = call(Method::POST, "/api/listProjects", &token)
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let health = call(Method::GET, "/api/health/server", &token)
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    // This computer's own token is not taken here.
    let own = call(Method::POST, "/api/listProjects", &state.auth_token.clone())
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::UNAUTHORIZED);
    // Routes served before sign-in and every other endpoint are not there at all.
    let mut walked = 0;
    for path in every_endpoint_path().into_iter().chain(
        [
            "/api/health",
            "/api/pairDevice",
            "/visual/x",
            "/open-file/x",
            "/ext/x/y",
            "/api/computers/S5w/relay/api/listProjects",
        ]
        .map(str::to_string),
    ) {
        if crate::connected_computers::is_peer_allowed(&path) {
            continue;
        }
        let refused = call(Method::POST, &path, &token).await.unwrap();
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "{path}");
        let refused = call(Method::GET, &path, &token).await.unwrap();
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "{path}");
        walked += 1;
    }
    assert!(walked > 250, "walked only {walked}");
    // Agent access off: the token is dropped and refused.
    assert_eq!(grant(&state, "S5w", false, false).await, None);
    let dropped = call(Method::POST, "/api/listProjects", &token)
        .await
        .unwrap();
    assert_eq!(dropped.status(), StatusCode::UNAUTHORIZED);
    crate::connected_computers::tests::reset();
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("params object")
}

/// A project and an agent session in `state`'s own database, with its activity as hooks set it.
fn agent_session(state: &Arc<AppState>, activity: Value) -> (String, String) {
    let db = crate::storage::open_gxserver_database(&state.paths).expect("open db");
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project_id = repository
        .create_project(&params(
            json!({ "name": "Shared", "path": std::env::temp_dir() }),
        ))
        .expect("create project")["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    let session = repository
        .create_session(
            &params(json!({
                "projectId": project_id,
                "kind": "agent",
                "agentId": "claude",
                "title": "Build it there",
                "lifecycleState": "running",
                "runtimeSettings": { "agentActivity": activity },
            })),
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

fn set_activity(state: &Arc<AppState>, key: &(String, String), activity: Value) {
    let db = crate::storage::open_gxserver_database(&state.paths).expect("open db");
    db.execute(
        "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.agentActivity', json(?1)) WHERE projectId = ?2 AND sessionId = ?3",
        rusqlite::params![activity.to_string(), key.0, key.1],
    )
    .expect("update activity");
}

/// The supervisor of threads on another computer: a thread there is seen working, then out of
/// work long enough to report a finished turn, then waiting on a question, then closed; and while
/// its computer is not connected it is left alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_thread_on_another_computer_reports_its_turns_to_the_coordinator_here() {
    use super::coordinator_remote_runtime::{tick, RemoteReport, RemoteSupervisorMemory};

    let _serial = crate::connected_computers::tests::serial();
    let (_remote_dir, remote) = temp_state("S5t");
    let (_local_dir, local) = temp_state("S6t");
    let remote_url = serve(remote.clone()).await;
    let thread = agent_session(&remote, json!({ "activity": "working" }));
    let coordinator = agent_session(&local, json!({ "activity": "idle" }));
    {
        let db = crate::storage::open_gxserver_database(&local.paths).expect("open db");
        db.execute(
            "INSERT INTO coordinators (projectId, sessionId, createdAt, updatedAt) VALUES (?1, ?2, 'x', 'x')",
            rusqlite::params![coordinator.0, coordinator.1],
        )
        .expect("coordinator row");
    }
    let linked = route_http(
        local.clone(),
        rpc_request(
            "/api/linkRemoteCoordinatorThread",
            &local.auth_token,
            json!({ "params": {
                "coordinatorProjectId": coordinator.0, "coordinatorSessionId": coordinator.1,
                "serverId": "S5t", "projectId": thread.0, "sessionId": thread.1,
                "task": "Build it", "title": "Build it there",
            }}),
        ),
        "link".to_string(),
    )
    .await;
    assert_eq!(linked.response.status(), StatusCode::OK);
    assert_eq!(
        response_json(linked.response).await["result"]["globalRef"],
        format!("S5t:{}:{}", thread.0, thread.1)
    );
    let key = ("S5t".to_string(), thread.0.clone(), thread.1.clone());
    let remote_grant = grant(&remote, "S6t", true, false).await.expect("a token");
    let memory = Mutex::new(RemoteSupervisorMemory::default());
    let pass = |local: Arc<AppState>, memory: &Mutex<RemoteSupervisorMemory>| {
        let reports = tick(&local, memory);
        let kinds: Vec<_> = reports
            .iter()
            .map(|report| format!("{:?}", report.kind))
            .collect();
        // A report in flight is settled by delivery; the test settles it by hand.
        memory.lock().unwrap().in_flight.clear();
        kinds
    };

    // Not connected: nothing is judged.
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert!(kinds.is_empty());

    register(
        &local,
        json!({ "serverId": "S5t", "name": "Omarchy", "baseUrl": remote_url, "token": remote_grant }),
    )
    .await;
    let read = |local: &Arc<AppState>| {
        let db = crate::storage::open_gxserver_database(&local.paths).expect("open db");
        crate::coordinators::read_remote_thread(&db, &key)
            .unwrap()
            .unwrap()
    };

    // Working: seen, no report.
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert!(kinds.is_empty(), "{kinds:?}");
    let seen = read(&local);
    assert!(seen.observed_working);
    assert_eq!(seen.last_state.as_deref(), Some("working"));

    // Out of work: a finished turn only after the stability window.
    set_activity(
        &remote,
        &thread,
        json!({ "activity": "attention", "attentionSource": "turnComplete" }),
    );
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert!(kinds.is_empty(), "{kinds:?}");
    memory
        .lock()
        .unwrap()
        .not_working_since
        .insert(key.clone(), chrono::Utc::now().timestamp_millis() - 5_000);
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert_eq!(kinds, vec![format!("{:?}", RemoteReport::Finished)]);

    // Its computer turned Agent access off: left alone, as when it is away.
    register(
        &local,
        json!({ "serverId": "S5t", "name": "Omarchy", "baseUrl": remote_url, "token": remote_grant, "agentAccess": false }),
    )
    .await;
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert!(kinds.is_empty(), "{kinds:?}");
    assert_eq!(read(&local).last_state.as_deref(), Some("unreachable"));
    register(
        &local,
        json!({ "serverId": "S5t", "name": "Omarchy", "baseUrl": remote_url, "token": remote_grant }),
    )
    .await;

    // A question.
    {
        let db = crate::storage::open_gxserver_database(&remote.paths).expect("open db");
        db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatAsyncQuestionIds', json('[\"q1\"]')) WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![thread.0, thread.1],
        )
        .expect("question");
    }
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert_eq!(kinds, vec![format!("{:?}", RemoteReport::Waiting)]);

    // Closed on its computer.
    {
        let db = crate::storage::open_gxserver_database(&remote.paths).expect("open db");
        db.execute(
            "DELETE FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![thread.0, thread.1],
        )
        .expect("close");
    }
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert_eq!(kinds, vec![format!("{:?}", RemoteReport::Closed)]);

    // The coordinator's presentation names the thread for the sidebar.
    let snapshot = route_http(
        local.clone(),
        rpc_request(
            "/api/readPresentationSnapshot",
            &local.auth_token,
            json!({ "params": {} }),
        ),
        "snapshot".to_string(),
    )
    .await;
    let snapshot = response_json(snapshot.response).await;
    let coordinator_row = snapshot["result"]["snapshot"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["sessionId"] == coordinator.1.as_str())
        .cloned()
        .expect("the coordinator row");
    assert_eq!(
        coordinator_row["coordinatorRemoteThreads"][0]["serverId"],
        "S5t"
    );
    // The phone's summary carries it through both whitelists, so its Unified list nests the thread
    // too (CDXC:Sidebar 2026-11-09 in gx-core unified_sidebar/tree.rs).
    let summary = crate::ghostex_cli::sessions::mobile_summary_for_session(
        &json!({ "projectId": coordinator.0, "sessionId": coordinator.1 }),
        &json!({ "projectId": coordinator.0 }),
        &coordinator_row,
    );
    assert_eq!(summary["coordinatorRemoteThreads"][0]["serverId"], "S5t");
    assert_eq!(
        summary["coordinatorRemoteThreads"][0]["sessionId"],
        thread.1.as_str()
    );
    crate::connected_computers::tests::reset();
}

/// A turn that started and ended between two passes (never seen working) is still noticed: the
/// row's activity time moved past the last report. Once its chat was read at that time it is not
/// read again, and a session left out of the snapshot but still running is not closed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_turn_finished_between_two_passes_is_noticed_from_the_rows_activity_time() {
    use super::coordinator_remote_runtime::{tick, RemoteReport, RemoteSupervisorMemory};

    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let (_remote_dir, remote) = temp_state("S5m");
    let (_local_dir, local) = temp_state("S6m");
    let remote_url = serve(remote.clone()).await;
    let thread = agent_session(&remote, json!({ "activity": "idle" }));
    let coordinator = agent_session(&local, json!({ "activity": "idle" }));
    let db = crate::storage::open_gxserver_database(&local.paths).expect("open db");
    db.execute(
        "INSERT INTO coordinators (projectId, sessionId, createdAt, updatedAt) VALUES (?1, ?2, 'x', 'x')",
        rusqlite::params![coordinator.0, coordinator.1],
    )
    .expect("coordinator row");
    let key = ("S5m".to_string(), thread.0.clone(), thread.1.clone());
    crate::coordinators::link_remote_thread(&db, &key, &coordinator, "", "Quick one").unwrap();
    // Its last check covered the other computer's 09:00; the turn since then (10:00 on that
    // computer's clock) was never seen working. This computer's clock runs ahead: its last report
    // says 11:00, which a comparison with this clock would have taken as newer than the turn.
    let checked = chrono::DateTime::parse_from_rfc3339("2026-11-03T09:00:00.000Z")
        .unwrap()
        .timestamp_millis();
    db.execute(
        "UPDATE coordinator_remote_threads SET reportedAt = '2026-11-03T11:00:00.000Z', observedWorking = 0, checkedActivityMs = ?1",
        rusqlite::params![checked],
    )
    .unwrap();
    let remote_db = crate::storage::open_gxserver_database(&remote.paths).expect("open db");
    remote_db
        .execute(
            "UPDATE sessions SET lastActiveAt = '2026-11-03T10:00:00.000Z' WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![thread.0, thread.1],
        )
        .unwrap();
    let remote_grant = grant(&remote, "S6m", true, false).await.expect("a token");
    register(
        &local,
        json!({ "serverId": "S5m", "name": "Omarchy", "baseUrl": remote_url, "token": remote_grant }),
    )
    .await;
    let memory = Mutex::new(RemoteSupervisorMemory::default());
    let pass = |memory: &Mutex<RemoteSupervisorMemory>| {
        let local = local.clone();
        tokio::task::block_in_place(|| {
            let kinds: Vec<_> = tick(&local, memory)
                .iter()
                .map(|report| format!("{:?}", report.kind))
                .collect();
            memory.lock().unwrap().in_flight.clear();
            kinds
        })
    };
    assert!(
        pass(&memory).is_empty(),
        "still inside the stability window"
    );
    memory
        .lock()
        .unwrap()
        .not_working_since
        .insert(key.clone(), chrono::Utc::now().timestamp_millis() - 5_000);
    assert_eq!(pass(&memory), vec![format!("{:?}", RemoteReport::Finished)]);
    // Delivery covers that activity time, reported or not; the next passes leave it alone.
    let marker = chrono::DateTime::parse_from_rfc3339("2026-11-03T10:00:00.000Z")
        .unwrap()
        .timestamp_millis();
    crate::coordinators::record_remote_thread_checked(&db, &key, marker).unwrap();
    memory
        .lock()
        .unwrap()
        .not_working_since
        .insert(key.clone(), chrono::Utc::now().timestamp_millis() - 5_000);
    assert!(pass(&memory).is_empty());
    // A thread reported before it had a time of the other computer's gets one from that
    // computer's clock on the next pass, and is not reported for it.
    db.execute(
        "UPDATE coordinator_remote_threads SET checkedActivityMs = NULL",
        [],
    )
    .unwrap();
    memory
        .lock()
        .unwrap()
        .not_working_since
        .insert(key.clone(), chrono::Utc::now().timestamp_millis() - 5_000);
    assert!(pass(&memory).is_empty());
    let baseline = crate::coordinators::read_remote_thread(&db, &key)
        .unwrap()
        .unwrap()
        .checked_activity_ms;
    assert_eq!(baseline, Some(marker));
    crate::connected_computers::tests::reset();
}

/// Through the endpoints the desktop apps call: this computer's app turns a computer off; the
/// other computer's app keeps renewing ("on") and gets nothing until this app turns it on again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_other_computers_renewals_cannot_undo_this_computers_off_switch() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let (_dir, state) = temp_state("S6d");
    let token = state.auth_token.clone();
    let other = |rotate: bool| json!({ "serverId": "S5d", "name": "Omarchy", "rotate": rotate });
    let (_, issued) = post(&state, "/api/issueComputerGrant", &token, other(false)).await;
    let first = issued["result"]["peerToken"]
        .as_str()
        .expect("a token")
        .to_string();
    // This computer's app: off.
    let (status, _) = post(
        &state,
        "/api/revokeComputerGrant",
        &token,
        json!({ "serverId": "S5d", "deny": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, registered) = post(
        &state,
        "/api/registerConnectedComputer",
        &token,
        json!({ "serverId": "S5d", "name": "Omarchy", "agentAccess": false, "owner": true }),
    )
    .await;
    assert_eq!(registered["result"]["agentAccess"], false);
    // The other computer's app, a minute later: asks for a token and registers itself on.
    for rotate in [false, true] {
        let (_, issued) = post(&state, "/api/issueComputerGrant", &token, other(rotate)).await;
        assert_eq!(issued["result"]["denied"], true);
        assert!(issued["result"]["peerToken"].is_null());
    }
    let (_, registered) = post(
        &state,
        "/api/registerConnectedComputer",
        &token,
        json!({ "serverId": "S5d", "name": "Omarchy", "direction": "inbound",
                "baseUrl": "http://127.0.0.1:58800", "token": "gxpeer_from_there", "agentAccess": true }),
    )
    .await;
    assert_eq!(registered["result"]["agentAccess"], false, "it stays off");
    let (status, _) = post(&state, "/api/listProjects", &first, json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // This computer's app: on again. The other computer is served again.
    let (_, issued) = post(
        &state,
        "/api/issueComputerGrant",
        &token,
        json!({ "serverId": "S5d", "name": "Omarchy", "owner": true }),
    )
    .await;
    assert!(issued["result"]["peerToken"].is_string());
    let (_, issued) = post(&state, "/api/issueComputerGrant", &token, other(false)).await;
    assert_eq!(issued["result"]["denied"], false);
    assert!(issued["result"]["peerToken"].is_string());
    crate::connected_computers::tests::reset();
}

/// A snapshot read for named sessions (`onlySessions`, what a remote coordinator asks for its
/// threads) holds exactly the rows the whole snapshot holds for them, and nothing else.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_snapshot_for_named_sessions_holds_their_rows_as_the_whole_snapshot_does() {
    let (_dir, state) = temp_state("S7o");
    let first = agent_session(&state, json!({ "activity": "working" }));
    let second = agent_session(&state, json!({ "activity": "idle" }));
    let third = agent_session(&state, json!({ "activity": "idle" }));
    let read = |params: Value| {
        let state = state.clone();
        async move {
            let response = route_http(
                state.clone(),
                rpc_request(
                    "/api/readPresentationSnapshot",
                    &state.auth_token,
                    json!({ "params": params }),
                ),
                "snapshot".to_string(),
            )
            .await;
            response_json(response.response).await["result"]["snapshot"].clone()
        }
    };
    let whole_snapshot = read(json!({})).await;
    let whole = whole_snapshot["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let named_snapshot = read(json!({ "onlySessions": [
        { "projectId": third.0, "sessionId": third.1 },
        { "projectId": first.0, "sessionId": first.1 },
        { "projectId": "Pnone", "sessionId": "Gnone" },
    ] }))
    .await;
    let named = named_snapshot["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // What does not follow sessions (projects, workspace groups, collections, Spaces) is whole.
    for part in [
        "projects",
        "workspaceGroups",
        "sidebarProjectCollections",
        "sidebarSpaces",
    ] {
        assert_eq!(named_snapshot[part], whole_snapshot[part], "{part}");
    }
    // The rows keep the whole snapshot's order, whatever order they were asked in.
    let order = |rows: &[Value]| {
        rows.iter()
            .filter(|row| row["sessionId"] != second.1.as_str())
            .map(|row| row["sessionId"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&named), order(&whole));
    let row = |rows: &[Value], key: &(String, String)| {
        rows.iter()
            .find(|row| row["projectId"] == key.0.as_str() && row["sessionId"] == key.1.as_str())
            .cloned()
    };
    assert_eq!(named.len(), 2, "{named:?}");
    for key in [&first, &third] {
        let expected = row(&whole, key).expect("in the whole snapshot");
        let found = row(&named, key).expect("in the named snapshot");
        assert_eq!(found, expected);
    }
    assert!(row(&named, &second).is_none());
}

/// The desktop builds its extension context from the sessions its live store holds (typed
/// `PresentationSession` rows) instead of fetching the snapshot again
/// (apps/desktop/src/app/extensions/snapshot.rs). Every field that reads, as the snapshot sends it,
/// survives the typed row unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_fields_the_extension_context_reads_survive_the_typed_store_row() {
    let (_dir, state) = temp_state("S8x");
    agent_session(&state, json!({ "activity": "working" }));
    agent_session(&state, json!({ "activity": "idle" }));
    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/readPresentationSnapshot",
            &state.auth_token,
            json!({ "params": {} }),
        ),
        "snapshot".to_string(),
    )
    .await;
    let sessions = response_json(response.response).await["result"]["snapshot"]["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(sessions.len() >= 2);
    for session in sessions {
        let typed: ghostex_gx_protocol::PresentationSession =
            serde_json::from_value(session.clone()).expect("a typed row");
        let round_trip = serde_json::to_value(&typed).expect("serialize");
        for field in [
            "sessionId",
            "projectId",
            "displayTitle",
            "primaryTitle",
            "alias",
            "terminalTitle",
            "sessionPersistenceProvider",
            "sessionPersistenceName",
            "remoteMachine",
            "remoteMachineName",
            "sessionRoutingId",
            "sessionKind",
            "kind",
            "lifecycleState",
            "status",
            "activityLabel",
            "activity",
            "agentIcon",
            "agentSessionId",
            "detail",
            "lastInteractionAt",
        ] {
            let text = |value: &Value| value.get(field).and_then(Value::as_str).map(str::to_string);
            assert_eq!(text(&round_trip), text(&session), "{field}");
        }
    }
}

/// What the desktop's live store holds for a machine (`PresentationSnapshot`, typed) carries the
/// snapshot's sessions and projects whole, so the app's readers can take them from the store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_snapshot_survives_the_typed_store_whole() {
    let (_dir, state) = temp_state("S9w");
    agent_session(&state, json!({ "activity": "working" }));
    agent_session(&state, json!({ "activity": "idle" }));
    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/readPresentationSnapshot",
            &state.auth_token,
            json!({ "params": {} }),
        ),
        "snapshot".to_string(),
    )
    .await;
    let snapshot = response_json(response.response).await["result"]["snapshot"].clone();
    let typed: ghostex_gx_protocol::PresentationSnapshot =
        serde_json::from_value(snapshot.clone()).expect("a typed snapshot");
    let round_trip = serde_json::to_value(&typed).expect("serialize");
    for list in ["sessions", "projects", "groups"] {
        let rows = |value: &Value| value[list].as_array().cloned().unwrap_or_default();
        let (before, after) = (rows(&snapshot), rows(&round_trip));
        assert_eq!(before.len(), after.len(), "{list}");
        for (before, after) in before.iter().zip(&after) {
            let missing: Vec<&String> = before
                .as_object()
                .unwrap()
                .iter()
                .filter(|(key, value)| !value.is_null() && after.get(key.as_str()) != Some(value))
                .map(|(key, _)| key)
                .collect();
            assert!(missing.is_empty(), "{list}: {missing:?}");
        }
    }
}

/// `ghostex coordinator promote <ref> --computer <name>`: the relay forwards the promote, and the
/// other computer records the coordinator and installs the orchestrator skill in its own agent
/// home, not this one's (CDXC:Coordinators 2026-11-07 in connected_computers/scope.rs).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_on_a_connected_computer_is_made_a_coordinator_there() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let (_remote_dir, remote) = temp_state("S5p");
    let (_local_dir, local) = temp_state("S6p");
    let remote_url = serve(remote.clone()).await;
    let token_for_local = grant(&remote, "S6p", true, false).await.expect("a token");
    let (status, _) = post(
        &local,
        "/api/registerConnectedComputer",
        &local.auth_token.clone(),
        json!({ "serverId": "S5p", "name": "Mac mini", "baseUrl": remote_url, "token": token_for_local }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (project_id, session_id) = agent_session(&remote, json!({ "activity": "idle" }));

    let (status, promoted) = post(
        &local,
        "/api/computers/S5p/relay/api/promoteCoordinator",
        &local.auth_token.clone(),
        json!({ "projectId": project_id, "sessionId": session_id, "goal": "Ship it" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{promoted}");
    let result = &promoted["result"];
    assert_eq!(result["ok"], true, "{promoted}");
    assert!(
        result["globalRef"].as_str().unwrap().starts_with("S5p:"),
        "{promoted}"
    );

    let db = crate::storage::open_gxserver_database(&remote.paths).expect("open db");
    let recorded: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM coordinators WHERE projectId = ?1 AND sessionId = ?2",
            [&project_id, &session_id],
            |row| row.get(0),
        )
        .expect("count coordinators");
    assert_eq!(recorded, 1, "the other computer keeps the coordinator");
    let skill = |state: &Arc<AppState>| {
        state
            .paths
            .agent_config_home_dir()
            .join(crate::agent_skills::CANONICAL_AGENT_SKILLS_DIR)
            .join("ghostex-orchestrator")
            .join("SKILL.md")
    };
    assert!(skill(&remote).is_file(), "installed on the other computer");
    assert!(!skill(&local).is_file(), "not on this one");
}
