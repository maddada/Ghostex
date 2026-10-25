use anyhow::{anyhow, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{
        header::{self, HeaderName, HeaderValue},
        HeaderMap, Method, Response, StatusCode, Uri,
    },
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::process::Command as StdCommand;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::broadcast,
};
use uuid::Uuid;

use crate::{
    agent_hooks::{
        install_agent_hooks, read_agent_hook_status, read_codex_hook_session_identities,
        uninstall_agent_hooks,
    },
    agents::{
        agent_metadata_title_revision, apply_created_session_identity,
        apply_live_process_session_identity, create_agent_session_params_for_project,
        default_agent_command, dispatch_agent_endpoint, get_visible_terminal_title,
        is_terminal_auto_working_directory_title, normalize_agent_hook_event_activity,
        one_shot_text_job_command, read_agent_settings, read_first_user_input_draft,
        read_text_from_map, reconcile_agent_metadata_title_for_session,
        resolve_project_agent_config, terminal_title_indicates_agent_identity, AgentEndpointError,
        FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY, FIRST_USER_INPUT_DRAFT_STATUS_KEY,
        FIRST_USER_INPUT_DRAFT_UPDATED_AT_KEY,
    },
    auth::{is_authorized_client_headers, is_authorized_client_token},
    automations::handle_automation_endpoint,
    config::GxserverConfig,
    constants::{
        GXSERVER_CAPABILITIES, GXSERVER_JSON_BODY_LIMIT_BYTES, GXSERVER_PRODUCT,
        GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION,
    },
    domain::{
        read_domain_rpc_params, read_project_id, read_session_id, DomainRepository,
        DomainStateError,
    },
    events::EventClientSender,
    ids::is_gxserver_project_id,
    logging::{
        query_gxserver_logs, DiagnosticLogScenario, GxserverLogInput, LogLevel, LogQueryError,
    },
    paths::GxserverPaths,
    platform::shell::command_shell,
    presentation::{
        build_presentation_project_delta, build_presentation_session_delta,
        increment_presentation_revision, read_presentation_snapshot,
    },
    project_docs, project_git_remote, project_icon,
    protocol::{
        protocol_mismatch_error, rpc_error, rpc_success, ApiPermission, MinimalHealthResponse,
        ServerHealthResponse,
    },
    repository_clone::{
        dispatch_repository_clone_endpoint, RepositoryCloneError, RepositoryCloneRuntime,
    },
    session_chat_follower::sync_session_chat_followers_for_all_sessions,
    session_git_status, session_keep_awake, session_lifecycle,
    session_status::{agent_activity_presentation_refresh_delay_ms, iso_from_ms},
    sidebar_hud::read_sidebar_hud_commands_by_project,
    sidebar_spaces::prune_sidebar_spaces_for_collections,
    source_control::{dispatch_source_control_endpoint, SourceControlError},
    storage::{open_gxserver_database, open_gxserver_database_with_busy_timeout},
    terminal_ws::{handle_terminal_socket, TerminalWsState},
    toolchain::{get_gxserver_tool_statuses, require_bundled_zmx, require_system_bd},
    typed_operations::{
        create_pull_request_for_project, dispatch_typed_operation_endpoint,
        dispatch_worktree_path_operation, typed_operation_log_details, typed_operation_log_level,
        TypedOperationError,
    },
    worktree_sessions,
    zmx::{
        append_zmx_endpoint_error_context, compensate_created_workspace_terminal,
        create_started_workspace_terminal, dispatch_zmx_lifecycle_endpoint,
        dispatch_zmx_session_interaction_endpoint, get_persisted_provider_startup_text_for_session,
        merge_session_with_renderer_result, prepare_focus_session_renderer_command,
        read_cached_zmx_existing_session_names, read_cached_zmx_session_process_identities,
        ZmxEndpointError, ZmxServerContext,
    },
};

#[cfg(test)]
use crate::{
    automations::AutomationRuntime,
    delayed_sends::DelayedSendRuntime,
    events::GxserverEventHub,
    extensions::ExtensionRegistry,
    logging::GxserverLogger,
    paths::get_gxserver_paths,
    protocol::{MigrationStatus, RuntimeMetadata},
    repository_clone::RepositoryCloneJobManager,
};
#[cfg(test)]
use axum::http::Request;

pub(crate) mod accounts_http;
pub(crate) mod agent_cli_http;
pub mod agent_http;
mod agent_message_delivery_runtime;
pub mod agent_prompt_search_http;
pub(crate) mod agentbox_http;
mod app_state;
pub mod background_tasks;
mod bot_sync;
mod browser_tcp;
mod close_after_done_runtime;
pub mod commit_message_generation;
mod coordinator_open_http;
mod coordinator_resolve;
mod coordinator_runtime;
mod empty_session_cleanup_runtime;
mod foreground;
mod http_dispatch;
pub mod http_endpoints;
pub mod http_infra;
pub(crate) mod managed_tools_http;
mod open_conversation_http;
pub mod presentation_delta;
mod previous_session_restore_http;
mod project_docs_http;
pub mod project_paths;
mod route_http;
mod session_auto_sleep_sweep;
pub mod session_state_sync;
mod shutdown_bounds;
mod sidebar_spaces_switch;
mod stale_server_takeover;
mod startup_gate;
pub mod telemetry_http;
pub mod telemetry_tasks;
#[cfg(test)]
mod tests;
pub mod title_generation;
pub(crate) mod title_job_recovery;
pub mod typed_operation_http;
mod work_mode_sync;
mod session_links_sync;
mod workspaces_switch;
pub mod worktree_ops;
pub mod ws;
pub mod zmx_http;

pub(crate) use agent_http::*;
pub(crate) use agent_prompt_search_http::*;
pub use app_state::*;
pub(crate) use background_tasks::*;
pub(crate) use commit_message_generation::*;
pub(crate) use coordinator_runtime::queue_session_chat_prompt;
pub use foreground::*;
use http_dispatch::*;
pub(crate) use http_endpoints::*;
pub(crate) use http_infra::*;
pub(crate) use presentation_delta::*;
pub(crate) use project_paths::*;
use route_http::*;
pub(crate) use session_state_sync::*;
use stale_server_takeover::*;
use startup_gate::*;
pub(crate) use telemetry_http::*;
pub(crate) use telemetry_tasks::*;
pub(crate) use title_generation::*;
pub(crate) use typed_operation_http::*;
pub(crate) use worktree_ops::*;
pub(crate) use ws::*;
pub(crate) use zmx_http::*;
