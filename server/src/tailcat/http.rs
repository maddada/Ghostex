use std::time::Instant;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
    storage::open_gxserver_database,
};

use super::status::*;
use super::types::*;

pub(crate) async fn handle_tailcat_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return tailcat_error_response(endpoint_path, request_id, error.code, error.message);
        }
    };
    let paths = state.paths.clone();
    let runtime = state.tailcat_runtime.clone();
    let logger = state.logger.clone();

    if endpoint_path == "/api/installTailcat" {
        super::install::start_installation(paths.clone(), runtime.clone());
    }

    if endpoint_path == "/api/tailcatStatus" || endpoint_path == "/api/installTailcat" {
        let status = tokio::task::spawn_blocking(move || {
            read_tailcat_status_payload_for_paths(&paths, &runtime)
        })
        .await;
        return match status {
            Ok(status) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "status": status })),
            ),
            Err(error) => tailcat_error_response(
                endpoint_path,
                request_id,
                "internalError",
                format!("tailcat status read failed: {error}"),
            ),
        };
    }

    let update = match serde_json::from_value::<TailcatStateUpdate>(Value::Object(params)) {
        Ok(update) => update,
        Err(error) => {
            return tailcat_error_response(
                endpoint_path,
                request_id,
                "badRequest",
                format!("Invalid tailcat state update payload: {error}"),
            );
        }
    };

    if let TailcatStateUpdate::SetRelay {
        mode: TailcatRelayMode::Custom,
        custom_host,
    } = &update
    {
        // An empty field picks the hostname saved earlier (checked against it on save).
        if !custom_host.trim().is_empty() && normalize_tailcat_relay_host(custom_host).is_none() {
            return tailcat_error_response(
                endpoint_path,
                request_id,
                "badRequest",
                "Enter a relay hostname such as relay.example.com.",
            );
        }
    }

    let started_at = Instant::now();
    let outcome = tokio::task::spawn_blocking({
        let update = update.clone();
        move || {
            let db = open_gxserver_database(&paths).map_err(|error| {
                (
                    TailcatLogErrorCode::StateUpdateDatabaseUnavailable,
                    format!("SQLite gxserver state error: {error}"),
                    false,
                )
            })?;
            let record =
                apply_tailcat_state_update(&paths, &db, &runtime, update).map_err(|error| {
                    // A refusal is the caller's to fix: said as it is, answered as a bad request.
                    if let Some(refusal) = error.downcast_ref::<TailcatRefusal>() {
                        return (
                            TailcatLogErrorCode::StateUpdateFailed,
                            refusal.0.clone(),
                            true,
                        );
                    }
                    let refused = error.to_string().contains(super::EASY_CONNECT_OFF_REFUSAL);
                    (
                        TailcatLogErrorCode::StateUpdateFailed,
                        format!("tailcat state update failed: {error}"),
                        refused,
                    )
                })?;
            Ok((record, read_tailcat_status_payload(&paths, &db, &runtime)))
        }
    })
    .await;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => Err((
            TailcatLogErrorCode::StateUpdateFailed,
            format!("tailcat state update task failed: {error}"),
            false,
        )),
    };

    let devices_let_in = match (&update, &outcome) {
        (TailcatStateUpdate::EnableForBoxDevices {}, Ok((record, _))) => {
            Some(record.state.allowed_client_keys.len())
        }
        _ => None,
    };
    crate::audit::record(
        easy_connect_audit_action(&update, devices_let_in),
        crate::audit::Actor::current(),
        match &outcome {
            Ok(_) => crate::audit::Outcome::Done,
            Err((_, message, _)) => crate::audit::Outcome::Failed(message.clone()),
        },
        None,
    );
    match outcome {
        Ok((record, mut status)) => {
            /*
            CDXC:RemotePairing 2026-11-11 FIX:
            GitHub #559: Easy Connect was turned on with no helper on the computer and nothing ever installed one, so it stayed on, stopped, with "Reinstall Ghostex" under it. Turning it on now starts the helper's install when it is missing; Easy Connect starts on the helper the moment it is in place, and the pages show the install's steps from the status below.
            */
            if matches!(
                update,
                TailcatStateUpdate::SetEnabled { enabled: true }
                    | TailcatStateUpdate::EnableForBoxDevices {}
            ) && !status.binary_found
            {
                super::install::start_installation(
                    state.paths.clone(),
                    state.tailcat_runtime.clone(),
                );
                let installation = super::install::installation_status();
                status.installing = installation.running;
                status.install_progress = installation.progress;
                status.install_error = installation.error;
            }
            log_tailcat_state_update_success(
                &logger,
                &update,
                &record,
                started_at.elapsed().as_millis(),
            );
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "status": status })),
            )
        }
        Err((error_code, message, refused)) => {
            log_tailcat_state_update_failure(
                &logger,
                &update,
                error_code,
                started_at.elapsed().as_millis(),
            );
            // A refusal (a device added while Easy Connect is off, a box update with no box ticket
            // naming devices) is the caller's to fix, not a fault here.
            let code = if refused {
                "badRequest"
            } else {
                "internalError"
            };
            tailcat_error_response(endpoint_path, request_id, code, message)
        }
    }
}

/// The security log's line for an Easy Connect change; `devices_let_in` counts the devices a
/// Cloud Box's boot let in.
fn easy_connect_audit_action(
    update: &TailcatStateUpdate,
    devices_let_in: Option<usize>,
) -> crate::audit::Action {
    use crate::audit::Action;
    match update {
        TailcatStateUpdate::SetEnabled { enabled: true } => {
            Action::new("easyConnect.enable", "Turned on Easy Connect")
        }
        TailcatStateUpdate::SetEnabled { enabled: false } => {
            Action::new("easyConnect.disable", "Turned off Easy Connect")
        }
        TailcatStateUpdate::EnableForBoxDevices {} => Action::new(
            "easyConnect.enable",
            match devices_let_in {
                Some(1) => {
                    "Turned on Easy Connect for this Cloud Box and let 1 device connect".to_string()
                }
                Some(count) => format!(
                    "Turned on Easy Connect for this Cloud Box and let {count} devices connect"
                ),
                None => "Turn on Easy Connect for this Cloud Box's devices".to_string(),
            },
        ),
        TailcatStateUpdate::SetPorts { ports } => Action::new(
            "easyConnect.ports",
            format!(
                "Changed the ports Easy Connect opens to {}",
                ports
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        TailcatStateUpdate::SetAllowedClientKeys {
            allowed_client_keys,
        } => Action::new(
            "easyConnect.allowList",
            format!(
                "Changed who may connect through Easy Connect ({} allowed)",
                allowed_client_keys.len()
            ),
        ),
        TailcatStateUpdate::SetRelay { mode, custom_host } => Action::new(
            "easyConnect.relay",
            match mode {
                TailcatRelayMode::Custom => {
                    format!("Easy Connect now uses the relay {}", custom_host.trim())
                }
                TailcatRelayMode::Ghostex => "Easy Connect now uses Ghostex's relay".to_string(),
                TailcatRelayMode::Public => {
                    "Easy Connect now uses only Tailscale's public relays".to_string()
                }
            },
        ),
    }
}

fn tailcat_error_response(
    endpoint_path: String,
    request_id: String,
    code: &'static str,
    message: impl Into<String>,
) -> RoutedResponse {
    let status = if code == "badRequest" {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(code, message.into(), Some(request_id)),
    )
}
