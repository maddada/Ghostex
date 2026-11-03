//! Voice mode with a connected computer, end to end in one process: this computer's gxserver is
//! served on its own loopback port (which the voice tools call), and a stand-in for the other
//! computer records every request that reaches it. Everything goes through the real relay, so the
//! Agent access switch and the peer allowlist decide as they do for agents.

use super::*;

type Seen = Arc<Mutex<Vec<(String, String, Value)>>>;

const PEER_TOKEN: &str = "gxpeer_voice_test_token";

/// Answers what the other computer's gxserver would, for the routes the voice tools use, and
/// records the path, the authorization and the params of every request.
async fn fake_other_computer() -> (String, Seen) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    let app = axum::Router::new().fallback_service(tower::service_fn(
        move |request: Request<Body>| {
            let recorder = recorder.clone();
            async move {
                let path = request.uri().path().to_string();
                let authorization = request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let body = to_bytes(request.into_body(), 1 << 20)
                    .await
                    .unwrap_or_default();
                let params = serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|body| body.get("params").cloned())
                    .unwrap_or(Value::Null);
                recorder
                    .lock()
                    .unwrap()
                    .push((path.clone(), authorization, params));
                let result = match path.as_str() {
                    "/api/listSessions" => json!({"sessions": [
                        {"projectId": "P1", "sessionId": "G1", "title": "Linux build fix", "agentId": "claude", "lifecycleState": "running", "lastActiveAt": "2026-11-03T10:00:00.000Z"},
                        {"projectId": "P1", "sessionId": "G9", "title": "Terminal box", "lifecycleState": "running", "lastActiveAt": "2026-11-03T09:00:00.000Z"},
                        {"projectId": "P1", "sessionId": "G0", "title": "Long gone", "agentId": "claude", "lifecycleState": "stopped"},
                    ]}),
                    "/api/listProjects" => json!({"projects": [
                        {"projectId": "P1", "name": "Ghostex", "path": "/home/me/Ghostex"},
                    ]}),
                    "/api/readSessionChat" => json!({"working": false, "messages": [
                        {"id": "m1", "role": "user", "blocks": [{"type": "text", "text": "Build it on Linux."}]},
                        {"id": "m2", "role": "assistant", "blocks": [{"type": "text", "text": "Built it, all green."}]},
                    ]}),
                    "/api/readSidebarHud" => json!({"agents": [
                        {"agentId": "codex", "name": "Codex"},
                        {"agentId": "claude", "name": "Claude"},
                    ]}),
                    "/api/createAgentSession" => {
                        json!({"session": {"projectId": "P1", "sessionId": "G2"}})
                    }
                    _ => json!({}),
                };
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"ok": true, "result": result}).to_string()))
                    .expect("response");
                Ok::<_, std::convert::Infallible>(response)
            }
        },
    ));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://127.0.0.1:{}", address.port()), seen)
}

