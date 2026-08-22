use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command as StdCommand,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use anyhow::{anyhow, Context, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{
        header::{self, HeaderName, HeaderValue},
        HeaderMap, Method, Request, Response, StatusCode, Uri,
    },
    response::IntoResponse,
    routing::any,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpListener,
    process::Command,
    sync::broadcast,
};
use tower::service_fn;
use uuid::Uuid;

use crate::{
    agent_hooks::{
        install_agent_hooks, read_agent_hook_status, read_codex_hook_session_identities,
        uninstall_agent_hooks,
    },
    agent_skills::{install_agent_skills, read_agent_skill_status},
    agents::{
        apply_created_session_identity, apply_live_process_session_identity,
        agent_metadata_title_revision, create_agent_session_params_for_project,
        default_agent_command, dispatch_agent_endpoint, get_visible_terminal_title,
        normalize_agent_hook_activity, read_agent_settings, read_first_user_input_draft,
        read_text_from_map, reconcile_agent_metadata_title_for_session,
        resolve_project_agent_config, terminal_title_indicates_agent_identity, AgentEndpointError,
        FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY, FIRST_USER_INPUT_DRAFT_STATUS_KEY,
        FIRST_USER_INPUT_DRAFT_UPDATED_AT_KEY,
    },
    auth::{
        ensure_gxserver_auth_token, is_authorized_headers, is_expected_gxserver_auth_token,
        read_gxserver_auth_token,
    },
    automations::{handle_automation_endpoint, AutomationRuntime},
    config::{read_gxserver_config, GxserverConfig},
    constants::{
        GXSERVER_CAPABILITIES, GXSERVER_JSON_BODY_LIMIT_BYTES, GXSERVER_PRODUCT,
        GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION,
    },
    delayed_sends::DelayedSendRuntime,
    domain::{
        read_domain_rpc_params, read_optional_project_id, read_project_id, read_session_id,
        DomainRepository, DomainStateError,
    },
    events::{EventClientSender, GxserverEventHub},
    http_client,
    identity::ensure_gxserver_identity,
    ids::{is_gxserver_project_id, is_gxserver_session_id},
    logging::{
        log_level_from_status, query_gxserver_logs, DiagnosticLogScenario, GxserverLogInput,
        GxserverLogger, LogLevel, LogQueryError,
    },
    paths::{get_gxserver_paths, GxserverPaths},
    platform::shell::command_shell,
    portless::{
        apply_portless_state_update, log_portless_background_sync_failure,
        log_portless_background_sync_outcome, log_portless_state_update_failure,
        log_portless_state_update_success, read_portless_presentation_payload,
        read_portless_status_payload, read_portless_status_payload_for_paths,
        run_portless_background_sync_once, PortlessLogErrorCode, PortlessStateUpdate,
    },
    presentation::{
        build_presentation_project_delta, build_presentation_session_delta,
        increment_presentation_revision, list_previous_sessions, read_presentation_snapshot,
        search_presentation_sessions,
    },
    project_docs, project_git_remote, project_icon,
    protocol::{
        endpoint_for, is_remote_endpoint_allowed, protocol_mismatch_error, rpc_error, rpc_success,
        ApiPermission, ListenerKind, MigrationStatus, MinimalHealthResponse, RuntimeMetadata,
        ServerHealthResponse, Transport,
    },
    repository_clone::{
        dispatch_repository_clone_endpoint, RepositoryCloneError, RepositoryCloneJobManager,
        RepositoryCloneRuntime,
    },
    runtime::{
        create_source_build_identity, is_build_identity_reusable, remove_runtime_metadata,
        write_runtime_metadata,
    },
    navigation_history::{navigate_history, read_navigation_history, record_navigation_visit},
    session_chat_files::{
        handle_read_session_chat_files_http,
        handle_read_session_chat_image_http,
        handle_save_session_chat_attachment_http,
        handle_save_session_chat_image_http,
    },
    session_chat_follower::{
        stop_all_session_chat_followers,
        sync_session_chat_followers_for_all_sessions,
    },
    session_chat_options::SessionChatOptionCacheEntry,
    session_chat_queue_runtime::{
        handle_session_chat_queue_http,
        session_chat_queue_notice_reader,
        session_chat_queue_publisher_factory,
        session_chat_queue_sender_factory,
    },
    session_chat_read::handle_read_session_chat_http,
    session_chat_send::{
        handle_answer_session_chat_prompt_http,
        handle_handoff_session_chat_draft_http,
        handle_interrupt_session_chat_http,
        handle_send_session_chat_message_http,
    },
    session_chat_skills::handle_read_session_chat_skills_http,
    session_git_status, session_keep_awake, session_lifecycle,
    session_status::{agent_activity_presentation_refresh_delay_ms, iso_from_ms},
    session_transcript_export::handle_export_session_transcript_http,
    sidebar_hud::{
        create_sidebar_hud_settings_mutation, read_sidebar_hud,
        read_sidebar_hud_commands_by_project, read_sidebar_hud_global_commands,
        GlobalSidebarCommandUpdate,
    },
    sidebar_project_collections::{
        assign_project_to_sidebar_collection, read_sidebar_project_collections,
        update_sidebar_project_collections,
    },
    source_control::{dispatch_source_control_endpoint, SourceControlError},
    storage::{
        create_gxserver_migration_status, initialize_gxserver_storage, open_gxserver_database,
        open_gxserver_database_with_busy_timeout,
    },
    terminal_ws::{handle_terminal_socket, TerminalWsState},
    toolchain::{get_gxserver_tool_statuses, require_bundled_zmx, require_system_bd},
    typed_operations::{
        create_pull_request_for_project, dispatch_typed_operation_endpoint,
        dispatch_worktree_path_operation, typed_operation_log_details, typed_operation_log_level,
        TypedOperationError,
    },
    workspace_groups::{read_workspace_session_groups, update_workspace_session_groups},
    worktree_sessions,
    zmx::{
        append_zmx_endpoint_error_context, compensate_created_workspace_terminal,
        create_started_workspace_terminal, dispatch_zmx_lifecycle_endpoint,
        dispatch_zmx_session_interaction_endpoint, get_persisted_provider_startup_text_for_session,
        merge_session_with_renderer_result, prepare_focus_session_renderer_command,
        read_zmx_existing_session_names, read_zmx_session_process_identities, ZmxEndpointError,
        ZmxServerContext,
    },
};

pub mod agent_http;
pub mod agent_prompt_search_http;
pub mod background_tasks;
pub mod commit_message_generation;
pub mod http_endpoints;
pub mod http_infra;
pub mod presentation_delta;
pub mod project_paths;
pub mod session_state_sync;
pub mod title_generation;
pub mod typed_operation_http;
pub mod web_static;
pub mod worktree_ops;
pub mod ws;
pub mod zmx_http;
#[cfg(test)]
mod tests;

