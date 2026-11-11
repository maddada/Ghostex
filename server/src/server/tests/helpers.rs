use super::*;
use crate::{
    config::create_default_gxserver_config,
    constants::GXSERVER_PROTOCOL_HEADER,
    storage::{create_gxserver_migration_status, initialize_gxserver_storage},
};
use std::fs;

pub(crate) fn test_app_state(paths: GxserverPaths) -> Arc<AppState> {
    test_app_state_for(paths, "S7k")
}

/// A test daemon with its own server id, for tests that run two computers in one process.
pub(crate) fn test_app_state_for(paths: GxserverPaths, server_id: &str) -> Arc<AppState> {
    test_app_state_on(paths, server_id, None)
}

/// A test state whose own routes (the loopback the voice agent and automations call) are on
/// `port` when given.
pub(super) fn test_app_state_on(
    paths: GxserverPaths,
    server_id: &str,
    port: Option<u16>,
) -> Arc<AppState> {
    // Test daemons never reach Expo: their sender points at a closed loopback port.
    let push = crate::push::PushRuntime::start_with(
        paths.clone(),
        Arc::new(GxserverLogger::new(paths.clone())),
        Arc::new(crate::push::runtime::SystemClock),
        crate::push::expo::ExpoClient::for_test_origin("http://127.0.0.1:9".to_string()),
    );
    test_app_state_on_with_push(paths, server_id, port, Arc::new(push))
}

/// `test_app_state` with the given push sender (one aimed at a test's fake Expo).
pub(crate) fn test_app_state_with_push(
    paths: GxserverPaths,
    push: Arc<crate::push::PushRuntime>,
) -> Arc<AppState> {
    test_app_state_on_with_push(paths, "S7k", None, push)
}

/// Test daemons never reach the Live Activity relay: their sender points at a closed loopback port.
pub(crate) fn closed_live_activity(
    paths: &GxserverPaths,
    server_id: &str,
) -> Arc<crate::live_activity::LiveActivityRuntime> {
    Arc::new(crate::live_activity::LiveActivityRuntime::start_with(
        paths.clone(),
        server_id.to_string(),
        Arc::new(GxserverLogger::new(paths.clone())),
        Arc::new(crate::push::runtime::SystemClock),
        crate::live_activity::relay::RelayClient::new(
            "http://127.0.0.1:9/live-activity/v1/send".to_string(),
        ),
        crate::live_activity::runtime::Timings::default(),
        Arc::new(Vec::new),
    ))
}

fn test_app_state_on_with_push(
    paths: GxserverPaths,
    server_id: &str,
    port: Option<u16>,
    push: Arc<crate::push::PushRuntime>,
) -> Arc<AppState> {
    let storage = initialize_gxserver_storage(&paths).expect("storage");
    let mut config = create_default_gxserver_config().expect("config");
    if let Some(port) = port {
        config.listeners.local.host = "127.0.0.1".to_string();
        config.listeners.local.port = port;
    }
    let metadata = RuntimeMetadata {
        build_identity: "test-build".to_string(),
        pid: std::process::id(),
        port: config.listeners.local.port,
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        server_id: server_id.to_string(),
        started_at: "2026-05-30T10:00:00.000Z".to_string(),
        version: "0.0.0-test".to_string(),
    };
    let (shutdown_tx, _) = broadcast::channel(8);
    let automation_runtime = AutomationRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        format!(
            "http://{}:{}",
            config.listeners.local.host, config.listeners.local.port
        ),
    );
    let event_hub = GxserverEventHub::new(metadata.server_id.clone());
    let presentation_event_sequence = Arc::new(Mutex::new(()));
    let delayed_send_runtime = DelayedSendRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        event_hub.clone(),
        presentation_event_sequence.clone(),
    );
    let extension_registry = ExtensionRegistry::new_with_api_url(
        &paths,
        format!(
            "http://{}:{}",
            config.listeners.local.host, config.listeners.local.port
        ),
    );
    let live_activity = closed_live_activity(&paths, &metadata.server_id);
    Arc::new(AppState {
        accounts: Arc::new(crate::accounts::runtime::AccountRuntime::default()),
        auth_token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        device_api_tokens: crate::auth::DeviceApiTokens::default(),
        security: crate::security::SecurityState::new("S1"),
        cef_devtools: Default::default(),
        automation_runtime,
        delayed_send_runtime,
        board_start_work_gate: Arc::new(Mutex::new(())),
        build_identity: "test-build".to_string(),
        config,
        event_hub,
        extension_registry,
        logger: Arc::new(GxserverLogger::new(paths.clone())),
        metadata,
        migration: create_gxserver_migration_status(&storage),
        paths,
        presentation_event_sequence,
        push,
        live_activity,
        remote_pairing_runtime: crate::remote_access::RemotePairingRuntime::new(),
        repository_clone_jobs: RepositoryCloneJobManager::default(),
        session_chat_followers: Arc::new(Mutex::new(HashMap::new())),
        session_chat_option_cache: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx,
        stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
        tailcat_runtime: crate::tailcat::TailcatRuntime::new(),
        version: "0.0.0-test".to_string(),
        zmx_title_observers: Arc::new(Mutex::new(HashMap::new())),
    })
}

