use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::paths::GxserverPaths;
use crate::storage::open_gxserver_database;

use super::admin::*;
use super::repository::*;
use super::sync::*;
use super::types::*;

pub fn read_portless_status_payload_for_paths(paths: &GxserverPaths) -> PortlessStatusPayload {
    match open_gxserver_database(paths) {
        Ok(db) => read_portless_status_payload(&db),
        Err(_) => unavailable_portless_status_payload(),
    }
}

pub fn read_portless_status_payload(db: &Connection) -> PortlessStatusPayload {
    match PortlessRepository::new(db).read_state() {
        Ok(record) => {
            let source_status = if record.is_some() {
                PortlessPayloadSourceStatus::Current
            } else {
                PortlessPayloadSourceStatus::Missing
            };
            portless_status_payload_from_record(record, source_status)
        }
        Err(_) => unavailable_portless_status_payload(),
    }
}

pub fn unavailable_portless_status_payload() -> PortlessStatusPayload {
    portless_status_payload_from_record(None, PortlessPayloadSourceStatus::Unavailable)
}

pub fn apply_portless_state_update(
    paths: &GxserverPaths,
    db: &Connection,
    update: PortlessStateUpdate,
) -> Result<PortlessStateRecord> {
    let repository = PortlessRepository::new(db);
    match update {
        PortlessStateUpdate::SetEnabled { enabled } => {
            apply_portless_enabled_update(paths, &repository, enabled)
        }
        PortlessStateUpdate::SetProtocol { protocol } => {
            apply_portless_protocol_update(&repository, protocol)
        }
        PortlessStateUpdate::RecordAdminResult {
            action,
            ok,
            protocol,
        } => apply_portless_admin_result_update(paths, &repository, action, ok, protocol),
    }
}

fn apply_portless_enabled_update(
    paths: &GxserverPaths,
    repository: &PortlessRepository<'_>,
    enabled: bool,
) -> Result<PortlessStateRecord> {
    let mut state = read_portless_state_or_default(repository)?;
    state.enabled = enabled;
    if enabled {
        if state.setup_status == PortlessSetupStatus::Disabled {
            state.setup_status = match state.setup_ownership {
                PortlessSetupOwnership::Unknown => PortlessSetupStatus::Unknown,
                PortlessSetupOwnership::Missing
                | PortlessSetupOwnership::Ghostex
                | PortlessSetupOwnership::Standalone => PortlessSetupStatus::Needed,
            };
            state.runtime_status = PortlessRuntimeStatus::Unknown;
        }
        return repository.upsert_state(state);
    }

    state.setup_status = PortlessSetupStatus::Disabled;
    state.runtime_status = PortlessRuntimeStatus::Inactive;
    let record = repository.upsert_state(state)?;
    sync_portless_routes(paths, &[])?;
    Ok(record)
}

fn apply_portless_protocol_update(
    repository: &PortlessRepository<'_>,
    protocol: PortlessProtocol,
) -> Result<PortlessStateRecord> {
    let mut state = read_portless_state_or_default(repository)?;
    state.protocol = protocol;
    if state.enabled
        && state.setup_status != PortlessSetupStatus::Disabled
        && is_portless_installed_setup_ownership(state.setup_ownership)
    {
        state.setup_status = PortlessSetupStatus::Needed;
        state.runtime_status = PortlessRuntimeStatus::Inactive;
    }
    repository.upsert_state(state)
}

fn apply_portless_admin_result_update(
    paths: &GxserverPaths,
    repository: &PortlessRepository<'_>,
    action: PortlessAdminResultAction,
    ok: bool,
    protocol: Option<PortlessProtocol>,
) -> Result<PortlessStateRecord> {
    let mut state = read_portless_state_or_default(repository)?;
    if let Some(protocol) = protocol {
        state.protocol = protocol;
    }

    match (action, ok) {
        (PortlessAdminResultAction::Remove, true) => {
            state.setup_ownership = PortlessSetupOwnership::Missing;
            state.setup_status = if state.enabled {
                PortlessSetupStatus::Needed
            } else {
                PortlessSetupStatus::Disabled
            };
            state.runtime_status = PortlessRuntimeStatus::Inactive;
            let record = repository.upsert_state(state)?;
            sync_portless_routes(paths, &[])?;
            Ok(record)
        }
        (PortlessAdminResultAction::Remove, false) => repository.upsert_state(state),
        (_, true) => {
            state.enabled = true;
            state.setup_ownership = PortlessSetupOwnership::Ghostex;
            state.setup_status = PortlessSetupStatus::Active;
            state.runtime_status = PortlessRuntimeStatus::Active;
            repository.upsert_state(state)
        }
        (_, false) => {
            state.enabled = true;
            state.setup_ownership = PortlessSetupOwnership::Ghostex;
            state.setup_status = PortlessSetupStatus::Failed;
            state.runtime_status = PortlessRuntimeStatus::Failed;
            repository.upsert_state(state)
        }
    }
}