pub(crate) use agent_http::*;
pub(crate) use agent_prompt_search_http::*;
pub(crate) use background_tasks::*;
pub(crate) use commit_message_generation::*;
pub(crate) use http_endpoints::*;
pub(crate) use http_infra::*;
pub(crate) use presentation_delta::*;
pub(crate) use project_paths::*;
pub(crate) use session_state_sync::*;
pub(crate) use title_generation::*;
pub(crate) use typed_operation_http::*;
pub(crate) use web_static::*;
pub(crate) use worktree_ops::*;
pub(crate) use ws::*;
pub(crate) use zmx_http::*;

pub struct GxserverForegroundOptions {
    pub build_identity: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub version: String,
}

pub struct GxserverForegroundResult {
    pub reused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExistingGxserverState {
    Reusable,
    Running,
    Stopped,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) auth_token: String,
    pub(crate) automation_runtime: AutomationRuntime,
    pub(crate) delayed_send_runtime: DelayedSendRuntime,
    /// Serializes `/api/startBoardWork` so concurrent calls for one bead
    /// cannot both observe "no usable link" and create two worker sessions.
    pub(crate) board_start_work_gate: Arc<Mutex<()>>,
    pub(crate) build_identity: String,
    pub(crate) config: GxserverConfig,
    pub(crate) event_hub: GxserverEventHub,
    pub(crate) logger: Arc<GxserverLogger>,
    pub(crate) metadata: RuntimeMetadata,
    pub(crate) migration: MigrationStatus,
    pub(crate) paths: GxserverPaths,
    pub(crate) presentation_event_sequence: Arc<Mutex<()>>,
    pub(crate) repository_clone_jobs: RepositoryCloneJobManager,
    pub(crate) session_chat_followers: Arc<Mutex<HashMap<String, SessionChatFollowerEntry>>>,
    /// Per-session model/effort detection cache (observer key → last detect).
    /// Detection spawns `zmx history`, so every trigger reads through this.
    pub(crate) session_chat_option_cache: Arc<Mutex<HashMap<String, SessionChatOptionCacheEntry>>>,
    pub(crate) shutdown_tx: broadcast::Sender<()>,
    pub(crate) stale_activity_timers: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub(crate) version: String,
    pub(crate) zmx_title_observers: Arc<Mutex<HashMap<String, ZmxTitleObserverTask>>>,
}

pub(crate) struct RoutedResponse {
    pub(crate) endpoint_path: Option<String>,
    pub(crate) response: Response<Body>,
}

pub(crate) struct ZmxTitleObserverTask {
    pub(crate) handle: tokio::task::JoinHandle<()>,
    pub(crate) zmx_name: String,
}

/*
CDXC:SessionChatCore 2026-07-31:
Session Chat transcript followers mirror the zmx title-observer lifecycle
(sync from presentation deltas, boot sync, shutdown stop-all) but are ALSO
refcounted by live /api/events subscribers: the tail-follow task runs only
while at least one client subscribes AND the session is running. The entry
outlives the task so epoch/seq (stream) and the resnapshot signal survive
respawns, and so a sleeping session's subscribers pick the stream back up on
wake without resubscribing.
*/
pub(crate) struct SessionChatFollowerEntry {
    pub(crate) subscribers: usize,
    pub(crate) fingerprint: String,
    pub(crate) limit: usize,
    pub(crate) task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) stream: Arc<crate::session_chat::SessionChatStream>,
    pub(crate) resnapshot: Arc<tokio::sync::Notify>,
}

const GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS: u64 = 3_000;
const GXSERVER_FORK_INITIAL_RENAME_READY_DELAY_MS: u64 = 4_000;
const GXSERVER_FIRST_USER_INPUT_DRAFT_READY_DELAY_MS: u64 = 4_000;
const GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS: u64 = 300;
const GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH: usize = 250;
const GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH: usize = 39;
/*
CDXC:SessionHistoryTitleSource 2026-07-29:
Empty-title Generate Name summarizes the last few transcript user prompts
instead of one pasted blob. Recent messages carry the naming signal, so the
budget is per-message with a wider overall cap than the single-prompt source.
*/
const GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_COUNT: usize = 5;
const GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_MAX_LENGTH: usize = 400;
const GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MAX_LENGTH: usize = 2200;
const GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS: u64 = 30_000;
const GXSERVER_COMMIT_MESSAGE_GENERATION_TIMEOUT_MS: u64 = 120_000;
const GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES: u64 = 1024 * 1024;

const RENDERER_COMMAND_ACTIONS: &[&str] = &[
    "assertSidebarCard",
    "clickButton",
    "focusGroup",
    "focusSession",
    "fullReloadSession",
    "moveProject",
    "moveSidebar",
    "openBrowser",
    "openBrowserPane",
    "openPaths",
    "restartSession",
    /*
    CDXC:GenerateTitleSkill 2026-06-17-17:02:
    Generated-title `ghostex rename-command` now enters gxserver as a renderer command so macOS can submit Claude Code `/rename <title>` with a real native Enter event instead of zmx carriage-return text. Keep Rust's action allow-list in lockstep with the TypeScript daemon before full cutover.
    */
    "renameCommand",
    "runCommand",
    "saveAgent",
    "sendMessage",
    "setViewMode",
    "setVisibleCount",
    "switchProject",
    "toggleCloseAfterDone",
    "toggleSidebarCollapsed",
    "waitFor",
];
const PORTLESS_BACKGROUND_SYNC_INTERVAL: Duration = Duration::from_secs(10);
const AGENT_METADATA_TITLE_SYNC_INTERVAL: Duration = Duration::from_secs(1);
const SESSION_LIFECYCLE_SWEEP_INTERVAL: Duration =
    Duration::from_secs(session_lifecycle::SESSION_LIFECYCLE_SWEEP_INTERVAL_SECONDS);
const SESSION_GIT_STATUS_REFRESH_INTERVAL: Duration =
    Duration::from_secs(session_git_status::SESSION_GIT_STATUS_REFRESH_INTERVAL_SECONDS);
const WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL: Duration =
    Duration::from_secs(worktree_sessions::WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL_SECONDS);

