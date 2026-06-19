use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Instant};

use anyhow::{anyhow, Context, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
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
use tokio::{net::TcpListener, sync::broadcast};
use uuid::Uuid;

use crate::{
    agent_hooks::{install_agent_hooks, read_agent_hook_status, uninstall_agent_hooks},
    agent_skills::{install_agent_skills, read_agent_skill_status},
    agents::{dispatch_agent_endpoint, AgentEndpointError},
    auth::{
        ensure_gxserver_auth_token, is_authorized_headers, is_expected_gxserver_auth_token,
        read_gxserver_auth_token,
    },
    config::{read_gxserver_config, GxserverConfig},
    constants::{
        GXSERVER_CAPABILITIES, GXSERVER_JSON_BODY_LIMIT_BYTES, GXSERVER_PRODUCT,
        GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION,
    },
    domain::{
        read_domain_rpc_params, read_optional_project_id, read_project_id, DomainRepository,
        DomainStateError,
    },
    events::{EventClientSender, GxserverEventHub},
    http_client,
    identity::ensure_gxserver_identity,
    logging::{
        log_level_from_status, query_gxserver_logs, GxserverLogInput, GxserverLogger, LogQueryError,
    },
    paths::{get_gxserver_paths, GxserverPaths},
    presentation::{
        build_presentation_project_delta, build_presentation_session_delta,
        increment_presentation_revision, list_previous_sessions, read_presentation_snapshot,
        search_presentation_sessions,
    },
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
    storage::{
        create_gxserver_migration_status, initialize_gxserver_storage, open_gxserver_database,
    },
    toolchain::get_gxserver_tool_statuses,
    typed_operations::{
        dispatch_typed_operation_endpoint, typed_operation_log_details, typed_operation_log_level,
        TypedOperationError,
    },
    zmx::{
        dispatch_zmx_lifecycle_endpoint, dispatch_zmx_session_interaction_endpoint,
        merge_session_with_renderer_result, prepare_focus_session_renderer_command,
        ZmxEndpointError, ZmxServerContext,
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

#[derive(Clone)]
struct AppState {
    auth_token: String,
    build_identity: String,
    config: GxserverConfig,
    event_hub: GxserverEventHub,
    logger: Arc<GxserverLogger>,
    metadata: RuntimeMetadata,
    migration: MigrationStatus,
    paths: GxserverPaths,
    repository_clone_jobs: RepositoryCloneJobManager,
    shutdown_tx: broadcast::Sender<()>,
    version: String,
}

struct RoutedResponse {
    endpoint_path: Option<String>,
    response: Response<Body>,
}

const RENDERER_COMMAND_ACTIONS: &[&str] = &[
    "assertSidebarCard",
    "automationArchiveRun",
    "automationMarkRunRead",
    "automationRunNow",
    "automationSave",
    "automationSetEnabled",
    "automationState",
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
    "toggleSidebarCollapsed",
    "waitFor",
];

/*
CDXC:GxserverRustPort 2026-06-14-20:37:
Phase 1 must be a real foreground daemon, not a mock harness. Startup creates TypeScript-compatible auth, config, identity, SQLite, runtime metadata, logs directory, local HTTP listener, health/control endpoints, and the minimal event stream needed by Phase 0 compatibility.
*/
pub async fn run_gxserver_foreground(
    options: GxserverForegroundOptions,
) -> Result<GxserverForegroundResult> {
    let version = options.version;
    let build_identity = options
        .build_identity
        .unwrap_or_else(|| create_source_build_identity(&version));
    let paths = get_gxserver_paths(options.home_dir);

    if let Some(existing_auth) = read_gxserver_auth_token(&paths)? {
        if let Ok(Some(existing)) =
            http_client::fetch_server_health(Some(&existing_auth.token), 800)
        {
            if is_build_identity_reusable(Some(&existing.build_identity), Some(&build_identity)) {
                return Ok(GxserverForegroundResult { reused: true });
            }
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

    let state = Arc::new(AppState {
        auth_token: auth.token,
        build_identity,
        config,
        event_hub,
        logger: logger.clone(),
        metadata: metadata.clone(),
        migration,
        paths: paths.clone(),
        repository_clone_jobs: RepositoryCloneJobManager::default(),
        shutdown_tx: shutdown_tx.clone(),
        version,
    });
    let app = Router::new()
        .route("/api/events", any(handle_events))
        .fallback(any(handle_http))
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

    let mut shutdown_rx = shutdown_tx.subscribe();
    let shutdown_for_signal = shutdown_tx.clone();
    let state_for_signal = state.clone();
    tokio::spawn(async move {
        wait_for_process_signal().await;
        broadcast_server_stopping(&state_for_signal);
        let _ = shutdown_for_signal.send(());
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        })
        .await
        .with_context(|| "run gxserver HTTP listener")?;

    remove_runtime_metadata(&paths)?;
    Ok(GxserverForegroundResult { reused: false })
}

async fn handle_http(State(state): State<Arc<AppState>>, request: Request<Body>) -> Response<Body> {
    let started_at = Instant::now();
    let client = request
        .extensions()
        .get::<SocketAddr>()
        .map(|address| address.ip().to_string());
    let headers = request.headers().clone();
    let request_id = request_id(&headers);
    let mut routed = route_http(state.clone(), request, request_id.clone()).await;
    apply_cors_headers(&headers, &mut routed.response, &state.config);
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
    routed.response
}

async fn route_http(
    state: Arc<AppState>,
    request: Request<Body>,
    request_id: String,
) -> RoutedResponse {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.clone();

    if method == Method::GET && path == "/api/health" {
        return routed_json(
            Some("/api/health".to_string()),
            StatusCode::OK,
            MinimalHealthResponse::new(&state.version),
        );
    }

    let Some(endpoint) = endpoint_for(&path) else {
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

    if method == Method::OPTIONS {
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

    if endpoint.transport != Transport::Http {
        return routed_json(
            Some(endpoint.path),
            StatusCode::NOT_FOUND,
            rpc_error(
                "notFound",
                format!("No gxserver endpoint for {}.", path),
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
                )
            }
            Err(ReadBodyError::InvalidJson) => {
                return routed_json(
                    Some(endpoint.path),
                    StatusCode::BAD_REQUEST,
                    rpc_error("badRequest", "Request body must be valid JSON.", Some(request_id)),
                )
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
        "/api/readProjectStatus" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.read_project_status(params),
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
        "/api/createSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let session = repository.create_session(params, false)?;
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
                let session = repository.create_session(params, true)?;
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
            |repository, _, params, _| {
                let project_id = read_optional_project_id(params)?;
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
            |_, db, _, server_id| {
                read_presentation_snapshot(db, server_id)
                    .map(|snapshot| json!({ "snapshot": snapshot }))
            },
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
        "/api/attachSessionMetadata"
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
        "/api/queryLogs" => handle_query_logs_http(&state, endpoint.path, request_id, &body_json),
        "/api/previewRepositoryClone"
        | "/api/startRepositoryClone"
        | "/api/readRepositoryCloneJob"
        | "/api/cancelRepositoryCloneJob" => {
            handle_repository_clone_http(&state, endpoint.path, request_id, &body_json).await
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
            )
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
            )
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
    match dispatch_agent_endpoint(&repository, &db, &endpoint_path, &params, Some(&context)) {
        Ok(output) => {
            if let Some((project_id, session_id)) = output.presentation_session {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
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
        Err(error) => agent_error_response(endpoint_path, request_id, error),
    }
}

fn handle_agent_hook_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    /*
    CDXC:AgentHooks 2026-06-19-14:15:
    Hook uninstall is routed with read/install through the local authenticated RPC envelope. The handler returns the TypeScript-compatible status payload plus removedPaths without writing provider paths, hook commands, or hook file contents to persistent logs.
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
                )
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
        Err(error) => typed_operation_error_response(endpoint_path, request_id, error),
    }
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
        logger: state.logger.clone(),
        paths: state.paths.clone(),
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
            )
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
    match dispatch_zmx_lifecycle_endpoint(&repository, &endpoint_path, &params, &context) {
        Ok(output) => {
            if let Some((project_id, session_id)) = output.presentation_session {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
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
                    )
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
            )
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
            )
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
            )
        }
    };
    let timeout_ms = match normalize_renderer_command_timeout_ms(params.get("timeoutMs")) {
        Ok(timeout_ms) => timeout_ms,
        Err(message) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", message, Some(request_id)),
            )
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
    let delta = build_presentation_session_delta(repository, project_id, session_id)?;
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
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    uri: Uri,
) -> Response<Body> {
    let request_id = request_id(&headers);
    /*
    CDXC:GxserverPresentationEvents 2026-06-14-20:37:
    Browser WebSocket clients cannot set Authorization headers, so Rust keeps the TypeScript authToken query option and protocolVersion query/header gate for /api/events.
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

async fn handle_event_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = state.event_hub.client_channel();
    let mut broadcast_rx = state.event_hub.subscribe();
    let _ = outbound_tx.send(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": "eventStreamReady",
    }));
    let sender_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                outbound = outbound_rx.recv() => {
                    let Some(event) = outbound else {
                        break;
                    };
                    if send_event_message(&mut socket_sender, event).await.is_err() {
                        break;
                    }
                }
                broadcast = broadcast_rx.recv() => {
                    match broadcast {
                        Ok(event) => {
                            if send_event_message(&mut socket_sender, event).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });
    let mut renderer_client_id: Option<String> = None;
    while let Some(message) = socket_receiver.next().await {
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
    sender_task.abort();
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
            if parsed.get("rendererCommands").and_then(Value::as_bool) == Some(true) {
                let client_id = format!("renderer-client-{}", Uuid::new_v4());
                *renderer_client_id = Some(
                    state
                        .event_hub
                        .register_renderer_client(client_id, outbound_tx.clone())
                        .await,
                );
            }
            send_presentation_snapshot_for_subscription(state, outbound_tx, &parsed);
            true
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
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let Ok(snapshot) = read_presentation_snapshot(&db, &state.metadata.server_id) else {
        return;
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
    let _ = outbound_tx.send(Value::Object(event));
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
        port: state.metadata.port,
        server_id: state.metadata.server_id.clone(),
        started_at: state.metadata.started_at.clone(),
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
    query_value(uri, "protocolVersion").map(|value| parse_protocol_version(&value))
}

fn parse_protocol_version(value: &str) -> Value {
    value
        .parse::<u64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::String(value.to_string()))
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
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
    if !config
        .cors
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
    {
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
        Arc::new(AppState {
            auth_token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            build_identity: "test-build".to_string(),
            config,
            event_hub: GxserverEventHub::new(metadata.server_id.clone()),
            logger: Arc::new(GxserverLogger::new(paths.clone())),
            metadata,
            migration: create_gxserver_migration_status(&storage),
            paths,
            repository_clone_jobs: RepositoryCloneJobManager::default(),
            shutdown_tx,
            version: "0.0.0-test".to_string(),
        })
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
