use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
    storage::open_gxserver_database,
    tailcat::{
        hold_tailcat_pairing_window_open, read_tailcat_status_payload_for_paths,
        tailcat_gxserver_api_port,
    },
};

use super::easy_connect_addresses::*;
use super::pair_device::*;
use super::paired_devices::*;
use super::pairing_code::*;
use super::probe_cache::*;
use super::ssh_enable::*;
use super::ssh_status::*;

/*
CDXC:RemotePairing 2026-09-03:
Every handler does its OS probing on the blocking pool: the SSH probe, the
(cached, see `probe_cache.rs`) `tailscale status` and `scutil` calls, and the
privileged enable all block, and the enable can sit in an admin prompt for as
long as the user takes.
*/
pub(crate) async fn handle_remote_access_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    /*
    CDXC:RemotePairing 2026-09-03:
    Every client (mobile pairing, web, desktop, the `ghostex` CLI) posts the
    JSON-RPC envelope `{ requestId, protocolVersion, params }`, so handler
    input is always the `params` object, never the top-level body.
    */
    let params = match read_domain_rpc_params(body) {
        Ok(params) => Value::Object(params),
        Err(error) => {
            return RemoteAccessHttpError::bad_request(error.message)
                .into_response(endpoint_path, request_id);
        }
    };
    let paths = state.paths.clone();
    let tailcat_runtime = state.tailcat_runtime.clone();
    let pairing_runtime = state.remote_pairing_runtime.clone();
    let device_tokens = state.device_api_tokens.clone();
    let logger = state.logger.clone();
    // Read here: the security log is written from the blocking pool, where the request's caller
    // is not visible.
    let actor = crate::audit::Actor::current();

    let outcome = match endpoint_path.as_str() {
        "/api/remoteAccessStatus" => {
            tokio::task::spawn_blocking(move || {
                let served_ports =
                    read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime).ports;
                let identity = cached_remote_access_identity()?;
                Ok(json!({
                    "computerName": identity.computer_name,
                    "username": identity.username,
                    "platform": identity.platform,
                    "ssh": read_ssh_access_status(ssh_port_for_pairing(&served_ports)),
                    "tailscale": cached_tailscale_status(),
                }))
            })
            .await
        }
        "/api/enableSshAccess" => {
            tokio::task::spawn_blocking(move || {
                let served_ports =
                    read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime).ports;
                let port = ssh_port_for_pairing(&served_ports);
                let result = enable_ssh_access(port);
                crate::audit::record(
                    crate::audit::Action::new(
                        "pairing.enableSsh",
                        "Asked to turn on SSH access (Remote Login) for phones",
                    ),
                    actor,
                    crate::audit::Outcome::Done,
                    None,
                );
                Ok(json!({
                    "outcome": result.outcome,
                    "message": result.message,
                    "ssh": read_ssh_access_status(port),
                }))
            })
            .await
        }
        "/api/remotePairingCode" => {
            // The Show QR dialog is open: a new phone must get through the allow-list to pair.
            let open_tunnel = params
                .get("openTunnel")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            tokio::task::spawn_blocking(move || {
                if open_tunnel {
                    hold_tailcat_pairing_window_open(&paths, &tailcat_runtime);
                }
                let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                let identity = cached_remote_access_identity()?;
                let ssh_port = ssh_port_for_pairing(&status.ports);
                let mut result = serde_json::Map::new();
                if let (Some(qr_address), Some(text_address)) =
                    (pairing_qr_address(&status), public_pairing_address(&status))
                {
                    let db = open_gxserver_database(&paths)?;
                    let secret = pairing_runtime.ensure_secret(&db)?;
                    log_new_pairing_code(&secret.secret_hash, actor);
                    let code = EasyConnectCode {
                        v: 1,
                        address: text_address,
                        name: identity.computer_name.clone(),
                        user: identity.username.clone(),
                        port: tailcat_gxserver_api_port(),
                        ssh_port,
                        secret: Some(secret.secret.clone()),
                        // The phone learns a code expired from `/api/pairDevice`; leaving the
                        // time out of the QR keeps it smaller.
                        expires_at: None,
                    };
                    let text = encode_easy_connect_code(&code);
                    // The QR names our relay when it answers; Copy as text keeps the public address.
                    let qr_code = EasyConnectCode {
                        address: qr_address,
                        ..code.clone()
                    };
                    result.insert(
                        "easyConnect".to_string(),
                        json!({ "payload": easy_connect_pairing_link(&qr_code), "text": text, "code": code }),
                    );
                }
                if let Some((host, ip)) = cached_tailscale_status().reachable_address() {
                    let code = TailscaleCode {
                        v: 1,
                        name: identity.computer_name,
                        host,
                        ip,
                        port: ssh_port,
                        user: identity.username,
                    };
                    let text = encode_tailscale_code(&code);
                    result.insert(
                        "tailscale".to_string(),
                        json!({ "payload": pairing_link(&text), "text": text, "code": code }),
                    );
                }
                Ok(Value::Object(result))
            })
            .await
        }
        "/api/pairedDevices" => {
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                Ok(json!({ "devices": list_paired_devices_json(&db)? }))
            })
            .await
        }
        "/api/removePairedDevice" => {
            let device_id = match read_device_id(&params) {
                Ok(device_id) => device_id,
                Err(error) => return error.into_response(endpoint_path, request_id),
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                let Some(removed) = remove_paired_device(
                    &paths,
                    &db,
                    &tailcat_runtime,
                    &device_tokens,
                    &device_id,
                )?
                else {
                    return Err(RemoteAccessHttpError::unknown_device(&device_id));
                };
                crate::audit::record(
                    crate::audit::Action::new("pairing.remove", "Removed a paired phone")
                        .target(removed.name),
                    actor,
                    crate::audit::Outcome::Done,
                    None,
                );
                Ok(json!({ "devices": list_paired_devices_json(&db)? }))
            })
            .await
        }
        "/api/pairedDeviceSeen" => {
            let device_id = match read_device_id(&params) {
                Ok(device_id) => device_id,
                Err(error) => return error.into_response(endpoint_path, request_id),
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                let device = touch_paired_device_seen(&db, &device_id)?
                    .ok_or_else(|| RemoteAccessHttpError::unknown_device(&device_id))?;
                Ok(json!({ "device": paired_device_json(&device) }))
            })
            .await
        }
        "/api/easyConnectAddresses" => {
            if let Some((details, error)) = failed_dial_report(&params) {
                log_failed_dial(&logger, details, error);
            }
            tokio::task::spawn_blocking(move || {
                let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                Ok(json!({ "addresses": easy_connect_addresses(&status) }))
            })
            .await
        }
        "/api/pairDevice" => {
            let params = match parse_pair_device_params(&params) {
                Ok(params) => params,
                Err(error) => {
                    return RemoteAccessHttpError::from(error)
                        .into_response(endpoint_path, request_id)
                }
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                let device_name = params.device_name.clone();
                let outcome = pair_device(
                    &paths,
                    &db,
                    &tailcat_runtime,
                    &pairing_runtime,
                    &device_tokens,
                    &logger,
                    params,
                );
                // The phone pairing calls with the code alone; the log names it.
                let actor = match actor {
                    crate::audit::Actor::Ghostex => crate::audit::Actor::PairedDevice {
                        id: outcome
                            .as_ref()
                            .ok()
                            .map(|outcome| outcome.device_id.clone()),
                        name: device_name.clone(),
                    },
                    actor => actor,
                };
                crate::audit::record(
                    crate::audit::Action::new("pairing.pair", "A phone paired with this computer")
                        .target(device_name),
                    actor,
                    match &outcome {
                        Ok(_) => crate::audit::Outcome::Done,
                        Err(PairDeviceError::RateLimited) => {
                            crate::audit::Outcome::Refused("Too many pairing attempts.".to_string())
                        }
                        Err(PairDeviceError::CodeExpired) => crate::audit::Outcome::Refused(
                            "The pairing code was wrong, used or expired.".to_string(),
                        ),
                        Err(PairDeviceError::BadRequest(message)) => {
                            crate::audit::Outcome::Refused(message.clone())
                        }
                        Err(PairDeviceError::ApprovalDeclined) => crate::audit::Outcome::Denied,
                        Err(PairDeviceError::Internal(error)) => {
                            crate::audit::Outcome::Failed(format!("{error:#}"))
                        }
                    },
                    None,
                );
                let outcome = outcome?;
                let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                Ok(json!({
                    "deviceId": outcome.device_id,
                    "user": outcome.user,
                    "computerName": outcome.computer_name,
                    "deviceApiToken": outcome.device_api_token,
                    "easyConnectAddresses": easy_connect_addresses(&status),
                }))
            })
            .await
        }
        _ => {
            return RemoteAccessHttpError::bad_request("Unknown remote access endpoint.")
                .into_response(endpoint_path, request_id);
        }
    };

    let outcome: Result<Value, RemoteAccessHttpError> = match outcome {
        Ok(outcome) => outcome,
        Err(error) => Err(anyhow::anyhow!("remote access task failed: {error}").into()),
    };
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => error.into_response(endpoint_path, request_id),
    }
}

