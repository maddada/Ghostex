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
    agent_hooks::{install_agent_hooks, read_agent_hook_status, uninstall_agent_hooks},
    agent_skills::{install_agent_skills, read_agent_skill_status},
    agents::{
        apply_created_session_identity, apply_live_process_session_identity,
        create_agent_session_params_for_project, default_agent_command, dispatch_agent_endpoint,
        get_visible_terminal_title, normalize_agent_hook_activity, read_agent_settings,
        read_text_from_map, reconcile_agent_metadata_title_for_session,
        resolve_project_agent_config, AgentEndpointError,
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
    domain::{
        read_domain_rpc_params, read_optional_project_id, read_project_id, read_session_id,
        DomainRepository, DomainStateError,
    },
    events::{EventClientSender, GxserverEventHub},
    http_client,
    identity::ensure_gxserver_identity,
    ids::{is_gxserver_project_id, is_gxserver_session_id},
    logging::{
        log_level_from_status, query_gxserver_logs, GxserverLogInput, GxserverLogger, LogLevel,
        LogQueryError,
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
    project_git_remote, project_icon,
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
    session_git_status, session_lifecycle,
    session_status::agent_activity_presentation_refresh_delay_ms,
    sidebar_hud::{
        create_sidebar_hud_settings_mutation, read_sidebar_hud,
        read_sidebar_hud_commands_by_project,
    },
    sidebar_project_collections::{
        read_sidebar_project_collections, update_sidebar_project_collections,
    },
    source_control::{dispatch_source_control_endpoint, SourceControlError},
    storage::{
        create_gxserver_migration_status, initialize_gxserver_storage, open_gxserver_database,
        open_gxserver_database_with_busy_timeout,
    },
    t3_runtime::{
        parse_t3_runtime_panes_params, parse_t3_runtime_start_params, T3RuntimeManager,
        T3RuntimeStatusPayload,
    },
    terminal_ws::{handle_terminal_socket, TerminalWsState},
    toolchain::{get_gxserver_tool_statuses, require_bundled_zmx},
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
        dispatch_zmx_session_interaction_endpoint, merge_session_with_renderer_result,
        prepare_focus_session_renderer_command, read_zmx_existing_session_names,
        read_zmx_session_process_identities, ZmxEndpointError, ZmxServerContext,
    },
};

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
struct AppState {
    auth_token: String,
    automation_runtime: AutomationRuntime,
    build_identity: String,
    config: GxserverConfig,
    event_hub: GxserverEventHub,
    logger: Arc<GxserverLogger>,
    metadata: RuntimeMetadata,
    migration: MigrationStatus,
    paths: GxserverPaths,
    presentation_event_sequence: Arc<Mutex<()>>,
    repository_clone_jobs: RepositoryCloneJobManager,
    shutdown_tx: broadcast::Sender<()>,
    stale_activity_timers: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    t3_runtime: T3RuntimeManager,
    version: String,
    zmx_title_observers: Arc<Mutex<HashMap<String, ZmxTitleObserverTask>>>,
}

struct RoutedResponse {
    endpoint_path: Option<String>,
    response: Response<Body>,
}

struct ZmxTitleObserverTask {
    handle: tokio::task::JoinHandle<()>,
    zmx_name: String,
}

const GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS: u64 = 3_000;
const GXSERVER_FORK_INITIAL_RENAME_READY_DELAY_MS: u64 = 4_000;
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
    /*
    CDXC:MobileDelayedSend 2026-07-24:
    Mobile arms Delayed Send / Close After Done through `ghostex delayed-send`
    and `ghostex close-after-done`; the timers live in the connected sidebar
    renderer, so those actions must pass this allowlist as renderer commands.
    */
    "cancelDelayedSend",
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
    "scheduleDelayedSend",
    "sendMessage",
    "setViewMode",
    "setVisibleCount",
    "switchProject",
    "toggleCloseAfterDone",
    "toggleSidebarCollapsed",
    "waitFor",
];
const PORTLESS_BACKGROUND_SYNC_INTERVAL: Duration = Duration::from_secs(10);
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

    let state = Arc::new(AppState {
        auth_token: auth.token,
        automation_runtime,
        build_identity,
        config,
        event_hub,
        logger: logger.clone(),
        metadata: metadata.clone(),
        migration,
        paths: paths.clone(),
        presentation_event_sequence: Arc::new(Mutex::new(())),
        repository_clone_jobs: RepositoryCloneJobManager::default(),
        shutdown_tx: shutdown_tx.clone(),
        stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
        t3_runtime: T3RuntimeManager::new(&paths),
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
    let _ = logger.log(GxserverLogInput {
        level: crate::logging::LogLevel::Info,
        event: "serverStarted".to_string(),
        server_id: Some(metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: None,
    });
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": metadata.server_id.clone(),
        "type": "serverStarted",
    }));
    state.automation_runtime.start(shutdown_tx.subscribe());
    sync_zmx_title_observers_for_all_sessions(&state, "server-start");
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
    portless_background_sync_task.abort();
    session_lifecycle_sweep_task.abort();
    session_git_status_refresh_task.abort();
    worktree_branch_rename_task.abort();
    serve_result.with_context(|| "run gxserver HTTP listener")?;

    remove_runtime_metadata(&paths)?;
    stop_all_zmx_title_observers(&state);
    state.t3_runtime.abort_background_tasks();
    Ok(GxserverForegroundResult { reused: false })
}

fn spawn_portless_background_sync_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    /*
    CDXC:PortlessBackgroundSync 2026-06-22-23:40:
    Phase 9 route sync must run inside gxserver-rs without depending on Resources/sidebar polling, while staying lightweight for startup and shutdown. Run each sync pass off the async worker, retry on a conservative interval, and listen to gxserver's existing shutdown broadcast instead of adding a separate lifecycle channel.
    */
    let paths = state.paths.clone();
    let logger = state.logger.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let sync_paths = paths.clone();
            let sync_logger = logger.clone();
            let sync_result = tokio::task::spawn_blocking(move || {
                let started_at = Instant::now();
                match run_portless_background_sync_once(&sync_paths) {
                    Ok(outcome) => {
                        log_portless_background_sync_outcome(
                            &sync_logger,
                            &outcome,
                            started_at.elapsed().as_millis(),
                        );
                    }
                    Err(_) => {
                        log_portless_background_sync_failure(
                            &sync_logger,
                            PortlessLogErrorCode::BackgroundSyncFailed,
                            started_at.elapsed().as_millis(),
                        );
                    }
                }
            })
            .await;
            if sync_result.is_err() {
                log_portless_background_sync_failure(
                    &logger,
                    PortlessLogErrorCode::BackgroundSyncTaskJoinFailed,
                    0,
                );
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(PORTLESS_BACKGROUND_SYNC_INTERVAL) => {}
            }
        }
    })
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
Sidebar V2's auto-settle window and spent-snooze collection are server rules, so
they run on gxserver's own clock instead of whichever client happens to be open.
The pass is deliberately cheap (one SQLite read, at most
`SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS` narrow writes) and rides the existing
background-task shape: blocking work off the async worker, shutdown through the
shared broadcast, one presentation delta per changed session.

Precision: auto-settle boundaries are days away and snooze wakes are resolved
client-side from `snoozedUntil` to the millisecond, so a one-minute cadence adds
no user-visible latency.

Scope: the auto-settle rule only applies when the shared sidebar settings file
selects Sidebar V2 (`read_sweep_auto_settle_after_days` returns `None` for V1,
which is also the default). Spent-snooze collection and the activity reset run
for everyone, so a user who flips back to V1 with snoozed rows still gets that
state groomed.
*/
fn spawn_session_lifecycle_sweep_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let sweep_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = sweep_state.clone();
            let pass_result = tokio::task::spawn_blocking(move || {
                if let Err(error) = run_session_lifecycle_sweep_once(&pass_state) {
                    log_session_lifecycle_sweep_failure(&pass_state, &error.message);
                }
            })
            .await;
            if pass_result.is_err() {
                log_session_lifecycle_sweep_failure(&sweep_state, "sweep task join failed");
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(SESSION_LIFECYCLE_SWEEP_INTERVAL) => {}
            }
        }
    })
}

fn run_session_lifecycle_sweep_once(
    state: &Arc<AppState>,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let auto_settle_after_days = session_lifecycle::read_sweep_auto_settle_after_days(&state.paths);
    let auto_settle_on_finished_pull_request =
        session_lifecycle::auto_settle_on_finished_pull_request(auto_settle_after_days);
    let options = session_lifecycle::SessionLifecycleSweepOptions {
        auto_settle_after_days,
        auto_settle_on_finished_pull_request,
        max_mutations: session_lifecycle::SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS,
        now_iso: now_iso(),
    };
    /*
    CDXC:SidebarV2GitStatus 2026-07-29-00:00:
    The sweep reads the git-status cache the refresh pass below maintains; it
    never probes. A cwd that has not been probed yet resolves to "unknown", which
    settles nothing, so a cold daemon simply waits for its first refresh pass.

    CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
    The disposition is resolved through the session's project, exactly like the
    probe pass and presentation, so PR-driven auto-settle can fire for a cwd-less
    agent session instead of silently reading "unknown" forever. The lookup map is
    built once per sweep; a session whose project row is gone resolves `None` and
    keeps the old "unknown settles nothing" behaviour.
    */
    let projects = repository.list_projects()?;
    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|project_id| (project_id, project))
        })
        .collect();
    let outcome = session_lifecycle::run_session_lifecycle_sweep(
        &repository,
        &options,
        &|session: &Value| {
            let project = session
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|project_id| projects_by_id.get(project_id).copied());
            session_git_status::session_pull_request_disposition(session, project)
        },
    )?;
    for (project_id, session_id) in &outcome.changed {
        schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)?;
    }
    Ok(())
}

/*
CDXC:SidebarV2GitStatus 2026-07-29-00:00:
Sidebar V2's card row (branch, +n −n, PR badge) is server-owned, so the probing
happens here on gxserver's own clock instead of in whichever client is open.
The pass shape mirrors the lifecycle sweep: blocking work off the async worker,
shutdown through the shared broadcast, one presentation delta per changed
session.

Cost control lives in `session_git_status`: probes are keyed by UNIQUE session
cwd, cached with TTLs, bounded per pass, and every subprocess is time-boxed. The
pass only emits deltas for cwds whose status MEANINGFULLY changed — a re-probe
that finds the same branch and the same counts is silent, so a quiet machine
produces no event traffic at all.
*/
fn spawn_session_git_status_refresh_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let refresh_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = refresh_state.clone();
            let pass_result = tokio::task::spawn_blocking(move || {
                /*
                CDXC:SidebarV2DataGate 2026-07-29:
                All three passes below feed Sidebar V2 surfaces and nothing else,
                so they run only while this machine is ON V2 — the same
                `sidebarVersion` gate the auto-settle sweep already applies. One
                settings read per PASS, shared by the three, and deliberately
                inside the loop rather than hoisted to task spawn: flipping the
                toggle then takes effect within one interval instead of needing a
                daemon restart.
                */
                let sidebar_v2_selected =
                    session_lifecycle::read_sidebar_v2_selected(&pass_state.paths);
                if let Err(error) =
                    run_session_git_status_refresh_once(&pass_state, sidebar_v2_selected)
                {
                    log_session_git_status_refresh_failure(&pass_state, &error.message);
                }
                /*
                CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
                The per-project `origin` remote probe rides this same blocking
                worker and this same 60s wake-up instead of adding a fifth
                background task: it is the same kind of work (time-boxed git
                spawns feeding a TTL cache that presentation only reads), and its
                own 10-minute TTL means almost every pass here is a no-op that
                spawns nothing. A failure in one pass must not skip the other, so
                they are independent statements rather than a `?` chain.
                */
                if let Err(error) =
                    run_project_git_remote_refresh_once(&pass_state, sidebar_v2_selected)
                {
                    log_project_git_remote_refresh_failure(&pass_state, &error.message);
                }
                /*
                CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
                The project-icon discovery pass rides the same worker and the
                same wake-up as the two git passes above, for the same reasons:
                it is bounded filesystem work feeding a TTL cache presentation
                only reads, and with a 10/30-minute TTL almost every pass here
                finds nothing stale and touches no files at all. Independent
                statement, so a failure in one pass never skips another.
                */
                if let Err(error) = run_project_icon_refresh_once(&pass_state, sidebar_v2_selected)
                {
                    log_project_icon_refresh_failure(&pass_state, &error.message);
                }
            })
            .await;
            if pass_result.is_err() {
                log_session_git_status_refresh_failure(&refresh_state, "refresh task join failed");
            }
            let abandoned_readers = session_git_status::take_abandoned_command_readers();
            if abandoned_readers > 0 {
                log_session_git_status_reader_abandoned(&refresh_state, abandoned_readers);
            }
            /*
            The `origin` probe keeps its own counter and its own event name: the
            two passes run different commands against different directories, so
            reporting one under the other's name would send anyone reading the
            log to the wrong probe.
            */
            let abandoned_remote_readers =
                session_git_status::take_abandoned_project_git_remote_readers();
            if abandoned_remote_readers > 0 {
                log_project_git_remote_reader_abandoned(&refresh_state, abandoned_remote_readers);
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(SESSION_GIT_STATUS_REFRESH_INTERVAL) => {}
            }
        }
    })
}

fn run_session_git_status_refresh_once(
    state: &Arc<AppState>,
    sidebar_v2_selected: bool,
) -> std::result::Result<(), DomainStateError> {
    /*
    CDXC:SidebarV2DataGate 2026-07-29:
    `refresh_session_git_status_cache` enforces the gate itself, so this early
    return is about the rest of the pass: on a V1 machine there is no reason to
    open SQLite and walk every session row to build a cwd set nobody will probe.
    */
    if !sidebar_v2_selected {
        return Ok(());
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    /*
    Only LIVE sessions are probed, and many of them share a checkout, so the
    cache is fed the DEDUPLICATED cwd set: one git (and at most one `gh`) call
    answers every row pointing at that directory.

    This is narrower than what presentation publishes: pinned and stopped rows
    reach the sidebar too, and they are deliberately NOT probed. The trade-off is
    that such a row shows whatever git status its cwd last had (or none at all,
    if nothing live ever pointed there) rather than costing a git spawn a minute
    for a checkout nobody is working in.

    CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
    The set is built from each session's EFFECTIVE cwd (`session.cwd` else
    `project.path`, see `session_git_status::effective_session_git_cwd`), so the
    project ROOT of a cwd-less agent session actually gets probed. One project
    lookup map per pass keeps that resolution O(1) per session, and because the
    cache is keyed by cwd, one probe of the project path lights up every row
    pointing at it — including the stopped and pinned rows this pass never probes
    for.
    */
    let projects = repository.list_projects()?;
    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|project_id| (project_id, project))
        })
        .collect();
    let session_project = |session: &Value| -> Option<&Value> {
        session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|project_id| projects_by_id.get(project_id).copied())
    };
    let sessions = repository.list_sessions(None)?;
    let mut cwds: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for session in &sessions {
        if !crate::presentation::is_active(session) {
            continue;
        }
        let Some(cwd) =
            session_git_status::effective_session_git_cwd(session, session_project(session))
        else {
            continue;
        };
        if seen.insert(cwd.clone()) {
            cwds.push(cwd);
        }
    }

    let changed: HashSet<String> =
        session_git_status::refresh_session_git_status_cache(&cwds, sidebar_v2_selected)
            .into_iter()
            .collect();
    if changed.is_empty() {
        return Ok(());
    }
    for session in &sessions {
        if !crate::presentation::is_active(session) {
            continue;
        }
        let Some(cwd) =
            session_git_status::effective_session_git_cwd(session, session_project(session))
        else {
            continue;
        };
        if !changed.contains(&cwd) {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)?;
    }
    Ok(())
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
Sidebar V2 groups the same repository across machines by its `origin` remote, so
gxserver resolves that remote for its own projects and ships it in presentation.

Scope: the projects presentation actually publishes — parked Recent Projects and
hidden carrier projects are skipped so they cost no git spawns. Registered
worktree projects resolve their FAMILY ROOT (see `project_git_remote_key`), so a
project and its worktrees share one probe and one answer.

Cost: with a 10-minute TTL almost every 60s pass finds nothing stale and spawns
nothing at all. A pass only emits deltas for projects whose remote actually
CHANGED, which for real repositories is approximately never.
*/
fn run_project_git_remote_refresh_once(
    state: &Arc<AppState>,
    sidebar_v2_selected: bool,
) -> std::result::Result<(), DomainStateError> {
    // Same gate, same reason as the git-status pass above: a V1 machine does not
    // open the database to assemble a project path set it will not probe.
    if !sidebar_v2_selected {
        return Ok(());
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let mut paths: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_git_remote::project_git_remote_key(project) else {
            continue;
        };
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    let changed: HashSet<String> =
        project_git_remote::refresh_project_git_remote_cache(&paths, sidebar_v2_selected)
            .into_iter()
            .collect();
    if changed.is_empty() {
        return Ok(());
    }
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_git_remote::project_git_remote_key(project) else {
            continue;
        };
        if !changed.contains(&path) {
            continue;
        }
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")?;
    }
    Ok(())
}

/*
CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
Sidebar V2 shows each project the icon its own repository ships, so gxserver
discovers that icon for its own projects and ships it in presentation.

Scope, keying, and cost mirror the `origin` pass exactly: only the projects
presentation publishes (parked and hidden ones cost nothing), keyed on the
worktree FAMILY ROOT so a project and its worktrees share one answer, and a delta
only for projects whose icon CONTENT actually changed — which for a real
repository is approximately never.

It is a separate pass rather than an extra step inside the `origin` one so that a
failure, a budget, or a TTL change on either side stays local to that probe.
*/
fn run_project_icon_refresh_once(
    state: &Arc<AppState>,
    sidebar_v2_selected: bool,
) -> std::result::Result<(), DomainStateError> {
    // Same gate, same reason as the two git passes above.
    if !sidebar_v2_selected {
        return Ok(());
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let mut paths: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_icon::project_icon_key(project) else {
            continue;
        };
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    let changed: HashSet<String> =
        project_icon::refresh_project_icon_cache(&paths, sidebar_v2_selected)
            .into_iter()
            .collect();
    if changed.is_empty() {
        return Ok(());
    }
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_icon::project_icon_key(project) else {
            continue;
        };
        if !changed.contains(&path) {
            continue;
        }
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")?;
    }
    Ok(())
}

fn log_project_icon_refresh_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "projectIconRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

fn log_project_git_remote_refresh_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "projectGitRemoteRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

fn log_session_git_status_refresh_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionGitStatusRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

/*
The pass itself is fine: a probe command exited, but something it left behind
still held its stdout pipe, so the thread draining that pipe had to be abandoned
instead of joined. Worth a line, because a machine that leaks these has a git or
`gh` helper surviving its process group.
*/
/// The same fact for the per-project `origin` probe, under its own event so the
/// leak is attributed to the pass that produced it.
fn log_project_git_remote_reader_abandoned(state: &Arc<AppState>, count: usize) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "projectGitRemoteReaderAbandoned".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({ "count": count })),
    });
}

fn log_session_git_status_reader_abandoned(state: &Arc<AppState>, count: usize) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionGitStatusReaderAbandoned".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({ "count": count })),
    });
}

fn log_session_lifecycle_sweep_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionLifecycleSweepFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
A worktree session starts on `ghostex/<8hex>` because nothing yet knows what the
work is. Once the session has a REAL title — from the auto-rename skill, from
first-prompt title generation, or from the user typing one — that name is the
better branch name, so gxserver renames the branch to `ghostex/<slug>`.

Why a pass instead of a hook: a title reaches a session through several
independent paths (rename RPC, agent-metadata reconciliation, the generated-title
job, terminal title events), and every one of them ends in the same durable
place. Reconciling from durable state is one code path instead of five, and it is
idempotent — `plan_worktree_branch_rename` is a pure function of the row, and the
marker it stamps makes the rename happen exactly once.

Cost: a pure read of the session table per minute. Git only runs when a rename is
actually due, and the pass touches at most
`WORKTREE_BRANCH_RENAME_MAX_PER_PASS` branches. A rename lands within one pass of
the title, and the branch label on the card refreshes with it because the git
status cache is re-probed for that cwd before the delta goes out.
*/
fn spawn_worktree_branch_rename_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let sweep_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = sweep_state.clone();
            let pass_result = tokio::task::spawn_blocking(move || {
                if let Err(error) = run_worktree_branch_rename_once(&pass_state) {
                    log_worktree_branch_rename_failure(&pass_state, &error.message);
                }
            })
            .await;
            if pass_result.is_err() {
                log_worktree_branch_rename_failure(&sweep_state, "rename task join failed");
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL) => {}
            }
        }
    })
}

fn run_worktree_branch_rename_once(
    state: &Arc<AppState>,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let plans = repository
        .list_sessions(None)?
        .iter()
        .filter_map(worktree_sessions::plan_worktree_branch_rename)
        .take(worktree_sessions::WORKTREE_BRANCH_RENAME_MAX_PER_PASS)
        .collect::<Vec<_>>();
    for plan in plans {
        /*
        Re-read the branch from the checkout itself: the marker says what
        gxserver created, but the user may have switched or renamed the branch
        in the meantime, and that decision wins.
        */
        if worktree_sessions::current_worktree_branch(&plan.worktree_path).as_deref()
            != Some(plan.from_branch.as_str())
        {
            continue;
        }
        let worktree_path = plan.worktree_path.clone();
        let Some(renamed) = worktree_sessions::resolve_renamed_branch_name(
            &plan.title,
            &plan.from_branch,
            &|candidate| worktree_sessions::worktree_branch_exists(&worktree_path, candidate),
        ) else {
            continue;
        };
        if !worktree_sessions::rename_worktree_branch(
            &plan.worktree_path,
            &plan.from_branch,
            &renamed,
        ) {
            continue;
        }
        let Some(session) = repository.get_session(&plan.project_id, &plan.session_id)? else {
            continue;
        };
        if let Some(runtime_settings) =
            worktree_sessions::runtime_settings_with_renamed_worktree_branch(
                &session,
                &renamed,
                &now_iso(),
            )
        {
            let mut update = Map::new();
            update.insert("projectId".to_string(), json!(plan.project_id));
            update.insert("sessionId".to_string(), json!(plan.session_id));
            update.insert(
                "runtimeSettings".to_string(),
                Value::Object(runtime_settings),
            );
            repository.update_session(&update)?;
        }
        /*
        CDXC:SidebarV2DataGate 2026-07-29:
        Re-probing the renamed checkout is Sidebar V2 card data, so it answers to
        the same `sidebarVersion` gate as the background passes; on a V1 machine
        the rename still lands, it just does not warm a cache nothing reads. The
        read sits here rather than at the top of the pass because a rename is a
        rare event — most passes reach this line zero times.
        */
        session_git_status::refresh_session_git_status_cache(
            &[plan.worktree_path.clone()],
            session_lifecycle::read_sidebar_v2_selected(&state.paths),
        );
        schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &plan.project_id,
            &plan.session_id,
        )?;
    }
    Ok(())
}

fn log_worktree_branch_rename_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "worktreeBranchRenameFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
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
    let _ = state.logger.log(GxserverLogInput {
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

fn handle_web_bootstrap(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    method: Method,
    request_id: String,
) -> RoutedResponse {
    let endpoint_path = Some("/api/webBootstrap".to_string());
    if method != Method::POST {
        return routed_json(
            endpoint_path,
            StatusCode::METHOD_NOT_ALLOWED,
            rpc_error(
                "methodNotAllowed",
                "/api/webBootstrap requires POST.",
                Some(request_id),
            ),
        );
    }
    let Some(base_url) = request_host_origin(headers, uri) else {
        return routed_json(
            endpoint_path,
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                "A valid Host header is required for web bootstrap.",
                Some(request_id),
            ),
        );
    };
    if !web_bootstrap_origin_matches(headers, &base_url) {
        return routed_json(
            endpoint_path,
            StatusCode::FORBIDDEN,
            rpc_error(
                "forbidden",
                "/api/webBootstrap is available only to the same origin.",
                Some(request_id),
            ),
        );
    }
    let machine_label = match local_machine_label() {
        Ok(machine_label) => machine_label,
        Err(message) => {
            return routed_json(
                endpoint_path,
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", message, Some(request_id)),
            );
        }
    };
    routed_json(
        endpoint_path,
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "authToken": state.auth_token,
                "baseUrl": base_url,
                "machineLabel": machine_label,
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            }),
        ),
    )
}

fn request_host_origin(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .or_else(|| uri.authority().map(|authority| authority.as_str()))?
        .trim();
    let authority = host.parse::<axum::http::uri::Authority>().ok()?;
    Some(format!("http://{authority}"))
}

fn web_bootstrap_origin_matches(headers: &HeaderMap, base_url: &str) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    match origins.next() {
        None => true,
        Some(origin) => {
            matches!(origin.to_str(), Ok(origin) if origin == base_url) && origins.next().is_none()
        }
    }
}

fn local_machine_label() -> std::result::Result<String, String> {
    let mut buffer = [0_u8; 256];
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return Err("Failed to read the local machine hostname.".to_string());
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| "The local machine hostname is too long.".to_string())?;
    let hostname = std::str::from_utf8(&buffer[..length])
        .map_err(|_| "The local machine hostname is not valid UTF-8.".to_string())?;
    if hostname.is_empty() {
        return Err("The local machine hostname is empty.".to_string());
    }
    Ok(hostname.to_string())
}

async fn serve_web_static(config: &GxserverConfig, request_path: &str) -> RoutedResponse {
    let config = config.clone();
    let request_path = request_path.to_string();
    match tokio::task::spawn_blocking(move || serve_web_static_sync(&config, &request_path)).await {
        Ok(response) => response,
        Err(_) => static_status_response(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn serve_web_static_sync(config: &GxserverConfig, request_path: &str) -> RoutedResponse {
    let relative_path = match decode_web_path(request_path) {
        Ok(path) => path,
        Err(()) => return static_status_response(StatusCode::FORBIDDEN),
    };
    let dist_dir = resolve_web_dist_dir(config);
    let canonical_dist_dir = match fs::canonicalize(&dist_dir) {
        Ok(path) if path.is_dir() => path,
        _ => return web_not_built_response(),
    };
    let requested_relative = if relative_path.as_os_str().is_empty() {
        PathBuf::from("index.html")
    } else {
        relative_path
    };
    let requested_path = canonical_dist_dir.join(&requested_relative);

    match read_static_file(&canonical_dist_dir, &requested_path) {
        Ok(Some(response)) => return response,
        Ok(None) => {}
        Err(()) => return static_status_response(StatusCode::FORBIDDEN),
    }
    if requested_relative.extension().is_none() {
        let index_path = canonical_dist_dir.join("index.html");
        match read_static_file(&canonical_dist_dir, &index_path) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(()) => return static_status_response(StatusCode::FORBIDDEN),
        }
    }
    static_status_response(StatusCode::NOT_FOUND)
}

fn resolve_web_dist_dir(config: &GxserverConfig) -> PathBuf {
    if let Some(configured) = config.web.dist_dir.as_ref() {
        return configured.clone();
    }
    // Packaged builds resolve beside the executable; source builds use the checkout containing this crate.
    let executable_candidate = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .map(|directory| directory.join("ghostex-web/dist"));
    if let Some(candidate) = executable_candidate.filter(|candidate| candidate.is_dir()) {
        return candidate;
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join("ghostex-web/dist")
}

fn decode_web_path(request_path: &str) -> std::result::Result<PathBuf, ()> {
    let bytes = request_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let high = decode_hex_digit(bytes[index + 1]).ok_or(())?;
            let low = decode_hex_digit(bytes[index + 2]).ok_or(())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    if decoded.contains(&0) {
        return Err(());
    }
    let decoded = std::str::from_utf8(&decoded).map_err(|_| ())?;
    let mut relative = PathBuf::new();
    for segment in decoded.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err(()),
            segment => relative.push(segment),
        }
    }
    Ok(relative)
}

fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn read_static_file(
    dist_dir: &Path,
    path: &Path,
) -> std::result::Result<Option<RoutedResponse>, ()> {
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Ok(None),
    };
    if !canonical_path.starts_with(dist_dir) {
        return Err(());
    }
    if !canonical_path.is_file() {
        return Ok(None);
    }
    let bytes = match fs::read(&canonical_path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let content_type = static_content_type(&canonical_path);
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    let cache_control =
        if canonical_path.file_name().and_then(|name| name.to_str()) == Some("index.html") {
            "no-cache"
        } else if is_hashed_asset(&canonical_path) {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    Ok(Some(RoutedResponse {
        endpoint_path: None,
        response,
    }))
}

fn static_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        Some("map") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn is_hashed_asset(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((_, suffix)) = file_name.rsplit_once('-') else {
        return false;
    };
    let hash = suffix.split('.').next().unwrap_or_default();
    hash.len() >= 8
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn web_not_built_response() -> RoutedResponse {
    let html = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Ghostex Web</title></head><body><h1>ghostex-web is not built</h1><p>Run <code>bun run web:build</code> from the Ghostex checkout.</p></body></html>";
    let mut response = Response::new(Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}

fn static_status_response(status: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: status.into_response(),
    }
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
        match read_json_body(&parts.headers, body).await {
            Ok(value) => value,
            Err(ReadBodyError::TooLarge) => {
                return routed_json(
                    Some(endpoint.path),
                    StatusCode::PAYLOAD_TOO_LARGE,
                    rpc_error(
                        "badRequest",
                        format!(
                            "Request body exceeds the gxserver JSON RPC limit of {GXSERVER_JSON_BODY_LIMIT_BYTES} bytes."
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
        "/api/syncT3EmbeddedSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let session = sync_t3_embedded_session(repository, params)?;
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
                if params
                    .get("includeAllProjectCommands")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    if let Some(hud) = hud.as_object_mut() {
                        hud.insert(
                            "commandsByProject".to_string(),
                            read_sidebar_hud_commands_by_project(&projects),
                        );
                    }
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
                let item_ids = mutation.item_ids;
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
                let projects = repository.list_projects()?;
                let mut hud = read_sidebar_hud(&projects, hud_active_project_id.as_deref());
                /*
                CDXC:ProjectActions 2026-08-01:
                Clients that render per-project quick actions (GPUI sidebar rows)
                replace their whole HUD snapshot with this response, so the
                mutation mirrors readSidebarHud's opt-in commandsByProject block.
                Without it a Settings save would drop the per-project rows until
                the next full HUD poll.
                */
                if params
                    .get("includeAllProjectCommands")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    if let Some(hud) = hud.as_object_mut() {
                        hud.insert(
                            "commandsByProject".to_string(),
                            read_sidebar_hud_commands_by_project(&projects),
                        );
                    }
                }
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
        "/api/generateCommitMessage" => {
            handle_generate_commit_message_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/generateSessionTitle" => {
            handle_generate_session_title_http(&state, endpoint.path, request_id, &body_json).await
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
        "/api/discoverSourceControl" | "/api/lookupRepository" => {
            handle_source_control_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/resolveGitRootForPath" => {
            handle_resolve_git_root_for_path_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/t3Runtime/status"
        | "/api/t3Runtime/start"
        | "/api/t3Runtime/stop"
        | "/api/t3Runtime/panes" => {
            handle_t3_runtime_http(&state, endpoint.path, request_id, &body_json).await
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

/*
CDXC:GxserverLogs 2026-06-19-14:45:
Rust must route `/api/queryLogs` instead of returning milestone `notImplemented`. Keep the TypeScript RPC envelope and local authenticated gates while returning only sanitized JSONL entries already present in the support log file.
*/
fn handle_query_logs_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match query_gxserver_logs(&state.paths, &params) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(LogQueryError::Input(message)) => routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error("badRequest", message, Some(request_id)),
        ),
        Err(LogQueryError::Io(_)) => routed_json(
            Some(endpoint_path),
            StatusCode::INTERNAL_SERVER_ERROR,
            rpc_error(
                "internalError",
                "Failed to query gxserver logs.",
                Some(request_id),
            ),
        ),
    }
}

async fn handle_t3_runtime_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match endpoint_path.as_str() {
        "/api/t3Runtime/status" => t3_runtime_status_snapshot(state).await,
        "/api/t3Runtime/start" => match parse_t3_runtime_start_params(&params) {
            Ok(request) => {
                state.t3_runtime.request_start(request);
                t3_runtime_status_snapshot(state).await
            }
            Err(error) => Err(error),
        },
        "/api/t3Runtime/stop" => {
            let manager = state.t3_runtime.clone();
            tokio::task::spawn_blocking(move || manager.stop_runtime())
                .await
                .map_err(t3_runtime_task_error)
        }
        "/api/t3Runtime/panes" => match parse_t3_runtime_panes_params(&params) {
            Ok((client_id, session_ids)) => {
                state.t3_runtime.update_panes(client_id, session_ids);
                t3_runtime_status_snapshot(state).await
            }
            Err(error) => Err(error),
        },
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a T3 runtime endpoint."
        ))),
    };
    match result {
        Ok(status) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "t3Runtime": status })),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

async fn t3_runtime_status_snapshot(
    state: &Arc<AppState>,
) -> std::result::Result<T3RuntimeStatusPayload, DomainStateError> {
    let manager = state.t3_runtime.clone();
    tokio::task::spawn_blocking(move || manager.status_snapshot())
        .await
        .map_err(t3_runtime_task_error)
}

fn t3_runtime_task_error(error: tokio::task::JoinError) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("T3 runtime task failed: {error}"),
    }
}

fn handle_portless_state_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    /*
    CDXC:PortlessFailureUX 2026-06-23-04:28:
    Native-sidebar reports only enum-like Portless settings/admin outcomes to
    gxserver-rs. The daemon persists setup recovery state, clears route files
    for Disable/remove, and returns refreshed metadata without paths, command
    text, process output, environment values, URLs, tokens, or file contents.
    */
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let update = match serde_json::from_value::<PortlessStateUpdate>(Value::Object(params.clone()))
    {
        Ok(update) => update,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError::bad_request(format!(
                    "Invalid Portless state update payload: {error}"
                )),
            );
        }
    };
    let started_at = Instant::now();
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            log_portless_state_update_failure(
                &state.logger,
                &update,
                PortlessLogErrorCode::StateUpdateDatabaseUnavailable,
                started_at.elapsed().as_millis(),
            );
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
    match apply_portless_state_update(&state.paths, &db, update.clone()) {
        Ok(record) => {
            log_portless_state_update_success(
                &state.logger,
                &update,
                &record,
                started_at.elapsed().as_millis(),
            );
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({
                        "presentation": read_portless_presentation_payload(&db),
                        "status": read_portless_status_payload(&db),
                    }),
                ),
            )
        }
        Err(error) => {
            log_portless_state_update_failure(
                &state.logger,
                &update,
                PortlessLogErrorCode::StateUpdateFailed,
                started_at.elapsed().as_millis(),
            );
            domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("Portless state update failed: {error}"),
                },
            )
        }
    }
}

/*
CDXC:GxserverRustPort 2026-06-14-22:38:
Phase 3 Rust domain endpoints must share the TypeScript RPC envelope, durable SQLite database, and error-status mapping. Keep routing synchronous and explicit here so unsupported lifecycle/provider endpoints still return milestone notImplemented instead of silently mutating partial state.
*/
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