pub fn log_portless_background_sync_outcome(
    logger: &crate::logging::GxserverLogger,
    outcome: &PortlessBackgroundSyncOutcome,
    duration_ms: u128,
) {
    let _ = logger.log_routine(
        crate::logging::DiagnosticLogScenario::Portless,
        crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Info,
            event: "portless.backgroundSync".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: Some(duration_ms),
            error: None,
            details: Some(json!({
                "action": outcome.action.as_str(),
                "desiredRouteCount": outcome.desired_route_count,
                "liveListenerCount": outcome.live_listener_count,
                "routeCount": outcome.desired_route_count,
                "status": outcome.status.as_str(),
            })),
        },
    );
}

pub fn log_portless_background_sync_failure(
    logger: &crate::logging::GxserverLogger,
    error_code: PortlessLogErrorCode,
    duration_ms: u128,
) {
    log_portless_failure(
        logger,
        "portless.backgroundSyncFailed",
        error_code,
        duration_ms,
        None,
    );
}

pub fn log_portless_state_update_success(
    logger: &crate::logging::GxserverLogger,
    update: &PortlessStateUpdate,
    record: &PortlessStateRecord,
    duration_ms: u128,
) {
    let mut details = portless_state_update_log_details(update);
    if let Some(object) = details.as_object_mut() {
        object.insert("enabled".to_string(), json!(record.state.enabled));
        object.insert(
            "protocol".to_string(),
            json!(record.state.protocol.as_str()),
        );
        object.insert(
            "runtimeStatus".to_string(),
            json!(record.state.runtime_status.as_str()),
        );
        object.insert(
            "setupOwnership".to_string(),
            json!(record.state.setup_ownership.as_str()),
        );
        object.insert(
            "setupStatus".to_string(),
            json!(record.state.setup_status.as_str()),
        );
    }
    let _ = logger.log_routine(
        crate::logging::DiagnosticLogScenario::Portless,
        crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Info,
            event: "portless.stateUpdate".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: Some(duration_ms),
            error: None,
            details: Some(details),
        },
    );
}

pub fn log_portless_state_update_failure(
    logger: &crate::logging::GxserverLogger,
    update: &PortlessStateUpdate,
    error_code: PortlessLogErrorCode,
    duration_ms: u128,
) {
    log_portless_failure(
        logger,
        "portless.stateUpdateFailed",
        error_code,
        duration_ms,
        Some(portless_state_update_log_details(update)),
    );
}

fn log_portless_failure(
    logger: &crate::logging::GxserverLogger,
    event: &str,
    error_code: PortlessLogErrorCode,
    duration_ms: u128,
    details: Option<Value>,
) {
    let mut details = details.unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.insert("errorCode".to_string(), json!(error_code.as_str()));
    }
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: Some(duration_ms),
        error: Some(error_code.as_str().to_string()),
        details: Some(details),
    });
}

fn portless_state_update_log_details(update: &PortlessStateUpdate) -> Value {
    match update {
        PortlessStateUpdate::SetEnabled { enabled } => {
            json!({
                "enabled": *enabled,
                "updateKind": "setEnabled",
            })
        }
        PortlessStateUpdate::SetProtocol { protocol } => {
            json!({
                "protocol": protocol.as_str(),
                "updateKind": "setProtocol",
            })
        }
        PortlessStateUpdate::RecordAdminResult {
            action,
            ok,
            protocol,
        } => {
            let mut details = json!({
                "adminAction": action.as_str(),
                "ok": *ok,
                "protocolPresent": protocol.is_some(),
                "updateKind": "recordAdminResult",
            });
            if let (Some(protocol), Some(object)) = (protocol, details.as_object_mut()) {
                object.insert("protocol".to_string(), json!(protocol.as_str()));
            }
            details
        }
    }
}