/// This computer's gxserver, served on the port its own state names, so the voice tools' loopback
/// calls (and the relay behind them) reach it.
async fn this_computer() -> (tempfile::TempDir, Arc<AppState>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let state = test_app_state_on(paths, "S6l", Some(port));
    let served = state.clone();
    let app = axum::Router::new().fallback_service(tower::service_fn(move |request| {
        handle_http_request(served.clone(), request)
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (temp, state)
}

async fn register_other_computer(state: &Arc<AppState>, base_url: &str, agent_access: bool) {
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/registerConnectedComputer",
            &state.auth_token.clone(),
            json!({ "params": {
                "serverId": "S9r",
                "name": "Omarchy",
                "baseUrl": base_url,
                "token": PEER_TOKEN,
                "agentAccess": agent_access,
                "owner": true,
            }}),
        ),
        "register".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);
}

fn paths_seen(seen: &Seen) -> Vec<String> {
    seen.lock()
        .unwrap()
        .iter()
        .map(|(path, _, _)| path.clone())
        .collect()
}

/// The params of the last request to `path` the other computer received.
fn last_params(seen: &Seen, path: &str) -> Value {
    seen.lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(seen_path, _, _)| seen_path == path)
        .map(|(_, _, params)| params.clone())
        .unwrap_or_else(|| panic!("{path} never reached the other computer"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn voice_mode_lists_and_works_on_a_connected_computers_sessions() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let (remote_url, seen) = fake_other_computer().await;
    let (_temp, state) = this_computer().await;
    register_other_computer(&state, &remote_url, true).await;
    let tool = |name: &'static str, args: Value| {
        let state = state.clone();
        async move { crate::voice_agent::run_tool_for_tests(&state, name, args).await }
    };

    // The list holds the other computer's open sessions, named by their computer.
    let listed = tool("list_sessions", json!({})).await;
    assert_eq!(listed["ok"], true, "{listed}");
    let rows = listed["sessions"].as_array().expect("sessions");
    let refs: Vec<&str> = rows.iter().filter_map(|row| row["ref"].as_str()).collect();
    assert_eq!(refs, ["S9r:P1:G1", "S9r:P1:G9"], "{listed}");
    assert!(rows.iter().all(|row| row["computer"] == "Omarchy"));
    assert_eq!(rows[0]["project"], "Ghostex");
    assert!(listed.get("unavailableComputers").is_none());
    // Every request went through the relay with the token the other computer gave this one.
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .all(|(_, authorization, _)| authorization == &format!("Bearer {PEER_TOKEN}")));
    let projects = tool("list_projects", json!({})).await;
    assert_eq!(projects["projects"][0]["computer"], "Omarchy", "{projects}");

    // Reading and sending go to that computer, with its own ids.
    let read = tool("read_session", json!({"session": "the linux build fix"})).await;
    assert_eq!(read["ok"], true, "{read}");
    assert_eq!(read["messages"][1]["text"], "Built it, all green.");
    assert_eq!(read["session"]["computer"], "Omarchy");
    assert_eq!(
        last_params(&seen, "/api/readSessionChat")["sessionId"],
        "G1"
    );
    let sent = tool(
        "send_prompt",
        json!({"session": "S9r:P1:G1", "text": "Run the tests too."}),
    )
    .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let params = last_params(&seen, "/api/sendSessionChatMessage");
    assert_eq!(
        (&params["projectId"], &params["sessionId"], &params["text"]),
        (&json!("P1"), &json!("G1"), &json!("Run the tests too."))
    );

    // Stopping a turn works there too.
    let stopped = tool("stop_session", json!({"session": "Linux build fix"})).await;
    assert_eq!(stopped["stopped"], true, "{stopped}");
    assert_eq!(
        last_params(&seen, "/api/interruptSessionChat")["sessionId"],
        "G1"
    );

    // A terminal-only session gets a draft typed into its input box; a chat's synced draft lives
    // in that computer's gxserver, so the tool says so and sends nothing.
    let typed = tool(
        "draft_prompt",
        json!({"session": "Terminal box", "text": "ls"}),
    )
    .await;
    assert_eq!(typed["drafted"], true, "{typed}");
    assert_eq!(
        last_params(&seen, "/api/sendSessionText")["sessionId"],
        "G9"
    );
    let before = paths_seen(&seen).len();
    let drafted = tool(
        "draft_prompt",
        json!({"session": "Linux build fix", "text": "Later"}),
    )
    .await;
    assert_eq!(drafted["ok"], false);
    assert!(
        drafted["error"]
            .as_str()
            .unwrap()
            .contains("Drafting into a chat on Omarchy"),
        "{drafted}"
    );
    let answered = tool(
        "answer_prompt",
        json!({"session": "Linux build fix", "answers": [{"question": "Q12", "options": [1]}]}),
    )
    .await;
    assert_eq!(answered["ok"], false);
    assert!(
        answered["error"]
            .as_str()
            .unwrap()
            .contains("Question forms and pages in a chat on Omarchy"),
        "{answered}"
    );
    let shown = tool("show_session", json!({"session": "Linux build fix"})).await;
    assert_eq!(shown["shown"], true, "{shown}");
    assert_eq!(shown["session"]["ref"], "S9r:P1:G1");
    // The two refusals and the show sent nothing but the session list reads.
    assert!(paths_seen(&seen)[before..]
        .iter()
        .all(|path| path == "/api/listSessions" || path == "/api/listProjects"));

    // A new agent in that computer's project starts there, with its first prompt.
    let started = tool(
        "start_agent",
        json!({"project": "Ghostex", "agent": "Claude", "prompt": "Check the CI."}),
    )
    .await;
    assert_eq!(started["ok"], true, "{started}");
    assert_eq!(started["ref"], "S9r:P1:G2");
    assert_eq!(started["computer"], "Omarchy");
    assert_eq!(
        last_params(&seen, "/api/createAgentSession")["agentId"],
        "claude"
    );
    assert_eq!(
        last_params(&seen, "/api/startSessionProvider")["sessionId"],
        "G2"
    );
    assert_eq!(
        last_params(&seen, "/api/sendSessionChatMessage")["text"],
        "Check the CI."
    );

    // With Agent access off, its sessions are gone from the list with the reason, and naming one
    // says why; nothing reaches the other computer.
    register_other_computer(&state, &remote_url, false).await;
    let before = paths_seen(&seen).len();
    let listed = tool("list_sessions", json!({})).await;
    assert_eq!(listed["total"], 0, "{listed}");
    let reason = listed["unavailableComputers"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert_eq!(listed["unavailableComputers"][0]["computer"], "Omarchy");
    assert!(
        reason.contains("Agent access to Omarchy is off"),
        "{listed}"
    );
    let refused = tool(
        "send_prompt",
        json!({"session": "Linux build fix", "text": "Anything"}),
    )
    .await;
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"].as_str().unwrap().contains(&reason),
        "{refused}"
    );
    assert_eq!(paths_seen(&seen).len(), before, "nothing was sent");

    crate::connected_computers::tests::reset();
}