/*
CDXC:GxserverRustPort 2026-06-14-20:37:
Phase 1 must be a real foreground daemon, not a mock harness. Startup creates TypeScript-compatible auth, config, identity, SQLite, runtime metadata, logs directory, local HTTP listener, health/control endpoints, and the minimal event stream needed by Phase 0 compatibility.

CDXC:GxserverLifecycle 2026-06-22-04:53:
Foreground Rust startup must own the selected loopback port like TypeScript: reuse the same build, stop and replace a same-protocol build mismatch, and surface protocol mismatches before binding so selected-port failures are explicit instead of falling through to generic EADDRINUSE.
*/
pub async fn run_gxserver_foreground(
    options: GxserverForegroundOptions,
) -> Result<GxserverForegroundResult> {
    let version = options.version;
    let build_identity = options
        .build_identity
        .unwrap_or_else(|| create_source_build_identity(&version));
    let paths = get_gxserver_paths(options.home_dir);

    let existing_auth = read_gxserver_auth_token(&paths)?;
    if let Some(existing) = http_client::fetch_server_health(
        existing_auth.as_ref().map(|auth| auth.token.as_str()),
        800,
    )? {
        match classify_existing_gxserver(Some(&existing), &build_identity) {
            ExistingGxserverState::Reusable => {
                return Ok(GxserverForegroundResult { reused: true });
            }
            ExistingGxserverState::Running => {
                let _ = http_client::request_server_stop(
                    existing_auth.as_ref().map(|auth| auth.token.as_str()),
                    2_000,
                )?;
                match wait_for_mismatched_gxserver_to_stop(
                    existing_auth.as_ref().map(|auth| auth.token.as_str()),
                    &build_identity,
                )
                .await?
                {
                    ExistingGxserverState::Reusable => {
                        return Ok(GxserverForegroundResult { reused: true });
                    }
                    ExistingGxserverState::Stopped => {}
                    ExistingGxserverState::Running => {
                        return Err(anyhow!(
                            "gxserver build identity changed, but the old control plane did not stop. Stop gxserver and launch Ghostex again so the current migration code can run."
                        ));
                    }
                }
            }
            ExistingGxserverState::Stopped => {}
        }
    }

    let storage = initialize_gxserver_storage(&paths)?;
    let config = read_gxserver_config(&paths)?;
    let identity = ensure_gxserver_identity(&paths)?;
    let auth = ensure_gxserver_auth_token(&paths)?;
    let logger = Arc::new(GxserverLogger::new(paths.clone()));
    let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let metadata = RuntimeMetadata {
        build_identity: build_identity.clone(),
        pid: std::process::id(),
        port: config.listeners.local.port,
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        server_id: identity.server_id,
        started_at,
        version: version.clone(),
    };
    let migration = create_gxserver_migration_status(&storage);
    let event_hub = GxserverEventHub::new(metadata.server_id.clone());
    let presentation_event_sequence = Arc::new(Mutex::new(()));
    let (shutdown_tx, _) = broadcast::channel(8);
    let local_host = config.listeners.local.host.clone();
    let local_port = config.listeners.local.port;
    let automation_runtime = AutomationRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        format!(
            "http://{}:{}",
            config.listeners.local.host, config.listeners.local.port
        ),
    );
    let delayed_send_runtime = DelayedSendRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        event_hub.clone(),
        presentation_event_sequence.clone(),
    );

    let state = Arc::new(AppState {
        auth_token: auth.token,
        automation_runtime,
        delayed_send_runtime,
        board_start_work_gate: Arc::new(Mutex::new(())),
        build_identity,
        config,
        event_hub,
        logger: logger.clone(),
        metadata: metadata.clone(),
        migration,
        paths: paths.clone(),
        presentation_event_sequence,
        repository_clone_jobs: RepositoryCloneJobManager::default(),
        session_chat_followers: Arc::new(Mutex::new(HashMap::new())),
        session_chat_option_cache: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: shutdown_tx.clone(),
        stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
        version,
        zmx_title_observers: Arc::new(Mutex::new(HashMap::new())),
    });
    /*
    CDXC:GxserverRustBuild 2026-06-24-20:22:
    The JSON RPC catch-all needs the raw Request so gxserver-rs can preserve the
    TypeScript protocol/auth/body gate order for every endpoint, including app
    user data. Use Axum's service fallback instead of the Handler extractor path
    so all non-/api/events requests still flow through the single RPC router.
    */
    let http_state = state.clone();
    let app = Router::new()
        .route("/api/events", any(handle_events))
        .route("/api/terminal", any(handle_terminal))
        .fallback_service(service_fn(move |request| {
            handle_http_request(http_state.clone(), request)
        }))
        .with_state(state.clone());

    let address: SocketAddr = format!("{local_host}:{local_port}")
        .parse()
        .expect("valid listener address");
    let listener = TcpListener::bind(address).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            anyhow!("Port {local_port} is already in use and did not respond as a compatible gxserver. Stop the conflicting process or update Ghostex/gxserver so their protocol versions match.")
        } else {
            anyhow!(error)
        }
    })?;

    write_runtime_metadata(&paths, &metadata)?;
    let _ = logger.log_routine(
        DiagnosticLogScenario::ServerLifecycle,
        GxserverLogInput {
            level: crate::logging::LogLevel::Info,
            event: "serverStarted".to_string(),
            server_id: Some(metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: None,
        },
    );
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": metadata.server_id.clone(),
        "type": "serverStarted",
    }));
    state.automation_runtime.start(shutdown_tx.subscribe());
    state.delayed_send_runtime.start(shutdown_tx.subscribe());
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    A queued prompt left in `sending` is ambiguous after a restart — the bytes
    may already have reached the agent — so it is retired as `failed` with an
    explicit reason and waits for the user. Never silently re-sent.
    */
    let _ = crate::session_chat_queue::recover_session_chat_queue_after_restart(&paths);
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    The queue scheduler is built HERE rather than beside the other runtimes
    because its three handles all close over the finished `Arc<AppState>`: the
    internal chat send (so a queued prompt inherits the per-session send mutex),
    the state-frame publisher, and the cached terminal notice. It must be
    started after restart recovery, so a row left `sending` by the previous
    process is already retired before the first tick can look at it.
    */
    crate::session_chat_queue_runtime::SessionChatQueueRuntime::new(
        paths.clone(),
        metadata.server_id.clone(),
        session_chat_queue_sender_factory(&state),
        session_chat_queue_publisher_factory(&state),
        session_chat_queue_notice_reader(&state),
    )
    .start(shutdown_tx.subscribe());
    sync_zmx_title_observers_for_all_sessions(&state, "server-start");
    sync_session_chat_followers_for_all_sessions(&state, "server-start");
    let agent_metadata_title_sync_task = spawn_agent_metadata_title_sync_task(&state);
    let portless_background_sync_task = spawn_portless_background_sync_task(&state);
    let session_lifecycle_sweep_task = spawn_session_lifecycle_sweep_task(&state);
    let session_git_status_refresh_task = spawn_session_git_status_refresh_task(&state);
    let worktree_branch_rename_task = spawn_worktree_branch_rename_task(&state);

    let mut shutdown_rx = shutdown_tx.subscribe();
    let shutdown_for_signal = shutdown_tx.clone();
    let state_for_signal = state.clone();
    tokio::spawn(async move {
        wait_for_process_signal().await;
        broadcast_server_stopping(&state_for_signal);
        let _ = shutdown_for_signal.send(());
    });

    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        })
        .await;
    agent_metadata_title_sync_task.abort();
    portless_background_sync_task.abort();
    session_lifecycle_sweep_task.abort();
    session_git_status_refresh_task.abort();
    worktree_branch_rename_task.abort();
    serve_result.with_context(|| "run gxserver HTTP listener")?;

    remove_runtime_metadata(&paths)?;
    stop_all_zmx_title_observers(&state);
    stop_all_session_chat_followers(&state);
    Ok(GxserverForegroundResult { reused: false })
}

