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
    paths: &GxserverPaths,
    db: &Connection,
    runtime: &TailcatRuntime,
) -> TailcatStatusPayload {
    let state = TailcatRepository::new(db)
        .read_state()
        .ok()
        .flatten()
        .map(|record| record.state)
        .unwrap_or_else(default_tailcat_state);
    tailcat_status_payload_from_state(paths, state, runtime)
}

pub fn read_tailcat_status_payload_for_paths(
    paths: &GxserverPaths,
    runtime: &TailcatRuntime,
) -> TailcatStatusPayload {
    match open_gxserver_database(paths) {
        Ok(db) => read_tailcat_status_payload(paths, &db, runtime),
        Err(_) => tailcat_status_payload_from_state(paths, default_tailcat_state(), runtime),
    }
}

fn tailcat_status_payload_from_state(
    paths: &GxserverPaths,
    state: TailcatState,
    runtime: &TailcatRuntime,
) -> TailcatStatusPayload {
    let binary_path = resolve_host_tailcat_binary(paths);
    let binary_version = binary_path
        .as_ref()
        .and_then(|path| runtime.binary_version(path));
    let snapshot = runtime.snapshot();
    let installation = super::install::installation_status();
    TailcatStatusPayload {
        installing: installation.running,
        install_progress: installation.progress,
        install_error: installation.error,
        enabled: state.enabled,
        running: snapshot.running,
        binary_found: binary_path.is_some(),
        binary_path: binary_path.map(|path| path.to_string_lossy().to_string()),
        binary_version,
        token: snapshot.token,
        ports: tailcat_served_ports(&state),
        allowed_client_keys: state.allowed_client_keys,
        last_error: snapshot.last_error,
        relay: TailcatRelayStatus {
            mode: state.relay.mode,
            custom_host: state.relay.custom_host,
            host: snapshot.relay_host,
            running: snapshot.relay.running,
            token: snapshot.relay.token,
            reachable: snapshot.relay_reachable,
            last_error: snapshot.relay.last_error,
        },
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
        TailcatStateUpdate::SetPorts { ports } => {
            if let Some(port) = ports.iter().find(|port| !tailcat_port_allowed(**port)) {
                anyhow::bail!(
                    "Easy Connect serves only SSH (port {}) and Ghostex (port {}); port {port} can't be added.",
                    crate::remote_access::ssh_status::DEFAULT_SSH_PORT,
                    tailcat_gxserver_api_port()
                );
            }
            state.ports = normalize_tailcat_ports(ports)
        }
        TailcatStateUpdate::SetAllowedClientKeys {
            allowed_client_keys,
        } => state.allowed_client_keys = normalize_tailcat_client_keys(allowed_client_keys),
        TailcatStateUpdate::SetRelay { mode, custom_host } => {
            let custom_host = custom_host.trim().to_string();
            // An empty field keeps the last hostname, so the switch back to Custom restores it.
            let custom_host = if custom_host.is_empty() {
                state.relay.custom_host
            } else {
                custom_host
            };
            if mode == TailcatRelayMode::Custom
                && normalize_tailcat_relay_host(&custom_host).is_none()
            {
                anyhow::bail!("Enter a relay hostname such as relay.example.com.");
            }
            state.relay = TailcatRelaySetting { mode, custom_host };
        }
    }
    let record = repository.upsert_state(state)?;
    runtime.apply(paths, &record.state);
    Ok(record)
}

/// How long one `/api/remotePairingCode` ask keeps Easy Connect open to new phones.
pub const TAILCAT_PAIRING_WINDOW_HOLD: std::time::Duration = std::time::Duration::from_secs(60);