/// A real second gxserver as the other computer: its own `/api/listSessions` rows read through
/// its peer gate with the token it issued this computer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn voice_mode_reads_a_real_second_gxservers_sessions_through_its_peer_gate() {
    let _serial = crate::connected_computers::tests::serial();
    crate::connected_computers::tests::reset();
    let remote_temp = tempfile::tempdir().expect("tempdir");
    let remote_paths = get_gxserver_paths(Some(remote_temp.path().to_path_buf()));
    std::fs::create_dir_all(remote_paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let remote = test_app_state_for(remote_paths, "S5r");
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

    // A project and a session on the other computer.
    let folder = remote_temp.path().join("linux-app");
    std::fs::create_dir_all(&folder).expect("project folder");
    let project = add_project_path_for_server_test(
        remote.clone(),
        &remote.auth_token.clone(),
        &folder,
        Some("Linux app"),
    )
    .await;
    let project_id = project["projectId"]
        .as_str()
        .expect("project id")
        .to_string();
    let created = route_http(
        remote.clone(),
        rpc_request(
            "/api/createSession",
            &remote.auth_token.clone(),
            json!({ "params": {
                "cwd": path_to_string(&folder),
                "kind": "terminal",
                "projectId": project_id,
                "title": "Kernel bisect",
            }}),
        ),
        "create".to_string(),
    )
    .await;
    assert_eq!(created.response.status(), StatusCode::OK);
    let session_id = response_json(created.response).await["result"]["session"]["sessionId"]
        .as_str()
        .expect("session id")
        .to_string();
    // Its terminal is up (a new row is "unknown" until its provider starts, which needs zmx).
    crate::storage::open_gxserver_database(&remote.paths)
        .expect("database")
        .execute(
            "UPDATE sessions SET lifecycleState = 'running' WHERE sessionId = ?1",
            [&session_id],
        )
        .expect("mark running");

    // The other computer issues this one a token, as the desktop asks it to.
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

    let (_temp, state) = this_computer().await;
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/registerConnectedComputer",
            &state.auth_token.clone(),
            json!({ "params": {"serverId": "S5r", "name": "Omarchy", "baseUrl": remote_url, "token": token, "owner": true} }),
        ),
        "register".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);

    let listed = crate::voice_agent::run_tool_for_tests(&state, "list_sessions", json!({})).await;
    assert_eq!(listed["ok"], true, "{listed}");
    let row = &listed["sessions"][0];
    assert_eq!(
        row["ref"],
        format!("S5r:{project_id}:{session_id}"),
        "{listed}"
    );
    assert_eq!(row["title"], "Kernel bisect");
    assert_eq!(row["project"], "Linux app");
    assert_eq!(row["computer"], "Omarchy");
    assert!(listed.get("unavailableComputers").is_none(), "{listed}");

    // A chat draft is refused before it is sent; the other computer refuses it on its own too.
    let (status, body) = {
        let routed = route_http(
            remote.clone(),
            rpc_request(
                "/api/setSessionChatDraft",
                &token,
                json!({ "params": {"projectId": project_id, "sessionId": session_id, "content": "x"} }),
            ),
            "draft".to_string(),
        )
        .await;
        let status = routed.response.status();
        (status, response_json(routed.response).await)
    };
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    crate::connected_computers::tests::reset();
}