fn create_quick_project_params(
    home_dir: &Path,
    params: &Map<String, Value>,
) -> std::result::Result<Map<String, Value>, DomainStateError> {
    /*
    CDXC:GPUIQuickActions 2026-07-11:
    Quick actions must mirror macOS by creating a real projectless workspace
    under ~/ghostex/chats before creating its first terminal or agent. Keep the
    filesystem authority in gxserver, which already owns the authenticated
    project registry and the user's configured HOME, rather than deriving a
    private home path in the sidebar renderer.
    */
    let kind = params
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|kind| matches!(*kind, "terminal" | "agent"))
        .ok_or_else(|| DomainStateError {
            code: "badRequest",
            message: "kind must be terminal or agent.".to_string(),
        })?;
    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d-%H%M%S%3f");
    let suffix = Uuid::new_v4().simple().to_string();
    let suffix = &suffix[..8];
    let chat_path = home_dir
        .join("ghostex")
        .join("chats")
        .join(format!("{timestamp}-{kind}-{suffix}"));
    fs::create_dir_all(&chat_path).map_err(|_| DomainStateError {
        code: "internalError",
        message: "Unable to create the Quick workspace directory.".to_string(),
    })?;

    let mut launch_settings = Map::new();
    launch_settings.insert("isChat".to_string(), Value::Bool(true));
    launch_settings.insert("isQuick".to_string(), Value::Bool(true));
    launch_settings.insert(
        "quickKind".to_string(),
        Value::String("terminal".to_string()),
    );

    let mut project_params = Map::new();
    project_params.insert(
        "name".to_string(),
        Value::String(now.format("Chat %Y-%m-%d %H:%M").to_string()),
    );
    project_params.insert(
        "path".to_string(),
        Value::String(chat_path.to_string_lossy().to_string()),
    );
    project_params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    Ok(project_params)
}

fn domain_error_response(
    endpoint_path: String,
    request_id: String,
    error: DomainStateError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "notFound" => StatusCode::NOT_FOUND,
        "corruptState" => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

async fn handle_automation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    /*
    CDXC:GxserverAutomations 2026-06-29-15:55:
    Automation RPCs are first-class gxserver endpoints now. Route them through the modular automation runtime instead of `/api/dispatchRendererCommand` so CLI, macOS, and remote clients do not depend on a native sidebar renderer being connected.
    */
    match handle_automation_endpoint(&state.automation_runtime, &endpoint_path, body).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/*
CDXC:GxserverRustPort 2026-06-16-10:00:
Phase 6 agent endpoints share the durable domain repository and authenticated RPC envelope with earlier Rust milestones. Keep zmx fork startup explicit through the selected listener port and schedule presentation deltas only after the session row has been updated.
*/
async fn handle_agent_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
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
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    match dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        &endpoint_path,
        &params,
        Some(&context),
    ) {
        Ok(output) => {
            let presentation_session = output.presentation_session.clone();
            let mut result = output.result;
            let fork_initial_rename = fork_initial_rename_target(&endpoint_path, &result);
            log_agent_hook_passive_identity_conflict(state, &endpoint_path, &params, &result);
            strip_agent_hook_internal_result_fields(&endpoint_path, &mut result);
            let should_queue_agent_title_metadata_check =
                should_schedule_agent_title_metadata_check(&endpoint_path, &result);
            let should_schedule_first_prompt_auto_title =
                result.get("reason").and_then(Value::as_str)
                    == Some("first-prompt-auto-title-claimed");
            if endpoint_path == "/api/updateAgentActivity" {
                if let Some((project_id, session_id)) = presentation_session.as_ref() {
                    let _ = reconcile_agent_metadata_title_for_session(
                        &repository,
                        project_id,
                        session_id,
                        &state.paths.home_dir,
                        "pending",
                    );
                }
            }
            if let Some((project_id, session_id)) = presentation_session.as_ref() {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                ) {
                    return domain_error_response(endpoint_path, request_id, error);
                }
                if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
                    schedule_stale_activity_presentation_refresh(
                        state,
                        &session,
                        stale_activity_refresh_reason(&endpoint_path),
                    );
                }
            }
            if presentation_session.is_none()
                && endpoint_path == "/api/ingestTerminalTitleEvent"
                && result_activity(&result) == Some("working")
            {
                if let Some(session) = result.get("session") {
                    schedule_stale_activity_presentation_refresh(
                        state,
                        session,
                        "terminal-title-stale-activity",
                    );
                }
            }
            if should_queue_agent_title_metadata_check {
                if let Some((project_id, session_id)) = presentation_session {
                    schedule_agent_title_metadata_check(state.clone(), project_id, session_id);
                }
            }
            if should_schedule_first_prompt_auto_title {
                if let Some(session) = result.get("session") {
                    if let (Some(project_id), Some(session_id)) = (
                        read_session_text(session, "projectId"),
                        read_session_text(session, "sessionId"),
                    ) {
                        schedule_first_prompt_auto_title_job(state.clone(), project_id, session_id);
                    }
                }
            }
            if let Some(target) = fork_initial_rename {
                schedule_fork_initial_rename(state.clone(), target);
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => agent_error_response(endpoint_path, request_id, error),
    }
}

fn schedule_agent_title_metadata_check(state: AppState, project_id: String, session_id: String) {
    /*
    CDXC:GxserverAgentTitles 2026-06-21-15:35:
    Agent CLI renames are accepted asynchronously after Ghostex submits `/rename`. Match TypeScript gxserver's three-second trailing metadata check so Rust promotes the Codex session-index title and broadcasts a presentation delta after the CLI writes the canonical thread name.
    */
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS,
        ))
        .await;
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(changed) = reconcile_agent_metadata_title_for_session(
            &repository,
            &project_id,
            &session_id,
            &state.paths.home_dir,
            "metadata-mismatch",
        ) else {
            return;
        };
        if changed {
            let _ = schedule_presentation_session_delta(
                &state,
                &db,
                &repository,
                &project_id,
                &session_id,
            );
        }
    });
}

fn schedule_agent_title_metadata_checks_for_sessions(state: &AppState, sessions: &[Value]) {
    for session in sessions {
        if !should_check_agent_metadata_title_for_project_status(session) {
            continue;
        }
        let Some(project_id) = read_session_text(session, "projectId") else {
            continue;
        };
        let Some(session_id) = read_session_text(session, "sessionId") else {
            continue;
        };
        schedule_agent_title_metadata_check(state.clone(), project_id, session_id);
    }
}

fn should_check_agent_metadata_title_for_project_status(session: &Value) -> bool {
    if !is_agent_associated_session_for_project_status(session) {
        return false;
    }
    if read_runtime_text(session, "pendingAgentTitleRequestStatus").as_deref() == Some("pending") {
        return true;
    }
    read_runtime_text(session, "titleMetadataSource").as_deref() != Some("agent-metadata")
        && trusted_resume_title_for_project_status(session).is_none()
}

fn is_agent_associated_session_for_project_status(session: &Value) -> bool {
    read_session_text(session, "kind").as_deref() == Some("agent")
        || read_session_text(session, "agentId").is_some()
        || read_runtime_text(session, "agentName").is_some()
        || read_runtime_text(session, "agentId").is_some()
        || read_runtime_text(session, "agentSessionId").is_some()
        || read_runtime_text(session, "agentSessionPath").is_some()
}

fn trusted_resume_title_for_project_status(session: &Value) -> Option<String> {
    let title = read_session_text(session, "title")?;
    let title_source = normalize_project_status_title_source(
        read_runtime_text(session, "titleSource")
            .or_else(|| read_runtime_text(session, "restoreTitleSource"))
            .as_deref(),
        &title,
    );
    if title_source == "placeholder" {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_project_status_resume_title(&visible)).then_some(visible)
}

fn normalize_project_status_title_source(value: Option<&str>, title: &str) -> &'static str {
    match value {
        Some("browser-auto") => "browser-auto",
        Some("generated") => "generated",
        Some("placeholder") => "placeholder",
        Some("terminal-auto") => "terminal-auto",
        Some("user") => "user",
        _ if is_temporary_project_status_title(title) => "placeholder",
        _ => "user",
    }
}

fn is_rejected_project_status_resume_title(title: &str) -> bool {
    let normalized = title.trim();
    let lower = normalized.to_ascii_lowercase();
    normalized == "ð^ß^Ñ»"
        || is_temporary_project_status_title(normalized)
        || normalized.starts_with('ð') && normalized.ends_with('»')
        || is_gxserver_session_id(normalized)
        || normalized.chars().any(char::is_control)
        || lower.starts_with("codex ")
        || lower.starts_with("claude ")
        || lower.starts_with("cursor-agent ")
        || lower.starts_with("opencode ")
}

fn is_temporary_project_status_title(title: &str) -> bool {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .eq_ignore_ascii_case("search by text")
}

#[derive(Clone)]
struct ForkInitialRenameTarget {
    agent_name: String,
    project_id: String,
    session_id: String,
    title: String,
}

fn fork_initial_rename_target(
    endpoint_path: &str,
    result: &Value,
) -> Option<ForkInitialRenameTarget> {
    if endpoint_path != "/api/forkSession" {
        return None;
    }
    let fork = result.get("fork")?;
    let session = fork.get("session")?;
    Some(ForkInitialRenameTarget {
        agent_name: fork
            .get("plan")
            .and_then(|plan| plan.get("agentId"))
            .and_then(Value::as_str)
            .or_else(|| session.get("agentId").and_then(Value::as_str))?
            .trim()
            .to_string(),
        project_id: read_session_text(session, "projectId")?,
        session_id: read_session_text(session, "sessionId")?,
        title: read_session_text(session, "title")?,
    })
}

fn schedule_fork_initial_rename(state: AppState, target: ForkInitialRenameTarget) {
    /*
    CDXC:GxserverForkTitles 2026-07-11:
    Fork provider startup already owns the resumed CLI process. Give its prompt
    editor the same four-second readiness window used by automated agent
    prompts, then submit the provisional `Fork: <old title>` through zmx's
    separate text/Enter path. Pi uses `/name`; Codex and Claude use `/rename`.
    If the user has already sent the fork's first prompt, its generated-title
    job wins and this provisional rename is skipped.
    */
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            GXSERVER_FORK_INITIAL_RENAME_READY_DELAY_MS,
        ))
        .await;
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(Some(session)) = repository.get_session(&target.project_id, &target.session_id)
        else {
            return;
        };
        if read_runtime_text(&session, "gxserverForkInitialRenameStatus").as_deref()
            != Some("pending")
            || read_runtime_text(&session, "gxserverFirstPromptAutoTitleStatus").is_some()
        {
            return;
        }
        let command = if normalize_agent_name(Some(&target.agent_name)).as_deref() == Some("pi") {
            format!("/name {}", target.title)
        } else {
            format!("/rename {}", target.title)
        };
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(target.project_id.clone()));
        params.insert("sessionId".to_string(), json!(target.session_id.clone()));
        params.insert("submit".to_string(), Value::Bool(true));
        params.insert("text".to_string(), Value::String(command));
        let status = if dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionMessage",
            &params,
        )
        .is_ok()
        {
            "applied"
        } else {
            "failed"
        };
        let Ok(Some(latest_session)) =
            repository.get_session(&target.project_id, &target.session_id)
        else {
            return;
        };
        let mut runtime_settings = latest_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert("gxserverForkInitialRenameStatus".to_string(), json!(status));
        runtime_settings.insert(
            "gxserverForkInitialRenameUpdatedAt".to_string(),
            json!(now_iso()),
        );
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(target.project_id.clone()));
        update.insert("sessionId".to_string(), json!(target.session_id.clone()));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let _ = repository.update_session(&update);
        schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
    });
}

fn schedule_first_prompt_auto_title_job(state: AppState, project_id: String, session_id: String) {
    /*
    CDXC:GxserverSessionTitle 2026-06-21-19:26:
    Rust gxserver-rs must finish the same first-prompt auto-title flow as TypeScript gxserver after hooks claim a job: decide eligibility centrally, generate or stage the provider rename command, and persist applied/skipped/failed status.

    CDXC:GxserverSessionTitle 2026-07-02-15:10:
    gxserver submits the staged rename command itself with a separate zmx `\r` write instead of asking clients to send a native Enter on the running→applied presentation transition. Client-side submission only worked for currently visible native panes: `sessions[sessionId]` has no Ghostty surface for background/automation-started sessions, so their staged `/rename` sat unsubmitted in the agent composer forever. A separate PTY-level CR after a settle delay is a real Enter keypress to agent prompt editors (a CR appended to the same text payload is treated as a pasted newline), works for invisible panes, remote daemons, and GPUI, and removes the fragile transition-observation race entirely.
    */
    tokio::spawn(async move {
        if let Err(()) =
            run_first_prompt_auto_title_job(state.clone(), project_id.clone(), session_id.clone())
                .await
        {
            mark_first_prompt_auto_title_failed(&state, &project_id, &session_id);
        }
    });
}

#[derive(Clone)]
struct FirstPromptAutoTitleDecision {
    normalized_prompt: Option<String>,
    reason: String,
    should_run: bool,
    strategy: Option<&'static str>,
}

async fn run_first_prompt_auto_title_job(
    state: AppState,
    project_id: String,
    session_id: String,
) -> Result<(), ()> {
    let (project_path, session, prompt, decision) = {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        let Some(project) = repository.get_project(&project_id).map_err(|_| ())? else {
            return Ok(());
        };
        let prompt = read_runtime_text(&session, "firstUserMessage");
        let decision = decide_first_prompt_auto_title(&session, prompt.as_deref(), true);
        (
            read_session_text(&project, "path")
                .unwrap_or_else(|| state.paths.home_dir.to_string_lossy().to_string()),
            session,
            prompt,
            decision,
        )
    };

    if !decision.should_run || decision.normalized_prompt.is_none() || decision.strategy.is_none() {
        mark_first_prompt_auto_title_skipped(&state, &project_id, &session_id, &decision.reason);
        schedule_delta_for_ids(&state, &project_id, &session_id);
        return Ok(());
    }

    let title = if matches!(
        decision.strategy,
        Some("generateTitleAndRename" | "generateTitleAndName")
    ) {
        Some(
            generate_first_prompt_session_title(
                &state,
                Some(&project_path),
                decision.normalized_prompt.as_deref().ok_or(())?,
                GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
                &session,
            )
            .await
            .map_err(|_| ())?,
        )
    } else {
        None
    };

    let command_text = match decision.strategy {
        Some("sendBareRenameCommand") => "/rename".to_string(),
        Some("generateTitleAndName") => format!("/name {}", title.as_deref().ok_or(())?),
        _ => format!("/rename {}", title.as_deref().ok_or(())?),
    };
    {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(latest_session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        if read_runtime_text(&latest_session, "gxserverFirstPromptAutoTitleStatus").as_deref()
            == Some("cancelled")
        {
            schedule_delta_for_ids(&state, &project_id, &session_id);
            return Ok(());
        }
        if read_runtime_text(&latest_session, "gxserverFirstPromptAutoTitleStatus").as_deref()
            != Some("running")
            || normalize_first_prompt_title_prompt(
                read_runtime_text(&latest_session, "firstUserMessage").as_deref(),
            ) != decision.normalized_prompt
        {
            return Ok(());
        }
        let mut send_params = Map::new();
        send_params.insert("projectId".to_string(), json!(project_id.clone()));
        send_params.insert("sessionId".to_string(), json!(session_id.clone()));
        send_params.insert("text".to_string(), json!(command_text));
        dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionText",
            &send_params,
        )
        .map_err(|_| ())?;
    }

    /*
    CDXC:GxserverSessionTitle 2026-07-02-15:10:
    Submit the staged command with a separate zmx `\r` write after a settle delay so agent prompt editors read it as a real Enter keypress rather than part of a pasted payload. The database handle is reopened around the sleep because rusqlite connections cannot be held across await points, and the running-status/prompt re-check keeps an Escape cancellation inside the delay from submitting the staged command.
    */
    tokio::time::sleep(Duration::from_millis(
        GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS,
    ))
    .await;

    let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(latest_session) = repository
        .get_session(&project_id, &session_id)
        .map_err(|_| ())?
    else {
        return Ok(());
    };
    if read_runtime_text(&latest_session, "gxserverFirstPromptAutoTitleStatus").as_deref()
        == Some("cancelled")
    {
        schedule_delta_for_ids(&state, &project_id, &session_id);
        return Ok(());
    }
    if read_runtime_text(&latest_session, "gxserverFirstPromptAutoTitleStatus").as_deref()
        != Some("running")
        || normalize_first_prompt_title_prompt(
            read_runtime_text(&latest_session, "firstUserMessage").as_deref(),
        ) != decision.normalized_prompt
    {
        return Ok(());
    }
    let mut enter_params = Map::new();
    enter_params.insert("projectId".to_string(), json!(project_id.clone()));
    enter_params.insert("sessionId".to_string(), json!(session_id.clone()));
    dispatch_zmx_session_interaction_endpoint(&repository, "/api/sendSessionEnter", &enter_params)
        .map_err(|_| ())?;

    let mut runtime_settings = latest_session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.remove("forkFirstPromptAutoTitlePending");
    runtime_settings.remove("gxserverForkInitialRenameStatus");
    runtime_settings.remove("gxserverForkInitialRenameUpdatedAt");
    runtime_settings.insert("autoTitleFromFirstPrompt".to_string(), Value::Bool(true));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleAppliedAt".to_string(),
        json!(now_iso()),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleReason".to_string(),
        json!(decision.reason),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("applied"),
    );
    if title.is_some() {
        runtime_settings.insert("titleSource".to_string(), json!("generated"));
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id.clone()));
    update.insert("sessionId".to_string(), json!(session_id.clone()));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if let Some(title) = title {
        update.insert("title".to_string(), json!(title));
    }
    repository.update_session(&update).map_err(|_| ())?;
    schedule_delta_for_ids(&state, &project_id, &session_id);
    let _ = prompt;
    Ok(())
}

fn mark_first_prompt_auto_title_skipped(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    reason: &str,
) {
    update_first_prompt_auto_title_runtime(state, project_id, session_id, |runtime| {
        runtime.insert(
            "gxserverFirstPromptAutoTitleReason".to_string(),
            json!(reason),
        );
        runtime.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("skipped"),
        );
    });
}

fn mark_first_prompt_auto_title_failed(state: &AppState, project_id: &str, session_id: &str) {
    update_first_prompt_auto_title_runtime(state, project_id, session_id, |runtime| {
        runtime.insert(
            "gxserverFirstPromptAutoTitleFailedAt".to_string(),
            json!(now_iso()),
        );
        runtime.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("failed"),
        );
    });
    schedule_delta_for_ids(state, project_id, session_id);
}

fn update_first_prompt_auto_title_runtime<F>(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    apply: F,
) where
    F: FnOnce(&mut Map<String, Value>),
{
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    apply(&mut runtime_settings);
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    let _ = repository.update_session(&update);
}

fn schedule_delta_for_ids(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

/*
CDXC:ManualSessionTitleGeneration 2026-07-29:
Rename-modal "Generate Name" reuses the first-prompt auto-title machinery for
an existing session: the same generation agent command summarizes the pasted
text into a short title, the same `gxserverFirstPromptAutoTitleStatus:
"running"` state drives the session card's generating chrome, and the same
staged zmx command text plus delayed Enter renames the Agent CLI thread. The
manual path intentionally skips first-prompt eligibility gates (the user asked
explicitly), kills any composer draft with Ctrl+U before staging and restores
it with Ctrl+Y after the submit, and applies the generated title with
`titleSource: "generated"`.
*/
async fn handle_generate_session_title_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "generateSessionTitle requires projectId and sessionId.".to_string(),
            },
        );
    }
    let generation_agent = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let generation_command = params
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    {
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
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = match repository.get_session(&project_id, &session_id) {
            Ok(Some(session)) => session,
            Ok(None) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "notFound",
                        message: "The session no longer exists.".to_string(),
                    },
                );
            }
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
        /*
        CDXC:SessionHistoryTitleSource 2026-07-29:
        An empty `text` asks the job to summarize the session's recent
        transcript user prompts. Only agents with a known local transcript
        format support that, so other agents keep requiring pasted text.
        */
        if text.is_empty() {
            let session_agent = normalize_agent_name(first_prompt_agent_name(&session).as_deref());
            if !crate::agent_transcripts::agent_supports_session_history_title_source(
                session_agent.as_deref(),
            ) {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message:
                            "generateSessionTitle requires text for this agent; only Claude Code, Codex, and Cursor CLI sessions can generate from recent messages."
                                .to_string(),
                    },
                );
            }
        }
        let mut runtime_settings = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("running"),
        );
        runtime_settings.insert(
            "gxserverManualTitleGenerationRequestedAt".to_string(),
            json!(now_iso()),
        );
        if let Some(agent) = generation_agent.as_deref() {
            runtime_settings.insert("firstPromptTitleGenerationAgent".to_string(), json!(agent));
        }
        if let Some(command) = generation_command.as_deref() {
            runtime_settings.insert(
                "firstPromptTitleGenerationCommand".to_string(),
                json!(command),
            );
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id.clone()));
        update.insert("sessionId".to_string(), json!(session_id.clone()));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        if let Err(error) = repository.update_session(&update) {
            return domain_error_response(endpoint_path, request_id, error);
        }
    }
    schedule_delta_for_ids(state, &project_id, &session_id);
    let job_state = state.clone();
    let job_project_id = project_id.clone();
    let job_session_id = session_id.clone();
    tokio::spawn(async move {
        if let Err(()) = run_manual_session_title_generation_job(
            job_state.clone(),
            job_project_id.clone(),
            job_session_id.clone(),
            text,
        )
        .await
        {
            mark_first_prompt_auto_title_failed(&job_state, &job_project_id, &job_session_id);
        }
    });
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "started": true })),
    )
}

async fn run_manual_session_title_generation_job(
    state: AppState,
    project_id: String,
    session_id: String,
    text: String,
) -> Result<(), ()> {
    let (project_path, session) = {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        let Some(project) = repository.get_project(&project_id).map_err(|_| ())? else {
            return Ok(());
        };
        (
            read_session_text(&project, "path")
                .unwrap_or_else(|| state.paths.home_dir.to_string_lossy().to_string()),
            session,
        )
    };
    if read_runtime_text(&session, "gxserverFirstPromptAutoTitleStatus").as_deref()
        != Some("running")
    {
        return Ok(());
    }
    /*
    CDXC:SessionHistoryTitleSource 2026-07-29:
    Empty text means "name this session from what the user recently asked it".
    Resolve the provider transcript via the hook-captured session identity and
    summarize the last few visible user prompts; failing to find any is a real
    failure so the card's generating state resolves instead of hanging.
    */
    let (source_text, source_max_length) = if text.trim().is_empty() {
        let Some(source) = session_history_title_source(&session) else {
            return Err(());
        };
        (source, GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MAX_LENGTH)
    } else {
        (text, GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH)
    };
    let title = generate_first_prompt_session_title(
        &state,
        Some(&project_path),
        &source_text,
        source_max_length,
        &session,
    )
    .await
    .map_err(|_| ())?;
    let session_agent = session
        .get("agentId")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase());
    let command_text = if session_agent.as_deref() == Some("pi") {
        format!("/name {title}")
    } else {
        format!("/rename {title}")
    };
    {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(latest_session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        if read_runtime_text(&latest_session, "gxserverFirstPromptAutoTitleStatus").as_deref()
            != Some("running")
        {
            return Ok(());
        }
        // Kill any in-progress composer draft, then stage the rename command.
        let mut kill_params = Map::new();
        kill_params.insert("projectId".to_string(), json!(project_id.clone()));
        kill_params.insert("sessionId".to_string(), json!(session_id.clone()));
        kill_params.insert("text".to_string(), json!("\u{15}"));
        dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionText",
            &kill_params,
        )
        .map_err(|_| ())?;
        let mut send_params = Map::new();
        send_params.insert("projectId".to_string(), json!(project_id.clone()));
        send_params.insert("sessionId".to_string(), json!(session_id.clone()));
        send_params.insert("text".to_string(), json!(command_text));
        dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionText",
            &send_params,
        )
        .map_err(|_| ())?;
    }
    tokio::time::sleep(Duration::from_millis(
        GXSERVER_FIRST_PROMPT_STAGED_COMMAND_SUBMIT_DELAY_MS,
    ))
    .await;
    {
        let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(latest_session) = repository
            .get_session(&project_id, &session_id)
            .map_err(|_| ())?
        else {
            return Ok(());
        };
        if read_runtime_text(&latest_session, "gxserverFirstPromptAutoTitleStatus").as_deref()
            != Some("running")
        {
            return Ok(());
        }
        let mut enter_params = Map::new();
        enter_params.insert("projectId".to_string(), json!(project_id.clone()));
        enter_params.insert("sessionId".to_string(), json!(session_id.clone()));
        dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionEnter",
            &enter_params,
        )
        .map_err(|_| ())?;
        let mut runtime_settings = latest_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleAppliedAt".to_string(),
            json!(now_iso()),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleReason".to_string(),
            json!("manual-generate-name"),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("applied"),
        );
        runtime_settings.insert("titleSource".to_string(), json!("generated"));
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id.clone()));
        update.insert("sessionId".to_string(), json!(session_id.clone()));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        update.insert("title".to_string(), json!(title));
        repository.update_session(&update).map_err(|_| ())?;
    }
    schedule_delta_for_ids(&state, &project_id, &session_id);
    // Restore the killed draft once the submit has settled in the CLI.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let db = open_gxserver_database(&state.paths).map_err(|_| ())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut yank_params = Map::new();
    yank_params.insert("projectId".to_string(), json!(project_id.clone()));
    yank_params.insert("sessionId".to_string(), json!(session_id.clone()));
    yank_params.insert("text".to_string(), json!("\u{19}"));
    let _ = dispatch_zmx_session_interaction_endpoint(
        &repository,
        "/api/sendSessionText",
        &yank_params,
    );
    Ok(())
}

fn decide_first_prompt_auto_title(
    session: &Value,
    prompt: Option<&str>,
    allow_running: bool,
) -> FirstPromptAutoTitleDecision {
    let status = read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus");
    let fork_first_prompt_rearmed = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("forkFirstPromptAutoTitlePending"))
        .and_then(Value::as_bool)
        == Some(true);
    let raw_prompt = prompt;
    let normalized_prompt = normalize_first_prompt_title_prompt(prompt);
    let cancelled_prompt = normalize_first_prompt_title_prompt(
        read_runtime_text(session, "gxserverFirstPromptAutoTitleCancelledPrompt").as_deref(),
    )
    .or_else(|| {
        normalize_first_prompt_title_prompt(
            read_runtime_text(session, "firstUserMessage").as_deref(),
        )
    });
    let is_cancelled_retry_prompt = status.as_deref() == Some("cancelled")
        && normalized_prompt.is_some()
        && normalized_prompt != cancelled_prompt;
    if (status.as_deref() == Some("running") && !allow_running)
        || matches!(status.as_deref(), Some("applied" | "failed" | "skipped"))
        || (status.as_deref() == Some("cancelled") && !is_cancelled_retry_prompt)
    {
        return FirstPromptAutoTitleDecision {
            normalized_prompt,
            reason: format!("already-{}", status.unwrap_or_default()),
            should_run: false,
            strategy: None,
        };
    }
    if !fork_first_prompt_rearmed
        && session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("autoTitleFromFirstPrompt"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        return decision(normalized_prompt, "alreadyAutoNamed", false, None);
    }
    let agent_name = first_prompt_agent_name(session);
    let strategy = first_prompt_auto_title_strategy(agent_name.as_deref());
    if strategy.is_none() {
        return decision(normalized_prompt, "unsupportedAgent", false, None);
    }
    let Some(prompt) = normalized_prompt.clone() else {
        return decision(normalized_prompt, "emptyPrompt", false, strategy);
    };
    if is_first_prompt_meta_prompt(&prompt) {
        return decision(Some(prompt), "metaPrompt", false, strategy);
    }
    if is_first_prompt_slash_command(raw_prompt, &prompt) {
        return decision(Some(prompt), "slashCommand", false, strategy);
    }
    if !fork_first_prompt_rearmed
        && !is_generic_agent_session_title(
            agent_name.as_deref(),
            read_session_text(session, "title").as_deref(),
        )
    {
        return decision(Some(prompt), "nonGenericCurrentTitle", false, strategy);
    }
    decision(Some(prompt), "eligible", true, strategy)
}

fn decision(
    normalized_prompt: Option<String>,
    reason: &str,
    should_run: bool,
    strategy: Option<&'static str>,
) -> FirstPromptAutoTitleDecision {
    FirstPromptAutoTitleDecision {
        normalized_prompt,
        reason: reason.to_string(),
        should_run,
        strategy,
    }
}

fn first_prompt_agent_name(session: &Value) -> Option<String> {
    read_session_text(session, "agentId").or_else(|| read_runtime_text(session, "agentName"))
}

fn first_prompt_auto_title_strategy(agent_name: Option<&str>) -> Option<&'static str> {
    match normalize_agent_name(agent_name).as_deref() {
        Some("claude") => Some("sendBareRenameCommand"),
        Some("codex") => Some("generateTitleAndRename"),
        Some("pi") => Some("generateTitleAndName"),
        _ => None,
    }
}

fn normalize_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "openai codex" | "codex cli" => Some("codex".to_string()),
        "claude code" => Some("claude".to_string()),
        "cursor cli" | "cursor agent" | "cursor-agent" => Some("cursor".to_string()),
        "π" => Some("pi".to_string()),
        other => Some(other.to_string()),
    }
}

