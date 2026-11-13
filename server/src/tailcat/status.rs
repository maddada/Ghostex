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
    change_tailcat_state(paths, db, Some(runtime), |state| {
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
                    state.relay.custom_host.clone()
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
        Ok(())
    })
    .map(|(record, ())| record)
}

/// Changes the allow-list from the list as it stands when the change runs (never from a copy read
/// earlier, which would bring back a key taken off meanwhile). `change` gets the current list and
/// returns the next one. Returns whether the list changed.
pub fn change_tailcat_client_keys(
    paths: &GxserverPaths,
    db: &Connection,
    runtime: &TailcatRuntime,
    change: impl FnOnce(&[String]) -> Result<Vec<String>>,
) -> Result<bool> {
    change_tailcat_state(paths, db, Some(runtime), |state| {
        let next = normalize_tailcat_client_keys(change(&state.allowed_client_keys)?);
        let changed = next != state.allowed_client_keys;
        state.allowed_client_keys = next;
        Ok(changed)
    })
    .map(|(_, changed)| changed)
}

/// Every tailcat state write in this process, so each one reads, checks, writes and reconciles the
/// helpers as one step, in the order the writes land.
static TAILCAT_STATE_WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/*
CDXC:RemotePairing 2026-11-13 WHY:
The off check once read `enabled` from one read and the write saved the whole state, so an add that read "on" just before the user turned Easy Connect off wrote "on" back with its key. Every write now reads, checks and saves inside one IMMEDIATE transaction under one lock, and the helpers are reconciled before the next write starts, so the user's Off cannot be undone and no key lands after it.
*/
fn change_tailcat_state<T>(
    paths: &GxserverPaths,
    db: &Connection,
    runtime: Option<&TailcatRuntime>,
    change: impl FnOnce(&mut TailcatState) -> Result<T>,
) -> Result<(TailcatStateRecord, T)> {
    let _writes = TAILCAT_STATE_WRITES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // A caller already inside a transaction keeps it; otherwise this write takes SQLite's lock.
    let transaction = if db.is_autocommit() {
        Some(rusqlite::Transaction::new_unchecked(
            db,
            rusqlite::TransactionBehavior::Immediate,
        )?)
    } else {
        None
    };
    let repository = TailcatRepository::new(db);
    let before = read_tailcat_state_or_default(&repository)?;
    let mut state = before.clone();
    let out = change(&mut state)?;
    if !state.enabled && adds_client_key(&before.allowed_client_keys, &state.allowed_client_keys) {
        anyhow::bail!(EASY_CONNECT_OFF_REFUSAL);
    }
    let record = repository.upsert_state(state)?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    if let Some(runtime) = runtime {
        runtime.apply(paths, &record.state);
    }
    Ok((record, out))
}

/// Runs `change` against the saved state with the same lock and transaction as every other write,
/// so other rows written through `db` inside it (a device's key, say) are saved together with the
/// allow-list or not at all. The off rule holds as for any write. With `runtime`, the helpers are
/// reconciled right after; without it, the caller does so later with [`reconcile_tailcat_runtime`].
pub fn with_tailcat_state<T>(
    paths: &GxserverPaths,
    db: &Connection,
    runtime: Option<&TailcatRuntime>,
    change: impl FnOnce(&mut TailcatState) -> Result<T>,
) -> Result<T> {
    change_tailcat_state(paths, db, runtime, change).map(|(_, out)| out)
}

/// Brings the helpers in line with the state as saved now, in turn with the writes.
pub fn reconcile_tailcat_runtime(
    paths: &GxserverPaths,
    db: &Connection,
    runtime: &TailcatRuntime,
) -> Result<()> {
    let _writes = TAILCAT_STATE_WRITES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state = read_tailcat_state_or_default(&TailcatRepository::new(db))?;
    runtime.apply(paths, &state);
    Ok(())
}

/*
CDXC:RemotePairing 2026-11-13 DECISION:
User (Sebastian, after 11.6.0 put his SSH-only iPhone on a computer's Easy Connect allow-list with no click): "Remove ALL. Do not auto force Easy Connect ever. ... We cannot auto promote this from the phone without the users approval ... Just use whatever the user asked to use", and as a hard rule: while Easy Connect is off on a computer, no key can be added to its allow-list by any path (pairing, a phone's request, the API, the CLI); it is refused. Every tailcat state write goes through `change_tailcat_state`, so the refusal lives there; taking keys off always works.
SEE-ALSO: remote_access/pair_device.rs (`finish_pairing`, `/api/pairOverSsh` refuse before anything is written), apps/mobile/app/src/machines/sshPairing.ts.
*/
/// The refusal for a key added while Easy Connect is off.
pub const EASY_CONNECT_OFF_REFUSAL: &str =
    "Easy Connect is off on this computer, so no device can be added to it. Turn it on in Ghostex Settings › Remote first.";

/// Whether `next` holds a key `saved` does not (the `none` placeholder is no key).
fn adds_client_key(saved: &[String], next: &[String]) -> bool {
    next.iter()
        .any(|key| key != TAILCAT_ALLOW_NONE && !saved.iter().any(|known| known == key))
}

/// Whether the user turned Easy Connect on here (the saved switch, not whether the helper runs).
pub fn easy_connect_turned_on(db: &Connection) -> Result<bool> {
    Ok(read_tailcat_state_or_default(&TailcatRepository::new(db))?.enabled)
}

/// How long one `/api/remotePairingCode` ask keeps Easy Connect open to new phones.
pub const TAILCAT_PAIRING_WINDOW_HOLD: std::time::Duration = std::time::Duration::from_secs(60);

/// Holds the pairing window open while a pairing code is on screen or a device waits as Pending:
/// the door runs open and a new device's knock is answered (CDXC:RemotePairing 2026-11-10 in
/// `door.rs`). The helpers paired devices are on are not touched.
pub fn hold_tailcat_pairing_window_open(paths: &GxserverPaths, runtime: &TailcatRuntime) {
    // Every ask after the first only moves the deadline.
    if runtime.extend_open_pairing_window(TAILCAT_PAIRING_WINDOW_HOLD) {
        return;
    }
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
}