pub fn read_portless_presentation_payload(db: &Connection) -> PortlessPresentationPayload {
    let status = read_portless_status_payload(db);
    let assigned_domains = read_portless_assigned_domains(db).unwrap_or_default();
    if !status.enabled || status.setup_status == PortlessSetupStatus::Disabled {
        return PortlessPresentationPayload {
            assigned_domains,
            live_listener_count: 0,
            route_preview_status: PortlessRoutePreviewStatus::Disabled,
            route_previews: Vec::new(),
            status,
        };
    }

    match compute_live_portless_owned_listeners(db).and_then(|listeners| {
        let routes = compute_desired_portless_routes(db, &listeners)?;
        Ok((listeners, routes))
    }) {
        Ok((listeners, routes)) => {
            let route_previews =
                portless_route_previews_for_desired_routes(status.protocol, &listeners, &routes);
            PortlessPresentationPayload {
                assigned_domains,
                live_listener_count: listeners.len(),
                route_preview_status: PortlessRoutePreviewStatus::Current,
                route_previews,
                status,
            }
        }
        Err(_) => PortlessPresentationPayload {
            assigned_domains,
            live_listener_count: 0,
            route_preview_status: PortlessRoutePreviewStatus::Unavailable,
            route_previews: Vec::new(),
            status,
        },
    }
}

fn read_portless_assigned_domains(db: &Connection) -> Result<Vec<PortlessAssignedDomain>> {
    /*
    CDXC:PortlessSettings 2026-06-23-04:02:
    Settings -> Projects must show assigned project/worktree domains even when
    no dev server is currently listening. Derive hostnames from persisted
    Portless slugs and expose only stable ids plus hostnames, never paths,
    names, full URLs, command text, process output, or environment values.
    */
    let identities = PortlessRepository::new(db).backfill_domain_identities()?;
    let mut domains = Vec::new();
    for project in identities.projects {
        domains.push(PortlessAssignedDomain {
            hostname: format!("{}.localhost", project.slug),
            kind: PortlessAssignedDomainKind::Project,
            parent_project_id: None,
            project_id: project.project_id,
        });
    }
    for worktree in identities.worktrees {
        domains.push(PortlessAssignedDomain {
            hostname: format!(
                "{}.{}.localhost",
                worktree.project_slug, worktree.worktree_slug
            ),
            kind: PortlessAssignedDomainKind::Worktree,
            parent_project_id: Some(worktree.parent_project_id),
            project_id: worktree.worktree_project_id,
        });
    }
    domains.sort_by(|a, b| {
        a.project_id
            .cmp(&b.project_id)
            .then_with(|| a.hostname.cmp(&b.hostname))
    });
    Ok(domains)
}

pub(crate) fn portless_status_payload_from_record(
    record: Option<PortlessStateRecord>,
    source_status: PortlessPayloadSourceStatus,
) -> PortlessStatusPayload {
    let (state, updated_at) = match record {
        Some(record) => (record.state, Some(record.updated_at)),
        None => (default_portless_state(), None),
    };
    PortlessStatusPayload {
        actions: portless_admin_action_set(&state),
        enabled: state.enabled,
        protocol: state.protocol,
        runtime_status: state.runtime_status,
        setup_ownership: state.setup_ownership,
        setup_status: state.setup_status,
        source_status,
        updated_at,
    }
}

fn read_portless_state_or_default(repository: &PortlessRepository<'_>) -> Result<PortlessState> {
    Ok(repository
        .read_state()?
        .map(|record| record.state)
        .unwrap_or_else(default_portless_state))
}

fn default_portless_state() -> PortlessState {
    PortlessState {
        // CDXC:PortlessSettingsDisabled 2026-07-25: Portless remains
        // implemented for later use, but gxserver must not create routes before
        // an app explicitly re-enables the currently hidden integration.
        enabled: false,
        protocol: PortlessProtocol::Https,
        setup_ownership: PortlessSetupOwnership::Unknown,
        setup_status: PortlessSetupStatus::Disabled,
        runtime_status: PortlessRuntimeStatus::Inactive,
    }
}