fn is_generic_agent_session_title(agent_name: Option<&str>, title: Option<&str>) -> bool {
    let normalized_title = title
        .map(|value| {
            value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if normalized_title.is_empty() {
        return true;
    }
    let normalized_agent = normalize_agent_name(agent_name);
    let generic = [
        "terminal",
        "terminal session",
        "agent",
        "agent session",
        "claude",
        "claude code",
        "claude session",
        "codex",
        "codex cli",
        "codex session",
        "openai codex",
        "openai codex session",
        "pi",
        "π",
        "pi session",
    ];
    generic.contains(&normalized_title.as_str())
        || normalized_agent.as_deref() == Some(normalized_title.as_str())
}

fn normalize_first_prompt_title_prompt(prompt: Option<&str>) -> Option<String> {
    let normalized = prompt?.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let stripped = strip_first_prompt_title_prefixes(normalized);
    let cleaned = stripped
        .trim()
        .trim_end_matches(['.', '?', '!', ':', ';', ','])
        .trim();
    Some(
        if cleaned.is_empty() {
            normalized
        } else {
            cleaned
        }
        .to_string(),
    )
}

/*
CDXC:GxserverSessionTitle 2026-06-22-08:12:
First-prompt title eligibility must be decided before Rust claims a background job, using the same prompt-normalization and slash-command rules as TypeScript gxserver. Repeated polite prefixes are stripped only for title generation, while slash-command suppression scans the original prompt by line so short command prompts never enter the title job.
*/
fn strip_first_prompt_title_prefixes(value: &str) -> &str {
    let mut stripped = value;
    loop {
        let lower = stripped.to_lowercase();
        let prefix = [
            "please ",
            "kindly ",
            "hey ",
            "hi ",
            "hello ",
            "can you ",
            "could you ",
            "would you ",
            "will you ",
            "can we ",
            "could we ",
            "would we ",
            "help me ",
            "i need you to ",
            "i need to ",
            "i need ",
            "how do i ",
            "how does ",
            "is there any way to ",
            "is there way to ",
        ]
        .into_iter()
        .find(|prefix| lower.starts_with(prefix));
        let Some(prefix) = prefix else {
            return stripped;
        };
        stripped = &stripped[prefix.len()..];
    }
}

fn is_first_prompt_slash_command(raw_prompt: Option<&str>, normalized_prompt: &str) -> bool {
    if js_string_length(normalized_prompt) > 50 {
        return false;
    }
    let Some(raw_prompt) = raw_prompt else {
        return false;
    };
    raw_prompt
        .split('\n')
        .any(is_first_prompt_slash_command_line)
}

fn is_first_prompt_slash_command_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut consumed_bytes = first.len_utf8();
    for (index, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            consumed_bytes = index + ch.len_utf8();
            continue;
        }
        consumed_bytes = index;
        break;
    }
    let suffix = &rest[consumed_bytes..];
    suffix
        .chars()
        .next()
        .map(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ')' | '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '`'
                )
        })
        .unwrap_or(true)
}

fn is_first_prompt_meta_prompt(prompt: &str) -> bool {
    prompt.starts_with("# AGENTS")
        || prompt.contains("tool_use_id")
        || [
            "<command",
            "<environment_context",
            "<permissions instructions>",
            "<user_instructions>",
            "<INSTRUCTIONS>",
            "<collaboration_mode>",
            "<app-context>",
            "<turn_aborted>",
            "<ide_opened_file>",
            "<local-",
            "[Tool Result]",
            "Caveat:",
        ]
        .iter()
        .any(|prefix| prompt.starts_with(prefix))
}

fn session_history_title_source(session: &Value) -> Option<String> {
    let agent = normalize_agent_name(first_prompt_agent_name(session).as_deref())?;
    if !crate::agent_transcripts::agent_supports_session_history_title_source(Some(agent.as_str()))
    {
        return None;
    }
    let prompts = crate::agent_transcripts::recent_session_user_prompts(
        &agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    );
    build_session_history_title_source(&prompts)
}

fn build_session_history_title_source(prompts: &[String]) -> Option<String> {
    let mut recent: Vec<String> = prompts
        .iter()
        .rev()
        .take(GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_COUNT)
        .map(|prompt| {
            js_string_slice_prefix(
                prompt,
                GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_MAX_LENGTH,
            )
            .trim()
            .to_string()
        })
        .collect();
    recent.reverse();
    let joined = recent.join("\n\n");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

async fn generate_first_prompt_session_title(
    state: &AppState,
    cwd: Option<&str>,
    prompt: &str,
    source_max_length: usize,
    session: &Value,
) -> Result<String, String> {
    let source_text = js_string_slice_prefix(prompt, source_max_length);
    let generation_prompt = build_first_prompt_title_generation_prompt(&source_text);
    let delimiter = format!(
        "ghostex_GXSERVER_SESSION_TITLE_{}",
        chrono::Utc::now().timestamp_millis()
    );
    let agent = normalize_title_generation_agent(
        read_runtime_text(session, "firstPromptTitleGenerationAgent").as_deref(),
    );
    let command = read_title_generation_command(session, &agent)?;
    let shell_command =
        build_title_generation_command(&agent, &command, &delimiter, &generation_prompt)?;
    let shell = command_shell();
    let mut child = Command::new(&shell.executable);
    child.args(shell.interactive_script_args(&shell_command));
    child.current_dir(cwd.unwrap_or_else(|| state.paths.home_dir.to_str().unwrap_or(".")));
    child.envs(internal_prompt_generation_environment(
        &state.paths.home_dir,
    ));
    child.stdout(std::process::Stdio::piped());
    child.stderr(std::process::Stdio::piped());
    let output = tokio::time::timeout(
        Duration::from_millis(GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS),
        child.output(),
    )
    .await
    .map_err(|_| "title generation timed out".to_string())?
    .map_err(|error| format!("title generation failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "title generation exited {:?}",
            output.status.code()
        ));
    }
    parse_generated_session_title_text(&String::from_utf8_lossy(&output.stdout))
}

fn normalize_title_generation_agent(value: Option<&str>) -> String {
    match value {
        Some("cursor" | "claude" | "grok" | "custom") => value.unwrap().to_string(),
        _ => "codex".to_string(),
    }
}

fn read_title_generation_command(session: &Value, agent: &str) -> Result<String, String> {
    if let Some(command) = read_runtime_text(session, "firstPromptTitleGenerationCommand") {
        return Ok(command);
    }
    match agent {
        "codex" => Ok("codex".to_string()),
        "cursor" => Ok("cursor-agent".to_string()),
        "claude" => Ok("claude".to_string()),
        "grok" => Ok("grok".to_string()),
        "custom" => Err("Custom title generation command is not configured.".to_string()),
        _ => Ok("codex".to_string()),
    }
}

fn build_title_generation_command(
    agent: &str,
    command: &str,
    delimiter: &str,
    prompt: &str,
) -> Result<String, String> {
    Ok(match agent {
        "codex" => create_here_doc_command(
            &format!(
                "{command} exec --ephemeral --skip-git-repo-check -m gpt-5.4-mini -c 'model_reasoning_effort=\"low\"'"
            ),
            delimiter,
            prompt,
        ),
        "cursor" => format!(
            "{command} --print --yolo --trust --output-format text {}",
            quote_shell_arg(prompt)
        ),
        "claude" => {
            create_here_doc_command(&format!("{command} -p --model haiku"), delimiter, prompt)
        }
        "grok" => format!(
            "{command} -p --model grok-composer-2.5-fast --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 {}",
            quote_shell_arg(prompt)
        ),
        "custom" => create_here_doc_command(command, delimiter, prompt),
        other => return Err(format!("Unsupported title generation agent: {other}")),
    })
}

fn create_here_doc_command(command: &str, delimiter: &str, body: &str) -> String {
    format!("{command} <<'{delimiter}'\n{body}\n{delimiter}")
}

fn build_first_prompt_title_generation_prompt(source_text: &str) -> String {
    [
        "Write a concise session title that summarizes the user's text.",
        "Return plain text only.",
        "Rules:",
        "- keep it specific and scannable",
        "- prefer 2 to 4 words when possible",
        &format!(
            "- must be fewer than {} characters",
            GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH + 1
        ),
        "- do not abbreviate with ellipses",
        "- do not use quotes, markdown, or commentary",
        "- do not end with punctuation",
        "- focus on the task, bug, feature, or topic",
        "",
        "User text:",
        source_text,
        "",
        "Output handling:",
        "- Produce only the final session title.",
        "- Do not wrap the result in backticks.",
        "- Print only the final result to stdout.",
    ]
    .join("\n")
}

fn parse_generated_session_title_text(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let normalized = if trimmed.starts_with("```") && trimmed.ends_with("```") {
        trimmed
            .trim_start_matches('`')
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches('`')
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    let Some(line) = normalized.lines().find(|line| !line.trim().is_empty()) else {
        return Err("Title generation returned an empty session title.".to_string());
    };
    let sanitized = line
        .trim()
        .trim_matches(['"', '\'', '`'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '…'])
        .trim()
        .to_string();
    if sanitized.is_empty() {
        return Err("Title generation returned an empty session title.".to_string());
    }
    Ok(clamp_generated_session_title_length(&sanitized))
}

fn clamp_generated_session_title_length(value: &str) -> String {
    if js_string_length(value) <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH {
        return value.to_string();
    }
    let mut candidate = String::new();
    for word in value.split_whitespace() {
        let next = if candidate.is_empty() {
            word.to_string()
        } else {
            format!("{candidate} {word}")
        };
        if js_string_length(&next) > GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH {
            break;
        }
        candidate = next;
    }
    if candidate.is_empty() {
        js_string_slice_prefix(value, GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH)
            .trim()
            .to_string()
    } else {
        candidate
    }
}

/*
CDXC:GxserverSessionTitle 2026-06-22-07:21:
TypeScript title caps use JavaScript string length and slice semantics, so Rust must count UTF-16 code units rather than Unicode scalar values for first-prompt source text and generated session titles. Rust strings cannot store lone surrogate halves; when a JS slice would expose one, use the replacement character that Node writes at the UTF-8 boundary.
*/
fn js_string_length(text: &str) -> usize {
    text.encode_utf16().count()
}

fn js_string_slice_prefix(text: &str, max_code_units: usize) -> String {
    let mut output = String::new();
    let mut code_units = 0usize;
    for ch in text.chars() {
        let width = ch.len_utf16();
        if code_units + width > max_code_units {
            if code_units < max_code_units {
                output.push(char::REPLACEMENT_CHARACTER);
            }
            break;
        }
        output.push(ch);
        code_units += width;
        if code_units == max_code_units {
            break;
        }
    }
    output
}

fn internal_prompt_generation_environment(home_dir: &std::path::Path) -> Vec<(String, String)> {
    /*
    CDXC:GxserverPromptGeneration 2026-06-24-16:11:
    Background title and commit-message generation must not inherit active
    Ghostex session identity. Clear session-binding variables and mark the
    process as internal so installed agent hooks do not attach generated prompt
    runs to user-restorable terminal sessions.
    */
    let mut environment = std::env::vars().collect::<std::collections::HashMap<_, _>>();
    for key in [
        "ANSI_COLORS_DISABLED",
        "NO_COLOR",
        "NODE_DISABLE_COLORS",
        "GHOSTEX_GLOBAL_SESSION_REF",
        "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
        "GHOSTEX_GXSERVER_BASE_URL",
        "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
        "GHOSTEX_SESSION_ID",
        "GHOSTEX_SESSION_STATE_FILE",
        "GHOSTEX_WORKSPACE_ID",
        "GHOSTEX_WORKSPACE_ROOT",
        "VSMUX_SESSION_ID",
        "VSMUX_SESSION_STATE_FILE",
        "VSMUX_WORKSPACE_ID",
        "VSMUX_WORKSPACE_ROOT",
        "ghostex_SESSION_STATE_FILE",
        "ghostex_WORKSPACE_ID",
        "ghostex_WORKSPACE_ROOT",
    ] {
        environment.remove(key);
    }
    environment.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
    environment.insert(
        "GHOSTEX_INTERNAL_PROMPT_GENERATION".to_string(),
        "1".to_string(),
    );
    environment.insert(
        "GHOSTEX_INTERNAL_TITLE_GENERATION".to_string(),
        "1".to_string(),
    );
    environment.into_iter().collect()
}

fn quote_shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn handle_agent_hook_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", error.message, Some(request_id)),
            )
        }
    };
    /*
    CDXC:AgentHooks 2026-06-19-14:15:
    Hook uninstall is routed with read/install through the local authenticated RPC envelope. The handler returns the TypeScript-compatible status payload plus removedPaths without writing provider paths, hook commands, or hook file contents to persistent logs.

    CDXC:AgentHooks 2026-06-22-08:23:
    Area 27 status-code parity keeps malformed hook RPC params on TypeScript gxserver's generic internalError path. File/config hook operation failures still use the shared domain-error mapping after params have been accepted.
    */
    let result = match endpoint_path.as_str() {
        "/api/installAgentHooks" => install_agent_hooks(&state.paths, &params),
        "/api/uninstallAgentHooks" => uninstall_agent_hooks(&state.paths, &params),
        _ => read_agent_hook_status(&state.paths, &params),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/*
CDXC:AgentSkills 2026-06-19-13:59:
Agent skill status/install endpoints are local-only because they inspect or mutate user-local agent skill directories. Route them outside domain-state handlers so install stdout/stderr are returned only in the RPC response and are not written to persistent gxserver logs.
*/
async fn handle_agent_skill_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = if endpoint_path == "/api/installAgentSkills" {
        install_agent_skills(&state.paths, &params).await
    } else {
        read_agent_skill_status(&state.paths, &params)
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn agent_error_response(
    endpoint_path: String,
    request_id: String,
    error: AgentEndpointError,
) -> RoutedResponse {
    match error {
        AgentEndpointError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        AgentEndpointError::DependencyUnavailable(message) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error("dependencyUnavailable", message, Some(request_id)),
        ),
    }
}

/*
CDXC:GxserverRustPort 2026-06-16-00:49:
Phase 7 typed operation endpoints reuse the durable project registry before building allowlisted subprocesses. Keep returned command metadata redacted and persistent logs metadata-only, because argv, cwd, stdout, stderr, branch names, file paths, and setup commands can contain user-owned content.
*/
async fn handle_typed_operation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let projects = {
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
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        match repository.list_projects() {
            Ok(projects) => projects,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        }
    };
    match dispatch_typed_operation_endpoint(&endpoint_path, &params, projects).await {
        Ok(result) => {
            let action = result
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let exit_code = result.get("exitCode").and_then(Value::as_i64).unwrap_or(1) as i32;
            let has_error = result.get("error").is_some();
            let level = if typed_operation_log_level(&action, exit_code, has_error) == "info" {
                crate::logging::LogLevel::Info
            } else {
                crate::logging::LogLevel::Warn
            };
            let _ = state.logger.log(GxserverLogInput {
                level,
                event: "typedOperation".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: Some(request_id.clone()),
                client: None,
                duration_ms: None,
                error: None,
                details: Some(typed_operation_log_details(&result)),
            });
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => {
            if error.scope_rejection {
                /*
                CDXC:ProjectBoardRouting 2026-06-22-10:22:
                Project Board and typed-operation scope misses are support-relevant but private. Match TypeScript by logging only endpoint/action/error identity and presence booleans, never raw project paths, command text, URLs, or user-owned board content.
                */
                let _ = state.logger.log(GxserverLogInput {
                    level: LogLevel::Warn,
                    event: "typedOperation.scopeRejected".to_string(),
                    server_id: Some(state.metadata.server_id.clone()),
                    request_id: Some(request_id.clone()),
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: Some(typed_operation_scope_rejection_details(
                        &endpoint_path,
                        &params,
                        &error,
                    )),
                });
            }
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}

async fn handle_generate_commit_message_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match generate_commit_message_for_project(state, &params).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

async fn handle_create_pull_request_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let projects = {
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
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        match repository.list_projects() {
            Ok(projects) => projects,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        }
    };
    match create_pull_request_for_project(&params, projects).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => typed_operation_error_response(endpoint_path, request_id, error),
    }
}

struct CommitMessageGenerationAgent {
    agent_id: String,
    command: String,
    is_default: bool,
    name: String,
}

/*
CDXC:GPUISidebarGit 2026-06-24-16:11:
GPUI blank commit-message generation is a local gxserver operation over a
registered project, not renderer shell execution. The endpoint stages only the
review-approved project-relative paths, derives branch/diff text from fixed Git
actions, resolves prompt-agent commands from stored gxserver project/settings
state, and returns only parsed subject/body text to the commit pipeline.
*/
async fn generate_commit_message_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let project_id = read_project_id(params)?;
    let requested_file_paths = read_commit_message_generation_file_paths(params)?;
    let (project, project_path, projects, settings) = {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository.get_project(&project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        let project_path = project
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| DomainStateError::bad_request("Project has no filesystem path."))?
            .to_string();
        let projects = repository.list_projects()?;
        let settings = read_agent_settings(&db)?;
        /*
        CDXC:GxserverRustBuild 2026-06-24-20:22:
        Axum requires the HTTP fallback future to be Send. Read commit-message
        generation metadata from SQLite up front and leave the rusqlite-backed
        repository inside this block before awaiting Git actions or agent output.
        */
        (project, project_path, projects, settings)
    };

    let status =
        run_commit_message_generation_git_action(&projects, &project_id, "statusPorcelainZ", None)
            .await?;
    ensure_commit_message_generation_git_success(&status, "Could not inspect selected changes.")?;
    let mut file_paths = retain_current_commit_message_generation_paths(
        &requested_file_paths,
        typed_result_stdout_raw(&status),
    )?;

    let mut add = run_commit_message_generation_git_action(
        &projects,
        &project_id,
        "addAll",
        Some(&file_paths),
    )
    .await?;
    for retry_delay_ms in [40_u64, 120] {
        if commit_message_generation_git_succeeded(&add) {
            break;
        }
        let stderr = add
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !commit_message_generation_stage_failure_is_transient(stderr) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
        let refreshed_status = run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "statusPorcelainZ",
            None,
        )
        .await?;
        ensure_commit_message_generation_git_success(
            &refreshed_status,
            "Could not refresh selected changes before staging.",
        )?;
        file_paths = retain_current_commit_message_generation_paths(
            &file_paths,
            typed_result_stdout_raw(&refreshed_status),
        )?;
        add = run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "addAll",
            Some(&file_paths),
        )
        .await?;
    }
    ensure_commit_message_generation_git_success(
        &add,
        commit_message_generation_stage_failure_message(&add),
    )?;

    let (summary, patch, branch) = tokio::try_join!(
        run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "diffCachedStatFiles",
            Some(&file_paths),
        ),
        run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "diffCachedFiles",
            Some(&file_paths),
        ),
        run_commit_message_generation_git_action(&projects, &project_id, "branch", None),
    )?;
    ensure_commit_message_generation_git_success(
        &summary,
        "Could not inspect selected staged changes.",
    )?;
    ensure_commit_message_generation_git_success(
        &patch,
        "Could not inspect selected staged changes.",
    )?;

    let staged_summary = typed_result_stdout(&summary);
    let staged_patch = typed_result_stdout(&patch);
    if staged_summary.trim().is_empty() && staged_patch.trim().is_empty() {
        return Err(DomainStateError::bad_request(
            "No staged changes are available for commit message generation.",
        ));
    }

    let agent = resolve_commit_message_generation_agent(&project, params, &settings)?;
    let prompt = build_gxserver_commit_message_generation_prompt(
        typed_result_stdout(&branch).trim(),
        read_project_generate_commit_body(&project),
        &staged_summary,
        &staged_patch,
    );
    let stdout = run_commit_message_generation_agent(state, &project_path, &agent, &prompt).await?;
    parse_gxserver_generated_commit_message(&stdout, &agent.name)
}

fn read_commit_message_generation_file_paths(
    params: &Map<String, Value>,
) -> std::result::Result<Vec<String>, DomainStateError> {
    let file_paths = params
        .get("filePaths")
        .and_then(Value::as_array)
        .ok_or_else(|| DomainStateError::bad_request("filePaths must be a non-empty array."))?;
    if file_paths.is_empty() {
        return Err(DomainStateError::bad_request(
            "filePaths must include at least one changed file.",
        ));
    }
    if file_paths.len() > 500 {
        return Err(DomainStateError::bad_request(
            "filePaths exceeds the 500-file limit.",
        ));
    }
    let mut normalized = Vec::with_capacity(file_paths.len());
    for value in file_paths {
        let text = value
            .as_str()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request("filePaths must contain relative paths.")
            })?;
        if text.contains('\0')
            || Path::new(text).is_absolute()
            || text.split(['/', '\\']).any(|part| part == "..")
        {
            return Err(DomainStateError::bad_request(
                "filePaths must contain relative paths inside the project.",
            ));
        }
        let path = text.replace('\\', "/").trim_start_matches('/').to_string();
        if !normalized.contains(&path) {
            normalized.push(path);
        }
    }
    Ok(normalized)
}

async fn run_commit_message_generation_git_action(
    projects: &[Value],
    project_id: &str,
    action: &str,
    file_paths: Option<&[String]>,
) -> std::result::Result<Value, DomainStateError> {
    let mut params = Map::new();
    params.insert("action".to_string(), json!(action));
    params.insert("projectId".to_string(), json!(project_id));
    if let Some(file_paths) = file_paths {
        params.insert(
            "filePaths".to_string(),
            Value::Array(file_paths.iter().map(|path| json!(path)).collect()),
        );
    }
    dispatch_typed_operation_endpoint("/api/runGitAction", &params, projects.to_vec())
        .await
        .map_err(|error| typed_operation_commit_generation_error(error, "Git inspection failed."))
}

fn ensure_commit_message_generation_git_success(
    result: &Value,
    message: &str,
) -> std::result::Result<(), DomainStateError> {
    let exit_code = result.get("exitCode").and_then(Value::as_i64).unwrap_or(1);
    if exit_code == 0 && result.get("error").is_none() {
        return Ok(());
    }
    Err(DomainStateError {
        code: "badRequest",
        message: message.to_string(),
    })
}

fn commit_message_generation_git_succeeded(result: &Value) -> bool {
    result.get("exitCode").and_then(Value::as_i64) == Some(0) && result.get("error").is_none()
}

fn commit_message_generation_stage_failure_is_transient(stderr: &str) -> bool {
    let normalized = stderr.to_ascii_lowercase();
    normalized.contains("index.lock")
        || normalized.contains("another git process")
        || normalized.contains("pathspec")
}

fn commit_message_generation_stage_failure_message(result: &Value) -> &'static str {
    let stderr = result
        .get("stderr")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if stderr.contains("index.lock") || stderr.contains("another git process") {
        "Could not stage selected changes because another Git operation is still running."
    } else if stderr.contains("pathspec") {
        "Could not stage selected changes because reviewed files changed again."
    } else {
        "Could not stage selected changes."
    }
}

fn typed_operation_commit_generation_error(
    error: TypedOperationError,
    message: &str,
) -> DomainStateError {
    DomainStateError {
        code: error.code,
        message: message.to_string(),
    }
}

fn typed_result_stdout(result: &Value) -> String {
    result
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn typed_result_stdout_raw(result: &Value) -> &str {
    result
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/*
CDXC:GPUISidebarGit 2026-06-24-16:11:
The generation endpoint must re-derive the current changed-file set from
gxserver-owned Git status before staging. The renderer's review request chooses
from that set, but Rust rejects stale or arbitrary file paths at the writer
boundary so prompt generation cannot inspect unrelated project content.
*/
fn retain_current_commit_message_generation_paths(
    file_paths: &[String],
    status_stdout: &str,
) -> std::result::Result<Vec<String>, DomainStateError> {
    /*
    CDXC:GPUISidebarGit 2026-07-11-06:23:
    A commit review is path-trusted when the modal opens, but other agents can
    finish or replace one of those files before the user confirms. Keep only
    reviewed paths that are still changed at the authoritative gxserver check;
    never add newly changed paths and never allow an arbitrary requested path
    through. Reject only when the entire reviewed selection is now stale.
    */
    let changed_paths = parse_commit_message_generation_status_paths(status_stdout);
    let current_paths = file_paths
        .iter()
        .filter(|path| changed_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    if !current_paths.is_empty() {
        return Ok(current_paths);
    }
    Err(DomainStateError::bad_request(
        "None of the selected files are still part of the current Git review.",
    ))
}

fn parse_commit_message_generation_status_paths(status_stdout: &str) -> HashSet<String> {
    let mut changed_paths = HashSet::new();
    let mut entries = status_stdout.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let status = &entry[..2];
        if let Some(path) = normalize_commit_message_generation_status_path(&entry[3..]) {
            changed_paths.insert(path);
        }
        if status.contains('R') || status.contains('C') {
            let _ = entries.next();
        }
    }
    changed_paths
}

fn normalize_commit_message_generation_status_path(path: &str) -> Option<String> {
    let text = path.trim();
    if text.is_empty()
        || text.contains('\0')
        || Path::new(text).is_absolute()
        || text.split(['/', '\\']).any(|part| part == "..")
    {
        return None;
    }
    let normalized = text.replace('\\', "/").trim_start_matches('/').to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn read_project_generate_commit_body(project: &Value) -> bool {
    project
        .get("gitConfig")
        .and_then(Value::as_object)
        .and_then(|config| config.get("generateCommitBody"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn resolve_commit_message_generation_agent(
    project: &Value,
    params: &Map<String, Value>,
    settings: &Map<String, Value>,
) -> std::result::Result<CommitMessageGenerationAgent, DomainStateError> {
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| read_text_from_map(settings, "defaultPromptAgentId"))
        .unwrap_or_else(|| "codex".to_string());
    let agent_config = resolve_project_agent_config(project, &agent_id, None);
    let command = read_text_from_map(&agent_config, "command")
        .or_else(|| default_agent_command(&agent_id).map(str::to_string))
        .ok_or_else(|| DomainStateError {
            code: "dependencyUnavailable",
            message: "Choose a configured prompt agent before generating a commit message."
                .to_string(),
        })?;
    let is_default = default_agent_command(&agent_id).is_some();
    let name = read_text_from_map(&agent_config, "name").unwrap_or_else(|| {
        default_agent_name(&agent_id)
            .unwrap_or(agent_id.as_str())
            .to_string()
    });
    Ok(CommitMessageGenerationAgent {
        agent_id,
        command,
        is_default,
        name,
    })
}

fn default_agent_name(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some("Amp CLI"),
        "antigravity" => Some("Antigravity CLI"),
        "claude" => Some("Claude"),
        "codebuddy" => Some("CodeBuddy"),
        "codex" => Some("Codex"),
        "copilot" => Some("Copilot"),
        "cursor" => Some("Cursor CLI"),
        "droid" => Some("Factory Droid"),
        "gemini" => Some("Gemini"),
        "grok" => Some("Grok Build"),
        "hermes-agent" => Some("Hermes Agent"),
        "kiro" => Some("Kiro CLI"),
        "omp" => Some("OMP"),
        "opencode" => Some("OpenCode"),
        "pi" => Some("Pi Agent"),
        "qoder" => Some("Qoder"),
        "rovodev" => Some("Rovo Dev"),
        "t3" => Some("T3 Code"),
        _ => None,
    }
}

fn build_gxserver_commit_message_generation_prompt(
    branch: &str,
    generate_body: bool,
    staged_summary: &str,
    staged_patch: &str,
) -> String {
    let branch_line = format!(
        "Branch: {}",
        if branch.trim().is_empty() {
            "(detached)"
        } else {
            branch.trim()
        }
    );
    let summary = staged_summary.chars().take(6_000).collect::<String>();
    let patch = staged_patch.chars().take(40_000).collect::<String>();
    [
        "You write concise git commit messages.",
        "Return only a JSON object with keys: subject, body.",
        "Rules:",
        "- subject must be imperative, <= 72 chars, and no trailing period",
        if generate_body {
            "- body can be empty string or short bullet points"
        } else {
            "- body must be an empty string"
        },
        "- capture the primary user-visible or developer-visible change",
        "",
        &branch_line,
        "",
        "Staged files:",
        &summary,
        "",
        "Staged patch:",
        &patch,
    ]
    .join("\n")
}

async fn run_commit_message_generation_agent(
    state: &AppState,
    cwd: &str,
    agent: &CommitMessageGenerationAgent,
    prompt: &str,
) -> std::result::Result<String, DomainStateError> {
    let delimiter = format!(
        "ghostex_GXSERVER_GIT_COMMIT_{}",
        chrono::Utc::now().timestamp_millis()
    );
    let shell_command = build_commit_message_generation_shell_command(agent, &delimiter, prompt)?;
    let shell = command_shell();
    let mut child = Command::new(&shell.executable);
    child
        .args(shell.interactive_script_args(&shell_command))
        .current_dir(cwd)
        .envs(internal_prompt_generation_environment(
            &state.paths.home_dir,
        ))
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = tokio::time::timeout(
        Duration::from_millis(GXSERVER_COMMIT_MESSAGE_GENERATION_TIMEOUT_MS),
        child.output(),
    )
    .await
    .map_err(|_| DomainStateError {
        code: "dependencyUnavailable",
        message: "Commit message generation timed out.".to_string(),
    })?
    .map_err(|_| DomainStateError {
        code: "dependencyUnavailable",
        message: "Could not start commit message generation.".to_string(),
    })?;
    if !output.status.success() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: format!("{} commit message generation failed.", agent.name),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn build_commit_message_generation_shell_command(
    agent: &CommitMessageGenerationAgent,
    delimiter: &str,
    prompt: &str,
) -> std::result::Result<String, DomainStateError> {
    Ok(match agent.agent_id.as_str() {
        "codex" => create_here_doc_command(
            &format!(
                "{} exec --ephemeral --skip-git-repo-check -m gpt-5.4-mini -c 'model_reasoning_effort=\"low\"'",
                agent.command
            ),
            delimiter,
            prompt,
        ),
        "cursor" => format!(
            "{} --print --mode ask --trust --output-format text {}",
            agent.command,
            quote_shell_arg(prompt)
        ),
        "claude" | "gemini" => {
            create_here_doc_command(&format!("{} -p", agent.command), delimiter, prompt)
        }
        _ if !agent.is_default => create_here_doc_command(&agent.command, delimiter, prompt),
        _ => {
            return Err(DomainStateError {
                code: "badRequest",
                message: format!(
                    "{} does not support background commit message generation.",
                    agent.name
                ),
            })
        }
    })
}

fn parse_gxserver_generated_commit_message(
    stdout: &str,
    agent_name: &str,
) -> std::result::Result<Value, DomainStateError> {
    let start = stdout.find('{').ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        ))
    })?;
    let end = stdout.rfind('}').ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        ))
    })?;
    if end < start {
        return Err(DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        )));
    }
    let parsed = serde_json::from_str::<Value>(&stdout[start..=end]).map_err(|_| {
        DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        ))
    })?;
    let subject = parsed
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '。'])
        .chars()
        .take(72)
        .collect::<String>()
        .trim()
        .to_string();
    if subject.is_empty() {
        return Err(DomainStateError::bad_request(format!(
            "{agent_name} returned an empty commit subject."
        )));
    }
    let body = parsed
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(json!({ "body": body, "subject": subject }))
}

fn typed_operation_scope_rejection_details(
    endpoint_path: &str,
    params: &Map<String, Value>,
    error: &TypedOperationError,
) -> Value {
    let mut details = Map::new();
    if let Some(action) = params.get("action").and_then(Value::as_str) {
        details.insert("action".to_string(), json!(action));
    }
    details.insert(
        "endpoint".to_string(),
        json!(endpoint_path.trim_start_matches("/api/")),
    );
    details.insert("errorCode".to_string(), json!(error.code));
    details.insert("errorType".to_string(), json!("GxserverProjectPathError"));
    details.insert(
        "hasProjectId".to_string(),
        json!(params
            .get("projectId")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())),
    );
    details.insert(
        "hasProjectPath".to_string(),
        json!(params
            .get("projectPath")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())),
    );
    Value::Object(details)
}

fn typed_operation_error_response(
    endpoint_path: String,
    request_id: String,
    error: TypedOperationError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "forbidden" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

#[derive(Clone)]
struct ProjectWorktreeOperationContext {
    parent_path: String,
    parent_project: Value,
    parent_project_id: String,
    projects: Vec<Value>,
    source_path: String,
    source_project_id: String,
}

#[derive(Clone)]
struct ProjectWorktreeOptionRow {
    branch: String,
    is_current_project: bool,
    is_registered: bool,
    name: String,
    path: String,
    worktree_key: String,
}

struct ProjectWorktreeTarget {
    branch: String,
    name: String,
    path: String,
}

enum ProjectWorktreeOperationError {
    Domain(DomainStateError),
    ProjectPath(ProjectPathHttpError),
    Typed(TypedOperationError),
}

impl From<DomainStateError> for ProjectWorktreeOperationError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ProjectPathHttpError> for ProjectWorktreeOperationError {
    fn from(error: ProjectPathHttpError) -> Self {
        Self::ProjectPath(error)
    }
}

impl From<TypedOperationError> for ProjectWorktreeOperationError {
    fn from(error: TypedOperationError) -> Self {
        Self::Typed(error)
    }
}

/*
CDXC:RemoteWorktrees 2026-06-24-18:40:
GPUI remote Add Worktree, Open Existing, direct merge, and commit-on-new-branch
must be id-scoped gxserver operations. The daemon resolves registered
project/worktree ids to paths, derives target branch/path names from bounded
labels, re-lists worktrees before opening by opaque key, and never accepts a
renderer-provided absolute path as remote mutation authority.
*/
async fn handle_project_worktree_operation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match endpoint_path.as_str() {
        "/api/listProjectWorktrees" => list_project_worktrees_for_project(state, &params).await,
        "/api/createProjectWorktree" => create_project_worktree_for_project(state, &params).await,
        "/api/openProjectWorktree" => open_project_worktree_for_project(state, &params).await,
        "/api/mergeWorktreeIntoMain" => merge_worktree_into_main_for_project(state, &params).await,
        "/api/checkoutProjectNewBranch" => {
            checkout_project_new_branch_for_commit(state, &params).await
        }
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver worktree endpoint."
        ))
        .into()),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_worktree_operation_error_response(endpoint_path, request_id, error),
    }
}

async fn list_project_worktrees_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    project_worktree_list_payload(&context).await
}

async fn create_project_worktree_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let base_ref = normalize_project_worktree_git_ref(params.get("baseRef"), "baseRef")?;
    let name_hint = normalize_project_worktree_name_hint(params.get("nameHint"))?;
    let target = resolve_unique_project_worktree_target(&context, &name_hint).await?;
    let mut create_params = Map::new();
    create_params.insert("baseRef".to_string(), Value::String(base_ref));
    create_params.insert("branch".to_string(), Value::String(target.branch.clone()));
    create_params.insert(
        "worktreePath".to_string(),
        Value::String(target.path.clone()),
    );
    let create = run_project_worktree_action(
        &context.projects,
        "create",
        &context.source_path,
        create_params,
    )
    .await?;
    if exit_code(&create) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&create, "git worktree add failed."),
            scope_rejection: false,
        }
        .into());
    }

    let parent_name = context
        .parent_project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Project");
    let project = register_project_worktree_path(
        state,
        &target.path,
        &format!("{parent_name}-{}", target.name),
        "projectAdded",
    )?;
    prepare_registered_worktree_project(state, &project, &context.source_project_id).await?;
    Ok(json!({ "project": project }))
}

async fn open_project_worktree_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let worktree_key = normalize_project_worktree_key(params.get("worktreeKey"))?;
    let options = project_worktree_options(&context).await?;
    let selected = options
        .into_iter()
        .find(|option| option.worktree_key == worktree_key)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Selected worktree is no longer in the current gxserver worktree list.",
            )
        })?;
    let project =
        register_project_worktree_path(state, &selected.path, &selected.name, "projectAdded")?;
    prepare_registered_worktree_project(state, &project, &context.source_project_id).await?;
    Ok(json!({ "project": project }))
}

async fn merge_worktree_into_main_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let plan = prepare_project_worktree_merge_plan(state, params)?;
    let branch = resolve_current_project_worktree_branch_name(&plan.projects, &plan.worktree_path)
        .await?
        .or(plan.worktree_branch)
        .ok_or_else(|| {
            DomainStateError::bad_request("Create and checkout a branch before merging.")
        })?;

    let mut main_params = Map::new();
    main_params.insert("ref".to_string(), Value::String("main".to_string()));
    let main_check =
        run_project_git_action(&plan.projects, "verifyRef", &plan.parent_path, main_params).await?;
    if exit_code(&main_check) != 0 {
        return Err(DomainStateError::bad_request(
            "The parent project does not have a local main branch.",
        )
        .into());
    }

    let status =
        run_project_git_action(&plan.projects, "status", &plan.parent_path, Map::new()).await?;
    if exit_code(&status) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&status, "Could not read parent project status."),
            scope_rejection: false,
        }
        .into());
    }
    if has_porcelain_status_changes(status.get("stdout").and_then(Value::as_str).unwrap_or("")) {
        return Err(DomainStateError::bad_request(
            "Commit or stash changes in the main project before merging this worktree.",
        )
        .into());
    }

    let mut checkout_params = Map::new();
    checkout_params.insert("branch".to_string(), Value::String("main".to_string()));
    let checkout = run_project_git_action(
        &plan.projects,
        "checkout",
        &plan.parent_path,
        checkout_params,
    )
    .await?;
    if exit_code(&checkout) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&checkout, "Could not checkout main."),
            scope_rejection: false,
        }
        .into());
    }

    let mut merge_params = Map::new();
    merge_params.insert("branch".to_string(), Value::String(branch));
    let merge =
        run_project_git_action(&plan.projects, "merge", &plan.parent_path, merge_params).await?;
    let status = if exit_code(&merge) == 0 {
        "merged"
    } else {
        "conflicts"
    };
    Ok(json!({
        "parentProjectId": plan.parent_project_id,
        "status": status,
    }))
}

async fn checkout_project_new_branch_for_commit(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let branch_label = normalize_project_branch_label(params.get("branchLabel"))?;
    let (projects, project_path) = resolve_registered_project_path(state, &project_id)?;
    for index in 0..20 {
        let branch = if index == 0 {
            branch_label.clone()
        } else {
            format!("{branch_label}-{}", index + 1)
        };
        let mut verify_params = Map::new();
        verify_params.insert(
            "ref".to_string(),
            Value::String(format!("refs/heads/{branch}")),
        );
        let exists =
            run_project_git_action(&projects, "verifyRef", &project_path, verify_params).await?;
        if exit_code(&exists) == 0 {
            continue;
        }
        let mut checkout_params = Map::new();
        checkout_params.insert("branch".to_string(), Value::String(branch));
        let checkout = run_project_git_action(
            &projects,
            "checkoutNewBranch",
            &project_path,
            checkout_params,
        )
        .await?;
        if exit_code(&checkout) == 0 {
            return Ok(json!({ "checkedOut": true }));
        }
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&checkout, "Could not create a new branch."),
            scope_rejection: false,
        }
        .into());
    }
    Err(DomainStateError::bad_request("Could not create a unique branch.").into())
}

// ---------------------------------------------------------------------------
// Sidebar V2 worktree sessions
// ---------------------------------------------------------------------------

/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
`createWorktreeSession` is one atomic server operation: optional `git fetch
origin`, `git worktree add -b ghostex/<8hex>`, the project's own worktree setup
command, then an ORDINARY gxserver session created through the same
create/identity/start machinery every other session uses, with the worktree as
its cwd. Anything that fails after the checkout exists rolls the checkout back,
so a failed attempt never leaves a stray directory or branch behind.

The worktree is deliberately NOT registered as a project (no
`registerProjectPath`): in Sidebar V2 a worktree is an attribute of a session,
and the branch on its card comes from the per-session git probe reading that cwd.
*/
const GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_MAX_BYTES: usize = 16_384;
/*
The same settle window GPUI uses between starting an agent session's provider
and submitting its first prompt: the agent CLI has to draw its composer before
typed text means anything.
*/
const GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_READY_DELAY_MS: u64 = 4_000;
const GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS: usize = 8;
const GXSERVER_WORKTREE_SESSION_DEFAULT_TITLE: &str = "Terminal";
/*
Warnings are user-facing strings by contract (`warnings?: readonly string[]`),
so they stay fixed, bounded sentences: raw git stdout/stderr never reaches a
client through this endpoint.
*/
const WORKTREE_SESSION_DIRTY_WARNING: &str = "This worktree has uncommitted changes.";

struct WorktreeSessionCreateRequest {
    agent_id: Option<String>,
    base_branch: Option<String>,
    existing_worktree_path: Option<String>,
    first_prompt: Option<String>,
    start_from_origin: bool,
}

struct PreparedWorktreeCheckout {
    branch: String,
    /// False when an existing worktree was adopted: rollback must never remove a
    /// checkout this request did not create.
    created: bool,
    path: String,
}

/*
`git worktree list` prints the REAL path (symlinks resolved), while registered
project paths and client-supplied paths keep whatever form the user typed. Both
are compared through their resolved form so a repository reached through a
symlink still matches its own worktree list, while every git command keeps
running against the path form the registered project family is expressed in.
*/
fn canonical_worktree_path_key(path: &str) -> String {
    fs::canonicalize(path)
        .map(|resolved| normalize_project_path_for_comparison(&path_to_string(&resolved)))
        .unwrap_or_else(|_| normalize_project_path_for_comparison(path))
}

async fn handle_worktree_session_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match endpoint_path.as_str() {
        "/api/createWorktreeSession" => create_worktree_session(state, &params).await,
        "/api/removeSessionWorktree" => remove_session_worktree(state, &params).await,
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver worktree session endpoint."
        ))
        .into()),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_worktree_operation_error_response(endpoint_path, request_id, error),
    }
}

