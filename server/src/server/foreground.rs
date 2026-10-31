use anyhow::{anyhow, Context, Result};
use axum::{routing::any, Router};
use serde_json::json;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::broadcast};
use tower::service_fn;

use crate::{
    auth::{ensure_gxserver_auth_token, read_gxserver_auth_token},
    automations::AutomationRuntime,
    config::read_gxserver_config,
    constants::GXSERVER_PROTOCOL_VERSION,
    delayed_sends::DelayedSendRuntime,
    domain::DomainRepository,
    events::GxserverEventHub,
    extensions::ExtensionRegistry,
    http_client,
    identity::ensure_gxserver_identity,
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger},
    paths::get_gxserver_paths,
    protocol::{RuntimeMetadata, ServerHealthResponse},
    remote_access::RemotePairingRuntime,
    repository_clone::RepositoryCloneJobManager,
    runtime::{
        create_source_build_identity, is_build_identity_reusable, remove_runtime_metadata,
        write_runtime_metadata,
    },
    session_chat_follower::{
        stop_all_session_chat_followers, sync_session_chat_followers_for_all_sessions,
    },
    session_chat_queue_runtime::{
        session_chat_queue_compacting_refresher, session_chat_queue_composer_reader,
        session_chat_queue_notice_reader, session_chat_queue_publisher_factory,
        session_chat_queue_sender_factory, session_chat_queue_waker,
    },
    storage::{
        create_gxserver_migration_status, initialize_gxserver_storage, open_gxserver_database,
    },
    tailcat::{start_tailcat_from_persisted_state, TailcatRuntime},
    zmx::cycle_wire_incompatible_zmx_session_daemons,
};

use super::*;

