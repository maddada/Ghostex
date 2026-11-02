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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_reaches_a_connected_computer_and_its_switch_cuts_both_ways() {
    let _serial = crate::connected_computers::tests::serial();
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

    let registered = register(
        &local,
        json!({
            "serverId": "S5r",
            "name": "Omarchy",
            "baseUrl": remote_url,
            "token": remote.auth_token,
        }),
    )
    .await;
    let peer_token = registered["result"]["peerToken"]
        .as_str()
        .expect("the other computer gets a token")
        .to_string();
    assert_eq!(registered["result"]["serverId"], "S6l");

    // The relay answers with the other computer's own health.
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

    // An RPC goes through too.
    let projects = route_http(
        local.clone(),
        rpc_request(
            "/api/computers/S5r/relay/api/listProjects",
            &local.auth_token,
            json!({ "params": {} }),
        ),
        "projects".to_string(),
    )
    .await;
    assert_eq!(projects.response.status(), StatusCode::OK);
    assert_eq!(response_json(projects.response).await["ok"], true);

    // Only this computer's own token relays; the other computer's token never chains onward.
    let chained = route_http(
        local.clone(),
        request(
            Method::GET,
            "/api/computers/S5r/relay/api/health/server",
            &peer_token,
            None,
        ),
        "chained".to_string(),
    )
    .await;
    assert_eq!(chained.response.status(), StatusCode::UNAUTHORIZED);

    // The other computer's agents may use what a remote client may, and nothing else.
    let peer_projects = route_http(
        local.clone(),
        rpc_request("/api/listProjects", &peer_token, json!({ "params": {} })),
        "peer-projects".to_string(),
    )
    .await;
    assert_eq!(peer_projects.response.status(), StatusCode::OK);
    let peer_admin = route_http(
        local.clone(),
        rpc_request(
            "/api/listConnectedComputers",
            &peer_token,
            json!({ "params": {} }),
        ),
        "peer-admin".to_string(),
    )
    .await;
    assert_eq!(peer_admin.response.status(), StatusCode::FORBIDDEN);

    // The listing never shows a token or the address.
    let listed = route_http(
        local.clone(),
        rpc_request(
            "/api/listConnectedComputers",
            &local.auth_token,
            json!({ "params": {} }),
        ),
        "list".to_string(),
    )
    .await;
    let listed = response_json(listed.response).await.to_string();
    assert!(listed.contains("Omarchy"));
    assert!(!listed.contains(&remote.auth_token));
    assert!(!listed.contains(&peer_token));
    assert!(!listed.contains(&remote_url));

    // Agent access off: the relay and the other computer's token are both refused, plainly.
    let renewed = register(
        &local,
        json!({
            "serverId": "S5r",
            "name": "Omarchy",
            "baseUrl": remote_url,
            "token": remote.auth_token,
            "agentAccess": false,
        }),
    )
    .await;
    assert_eq!(renewed["result"]["peerToken"], peer_token.as_str());
    let off = route_http(
        local.clone(),
        rpc_request(
            "/api/computers/S5r/relay/api/listProjects",
            &local.auth_token,
            json!({ "params": {} }),
        ),
        "off".to_string(),
    )
    .await;
    assert_eq!(off.response.status(), StatusCode::FORBIDDEN);
    let body = response_json(off.response).await;
    assert_eq!(body["error"], "agentAccessOff");
    assert!(body["message"].as_str().unwrap().contains("Omarchy"));
    let peer_off = route_http(
        local.clone(),
        rpc_request("/api/listProjects", &peer_token, json!({ "params": {} })),
        "peer-off".to_string(),
    )
    .await;
    assert_eq!(peer_off.response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(peer_off.response).await["error"],
        "agentAccessOff"
    );

    // Disconnected: the token stops working at all.
    let unregistered = route_http(
        local.clone(),
        rpc_request(
            "/api/unregisterConnectedComputer",
            &local.auth_token,
            json!({ "params": { "serverId": "S5r" } }),
        ),
        "unregister".to_string(),
    )
    .await;
    assert_eq!(unregistered.response.status(), StatusCode::OK);
    let gone = route_http(
        local.clone(),
        rpc_request("/api/listProjects", &peer_token, json!({ "params": {} })),
        "gone".to_string(),
    )
    .await;
    assert_eq!(gone.response.status(), StatusCode::UNAUTHORIZED);
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
        json!({ "serverId": "S5t", "name": "Omarchy", "baseUrl": remote_url, "token": remote.auth_token }),
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
        json!({ "serverId": "S5t", "name": "Omarchy", "baseUrl": remote_url, "token": remote.auth_token, "agentAccess": false }),
    )
    .await;
    let local_for_pass = local.clone();
    let kinds = tokio::task::block_in_place(|| pass(local_for_pass, &memory));
    assert!(kinds.is_empty(), "{kinds:?}");
    assert_eq!(read(&local).last_state.as_deref(), Some("unreachable"));
    register(
        &local,
        json!({ "serverId": "S5t", "name": "Omarchy", "baseUrl": remote_url, "token": remote.auth_token }),
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
    crate::connected_computers::tests::reset();
}