/// Logs a pairing code once per code: the Show QR sheet polls `/api/remotePairingCode` while it
/// is open, and the same code comes back until it is used or expires.
fn log_new_pairing_code(secret_hash: &str, actor: crate::audit::Actor) {
    static LAST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if last.as_deref() == Some(secret_hash) {
        return;
    }
    *last = Some(secret_hash.to_string());
    crate::audit::record(
        crate::audit::Action::new("pairing.code", "A pairing code for Easy Connect was shown"),
        actor,
        crate::audit::Outcome::Done,
        None,
    );
}

fn read_device_id(body: &Value) -> Result<String, RemoteAccessHttpError> {
    body.get("deviceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|device_id| !device_id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| RemoteAccessHttpError::bad_request("deviceId is required."))
}

/*
CDXC:RemotePairing 2026-09-03:
One error type for every handler so the HTTP status, the RPC error code, and
the message always travel together. `/api/pairDevice` is the reason the
statuses vary: the phone tells "code expired" (410 `pairingCodeExpired`)
apart from "too many attempts" (429 `pairingRateLimited`) by the code alone.
*/
struct RemoteAccessHttpError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl RemoteAccessHttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "badRequest",
            message: message.into(),
        }
    }

    fn unknown_device(device_id: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "notFound",
            message: format!("No paired device with id {device_id}."),
        }
    }

    fn into_response(self, endpoint_path: String, request_id: String) -> RoutedResponse {
        routed_json(
            Some(endpoint_path),
            self.status,
            rpc_error(self.code, self.message, Some(request_id)),
        )
    }
}