async fn wait_for_mismatched_gxserver_to_stop(
    token: Option<&str>,
    expected_build_identity: &str,
) -> Result<ExistingGxserverState> {
    let deadline = Instant::now() + Duration::from_millis(5_000);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let state = probe_existing_gxserver_state(token, expected_build_identity)?;
        if state != ExistingGxserverState::Running {
            return Ok(state);
        }
    }
    probe_existing_gxserver_state(token, expected_build_identity)
}

fn probe_existing_gxserver_state(
    token: Option<&str>,
    expected_build_identity: &str,
) -> Result<ExistingGxserverState> {
    let health = http_client::fetch_server_health(token, 500)?;
    Ok(classify_existing_gxserver(
        health.as_ref(),
        expected_build_identity,
    ))
}

fn classify_existing_gxserver(
    health: Option<&ServerHealthResponse>,
    expected_build_identity: &str,
) -> ExistingGxserverState {
    match health {
        Some(health)
            if is_build_identity_reusable(
                Some(&health.build_identity),
                Some(expected_build_identity),
            ) =>
        {
            ExistingGxserverState::Reusable
        }
        Some(_) => ExistingGxserverState::Running,
        None => ExistingGxserverState::Stopped,
    }
}

async fn handle_http_request(
    state: Arc<AppState>,
    request: Request<Body>,
) -> std::result::Result<Response<Body>, Infallible> {
    let started_at = Instant::now();
    let client = request
        .extensions()
        .get::<SocketAddr>()
        .map(|address| address.ip().to_string());
    let is_web_bootstrap = request.uri().path() == "/api/webBootstrap";
    let headers = request.headers().clone();
    let request_id = request_id(&headers);
    let mut routed = route_http(state.clone(), request, request_id.clone()).await;
    if !is_web_bootstrap {
        apply_cors_headers(&headers, &mut routed.response, &state.config);
    }
    let status = routed.response.status().as_u16();
    let _ = state
        .logger
        .log_routine(DiagnosticLogScenario::ApiRequests, GxserverLogInput {
            level: log_level_from_status(status),
            event: "apiRequest".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client,
            duration_ms: Some(started_at.elapsed().as_millis()),
            error: None,
            details: Some(json!({
                "method": "http",
                "path": routed.endpoint_path,
                "statusCode": status,
            })),
        });
    if let Some(endpoint_path) = routed.endpoint_path.clone() {
        state.event_hub.broadcast(json!({
            "path": endpoint_path,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "requestId": request_id,
            "serverId": state.metadata.server_id.clone(),
            "type": "apiRequestHandled",
        }));
    }
    Ok(routed.response)
}

