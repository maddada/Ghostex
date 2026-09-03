use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::paths::GxserverPaths;
use crate::storage::open_gxserver_database;

use super::binary::*;
use super::repository::*;
use super::supervisor::*;
use super::types::*;

pub fn read_tailcat_status_payload(
    db: &Connection,
    runtime: &TailcatRuntime,
) -> TailcatStatusPayload {
    let state = TailcatRepository::new(db)
        .read_state()
        .ok()
        .flatten()
        .map(|record| record.state)
        .unwrap_or_else(default_tailcat_state);
    tailcat_status_payload_from_state(state, runtime)
}

pub fn read_tailcat_status_payload_for_paths(
    paths: &GxserverPaths,
    runtime: &TailcatRuntime,
) -> TailcatStatusPayload {
    match open_gxserver_database(paths) {
        Ok(db) => read_tailcat_status_payload(&db, runtime),
        Err(_) => tailcat_status_payload_from_state(default_tailcat_state(), runtime),
    }
}

fn tailcat_status_payload_from_state(
    state: TailcatState,
    runtime: &TailcatRuntime,
) -> TailcatStatusPayload {
    let binary_path = resolve_tailcat_binary();
    let binary_version = binary_path
        .as_ref()
        .and_then(|path| runtime.binary_version(path));
    let snapshot = runtime.snapshot();
    TailcatStatusPayload {
        enabled: state.enabled,
        running: snapshot.running,
        binary_found: binary_path.is_some(),
        binary_path: binary_path.map(|path| path.to_string_lossy().to_string()),
        binary_version,
        token: snapshot.token,
        ports: state.ports,
        allowed_client_keys: state.allowed_client_keys,
        last_error: snapshot.last_error,
    }
}

/*
CDXC:RemotePairing 2026-09-01:
Every mutation persists first and then reconciles the sidecar from the stored
state, so the running child is always explained by a row a later status read
can see. Ports and keys are normalized on the way in, which is also what makes
"no real change" cheap enough to skip a restart.
*/
pub fn apply_tailcat_state_update(
    paths: &GxserverPaths,
    db: &Connection,
    runtime: &TailcatRuntime,
    update: TailcatStateUpdate,
) -> Result<TailcatStateRecord> {
    let repository = TailcatRepository::new(db);
    let mut state = read_tailcat_state_or_default(&repository)?;
    match update {
        TailcatStateUpdate::SetEnabled { enabled } => state.enabled = enabled,
        TailcatStateUpdate::SetPorts { ports } => state.ports = normalize_tailcat_ports(ports),
        TailcatStateUpdate::SetAllowedClientKeys {
            allowed_client_keys,
        } => state.allowed_client_keys = normalize_tailcat_client_keys(allowed_client_keys),
    }
    let record = repository.upsert_state(state)?;
    runtime.apply(paths, &record.state);
    Ok(record)
}

/// Boot reconcile: restart the sidecar for a state that was left enabled.
pub fn start_tailcat_from_persisted_state(paths: &GxserverPaths, runtime: &TailcatRuntime) {
    let Ok(db) = open_gxserver_database(paths) else {
        return;
    };
    let Ok(state) = read_tailcat_state_or_default(&TailcatRepository::new(&db)) else {
        return;
    };
    if !state.enabled {
        return;
    }
    runtime.apply(paths, &state);
}

pub fn log_tailcat_state_update_success(
    logger: &crate::logging::GxserverLogger,
    update: &TailcatStateUpdate,
    record: &TailcatStateRecord,
    duration_ms: u128,
) {
    let _ = logger.log_routine(
        crate::logging::DiagnosticLogScenario::ServerLifecycle,
        crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Info,
            event: "tailcat.stateUpdate".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: Some(duration_ms),
            error: None,
            details: Some(json!({
                "allowedClientKeyCount": record.state.allowed_client_keys.len(),
                "enabled": record.state.enabled,
                "portCount": record.state.ports.len(),
                "updateKind": update.kind(),
            })),
        },
    );
}

pub fn log_tailcat_state_update_failure(
    logger: &crate::logging::GxserverLogger,
    update: &TailcatStateUpdate,
    error_code: TailcatLogErrorCode,
    duration_ms: u128,
) {
    let details: Value = json!({
        "errorCode": error_code.as_str(),
        "updateKind": update.kind(),
    });
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "tailcat.stateUpdateFailed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: Some(duration_ms),
        error: Some(error_code.as_str().to_string()),
        details: Some(details),
    });
}