/*
CDXC:RepoStructure 2026-06-14-20:37:
Phase 1 must be a real foreground daemon, not a mock harness. Startup creates TypeScript-compatible auth, config, identity, SQLite, runtime metadata, logs directory, local HTTP listener, health/control endpoints, and the minimal event stream needed by Phase 0 compatibility.

CDXC:ServerDaemon 2026-06-22-04:53:
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
                    // An old build that answers but will not stop is ended, so the current migration code still runs.
                    ExistingGxserverState::Running if end_stale_gxserver(existing.pid) => {}
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
    crate::storage::hold_gxserver_database_open(&paths)?;
    let config = read_gxserver_config(&paths)?;
    let identity = ensure_gxserver_identity(&paths)?;
    let auth = ensure_gxserver_auth_token(&paths)?;
    let logger = Arc::new(GxserverLogger::new(paths.clone()));
    crate::crash_report::report_previous_crash(&paths, &logger, &version);
    crate::crash_report::install(&paths);
    /*
    CDXC:AgentSkills 2026-08-24:
    Older Ghostex builds installed skills that are now folded into the CLI
    help. Clean up unmodified shipped copies on every launch so user machines
    converge on the consolidated skill set without manual steps.
    */
    let removed_retired_skills = crate::agent_skills::remove_retired_ghostex_agent_skills(&paths);
    if !removed_retired_skills.is_empty() {
        let _ = logger.log(GxserverLogInput {
            level: crate::logging::LogLevel::Warn,
            event: "retiredAgentSkillsRemoved".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({ "removed": removed_retired_skills })),
        });
    }
    #[cfg(windows)]
    if let Some(details) = crate::platform::launch_context::launch_context_warning_details() {
        let _ = logger.log(GxserverLogInput {
            level: crate::logging::LogLevel::Warn,
            event: "windowsLaunchContextLimited".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details),
        });
    }
    crate::agent_skills_remote::spawn_startup_skill_refresh(paths.clone(), logger.clone());
    let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    // Install age for the analytics heartbeat, taken before `identity` is
    // consumed by the runtime metadata below.
    let install_created_at = identity.created_at.clone();
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
    crate::agent_model_catalog::start(
        paths.clone(),
        event_hub.clone(),
        metadata.server_id.clone(),
        logger.clone(),
    );
    crate::agent_model_pins::init(&paths);
    let presentation_event_sequence = Arc::new(Mutex::new(()));
    let (shutdown_tx, _) = broadcast::channel(8);
    crate::zmx::set_zmx_process_identity_shutdown(shutdown_tx.subscribe());
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
        accounts: Arc::new(crate::accounts::runtime::AccountRuntime::default()),
        auth_token: auth.token,
        automation_runtime,
        delayed_send_runtime,
        board_start_work_gate: Arc::new(Mutex::new(())),
        build_identity,
        config,
        event_hub,
        extension_registry: ExtensionRegistry::new_with_api_url(
            &paths,
            format!("http://{local_host}:{local_port}"),
        ),
        logger: logger.clone(),
        metadata: metadata.clone(),
        migration,
        paths: paths.clone(),
        presentation_event_sequence,
        remote_pairing_runtime: RemotePairingRuntime::new(),
        repository_clone_jobs: RepositoryCloneJobManager::default(),
        session_chat_followers: Arc::new(Mutex::new(HashMap::new())),
        session_chat_option_cache: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: shutdown_tx.clone(),
        stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
        tailcat_runtime: TailcatRuntime::new(),
        version,
        zmx_title_observers: Arc::new(Mutex::new(HashMap::new())),
    });
    /*
    CDXC:Build 2026-06-24-20:22:
    The JSON RPC catch-all needs the raw Request so server can preserve the
    TypeScript protocol/auth/body gate order for every endpoint, including app
    user data. Use Axum's service fallback instead of the Handler extractor path
    so all non-/api/events requests still flow through the single RPC router.
    */
    let http_state = state.clone();
    let app = Router::new()
        .route("/api/events", any(handle_events))
        .route("/api/terminal", any(handle_terminal))
        .route("/api/browserTcp", any(browser_tcp::handle_browser_tcp))
        .route("/api/voice", any(crate::voice_agent::handle_voice))
        .fallback_service(service_fn(move |request| {
            handle_http_request(http_state.clone(), request)
        }))
        .with_state(state.clone());
    let startup_gate = StartupGate::new();
    let app = {
        let gate = startup_gate.clone();
        let gate_state = state.clone();
        app.layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                startup_gate_middleware(gate.clone(), gate_state.clone(), request, next)
            },
        ))
    };

    let address: SocketAddr = format!("{local_host}:{local_port}")
        .parse()
        .expect("valid listener address");
    let Some(listener) = bind_local_listener_taking_over_stale_holder(
        address,
        &paths,
        &state.build_identity,
        &logger,
    )
    .await?
    else {
        return Ok(GxserverForegroundResult { reused: true });
    };
    let loopback_v6_listener = match bind_loopback_ipv6_twin(address).await {
        Some(Ok(listener)) => Some(listener),
        Some(Err(error)) => {
            let _ = logger.log(GxserverLogInput {
                level: crate::logging::LogLevel::Warn,
                event: "loopbackIpv6ListenerUnavailable".to_string(),
                server_id: Some(metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.to_string()),
                details: Some(json!({ "port": local_port })),
            });
            None
        }
        None => None,
    };
    // Served from here on: until `startup_gate.open()` below every request is answered "starting".
    let cleanup_paths = paths.clone();
    let cleanup_owner = metadata.clone();
    let (metadata_cleanup_tx, metadata_cleanup_rx) = tokio::sync::oneshot::channel();
    let http_drain_rx = shutdown_tx.subscribe();
    let loopback_v6_task = loopback_v6_listener.map(|listener| {
        let mut shutdown_rx = shutdown_tx.subscribe();
        let app = app.clone();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.recv().await;
                })
                .await
        })
    });
    let mut serve_task = {
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.recv().await;
                    // Remove discovery while this daemon still holds the listener: a replacement
                    // cannot publish its record between our ownership check and unlink.
                    let _ = metadata_cleanup_tx
                        .send(remove_runtime_metadata(&cleanup_paths, &cleanup_owner));
                })
                .await
        })
    };

    startup_gate.step("recovering title jobs");
    title_job_recovery::recover_title_jobs_after_restart(&paths)?;
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
    startup_gate.step("starting its background runtimes");
    state.automation_runtime.start(shutdown_tx.subscribe());
    state.delayed_send_runtime.start(
        shutdown_tx.subscribe(),
        session_chat_queue_sender_factory(&state),
        session_chat_queue_publisher_factory(&state),
    );
    /*
    CDXC:SessionChat 2026-08-21:
    A queued prompt left in `sending` is ambiguous after a restart — the bytes
    may already have reached the agent — so it is retired as `failed` with an
    explicit reason and waits for the user. Never silently re-sent.
    */
    let _ = crate::session_chat_queue::recover_session_chat_queue_after_restart(&paths);
    crate::accounts::recovery::start(state.clone());
    crate::accounts::reset_watch::start(state.clone());
    crate::accounts::claude_swap_switch::start(state.clone());
    session_auto_sleep_sweep::start_session_auto_sleep_sweep(state.clone());
    sidebar_spaces_switch::start_sidebar_spaces_switch_watch(state.clone());
    workspaces_switch::start_workspaces_switch_watch(state.clone());
    crate::connectors::start_connectors(state.clone());
    bot_sync::start_bot_project_sync(state.clone());
    close_after_done_runtime::start_close_after_done_runtime(state.clone());
    coordinator_runtime::start_coordinator_runtime(state.clone());
    agent_message_delivery_runtime::start_agent_message_delivery_runtime(state.clone());
    crate::agentbox::start_agentbox_activity_poller(state.clone());
    crate::claude_cloud::start_claude_cloud_poller(state.clone());
    /*
    CDXC:SessionChat 2026-08-21:
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
        crate::session_chat_model_selection::sender(&state),
        session_chat_queue_publisher_factory(&state),
        session_chat_queue_notice_reader(&state),
        session_chat_queue_composer_reader(&state),
        session_chat_queue_compacting_refresher(&state),
        session_chat_queue_waker(&state),
        crate::session_chat_queue_runtime::session_chat_queue_stuck_notifier(&state),
    )
    .start(shutdown_tx.subscribe());
    /*
    CDXC:ZmxWireGeneration 2026-08-23:
    An app or remote-package update replaces the bundled zmx binary underneath
    daemons that keep running the code of the binary that started them, and the
    two ends of a broken IPC tag contract cannot talk at all — the user sees
    blank panes. Cycle those daemons before the first client can attach to one,
    and before the title observers and chat followers below subscribe to them,
    so each affected session is simply sleeping and the ordinary wake-on-open
    path restores it with its saved resume plan.
    */
    startup_gate.step("checking the session daemons");
    if let Ok(db) = open_gxserver_database(&paths) {
        let repository = DomainRepository::new(&db, metadata.server_id.as_str());
        cycle_wire_incompatible_zmx_session_daemons(&repository, &logger, &metadata.server_id);
        crate::zmx::end_unowned_zmx_session_daemons(&repository, &logger, &metadata.server_id);
    }
    /*
    CDXC:RemotePairing 2026-09-01:
    The tailcat sidecar is a child of THIS daemon, so a restart has to bring it
    back from persisted state rather than leaving remote access silently down
    until someone reopens Settings.
    */
    startup_gate.step("starting remote access");
    start_tailcat_from_persisted_state(&paths, &state.tailcat_runtime);
    startup_gate.step("watching sessions");
    crate::program_status::init_program_status_logger(state.logger.clone(), &metadata.server_id);
    sync_zmx_title_observers_for_all_sessions(&state, "server-start");
    sync_session_chat_followers_for_all_sessions(&state, "server-start");
    crate::voice_agent::recover_voice_agent_memory(&state);
    let agent_metadata_title_sync_task = spawn_agent_metadata_title_sync_task(&state);
    let session_lifecycle_sweep_task = spawn_session_lifecycle_sweep_task(&state);
    let session_git_status_refresh_task = spawn_session_git_status_refresh_task(&state);
    let worktree_branch_rename_task = spawn_worktree_branch_rename_task(&state);
    let session_chat_follower_sync_task = spawn_session_chat_follower_sync_task(&state);
    let session_chat_fleet_status_task =
        crate::session_chat_fleet_status::spawn_fleet_status_task(&state);
    let session_chat_async_question_status_task =
        crate::session_chat_async_questions::spawn_async_question_status_task(&state);
    crate::session_chat_mention_paths::install(&state.paths, &state.metadata.server_id);
    let session_chat_open_questions_task =
        crate::session_chat_open_questions::spawn_open_questions_task(&state);
    let freebuff_activity_task = crate::freebuff_activity::spawn_freebuff_activity_task(&state);
    let dsh_activity_task = crate::dsh_activity::spawn_dsh_activity_task(&state);
    let app_auto_update_task = crate::app_update::spawn_app_auto_update_task(&state);
    let zcode_turn_activity_task =
        crate::zcode_turn_activity::spawn_zcode_turn_activity_task(&state);
    let program_status_source_task = tokio::spawn(crate::program_status::run_program_status_watch(
        (*state).clone(),
    ));
    let team_sync_task = crate::team_sync::spawn_team_sync_task(&state);
    let superseded_retire_task = spawn_superseded_retire_task(&state);
    /*
    CDXC:Telemetry 2026-08-26:
    Analytics exists only in the long-running daemon. Starting it here rather
    than in `AppState` construction is what keeps one-shot `ghostex` CLI verbs
    — which build no server loop — completely silent.
    */
    let telemetry_tasks = spawn_telemetry_tasks(&state, install_created_at);

    let shutdown_for_signal = shutdown_tx.clone();
    let state_for_signal = state.clone();
    tokio::spawn(async move {
        wait_for_process_signal().await;
        broadcast_server_stopping(&state_for_signal);
        let _ = shutdown_for_signal.send(());
    });
    shutdown_bounds::arm_shutdown_exit_deadline(
        shutdown_tx.subscribe(),
        logger.clone(),
        metadata.server_id.clone(),
    );
    shutdown_bounds::stop_session_watchers_on_shutdown(&state);
    startup_gate.open();

    // The drain waits for every in-flight request; long waits are cut off (shutdown_bounds.rs).
    let serve_result = tokio::select! {
        result = &mut serve_task => result.unwrap_or_else(|error| Err(std::io::Error::other(error))),
        _ = shutdown_bounds::http_drain_limit_elapsed(http_drain_rx) => {
            serve_task.abort();
            shutdown_bounds::log_http_drain_abandoned(&state);
            Ok(())
        }
    };
    if let Some(task) = loopback_v6_task {
        task.abort();
    }
    let metadata_cleanup_result = metadata_cleanup_rx.await;
    if let Some(task) = superseded_retire_task {
        task.abort();
    }
    /*
    CDXC:ServerDaemon 2026-09-16 WHY:
    Everything from here to the return runs after the listener has closed, while clients already see "connection refused" and may start a replacement daemon.
    On 2026-09-16 that tail kept the 9.6.0 daemon alive for over two seconds after an app update asked it to stop, long enough for the desktop's launchd re-bootstrap to race it.
    The desktop now waits for the process, but a slow tail still delays every restart, so a tail over 1.5 s is logged with per-step timings to make the culprit visible.
    */
    let shutdown_tail_started = std::time::Instant::now();
    let mut shutdown_tail_steps: Vec<(&str, u128)> = Vec::new();
    let step_started = std::time::Instant::now();
    agent_metadata_title_sync_task.abort();
    session_lifecycle_sweep_task.abort();
    session_git_status_refresh_task.abort();
    worktree_branch_rename_task.abort();
    session_chat_follower_sync_task.abort();
    session_chat_fleet_status_task.abort();
    session_chat_async_question_status_task.abort();
    session_chat_open_questions_task.abort();
    freebuff_activity_task.abort();
    dsh_activity_task.abort();
    app_auto_update_task.abort();
    zcode_turn_activity_task.abort();
    program_status_source_task.abort();
    team_sync_task.abort();
    /*
    The telemetry flush task is AWAITED rather than aborted, because it does a
    final flush after the shutdown broadcast: aborting it would throw away
    everything captured since the last interval, i.e. most of a short session.
    The wait is bounded so a dead network can never delay quitting.
    */
    telemetry_tasks.heartbeat.abort();
    let _ = tokio::time::timeout(
        crate::telemetry::SHUTDOWN_FLUSH_TIMEOUT,
        telemetry_tasks.flush,
    )
    .await;
    shutdown_tail_steps.push(("telemetryFlush", step_started.elapsed().as_millis()));
    let step_started = std::time::Instant::now();
    crate::accounts::setup::cancel_all(&state);
    state.extension_registry.stop_all();
    crate::project_views::stop_all();
    shutdown_tail_steps.push((
        "accountsExtensionsProjectViews",
        step_started.elapsed().as_millis(),
    ));
    let step_started = std::time::Instant::now();
    state.tailcat_runtime.stop();
    shutdown_tail_steps.push(("tailcatStop", step_started.elapsed().as_millis()));
    serve_result.with_context(|| "run gxserver HTTP listener")?;

    if let Ok(result) = metadata_cleanup_result {
        result?;
    }
    let step_started = std::time::Instant::now();
    stop_all_zmx_title_observers(&state);
    stop_all_session_chat_followers(&state);
    shutdown_tail_steps.push((
        "zmxTitleObserversAndChatFollowers",
        step_started.elapsed().as_millis(),
    ));
    let shutdown_tail = shutdown_tail_started.elapsed();
    if shutdown_tail >= std::time::Duration::from_millis(1500) {
        let _ = state.logger.log(GxserverLogInput {
            level: crate::logging::LogLevel::Warn,
            event: "serverShutdownTailSlow".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: Some(shutdown_tail.as_millis()),
            error: None,
            details: Some(json!({
                "stepsMs": shutdown_tail_steps
                    .iter()
                    .map(|(name, ms)| (name.to_string(), json!(ms)))
                    .collect::<serde_json::Map<String, serde_json::Value>>(),
            })),
        });
    }
    Ok(GxserverForegroundResult { reused: false })
}

/// CDXC:ServerDaemon 2026-10-09 WHY:
/// gxserver also listens on `[::1]` at its own port, serving the same router and token, because an SSH forward to `localhost:<port>` must reach it. Windows OpenSSH resolves `localhost` to `::1` first, reports the forwarded channel open after that connect is refused, and never tries `127.0.0.1`, so every phone request through such a forward ended empty and the phone's chat could not send. `127.0.0.1` stays the address every client is given (`ghostex server endpoint`, runtime metadata, tunnels); this twin is never required, so a port taken on `::1` or IPv6 being off only logs a warning.
/// CDXC:ServerDaemon 2026-10-09 SEE-ALSO: `startPortForward` in apps/mobile/app/modules/ghostex-native/android/src/main/java/expo/modules/ghostexnative/GhostexSshConnection.kt, which now names `127.0.0.1` itself.
async fn bind_loopback_ipv6_twin(address: SocketAddr) -> Option<std::io::Result<TcpListener>> {
    if !matches!(address.ip(), std::net::IpAddr::V4(ip) if ip.is_loopback()) {
        return None;
    }
    let twin = SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, address.port()));
    Some(TcpListener::bind(twin).await)
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

pub(super) fn classify_existing_gxserver(
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