impl From<anyhow::Error> for RemoteAccessHttpError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internalError",
            message: format!("Remote access request failed: {error:#}"),
        }
    }
}

impl From<PairDeviceError> for RemoteAccessHttpError {
    fn from(error: PairDeviceError) -> Self {
        match error {
            PairDeviceError::RateLimited => Self {
                status: StatusCode::TOO_MANY_REQUESTS,
                code: PAIR_DEVICE_RATE_LIMITED_ERROR,
                message: "Too many pairing attempts. Wait a minute and scan again.".to_string(),
            },
            PairDeviceError::CodeExpired => Self {
                status: StatusCode::GONE,
                code: PAIR_DEVICE_CODE_EXPIRED_ERROR,
                message: "This code has expired. Show a fresh code on the computer and scan again."
                    .to_string(),
            },
            PairDeviceError::ApprovalDeclined => Self {
                status: StatusCode::FORBIDDEN,
                code: PAIR_DEVICE_APPROVAL_DECLINED_ERROR,
                message: "The computer asked for administrator approval to add this phone's key, and the Windows prompt was declined. Scan the new code on the computer and choose Yes when Windows asks.".to_string(),
            },
            PairDeviceError::BadRequest(message) => Self::bad_request(message),
            PairDeviceError::Internal(error) => error.into(),
        }
    }
}

/// The phone's own account of an Easy Connect dial it gave up on (`failed_dial_report`).
fn log_failed_dial(logger: &crate::logging::GxserverLogger, details: Value, error: String) {
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "easyConnect.dialFailed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: (!error.is_empty()).then_some(error),
        details: Some(details),
    });
}