async fn create_worktree_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let request = normalize_worktree_session_create_request(params)?;
    let prepared = prepare_worktree_session_checkout(state, &context, &request).await?;
    match start_worktree_session(state, &context, &request, &prepared).await {
        Ok(session_id) => Ok(json!({
            "branch": prepared.branch,
            "sessionId": session_id,
            "worktreePath": prepared.path,
        })),
        Err(error) => {
            if prepared.created {
                rollback_worktree_session_checkout(&context, &prepared).await;
            }
            Err(error)
        }
    }
}

fn normalize_worktree_session_create_request(
    params: &Map<String, Value>,
) -> std::result::Result<WorktreeSessionCreateRequest, DomainStateError> {
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(agent_id) = agent_id.as_deref() {
        if agent_id.len() > 64
            || !agent_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        {
            return Err(DomainStateError::bad_request(
                "agentId is not an allowed agent id.",
            ));
        }
    }
    let base_branch = match params.get("baseBranch") {
        None | Some(Value::Null) => None,
        Some(value) if value.as_str().map(str::trim) == Some("") => None,
        Some(value) => Some(normalize_project_worktree_git_ref(
            Some(value),
            "baseBranch",
        )?),
    };
    let first_prompt = params
        .get("firstPrompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(prompt) = first_prompt.as_deref() {
        if prompt.len() > GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_MAX_BYTES {
            return Err(DomainStateError::bad_request(
                "firstPrompt exceeds the 16384-byte limit.",
            ));
        }
    }
    let existing_worktree_path = match params.get("existingWorktree") {
        None | Some(Value::Null) => None,
        Some(Value::Object(existing)) => {
            let path = existing
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DomainStateError::bad_request("existingWorktree.path must be a non-empty path.")
                })?;
            Some(path.to_string())
        }
        Some(_) => {
            return Err(DomainStateError::bad_request(
                "existingWorktree must be an object with a path.",
            ))
        }
    };
    Ok(WorktreeSessionCreateRequest {
        agent_id,
        base_branch,
        existing_worktree_path,
        first_prompt,
        start_from_origin: params.get("startFromOrigin").and_then(Value::as_bool) == Some(true),
    })
}

/*
Either adopts the caller's existing worktree or creates a fresh one. An adopted
path is never trusted as given: it has to appear in THIS project family's current
`git worktree list`, which is the same authority `openProjectWorktree` uses, so a
renderer cannot point a session at an arbitrary directory.
*/
async fn prepare_worktree_session_checkout(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
) -> std::result::Result<PreparedWorktreeCheckout, ProjectWorktreeOperationError> {
    if let Some(existing_path) = request.existing_worktree_path.as_deref() {
        let requested = normalize_existing_directory_path(
            Some(&Value::String(existing_path.to_string())),
            "existingWorktree.path",
            &state.paths.home_dir,
        )?;
        let requested = normalize_project_path_for_comparison(&requested);
        let requested_key = canonical_worktree_path_key(&requested);
        let selected = project_worktree_options(context)
            .await?
            .into_iter()
            .find(|option| canonical_worktree_path_key(&option.path) == requested_key)
            .ok_or_else(|| {
                DomainStateError::bad_request(
                    "existingWorktree.path is not a worktree of this project.",
                )
            })?;
        let branch = if selected.branch.is_empty() {
            worktree_session_branch_for_path(context, &requested)
                .await?
                .unwrap_or_default()
        } else {
            selected.branch.clone()
        };
        return Ok(PreparedWorktreeCheckout {
            branch,
            created: false,
            path: requested,
        });
    }

    let base_ref = resolve_worktree_session_base_ref(context, request).await?;
    let source_path = Path::new(&context.source_path);
    let parent_directory = source_path.parent().unwrap_or_else(|| Path::new("/"));
    let project_folder_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let mut target: Option<(String, String)> = None;
    for _ in 0..GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS {
        let suffix = worktree_sessions::create_temp_branch_suffix();
        let branch = worktree_sessions::temp_branch_name(&suffix);
        let path = path_to_string(&parent_directory.join(
            worktree_sessions::worktree_directory_name(project_folder_name, &suffix),
        ));
        let mut branch_params = Map::new();
        branch_params.insert(
            "ref".to_string(),
            Value::String(format!("refs/heads/{branch}")),
        );
        let branch_check = run_project_git_action(
            &context.projects,
            "verifyRef",
            &context.source_path,
            branch_params,
        )
        .await?;
        let mut path_params = Map::new();
        path_params.insert("worktreePath".to_string(), Value::String(path.clone()));
        let path_check = run_project_worktree_action(
            &context.projects,
            "pathExists",
            &context.source_path,
            path_params,
        )
        .await?;
        if exit_code(&branch_check) != 0 && exit_code(&path_check) != 0 {
            target = Some((branch, path));
            break;
        }
    }
    let Some((branch, path)) = target else {
        return Err(DomainStateError::bad_request(
            "Could not reserve a unique worktree branch and directory.",
        )
        .into());
    };

    let mut create_params = Map::new();
    create_params.insert("baseRef".to_string(), Value::String(base_ref));
    create_params.insert("branch".to_string(), Value::String(branch.clone()));
    create_params.insert("worktreePath".to_string(), Value::String(path.clone()));
    let create = run_project_worktree_action(
        &context.projects,
        "create",
        &context.source_path,
        create_params,
    )
    .await?;
    if exit_code(&create) != 0 {
        /*
        A failed `git worktree add` is not always a no-op: it can leave a stale
        worktree registration behind, and `-b` may already have created the
        branch. The compensator therefore runs on this path too, so a refused
        request leaves the repository exactly as it found it.
        */
        rollback_worktree_session_checkout(
            context,
            &PreparedWorktreeCheckout {
                branch: branch.clone(),
                created: true,
                path: path.clone(),
            },
        )
        .await;
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&create, "git worktree add failed."),
            scope_rejection: false,
        }
        .into());
    }
    let prepared = PreparedWorktreeCheckout {
        branch,
        created: true,
        path,
    };

    if let Err(error) = run_worktree_session_setup_command(context, &prepared.path).await {
        rollback_worktree_session_checkout(context, &prepared).await;
        return Err(error);
    }
    Ok(prepared)
}

/// The project's own `worktreeCommand`, run with the new (unregistered) worktree
/// as cwd. A project without one resolves to a no-op inside the typed operation.
async fn run_worktree_session_setup_command(
    context: &ProjectWorktreeOperationContext,
    worktree_path: &str,
) -> std::result::Result<(), ProjectWorktreeOperationError> {
    let mut setup_params = Map::new();
    setup_params.insert(
        "action".to_string(),
        Value::String("worktreeSetupCommand".to_string()),
    );
    setup_params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    setup_params.insert(
        "setupCommandProjectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    setup_params.insert(
        "worktreePath".to_string(),
        Value::String(worktree_path.to_string()),
    );
    let setup = dispatch_worktree_path_operation(
        "/api/runProjectSetupCommand",
        &setup_params,
        context.projects.clone(),
    )
    .await?;
    if exit_code(&setup) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&setup, "Worktree setup command failed."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(())
}

/*
`startFromOrigin` means what it means in t3code: fetch the remote first and base
the new branch on the REMOTE tip, not on whatever the local branch happens to
point at. Without it the base is the requested branch, or the repository's own
default branch resolved by the shared P3 rules.
*/
async fn resolve_worktree_session_base_ref(
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
) -> std::result::Result<String, ProjectWorktreeOperationError> {
    let base = match request.base_branch.clone() {
        Some(base) => base,
        None => {
            let repository_path = context.parent_path.clone();
            let default_branch = tokio::task::spawn_blocking(move || {
                worktree_sessions::resolve_repository_default_branch(&repository_path)
            })
            .await
            .ok()
            .flatten();
            default_branch.map(|branch| branch.git_ref).ok_or_else(|| {
                DomainStateError::bad_request(
                    "This repository has no default branch to base a worktree on. Choose a base branch.",
                )
            })?
        }
    };
    if !request.start_from_origin {
        return Ok(base);
    }
    let repository_path = context.parent_path.clone();
    let fetch_base = base.clone();
    let commit = tokio::task::spawn_blocking(move || {
        if !worktree_sessions::fetch_worktree_origin(&repository_path) {
            return None;
        }
        worktree_sessions::resolve_origin_base_commit(&repository_path, &fetch_base)
    })
    .await
    .ok()
    .flatten();
    commit.ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Could not resolve origin/{} after fetching origin.",
            worktree_sessions::base_branch_short_name(&base)
        ))
        .into()
    })
}

async fn worktree_session_branch_for_path(
    context: &ProjectWorktreeOperationContext,
    worktree_path: &str,
) -> std::result::Result<Option<String>, ProjectWorktreeOperationError> {
    let mut params = Map::new();
    params.insert("action".to_string(), Value::String("branch".to_string()));
    params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    params.insert(
        "worktreePath".to_string(),
        Value::String(worktree_path.to_string()),
    );
    let branch =
        dispatch_worktree_path_operation("/api/runGitAction", &params, context.projects.clone())
            .await?;
    if exit_code(&branch) != 0 {
        return Ok(None);
    }
    Ok(branch
        .get("stdout")
        .and_then(Value::as_str)
        .and_then(normalize_branch_name))
}

/*
Compensation for a half-created worktree session. Every step is best effort and
independently useful: the checkout may exist without the branch being reachable,
`git worktree add` may have left a registration behind, and the caller has
already failed, so a cleanup failure must not replace the real error.
*/
async fn rollback_worktree_session_checkout(
    context: &ProjectWorktreeOperationContext,
    prepared: &PreparedWorktreeCheckout,
) {
    let mut remove_params = Map::new();
    remove_params.insert(
        "worktreePath".to_string(),
        Value::String(prepared.path.clone()),
    );
    remove_params.insert("force".to_string(), Value::Bool(true));
    let _ = run_project_worktree_action(
        &context.projects,
        "remove",
        &context.parent_path,
        remove_params,
    )
    .await;
    let _ =
        run_project_worktree_action(&context.projects, "prune", &context.parent_path, Map::new())
            .await;
    if worktree_sessions::is_managed_worktree_branch(&prepared.branch) {
        let mut branch_params = Map::new();
        branch_params.insert("branch".to_string(), Value::String(prepared.branch.clone()));
        let _ = run_project_git_action(
            &context.projects,
            "deleteLocalBranchForce",
            &context.parent_path,
            branch_params,
        )
        .await;
    }
}

/*
The session half. This is the ordinary gxserver create path — the same
`createAgentSession` parameter builder, the same `create_session` +
`apply_created_session_identity` pair, the same `startSessionProvider` — with
`cwd` pointed at the worktree. Nothing here is a worktree-specific session
concept; the only extra state is the marker that lets the branch auto-rename
recognise its own work later.
*/
async fn start_worktree_session(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
    prepared: &PreparedWorktreeCheckout,
) -> std::result::Result<String, ProjectWorktreeOperationError> {
    let (project_id, session_id) =
        create_and_start_worktree_session(state, context, request, prepared)?;
    if let Some(prompt) = request.first_prompt.as_deref() {
        /*
        Text and Enter are two separate zmx sends with a settle window between
        them (`sendSessionMessage` owns that split): bracketed-paste composers
        treat a carriage return inside the same burst as a newline and leave the
        prompt staged instead of submitted. A prompt that fails to land is not
        worth discarding a working session and its worktree over, so it is
        logged rather than rolled back.
        */
        tokio::time::sleep(Duration::from_millis(
            GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_READY_DELAY_MS,
        ))
        .await;
        send_worktree_session_first_prompt(state, &project_id, &session_id, prompt);
    }
    Ok(session_id)
}

fn create_and_start_worktree_session(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
    prepared: &PreparedWorktreeCheckout,
) -> std::result::Result<(String, String), ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository
        .get_project(&context.source_project_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Project {} does not exist.",
                context.source_project_id
            ))
        })?;
    let agent_settings = read_agent_settings(&db)?;

    let mut create_params = Map::new();
    create_params.insert("cwd".to_string(), Value::String(prepared.path.clone()));
    create_params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    create_params.insert(
        "surface".to_string(),
        Value::String("workspace".to_string()),
    );
    let mut create_params = if let Some(agent_id) = request.agent_id.as_deref() {
        create_params.insert("agentId".to_string(), Value::String(agent_id.to_string()));
        create_params.insert("requireLaunchCommand".to_string(), Value::Bool(true));
        if let Some(prompt) = request.first_prompt.as_deref() {
            create_params.insert(
                "runtimeSettings".to_string(),
                json!({ "firstUserMessage": prompt }),
            );
        }
        create_agent_session_params_for_project(&db, &project, &create_params)?
    } else {
        create_params.insert("kind".to_string(), Value::String("terminal".to_string()));
        create_params.insert(
            "title".to_string(),
            Value::String(GXSERVER_WORKTREE_SESSION_DEFAULT_TITLE.to_string()),
        );
        create_params
    };

    let initial_title = create_params
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(GXSERVER_WORKTREE_SESSION_DEFAULT_TITLE)
        .to_string();
    let mut runtime_settings = create_params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert(
        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY.to_string(),
        worktree_sessions::worktree_session_marker_value(
            &prepared.branch,
            &prepared.path,
            &initial_title,
            &now_iso(),
        ),
    );
    create_params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );

    let created = repository.create_session(&create_params, false)?;
    /*
    The durable row exists the moment `create_session` returns. If the identity
    pass (or reading the identity back) fails, the caller's rollback only removes
    the checkout — the row would survive pointing at a directory that no longer
    exists. Drop it here so the failure leaves nothing behind.
    */
    let identity =
        apply_created_session_identity(&repository, &created, &create_params).and_then(|session| {
            let project_id = value_text(&session, "projectId")?;
            let session_id = value_text(&session, "sessionId")?;
            Ok((project_id, session_id))
        });
    let (project_id, session_id) = match identity {
        Ok(identity) => identity,
        Err(error) => {
            remove_created_worktree_session_row(&repository, &created);
            return Err(error.into());
        }
    };

    let zmx_context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    let mut lifecycle_params = Map::new();
    lifecycle_params.insert("projectId".to_string(), Value::String(project_id.clone()));
    lifecycle_params.insert("sessionId".to_string(), Value::String(session_id.clone()));
    if let Err(error) = dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/startSessionProvider",
        &lifecycle_params,
        &zmx_context,
        &agent_settings,
    ) {
        /*
        The row exists but has no live terminal, so it must not survive as a
        ghost in the sidebar. Kill first in case a detached provider did come up
        after the failure, then drop the durable row.
        */
        let _ = dispatch_zmx_lifecycle_endpoint(
            &repository,
            "/api/killSession",
            &lifecycle_params,
            &zmx_context,
            &agent_settings,
        );
        let _ = repository.remove_session(&lifecycle_params);
        return Err(worktree_session_zmx_error(error).into());
    }
    /*
    Past this line the session is LIVE: a provider is running in the checkout.
    Failing the request now would roll the worktree back out from under it, so
    a delta that cannot be published is logged instead — the next presentation
    snapshot carries the row anyway.
    */
    if let Err(error) =
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)
    {
        log_worktree_session_failure(
            state,
            "worktreeSessionDeltaFailed",
            &project_id,
            &session_id,
            &error.message,
        );
    }
    Ok((project_id, session_id))
}

/*
Best-effort compensation for a session row whose identity pass failed: without
ids there is nothing to delete, and a delete that fails leaves the same orphan
the caller is already reporting, so neither case replaces the real error.
*/
fn remove_created_worktree_session_row(repository: &DomainRepository<'_>, created: &Value) {
    let (Ok(project_id), Ok(session_id)) = (
        value_text(created, "projectId"),
        value_text(created, "sessionId"),
    ) else {
        return;
    };
    let mut params = Map::new();
    params.insert("projectId".to_string(), Value::String(project_id));
    params.insert("sessionId".to_string(), Value::String(session_id));
    let _ = repository.remove_session(&params);
}

fn send_worktree_session_first_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt: &str,
) {
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            log_worktree_session_failure(
                state,
                "worktreeSessionFirstPromptFailed",
                project_id,
                session_id,
                &format!("SQLite gxserver state error: {error}"),
            );
            return;
        }
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut prompt_params = Map::new();
    prompt_params.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    prompt_params.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    prompt_params.insert("submit".to_string(), Value::Bool(true));
    prompt_params.insert("text".to_string(), Value::String(prompt.to_string()));
    if let Err(error) = dispatch_zmx_session_interaction_endpoint(
        &repository,
        "/api/sendSessionMessage",
        &prompt_params,
    ) {
        log_worktree_session_failure(
            state,
            "worktreeSessionFirstPromptFailed",
            project_id,
            session_id,
            &worktree_session_zmx_error(error).message,
        );
    }
}

fn worktree_session_zmx_error(error: ZmxEndpointError) -> DomainStateError {
    match error {
        ZmxEndpointError::Domain(error) => error,
        ZmxEndpointError::DependencyUnavailable(message) => DomainStateError {
            code: "dependencyUnavailable",
            message,
        },
    }
}

fn log_worktree_session_failure(
    state: &AppState,
    event: &str,
    project_id: &str,
    session_id: &str,
    message: &str,
) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: Some(json!({
            "projectId": project_id,
            "sessionId": session_id,
        })),
    });
}

/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
`removeSessionWorktree` backs the client's "last session in this worktree closed
— remove the worktree?" prompt. It answers the dirty question BEFORE destroying
anything, so the client can re-ask with `force`, and it only ever deletes a
branch gxserver itself minted (`ghostex/<8hex>` or the `ghostex/<slug>` it was
renamed to). Unlike `deleteWorktreeProject` it does not require the worktree to
be a registered project, because Sidebar V2 never registers one.
*/
async fn remove_session_worktree(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let force = params.get("force").and_then(Value::as_bool) == Some(true);
    let requested = normalize_existing_directory_path(
        params.get("worktreePath"),
        "worktreePath",
        &state.paths.home_dir,
    )?;
    let requested = normalize_project_path_for_comparison(&requested);
    let requested_key = canonical_worktree_path_key(&requested);
    let selected = project_worktree_options(&context)
        .await?
        .into_iter()
        .find(|option| canonical_worktree_path_key(&option.path) == requested_key)
        .ok_or_else(|| {
            DomainStateError::bad_request("worktreePath is not a worktree of this project.")
        })?;
    /*
    CDXC:SidebarV2Worktrees 2026-07-29:
    A checkout that is ALSO a registered project belongs to the V1 worktree
    project flow: deleting it here would remove the folder while the project row,
    its sessions and its own delete/merge affordances kept pointing at it. V2
    only ever owns worktrees it created as session attributes, so this is a
    refusal with a pointer at the flow that does own the checkout.

    The comparison runs through the same resolved-path key the worktree lookup
    above uses, not `selected.is_registered`: that flag compares the worktree
    list's REAL paths against registered paths as the user typed them, so a
    project registered through a symlink would read as unregistered here.
    */
    let is_registered_project = context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .any(|path| canonical_worktree_path_key(path) == requested_key);
    if is_registered_project {
        return Err(DomainStateError::bad_request(
            "This worktree is registered as its own project. Delete it from the project list instead.",
        )
        .into());
    }

    let mut status_params = Map::new();
    status_params.insert(
        "action".to_string(),
        Value::String("statusPorcelain".to_string()),
    );
    status_params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    status_params.insert("worktreePath".to_string(), Value::String(requested.clone()));
    let status = dispatch_worktree_path_operation(
        "/api/runGitAction",
        &status_params,
        context.projects.clone(),
    )
    .await?;
    if exit_code(&status) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&status, "Could not read worktree status."),
            scope_rejection: false,
        }
        .into());
    }
    let dirty = has_porcelain_status_changes(
        status
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if dirty && !force {
        return Ok(json!({
            "dirty": true,
            "removed": false,
            "warnings": [WORKTREE_SESSION_DIRTY_WARNING],
        }));
    }

    let branch = worktree_session_branch_for_path(&context, &requested)
        .await?
        .or_else(|| normalize_branch_name(&selected.branch));
    let mut remove_params = Map::new();
    remove_params.insert("worktreePath".to_string(), Value::String(requested.clone()));
    if dirty || force {
        remove_params.insert("force".to_string(), Value::Bool(true));
    }
    let remove = run_project_worktree_action(
        &context.projects,
        "remove",
        &context.parent_path,
        remove_params,
    )
    .await?;
    if exit_code(&remove) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&remove, "git worktree remove failed."),
            scope_rejection: false,
        }
        .into());
    }

    let mut warnings = Vec::new();
    if let Some(branch) =
        branch.filter(|branch| worktree_sessions::is_managed_worktree_branch(branch))
    {
        let mut branch_params = Map::new();
        branch_params.insert("branch".to_string(), Value::String(branch));
        let action = if force {
            "deleteLocalBranchForce"
        } else {
            "deleteLocalBranch"
        };
        let deleted = run_project_git_action(
            &context.projects,
            action,
            &context.parent_path,
            branch_params,
        )
        .await?;
        if exit_code(&deleted) != 0 {
            warnings.push(json!(
                "The worktree was removed, but its branch could not be deleted."
            ));
        }
    }
    let prune =
        run_project_worktree_action(&context.projects, "prune", &context.parent_path, Map::new())
            .await?;
    if exit_code(&prune) != 0 {
        warnings.push(json!(
            "The worktree was removed, but stale worktree records could not be pruned."
        ));
    }
    schedule_worktree_session_project_delta(state, &context.source_project_id)?;
    Ok(json!({
        "dirty": dirty,
        "removed": true,
        "warnings": warnings,
    }))
}

fn schedule_worktree_session_project_delta(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<(), ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")
        .map_err(Into::into)
}

fn resolve_project_worktree_operation_context(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<ProjectWorktreeOperationContext, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let source_project = repository.get_project(&project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    if source_project
        .get("isRecentProject")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(DomainStateError::bad_request(
            "Restore the project before using worktree actions.",
        )
        .into());
    }
    let source_path = project_required_path(&source_project, "Project")?;
    let source_path = normalize_existing_directory_path(
        Some(&Value::String(source_path)),
        "project.path",
        &state.paths.home_dir,
    )?;
    let source_project_id = value_text(&source_project, "projectId")?;
    let (parent_project, parent_project_id) =
        if let Some(worktree) = normalize_worktree_metadata(source_project.get("worktree")) {
            let parent_project = resolve_worktree_parent_project(&projects, &worktree)?;
            let parent_project_id = value_text(&parent_project, "projectId")?;
            (parent_project, parent_project_id)
        } else {
            (source_project.clone(), source_project_id.clone())
        };
    let parent_path = project_required_path(&parent_project, "Parent project")?;
    let parent_path = normalize_existing_directory_path(
        Some(&Value::String(parent_path)),
        "parentProject.path",
        &state.paths.home_dir,
    )?;
    Ok(ProjectWorktreeOperationContext {
        parent_path,
        parent_project,
        parent_project_id,
        projects,
        source_path,
        source_project_id,
    })
}

async fn project_worktree_list_payload(
    context: &ProjectWorktreeOperationContext,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let worktrees = project_worktree_options(context).await?;
    let branches = project_worktree_base_branches(context).await?;
    Ok(json!({
        "branches": branches,
        "parentProjectId": context.parent_project_id,
        "sourceProjectId": context.source_project_id,
        "worktrees": worktrees.into_iter().map(project_worktree_option_json).collect::<Vec<_>>(),
    }))
}

async fn project_worktree_options(
    context: &ProjectWorktreeOperationContext,
) -> std::result::Result<Vec<ProjectWorktreeOptionRow>, ProjectWorktreeOperationError> {
    let result =
        run_project_worktree_action(&context.projects, "list", &context.parent_path, Map::new())
            .await?;
    if exit_code(&result) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&result, "Could not list worktrees."),
            scope_rejection: false,
        }
        .into());
    }
    let entries = result
        .get("worktrees")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let main_path = entries
        .iter()
        .find(|entry| entry.get("bare").and_then(Value::as_bool) != Some(true))
        .and_then(|entry| entry.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| normalize_project_path_for_comparison(&context.parent_path));
    let source_path = normalize_project_path_for_comparison(&context.source_path);
    let registered_paths = context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .collect::<HashSet<_>>();

    let mut options = Vec::new();
    for entry in entries {
        if entry.get("bare").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .map(normalize_project_path_for_comparison)
            .unwrap_or_default();
        if path.is_empty() || path == main_path {
            continue;
        }
        let branch = entry
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        options.push(ProjectWorktreeOptionRow {
            branch: branch.clone(),
            is_current_project: path == source_path,
            is_registered: registered_paths.contains(&path),
            name: path_file_name_for_project(&path),
            worktree_key: project_worktree_selection_key(&path, &branch),
            path,
        });
    }
    Ok(options)
}

async fn project_worktree_base_branches(
    context: &ProjectWorktreeOperationContext,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let result = run_project_git_action(
        &context.projects,
        "listBranches",
        &context.parent_path,
        Map::new(),
    )
    .await?;
    if exit_code(&result) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&result, "Could not list branches."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(result
        .get("branches")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new())))
}

async fn resolve_unique_project_worktree_target(
    context: &ProjectWorktreeOperationContext,
    name_hint: &str,
) -> std::result::Result<ProjectWorktreeTarget, ProjectWorktreeOperationError> {
    let source_path = Path::new(&context.source_path);
    let parent_directory = source_path.parent().unwrap_or_else(|| Path::new("/"));
    let project_folder_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project");
    let registered_paths = context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .collect::<HashSet<_>>();
    for index in 0..50 {
        let name = if index == 0 {
            name_hint.to_string()
        } else {
            format!("{name_hint}-{}", index + 1)
        };
        let branch = name.clone();
        let path = path_to_string(&parent_directory.join(format!("{project_folder_name}-{name}")));
        let normalized_path = normalize_project_path_for_comparison(&path);
        let mut branch_params = Map::new();
        branch_params.insert(
            "ref".to_string(),
            Value::String(format!("refs/heads/{branch}")),
        );
        let mut path_params = Map::new();
        path_params.insert("worktreePath".to_string(), Value::String(path.clone()));
        let branch_check = run_project_git_action(
            &context.projects,
            "verifyRef",
            &context.source_path,
            branch_params,
        )
        .await?;
        let path_check = run_project_worktree_action(
            &context.projects,
            "pathExists",
            &context.source_path,
            path_params,
        )
        .await?;
        if exit_code(&branch_check) != 0
            && exit_code(&path_check) != 0
            && !registered_paths.contains(&normalized_path)
        {
            return Ok(ProjectWorktreeTarget {
                branch,
                name,
                path: normalized_path,
            });
        }
    }
    Err(DomainStateError::bad_request("Could not create a unique worktree name.").into())
}

fn register_project_worktree_path(
    state: &AppState,
    path: &str,
    name: &str,
    delta_type: &str,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut params = Map::new();
    params.insert("name".to_string(), Value::String(name.to_string()));
    params.insert("path".to_string(), Value::String(path.to_string()));
    let project = repository.add_project_path(&params)?;
    let project_id = value_text(&project, "projectId")?;
    schedule_presentation_project_delta(state, &db, &repository, &project_id, delta_type)?;
    Ok(project)
}

async fn prepare_registered_worktree_project(
    state: &AppState,
    project: &Value,
    setup_project_id: &str,
) -> std::result::Result<(), ProjectWorktreeOperationError> {
    let project_id = value_text(project, "projectId")?;
    let projects = list_domain_projects(state)?;
    let hooks = run_project_worktree_action_by_project_id(
        &projects,
        "ensureBeadsHooks",
        &project_id,
        Map::new(),
    )
    .await?;
    if exit_code(&hooks) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&hooks, "Could not prepare Beads hooks."),
            scope_rejection: false,
        }
        .into());
    }
    let setup_project = projects
        .iter()
        .find(|candidate| {
            candidate.get("projectId").and_then(Value::as_str) == Some(setup_project_id)
        })
        .cloned();
    if setup_project
        .as_ref()
        .and_then(|project| project.get("gitConfig"))
        .and_then(Value::as_object)
        .and_then(|config| config.get("worktreeCommand"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .is_none()
    {
        return Ok(());
    }
    let mut setup_params = Map::new();
    setup_params.insert(
        "action".to_string(),
        Value::String("worktreeSetupCommand".to_string()),
    );
    setup_params.insert("projectId".to_string(), Value::String(project_id));
    setup_params.insert(
        "setupCommandProjectId".to_string(),
        Value::String(setup_project_id.to_string()),
    );
    let setup =
        dispatch_typed_operation_endpoint("/api/runProjectSetupCommand", &setup_params, projects)
            .await?;
    if exit_code(&setup) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&setup, "Worktree setup command failed."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(())
}

struct ProjectWorktreeMergePlan {
    parent_path: String,
    parent_project_id: String,
    projects: Vec<Value>,
    worktree_branch: Option<String>,
    worktree_path: String,
}

fn prepare_project_worktree_merge_plan(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<ProjectWorktreeMergePlan, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let worktree_project = repository.get_project(&project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    let worktree = normalize_worktree_metadata(worktree_project.get("worktree"))
        .ok_or_else(|| DomainStateError::bad_request("Project is not a worktree."))?;
    let parent_project = resolve_worktree_parent_project(&projects, &worktree)?;
    let worktree_path = project_required_path(&worktree_project, "Worktree project")?;
    let parent_path = project_required_path(&parent_project, "Parent project")?;
    Ok(ProjectWorktreeMergePlan {
        parent_path: normalize_existing_directory_path(
            Some(&Value::String(parent_path)),
            "parentProject.path",
            &state.paths.home_dir,
        )?,
        parent_project_id: value_text(&parent_project, "projectId")?,
        projects,
        worktree_branch: worktree.branch,
        worktree_path: normalize_existing_directory_path(
            Some(&Value::String(worktree_path)),
            "project.path",
            &state.paths.home_dir,
        )?,
    })
}

async fn resolve_current_project_worktree_branch_name(
    projects: &[Value],
    worktree_path: &str,
) -> std::result::Result<Option<String>, ProjectWorktreeOperationError> {
    let branch = run_project_git_action(projects, "branch", worktree_path, Map::new()).await?;
    if exit_code(&branch) == 0 {
        if let Some(branch_name) = branch
            .get("stdout")
            .and_then(Value::as_str)
            .and_then(normalize_branch_name)
        {
            return Ok(Some(branch_name));
        }
    }
    Ok(None)
}

fn resolve_registered_project_path(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<(Vec<Value>, String), ProjectWorktreeOperationError> {
    let projects = list_domain_projects(state)?;
    let project = projects
        .iter()
        .find(|candidate| candidate.get("projectId").and_then(Value::as_str) == Some(project_id))
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
    let path = project_required_path(project, "Project")?;
    let path = normalize_existing_directory_path(
        Some(&Value::String(path)),
        "project.path",
        &state.paths.home_dir,
    )?;
    Ok((projects, path))
}

fn list_domain_projects(
    state: &AppState,
) -> std::result::Result<Vec<Value>, ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository.list_projects().map_err(Into::into)
}

async fn run_project_git_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runGitAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

async fn run_project_worktree_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

async fn run_project_worktree_action_by_project_id(
    projects: &[Value],
    action: &str,
    project_id: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

fn project_worktree_operation_error_response(
    endpoint_path: String,
    request_id: String,
    error: ProjectWorktreeOperationError,
) -> RoutedResponse {
    match error {
        ProjectWorktreeOperationError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        ProjectWorktreeOperationError::ProjectPath(error) => {
            project_path_error_response(endpoint_path, request_id, error)
        }
        ProjectWorktreeOperationError::Typed(error) => {
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}

fn project_worktree_option_json(option: ProjectWorktreeOptionRow) -> Value {
    json!({
        "branch": option.branch,
        "isCurrentProject": option.is_current_project,
        "isRegistered": option.is_registered,
        "name": option.name,
        "path": option.path,
        "worktreeKey": option.worktree_key,
    })
}

fn project_worktree_selection_key(path: &str, branch: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gxserver-worktree-selection-v1\0");
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(branch.as_bytes());
    let digest = hasher.finalize();
    format!("W{}", hex_prefix(&digest, 32))
}

fn hex_prefix(bytes: &[u8], max_chars: usize) -> String {
    let mut output = String::new();
    for byte in bytes {
        if output.len() >= max_chars {
            break;
        }
        output.push_str(&format!("{byte:02x}"));
    }
    output.truncate(max_chars);
    output
}

fn normalize_project_path_for_comparison(path: &str) -> String {
    let normalized = path_to_string(&resolve_path_syntax(PathBuf::from(path.trim())));
    if normalized == "/" {
        normalized
    } else {
        normalized.trim_end_matches('/').to_string()
    }
}

fn path_file_name_for_project(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Worktree")
        .to_string()
}

fn project_required_path(
    project: &Value,
    label: &str,
) -> std::result::Result<String, DomainStateError> {
    project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{label} has no filesystem path.")))
}

fn normalize_project_worktree_key(
    input: Option<&Value>,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("worktreeKey must be a non-empty string."))?;
    if value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(value.to_string())
    } else {
        Err(DomainStateError::bad_request(
            "worktreeKey contains unsupported characters.",
        ))
    }
}

fn normalize_project_worktree_git_ref(
    input: Option<&Value>,
    field: &str,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request(format!("{field} must be a non-empty string."))
        })?;
    if value.len() <= 200 && is_allowed_project_git_ref(value) {
        Ok(value.to_string())
    } else {
        Err(DomainStateError::bad_request(format!(
            "{field} is not an allowed Git ref."
        )))
    }
}

fn normalize_project_worktree_name_hint(
    input: Option<&Value>,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("nameHint must be a non-empty string."))?;
    normalize_project_slug_label(value, "nameHint")
}

fn normalize_project_branch_label(
    input: Option<&Value>,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("branchLabel must be a non-empty string."))?;
    normalize_project_slug_label(value, "branchLabel")
}

fn normalize_project_slug_label(
    value: &str,
    field: &str,
) -> std::result::Result<String, DomainStateError> {
    if value.chars().count() > 160 || value.contains('\0') {
        return Err(DomainStateError::bad_request(format!(
            "{field} exceeds the allowed label size."
        )));
    }
    let mut output = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            output.push('-');
            last_was_dash = true;
        }
        if output.len() >= 48 {
            break;
        }
    }
    let normalized = output.trim_matches('-').to_string();
    if normalized.is_empty()
        || !normalized
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(DomainStateError::bad_request(format!(
            "{field} must contain at least one ASCII letter or number."
        )));
    }
    Ok(normalized)
}

fn is_allowed_project_git_ref(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}

#[derive(Clone)]
struct DeleteWorktreeProjectParams {
    delete_local_branch: bool,
    delete_remote_branch: bool,
    project_id: String,
    remote_name: String,
}

#[derive(Clone)]
struct NormalizedWorktreeMetadata {
    branch: Option<String>,
    parent_project_id: String,
    parent_project_path: Option<String>,
}

struct DeleteWorktreeProjectPlan {
    params: DeleteWorktreeProjectParams,
    parent_path: String,
    projects: Vec<Value>,
    worktree_branch: Option<String>,
    worktree_path: String,
}

enum DeleteWorktreeProjectError {
    Domain(DomainStateError),
    ProjectPath(ProjectPathHttpError),
    Typed(TypedOperationError),
}

impl From<DomainStateError> for DeleteWorktreeProjectError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ProjectPathHttpError> for DeleteWorktreeProjectError {
    fn from(error: ProjectPathHttpError) -> Self {
        Self::ProjectPath(error)
    }
}

impl From<TypedOperationError> for DeleteWorktreeProjectError {
    fn from(error: TypedOperationError) -> Self {
        Self::Typed(error)
    }
}

