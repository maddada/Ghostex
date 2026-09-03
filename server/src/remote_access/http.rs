use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    constants::GXSERVER_LOCAL_API_PORT,
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
    storage::open_gxserver_database,
    tailcat::read_tailcat_status_payload_for_paths,
};

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
    let logger = state.logger.clone();

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
                Ok(json!({
                    "outcome": result.outcome,
                    "message": result.message,
                    "ssh": read_ssh_access_status(port),
                }))
            })
            .await
        }
        "/api/remotePairingCode" => {
            tokio::task::spawn_blocking(move || {
                let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                let identity = cached_remote_access_identity()?;
                let ssh_port = ssh_port_for_pairing(&status.ports);
                let mut result = serde_json::Map::new();
                if let Some(address) = status.token.filter(|_| status.running) {
                    let db = open_gxserver_database(&paths)?;
                    let secret = pairing_runtime.ensure_secret(&db)?;
                    let code = EasyConnectCode {
                        v: 1,
                        address,
                        name: identity.computer_name.clone(),
                        user: identity.username.clone(),
                        port: GXSERVER_LOCAL_API_PORT,
                        ssh_port,
                        secret: Some(secret.secret.clone()),
                        expires_at: Some(secret.expires_at_iso()),
                    };
                    result.insert(
                        "easyConnect".to_string(),
                        json!({ "payload": encode_easy_connect_code(&code), "code": code }),
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
                    result.insert(
                        "tailscale".to_string(),
                        json!({ "payload": encode_tailscale_code(&code), "code": code }),
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
                if remove_paired_device(&paths, &db, &tailcat_runtime, &device_id)?.is_none() {
                    return Err(RemoteAccessHttpError::unknown_device(&device_id));
                }
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
                let outcome = pair_device(
                    &paths,
                    &db,
                    &tailcat_runtime,
                    &pairing_runtime,
                    &logger,
                    params,
                )?;
                Ok(json!({
                    "deviceId": outcome.device_id,
                    "user": outcome.user,
                    "computerName": outcome.computer_name,
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
            PairDeviceError::BadRequest(message) => Self::bad_request(message),
            PairDeviceError::Internal(error) => error.into(),
        }
    }
}
