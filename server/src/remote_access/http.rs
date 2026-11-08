use axum::http::StatusCode;
use serde_json::{json, Value};

use ghostex_gx_protocol::pairing::PairingStatus;

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    security::Principal,
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
    caller: &Principal,
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
    let security = state.security.clone();
    let caller = caller.clone();
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
        /*
        CDXC:RemotePairing 2026-11-08 DECISION:
        User (GitHub #495, Q1721): "No API returns a pairing secret; only the desktop Show QR sheet
        makes one." The review found this route handing the one-time secret (and so a pairing) to
        any caller with the main token, every agent included. Now only the desktop app's owner
        session gets a code, and only while its Show QR sheet is open (`openTunnel`, which the
        sheet sends); the code lives five minutes and pairs nothing by itself (the phone waits as
        Pending for a signed approval). Every other caller, and the desktop with the sheet closed,
        gets the Easy Connect details without a code (empty `payload`, a `text` code with no `secret`, which a computer connects with and a phone can't pair with),
        and closing the sheet drops the live code. The Tailscale half carries no secret and is
        unchanged.
        */
        "/api/remotePairingCode" => {
            let open_tunnel = params
                .get("openTunnel")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let show_code = open_tunnel && may_show_pairing_code(&caller);
            let owner = may_show_pairing_code(&caller);
            tokio::task::spawn_blocking(move || {
                if show_code {
                    // The Show QR dialog is open: a new phone must get through the allow-list to pair.
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
                    let secret = if show_code {
                        let secret = pairing_runtime.ensure_secret(&db)?;
                        log_new_pairing_code(&secret.secret_hash, actor);
                        Some(secret.secret)
                    } else {
                        if owner {
                            pairing_runtime.cancel_secret(&db)?;
                        }
                        None
                    };
                    let code = EasyConnectCode {
                        v: 1,
                        address: text_address,
                        name: identity.computer_name.clone(),
                        user: identity.username.clone(),
                        port: tailcat_gxserver_api_port(),
                        ssh_port,
                        secret,
                        // The phone learns a code expired from `/api/pairDevice`; leaving the
                        // time out of the QR keeps it smaller.
                        expires_at: None,
                    };
                    result.insert(
                        "easyConnect".to_string(),
                        easy_connect_pairing_entry(code, qr_address),
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
        // `pending`: phones waiting for approval, with the words to compare (pairing v2).
        "/api/pairedDevices" => {
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                Ok(json!({
                    "devices": list_paired_devices_json(&db)?,
                    "pending": pairing_runtime.list_waiting(crate::security::now_ms()),
                }))
            })
            .await
        }
        /*
        CDXC:RemotePairing 2026-11-08 DECISION:
        User (GitHub #495, Q1721): "One-tap revoke." Removing is always allowed (it only closes a
        door), from the desktop or any paired phone: the device's SSH key line, its token, its push
        registration, its tailcat allow-list entry and its approver key go at once. A Pending
        device's id denies its approval request and drops it.
        */
        "/api/removePairedDevice" => {
            let device_id = match read_device_id(&params) {
                Ok(device_id) => device_id,
                Err(error) => return error.into_response(endpoint_path, request_id),
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                let now = crate::security::now_ms();
                if let Some(pending) = pairing_runtime.pending(&device_id, now) {
                    pairing_runtime.remove_pending(&device_id);
                    crate::audit::record(
                        crate::audit::Action::new("pairing.deny", "Denied a phone waiting to pair")
                            .target(pending.name.clone()),
                        actor,
                        crate::audit::Outcome::Done,
                        None,
                    );
                    if let Ok(request) = security.approvals.deny(&pending.approval_id, &caller, now)
                    {
                        security.publish(crate::security::SecurityEvent::ApprovalDenied {
                            request,
                            by: caller.clone(),
                        });
                    }
                    return Ok(json!({
                        "devices": list_paired_devices_json(&db)?,
                        "pending": pairing_runtime.list_waiting(now),
                    }));
                }
                let Some(device) = remove_paired_device(
                    &paths,
                    &db,
                    &tailcat_runtime,
                    &device_tokens,
                    &device_id,
                )?
                else {
                    return Err(RemoteAccessHttpError::unknown_device(&device_id));
                };
                forget_approver_key(&security, &device, &caller);
                // A revoke never needs approval, so it has no SensitiveAction; the log still says it.
                crate::audit::record(
                    crate::audit::Action::new("pairing.remove", "Removed a paired phone")
                        .target(device.name.clone()),
                    actor,
                    crate::audit::Outcome::Done,
                    None,
                );
                Ok(json!({
                    "devices": list_paired_devices_json(&db)?,
                    "pending": pairing_runtime.list_waiting(now),
                }))
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
                let started = begin_pairing(&db, &pairing_runtime, &security, params);
                // The phone asks with the code alone; the log names it.
                crate::audit::record(
                    crate::audit::Action::new(
                        "pairing.request",
                        "A new phone asked to pair with this computer",
                    )
                    .target(device_name.clone()),
                    crate::audit::Actor::PairedDevice {
                        id: None,
                        name: device_name,
                    },
                    match &started {
                        Ok(_) => crate::audit::Outcome::Pending,
                        Err(error) => pair_device_error_outcome(error),
                    },
                    None,
                );
                let started = started?;
                // The phone waits through the tunnel; keep a locked Easy Connect open to it.
                hold_tailcat_pairing_window_open(&paths, &tailcat_runtime);
                Ok(json!({
                    "status": PairingStatus::Pending,
                    "pendingId": started.pending_id,
                    "words": started.words,
                    "expiresAtMs": started.expires_at_ms,
                    "user": started.user,
                    "computerName": started.computer_name,
                }))
            })
            .await
        }
        "/api/pairingStatus" => {
            let ask = match parse_signed_ask(&params) {
                Ok(ask) => ask,
                Err(error) => {
                    return RemoteAccessHttpError::from(error)
                        .into_response(endpoint_path, request_id)
                }
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                let context = PairingContext {
                    paths: &paths,
                    db: &db,
                    tailcat_runtime: &tailcat_runtime,
                    pairing_runtime: &pairing_runtime,
                    device_tokens: &device_tokens,
                    security: &security,
                    logger: &logger,
                    append_key: &append_authorized_key,
                    remove_key: &remove_authorized_key,
                };
                let answer = pairing_status(&context, ask)?;
                if answer.status == PairingStatus::Pending {
                    hold_tailcat_pairing_window_open(&paths, &tailcat_runtime);
                }
                let mut result = json!({ "status": answer.status });
                if let (Some(object), Some(Value::Object(approved))) =
                    (result.as_object_mut(), answer.approved)
                {
                    object.extend(approved);
                    let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                    object.insert(
                        "easyConnectAddresses".to_string(),
                        json!(easy_connect_addresses(&status)),
                    );
                }
                Ok(result)
            })
            .await
        }
        "/api/renewDeviceToken" => {
            let ask = match parse_signed_ask(&params) {
                Ok(ask) => ask,
                Err(error) => {
                    return RemoteAccessHttpError::from(error)
                        .into_response(endpoint_path, request_id)
                }
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                Ok(renew_device_token(
                    &db,
                    &pairing_runtime,
                    &device_tokens,
                    ask,
                )?)
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

/// How a refused or failed pairing step reads in the security log.
fn pair_device_error_outcome(error: &PairDeviceError) -> crate::audit::Outcome {
    use crate::audit::Outcome;
    match error {
        PairDeviceError::RateLimited => Outcome::Refused("Too many pairing attempts.".to_string()),
        PairDeviceError::CodeExpired => {
            Outcome::Refused("The pairing code was wrong, used or expired.".to_string())
        }
        PairDeviceError::BadRequest(message) | PairDeviceError::BadSignature(message) => {
            Outcome::Refused(message.clone())
        }
        PairDeviceError::NeedsAppUpdate => {
            Outcome::Refused("The phone's Ghostex is too old to pair.".to_string())
        }
        PairDeviceError::ApprovalDeclined => Outcome::Denied,
        PairDeviceError::Internal(error) => Outcome::Failed(format!("{error:#}")),
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

/// The Easy Connect half of `/api/remotePairingCode`: `payload` (the QR link) only when `code`
/// carries a secret (the Show QR sheet), empty otherwise; `text` (Copy as text, and the code another
/// computer connects with, which needs no secret) carries the secret only in that same case.
pub(crate) fn easy_connect_pairing_entry(code: EasyConnectCode, qr_address: String) -> Value {
    let payload = if code.secret.is_some() {
        // The QR names our relay when it answers; Copy as text keeps the public address.
        let qr_code = EasyConnectCode {
            address: qr_address,
            ..code.clone()
        };
        easy_connect_pairing_link(&qr_code)
    } else {
        String::new()
    };
    let text = encode_easy_connect_code(&code);
    json!({ "payload": payload, "text": text, "code": code })
}

/// Only the desktop app's own owner session (GitHub #495 Phase 2: minted after it proved the
/// owner key, held in its memory) may get a pairing code.
pub(crate) fn may_show_pairing_code(caller: &Principal) -> bool {
    matches!(caller, Principal::Owner { .. })
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
            PairDeviceError::NeedsAppUpdate => Self {
                status: StatusCode::UPGRADE_REQUIRED,
                code: PAIR_DEVICE_NEEDS_UPDATE_ERROR,
                message: "Update Ghostex on this phone to pair it: this computer now pairs phones with their own device key, which this version of the app can't make.".to_string(),
            },
            PairDeviceError::BadSignature(message) => Self {
                status: StatusCode::UNAUTHORIZED,
                code: PAIRING_SIGNATURE_ERROR,
                message,
            },
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