async fn route_http(
    state: Arc<AppState>,
    request: Request<Body>,
    request_id: String,
) -> RoutedResponse {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.clone();

    if path == "/api/webBootstrap" {
        return handle_web_bootstrap(&state, &parts.headers, &parts.uri, method, request_id);
    }
    if method == Method::GET && !path.starts_with("/api/") {
        return serve_web_static(&state.config, &path).await;
    }

    let endpoint = endpoint_for(&path);

    /*
    CDXC:GxserverProtocol 2026-06-22-04:10:
    Rust routing must preserve TypeScript's protocol gate order: CORS/OPTIONS is answered before minimal health, auth, method, body, and protocol checks. Unknown or WebSocket-only OPTIONS requests therefore return the HTTP-endpoint 404 envelope instead of the generic endpoint lookup message.
    */
    if method == Method::OPTIONS {
        let Some(endpoint) = endpoint else {
            return routed_json(
                None,
                StatusCode::NOT_FOUND,
                rpc_error(
                    "notFound",
                    format!("{path} is not a gxserver HTTP endpoint."),
                    Some(request_id),
                ),
            );
        };
        if endpoint.transport != Transport::Http {
            return routed_json(
                Some(endpoint.path),
                StatusCode::NOT_FOUND,
                rpc_error(
                    "notFound",
                    format!("{path} is not a gxserver HTTP endpoint."),
                    Some(request_id),
                ),
            );
        }
        if !is_remote_endpoint_allowed(ListenerKind::Local, endpoint.permission) {
            return routed_json(
                Some(endpoint.path.clone()),
                StatusCode::FORBIDDEN,
                rpc_error(
                    "forbidden",
                    format!(
                        "{} is not available on the remote gxserver listener.",
                        endpoint.path
                    ),
                    Some(request_id),
                ),
            );
        }
        return RoutedResponse {
            endpoint_path: Some(endpoint.path),
            response: StatusCode::NO_CONTENT.into_response(),
        };
    }

    if method == Method::GET && path == "/api/health" {
        return routed_json(
            Some("/api/health".to_string()),
            StatusCode::OK,
            MinimalHealthResponse::new(&state.version),
        );
    }

    let Some(endpoint) = endpoint else {
        return routed_json(
            None,
            StatusCode::NOT_FOUND,
            rpc_error(
                "notFound",
                format!("No gxserver endpoint for {} {}.", method.as_str(), path),
                Some(request_id),
            ),
        );
    };

    if endpoint.transport != Transport::Http {
        return routed_json(
            None,
            StatusCode::NOT_FOUND,
            rpc_error(
                "notFound",
                format!("No gxserver endpoint for {} {}.", method.as_str(), path),
                Some(request_id),
            ),
        );
    }

    if endpoint.path != "/api/health/server" && method != Method::POST {
        return routed_json(
            Some(endpoint.path.clone()),
            StatusCode::METHOD_NOT_ALLOWED,
            rpc_error(
                "methodNotAllowed",
                format!("{} requires POST.", endpoint.path),
                Some(request_id),
            ),
        );
    }
    if endpoint.path == "/api/health/server" && method != Method::GET {
        return routed_json(
            Some(endpoint.path.clone()),
            StatusCode::METHOD_NOT_ALLOWED,
            rpc_error(
                "methodNotAllowed",
                format!("{} requires GET.", endpoint.path),
                Some(request_id),
            ),
        );
    }

    if endpoint.requires_auth && !is_authorized_headers(&parts.headers, &state.auth_token) {
        return routed_json(
            Some(endpoint.path),
            StatusCode::UNAUTHORIZED,
            rpc_error(
                "unauthorized",
                "gxserver auth token is required for this endpoint.",
                Some(request_id),
            ),
        );
    }

    let body_json = if method == Method::POST {
        let body_limit_bytes = json_body_limit_bytes(&endpoint.path);
        match read_json_body(&parts.headers, body, body_limit_bytes).await {
            Ok(value) => value,
            Err(ReadBodyError::TooLarge) => {
                return routed_json(
                    Some(endpoint.path),
                    StatusCode::PAYLOAD_TOO_LARGE,
                    rpc_error(
                        "badRequest",
                        format!(
                            "Request body exceeds the gxserver JSON RPC limit of {body_limit_bytes} bytes."
                        ),
                        Some(request_id),
                    ),
                );
            }
            Err(ReadBodyError::InvalidJson) => {
                return routed_json(
                    Some(endpoint.path),
                    StatusCode::BAD_REQUEST,
                    rpc_error(
                        "badRequest",
                        "Request body must be valid JSON.",
                        Some(request_id),
                    ),
                );
            }
        }
    } else {
        json!({})
    };

    if endpoint.requires_protocol_version {
        let protocol_version = read_protocol_version(&parts.headers, &parts.uri, Some(&body_json));
        if !is_expected_protocol_version(protocol_version.as_ref()) {
            return routed_json(
                Some(endpoint.path),
                StatusCode::UPGRADE_REQUIRED,
                protocol_mismatch_error(protocol_version, Some(request_id)),
            );
        }
    }

    if !is_remote_endpoint_allowed(ListenerKind::Local, endpoint.permission) {
        return routed_json(
            Some(endpoint.path.clone()),
            StatusCode::FORBIDDEN,
            rpc_error(
                "forbidden",
                format!(
                    "{} is not available on the remote gxserver listener.",
                    endpoint.path
                ),
                Some(request_id),
            ),
        );
    }

    match endpoint.path.as_str() {
        "/api/health/server" => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            create_authenticated_health(&state),
        ),
        "/api/createProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.create_project(params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectAdded",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/updateProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.update_project(params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/relocateProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.relocate_project(params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/listProjects" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, _, _| {
                repository
                    .list_projects()
                    .map(|projects| json!({ "projects": projects }))
            },
        ),
        "/api/listRecentProjects" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, _, _| {
                repository
                    .list_recent_projects()
                    .map(|recent_projects| json!({ "recentProjects": recent_projects }))
            },
        ),
        "/api/closeProjectToRecent" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:GPUIRecentProjects 2026-06-24-12:38:
                GPUI's reused SidebarApp sends the same close-vs-remove command split as macOS. Close parks the canonical gxserver project with a server timestamp and broadcasts a presentation removal for active groups; remove remains the hard-delete endpoint.
                */
                let project_id = read_project_id(params)?;
                let project = repository.close_project_to_recent(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                let recent_projects = repository.list_recent_projects()?;
                Ok(json!({ "project": project, "recentProjects": recent_projects }))
            },
        ),
        "/api/restoreRecentProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:GPUIRecentProjects 2026-06-24-12:27:
                GPUI restores Recent Projects through gxserver project ids.
                The daemon clears explicit parked state and publishes the
                project presentation update; clients must not reconstruct rows
                from labels, stopped sessions, trusted client paths, or command
                output.
                */
                let project_id = read_project_id(params)?;
                let project = repository.restore_recent_project(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                let recent_projects = repository.list_recent_projects()?;
                Ok(json!({ "project": project, "recentProjects": recent_projects }))
            },
        ),
        "/api/removeRecentProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_project_id(params)?;
                let project = repository.remove_recent_project(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectRemoved",
                )?;
                let recent_projects = repository.list_recent_projects()?;
                Ok(json!({ "project": project, "recentProjects": recent_projects }))
            },
        ),
        "/api/readProjectStatus" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_project_id(params)?;
                let project = repository.get_project(&project_id)?.ok_or_else(|| {
                    DomainStateError::not_found(format!("Project {project_id} does not exist."))
                })?;
                /*
                CDXC:ProjectStatusParity 2026-06-22-06:21:
                readProjectStatus is a polling project-status read, but TypeScript gxserver repairs live zmx process identity before returning the project/session graph and schedules agent metadata title checks for eligible sessions. Keep those side effects here instead of treating the endpoint as a plain repository read so Rust clients receive the same status projection.
                */
                sync_zmx_provider_existence(&state, db, repository, Some(project_id.as_str()))?;
                sync_live_zmx_process_identities(
                    &state,
                    db,
                    repository,
                    Some(project_id.as_str()),
                    None,
                    "read-project-status",
                )?;
                let sessions = repository.list_sessions(Some(project_id.as_str()))?;
                schedule_agent_title_metadata_checks_for_sessions(&state, &sessions);
                Ok(json!({
                    "project": project,
                    "sessions": sessions,
                }))
            },
        ),
        "/api/addProjectPath" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.add_project_path(params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectAdded",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/createQuickProject" => {
            let home_dir = state.paths.home_dir.clone();
            let quick_project_state = state.clone();
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                move |repository, db, params, _| {
                    let project_params = create_quick_project_params(&home_dir, params)?;
                    let project = repository.add_project_path(&project_params)?;
                    let project_id = value_text(&project, "projectId")?;
                    schedule_presentation_project_delta(
                        &quick_project_state,
                        db,
                        repository,
                        &project_id,
                        "projectAdded",
                    )?;
                    Ok(json!({ "project": project }))
                },
            )
        }
        "/api/listProjectWorktrees"
        | "/api/createProjectWorktree"
        | "/api/openProjectWorktree"
        | "/api/mergeWorktreeIntoMain"
        | "/api/checkoutProjectNewBranch" => {
            handle_project_worktree_operation_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/removeProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_project_id(params)?;
                let project = repository.remove_project(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/deleteWorktreeProject" => {
            handle_delete_worktree_project_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/renameWorktreeProject" => {
            handle_rename_worktree_project_http(&state, endpoint.path, request_id, &body_json).await
        }
        /*
        CDXC:SidebarV2Worktrees 2026-07-29-00:00:
        Sidebar V2's worktree flow. Unlike `createProjectWorktree`, these
        endpoints never register the worktree as a project: the checkout is an
        attribute of one session (its cwd), and the branch shown on the card
        comes from the per-session git probe reading that cwd.
        */
        "/api/createWorktreeSession" | "/api/removeSessionWorktree" => {
            handle_worktree_session_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/createSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let created_session = repository.create_session(params, false)?;
                let session = apply_created_session_identity(repository, &created_session, params)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/createAgentSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.resolve_create_session_project(params)?;
                let create_params = create_agent_session_params_for_project(db, &project, params)?;
                let created_session = repository.create_session(&create_params, false)?;
                let session =
                    apply_created_session_identity(repository, &created_session, &create_params)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/listSessions" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_optional_project_id(params)?;
                sync_session_state_sidecars(
                    &state,
                    db,
                    repository,
                    project_id.as_deref(),
                    "list-sessions",
                )?;
                sync_zmx_provider_existence(&state, db, repository, project_id.as_deref())?;
                sync_live_zmx_process_identities(
                    &state,
                    db,
                    repository,
                    project_id.as_deref(),
                    None,
                    "list-sessions",
                )?;
                repository
                    .list_sessions(project_id.as_deref())
                    .map(|sessions| json!({ "sessions": sessions }))
            },
        ),
        "/api/updateSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let session = repository.update_session(params)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/updateSessionOrder" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let sessions = repository.update_session_order(params)?;
                for session in &sessions {
                    let project_id = value_text(session, "projectId")?;
                    let session_id = value_text(session, "sessionId")?;
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                }
                Ok(json!({ "sessions": sessions }))
            },
        ),
        /*
        CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
        Sidebar V2's settle/snooze commands. Guards live in
        `session_lifecycle` so a stale or raced client cannot park working or
        blocked-on-you work behind a settle, and every real change emits a
        presentation delta so all clients reclassify live. A no-op (double
        click, bulk settle over an already-settled row) intentionally skips the
        delta instead of churning the presentation revision.
        */
        "/api/settleSession"
        | "/api/unsettleSession"
        | "/api/snoozeSession"
        | "/api/unsnoozeSession" => {
            let lifecycle_path = endpoint.path.clone();
            let lifecycle_state = state.clone();
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                move |repository, db, params, _| {
                    let project_id = read_project_id(params)?;
                    let session_id = read_session_id(params)?;
                    let now = now_iso();
                    let outcome = match lifecycle_path.as_str() {
                        "/api/settleSession" => session_lifecycle::settle_session(
                            repository,
                            &project_id,
                            &session_id,
                            &now,
                        )?,
                        "/api/unsettleSession" => session_lifecycle::unsettle_session(
                            repository,
                            &project_id,
                            &session_id,
                            &now,
                        )?,
                        "/api/snoozeSession" => {
                            let snoozed_until = params
                                .get("snoozedUntil")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    DomainStateError::bad_request(
                                        "snoozedUntil must be an ISO timestamp in the future.",
                                    )
                                })?;
                            session_lifecycle::snooze_session(
                                repository,
                                &project_id,
                                &session_id,
                                snoozed_until,
                                &now,
                            )?
                        }
                        _ => session_lifecycle::unsnooze_session(
                            repository,
                            &project_id,
                            &session_id,
                            &now,
                        )?,
                    };
                    if outcome.changed {
                        schedule_presentation_session_delta(
                            &lifecycle_state,
                            db,
                            repository,
                            &project_id,
                            &session_id,
                        )?;
                    }
                    Ok(json!({
                        "changed": outcome.changed,
                        "session": outcome.session,
                    }))
                },
            )
        }
        "/api/removeSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let session = repository.remove_session(params)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/readPresentationSnapshot" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, _, server_id| {
                sync_session_state_sidecars(
                    &state,
                    db,
                    repository,
                    None,
                    "read-presentation-snapshot",
                )?;
                sync_zmx_provider_existence(&state, db, repository, None)?;
                sync_live_zmx_process_identities(
                    &state,
                    db,
                    repository,
                    None,
                    None,
                    "read-presentation-snapshot",
                )?;
                read_presentation_snapshot_in_sequence(&state, db, server_id)
                    .map(|snapshot| json!({ "snapshot": snapshot }))
            },
        ),
        "/api/readSidebarHud" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| {
                /*
                CDXC:SidebarHudContract 2026-06-24-20:34:
                GPUI Settings and SidebarApp read normalized launcher/action HUD rows through gxserver so app-modal Rust does not hand-mirror the shared TypeScript projection. The response is derived only from project domain metadata and carries no paths, project names, prompts, tokens, stdout/stderr, daemon bodies, or renderer payload authority.
                */
                let projects = repository.list_projects()?;
                let active_project_id = params
                    .get("activeProjectId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let mut hud = read_sidebar_hud(&projects, active_project_id);
                /*
                CDXC:MobileSidebarHud 2026-07-12-00:00:
                React Native Android renders agent-launcher and quick-action buttons for
                every visible project at once, so the mobile CLI transport asks
                for per-project command rows in one round trip instead of one
                readSidebarHud call per project each poll.
                */
                apply_commands_by_project_if_requested(&mut hud, &projects, params);
                /*
                CDXC:GlobalActions 2026-08-01-16:00:
                Global Actions live in their own daemon table rather than in
                project metadata, so they are attached here instead of inside
                read_sidebar_hud, which stays a pure projection of project rows.
                Served unconditionally rather than behind an opt-in flag: the
                list is one small array with no per-project fan-out, and every
                surface that renders the tab strip needs it on first paint.
                */
                if let Some(hud) = hud.as_object_mut() {
                    hud.insert(
                        "globalCommands".to_string(),
                        read_sidebar_hud_global_commands(
                            &repository.list_global_sidebar_commands()?,
                        ),
                    );
                }
                Ok(hud)
            },
        ),
        "/api/mutateSidebarHudSettings" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:SidebarHudSettingsMutation 2026-06-24-20:54:
                Settings mutation RPCs write through the production project repository after gxserver normalizes the narrow agent/action intent. Return refreshed HUD rows and updated project rows so GPUI clients do not reparse raw metadata or log command text, URLs, project names, paths, prompts, tokens, stdout/stderr, daemon bodies, or renderer payload contents.
                */
                let projects = repository.list_projects()?;
                let mutation = create_sidebar_hud_settings_mutation(&projects, params)?;
                let hud_active_project_id = mutation.hud_active_project_id;
                let mut item_ids = mutation.item_ids;
                /*
                CDXC:GlobalActions 2026-08-01-16:00:
                A reorder response must echo the order the daemon actually
                stored — the sidebar treats itemIds as the confirmation for its
                optimistic reorder and falls back to an empty list without it.
                The stored order can differ from the ids the client sent, since
                the repository keeps unlisted actions instead of dropping them.
                */
                let global_command_order_requested = matches!(
                    mutation.global_command_update,
                    Some(GlobalSidebarCommandUpdate::Order { .. })
                );
                let global_command_written = mutation.global_command_update.is_some();
                let mut updated_projects = Vec::new();
                for update in mutation.updates {
                    let project = repository.update_project(&update.params)?;
                    schedule_presentation_project_delta(
                        &state,
                        db,
                        repository,
                        &update.project_id,
                        "projectUpdated",
                    )?;
                    updated_projects.push(project);
                }
                /*
                CDXC:GlobalActions 2026-08-01-16:00:
                A Global Action write touches no project row, so it schedules no
                projectUpdated presentation delta.

                CDXC:GlobalActions 2026-08-07:
                It announces itself with its own event instead. Only the caller
                sees the refreshed HUD this response carries; every other live
                surface learns about HUD changes from a broadcast, and none of
                them polls the HUD on a timer. The GPUI sidebar refetches it
                when a projectUpdated delta arrives, which is why a project
                Action edit reaches the row at once and a Global Action edit did
                not — the row kept the stale list until some unrelated project
                delta happened to fire. The event carries no list, because
                /api/readSidebarHud stays the single projection of it, and it
                bumps the presentation revision so snapshot pollers converge as
                well, exactly like the sidebar-collection writes below.
                */
                match mutation.global_command_update {
                    Some(GlobalSidebarCommandUpdate::Save {
                        command_id,
                        definition,
                    }) => repository.save_global_sidebar_command(&command_id, &definition)?,
                    Some(GlobalSidebarCommandUpdate::Delete { command_id }) => {
                        repository.delete_global_sidebar_command(&command_id)?
                    }
                    Some(GlobalSidebarCommandUpdate::Order { command_ids }) => {
                        repository.order_global_sidebar_commands(&command_ids)?
                    }
                    None => {}
                }
                if global_command_written {
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let revision = increment_presentation_revision(db)?;
                    state.event_hub.broadcast(json!({
                        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                        "revision": revision,
                        "serverId": state.metadata.server_id.clone(),
                        "type": "globalSidebarCommandsChanged",
                    }));
                }
                let projects = repository.list_projects()?;
                let mut hud = read_sidebar_hud(&projects, hud_active_project_id.as_deref());
                let global_commands =
                    read_sidebar_hud_global_commands(&repository.list_global_sidebar_commands()?);
                if global_command_order_requested {
                    item_ids = Some(
                        global_commands
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_object)
                            .filter_map(|command| command.get("commandId"))
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect(),
                    );
                }
                if let Some(hud) = hud.as_object_mut() {
                    hud.insert("globalCommands".to_string(), global_commands);
                }
                /*
                CDXC:ProjectActions 2026-08-01:
                Clients that render per-project quick actions (GPUI sidebar rows)
                replace their whole HUD snapshot with this response, so the
                mutation mirrors readSidebarHud's opt-in commandsByProject block.
                Without it a Settings save would drop the per-project rows until
                the next full HUD poll.
                */
                apply_commands_by_project_if_requested(&mut hud, &projects, params);
                let mut result = Map::new();
                result.insert("hud".to_string(), hud);
                if let Some(item_ids) = item_ids {
                    result.insert(
                        "itemIds".to_string(),
                        Value::Array(item_ids.into_iter().map(Value::String).collect()),
                    );
                }
                result.insert("projects".to_string(), Value::Array(updated_projects));
                Ok(Value::Object(result))
            },
        ),
        /*
        CDXC:NavigationHistory 2026-08-19:
        Titlebar Back/Forward is one daemon-owned trail of previously active
        sessions and projects, shared by the gpui desktop titlebar and the web
        titlebar — see `navigation_history`. These three calls carry only
        opaque routing ids plus the display titles the sidebar already renders,
        so they sit with the other sidebar-state endpoints and need no
        repository or database access.
        */
        "/api/readNavigationHistory" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, _, params, _| read_navigation_history(params),
        ),
        "/api/recordNavigationVisit" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, _, params, _| record_navigation_visit(params),
        ),
        "/api/navigateHistory" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, _, params, _| navigate_history(params),
        ),
        "/api/readWorkspaceSessionGroups" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_workspace_session_groups(db).map(|groups| json!({ "groups": groups }))
            },
        ),
        "/api/updateWorkspaceSessionGroups" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| {
                /*
                CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
                GPUI write-through-syncs its whole normalized named-group overlay
                after each local edit. Bump the presentation revision and broadcast
                a dedicated event so snapshot pollers (mobile via CLI) and live
                sidebar clients converge without re-sending session rows.
                */
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let groups = update_workspace_session_groups(db, params)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "groups": groups.clone(),
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "type": "workspaceGroupsChanged",
                }));
                Ok(json!({ "groups": groups }))
            },
        ),
        "/api/readSidebarProjectCollections" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_sidebar_project_collections(db)
                    .map(|collections| json!({ "sidebarProjectCollections": collections }))
            },
        ),
        "/api/updateSidebarProjectCollections" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| {
                /*
                CDXC:SidebarProjectCollections 2026-07-18-00:00:
                Editors write-through-sync the whole normalized project-collection
                overlay after each local edit. Bump the presentation revision and
                broadcast a dedicated event so snapshot pollers (mobile via CLI)
                and live sidebar clients converge without re-sending project rows.
                */
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let collections = update_sidebar_project_collections(db, params)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "sidebarProjectCollections": collections.clone(),
                    "type": "sidebarProjectCollectionsChanged",
                }));
                Ok(json!({ "sidebarProjectCollections": collections }))
            },
        ),
        "/api/assignProjectToSidebarCollection" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = resolve_sidebar_collection_project_id(repository, params)?;
                let collection_title = params
                    .get("collectionTitle")
                    .or_else(|| params.get("group"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "group-project requires a non-empty sidebar group title.",
                        )
                    })?;
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let collections =
                    assign_project_to_sidebar_collection(db, &project_id, collection_title)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "sidebarProjectCollections": collections.clone(),
                    "type": "sidebarProjectCollectionsChanged",
                }));
                Ok(json!({
                    "projectId": project_id,
                    "sidebarProjectCollections": collections,
                }))
            },
        ),
        "/api/readAppUserData" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, _, _| repository.read_app_user_data(),
        ),
        "/api/readAutomationState"
        | "/api/saveAutomation"
        | "/api/deleteAutomation"
        | "/api/runAutomationNow"
        | "/api/setAutomationEnabled"
        | "/api/archiveAutomationRun"
        | "/api/markAutomationRunRead" => {
            handle_automation_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/scheduleDelayedSend" | "/api/cancelDelayedSend" | "/api/readDelayedSends" => {
            handle_delayed_send_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/saveScratchPad" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.save_scratch_pad(params),
        ),
        "/api/savePinnedPrompt" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.save_pinned_prompt(params),
        ),
        "/api/saveStashedPrompt" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.save_stashed_prompt(params),
        ),
        "/api/listStashedPrompts" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.list_stashed_prompts(params),
        ),
        "/api/deleteStashedPrompt" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.delete_stashed_prompt(params),
        ),
        /*
        CDXC:MobileKeepAwake 2026-08-19:
        A client that is ATTACHED to a session (Ghostex mobile, over its SSH CLI
        bridge) renews a keep-awake lease here so this machine's Auto Sleep sweep
        cannot retire a terminal somebody is actually looking at. The lease lives
        in memory with a TTL — see `session_keep_awake` — and is honored by
        `/api/sleepSession` only for automatic sweeps.
        */
        "/api/holdSessionsAwake" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _db, params, _| hold_sessions_awake(repository, params),
        ),
        "/api/searchSessions" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, server_id| search_presentation_sessions(db, server_id, params),
        ),
        "/api/listPreviousSessions" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, server_id| list_previous_sessions(db, server_id, params),
        ),
        "/api/readAgentSettings"
        | "/api/updateAgentSettings"
        | "/api/readAgentLaunchPlan"
        | "/api/readAgentResumePlan"
        | "/api/forkSession"
        | "/api/requestSessionRename"
        | "/api/cancelFirstPromptAutoTitle"
        | "/api/ingestSessionStateEvent"
        | "/api/ingestTerminalTitleEvent"
        | "/api/updateAgentActivity"
        | "/api/ingestAgentHookEvent" => {
            handle_agent_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readAgentSkillStatus" | "/api/installAgentSkills" => {
            handle_agent_skill_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readAgentHookStatus" | "/api/installAgentHooks" | "/api/uninstallAgentHooks" => {
            handle_agent_hook_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/createWorkspaceTerminal"
        | "/api/attachSessionMetadata"
        | "/api/probeSessionProvider"
        | "/api/startSessionProvider"
        | "/api/transitionSession"
        | "/api/sleepSession"
        | "/api/wakeSession"
        | "/api/killSession" => {
            handle_zmx_lifecycle_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readSessionText"
        | "/api/sendSessionText"
        | "/api/sendSessionMessage"
        | "/api/sendSessionEnter"
        | "/api/focusSession" => {
            handle_zmx_session_interaction_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/dispatchRendererCommand" => {
            handle_renderer_command_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/runGitAction"
        | "/api/runGitHubAction"
        | "/api/runWorktreeAction"
        | "/api/runProjectSetupCommand"
        | "/api/runBeadsAction" => {
            handle_typed_operation_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/runProjectDocsAction" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _db, params, _| {
                let project_id = read_project_id(params)?;
                let project = repository.get_project(&project_id)?.ok_or_else(|| {
                    DomainStateError::not_found(format!("Project {project_id} does not exist."))
                })?;
                let project_path = project
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| {
                        DomainStateError::bad_request("Project has no filesystem path.")
                    })?;
                /*
                CDXC:DocsRootDirectory 2026-08-09:
                Docs reads the project's own folder plus its configured Docs
                directory (then the Global Default). Resolving here keeps
                `run_project_docs_action` taking a plain root.

                CDXC:DocsRootAdditive 2026-08-09: a bad Docs directory no longer
                fails the request. It comes back as one unavailable mount inside
                the listing, so the project's own docs still show and the panel
                still names the path that could not be opened.
                */
                Ok(project_docs::run_project_docs_action(
                    &project_docs::resolve_project_docs_root(&project, project_path),
                    params,
                ))
            },
        ),
        "/api/startBoardWork" => {
            handle_board_start_work_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/generateCommitMessage" => {
            handle_generate_commit_message_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/generateSessionTitle" => {
            handle_generate_session_title_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/searchAgentPrompts" => {
            handle_search_agent_prompts_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/readAgentPromptText" => {
            handle_read_agent_prompt_text_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/toggleAgentPromptFavorite" => {
            handle_toggle_agent_prompt_favorite_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/resolveAgentPromptLaunch" => {
            handle_resolve_agent_prompt_launch_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/readSessionChat" => {
            handle_read_session_chat_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readSessionChatSkills" => {
            handle_read_session_chat_skills_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/readSessionChatFiles" => {
            handle_read_session_chat_files_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/sendSessionChatMessage" => {
            handle_send_session_chat_message_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/saveSessionChatImage" => {
            handle_save_session_chat_image_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/saveSessionChatAttachment" => {
            handle_save_session_chat_attachment_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/readSessionChatImage" => {
            handle_read_session_chat_image_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/answerSessionChatPrompt" => {
            handle_answer_session_chat_prompt_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/interruptSessionChat" => {
            handle_interrupt_session_chat_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/handoffSessionChatDraft" => {
            handle_handoff_session_chat_draft_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/readSessionChatQueue"
        | "/api/queueSessionChatPrompt"
        | "/api/updateSessionChatQueuedPrompt"
        | "/api/removeSessionChatQueuedPrompt"
        | "/api/reorderSessionChatQueue"
        | "/api/sendSessionChatQueuedPrompt"
        | "/api/setSessionChatDraft" => {
            handle_session_chat_queue_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/exportSessionTranscript" => {
            handle_export_session_transcript_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/createPullRequest" => {
            handle_create_pull_request_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/queryLogs" => handle_query_logs_http(&state, endpoint.path, request_id, &body_json),
        "/api/updatePortlessState" => {
            handle_portless_state_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/previewRepositoryClone"
        | "/api/startRepositoryClone"
        | "/api/readRepositoryCloneJob"
        | "/api/cancelRepositoryCloneJob" => {
            handle_repository_clone_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/browseProjectDirectories" => {
            handle_browse_project_directories_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/createProjectDirectory" => {
            handle_create_project_directory_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/discoverSourceControl" | "/api/lookupRepository" => {
            handle_source_control_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/resolveGitRootForPath" => {
            handle_resolve_git_root_for_path_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/control/stop" => {
            broadcast_server_stopping(&state);
            let response = routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(request_id, json!({})),
            );
            let _ = state.shutdown_tx.send(());
            response
        }
        "/api/control/stopAll" => {
            broadcast_server_stopping(&state);
            let response = routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({
                        "attemptedSessions": 0,
                        "failedSessions": 0,
                        "killedSessions": 0,
                        "skippedSessions": 0,
                    }),
                ),
            );
            let _ = state.shutdown_tx.send(());
            response
        }
        _ => routed_json(
            Some(endpoint.path.clone()),
            StatusCode::NOT_IMPLEMENTED,
            rpc_error(
                "notImplemented",
                format!(
                    "{} is defined but not implemented in this milestone.",
                    endpoint.path
                ),
                Some(request_id),
            ),
        ),
    }
}

fn handle_domain_http<F>(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    handler: F,
) -> RoutedResponse
where
    F: FnOnce(
        &DomainRepository<'_>,
        &rusqlite::Connection,
        &Map<String, Value>,
        &str,
    ) -> std::result::Result<Value, DomainStateError>,
{
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                },
            );
        }
    };
    let server_id = state.metadata.server_id.as_str();
    let repository = DomainRepository::new(&db, server_id);
    match handler(&repository, &db, &params, server_id) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