/*
CDXC:WorktreeDelete 2026-06-22-08:47:
Rust gxserver must own the same shared Delete Worktree workflow as TypeScript: validate the selected worktree project, remove the Git checkout from the registered parent, delete the durable project row before optional branch cleanup, return cleanup failures as warnings, and publish the presentation delta after the canonical row is gone.
*/
async fn handle_delete_worktree_project_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match prepare_delete_worktree_project_plan(state, &params) {
        Ok(plan) => delete_worktree_project_from_plan(state, plan).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => delete_worktree_project_error_response(endpoint_path, request_id, error),
    }
}

fn prepare_delete_worktree_project_plan(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<DeleteWorktreeProjectPlan, DeleteWorktreeProjectError> {
    let params = normalize_delete_worktree_project_params(params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let worktree_project = repository.get_project(&params.project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {} does not exist.", params.project_id))
    })?;
    let worktree_project_path = worktree_project
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Worktree project has no filesystem path."))?;
    let worktree = normalize_worktree_metadata(worktree_project.get("worktree"))
        .ok_or_else(|| DomainStateError::bad_request("Project is not a worktree."))?;
    let parent_project = resolve_worktree_parent_project(&projects, &worktree)?;
    let parent_project_path = parent_project
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Parent project {} does not exist.",
                worktree.parent_project_id
            ))
        })?;
    let worktree_path_value = Value::String(worktree_project_path.to_string());
    let parent_path_value = Value::String(parent_project_path.to_string());
    let worktree_path = normalize_existing_directory_path(
        Some(&worktree_path_value),
        "project.path",
        &state.paths.home_dir,
    )?;
    let parent_path = normalize_existing_directory_path(
        Some(&parent_path_value),
        "parentProject.path",
        &state.paths.home_dir,
    )?;
    Ok(DeleteWorktreeProjectPlan {
        params,
        parent_path,
        projects,
        worktree_branch: worktree.branch,
        worktree_path,
    })
}

async fn delete_worktree_project_from_plan(
    state: &AppState,
    plan: DeleteWorktreeProjectPlan,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    let branch_name = if plan.params.delete_local_branch || plan.params.delete_remote_branch {
        resolve_current_worktree_branch_name(&plan).await?
    } else {
        None
    };
    let checkout_removal = remove_worktree_checkout(&plan).await?;
    let project = remove_worktree_project_row(state, &plan.params.project_id)?;
    let mut warnings = delete_selected_worktree_branches(&plan, branch_name).await?;
    let prune =
        run_delete_worktree_action(&plan.projects, "prune", &plan.parent_path, Map::new()).await?;
    if exit_code(&prune) != 0 {
        warnings.push(json!({
            "kind": "pruneFailed",
            "message": operation_failure_message(&prune, "git worktree prune failed."),
        }));
    }
    schedule_deleted_worktree_project_delta(state, &plan.params.project_id)?;
    Ok(json!({
        "checkoutRemoval": checkout_removal,
        "project": project,
        "warnings": warnings,
    }))
}

fn normalize_delete_worktree_project_params(
    params: &Map<String, Value>,
) -> std::result::Result<DeleteWorktreeProjectParams, DomainStateError> {
    let project_id = read_project_id(params)?;
    let remote_name = params
        .get("remoteName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("origin")
        .to_string();
    if !is_allowed_git_remote_name(&remote_name) {
        return Err(DomainStateError::bad_request(
            "remoteName is not an allowed Git remote name.",
        ));
    }
    Ok(DeleteWorktreeProjectParams {
        delete_local_branch: params.get("deleteLocalBranch").and_then(Value::as_bool) == Some(true),
        delete_remote_branch: params.get("deleteRemoteBranch").and_then(Value::as_bool)
            == Some(true),
        project_id,
        remote_name,
    })
}

fn normalize_worktree_metadata(candidate: Option<&Value>) -> Option<NormalizedWorktreeMetadata> {
    let worktree = candidate.and_then(Value::as_object)?;
    let parent_project_id = worktree.get("parentProjectId").and_then(Value::as_str)?;
    if !is_gxserver_project_id(parent_project_id) {
        return None;
    }
    Some(NormalizedWorktreeMetadata {
        branch: worktree
            .get("branch")
            .and_then(Value::as_str)
            .and_then(normalize_branch_name),
        parent_project_id: parent_project_id.to_string(),
        parent_project_path: worktree
            .get("parentProjectPath")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_string),
    })
}

fn resolve_worktree_parent_project(
    projects: &[Value],
    worktree: &NormalizedWorktreeMetadata,
) -> std::result::Result<Value, DomainStateError> {
    let parent_project = projects
        .iter()
        .find(|project| {
            project.get("projectId").and_then(Value::as_str)
                == Some(worktree.parent_project_id.as_str())
        })
        .cloned()
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Parent project {} does not exist.",
                worktree.parent_project_id
            ))
        })?;
    let parent_path = parent_project
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Parent project {} does not exist.",
                worktree.parent_project_id
            ))
        })?;
    if let Some(expected_path) = worktree
        .parent_project_path
        .as_deref()
        .filter(|path| !path.is_empty())
    {
        let expected = path_to_string(&resolve_path_syntax(PathBuf::from(expected_path)));
        let actual = path_to_string(&resolve_path_syntax(PathBuf::from(parent_path)));
        if expected != actual {
            return Err(DomainStateError::bad_request(
                "Worktree parent project path does not match the registered parent project.",
            ));
        }
    }
    Ok(parent_project)
}

async fn resolve_current_worktree_branch_name(
    plan: &DeleteWorktreeProjectPlan,
) -> std::result::Result<Option<String>, DeleteWorktreeProjectError> {
    let branch =
        run_delete_git_action(&plan.projects, "branch", &plan.worktree_path, Map::new()).await?;
    if exit_code(&branch) == 0 {
        if let Some(branch_name) = branch
            .get("stdout")
            .and_then(Value::as_str)
            .and_then(normalize_branch_name)
        {
            return Ok(Some(branch_name));
        }
    }
    Ok(plan.worktree_branch.clone())
}

async fn remove_worktree_checkout(
    plan: &DeleteWorktreeProjectPlan,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    let status = run_delete_git_action(
        &plan.projects,
        "statusPorcelain",
        &plan.worktree_path,
        Map::new(),
    )
    .await?;
    if exit_code(&status) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&status, "Could not read worktree status."),
            scope_rejection: false,
        }
        .into());
    }
    let force_initial_remove = has_porcelain_status_changes(
        status
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut extra = Map::new();
    extra.insert(
        "worktreePath".to_string(),
        Value::String(plan.worktree_path.clone()),
    );
    if force_initial_remove {
        extra.insert("force".to_string(), Value::Bool(true));
    }
    let mut remove =
        run_delete_worktree_action(&plan.projects, "remove", &plan.parent_path, extra).await?;
    let mut retried_for_submodules = false;
    if exit_code(&remove) != 0 && !force_initial_remove && is_submodule_removal_refusal(&remove) {
        retried_for_submodules = true;
        let mut retry_extra = Map::new();
        retry_extra.insert(
            "worktreePath".to_string(),
            Value::String(plan.worktree_path.clone()),
        );
        retry_extra.insert("force".to_string(), Value::Bool(true));
        remove =
            run_delete_worktree_action(&plan.projects, "remove", &plan.parent_path, retry_extra)
                .await?;
    }
    if exit_code(&remove) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&remove, "git worktree remove failed."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(json!({
        "forced": force_initial_remove || retried_for_submodules,
        "retriedForSubmodules": retried_for_submodules,
    }))
}

async fn delete_selected_worktree_branches(
    plan: &DeleteWorktreeProjectPlan,
    branch_name: Option<String>,
) -> std::result::Result<Vec<Value>, DeleteWorktreeProjectError> {
    let mut warnings = Vec::new();
    if plan.params.delete_local_branch {
        if let Some(branch) = branch_name.as_deref() {
            let mut extra = Map::new();
            extra.insert("branch".to_string(), Value::String(branch.to_string()));
            let local_delete = run_delete_git_action(
                &plan.projects,
                "deleteLocalBranch",
                &plan.parent_path,
                extra,
            )
            .await?;
            if exit_code(&local_delete) != 0 {
                warnings.push(json!({
                    "kind": "localBranchDeleteFailed",
                    "message": operation_failure_message(&local_delete, "git branch -d failed."),
                }));
            }
        } else {
            warnings.push(json!({
                "kind": "localBranchNotResolved",
                "message": "No local branch could be resolved.",
            }));
        }
    }
    if plan.params.delete_remote_branch {
        if let Some(branch) = branch_name.as_deref() {
            let mut extra = Map::new();
            extra.insert("branch".to_string(), Value::String(branch.to_string()));
            extra.insert(
                "remoteName".to_string(),
                Value::String(plan.params.remote_name.clone()),
            );
            let remote_delete = run_delete_git_action(
                &plan.projects,
                "deleteRemoteBranch",
                &plan.parent_path,
                extra,
            )
            .await?;
            if exit_code(&remote_delete) != 0 {
                warnings.push(json!({
                    "kind": "remoteBranchDeleteFailed",
                    "message": operation_failure_message(&remote_delete, "git push origin --delete failed."),
                }));
            }
        } else {
            warnings.push(json!({
                "kind": "remoteBranchNotResolved",
                "message": "No branch name could be resolved.",
            }));
        }
    }
    Ok(warnings)
}

async fn run_delete_git_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runGitAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

async fn run_delete_worktree_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

fn remove_worktree_project_row(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository.remove_project(project_id).map_err(Into::into)
}

fn schedule_deleted_worktree_project_delta(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<(), DeleteWorktreeProjectError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")
        .map_err(Into::into)
}

fn delete_worktree_project_error_response(
    endpoint_path: String,
    request_id: String,
    error: DeleteWorktreeProjectError,
) -> RoutedResponse {
    match error {
        DeleteWorktreeProjectError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        DeleteWorktreeProjectError::ProjectPath(error) => {
            project_path_error_response(endpoint_path, request_id, error)
        }
        DeleteWorktreeProjectError::Typed(error) => {
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}

fn is_allowed_git_remote_name(value: &str) -> bool {
    value.chars().enumerate().all(|(index, ch)| {
        (index == 0 && ch.is_ascii_alphanumeric())
            || (index > 0 && (ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')))
    })
}

fn normalize_branch_name(branch: &str) -> Option<String> {
    let value = branch.trim();
    if value.is_empty() || value == "HEAD" || value == "detached" {
        None
    } else {
        Some(value.to_string())
    }
}

fn has_porcelain_status_changes(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with("##")
    })
}

fn is_submodule_removal_refusal(result: &Value) -> bool {
    let text = format!(
        "{}\n{}",
        result
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
    );
    text.to_lowercase()
        .contains("working trees containing submodules cannot be moved or removed")
}

fn operation_failure_message(result: &Value, fallback: &str) -> String {
    result
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| result.get("stderr").and_then(Value::as_str).map(str::trim))
        .filter(|value| !value.is_empty())
        .or_else(|| result.get("stdout").and_then(Value::as_str).map(str::trim))
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn exit_code(result: &Value) -> i64 {
    result.get("exitCode").and_then(Value::as_i64).unwrap_or(1)
}

async fn handle_repository_clone_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let runtime = RepositoryCloneRuntime {
        event_hub: state.event_hub.clone(),
        logger: state.logger.clone(),
        paths: state.paths.clone(),
        presentation_event_sequence: state.presentation_event_sequence.clone(),
        server_id: state.metadata.server_id.clone(),
    };
    match dispatch_repository_clone_endpoint(
        state.repository_clone_jobs.clone(),
        runtime,
        &endpoint_path,
        &params,
    )
    .await
    {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => repository_clone_error_response(endpoint_path, request_id, error),
    }
}

/*
CDXC:AddProjectDialog 2026-07-30:
Provider discovery and repository lookup shell out to `gh`/`glab`, so they run
on the async route like the clone endpoints do. The probe cwd is the daemon's
own home directory unless the caller names an existing one, which keeps the
endpoint from becoming a way to ask about arbitrary directories.
*/
async fn handle_source_control_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match dispatch_source_control_endpoint(&endpoint_path, &params, &state.paths.home_dir).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => source_control_error_response(endpoint_path, request_id, error),
    }
}

fn source_control_error_response(
    endpoint_path: String,
    request_id: String,
    error: SourceControlError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

fn repository_clone_error_response(
    endpoint_path: String,
    request_id: String,
    error: RepositoryCloneError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "forbidden" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

#[derive(Debug)]
struct ProjectPathHttpError {
    code: &'static str,
    message: String,
}

impl ProjectPathHttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

/*
CDXC:GxserverHttpApi 2026-06-22-04:19:
The Rust HTTP dispatcher must implement the same generic filesystem API surface
as TypeScript gxserver before domain-specific worktree deletion parity lands.
Remote project picking may browse directory names through `/api/browseProjectDirectories`,
and local open-path routing may probe a Git root through `/api/resolveGitRootForPath`;
the broader filesystem/admin endpoints remain cataloged but unimplemented.
*/
fn handle_browse_project_directories_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_project_directory_browse_params(body) {
        Ok(params) => params,
        Err(error) => return project_path_error_response(endpoint_path, request_id, error),
    };
    match browse_project_directories(&params, &state.paths.home_dir) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_path_error_response(endpoint_path, request_id, error),
    }
}

fn handle_resolve_git_root_for_path_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", error.message, Some(request_id)),
            );
        }
    };
    match resolve_git_root_for_existing_directory(params.get("path"), &state.paths.home_dir) {
        Ok(git_root) => {
            let mut result = Map::new();
            if let Some(git_root) = git_root {
                result.insert("gitRoot".to_string(), Value::String(git_root));
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, Value::Object(result)),
            )
        }
        Err(error) => project_path_error_response(endpoint_path, request_id, error),
    }
}

fn project_path_error_response(
    endpoint_path: String,
    request_id: String,
    error: ProjectPathHttpError,
) -> RoutedResponse {
    let status = match error.code {
        "forbidden" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

fn read_project_directory_browse_params(
    body: &Value,
) -> std::result::Result<Map<String, Value>, ProjectPathHttpError> {
    let Some(object) = body.as_object() else {
        return Err(ProjectPathHttpError::bad_request(
            "RPC request body must be an object.",
        ));
    };
    let Some(params) = object.get("params").and_then(Value::as_object) else {
        return Err(ProjectPathHttpError::bad_request(
            "RPC params must be an object.",
        ));
    };
    Ok(params.clone())
}

fn browse_project_directories(
    params: &Map<String, Value>,
    home_dir: &Path,
) -> std::result::Result<Value, ProjectPathHttpError> {
    let partial_path = normalize_browse_path_input(params.get("partialPath"), "partialPath")?;
    let limit = normalize_browse_limit(params.get("limit"))?;
    let cwd = params.get("cwd").and_then(Value::as_str);
    let resolved_input_path = resolve_browse_target(cwd, &partial_path, home_dir)?;
    let ends_with_separator =
        partial_path == "~" || partial_path.ends_with('/') || partial_path.ends_with('\\');
    let parent_path = if ends_with_separator {
        resolved_input_path.clone()
    } else {
        resolved_input_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| resolved_input_path.clone())
    };
    let prefix = if ends_with_separator {
        String::new()
    } else {
        resolved_input_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    /*
    CDXC:AddProjectDialog 2026-07-30:
    A path browser walks directories the user may not be allowed to read, and a
    hard error there would replace the suggestion list with a failure every time
    the caret crosses one. Permission failures therefore answer with an empty
    entry list for the resolved parent (the t3code `filesystem.browse`
    contract); every other read failure still surfaces as `notFound`.
    */
    let dirents = match fs::read_dir(&parent_path) {
        Ok(dirents) => dirents,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(json!({
                "entries": Vec::<Value>::new(),
                "parentPath": path_to_string(&parent_path),
            }));
        }
        Err(_) => {
            return Err(ProjectPathHttpError::not_found(format!(
                "Unable to browse directory: {}",
                path_to_string(&parent_path)
            )));
        }
    };
    let show_hidden = ends_with_separator || prefix.starts_with('.');
    let lower_prefix = prefix.to_lowercase();
    let mut entries = Vec::new();
    for dirent in dirents.flatten() {
        let Ok(file_type) = dirent.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = dirent.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().starts_with(&lower_prefix) {
            continue;
        }
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        entries.push(json!({
            "fullPath": path_to_string(&parent_path.join(&name)),
            "name": name,
            "sortKey": name.to_lowercase(),
        }));
    }
    /*
    CDXC:AddProjectDialog 2026-07-30:
    The browse list is read top-to-bottom by a human, so it sorts the way
    `localeCompare` does rather than by byte value: case-insensitive first, raw
    name only as the tiebreaker. A byte sort would file every capitalized folder
    ahead of every lowercase one.
    */
    entries.sort_by(|left, right| {
        let key = |value: &Value| {
            (
                value
                    .get("sortKey")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        };
        key(left).cmp(&key(right))
    });
    entries.truncate(limit);
    for entry in &mut entries {
        if let Some(object) = entry.as_object_mut() {
            object.remove("sortKey");
        }
    }
    Ok(json!({
        "entries": entries,
        "parentPath": path_to_string(&parent_path),
    }))
}

fn resolve_git_root_for_existing_directory(
    input: Option<&Value>,
    home_dir: &Path,
) -> std::result::Result<Option<String>, ProjectPathHttpError> {
    let cwd = normalize_existing_directory_path(input, "path", home_dir)?;
    let output = match StdCommand::new("git")
        .args(["-C", cwd.as_str(), "rev-parse", "--show-toplevel"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    if !output.status.success() {
        return Ok(None);
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Ok(None);
    }
    Ok(Some(match fs::canonicalize(&root) {
        Ok(path) => path_to_string(&path),
        Err(_) => {
            let trimmed = root.trim_end_matches('/').to_string();
            if trimmed.is_empty() {
                root
            } else {
                trimmed
            }
        }
    }))
}

fn resolve_browse_target(
    cwd: Option<&str>,
    partial_path: &str,
    home_dir: &Path,
) -> std::result::Result<PathBuf, ProjectPathHttpError> {
    if cfg!(not(windows)) && is_windows_absolute_path(partial_path) {
        return Err(ProjectPathHttpError::bad_request(
            "Windows-style paths are only supported on Windows.",
        ));
    }
    if !is_explicit_relative_path(partial_path) {
        return Ok(resolve_path_syntax(PathBuf::from(
            expand_home_path_for_browse(partial_path, home_dir),
        )));
    }
    let Some(cwd) = cwd else {
        return Err(ProjectPathHttpError::bad_request(
            "Relative filesystem browse paths require cwd.",
        ));
    };
    let cwd = normalize_absolute_path(Some(&Value::String(cwd.to_string())), "cwd", home_dir)?;
    Ok(resolve_path_syntax(PathBuf::from(cwd).join(partial_path)))
}

fn normalize_browse_path_input(
    input: Option<&Value>,
    field: &str,
) -> std::result::Result<String, ProjectPathHttpError> {
    let Some(value) = input.and_then(Value::as_str) else {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    };
    if value.is_empty() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    if value.contains('\0') {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must not contain null bytes."
        )));
    }
    if value.chars().count() > 1024 {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} exceeds 1024 characters."
        )));
    }
    Ok(value.to_string())
}

fn normalize_browse_limit(
    input: Option<&Value>,
) -> std::result::Result<usize, ProjectPathHttpError> {
    let Some(value) = input else {
        return Ok(200);
    };
    let Some(limit) = json_number_to_positive_integer(value) else {
        return Err(ProjectPathHttpError::bad_request(
            "limit must be a positive integer.",
        ));
    };
    Ok(limit.min(500))
}

fn json_number_to_positive_integer(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_u64() {
                return usize::try_from(value).ok().filter(|value| *value >= 1);
            }
            if let Some(value) = number.as_i64() {
                return usize::try_from(value).ok().filter(|value| *value >= 1);
            }
            let value = number.as_f64()?;
            if value.is_finite() && value >= 1.0 && value.fract() == 0.0 {
                Some(value.min(usize::MAX as f64) as usize)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn normalize_existing_directory_path(
    input: Option<&Value>,
    field: &str,
    home_dir: &Path,
) -> std::result::Result<String, ProjectPathHttpError> {
    let normalized = normalize_absolute_path(input, field, home_dir)?;
    let metadata = fs::metadata(&normalized).map_err(|_| {
        ProjectPathHttpError::not_found(format!("{field} does not exist: {normalized}"))
    })?;
    if !metadata.is_dir() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} is not a directory: {normalized}"
        )));
    }
    Ok(normalized)
}

fn normalize_absolute_path(
    input: Option<&Value>,
    field: &str,
    home_dir: &Path,
) -> std::result::Result<String, ProjectPathHttpError> {
    let Some(value) = input.and_then(Value::as_str).map(str::trim) else {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    };
    if value.is_empty() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(value, home_dir);
    if !Path::new(&expanded).is_absolute() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    Ok(path_to_string(&resolve_path_syntax(PathBuf::from(
        expanded,
    ))))
}

fn resolve_path_syntax(path: PathBuf) -> PathBuf {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn expand_home_path_for_browse(input: &str, home_dir: &Path) -> String {
    if input == "~" {
        return path_to_string(home_dir);
    }
    if input.starts_with("~/") || input.starts_with("~\\") {
        return path_to_string(&home_dir.join(&input[2..]));
    }
    input.to_string()
}

fn expand_user_path(input: &str, home_dir: &Path) -> String {
    if input == "~" {
        return path_to_string(home_dir);
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return path_to_string(&home_dir.join(rest));
    }
    input.to_string()
}

fn is_explicit_relative_path(value: &str) -> bool {
    value == "."
        || value == ".."
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
}

fn is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("\\\\")
        || (bytes.len() >= 2
            && bytes[1] == b':'
            && bytes[0].is_ascii_alphabetic()
            && (bytes.len() == 2 || bytes[2] == b'/' || bytes[2] == b'\\'))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/*
CDXC:GxserverRustPort 2026-06-15-18:06:
Phase 5 lifecycle and session-I/O endpoints run through the same authenticated RPC envelope as the TypeScript daemon while zmx process work stays behind explicit endpoint handlers. Presentation deltas are still scheduled from durable state after lifecycle mutations so clients never infer sidebar state from subprocess output.
*/
async fn handle_zmx_lifecycle_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let db_result = if endpoint_path == "/api/createWorkspaceTerminal" {
        /*
        Concurrent Windows terminal clicks use independent gxserver SQLite
        connections. Apply bounded lock waiting before even the connection
        PRAGMAs so the atomic create path cannot fail immediately with
        SQLITE_BUSY midway through provider materialization or compensation.
        Other lifecycle endpoints retain their existing connection behavior.
        */
        open_gxserver_database_with_busy_timeout(&state.paths, Duration::from_secs(10))
    } else {
        open_gxserver_database(&state.paths)
    };
    let db = match db_result {
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
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    if endpoint_path == "/api/createWorkspaceTerminal" {
        return match create_started_workspace_terminal(&repository, &params, &context) {
            Ok(output) => {
                if let Some((project_id, session_id)) = output.presentation_session.as_ref() {
                    if let Err(mut error) = schedule_presentation_session_delta(
                        state,
                        &db,
                        &repository,
                        project_id,
                        session_id,
                    ) {
                        if let Some(identity) = output.created_workspace_terminal.as_ref() {
                            if let Err(cleanup_error) =
                                compensate_created_workspace_terminal(&repository, identity)
                            {
                                error.message.push_str(&format!(
                                    " Compensating cleanup also failed for the new terminal: {cleanup_error}"
                                ));
                            }
                        }
                        /*
                        The row may have appeared in a concurrent snapshot
                        before the failed revision write. Re-project its exact
                        post-cleanup state: normally sessionRemoved, or the
                        surviving exact row if durable removal itself failed.
                        */
                        if let Err(repair_error) = schedule_presentation_session_delta(
                            state,
                            &db,
                            &repository,
                            project_id,
                            session_id,
                        ) {
                            error.message.push_str(&format!(
                                " Compensating presentation reconciliation also failed: {}",
                                repair_error.message
                            ));
                        }
                        return domain_error_response(endpoint_path, request_id, error);
                    }
                }
                routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, output.result),
                )
            }
            Err(mut failure) => {
                if let Some((project_id, session_id)) = failure.presentation_session.as_ref() {
                    if let Err(repair_error) = schedule_presentation_session_delta(
                        state,
                        &db,
                        &repository,
                        project_id,
                        session_id,
                    ) {
                        append_zmx_endpoint_error_context(
                            &mut failure.error,
                            &format!(
                                " Compensating presentation reconciliation also failed: {}",
                                repair_error.message
                            ),
                        );
                    }
                }
                zmx_error_response(endpoint_path, request_id, failure.error)
            }
        };
    }
    let agent_settings = match read_agent_settings(&db) {
        Ok(settings) => settings,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = dispatch_zmx_lifecycle_endpoint(
        &repository,
        &endpoint_path,
        &params,
        &context,
        &agent_settings,
    );
    match result {
        Ok(output) => {
            if let Some((project_id, session_id)) = output.presentation_session.as_ref() {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                ) {
                    return domain_error_response(endpoint_path, request_id, error);
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, output.result),
            )
        }
        Err(error) => zmx_error_response(endpoint_path, request_id, error),
    }
}

async fn handle_zmx_session_interaction_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if endpoint_path == "/api/sendSessionMessage" && !params.contains_key("sessionId") {
        return match state
            .event_hub
            .dispatch_renderer_command("sendMessage".to_string(), params, 15_000)
            .await
        {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => zmx_error_response(
                endpoint_path,
                request_id,
                ZmxEndpointError::DependencyUnavailable(error.message),
            ),
        };
    }
    if endpoint_path == "/api/focusSession" {
        let prepared = {
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
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            prepare_focus_session_renderer_command(&repository, &params)
        };
        let (session, payload) = match prepared {
            Ok(command) => command,
            Err(error) => return zmx_error_response(endpoint_path, request_id, error),
        };
        return match state
            .event_hub
            .dispatch_renderer_command("focusSession".to_string(), payload, 15_000)
            .await
        {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    merge_session_with_renderer_result(session, result),
                ),
            ),
            Err(error) => zmx_error_response(
                endpoint_path,
                request_id,
                ZmxEndpointError::DependencyUnavailable(error.message),
            ),
        };
    }
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
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    match dispatch_zmx_session_interaction_endpoint(&repository, &endpoint_path, &params) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => zmx_error_response(endpoint_path, request_id, error),
    }
}

fn zmx_error_response(
    endpoint_path: String,
    request_id: String,
    error: ZmxEndpointError,
) -> RoutedResponse {
    match error {
        ZmxEndpointError::Domain(error) => domain_error_response(endpoint_path, request_id, error),
        ZmxEndpointError::DependencyUnavailable(message) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error("dependencyUnavailable", message, Some(request_id)),
        ),
    }
}

async fn handle_renderer_command_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let action = match params.get("action").and_then(Value::as_str) {
        Some(action) if RENDERER_COMMAND_ACTIONS.contains(&action) => action.to_string(),
        _ => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error(
                    "badRequest",
                    format!(
                        "Unsupported renderer command action: {}",
                        params
                            .get("action")
                            .map(Value::to_string)
                            .unwrap_or_else(|| "undefined".to_string())
                    ),
                    Some(request_id),
                ),
            );
        }
    };
    let payload = match params.get("payload") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(payload)) => payload.clone(),
        Some(_) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error(
                    "badRequest",
                    "Renderer command payload must be an object.",
                    Some(request_id),
                ),
            );
        }
    };
    let payload = with_renderer_session_target(payload);
    let timeout_ms = match normalize_renderer_command_timeout_ms(params.get("timeoutMs")) {
        Ok(timeout_ms) => timeout_ms,
        Err(message) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", message, Some(request_id)),
            );
        }
    };
    match state
        .event_hub
        .dispatch_renderer_command(action, payload, timeout_ms)
        .await
    {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}

fn with_renderer_session_target(mut payload: Map<String, Value>) -> Map<String, Value> {
    if payload
        .get("sessionTarget")
        .and_then(Value::as_object)
        .is_some()
    {
        return payload;
    }
    let Some(project_id) = payload
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return payload;
    };
    let Some(session_id) = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return payload;
    };
    /*
    CDXC:GxserverRendererCommands 2026-06-21-19:22:
    gxserver renderer commands target durable project/session ids, but macOS may
    render sessions with combined presentation ids. Add a structured target at
    the daemon boundary so every CLI or API caller gets the same renderer lookup
    contract without depending on sidebar id encoding.
    */
    let mut session_target = Map::new();
    if let Some(global_ref) = payload
        .get("globalRef")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        session_target.insert(
            "globalRef".to_string(),
            Value::String(global_ref.to_string()),
        );
    }
    session_target.insert("projectId".to_string(), Value::String(project_id));
    session_target.insert("sessionId".to_string(), Value::String(session_id));
    payload.insert("sessionTarget".to_string(), Value::Object(session_target));
    payload
}

fn normalize_renderer_command_timeout_ms(value: Option<&Value>) -> Result<u64, String> {
    let Some(value) = value else {
        return Ok(15_000);
    };
    let Some(raw) = value.as_f64() else {
        return Err("Renderer command timeoutMs must be a positive number.".to_string());
    };
    if !raw.is_finite() || raw <= 0.0 {
        return Err("Renderer command timeoutMs must be a positive number.".to_string());
    }
    Ok(raw.round().clamp(1_000.0, 60_000.0) as u64)
}

fn schedule_presentation_project_delta(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    delta_type: &str,
) -> std::result::Result<(), DomainStateError> {
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00 (warm widened in the P5 fix
    round):
    A project should reach Sidebar V2 already carrying its `origin` remote, so
    the very delta that announces it lands in the right cross-machine group
    instead of moving there up to a minute later. That is true of a freshly
    registered project AND of one that returns to presentation after being
    parked, whose cache entry the refresh pass evicted while it was away — hence
    the gate is "presentation publishes this project", not "delta_type is
    projectAdded" (see `ensure_published_project_git_remote_probed`).

    This still warms only the FIRST sighting of a path, and it runs outside the
    presentation sequencer below, so no other producer waits on a git spawn;
    every later delta for the same project is a pure cache read.
    */
    /*
    CDXC:SidebarV2DataGate 2026-07-29:
    Both warms are Sidebar V2 data, so both answer to the same `sidebarVersion`
    gate as the background passes. The HOOK stays wired on every daemon and the
    PROBE is what the gate stops (see `ensure_project_git_remote_probed`), so a
    V1 machine's projectAdded is an ordinary delta that spawns nothing.
    */
    if let Some(project) = repository.get_project(project_id)? {
        let sidebar_v2_selected = session_lifecycle::read_sidebar_v2_selected(&state.paths);
        project_git_remote::ensure_published_project_git_remote_probed(
            &project,
            sidebar_v2_selected,
        );
        /*
        CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
        The project's own icon rides the same first-sighting warm, for the same
        reason and under the same gate: a project should reach the sidebar with
        its repository's icon already on it instead of showing a folder glyph
        until the next background pass. Same bounded cost — the warm reads the
        candidate list once per NEW family root and never again.
        */
        project_icon::ensure_published_project_icon_probed(&project, sidebar_v2_selected);
    }
    let _event_sequence = lock_presentation_event_sequence(state)?;
    let delta = build_presentation_project_delta(repository, project_id, delta_type)?;
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "delta": delta,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "type": "presentationDelta",
    }));
    Ok(())
}

fn schedule_presentation_session_delta(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> std::result::Result<(), DomainStateError> {
    {
        let _event_sequence = lock_presentation_event_sequence(state)?;
        let delta = build_presentation_session_delta(repository, project_id, session_id)?;
        let revision = increment_presentation_revision(db)?;
        state.event_hub.broadcast(json!({
            "delta": delta,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "revision": revision,
            "serverId": state.metadata.server_id.clone(),
            "type": "presentationDelta",
        }));
    }
    match repository.get_session(project_id, session_id) {
        Ok(Some(session)) => {
            sync_zmx_title_observer_for_session(state, &session, "presentation-session-delta")
        }
        _ => stop_zmx_title_observer(state, project_id, session_id, "session-removed"),
    }
    Ok(())
}

fn lock_presentation_event_sequence(
    state: &AppState,
) -> std::result::Result<std::sync::MutexGuard<'_, ()>, DomainStateError> {
    /*
    Every producer shares this short critical section for state projection,
    revision allocation, and event publication. That makes revision order and
    broadcast order identical, including compensating removal/repair deltas,
    while process work and durable lifecycle mutations remain outside it.
    */
    state
        .presentation_event_sequence
        .lock()
        .map_err(|_| DomainStateError::corrupt_state("Presentation event sequencer is poisoned."))
}

fn read_presentation_snapshot_in_sequence(
    state: &AppState,
    db: &rusqlite::Connection,
    server_id: &str,
) -> std::result::Result<Value, DomainStateError> {
    /*
    Snapshot callers own the sequencer exactly once, after any repair pass
    that may publish deltas. Hold it across every projection read, including
    the final revision read inside read_presentation_snapshot, so a producer
    cannot label stale rows with the revision for a delta it just published.
    */
    let auto_settle_after_days = session_lifecycle::read_sweep_auto_settle_after_days(&state.paths);
    let sidebar_v2_selected = session_lifecycle::read_sidebar_v2_selected(&state.paths);
    let _event_sequence = lock_presentation_event_sequence(state)?;
    read_presentation_snapshot(db, server_id, auto_settle_after_days, sidebar_v2_selected)
}

fn schedule_stale_activity_presentation_refresh(state: &AppState, session: &Value, _reason: &str) {
    let Some(project_id) = read_session_text(session, "projectId") else {
        return;
    };
    let Some(session_id) = read_session_text(session, "sessionId") else {
        return;
    };
    let key = format!("{project_id}/{session_id}");
    let delay_ms = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
        .and_then(|activity| {
            agent_activity_presentation_refresh_delay_ms(Some(activity), now_ms())
        });
    let Ok(mut timers) = state.stale_activity_timers.lock() else {
        return;
    };
    if let Some(handle) = timers.remove(&key) {
        handle.abort();
    }
    let Some(delay_ms) = delay_ms else {
        return;
    };
    /*
    CDXC:SessionStatus 2026-06-21-19:26:
    Rust event streams must match TypeScript gxserver's stale title-derived working refresh. zmx may not emit another terminal-title event after a spinner freezes, so schedule one presentation delta at the projection boundary without rewriting durable activity state or re-scheduling from the timer callback.

    CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
    The same timer also fires when an ongoing working stint crosses the
    meaningful-activity threshold, so recency sorting can promote a genuinely
    busy session even when no further hook or title event arrives; the
    combined deadline comes from agent_activity_presentation_refresh_delay_ms.
    */
    let state = state.clone();
    let timer_key = key.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms.max(0) as u64 + 25)).await;
        if let Ok(mut timers) = state.stale_activity_timers.lock() {
            timers.remove(&timer_key);
        }
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let _ =
            schedule_presentation_session_delta(&state, &db, &repository, &project_id, &session_id);
    });
    timers.insert(key, handle);
}