pub(super) async fn add_project_path_for_server_test(
    state: Arc<AppState>,
    token: &str,
    project_path: &Path,
    name: Option<&str>,
) -> Value {
    let mut params = Map::new();
    params.insert(
        "path".to_string(),
        Value::String(path_to_string(project_path)),
    );
    if let Some(name) = name {
        params.insert("name".to_string(), Value::String(name.to_string()));
    }
    let response = route_http(
        state,
        rpc_request(
            "/api/addProjectPath",
            token,
            json!({ "params": Value::Object(params) }),
        ),
        "request-add-project-path".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    response_json(response.response).await["result"]["project"].clone()
}

pub(super) fn git_available() -> bool {
    StdCommand::new("git")
        .arg("--version")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(super) fn create_git_repository_for_server_test(repository_path: &Path) {
    fs::create_dir_all(repository_path).expect("repo dir");
    run_git_for_server_test(repository_path, &["init"]);
    run_git_for_server_test(
        repository_path,
        &["config", "user.email", "ghostex-tests@example.invalid"],
    );
    run_git_for_server_test(repository_path, &["config", "user.name", "Ghostex Tests"]);
    fs::write(repository_path.join("README.md"), "initial\n").expect("readme");
    run_git_for_server_test(repository_path, &["add", "README.md"]);
    run_git_for_server_test(repository_path, &["commit", "-m", "initial"]);
}

pub(super) fn run_git_for_server_test(cwd: &Path, args: &[&str]) -> String {
    let output = run_git_status_for_server_test(cwd, args);
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub(super) fn run_git_status_for_server_test(cwd: &Path, args: &[&str]) -> std::process::Output {
    StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git command")
}

pub(super) fn test_health(build_identity: &str) -> ServerHealthResponse {
    let config = create_default_gxserver_config().expect("config");
    ServerHealthResponse {
        ok: true,
        product: GXSERVER_PRODUCT.to_string(),
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        version: "0.1.0".to_string(),
        build_identity: build_identity.to_string(),
        capabilities: vec![],
        listeners: config.listeners.clone(),
        migration: MigrationStatus {
            applied_migrations: vec![],
            current_version: 0,
            state_db_file: String::new(),
            state_imports: None,
        },
        pid: 123,
        port: config.listeners.local.port,
        server_id: "S7k".to_string(),
        started_at: "2026-05-30T10:00:00.000Z".to_string(),
        tools: vec![],
        launch_context: None,
        open_files: None,
    }
}

pub(super) fn rpc_request(path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(
            GXSERVER_PROTOCOL_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

pub(super) async fn response_json(response: Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}