/// Lifts the allow-list while a pairing code is on screen (CDXC:RemotePairing 2026-10-25 in
/// `supervisor.rs`); does nothing while the list is empty.
pub fn hold_tailcat_pairing_window_open(paths: &GxserverPaths, runtime: &TailcatRuntime) {
    let Ok(db) = open_gxserver_database(paths) else {
        return;
    };
    let Ok(state) = read_tailcat_state_or_default(&TailcatRepository::new(&db)) else {
        return;
    };
    runtime.hold_pairing_window_open(paths, &state, TAILCAT_PAIRING_WINDOW_HOLD);
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
                "relayMode": record.state.relay.mode.as_str(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (
        tempfile::TempDir,
        GxserverPaths,
        crate::storage::GxserverDatabase,
    ) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage");
        let db = open_gxserver_database(&paths).expect("database");
        (temp, paths, db)
    }

    #[test]
    fn the_relay_choice_is_stored_and_a_bad_custom_host_is_refused() {
        let (_temp, paths, db) = temp_db();
        let runtime = TailcatRuntime::new();
        let stored = TailcatRepository::new(&db)
            .read_state()
            .expect("read")
            .map(|record| record.state.relay);
        assert_eq!(stored, None);

        let record = apply_tailcat_state_update(
            &paths,
            &db,
            &runtime,
            TailcatStateUpdate::SetRelay {
                mode: TailcatRelayMode::Custom,
                custom_host: "Relay.Example.com".to_string(),
            },
        )
        .expect("custom relay");
        assert_eq!(record.state.relay.mode, TailcatRelayMode::Custom);
        assert_eq!(
            record.state.relay.relay_host().as_deref(),
            Some("relay.example.com")
        );

        // Public only keeps the hostname for the way back.
        let record = apply_tailcat_state_update(
            &paths,
            &db,
            &runtime,
            TailcatStateUpdate::SetRelay {
                mode: TailcatRelayMode::Public,
                custom_host: String::new(),
            },
        )
        .expect("public only");
        assert_eq!(record.state.relay.mode, TailcatRelayMode::Public);
        assert_eq!(record.state.relay.custom_host, "Relay.Example.com");
        let reread = TailcatRepository::new(&db)
            .read_state()
            .expect("read")
            .expect("row");
        assert_eq!(reread.state.relay, record.state.relay);

        let restored = apply_tailcat_state_update(
            &paths,
            &db,
            &runtime,
            TailcatStateUpdate::SetRelay {
                mode: TailcatRelayMode::Custom,
                custom_host: String::new(),
            },
        )
        .expect("custom again with the saved hostname");
        assert_eq!(
            restored.state.relay.relay_host().as_deref(),
            Some("relay.example.com")
        );

        let refused = apply_tailcat_state_update(
            &paths,
            &db,
            &runtime,
            TailcatStateUpdate::SetRelay {
                mode: TailcatRelayMode::Custom,
                custom_host: "not a host".to_string(),
            },
        );
        assert!(refused.is_err());
        // Easy Connect is off, so no helper started.
        let payload = read_tailcat_status_payload(&paths, &db, &runtime);
        assert!(!payload.relay.running);
        assert_eq!(payload.relay.mode, TailcatRelayMode::Custom);
    }

    #[test]
    fn easy_connect_serves_only_ssh_and_gxserver() {
        let (_temp, paths, db) = temp_db();
        let runtime = TailcatRuntime::new();
        let api = tailcat_gxserver_api_port();
        let refused = apply_tailcat_state_update(
            &paths,
            &db,
            &runtime,
            TailcatStateUpdate::SetPorts {
                ports: vec![22, 5900],
            },
        );
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("port 5900 can't be added"));
        let record = apply_tailcat_state_update(
            &paths,
            &db,
            &runtime,
            TailcatStateUpdate::SetPorts { ports: vec![22] },
        )
        .expect("ssh only");
        assert_eq!(tailcat_served_ports(&record.state), {
            let mut both = vec![22, api];
            both.sort_unstable();
            both
        });
        // A list saved before the limit is served without the other ports.
        let mut old = record.state.clone();
        old.ports = vec![22, 3000, 5900];
        assert!(!tailcat_served_ports(&old).contains(&3000));
        assert!(!tailcat_served_ports(&old).contains(&5900));
    }
}