fn sync_zmx_title_observers_for_all_sessions(state: &AppState, reason: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions(None) else {
        return;
    };
    let mut desired_keys = HashSet::new();
    for session in sessions {
        if let (Some(project_id), Some(session_id)) = (
            read_session_text(&session, "projectId"),
            read_session_text(&session, "sessionId"),
        ) {
            if is_zmx_title_observable_session(&session) {
                desired_keys.insert(session_observer_key(&project_id, &session_id));
            }
        }
        sync_zmx_title_observer_for_session(state, &session, reason);
    }
    let existing_keys = state
        .zmx_title_observers
        .lock()
        .map(|observers| observers.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for key in existing_keys {
        if !desired_keys.contains(&key) {
            stop_zmx_title_observer_by_key(state, &key, "session-no-longer-observable");
        }
    }
}

fn sync_zmx_title_observer_for_session(state: &AppState, session: &Value, reason: &str) {
    let Some(project_id) = read_session_text(session, "projectId") else {
        return;
    };
    let Some(session_id) = read_session_text(session, "sessionId") else {
        return;
    };
    if !is_zmx_title_observable_session(session) {
        stop_zmx_title_observer(state, &project_id, &session_id, reason);
        return;
    }
    let Some(zmx_name) = read_session_text(session, "zmxName") else {
        return;
    };
    let key = session_observer_key(&project_id, &session_id);
    let Ok(mut observers) = state.zmx_title_observers.lock() else {
        return;
    };
    if let Some(existing) = observers.get(&key) {
        if existing.zmx_name == zmx_name && !existing.handle.is_finished() {
            return;
        }
    }
    if let Some(existing) = observers.remove(&key) {
        existing.handle.abort();
    }
    /*
    CDXC:ZmxTitleObservations 2026-06-21-22:23:
    Rust gxserver must own the same zmx title-observation loop as TypeScript gxserver. Native deliberately avoids forwarding every zmx title frame, so without `zmx watch-title` the Rust daemon can keep Codex sessions stuck in working after hooks miss Stop or title-derived spinner state needs to expire.
    */
    let state = state.clone();
    let handle = tokio::spawn(run_zmx_title_observer(
        state,
        project_id,
        session_id,
        zmx_name.clone(),
    ));
    observers.insert(key, ZmxTitleObserverTask { handle, zmx_name });
}

fn stop_zmx_title_observer(state: &AppState, project_id: &str, session_id: &str, reason: &str) {
    stop_zmx_title_observer_by_key(state, &session_observer_key(project_id, session_id), reason);
}

fn stop_zmx_title_observer_by_key(state: &AppState, key: &str, _reason: &str) {
    if let Ok(mut observers) = state.zmx_title_observers.lock() {
        if let Some(existing) = observers.remove(key) {
            existing.handle.abort();
        }
    }
}

fn stop_all_zmx_title_observers(state: &AppState) {
    if let Ok(mut observers) = state.zmx_title_observers.lock() {
        for (_, existing) in observers.drain() {
            existing.handle.abort();
        }
    }
}

const ZMX_TITLE_OBSERVER_HEALTHY_WATCH_DURATION: Duration = Duration::from_secs(5);

async fn run_zmx_title_observer(
    state: AppState,
    project_id: String,
    session_id: String,
    zmx_name: String,
) {
    let mut failure_count = 0usize;
    loop {
        let zmx = match require_bundled_zmx() {
            Ok(zmx) => zmx,
            Err(_) => {
                failure_count += 1;
                delay_zmx_title_observer_retry(failure_count).await;
                continue;
            }
        };
        let mut child = match Command::new(&zmx.executable_path)
            .args(["watch-title", zmx_name.as_str()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(_) => {
                failure_count += 1;
                delay_zmx_title_observer_retry(failure_count).await;
                continue;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        /*
        watch-title spawns successfully even when the zmx session no longer
        exists, then exits immediately with no output. A spawn therefore does
        not prove a healthy watch: reset the retry ladder only after the watch
        produced output or stayed alive for a while, so dead sessions back off
        instead of respawning several times per second forever.
        */
        let watch_started_at = Instant::now();
        let mut observed_output = false;
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    observed_output = true;
                    if let Some(title) = parse_zmx_title_line(&line) {
                        ingest_zmx_title_observation(&state, &project_id, &session_id, &title);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        let _ = child.wait().await;
        if observed_output
            || watch_started_at.elapsed() >= ZMX_TITLE_OBSERVER_HEALTHY_WATCH_DURATION
        {
            failure_count = 0;
        }
        failure_count += 1;
        delay_zmx_title_observer_retry(failure_count).await;
    }
}

async fn delay_zmx_title_observer_retry(failure_count: usize) {
    /*
    The observer owns its session until sync stops it, so it must never exit
    on failures: a finished task handle makes every presentation delta respawn
    a fresh fast-retry cycle for the same dead session. Cap the backoff and
    keep waiting instead.
    */
    const DELAYS_MS: [u64; 6] = [250, 500, 1_000, 2_000, 5_000, 60_000];
    let delay_index = failure_count.saturating_sub(1).min(DELAYS_MS.len() - 1);
    tokio::time::sleep(Duration::from_millis(DELAYS_MS[delay_index])).await;
}

fn ingest_zmx_title_observation(state: &AppState, project_id: &str, session_id: &str, title: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("rawTitle".to_string(), json!(title));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("sessionPersistenceProvider".to_string(), json!("zmx"));
    let Ok(output) = dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        "/api/ingestTerminalTitleEvent",
        &params,
        None,
    ) else {
        return;
    };
    if let Some((project_id, session_id)) = output.presentation_session {
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id);
        if let Ok(Some(session)) = repository.get_session(&project_id, &session_id) {
            schedule_stale_activity_presentation_refresh(
                state,
                &session,
                "terminal-title-stale-activity",
            );
        }
    }
}

fn is_zmx_title_observable_session(session: &Value) -> bool {
    let kind = read_session_text(session, "kind");
    let lifecycle = read_session_text(session, "lifecycleState");
    let provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let provider_lifecycle = provider_state
        .get("lifecycleState")
        .and_then(Value::as_str)
        .map(str::trim);
    let provider_text = provider_state
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| read_runtime_text(session, "sessionPersistenceProvider"));
    matches!(kind.as_deref(), Some("terminal" | "agent"))
        && lifecycle.as_deref() == Some("running")
        && provider_lifecycle == Some("exists")
        && matches!(provider_text.as_deref(), None | Some("zmx"))
        && read_session_text(session, "zmxName").is_some()
}

fn parse_zmx_title_line(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line).ok().and_then(|value| {
        value
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    })
}

fn session_observer_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}/{session_id}")
}

fn sync_zmx_provider_existence(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: Option<&str>,
) -> std::result::Result<(), DomainStateError> {
    let sessions = repository.list_sessions(project_id)?;
    let candidates = sessions
        .iter()
        .filter(|session| {
            session.get("lifecycleState").and_then(Value::as_str) == Some("running")
                && session
                    .get("providerState")
                    .and_then(Value::as_object)
                    .and_then(|provider| provider.get("lifecycleState"))
                    .and_then(Value::as_str)
                    == Some("exists")
                && read_session_persistence_provider(session).as_deref() == Some("zmx")
                && session
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(|created_at| chrono::DateTime::parse_from_rfc3339(created_at).ok())
                    .is_some_and(|created_at| {
                        chrono::Utc::now()
                            .signed_duration_since(created_at.with_timezone(&chrono::Utc))
                            >= chrono::Duration::seconds(30)
                    })
        })
        .filter_map(|session| {
            Some((
                read_session_text(session, "projectId")?,
                read_session_text(session, "sessionId")?,
                read_session_text(session, "zmxName")?,
            ))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(());
    }
    let Ok(existing_names) = read_zmx_existing_session_names() else {
        return Ok(());
    };
    for (candidate_project_id, candidate_session_id, zmx_name) in candidates {
        if existing_names.contains(&zmx_name) {
            continue;
        }
        let Some(current) = repository.get_session(&candidate_project_id, &candidate_session_id)?
        else {
            continue;
        };
        let mut provider_state = current
            .get("providerState")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if provider_state.get("lifecycleState").and_then(Value::as_str) != Some("exists") {
            continue;
        }
        provider_state.remove("killError");
        provider_state.remove("probeError");
        provider_state.insert("lifecycleState".to_string(), json!("missing"));
        provider_state.insert("probedAt".to_string(), json!(now_iso()));
        provider_state.insert("zmxName".to_string(), json!(zmx_name));
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(candidate_project_id.clone()));
        update.insert("sessionId".to_string(), json!(candidate_session_id.clone()));
        update.insert(
            "lifecycleState".to_string(),
            json!(
                if current.get("surface").and_then(Value::as_str) == Some("commands") {
                    "stopped"
                } else {
                    current
                        .get("lifecycleState")
                        .and_then(Value::as_str)
                        .unwrap_or("running")
                }
            ),
        );
        update.insert("providerState".to_string(), Value::Object(provider_state));
        repository.update_session_for_lifecycle(&update)?;
        schedule_presentation_session_delta(
            state,
            db,
            repository,
            &candidate_project_id,
            &candidate_session_id,
        )?;
    }
    Ok(())
}

fn sync_live_zmx_process_identities(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: Option<&str>,
    _reason: &str,
) -> std::result::Result<(), DomainStateError> {
    let sessions = repository.list_sessions(project_id)?;
    let candidates = sessions
        .iter()
        .filter(|session| should_sync_live_zmx_process_identity(session))
        .filter_map(|session| {
            Some((
                read_session_text(session, "projectId")?,
                read_session_text(session, "sessionId")?,
                read_session_text(session, "zmxName")?,
            ))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(());
    }
    let session_names = candidates
        .iter()
        .map(|(_, _, zmx_name)| zmx_name.clone())
        .collect::<Vec<_>>();
    let Ok(identities) = read_zmx_session_process_identities(&session_names) else {
        return Ok(());
    };
    for (candidate_project_id, candidate_session_id, _) in candidates {
        let Some(current) = repository.get_session(&candidate_project_id, &candidate_session_id)?
        else {
            continue;
        };
        if !should_sync_live_zmx_process_identity(&current) {
            continue;
        }
        let Some(zmx_name) = read_session_text(&current, "zmxName") else {
            continue;
        };
        let Some(identity) = identities.get(&zmx_name) else {
            continue;
        };
        if identity.agent_id.is_none() {
            continue;
        }
        /*
        CDXC:GxserverSessionIdentity 2026-06-21-18:25:
        Rust must copy TypeScript gxserver's live zmx process repair before sidebar list/snapshot responses. A running zmx terminal whose foreground process is Codex/Claude/etc. must be promoted to the matching agent row in durable state so macOS shows the same session identity after the gxserver-rs cutover.
        */
        let changed = apply_live_process_session_identity(
            repository,
            &candidate_project_id,
            &candidate_session_id,
            identity.agent_id.clone(),
            identity.agent_session_id.clone(),
            identity.agent_session_path.clone(),
        )?;
        let reconciled = if changed {
            reconcile_agent_metadata_title_for_session(
                repository,
                &candidate_project_id,
                &candidate_session_id,
                &state.paths.home_dir,
                "pending",
            )
            .unwrap_or(false)
        } else {
            false
        };
        if changed || reconciled {
            schedule_presentation_session_delta(
                state,
                db,
                repository,
                &candidate_project_id,
                &candidate_session_id,
            )?;
        }
    }
    Ok(())
}

/*
CDXC:MobileSessionStatus 2026-06-22-00:47:
Rust gxserver must read the same hook sidecar files as TypeScript before list, snapshot, and subscription projections. Hooks can write provider session id and working/idle state even when an HTTP hook POST is missed, so presentation state must ingest those sidecars at the daemon boundary instead of leaving Codex rows idle or unresumable.
*/
fn sync_session_state_sidecars(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: Option<&str>,
    _reason: &str,
) -> std::result::Result<(), DomainStateError> {
    for session in repository.list_sessions(project_id)? {
        if !should_sync_session_state_sidecar(&session) {
            continue;
        }
        let Some(session_project_id) = read_session_text(&session, "projectId") else {
            continue;
        };
        let Some(session_id) = read_session_text(&session, "sessionId") else {
            continue;
        };
        let Some(sidecar) =
            read_session_state_sidecar(&state.paths, &session_project_id, &session_id)
        else {
            continue;
        };
        if !has_session_state_sidecar_payload(&sidecar) {
            continue;
        }
        let changed = apply_session_state_sidecar(
            state,
            db,
            repository,
            &session_project_id,
            &session_id,
            &sidecar,
        )?;
        if changed {
            schedule_presentation_session_delta(
                state,
                db,
                repository,
                &session_project_id,
                &session_id,
            )?;
        }
    }
    Ok(())
}

fn should_sync_session_state_sidecar(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && session.get("surface").and_then(Value::as_str) != Some("commands")
}

#[derive(Default)]
struct SessionStateSidecar {
    agent_name: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    first_user_message: Option<String>,
    last_activity_at: Option<String>,
    pending_first_prompt_auto_rename_prompt: Option<String>,
    status: Option<String>,
    status_updated_at: Option<String>,
    title: Option<String>,
}

fn read_session_state_sidecar(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> Option<SessionStateSidecar> {
    let path = build_session_state_sidecar_path(paths, project_id, session_id);
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    parse_session_state_sidecar(&raw)
}

fn build_session_state_sidecar_path(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> PathBuf {
    paths
        .root_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("session-state")
        .join(sanitize_session_state_sidecar_path_part(project_id))
        .join(format!(
            "{}.env",
            sanitize_session_state_sidecar_path_part(session_id)
        ))
}

fn sanitize_session_state_sidecar_path_part(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        let keep = ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-');
        if keep {
            output.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            output.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed
    }
}

fn parse_session_state_sidecar(raw: &str) -> Option<SessionStateSidecar> {
    let mut sidecar = SessionStateSidecar::default();
    for line in raw.lines() {
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let value = if matches!(key, "firstUserMessageBase64" | "agentSessionPath") {
            raw_value.trim().to_string()
        } else {
            raw_value.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        if value.is_empty() {
            continue;
        }
        match key {
            "agent" => sidecar.agent_name = Some(value),
            "agentSessionId" => sidecar.agent_session_id = Some(value),
            "agentSessionPath" => sidecar.agent_session_path = Some(value),
            "firstUserMessageBase64" => {
                sidecar.first_user_message = decode_session_state_sidecar_base64(&value);
            }
            "lastActivityAt" => sidecar.last_activity_at = normalize_sidecar_iso_timestamp(&value),
            "pendingFirstPromptAutoRenamePrompt" => {
                sidecar.pending_first_prompt_auto_rename_prompt = Some(value);
            }
            "status" if matches!(value.as_str(), "attention" | "idle" | "working") => {
                sidecar.status = Some(value);
            }
            "statusUpdatedAt" => {
                sidecar.status_updated_at = normalize_sidecar_iso_timestamp(&value);
            }
            "title" => sidecar.title = get_visible_terminal_title(&value),
            _ => {}
        }
    }
    if sidecar.first_user_message.is_none() {
        sidecar.first_user_message = sidecar.pending_first_prompt_auto_rename_prompt.clone();
    }
    Some(sidecar)
}

fn has_session_state_sidecar_payload(sidecar: &SessionStateSidecar) -> bool {
    sidecar.agent_name.is_some()
        || sidecar.agent_session_id.is_some()
        || sidecar.agent_session_path.is_some()
        || sidecar.first_user_message.is_some()
        || sidecar.status.is_some()
        || sidecar.title.is_some()
}

fn decode_session_state_sidecar_base64(value: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let decoded = STANDARD.decode(value).ok()?;
    String::from_utf8(decoded)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn normalize_sidecar_iso_timestamp(value: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| {
            timestamp
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
}

fn apply_session_state_sidecar(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    sidecar: &SessionStateSidecar,
) -> std::result::Result<bool, DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    insert_optional_json_string(&mut params, "agentName", sidecar.agent_name.as_deref());
    insert_optional_json_string(
        &mut params,
        "agentSessionId",
        sidecar.agent_session_id.as_deref(),
    );
    insert_optional_json_string(
        &mut params,
        "agentSessionPath",
        sidecar.agent_session_path.as_deref(),
    );
    insert_optional_json_string(
        &mut params,
        "firstUserMessage",
        sidecar.first_user_message.as_deref(),
    );
    insert_optional_json_string(&mut params, "status", sidecar.status.as_deref());
    insert_optional_json_string(
        &mut params,
        "statusUpdatedAt",
        sidecar
            .status_updated_at
            .as_deref()
            .or(sidecar.last_activity_at.as_deref()),
    );
    insert_optional_json_string(&mut params, "title", sidecar.title.as_deref());
    params.insert(
        "eventName".to_string(),
        Value::String("legacy-session-state".to_string()),
    );
    let output = dispatch_agent_endpoint(
        repository,
        db,
        &state.paths.home_dir,
        "/api/ingestAgentHookEvent",
        &params,
        None,
    )
    .map_err(|error| match error {
        AgentEndpointError::Domain(error) => error,
        AgentEndpointError::DependencyUnavailable(message) => DomainStateError {
            code: "dependencyUnavailable",
            message,
        },
    })?;
    let changed = output
        .result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if changed {
        if let Some(claimed) = output
            .result
            .get("reason")
            .and_then(Value::as_str)
            .filter(|reason| *reason == "first-prompt-auto-title-claimed")
        {
            let _ = claimed;
            schedule_first_prompt_auto_title_job(
                state.clone(),
                project_id.to_string(),
                session_id.to_string(),
            );
        }
        if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
            schedule_stale_activity_presentation_refresh(
                state,
                &session,
                "session-state-sidecar-stale-activity",
            );
        }
    }
    Ok(changed)
}

fn insert_optional_json_string(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn should_sync_live_zmx_process_identity(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && session.get("surface").and_then(Value::as_str) != Some("commands")
        && read_session_persistence_provider(session).as_deref() == Some("zmx")
}

/*
CDXC:GxserverSessionIdentity 2026-06-30-11:15:
Live process identity repair must use the same persistence-provider source as
presentation rows. Remote/attached zmx sessions can have providerState.provider
without runtimeSettings.sessionPersistenceProvider, and those rows still need
server-side Codex/Claude/etc. promotion so sidebar agent icons come from
canonical gxserver metadata instead of client guesses.
*/
fn read_session_persistence_provider(session: &Value) -> Option<String> {
    read_runtime_text(session, "sessionPersistenceProvider").or_else(|| {
        session
            .get("providerState")
            .and_then(Value::as_object)
            .and_then(|state| state.get("provider"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn read_runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_session_text(session: &Value, key: &str) -> Option<String> {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sync_t3_object_field(value: &Value, key: &str) -> Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn sync_t3_param_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    read_text_from_map(params, key)
        .filter(|value| value.chars().count() <= 1024 && !value.chars().any(char::is_control))
}

fn sync_t3_required_param_text(
    params: &Map<String, Value>,
    key: &str,
) -> std::result::Result<String, DomainStateError> {
    sync_t3_param_text(params, key)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}

fn sync_t3_metadata_text(
    runtime_t3: &Map<String, Value>,
    provider_t3: &Map<String, Value>,
    key: &str,
) -> Option<String> {
    read_text_from_map(runtime_t3, key).or_else(|| read_text_from_map(provider_t3, key))
}

fn sync_t3_thread_is_placeholder(thread_id: &str) -> bool {
    let normalized = thread_id.trim().to_ascii_lowercase();
    normalized.starts_with("ghostex-thread-")
        || normalized.starts_with("ghostex-draft-")
        || normalized.starts_with("pending-")
}

fn sync_t3_normalize_activity(value: Option<&Value>) -> Option<&'static str> {
    match value.and_then(Value::as_str).map(str::trim) {
        Some("attention") => Some("attention"),
        Some("idle") => Some("idle"),
        Some("working") => Some("working"),
        _ => None,
    }
}

fn sync_t3_normalize_lifecycle(value: Option<&Value>) -> Option<&'static str> {
    match value.and_then(Value::as_str).map(str::trim) {
        Some("missing") => Some("missing"),
        Some("running") => Some("running"),
        Some("sleeping") => Some("sleeping"),
        Some("stopped") => Some("stopped"),
        Some("unknown") => Some("unknown"),
        _ => None,
    }
}

fn sync_t3_sidebar_mode(value: Option<&Value>) -> &'static str {
    match value.and_then(Value::as_str).map(str::trim) {
        Some("normal") => "normal",
        _ => "collapsed",
    }
}

fn sync_t3_normalize_title(value: Option<&Value>) -> Option<String> {
    let normalized = value
        .and_then(Value::as_str)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() || normalized.chars().any(char::is_control) {
        return None;
    }
    let lower = normalized.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "t3 code" | "t3 code (alpha)" | "no active thread" | "pick a thread to continue"
    ) {
        return None;
    }
    Some(normalized.chars().take(240).collect())
}

fn sync_t3_agent_activity(activity: &str, previous: Option<&Value>) -> Value {
    let now = now_iso();
    let previous_seen_working = previous
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("hasSeenWorking"))
        .and_then(Value::as_bool)
        == Some(true);
    json!({
        "activity": activity,
        "hasSeenWorking": previous_seen_working || activity == "working",
        "isAcknowledged": activity != "attention",
        "lastChangedAt": now,
        "suppressedUntil": now,
    })
}

fn sync_t3_embedded_session(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    /*
    CDXC:T3SessionOwnership 2026-07-01-02:17:
    Embedded T3 sync updates exactly one existing Ghostex `kind: "t3"` row by Ghostex project/session id. Do not create fallback rows from T3 thread ids; validate the workspace/thread binding and then update only provider metadata, lifecycle/activity, title provenance, and cleanup-safe markers.
    */
    let ghostex_project_id = sync_t3_required_param_text(params, "ghostexProjectId")?;
    let ghostex_session_id = sync_t3_required_param_text(params, "ghostexSessionId")?;
    if !is_gxserver_project_id(&ghostex_project_id) || !is_gxserver_session_id(&ghostex_session_id)
    {
        return Err(DomainStateError::bad_request(
            "Ghostex T3 sync target identity is invalid.",
        ));
    }
    let current = repository
        .get_session(&ghostex_project_id, &ghostex_session_id)?
        .ok_or_else(|| DomainStateError::not_found("Ghostex T3 session does not exist."))?;
    if read_session_text(&current, "kind").as_deref() != Some("t3") {
        return Err(DomainStateError::bad_request(
            "Ghostex T3 sync target must be a T3 session.",
        ));
    }

    let mut runtime_settings = sync_t3_object_field(&current, "runtimeSettings");
    let mut provider_state = sync_t3_object_field(&current, "providerState");
    let mut runtime_t3 = runtime_settings
        .get("t3")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut provider_t3 = provider_state
        .get("t3")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let incoming_workspace_root = sync_t3_param_text(params, "workspaceRoot");
    let current_workspace_root = sync_t3_metadata_text(&runtime_t3, &provider_t3, "workspaceRoot")
        .or_else(|| read_session_text(&current, "cwd"));
    if let (Some(current_workspace_root), Some(incoming_workspace_root)) =
        (&current_workspace_root, &incoming_workspace_root)
    {
        if current_workspace_root != incoming_workspace_root {
            return Err(DomainStateError::bad_request(
                "T3 sync workspace does not match the Ghostex session.",
            ));
        }
    }
    let workspace_root = incoming_workspace_root.or(current_workspace_root);

    let incoming_t3_project_id = sync_t3_param_text(params, "t3ProjectId")
        .or_else(|| sync_t3_param_text(params, "projectId"));
    let current_t3_project_id = sync_t3_metadata_text(&runtime_t3, &provider_t3, "projectId");
    if let (Some(current_t3_project_id), Some(incoming_t3_project_id)) =
        (&current_t3_project_id, &incoming_t3_project_id)
    {
        if current_t3_project_id != incoming_t3_project_id {
            return Err(DomainStateError::bad_request(
                "T3 sync project does not match the Ghostex session binding.",
            ));
        }
    }
    let t3_project_id = incoming_t3_project_id.or(current_t3_project_id);

    let incoming_thread_id = sync_t3_param_text(params, "threadId");
    let current_thread_id = sync_t3_metadata_text(&runtime_t3, &provider_t3, "boundThreadId")
        .or_else(|| sync_t3_metadata_text(&runtime_t3, &provider_t3, "threadId"));
    let allow_rebind = params.get("allowThreadRebind").and_then(Value::as_bool) == Some(true);
    if let (Some(current_thread_id), Some(incoming_thread_id)) =
        (&current_thread_id, &incoming_thread_id)
    {
        if current_thread_id != incoming_thread_id
            && !allow_rebind
            && !sync_t3_thread_is_placeholder(current_thread_id)
        {
            return Err(DomainStateError::bad_request(
                "T3 sync thread does not match the Ghostex session binding.",
            ));
        }
    }
    let thread_id = incoming_thread_id.or(current_thread_id).ok_or_else(|| {
        DomainStateError::bad_request("T3 sync requires thread binding metadata.")
    })?;

    runtime_t3.insert(
        "ghostexProjectId".to_string(),
        Value::String(ghostex_project_id.clone()),
    );
    runtime_t3.insert(
        "ghostexSessionId".to_string(),
        Value::String(ghostex_session_id.clone()),
    );
    provider_t3.insert(
        "ghostexProjectId".to_string(),
        Value::String(ghostex_project_id.clone()),
    );
    provider_t3.insert(
        "ghostexSessionId".to_string(),
        Value::String(ghostex_session_id.clone()),
    );
    if let Some(t3_project_id) = t3_project_id {
        runtime_t3.insert(
            "projectId".to_string(),
            Value::String(t3_project_id.clone()),
        );
        provider_t3.insert("projectId".to_string(), Value::String(t3_project_id));
    }
    if let Some(server_origin) = sync_t3_param_text(params, "serverOrigin")
        .or_else(|| sync_t3_metadata_text(&runtime_t3, &provider_t3, "serverOrigin"))
    {
        runtime_t3.insert(
            "serverOrigin".to_string(),
            Value::String(server_origin.clone()),
        );
        provider_t3.insert("serverOrigin".to_string(), Value::String(server_origin));
    }
    if let Some(environment_id) = sync_t3_param_text(params, "environmentId")
        .or_else(|| sync_t3_metadata_text(&runtime_t3, &provider_t3, "environmentId"))
    {
        runtime_t3.insert(
            "environmentId".to_string(),
            Value::String(environment_id.clone()),
        );
        provider_t3.insert("environmentId".to_string(), Value::String(environment_id));
    }
    runtime_t3.insert(
        "boundThreadId".to_string(),
        Value::String(thread_id.clone()),
    );
    runtime_t3.insert("threadId".to_string(), Value::String(thread_id.clone()));
    provider_t3.insert(
        "boundThreadId".to_string(),
        Value::String(thread_id.clone()),
    );
    provider_t3.insert("threadId".to_string(), Value::String(thread_id));
    if let Some(workspace_root) = workspace_root {
        runtime_t3.insert(
            "workspaceRoot".to_string(),
            Value::String(workspace_root.clone()),
        );
        provider_t3.insert("workspaceRoot".to_string(), Value::String(workspace_root));
    }
    if let Some(created_at) = sync_t3_param_text(params, "createdAt")
        .or_else(|| sync_t3_metadata_text(&runtime_t3, &provider_t3, "createdAt"))
    {
        runtime_t3.insert("createdAt".to_string(), Value::String(created_at.clone()));
        provider_t3.insert("createdAt".to_string(), Value::String(created_at));
    }
    runtime_t3.insert(
        "createdBy".to_string(),
        Value::String("ghostex-embedded".to_string()),
    );
    provider_t3.insert(
        "createdBy".to_string(),
        Value::String("ghostex-embedded".to_string()),
    );
    let sidebar_mode = sync_t3_sidebar_mode(params.get("t3SidebarMode"));
    runtime_t3.insert(
        "t3SidebarMode".to_string(),
        Value::String(sidebar_mode.to_string()),
    );
    provider_t3.insert(
        "t3SidebarMode".to_string(),
        Value::String(sidebar_mode.to_string()),
    );

    runtime_settings.insert("provider".to_string(), Value::String("t3code".to_string()));
    runtime_settings.insert("t3".to_string(), Value::Object(runtime_t3));
    provider_state.insert("provider".to_string(), Value::String("t3code".to_string()));
    provider_state.insert("t3".to_string(), Value::Object(provider_t3));
    if let Some(lifecycle) = sync_t3_normalize_lifecycle(params.get("lifecycleState")) {
        provider_state.insert(
            "lifecycleState".to_string(),
            Value::String(
                if lifecycle == "stopped" {
                    "missing"
                } else {
                    "unknown"
                }
                .to_string(),
            ),
        );
    }
    if let Some(title_source) = sync_t3_param_text(params, "titleSource") {
        runtime_settings.insert("titleSource".to_string(), Value::String(title_source));
    }
    if let Some(activity) = sync_t3_normalize_activity(params.get("activity")) {
        let previous = runtime_settings.get("agentActivity").cloned();
        runtime_settings.insert(
            "agentActivity".to_string(),
            sync_t3_agent_activity(activity, previous.as_ref()),
        );
    }

    let mut update = Map::new();
    update.insert("kind".to_string(), Value::String("t3".to_string()));
    update.insert("projectId".to_string(), Value::String(ghostex_project_id));
    update.insert("sessionId".to_string(), Value::String(ghostex_session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update.insert("providerState".to_string(), Value::Object(provider_state));
    if let Some(lifecycle) = sync_t3_normalize_lifecycle(params.get("lifecycleState")) {
        update.insert(
            "lifecycleState".to_string(),
            Value::String(lifecycle.to_string()),
        );
    }
    if let Some(title) = sync_t3_normalize_title(params.get("title")) {
        update.insert("title".to_string(), Value::String(title));
    }
    repository.update_session(&update)
}

fn result_activity(result: &Value) -> Option<&str> {
    result
        .get("activity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str)
}

fn log_agent_hook_passive_identity_conflict(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if endpoint_path != "/api/ingestAgentHookEvent"
        || result.get("reason").and_then(Value::as_str) != Some("passive-session-identity-conflict")
    {
        return;
    }
    let Some(conflict) = result.get("identityConflict").and_then(Value::as_object) else {
        return;
    };
    if conflict.get("source").and_then(Value::as_str) != Some("passive") {
        return;
    }
    let agent_id = conflict.get("agentId").and_then(Value::as_str);
    let current_agent_session_id = conflict
        .get("currentAgentSessionId")
        .and_then(Value::as_str);
    let incoming_agent_session_id = conflict
        .get("incomingAgentSessionId")
        .and_then(Value::as_str);
    let reason = conflict.get("reason").and_then(Value::as_str);
    let source = conflict.get("source").and_then(Value::as_str);
    /*
    CDXC:AgentHooks 2026-06-22-08:31:
    Passive hook identity conflicts need the same support-bundle evidence as TypeScript without exposing thread ids, paths, titles, prompts, or hook payloads. Hash private agent-session ids at the writer boundary and log only enum fields, stable gxserver ids, and payload-shape booleans.
    */
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Debug,
        event: "sessionIdentity.updateBlocked".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "agentId": agent_id,
            "currentAgentSessionIdHash": hash_log_identity(current_agent_session_id),
            "currentAgentSessionIdPresent": current_agent_session_id.is_some(),
            "incomingAgentSessionIdHash": hash_log_identity(incoming_agent_session_id),
            "incomingAgentSessionIdPresent": incoming_agent_session_id.is_some(),
            "ownerProjectId": conflict.get("ownerProjectId").cloned(),
            "ownerSessionId": conflict.get("ownerSessionId").cloned(),
            "projectId": params.get("projectId").and_then(Value::as_str),
            "reason": reason,
            "sessionId": params.get("sessionId").and_then(Value::as_str),
            "source": source,
        })),
    });
    let hook_activity = normalize_agent_hook_activity(
        params.get("status"),
        params
            .get("eventName")
            .or_else(|| params.get("rawEventName")),
        params.get("agentName"),
    );
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionIdentity.passiveEventRejected".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "activity": hook_activity,
            "agentId": agent_id,
            "currentAgentSessionIdHash": hash_log_identity(current_agent_session_id),
            "currentAgentSessionIdPresent": current_agent_session_id.is_some(),
            "hasAgentSessionId": params.get("agentSessionId").is_some(),
            "hasAgentSessionPath": params.get("agentSessionPath").is_some(),
            "hasExplicitStatus": params.get("status").is_some(),
            "hasFirstUserMessage": params.get("firstUserMessage").is_some(),
            "hasHookEventName": params.get("eventName").is_some() || params.get("rawEventName").is_some(),
            "hasTitle": params.get("title").is_some(),
            "identitySource": source,
            "incomingAgentSessionIdHash": hash_log_identity(incoming_agent_session_id),
            "incomingAgentSessionIdPresent": incoming_agent_session_id.is_some(),
            "ownerProjectId": conflict.get("ownerProjectId").cloned(),
            "ownerSessionId": conflict.get("ownerSessionId").cloned(),
            "projectId": params.get("projectId").and_then(Value::as_str),
            "reason": reason,
            "sessionId": params.get("sessionId").and_then(Value::as_str),
            "source": "agent-hook-event",
        })),
    });
}

fn strip_agent_hook_internal_result_fields(endpoint_path: &str, result: &mut Value) {
    if endpoint_path != "/api/ingestAgentHookEvent" {
        return;
    }
    if let Some(object) = result.as_object_mut() {
        object.remove("identityConflict");
    }
}

fn hash_log_identity(value: Option<&str>) -> Option<String> {
    let value = value?;
    let digest = Sha256::digest(value.as_bytes());
    Some(
        digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
}

fn should_schedule_agent_title_metadata_check(endpoint_path: &str, result: &Value) -> bool {
    match endpoint_path {
        "/api/requestSessionRename" => {
            result.get("pendingAgentMetadata").and_then(Value::as_bool) == Some(true)
        }
        "/api/ingestSessionStateEvent" => {
            result.get("reason").and_then(Value::as_str)
                != Some("passive-session-identity-conflict")
                && result.get("reason").and_then(Value::as_str)
                    != Some("session-state-agent-mismatch")
        }
        "/api/ingestTerminalTitleEvent" => {
            result.get("changed").and_then(Value::as_bool) == Some(true)
        }
        "/api/ingestAgentHookEvent" => !matches!(
            result.get("reason").and_then(Value::as_str),
            Some("agent-hook-agent-mismatch" | "passive-session-identity-conflict")
        ),
        "/api/updateAgentActivity" => true,
        _ => false,
    }
}

fn stale_activity_refresh_reason(endpoint_path: &str) -> &'static str {
    match endpoint_path {
        "/api/ingestAgentHookEvent" => "agent-hook-stale-activity",
        "/api/ingestTerminalTitleEvent" => "terminal-title-stale-activity",
        _ => "agent-activity-stale-activity",
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn broadcast_server_stopping(state: &AppState) {
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": "serverStopping",
    }));
}

fn value_text(value: &Value, key: &str) -> std::result::Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::corrupt_state(format!("{key} missing from gxserver response state."))
        })
}

async fn handle_events(
    State(state): State<Arc<AppState>>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
) -> Response<Body> {
    let request_id = request_id(&headers);
    let Ok(ws) = ws else {
        let message = if method == Method::OPTIONS {
            format!("{} is not a gxserver HTTP endpoint.", uri.path())
        } else {
            format!(
                "No gxserver endpoint for {} {}.",
                method.as_str(),
                uri.path()
            )
        };
        let mut response = json_response(
            StatusCode::NOT_FOUND,
            rpc_error("notFound", message, Some(request_id)),
        );
        apply_cors_headers(&headers, &mut response, &state.config);
        return response;
    };
    /*
    CDXC:GxserverPresentationEvents 2026-06-14-20:37:
    Browser WebSocket clients cannot set Authorization headers, so Rust keeps the TypeScript authToken query option and protocolVersion query/header gate for /api/events.

    CDXC:GxserverProtocol 2026-06-22-04:10:
    Plain HTTP requests to the WebSocket-only event path must keep the TypeScript JSON `notFound` envelope. Do not expose Axum WebSocket extractor rejection bodies because clients rely on gxserver's product/protocol/requestId error shape.
    */
    let query_token = query_value(&uri, "authToken");
    let authorized = is_authorized_headers(&headers, &state.auth_token)
        || query_token
            .as_deref()
            .map(|token| is_expected_gxserver_auth_token(token, &state.auth_token))
            .unwrap_or(false);
    if !authorized {
        return json_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(
                "unauthorized",
                "gxserver auth token is required for this endpoint.",
                Some(request_id),
            ),
        );
    }
    let protocol_version = read_protocol_version(&headers, &uri, None);
    if !is_expected_protocol_version(protocol_version.as_ref()) {
        return json_response(
            StatusCode::UPGRADE_REQUIRED,
            protocol_mismatch_error(protocol_version, Some(request_id)),
        );
    }
    ws.on_upgrade(move |socket| handle_event_socket(socket, state))
}

async fn handle_terminal(
    State(state): State<Arc<AppState>>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
) -> Response<Body> {
    let Ok(ws) = ws else {
        let request_id = request_id(&headers);
        let message = if method == Method::OPTIONS {
            format!("{} is not a gxserver HTTP endpoint.", uri.path())
        } else {
            format!(
                "No gxserver endpoint for {} {}.",
                method.as_str(),
                uri.path()
            )
        };
        let mut response = json_response(
            StatusCode::NOT_FOUND,
            rpc_error("notFound", message, Some(request_id)),
        );
        apply_cors_headers(&headers, &mut response, &state.config);
        return response;
    };
    let terminal_state = TerminalWsState {
        auth_token: state.auth_token.clone(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
        paths: state.paths.clone(),
        server_id: state.metadata.server_id.clone(),
    };
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, uri, terminal_state))
}

async fn handle_event_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = state.event_hub.client_channel();
    let mut broadcast_rx = state.event_hub.subscribe();
    if outbound_tx
        .try_send(json!({
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "serverId": state.metadata.server_id.clone(),
            "type": "eventStreamReady",
        }))
        .is_err()
    {
        return;
    }
    /*
    Direct client events (including the subscription snapshot) and hub
    broadcasts must enter one FIFO before socket delivery. A separate unbiased
    select over two ready receivers can send revision R+1 before an already
    queued snapshot R. Forward broadcasts into the client queue instead; the
    snapshot enqueue is sequenced with presentation producers below, so a
    later revision cannot enter this queue first.
    */
    let broadcast_outbound_tx = outbound_tx.clone();
    let broadcast_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(event) => {
                    if broadcast_outbound_tx.try_send(event).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    /*
                    A lagged hub receiver has already lost ordered events.
                    Trigger the same explicit disconnect as a full client FIFO
                    so the client resubscribes from an authoritative snapshot.
                    */
                    broadcast_outbound_tx.signal_overflow();
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    let sender_task = tokio::spawn(async move {
        while let Some(event) = outbound_rx.recv().await {
            if send_event_message(&mut socket_sender, event).await.is_err() {
                break;
            }
        }
    });
    let mut renderer_client_id: Option<String> = None;
    loop {
        let message = tokio::select! {
            _ = outbound_tx.wait_for_overflow() => break,
            message = socket_receiver.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let Ok(message) = message else {
            break;
        };
        if !handle_event_client_message(&state, &outbound_tx, &mut renderer_client_id, message)
            .await
        {
            break;
        }
    }
    if let Some(client_id) = renderer_client_id {
        state.event_hub.unregister_renderer_client(&client_id).await;
    }
    drop(outbound_tx);
    broadcast_task.abort();
    sender_task.abort();
    let _ = broadcast_task.await;
    let _ = sender_task.await;
}

async fn send_event_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    event: Value,
) -> std::result::Result<(), axum::Error> {
    sender
        .send(Message::Text(format!("{event}\n").into()))
        .await
}

async fn handle_event_client_message(
    state: &AppState,
    outbound_tx: &EventClientSender,
    renderer_client_id: &mut Option<String>,
    message: Message,
) -> bool {
    let Some(parsed) = parse_event_client_message(message) else {
        return true;
    };
    match parsed.get("type").and_then(Value::as_str) {
        Some("rendererCommandResult") => {
            state
                .event_hub
                .handle_renderer_command_result(&parsed)
                .await;
            true
        }
        Some("subscribePresentation") => {
            if parsed.get("rendererCommands").and_then(Value::as_bool) == Some(true)
                && renderer_client_id.is_none()
            {
                let client_id = format!("renderer-client-{}", Uuid::new_v4());
                *renderer_client_id = Some(
                    state
                        .event_hub
                        .register_renderer_client(client_id, outbound_tx.clone())
                        .await,
                );
            }
            send_presentation_snapshot_for_subscription(state, outbound_tx, &parsed)
        }
        _ => true,
    }
}

fn parse_event_client_message(message: Message) -> Option<Map<String, Value>> {
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok()?,
        Message::Close(_) => return None,
        Message::Ping(_) | Message::Pong(_) => return Some(Map::new()),
    };
    serde_json::from_str::<Value>(&text)
        .ok()?
        .as_object()
        .cloned()
}

fn send_presentation_snapshot_for_subscription(
    state: &AppState,
    outbound_tx: &EventClientSender,
    parsed: &Map<String, Value>,
) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return true;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _ = sync_session_state_sidecars(state, &db, &repository, None, "presentation-subscribe");
    let _ =
        sync_live_zmx_process_identities(state, &db, &repository, None, "presentation-subscribe");
    /*
    Own the producer sequencer at this delivery boundary rather than calling
    read_presentation_snapshot_in_sequence and dropping its guard before the
    event is queued. Once this snapshot R enters the client's single FIFO,
    producers may publish R+1 into that same FIFO, never ahead of it.
    */
    let auto_settle_after_days = session_lifecycle::read_sweep_auto_settle_after_days(&state.paths);
    let sidebar_v2_selected = session_lifecycle::read_sidebar_v2_selected(&state.paths);
    let Ok(_event_sequence) = lock_presentation_event_sequence(state) else {
        return true;
    };
    let Ok(snapshot) = read_presentation_snapshot(
        &db,
        &state.metadata.server_id,
        auto_settle_after_days,
        sidebar_v2_selected,
    ) else {
        return true;
    };
    let revision = snapshot
        .get("revision")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let mut event = Map::new();
    if let Some(client_id) = parsed.get("clientId").and_then(Value::as_str) {
        event.insert("clientId".to_string(), Value::String(client_id.to_string()));
    }
    event.insert(
        "protocolVersion".to_string(),
        Value::Number(serde_json::Number::from(GXSERVER_PROTOCOL_VERSION)),
    );
    event.insert(
        "revision".to_string(),
        Value::Number(serde_json::Number::from(revision)),
    );
    event.insert(
        "serverId".to_string(),
        Value::String(state.metadata.server_id.clone()),
    );
    event.insert("snapshot".to_string(), snapshot);
    event.insert(
        "type".to_string(),
        Value::String("presentationSnapshot".to_string()),
    );
    outbound_tx.try_send(Value::Object(event)).is_ok()
}

fn create_authenticated_health(state: &AppState) -> ServerHealthResponse {
    let minimal = MinimalHealthResponse::new(&state.version);
    ServerHealthResponse {
        ok: minimal.ok,
        product: minimal.product,
        protocol_version: minimal.protocol_version,
        version: minimal.version,
        build_identity: state.build_identity.clone(),
        capabilities: GXSERVER_CAPABILITIES
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
        listeners: state.config.listeners.clone(),
        migration: state.migration.clone(),
        pid: state.metadata.pid,
        portless: read_portless_status_payload_for_paths(&state.paths),
        port: state.metadata.port,
        server_id: state.metadata.server_id.clone(),
        started_at: state.metadata.started_at.clone(),
        t3_runtime: Some(state.t3_runtime.status_snapshot()),
        tools: get_gxserver_tool_statuses(),
    }
}

async fn read_json_body(
    headers: &HeaderMap,
    body: Body,
) -> std::result::Result<Value, ReadBodyError> {
    if content_length(headers).map(|length| length > GXSERVER_JSON_BODY_LIMIT_BYTES as u64)
        == Some(true)
    {
        return Err(ReadBodyError::TooLarge);
    }
    let bytes = to_bytes(body, GXSERVER_JSON_BODY_LIMIT_BYTES + 1)
        .await
        .map_err(|_| ReadBodyError::TooLarge)?;
    if bytes.len() > GXSERVER_JSON_BODY_LIMIT_BYTES {
        return Err(ReadBodyError::TooLarge);
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| ReadBodyError::InvalidJson)?
        .trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(text).map_err(|_| ReadBodyError::InvalidJson)
}

enum ReadBodyError {
    InvalidJson,
    TooLarge,
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn read_protocol_version(headers: &HeaderMap, uri: &Uri, body: Option<&Value>) -> Option<Value> {
    if let Some(header) = headers
        .get(HeaderName::from_static(GXSERVER_PROTOCOL_HEADER))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(parse_protocol_version(header));
    }
    if let Some(value) = body.and_then(|body| body.get("protocolVersion")) {
        return Some(value.clone());
    }
    query_value(uri, "protocolVersion")
        .filter(|value| !value.is_empty())
        .map(|value| parse_protocol_version(&value))
}

fn parse_protocol_version(value: &str) -> Value {
    if value.chars().all(|ch| ch.is_ascii_digit()) {
        value
            .parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(value.to_string()))
    } else {
        Value::String(value.to_string())
    }
}

fn is_expected_protocol_version(value: Option<&Value>) -> bool {
    value.and_then(Value::as_u64) == Some(GXSERVER_PROTOCOL_VERSION)
}

fn query_value(uri: &Uri, key: &str) -> Option<String> {
    uri.query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.into_owned())
    })
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn routed_json<T: serde::Serialize>(
    endpoint_path: Option<String>,
    status: StatusCode,
    body: T,
) -> RoutedResponse {
    RoutedResponse {
        endpoint_path,
        response: json_response(status, body),
    }
}

fn json_response<T: serde::Serialize>(status: StatusCode, body: T) -> Response<Body> {
    let text = serde_json::to_string(&body).unwrap_or_else(|_| {
        format!(
            r#"{{"error":"internalError","message":"Failed to serialize gxserver response.","ok":false,"product":"{GXSERVER_PRODUCT}","protocolVersion":{GXSERVER_PROTOCOL_VERSION}}}"#
        )
    });
    let mut response = Response::new(Body::from(format!("{text}\n")));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn apply_cors_headers(
    request_headers: &HeaderMap,
    response: &mut Response<Body>,
    config: &GxserverConfig,
) {
    let Some(origin) = request_headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return;
    };
    append_vary_header(response, "Origin");
    append_vary_header(response, "Access-Control-Request-Private-Network");
    let explicitly_allowed = config
        .cors
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin);
    if !explicitly_allowed && !is_loopback_web_origin(origin) {
        return;
    }
    if let Ok(value) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "authorization, content-type, x-gxserver-protocol-version, x-request-id",
        ),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    if request_headers
        .get("access-control-request-private-network")
        .and_then(|value| value.to_str().ok())
        == Some("true")
    {
        response.headers_mut().insert(
            "access-control-allow-private-network",
            HeaderValue::from_static("true"),
        );
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
}

fn is_loopback_web_origin(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    ["127.0.0.1", "localhost", "[::1]"].iter().any(|host| {
        authority == *host
            || authority
                .strip_prefix(&format!("{host}:"))
                .is_some_and(|port| {
                    !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
                })
    })
}

fn append_vary_header(response: &mut Response<Body>, value: &str) {
    let existing = response
        .headers()
        .get(header::VARY)
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");
    let mut values = existing
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
    if let Ok(header_value) = HeaderValue::from_str(&values.join(", ")) {
        response.headers_mut().insert(header::VARY, header_value);
    }
}

async fn wait_for_process_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[allow(dead_code)]
fn _permission_name(permission: ApiPermission) -> &'static str {
    match permission {
        ApiPermission::FullLocal => "fullLocal",
        ApiPermission::RemoteAllowed => "remoteAllowed",
        ApiPermission::RemoteBlocked => "remoteBlocked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::create_default_gxserver_config,
        constants::GXSERVER_PROTOCOL_HEADER,
        storage::{create_gxserver_migration_status, initialize_gxserver_storage},
    };
    use std::fs;

    #[test]
    fn renderer_command_actions_include_generated_title_rename() {
        /*
        CDXC:GenerateTitleSkill 2026-06-17-17:02:
        Rust gxserver must accept the same renderer `renameCommand` action as the TypeScript daemon so a full cutover keeps Claude Code generated-title renames on the native Enter path.
        */
        assert!(RENDERER_COMMAND_ACTIONS.contains(&"renameCommand"));
    }

    #[test]
    fn renderer_command_actions_include_mobile_session_timer_actions() {
        /*
        CDXC:MobileDelayedSend 2026-07-24:
        `ghostex delayed-send` / `close-after-done` from mobile clients enter
        gxserver as renderer commands; dropping these from the allowlist
        silently breaks the mobile session context menu.
        */
        assert!(RENDERER_COMMAND_ACTIONS.contains(&"scheduleDelayedSend"));
        assert!(RENDERER_COMMAND_ACTIONS.contains(&"cancelDelayedSend"));
        assert!(RENDERER_COMMAND_ACTIONS.contains(&"toggleCloseAfterDone"));
    }

    #[test]
    fn typed_operation_scope_rejection_details_match_private_typescript_shape() {
        let mut params = Map::new();
        params.insert("action".to_string(), json!("board"));
        params.insert("projectId".to_string(), json!("P3a91"));
        params.insert(
            "projectPath".to_string(),
            json!("/Users/person/dev/private-project"),
        );
        let details = typed_operation_scope_rejection_details(
            "/api/runBeadsAction",
            &params,
            &TypedOperationError {
                code: "notFound",
                details: None,
                message: "projectPath does not exist".to_string(),
                scope_rejection: true,
            },
        );

        assert_eq!(details.get("action"), Some(&json!("board")));
        assert_eq!(details.get("endpoint"), Some(&json!("runBeadsAction")));
        assert_eq!(details.get("errorCode"), Some(&json!("notFound")));
        assert_eq!(
            details.get("errorType"),
            Some(&json!("GxserverProjectPathError"))
        );
        assert_eq!(details.get("hasProjectId"), Some(&json!(true)));
        assert_eq!(details.get("hasProjectPath"), Some(&json!(true)));
        assert!(!details.to_string().contains("private-project"));
    }

    #[test]
    fn foreground_classifies_selected_port_ownership_like_typescript() {
        let current = test_health("gxserver:0.1.0:current");
        let previous = test_health("gxserver:0.1.0:previous");

        assert_eq!(
            classify_existing_gxserver(Some(&current), "gxserver:0.1.0:current"),
            ExistingGxserverState::Reusable
        );
        assert_eq!(
            classify_existing_gxserver(Some(&previous), "gxserver:0.1.0:current"),
            ExistingGxserverState::Running
        );
        assert_eq!(
            classify_existing_gxserver(None, "gxserver:0.1.0:current"),
            ExistingGxserverState::Stopped
        );
    }

    #[test]
    fn project_status_agent_title_polling_predicate_matches_typescript() {
        let pending = json!({
            "kind": "agent",
            "projectId": "P1abc",
            "runtimeSettings": {
                "pendingAgentTitleRequestStatus": "pending"
            },
            "sessionId": "G1abc",
            "title": "Trusted user title"
        });
        assert!(should_check_agent_metadata_title_for_project_status(
            &pending
        ));

        let trusted = json!({
            "kind": "agent",
            "projectId": "P1abc",
            "runtimeSettings": {},
            "sessionId": "G1abc",
            "title": "Investigate renderer state"
        });
        assert!(!should_check_agent_metadata_title_for_project_status(
            &trusted
        ));

        let placeholder = json!({
            "kind": "agent",
            "projectId": "P1abc",
            "runtimeSettings": { "titleSource": "placeholder" },
            "sessionId": "G1abc",
            "title": "Codex Session"
        });
        assert!(should_check_agent_metadata_title_for_project_status(
            &placeholder
        ));

        let reconciled = json!({
            "kind": "agent",
            "projectId": "P1abc",
            "runtimeSettings": { "titleMetadataSource": "agent-metadata" },
            "sessionId": "G1abc",
            "title": "Codex Session"
        });
        assert!(!should_check_agent_metadata_title_for_project_status(
            &reconciled
        ));

        let terminal = json!({
            "kind": "terminal",
            "projectId": "P1abc",
            "runtimeSettings": {},
            "sessionId": "G1abc",
            "title": "Codex Session"
        });
        assert!(!should_check_agent_metadata_title_for_project_status(
            &terminal
        ));
    }

    #[test]
    fn renderer_command_payload_adds_structured_session_target() {
        /*
        CDXC:GxserverRendererCommands 2026-06-21-19:22:
        Rust gxserver must normalize renderer-command payloads from any client so macOS receives a project-scoped session target and does not have to match raw G ids against combined sidebar presentation ids.
        */
        let payload = Map::from_iter([
            ("globalRef".to_string(), json!("S90:P1a:G9a")),
            ("projectId".to_string(), json!("P1a")),
            ("sessionId".to_string(), json!("G9a")),
            ("title".to_string(), json!("GPUI Sidebar Resize Parity")),
        ]);

        let normalized = with_renderer_session_target(payload);

        assert_eq!(
            normalized.get("sessionTarget"),
            Some(&json!({
                "globalRef": "S90:P1a:G9a",
                "projectId": "P1a",
                "sessionId": "G9a",
            }))
        );
        assert_eq!(normalized.get("sessionId"), Some(&json!("G9a")));
    }

    #[test]
    fn first_prompt_auto_title_decides_provider_strategy_and_filters_meta_prompts() {
        let codex = json!({
            "agentId": "codex",
            "runtimeSettings": {},
            "title": "Codex Session",
        });
        let decision = decide_first_prompt_auto_title(
            &codex,
            Some("Please can you help me fix flaky tests."),
            false,
        );
        assert!(decision.should_run);
        assert_eq!(
            decision.normalized_prompt.as_deref(),
            Some("fix flaky tests")
        );
        assert_eq!(decision.strategy, Some("generateTitleAndRename"));

        let claude = json!({
            "agentId": "claude",
            "runtimeSettings": {},
            "title": "Claude Code",
        });
        let decision = decide_first_prompt_auto_title(&claude, Some("Summarize the logs"), false);
        assert!(decision.should_run);
        assert_eq!(decision.strategy, Some("sendBareRenameCommand"));

        let meta = decide_first_prompt_auto_title(&codex, Some("# AGENTS.md instructions"), false);
        assert!(!meta.should_run);
        assert_eq!(meta.reason, "metaPrompt");

        let slash = decide_first_prompt_auto_title(
            &codex,
            Some("notes before command\n  /status please"),
            false,
        );
        assert!(!slash.should_run);
        assert_eq!(slash.reason, "slashCommand");
    }

    #[test]
    fn generated_first_prompt_titles_are_sanitized_and_clamped() {
        let title = parse_generated_session_title_text(
            "```text\n\"Investigate Sidebar Resize Regression With Extra Words\"\n```",
        )
        .expect("title");
        assert_eq!(title, "Investigate Sidebar Resize Regression");
        assert!(js_string_length(&title) <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH);
    }

    #[test]
    fn first_prompt_title_caps_use_javascript_utf16_length() {
        let rocket = "\u{1F680}";
        let exact = js_string_slice_prefix(
            &rocket.repeat(126),
            GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
        );
        assert_eq!(exact, rocket.repeat(125));
        assert_eq!(
            js_string_length(&exact),
            GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH
        );

        let split = js_string_slice_prefix(
            &format!(
                "{}{}",
                "a".repeat(GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH - 1),
                rocket
            ),
            GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
        );
        assert_eq!(
            split,
            format!(
                "{}{}",
                "a".repeat(GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH - 1),
                char::REPLACEMENT_CHARACTER
            )
        );
        assert_eq!(
            js_string_length(&split),
            GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH
        );
    }

    #[test]
    fn generated_title_clamp_counts_non_bmp_as_javascript_utf16() {
        let rocket = "\u{1F680}";
        let title = parse_generated_session_title_text(&rocket.repeat(20)).expect("title");
        assert_eq!(
            title,
            format!("{}{}", rocket.repeat(19), char::REPLACEMENT_CHARACTER)
        );
        assert_eq!(
            js_string_length(&title),
            GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH
        );
    }

    #[tokio::test]
    async fn read_project_status_route_returns_project_sessions_and_missing_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let created_project = route_http(
            state.clone(),
            rpc_request(
                "/api/createProject",
                &token,
                json!({
                    "params": {
                        "name": "Status Project",
                        "runtimeSettings": { "defaultPromptAgentId": "codex" }
                    }
                }),
            ),
            "request-create-project".to_string(),
        )
        .await;
        assert_eq!(created_project.response.status(), StatusCode::OK);
        let body = response_json(created_project.response).await;
        let project_id = body["result"]["project"]["projectId"]
            .as_str()
            .expect("project id")
            .to_string();

        let created_session = route_http(
            state.clone(),
            rpc_request(
                "/api/createSession",
                &token,
                json!({
                    "params": {
                        "projectId": project_id.clone(),
                        "title": "Status Session"
                    }
                }),
            ),
            "request-create-session".to_string(),
        )
        .await;
        assert_eq!(created_session.response.status(), StatusCode::OK);
        let body = response_json(created_session.response).await;
        let session_id = body["result"]["session"]["sessionId"]
            .as_str()
            .expect("session id")
            .to_string();

        let status = route_http(
            state.clone(),
            rpc_request(
                "/api/readProjectStatus",
                &token,
                json!({ "params": { "projectId": project_id } }),
            ),
            "request-read-project-status".to_string(),
        )
        .await;
        assert_eq!(status.response.status(), StatusCode::OK);
        let body = response_json(status.response).await;
        assert_eq!(body["result"]["project"]["projectId"], json!(project_id));
        assert_eq!(
            body["result"]["project"]["runtimeSettings"]["defaultPromptAgentId"],
            json!("codex")
        );
        assert_eq!(body["result"]["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["result"]["sessions"][0]["sessionId"],
            json!(session_id)
        );

        let missing = route_http(
            state,
            rpc_request(
                "/api/readProjectStatus",
                &token,
                json!({ "params": { "projectId": "P9zzz" } }),
            ),
            "request-read-missing-project-status".to_string(),
        )
        .await;
        assert_eq!(missing.response.status(), StatusCode::NOT_FOUND);
        let body = response_json(missing.response).await;
        assert_eq!(body["error"], json!("notFound"));
        assert_eq!(body["message"], json!("Project P9zzz does not exist."));
    }

    #[tokio::test]
    async fn protocol_contract_gate_edges_match_typescript() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let unknown_options = route_http(
            state.clone(),
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/missing")
                .body(Body::empty())
                .expect("request"),
            "request-options".to_string(),
        )
        .await;
        assert_eq!(unknown_options.response.status(), StatusCode::NOT_FOUND);
        let body = response_json(unknown_options.response).await;
        assert_eq!(body["error"], json!("notFound"));
        assert_eq!(
            body["message"],
            json!("/api/missing is not a gxserver HTTP endpoint.")
        );

        let http_events = route_http(
            state.clone(),
            Request::builder()
                .method(Method::GET)
                .uri("/api/events")
                .body(Body::empty())
                .expect("request"),
            "request-events".to_string(),
        )
        .await;
        assert_eq!(http_events.response.status(), StatusCode::NOT_FOUND);
        let body = response_json(http_events.response).await;
        assert_eq!(body["error"], json!("notFound"));
        assert_eq!(
            body["message"],
            json!("No gxserver endpoint for GET /api/events.")
        );

        let header_wins = route_http(
            state,
            Request::builder()
                .method(Method::POST)
                .uri("/api/listSessions")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(GXSERVER_PROTOCOL_HEADER, "999")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "params": {}, "protocolVersion": GXSERVER_PROTOCOL_VERSION })
                        .to_string(),
                ))
                .expect("request"),
            "request-protocol".to_string(),
        )
        .await;
        assert_eq!(header_wins.response.status(), StatusCode::UPGRADE_REQUIRED);
        let body = response_json(header_wins.response).await;
        assert_eq!(body["error"], json!("protocolMismatch"));
        assert_eq!(
            body["message"],
            json!(
                "gxserver protocol mismatch. Expected protocol 1, got 999. Update Ghostex and gxserver so their protocol versions match."
            )
        );
    }

    #[tokio::test]
    async fn protocol_query_parsing_matches_typescript_edges() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let empty_query = route_http(
            state.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/api/listSessions?protocolVersion=")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "params": {} }).to_string()))
                .expect("request"),
            "request-empty-query".to_string(),
        )
        .await;
        assert_eq!(empty_query.response.status(), StatusCode::UPGRADE_REQUIRED);
        let body = response_json(empty_query.response).await;
        assert_eq!(
            body["message"],
            json!(
                "gxserver protocol mismatch. Expected protocol 1, got undefined. Update Ghostex and gxserver so their protocol versions match."
            )
        );

        let plus_query = route_http(
            state,
            Request::builder()
                .method(Method::POST)
                .uri("/api/listSessions?protocolVersion=%2B1")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "params": {} }).to_string()))
                .expect("request"),
            "request-plus-query".to_string(),
        )
        .await;
        assert_eq!(plus_query.response.status(), StatusCode::UPGRADE_REQUIRED);
        let body = response_json(plus_query.response).await;
        assert_eq!(
            body["message"],
            json!(
                "gxserver protocol mismatch. Expected protocol 1, got +1. Update Ghostex and gxserver so their protocol versions match."
            )
        );
    }

    #[test]
    fn request_id_preserves_non_empty_header_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static(" request-1 "));
        assert_eq!(request_id(&headers), " request-1 ");
    }

    #[test]
    fn session_state_sidecar_parser_matches_legacy_env_fields() {
        let raw = [
            "agent=codex",
            "agentSessionId=019eebdb-ba5a-7282-ac09-b926a9c09863",
            "agentSessionPath=/Users/example/.codex/sessions/2026/06/21/thread.jsonl",
            "firstUserMessageBase64=UGxlYXNlIGZpeCB0aGUgc2lkZWJhcg==",
            "lastActivityAt=2026-06-21T20:25:05.171Z",
            "status=working",
            "statusUpdatedAt=2026-06-21T20:25:06.000Z",
            "title=  GPUI Sidebar Resize Parity  ",
        ]
        .join("\n");
        let sidecar = parse_session_state_sidecar(&raw).expect("sidecar");

        assert_eq!(sidecar.agent_name.as_deref(), Some("codex"));
        assert_eq!(
            sidecar.agent_session_id.as_deref(),
            Some("019eebdb-ba5a-7282-ac09-b926a9c09863")
        );
        assert_eq!(
            sidecar.first_user_message.as_deref(),
            Some("Please fix the sidebar")
        );
        assert_eq!(sidecar.status.as_deref(), Some("working"));
        assert_eq!(
            sidecar.status_updated_at.as_deref(),
            Some("2026-06-21T20:25:06.000Z")
        );
        assert_eq!(sidecar.title.as_deref(), Some("GPUI Sidebar Resize Parity"));
        assert!(has_session_state_sidecar_payload(&sidecar));
        assert_eq!(
            sanitize_session_state_sidecar_path_part("P3lv0/../../G01q0"),
            "P3lv0-..-..-G01q0"
        );
    }

    #[test]
    fn session_state_sidecar_reader_uses_typescript_one_mib_cap() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let sidecar_path = build_session_state_sidecar_path(&paths, "P3lv0", "G01q0");
        fs::create_dir_all(sidecar_path.parent().expect("sidecar parent")).expect("sidecar dir");

        fs::write(
            &sidecar_path,
            format!("agent=codex\npadding={}", "x".repeat(70 * 1024)),
        )
        .expect("write sidecar under cap");
        let sidecar = read_session_state_sidecar(&paths, "P3lv0", "G01q0").expect("sidecar");
        assert_eq!(sidecar.agent_name.as_deref(), Some("codex"));

        fs::write(
            &sidecar_path,
            format!(
                "agent=codex\npadding={}",
                "x".repeat(GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES as usize + 1)
            ),
        )
        .expect("write sidecar over cap");
        assert!(read_session_state_sidecar(&paths, "P3lv0", "G01q0").is_none());
    }

    #[test]
    fn zmx_title_observer_parses_lines_and_filters_observable_sessions() {
        assert_eq!(
            parse_zmx_title_line(r#"{"title":"  Codex Session  "}"#).as_deref(),
            Some("Codex Session")
        );
        assert!(parse_zmx_title_line(r#"{"title":"   "}"#).is_none());
        assert!(parse_zmx_title_line("not-json").is_none());

        let observable = json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "providerState": { "lifecycleState": "exists", "provider": "zmx" },
            "projectId": "P1",
            "runtimeSettings": {},
            "sessionId": "G1",
            "zmxName": "S1-P1-G1"
        });
        assert!(is_zmx_title_observable_session(&observable));

        let missing_provider = json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "providerState": { "lifecycleState": "missing", "provider": "zmx" },
            "projectId": "P1",
            "sessionId": "G1",
            "zmxName": "S1-P1-G1"
        });
        assert!(!is_zmx_title_observable_session(&missing_provider));
    }

    #[test]
    fn live_zmx_process_identity_sync_accepts_provider_state_only_sessions() {
        let provider_state_only = json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "providerState": { "lifecycleState": "exists", "provider": "zmx" },
            "runtimeSettings": {},
            "surface": "workspace",
            "zmxName": "S1-P1-G1"
        });
        assert!(should_sync_live_zmx_process_identity(&provider_state_only));

        let runtime_provider = json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "providerState": { "lifecycleState": "exists" },
            "runtimeSettings": { "sessionPersistenceProvider": "zmx" },
            "surface": "workspace",
            "zmxName": "S1-P1-G1"
        });
        assert!(should_sync_live_zmx_process_identity(&runtime_provider));

        let command_surface = json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "providerState": { "lifecycleState": "exists", "provider": "zmx" },
            "runtimeSettings": {},
            "surface": "commands",
            "zmxName": "S1-P1-G1"
        });
        assert!(!should_sync_live_zmx_process_identity(&command_surface));
    }

    #[tokio::test]
    async fn query_logs_route_returns_filtered_logs_and_bad_request_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        fs::write(
            &paths.log_file,
            [
                serde_json::to_string(&json!({
                    "client": "cli",
                    "event": "agent.detected",
                    "level": "info",
                    "projectId": "P3a91",
                    "serverId": "S7k",
                    "sessionId": "G8v20",
                    "ts": "2026-05-30T10:00:00.000Z"
                }))
                .unwrap(),
                serde_json::to_string(&json!({
                    "client": "api",
                    "event": "zmx.kill.failed",
                    "level": "error",
                    "projectId": "P3a91",
                    "serverId": "S7k",
                    "sessionId": "G8v20",
                    "ts": "2026-05-30T10:01:00.000Z"
                }))
                .unwrap(),
            ]
            .join("\n")
                + "\n",
        )
        .expect("write logs");
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();

        let filtered = route_http(
            state.clone(),
            rpc_request(
                "/api/queryLogs",
                &token,
                json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "params": {
                        "eventPrefix": "agent.",
                        "limit": 1,
                        "order": "desc"
                    }
                }),
            ),
            "request-1".to_string(),
        )
        .await;
        assert_eq!(filtered.response.status(), StatusCode::OK);
        let body = response_json(filtered.response).await;
        assert_eq!(body["ok"], json!(true));
        assert_eq!(body["result"]["entries"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["result"]["entries"][0]["event"],
            json!("agent.detected")
        );
        assert_eq!(body["result"]["malformedLineCount"], json!(0));

        let bad_params = route_http(
            state,
            rpc_request(
                "/api/queryLogs",
                &token,
                json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "params": { "limit": 0 }
                }),
            ),
            "request-2".to_string(),
        )
        .await;
        assert_eq!(bad_params.response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(bad_params.response).await;
        assert_eq!(body["error"], json!("badRequest"));
    }

    #[tokio::test]
    async fn agent_hook_route_matches_typescript_bad_params_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let response = route_http(
            state,
            rpc_request(
                "/api/readAgentHookStatus",
                &token,
                json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "params": []
                }),
            ),
            "request-hook-bad-params".to_string(),
        )
        .await;

        assert_eq!(
            response.response.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        let body = response_json(response.response).await;
        assert_eq!(body["error"], json!("internalError"));
        assert_eq!(body["message"], json!("RPC params must be an object."));
    }

    #[tokio::test]
    async fn agent_hook_conflict_response_strips_private_log_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let log_file = paths.log_file.clone();
        let state = test_app_state(paths);
        let token = state.auth_token.clone();
        let current_codex_session_id = "019e7af5-c610-7f62-a129-db7bb510b48d";
        let incoming_codex_session_id = "019e7c39-7ba7-7ac3-b79c-02757e299516";

        let created_project = route_http(
            state.clone(),
            rpc_request(
                "/api/createProject",
                &token,
                json!({ "params": { "name": "Hook Conflict" } }),
            ),
            "request-create-hook-conflict-project".to_string(),
        )
        .await;
        assert_eq!(created_project.response.status(), StatusCode::OK);
        let body = response_json(created_project.response).await;
        let project_id = body["result"]["project"]["projectId"]
            .as_str()
            .expect("project id")
            .to_string();

        let created_session = route_http(
            state.clone(),
            rpc_request(
                "/api/createSession",
                &token,
                json!({
                    "params": {
                        "agentId": "codex",
                        "kind": "agent",
                        "projectId": project_id.clone(),
                        "runtimeSettings": {
                            "agentActivity": { "activity": "idle", "isAcknowledged": true },
                            "agentName": "codex",
                            "agentSessionId": current_codex_session_id,
                            "titleSource": "terminal-auto"
                        },
                        "title": "Target Codex Thread"
                    }
                }),
            ),
            "request-create-hook-conflict-session".to_string(),
        )
        .await;
        assert_eq!(created_session.response.status(), StatusCode::OK);
        let body = response_json(created_session.response).await;
        let session_id = body["result"]["session"]["sessionId"]
            .as_str()
            .expect("session id")
            .to_string();

        let ingested = route_http(
            state,
            rpc_request(
                "/api/ingestAgentHookEvent",
                &token,
                json!({
                    "params": {
                        "agentName": "codex",
                        "agentSessionId": incoming_codex_session_id,
                        "eventName": "Stop",
                        "firstUserMessage": "private prompt text",
                        "projectId": project_id,
                        "rawEventName": "Stop",
                        "sessionId": session_id.clone(),
                        "status": "attention",
                        "statusUpdatedAt": "2026-06-09T18:08:19.857Z",
                        "title": "Wrong Codex Thread"
                    }
                }),
            ),
            "request-ingest-hook-conflict".to_string(),
        )
        .await;

        assert_eq!(ingested.response.status(), StatusCode::OK);
        let body = response_json(ingested.response).await;
        assert_eq!(
            body["result"]["reason"],
            json!("passive-session-identity-conflict")
        );
        assert!(body["result"].get("identityConflict").is_none());
        assert_eq!(body["result"]["activity"]["activity"], json!("idle"));
        let logs = fs::read_to_string(log_file).expect("read hook conflict log");
        assert!(logs.contains("sessionIdentity.passiveEventRejected"));
        assert!(!logs.contains(current_codex_session_id));
        assert!(!logs.contains(incoming_codex_session_id));
        assert!(!logs.contains("private prompt text"));
        assert!(!logs.contains("Wrong Codex Thread"));
        assert!(!logs.contains("Target Codex Thread"));
    }

    #[tokio::test]
    async fn browse_project_directories_route_filters_directory_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("picker-parent");
        fs::create_dir_all(parent.join("alpha")).expect("alpha");
        fs::create_dir_all(parent.join("alpine")).expect("alpine");
        fs::create_dir_all(parent.join("beta")).expect("beta");
        fs::create_dir_all(parent.join(".hidden")).expect("hidden");
        fs::write(parent.join("alphabet.txt"), "not a directory\n").expect("file");
        let parent_path = path_to_string(&parent);

        let filtered = route_http(
            state.clone(),
            rpc_request(
                "/api/browseProjectDirectories",
                &token,
                json!({
                    "params": {
                        "limit": 5,
                        "partialPath": format!("{parent_path}/al")
                    }
                }),
            ),
            "request-browse-filtered".to_string(),
        )
        .await;
        assert_eq!(filtered.response.status(), StatusCode::OK);
        let body = response_json(filtered.response).await;
        assert_eq!(body["result"]["parentPath"], json!(parent_path));
        let names = body["result"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alpha", "alpine"]);

        let hidden = route_http(
            state.clone(),
            rpc_request(
                "/api/browseProjectDirectories",
                &token,
                json!({
                    "params": {
                        "partialPath": format!("{parent_path}/.h")
                    }
                }),
            ),
            "request-browse-hidden".to_string(),
        )
        .await;
        assert_eq!(hidden.response.status(), StatusCode::OK);
        let body = response_json(hidden.response).await;
        let names = body["result"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![".hidden"]);

        let relative = route_http(
            state,
            rpc_request(
                "/api/browseProjectDirectories",
                &token,
                json!({
                    "params": {
                        "cwd": parent_path,
                        "partialPath": "./a"
                    }
                }),
            ),
            "request-browse-relative".to_string(),
        )
        .await;
        assert_eq!(relative.response.status(), StatusCode::OK);
        let body = response_json(relative.response).await;
        let names = body["result"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alpha", "alpine"]);
    }

    #[tokio::test]
    async fn browse_project_directories_sorts_case_insensitively_and_swallows_permission_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("browse-order");
        for name in ["Zebra", "apple", "Banana", "cherry"] {
            fs::create_dir_all(parent.join(name)).expect("dir");
        }
        let parent_path = path_to_string(&parent);

        let sorted = route_http(
            state.clone(),
            rpc_request(
                "/api/browseProjectDirectories",
                &token,
                json!({ "params": { "partialPath": format!("{parent_path}/") } }),
            ),
            "request-browse-order".to_string(),
        )
        .await;
        assert_eq!(sorted.response.status(), StatusCode::OK);
        let body = response_json(sorted.response).await;
        let names = body["result"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["apple", "Banana", "cherry", "Zebra"]);
        assert!(body["result"]["entries"][0]
            .as_object()
            .unwrap()
            .get("sortKey")
            .is_none());

        let unreadable = paths.root_dir.join("browse-unreadable");
        fs::create_dir_all(unreadable.join("child")).expect("child");
        let mut permissions = fs::metadata(&unreadable).expect("metadata").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o000);
        }
        fs::set_permissions(&unreadable, permissions).expect("chmod");
        let denied = route_http(
            state,
            rpc_request(
                "/api/browseProjectDirectories",
                &token,
                json!({ "params": { "partialPath": format!("{}/", path_to_string(&unreadable)) } }),
            ),
            "request-browse-denied".to_string(),
        )
        .await;
        let denied_status = denied.response.status();
        let denied_body = response_json(denied.response).await;
        let mut restored = fs::metadata(&unreadable).expect("metadata").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            restored.set_mode(0o755);
        }
        fs::set_permissions(&unreadable, restored).expect("chmod restore");
        assert_eq!(denied_status, StatusCode::OK);
        assert_eq!(denied_body["result"]["entries"], json!([]));
        assert_eq!(
            denied_body["result"]["parentPath"],
            json!(path_to_string(&unreadable))
        );
    }

    #[tokio::test]
    async fn discover_source_control_reports_every_provider_with_a_hint() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths);
        let token = state.auth_token.clone();

        let response = route_http(
            state,
            rpc_request(
                "/api/discoverSourceControl",
                &token,
                json!({ "params": {} }),
            ),
            "request-discover-source-control".to_string(),
        )
        .await;
        assert_eq!(response.response.status(), StatusCode::OK);
        let body = response_json(response.response).await;
        let providers = body["result"]["discovery"]["providers"]
            .as_array()
            .expect("providers")
            .clone();
        let names = providers
            .iter()
            .map(|entry| entry["provider"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["github", "gitlab", "bitbucket", "azure-devops"]);
        for provider in &providers {
            assert!(provider["installHint"].as_str().is_some_and(|hint| !hint.is_empty()));
            assert!(provider["label"].as_str().is_some_and(|label| !label.is_empty()));
            assert!(provider["auth"]["status"].as_str().is_some());
            assert!(matches!(
                provider["status"].as_str(),
                Some("available") | Some("missing") | Some("unsupported")
            ));
        }
        for provider in providers.iter().filter(|entry| {
            matches!(
                entry["provider"].as_str(),
                Some("bitbucket") | Some("azure-devops")
            )
        }) {
            assert_eq!(provider["status"], json!("unsupported"));
        }
        assert!(body["result"]["discovery"]["checkedAt"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z')));
    }

    #[tokio::test]
    async fn lookup_repository_rejects_unsupported_providers_and_blank_repositories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths);
        let token = state.auth_token.clone();

        let unsupported = route_http(
            state.clone(),
            rpc_request(
                "/api/lookupRepository",
                &token,
                json!({ "params": { "provider": "bitbucket", "repository": "team/app" } }),
            ),
            "request-lookup-unsupported".to_string(),
        )
        .await;
        assert_eq!(unsupported.response.status(), StatusCode::BAD_REQUEST);

        let blank = route_http(
            state,
            rpc_request(
                "/api/lookupRepository",
                &token,
                json!({ "params": { "provider": "github", "repository": "  " } }),
            ),
            "request-lookup-blank".to_string(),
        )
        .await;
        assert_eq!(blank.response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(blank.response).await;
        assert_eq!(body["error"], json!("badRequest"));
        assert_eq!(
            body["message"],
            json!("repository must be a non-empty string.")
        );
    }

    #[tokio::test]
    async fn resolve_git_root_route_does_not_register_projects() {
        let git_available = StdCommand::new("git")
            .arg("--version")
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !git_available {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let repo = paths.root_dir.join("open-path-repo");
        let nested = repo.join("src").join("feature");
        let outside = paths.root_dir.join("outside-repo");
        fs::create_dir_all(&nested).expect("nested");
        fs::create_dir_all(&outside).expect("outside");
        assert!(StdCommand::new("git")
            .arg("init")
            .current_dir(&repo)
            .status()
            .expect("git init")
            .success());

        let resolved = route_http(
            state.clone(),
            rpc_request(
                "/api/resolveGitRootForPath",
                &token,
                json!({
                    "params": {
                        "path": path_to_string(&nested)
                    }
                }),
            ),
            "request-resolve-git-root".to_string(),
        )
        .await;
        assert_eq!(resolved.response.status(), StatusCode::OK);
        let body = response_json(resolved.response).await;
        assert_eq!(
            body["result"]["gitRoot"],
            json!(path_to_string(
                &fs::canonicalize(&repo).expect("canonical repo")
            ))
        );

        let projects = route_http(
            state.clone(),
            rpc_request("/api/listProjects", &token, json!({ "params": {} })),
            "request-list-projects".to_string(),
        )
        .await;
        let body = response_json(projects.response).await;
        assert_eq!(body["result"]["projects"], json!([]));

        let outside = route_http(
            state,
            rpc_request(
                "/api/resolveGitRootForPath",
                &token,
                json!({
                    "params": {
                        "path": path_to_string(&outside)
                    }
                }),
            ),
            "request-resolve-outside".to_string(),
        )
        .await;
        assert_eq!(outside.response.status(), StatusCode::OK);
        let body = response_json(outside.response).await;
        assert_eq!(body["result"], json!({}));
    }

    #[tokio::test]
    async fn delete_worktree_project_route_removes_clean_checkout_and_local_branch() {
        if !git_available() {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("delete-worktree-parent");
        let worktree = paths.root_dir.join("delete-worktree-parent-feature");
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(&parent, &["branch", "feature-clean"]);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                path_to_string(&worktree).as_str(),
                "feature-clean",
            ],
        );

        let parent_project = add_project_path_for_server_test(
            state.clone(),
            &token,
            &parent,
            Some("Delete Worktree Parent"),
        )
        .await;
        let worktree_project =
            add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;
        assert_eq!(
            worktree_project["worktree"]["parentProjectId"],
            parent_project["projectId"]
        );

        let response = route_http(
            state.clone(),
            rpc_request(
                "/api/deleteWorktreeProject",
                &token,
                json!({
                    "params": {
                        "deleteLocalBranch": true,
                        "projectId": worktree_project["projectId"]
                    }
                }),
            ),
            "request-delete-clean-worktree".to_string(),
        )
        .await;
        assert_eq!(response.response.status(), StatusCode::OK);
        let body = response_json(response.response).await;
        assert_eq!(
            body["result"]["checkoutRemoval"],
            json!({ "forced": false, "retriedForSubmodules": false })
        );
        assert_eq!(body["result"]["warnings"], json!([]));
        assert_eq!(
            body["result"]["project"]["projectId"],
            worktree_project["projectId"]
        );
        assert!(!worktree.exists());
        assert_eq!(
            run_git_status_for_server_test(&parent, &["rev-parse", "--verify", "feature-clean"])
                .status
                .code(),
            Some(128)
        );

        let projects = route_http(
            state,
            rpc_request("/api/listProjects", &token, json!({ "params": {} })),
            "request-list-after-delete-clean".to_string(),
        )
        .await;
        let body = response_json(projects.response).await;
        assert!(!body["result"]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["projectId"] == worktree_project["projectId"]));
    }

    #[tokio::test]
    async fn delete_worktree_project_route_force_removes_dirty_checkout_and_warns_for_remote() {
        if !git_available() {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let remote = paths.root_dir.join("delete-worktree-origin.git");
        let parent = paths.root_dir.join("delete-worktree-remote-parent");
        let worktree = paths.root_dir.join("delete-worktree-remote-parent-feature");
        run_git_for_server_test(
            &paths.root_dir,
            &["init", "--bare", path_to_string(&remote).as_str()],
        );
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(
            &parent,
            &["remote", "add", "origin", path_to_string(&remote).as_str()],
        );
        run_git_for_server_test(&parent, &["push", "-u", "origin", "HEAD:main"]);
        run_git_for_server_test(&parent, &["branch", "feature-remote"]);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                path_to_string(&worktree).as_str(),
                "feature-remote",
            ],
        );
        fs::write(worktree.join("dirty.txt"), "not committed\n").expect("dirty file");

        add_project_path_for_server_test(
            state.clone(),
            &token,
            &parent,
            Some("Delete Worktree Remote Parent"),
        )
        .await;
        let worktree_project =
            add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

        let response = route_http(
            state.clone(),
            rpc_request(
                "/api/deleteWorktreeProject",
                &token,
                json!({
                    "params": {
                        "deleteRemoteBranch": true,
                        "projectId": worktree_project["projectId"]
                    }
                }),
            ),
            "request-delete-dirty-worktree".to_string(),
        )
        .await;
        assert_eq!(response.response.status(), StatusCode::OK);
        let body = response_json(response.response).await;
        assert_eq!(
            body["result"]["checkoutRemoval"],
            json!({ "forced": true, "retriedForSubmodules": false })
        );
        assert!(body["result"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["kind"] == "remoteBranchDeleteFailed"));
        assert!(!worktree.exists());

        let projects = route_http(
            state,
            rpc_request("/api/listProjects", &token, json!({ "params": {} })),
            "request-list-after-delete-dirty".to_string(),
        )
        .await;
        let body = response_json(projects.response).await;
        assert!(!body["result"]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["projectId"] == worktree_project["projectId"]));
    }

    #[tokio::test]
    async fn delete_worktree_project_route_retries_clean_initialized_submodule_with_force() {
        if !git_available() {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let submodule = paths.root_dir.join("delete-worktree-submodule-source");
        let parent = paths.root_dir.join("delete-worktree-submodule-parent");
        let worktree = paths
            .root_dir
            .join("delete-worktree-submodule-parent-feature");
        create_git_repository_for_server_test(&submodule);
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(
            &parent,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                path_to_string(&submodule).as_str(),
                "deps/submodule",
            ],
        );
        run_git_for_server_test(&parent, &["commit", "-m", "add submodule"]);
        run_git_for_server_test(&parent, &["branch", "feature-submodule"]);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                path_to_string(&worktree).as_str(),
                "feature-submodule",
            ],
        );
        run_git_for_server_test(
            &worktree,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "--recursive",
            ],
        );

        add_project_path_for_server_test(
            state.clone(),
            &token,
            &parent,
            Some("Delete Worktree Submodule Parent"),
        )
        .await;
        let worktree_project =
            add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

        let response = route_http(
            state,
            rpc_request(
                "/api/deleteWorktreeProject",
                &token,
                json!({
                    "params": {
                        "projectId": worktree_project["projectId"]
                    }
                }),
            ),
            "request-delete-submodule-worktree".to_string(),
        )
        .await;
        assert_eq!(response.response.status(), StatusCode::OK);
        let body = response_json(response.response).await;
        assert_eq!(
            body["result"]["checkoutRemoval"],
            json!({ "forced": true, "retriedForSubmodules": true })
        );
        assert!(!worktree.exists());
    }

    #[tokio::test]
    async fn t3_runtime_endpoints_require_auth_and_stay_on_the_local_listener() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));

        for path in [
            "/api/t3Runtime/status",
            "/api/t3Runtime/start",
            "/api/t3Runtime/stop",
            "/api/t3Runtime/panes",
        ] {
            let endpoint = endpoint_for(path).expect("t3 runtime endpoint");
            assert_eq!(endpoint.permission, ApiPermission::RemoteBlocked);
            assert!(endpoint.requires_auth);
            assert!(endpoint.requires_protocol_version);
            assert_eq!(endpoint.transport, Transport::Http);
            assert!(!is_remote_endpoint_allowed(
                ListenerKind::Remote,
                endpoint.permission
            ));

            let response = route_http(
                state.clone(),
                rpc_request(path, "wrong-token", json!({ "params": {} })),
                "request-t3-runtime-auth".to_string(),
            )
            .await;
            assert_eq!(response.response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn t3_runtime_status_route_returns_the_stopped_contract_shape() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let response = route_http(
            state,
            rpc_request("/api/t3Runtime/status", &token, json!({ "params": {} })),
            "request-t3-runtime-status".to_string(),
        )
        .await;

        assert_eq!(response.response.status(), StatusCode::OK);
        let body = response_json(response.response).await;
        assert_eq!(
            body["result"]["t3Runtime"],
            json!({
                "running": false,
                "port": 3774,
                "authReady": false,
            })
        );
    }

    #[tokio::test]
    async fn t3_runtime_stop_route_is_a_clean_no_op_when_not_running() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let response = route_http(
            state,
            rpc_request("/api/t3Runtime/stop", &token, json!({ "params": {} })),
            "request-t3-runtime-stop".to_string(),
        )
        .await;

        assert_eq!(response.response.status(), StatusCode::OK);
        let body = response_json(response.response).await;
        assert_eq!(body["result"]["t3Runtime"]["running"], json!(false));
        assert!(body["result"]["t3Runtime"].get("pid").is_none());
        assert!(body["result"]["t3Runtime"].get("ownership").is_none());
    }

    #[tokio::test]
    async fn t3_runtime_start_route_rejects_invalid_params_before_launching() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();

        let missing_cwd = route_http(
            state.clone(),
            rpc_request("/api/t3Runtime/start", &token, json!({ "params": {} })),
            "request-t3-runtime-start-missing-cwd".to_string(),
        )
        .await;
        assert_eq!(missing_cwd.response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(missing_cwd.response).await;
        assert_eq!(body["error"], json!("badRequest"));

        let one_sided_plan = route_http(
            state,
            rpc_request(
                "/api/t3Runtime/start",
                &token,
                json!({
                    "params": {
                        "cwd": path_to_string(temp.path()),
                        "nodePath": "/usr/bin/env",
                    }
                }),
            ),
            "request-t3-runtime-start-one-sided-plan".to_string(),
        )
        .await;
        assert_eq!(one_sided_plan.response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn t3_runtime_panes_route_validates_params_and_touches_the_heartbeat_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let token = state.auth_token.clone();
        let heartbeat_file = state.t3_runtime.t3_paths().heartbeat_file.clone();

        let missing_sessions = route_http(
            state.clone(),
            rpc_request(
                "/api/t3Runtime/panes",
                &token,
                json!({ "params": { "clientId": "gpui" } }),
            ),
            "request-t3-runtime-panes-missing-sessions".to_string(),
        )
        .await;
        assert_eq!(missing_sessions.response.status(), StatusCode::BAD_REQUEST);
        assert!(!heartbeat_file.exists());

        let live_panes = route_http(
            state.clone(),
            rpc_request(
                "/api/t3Runtime/panes",
                &token,
                json!({ "params": { "clientId": "gpui", "sessionIds": ["G1", "G2"] } }),
            ),
            "request-t3-runtime-panes-live".to_string(),
        )
        .await;
        assert_eq!(live_panes.response.status(), StatusCode::OK);
        let body = response_json(live_panes.response).await;
        assert_eq!(body["result"]["t3Runtime"]["running"], json!(false));
        assert!(heartbeat_file.exists());
        assert!(state.t3_runtime.heartbeat_task_is_running());

        let empty_panes = route_http(
            state.clone(),
            rpc_request(
                "/api/t3Runtime/panes",
                &token,
                json!({ "params": { "clientId": "gpui", "sessionIds": [] } }),
            ),
            "request-t3-runtime-panes-empty".to_string(),
        )
        .await;
        assert_eq!(empty_panes.response.status(), StatusCode::OK);
        assert!(!state.t3_runtime.heartbeat_task_is_running());
    }

    // -----------------------------------------------------------------------
    // Sidebar V2 worktree sessions
    // -----------------------------------------------------------------------

    async fn worktree_session_context_for_test(
        state: Arc<AppState>,
        project_id: &str,
    ) -> ProjectWorktreeOperationContext {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        match resolve_project_worktree_operation_context(&state, &params) {
            Ok(context) => context,
            Err(_) => panic!("worktree operation context"),
        }
    }

    async fn set_worktree_command_for_test(
        state: Arc<AppState>,
        token: &str,
        project_id: &str,
        command: &str,
    ) {
        let response = route_http(
            state,
            rpc_request(
                "/api/updateProject",
                token,
                json!({
                    "params": {
                        "gitConfig": { "worktreeCommand": command },
                        "projectId": project_id,
                    }
                }),
            ),
            "request-set-worktree-command".to_string(),
        )
        .await;
        assert_eq!(response.response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn worktree_session_checkout_creates_a_temp_branch_and_runs_the_setup_command() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("worktree-session-parent");
        create_git_repository_for_server_test(&parent);
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
        let project_id = project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        set_worktree_command_for_test(
            state.clone(),
            &token,
            &project_id,
            "printf 'setup\\n' > setup-ran.txt",
        )
        .await;

        let context = worktree_session_context_for_test(state.clone(), &project_id).await;
        let request = normalize_worktree_session_create_request(&Map::new()).expect("request");
        let prepared = match prepare_worktree_session_checkout(&state, &context, &request).await {
            Ok(prepared) => prepared,
            Err(_) => panic!("prepare worktree checkout"),
        };

        assert!(prepared.created);
        assert!(
            worktree_sessions::is_worktree_temp_branch(&prepared.branch),
            "unexpected branch {}",
            prepared.branch
        );
        assert!(Path::new(&prepared.path).is_dir());
        assert!(
            Path::new(&prepared.path).join("setup-ran.txt").is_file(),
            "the project's worktree setup command runs inside the new checkout"
        );
        assert_eq!(
            run_git_for_server_test(Path::new(&prepared.path), &["branch", "--show-current"])
                .trim(),
            prepared.branch
        );
        assert_eq!(
            run_git_status_for_server_test(
                &parent,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/heads/{}", prepared.branch)
                ]
            )
            .status
            .code(),
            Some(0)
        );
        // The worktree is a session attribute, never a registered project.
        let projects = route_http(
            state.clone(),
            rpc_request("/api/listProjects", &token, json!({ "params": {} })),
            "request-list-after-worktree-session".to_string(),
        )
        .await;
        let body = response_json(projects.response).await;
        assert_eq!(body["result"]["projects"].as_array().unwrap().len(), 1);

        // An explicit base branch seeds the checkout from that branch's tip.
        run_git_for_server_test(&parent, &["checkout", "--quiet", "-b", "seed-branch"]);
        fs::write(parent.join("seed.txt"), "seed\n").expect("seed file");
        run_git_for_server_test(&parent, &["add", "seed.txt"]);
        run_git_for_server_test(&parent, &["commit", "-m", "seed"]);
        run_git_for_server_test(&parent, &["checkout", "--quiet", "-"]);
        let mut base_params = Map::new();
        base_params.insert("baseBranch".to_string(), json!("seed-branch"));
        let base_request =
            normalize_worktree_session_create_request(&base_params).expect("request");
        let based = match prepare_worktree_session_checkout(&state, &context, &base_request).await {
            Ok(prepared) => prepared,
            Err(_) => panic!("prepare worktree checkout from base branch"),
        };
        assert!(Path::new(&based.path).join("seed.txt").is_file());

        // Without a remote there is nothing to start from, and the refusal is
        // explicit instead of silently falling back to the local branch.
        let mut origin_params = Map::new();
        origin_params.insert("baseBranch".to_string(), json!("seed-branch"));
        origin_params.insert("startFromOrigin".to_string(), json!(true));
        let origin_request =
            normalize_worktree_session_create_request(&origin_params).expect("request");
        let error = prepare_worktree_session_checkout(&state, &context, &origin_request)
            .await
            .err()
            .expect("origin failure");
        match error {
            ProjectWorktreeOperationError::Domain(error) => {
                assert!(error.message.contains("origin/seed-branch"));
            }
            _ => panic!("expected a domain failure"),
        }
    }

    #[tokio::test]
    async fn worktree_session_checkout_rolls_back_when_the_setup_command_fails() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("worktree-session-rollback-parent");
        create_git_repository_for_server_test(&parent);
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
        let project_id = project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        set_worktree_command_for_test(state.clone(), &token, &project_id, "exit 3").await;

        let context = worktree_session_context_for_test(state.clone(), &project_id).await;
        let request = normalize_worktree_session_create_request(&Map::new()).expect("request");
        let error = prepare_worktree_session_checkout(&state, &context, &request)
            .await
            .err()
            .expect("setup failure");
        match error {
            ProjectWorktreeOperationError::Typed(error) => {
                assert!(error.message.contains("Worktree setup command failed."));
            }
            _ => panic!("expected a typed operation failure"),
        }

        let worktrees =
            run_git_for_server_test(&parent, &["worktree", "list", "--porcelain"]).to_string();
        assert_eq!(
            worktrees.matches("worktree ").count(),
            1,
            "the failed checkout is removed again: {worktrees}"
        );
        let branches = run_git_for_server_test(&parent, &["branch", "--list", "ghostex/*"]);
        assert!(
            branches.trim().is_empty(),
            "the temp branch is deleted too: {branches}"
        );
        let siblings = fs::read_dir(&paths.root_dir)
            .expect("root dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("worktree-session-rollback-parent-")
            })
            .count();
        assert_eq!(siblings, 0, "no stray worktree directory survives");
    }

    #[tokio::test]
    async fn create_worktree_session_route_rejects_a_foreign_existing_worktree_path() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("worktree-session-foreign-parent");
        let foreign = paths.root_dir.join("worktree-session-foreign-other");
        create_git_repository_for_server_test(&parent);
        create_git_repository_for_server_test(&foreign);
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

        let response = route_http(
            state.clone(),
            rpc_request(
                "/api/createWorktreeSession",
                &token,
                json!({
                    "params": {
                        "existingWorktree": { "path": path_to_string(&foreign) },
                        "projectId": project["projectId"],
                    }
                }),
            ),
            "request-create-worktree-session-foreign".to_string(),
        )
        .await;
        assert_eq!(response.response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(response.response).await;
        assert_eq!(body["error"], json!("badRequest"));
        assert!(body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("is not a worktree of this project"));

        let missing = route_http(
            state,
            rpc_request(
                "/api/createWorktreeSession",
                &token,
                json!({
                    "params": {
                        "existingWorktree": { "path": path_to_string(&paths.root_dir.join("nope")) },
                        "projectId": project["projectId"],
                    }
                }),
            ),
            "request-create-worktree-session-missing".to_string(),
        )
        .await;
        assert_eq!(missing.response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn remove_session_worktree_route_answers_dirty_before_removing_and_force_overrides() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("remove-session-worktree-dirty-parent");
        let worktree = paths
            .root_dir
            .join("remove-session-worktree-dirty-parent-0123abcd");
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                "-b",
                "ghostex/0123abcd",
                path_to_string(&worktree).as_str(),
            ],
        );
        fs::write(worktree.join("README.md"), "dirty\n").expect("dirty file");
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

        let dirty = route_http(
            state.clone(),
            rpc_request(
                "/api/removeSessionWorktree",
                &token,
                json!({
                    "params": {
                        "projectId": project["projectId"],
                        "worktreePath": path_to_string(&worktree),
                    }
                }),
            ),
            "request-remove-session-worktree-dirty".to_string(),
        )
        .await;
        assert_eq!(dirty.response.status(), StatusCode::OK);
        let body = response_json(dirty.response).await;
        assert_eq!(body["result"]["removed"], json!(false));
        assert_eq!(body["result"]["dirty"], json!(true));
        assert_eq!(
            body["result"]["warnings"],
            json!(["This worktree has uncommitted changes."])
        );
        assert!(
            worktree.is_dir(),
            "a dirty worktree is never removed silently"
        );

        let forced = route_http(
            state,
            rpc_request(
                "/api/removeSessionWorktree",
                &token,
                json!({
                    "params": {
                        "force": true,
                        "projectId": project["projectId"],
                        "worktreePath": path_to_string(&worktree),
                    }
                }),
            ),
            "request-remove-session-worktree-force".to_string(),
        )
        .await;
        assert_eq!(forced.response.status(), StatusCode::OK);
        let body = response_json(forced.response).await;
        assert_eq!(body["result"]["removed"], json!(true));
        assert_eq!(body["result"]["dirty"], json!(true));
        assert_eq!(body["result"]["warnings"], json!([]));
        assert!(!worktree.exists());
        assert_eq!(
            run_git_status_for_server_test(
                &parent,
                &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
            )
            .status
            .code(),
            Some(128),
            "force deletes the managed temp branch too"
        );
    }

    #[tokio::test]
    async fn remove_session_worktree_route_keeps_branches_it_does_not_manage() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("remove-session-worktree-clean-parent");
        let managed = paths
            .root_dir
            .join("remove-session-worktree-clean-parent-0123abcd");
        let foreign = paths
            .root_dir
            .join("remove-session-worktree-clean-parent-feature");
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                "-b",
                "ghostex/0123abcd",
                path_to_string(&managed).as_str(),
            ],
        );
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                "-b",
                "feature-work",
                path_to_string(&foreign).as_str(),
            ],
        );
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

        let removed = route_http(
            state.clone(),
            rpc_request(
                "/api/removeSessionWorktree",
                &token,
                json!({
                    "params": {
                        "projectId": project["projectId"],
                        "worktreePath": path_to_string(&managed),
                    }
                }),
            ),
            "request-remove-session-worktree-clean".to_string(),
        )
        .await;
        assert_eq!(removed.response.status(), StatusCode::OK);
        let body = response_json(removed.response).await;
        assert_eq!(body["result"]["removed"], json!(true));
        assert_eq!(body["result"]["dirty"], json!(false));
        assert_eq!(body["result"]["warnings"], json!([]));
        assert!(!managed.exists());
        assert_eq!(
            run_git_status_for_server_test(
                &parent,
                &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
            )
            .status
            .code(),
            Some(128)
        );

        let untouched = route_http(
            state.clone(),
            rpc_request(
                "/api/removeSessionWorktree",
                &token,
                json!({
                    "params": {
                        "projectId": project["projectId"],
                        "worktreePath": path_to_string(&foreign),
                    }
                }),
            ),
            "request-remove-session-worktree-foreign-branch".to_string(),
        )
        .await;
        assert_eq!(untouched.response.status(), StatusCode::OK);
        let body = response_json(untouched.response).await;
        assert_eq!(body["result"]["removed"], json!(true));
        assert!(!foreign.exists());
        assert_eq!(
            run_git_status_for_server_test(
                &parent,
                &["rev-parse", "--verify", "refs/heads/feature-work"]
            )
            .status
            .code(),
            Some(0),
            "a branch gxserver did not mint survives the worktree removal"
        );

        let outside = route_http(
            state,
            rpc_request(
                "/api/removeSessionWorktree",
                &token,
                json!({
                    "params": {
                        "projectId": project["projectId"],
                        "worktreePath": path_to_string(&parent),
                    }
                }),
            ),
            "request-remove-session-worktree-main".to_string(),
        )
        .await;
        assert_eq!(
            outside.response.status(),
            StatusCode::BAD_REQUEST,
            "the project's own checkout is not a removable worktree"
        );
    }

    #[tokio::test]
    async fn remove_session_worktree_route_refuses_a_registered_worktree_project() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths
            .root_dir
            .join("remove-session-worktree-registered-parent");
        let registered = paths
            .root_dir
            .join("remove-session-worktree-registered-parent-0123abcd");
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                "-b",
                "ghostex/0123abcd",
                path_to_string(&registered).as_str(),
            ],
        );
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
        // The V1 flow's registration: the worktree is a project in its own right.
        add_project_path_for_server_test(state.clone(), &token, &registered, Some("Worktree"))
            .await;

        let refused = route_http(
            state,
            rpc_request(
                "/api/removeSessionWorktree",
                &token,
                json!({
                    "params": {
                        "projectId": project["projectId"],
                        "worktreePath": path_to_string(&registered),
                    }
                }),
            ),
            "request-remove-session-worktree-registered".to_string(),
        )
        .await;
        assert_eq!(refused.response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(refused.response).await;
        assert_eq!(body["error"], json!("badRequest"));
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("registered as its own project"),
            "the refusal points at the project delete flow: {}",
            body["message"]
        );
        assert!(
            registered.is_dir(),
            "a registered worktree project's checkout survives the refusal"
        );
        assert_eq!(
            run_git_status_for_server_test(
                &parent,
                &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
            )
            .status
            .code(),
            Some(0),
            "its branch survives too"
        );
    }

    #[tokio::test]
    async fn worktree_branch_rename_pass_renames_only_a_titled_temp_branch() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let parent = paths.root_dir.join("rename-parent");
        let worktree = paths.root_dir.join("rename-parent-0123abcd");
        create_git_repository_for_server_test(&parent);
        run_git_for_server_test(
            &parent,
            &[
                "worktree",
                "add",
                "-b",
                "ghostex/0123abcd",
                path_to_string(&worktree).as_str(),
            ],
        );
        let project =
            add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
        let marker = worktree_sessions::worktree_session_marker_value(
            "ghostex/0123abcd",
            &path_to_string(&worktree),
            "Codex Session",
            "2026-07-29T00:00:00.000Z",
        );
        let created = route_http(
            state.clone(),
            rpc_request(
                "/api/createSession",
                &token,
                json!({
                    "params": {
                        "cwd": path_to_string(&worktree),
                        "kind": "terminal",
                        "projectId": project["projectId"],
                        "runtimeSettings": {
                            worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker.clone(),
                        },
                        "title": "Codex Session",
                    }
                }),
            ),
            "request-create-worktree-session-row".to_string(),
        )
        .await;
        assert_eq!(created.response.status(), StatusCode::OK);
        let session = response_json(created.response).await["result"]["session"].clone();

        // A row still carrying its creation title is not due a rename.
        run_worktree_branch_rename_once(&state).expect("rename pass");
        assert_eq!(
            run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
            "ghostex/0123abcd"
        );

        let renamed = route_http(
            state.clone(),
            rpc_request(
                "/api/updateSession",
                &token,
                json!({
                    "params": {
                        "projectId": session["projectId"],
                        "runtimeSettings": {
                            "titleSource": "generated",
                            worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker,
                        },
                        "sessionId": session["sessionId"],
                        "title": "Fix the flaky login test",
                    }
                }),
            ),
            "request-title-worktree-session-row".to_string(),
        )
        .await;
        assert_eq!(renamed.response.status(), StatusCode::OK);

        run_worktree_branch_rename_once(&state).expect("rename pass");
        assert_eq!(
            run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
            "ghostex/fix-the-flaky-login-test"
        );

        // The marker now records the new branch, so the next pass is a no-op.
        let listed = route_http(
            state.clone(),
            rpc_request(
                "/api/listSessions",
                &token,
                json!({ "params": { "projectId": session["projectId"] } }),
            ),
            "request-list-renamed-worktree-session".to_string(),
        )
        .await;
        let body = response_json(listed.response).await;
        let stored = body["result"]["sessions"][0].clone();
        assert_eq!(
            stored["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY]["branch"],
            json!("ghostex/fix-the-flaky-login-test")
        );
        assert!(
            stored["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY]["renamedAt"]
                .is_string()
        );
        run_worktree_branch_rename_once(&state).expect("rename pass");
        assert_eq!(
            run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
            "ghostex/fix-the-flaky-login-test"
        );
    }

    fn test_app_state(paths: GxserverPaths) -> Arc<AppState> {
        let storage = initialize_gxserver_storage(&paths).expect("storage");
        let config = create_default_gxserver_config().expect("config");
        let metadata = RuntimeMetadata {
            build_identity: "test-build".to_string(),
            pid: std::process::id(),
            port: config.listeners.local.port,
            protocol_version: GXSERVER_PROTOCOL_VERSION,
            server_id: "S7k".to_string(),
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
        let t3_runtime = crate::t3_runtime::test_t3_runtime_manager(&paths);
        Arc::new(AppState {
            auth_token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            automation_runtime,
            build_identity: "test-build".to_string(),
            config,
            event_hub: GxserverEventHub::new(metadata.server_id.clone()),
            logger: Arc::new(GxserverLogger::new(paths.clone())),
            metadata,
            migration: create_gxserver_migration_status(&storage),
            paths,
            presentation_event_sequence: Arc::new(Mutex::new(())),
            repository_clone_jobs: RepositoryCloneJobManager::default(),
            shutdown_tx,
            stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
            t3_runtime,
            version: "0.0.0-test".to_string(),
            zmx_title_observers: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    async fn add_project_path_for_server_test(
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

    fn git_available() -> bool {
        StdCommand::new("git")
            .arg("--version")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn create_git_repository_for_server_test(repository_path: &Path) {
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

    fn run_git_for_server_test(cwd: &Path, args: &[&str]) -> String {
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

    fn run_git_status_for_server_test(cwd: &Path, args: &[&str]) -> std::process::Output {
        StdCommand::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git command")
    }

    fn test_health(build_identity: &str) -> ServerHealthResponse {
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
            portless: crate::portless::unavailable_portless_status_payload(),
            port: config.listeners.local.port,
            server_id: "S7k".to_string(),
            started_at: "2026-05-30T10:00:00.000Z".to_string(),
            t3_runtime: None,
            tools: vec![],
        }
    }

    fn rpc_request(path: &str, token: &str, body: Value) -> Request<Body> {
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

    async fn response_json(response: Response<Body>) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        serde_json::from_slice(&bytes).expect("json body")
    }
}
