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
                    "ssh": read_ssh_access_status_now(port),
                }))
            })
            .await
        }
        /*
        CDXC:RemotePairing 2026-11-08 DECISION:
        User (GitHub #495, Q1721): "No API returns a pairing secret; only the desktop Show QR sheet
        makes one." The review found this route handing the one-time secret (and so a pairing) to
        any caller with the main token, every agent included. Now only the desktop app's owner
        session gets a code, and only while its Show QR sheet (or the 11.0.1 notice's QR) is open
        (`openTunnel`, which both send); the code lives five minutes and pairs nothing by itself (the phone waits as
        Pending for a signed approval). Every other caller, and the desktop with the sheet closed,
        gets the Easy Connect details without a code (empty `payload`, a `text` code with no `secret`, which a computer connects with and a phone can't pair with),
        and the live code lapses by itself. The Tailscale half carries no secret and is unchanged.
        */
        "/api/remotePairingCode" => {
            let open_tunnel = params
                .get("openTunnel")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let show_code = open_tunnel && may_show_pairing_code(&caller);
            tokio::task::spawn_blocking(move || {
                // The Show QR dialog is open: a new phone must get past the allow-list to pair. It
                // comes in through the door, whose address the QR then carries; the helpers paired
                // devices are on are not touched (CDXC:RemotePairing 2026-11-10 in tailcat/door.rs).
                let door = if show_code {
                    hold_tailcat_pairing_window_open(&paths, &tailcat_runtime);
                    tailcat_runtime.wait_for_open_door(DOOR_READY_TIMEOUT)
                } else {
                    None
                };
                let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                let identity = cached_remote_access_identity()?;
                let ssh_port = ssh_port_for_pairing(&status.ports);
                let mut result = serde_json::Map::new();
                // Without a door (its helper could not start) the QR names a main helper as
                // before, where the new phone's knock lets it in.
                let qr_address = door
                    .map(|door| door.address)
                    .or_else(|| pairing_qr_address(&status));
                if let (Some(qr_address), Some(text_address)) =
                    (qr_address, public_pairing_address(&status))
                {
                    let db = open_gxserver_database(&paths)?;
                    // A closed sheet doesn't drop the live code: the 11.0.1 notice shows the same
                    // code while Settings > Remote polls without `openTunnel`, and the code lapses by
                    // itself within five minutes.
                    let secret = if show_code {
                        let secret = pairing_runtime.ensure_secret(&db)?;
                        log_new_pairing_code(&secret.secret_hash, actor);
                        Some(secret.secret)
                    } else {
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
                    "pending": waiting_devices(&pairing_runtime, &security),
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
                    /*
                    CDXC:RemotePairing 2026-11-08 FIX:
                    User (GitHub #495, 11.1.1): "a denied phone hears 'denied', not 'expired' (in
                    the notice and in Settings' Deny)". Deny used to drop the pending entry, so the
                    phone's next signed ask found nothing and heard "expired". Now only its approval
                    request is denied: the lists stop showing it (they show pending requests only),
                    and the phone's next ask finds the denial, hears "denied", and the entry goes
                    then (or when it lapses).
                    */
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
                        "pending": waiting_devices(&pairing_runtime, &security),
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
                    "pending": waiting_devices(&pairing_runtime, &security),
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
                // The asking device's own key decides the order (`easy_connect_addresses_for`).
                let key = match &caller {
                    Principal::Device { device_id } => open_gxserver_database(&paths)
                        .ok()
                        .and_then(|db| device_dial_key(&db, device_id)),
                    _ => None,
                };
                Ok(json!({ "addresses": addresses_for_key(&status, &tailcat_runtime, key.as_deref()) }))
            })
            .await
        }
        // The desktop app's key for a computer it reaches over Easy Connect (computer_link_keys.rs).
        super::computer_link_keys::EASY_CONNECT_LINK_KEY_ENDPOINT => {
            tokio::task::spawn_blocking(move || {
                Ok(super::computer_link_keys::easy_connect_link_key_answer(
                    &paths, &params,
                )?)
            })
            .await
        }
        // A paired computer registers keys of its own, signed with its pairing device key
        // (CDXC:RemotePairing 2026-11-10 in pair_device.rs). No token: the signature names it.
        COMPUTER_EASY_CONNECT_KEYS_ENDPOINT => {
            let ask: SignedComputerKeysParams = match serde_json::from_value(params.clone()) {
                Ok(ask) => ask,
                Err(error) => {
                    return RemoteAccessHttpError::bad_request(format!("Bad request: {error}"))
                        .into_response(endpoint_path, request_id)
                }
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                let answer = register_computer_keys_signed(
                    &paths,
                    &db,
                    &tailcat_runtime,
                    &pairing_runtime,
                    ask,
                )?;
                if answer.get("changed") == Some(&Value::Bool(true)) {
                    crate::audit::record(
                        crate::audit::Action::new(
                            "pairing.linkKey",
                            "A paired computer gave Easy Connect keys of its own for this computer",
                        ),
                        actor,
                        crate::audit::Outcome::Done,
                        None,
                    );
                }
                Ok(answer)
            })
            .await
        }
        // A paired phone or computer names a key of its own for this computer (CDXC:RemotePairing
        // 2026-11-10 in paired_devices.rs). Only for its own row: the device token says which.
        EASY_CONNECT_CLIENT_KEY_ENDPOINT => {
            let Principal::Device { device_id } = caller.clone() else {
                return RemoteAccessHttpError {
                    status: StatusCode::FORBIDDEN,
                    code: "notPairedDevice",
                    message:
                        "Only a paired phone or computer can register its own Easy Connect key."
                            .to_string(),
                }
                .into_response(endpoint_path, request_id);
            };
            let Some(client_key) = params
                .get("clientKey")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return RemoteAccessHttpError::bad_request("clientKey is required.")
                    .into_response(endpoint_path, request_id);
            };
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&paths)?;
                match register_device_link_key(&paths, &db, &tailcat_runtime, &device_id, &client_key)? {
                    Ok(changed) => {
                        if changed {
                            crate::audit::record(
                                crate::audit::Action::new(
                                    "pairing.linkKey",
                                    "A paired device gave its own Easy Connect key for this computer",
                                )
                                .target(device_id.clone()),
                                actor,
                                crate::audit::Outcome::Done,
                                None,
                            );
                        }
                        Ok(json!({ "clientKey": client_key.trim(), "changed": changed }))
                    }
                    Err(LinkKeyError::BadKey) => Err(RemoteAccessHttpError::bad_request(
                        "clientKey must be a nodekey: public key.",
                    )),
                    Err(LinkKeyError::NotPaired) => Err(RemoteAccessHttpError::unknown_device(&device_id)),
                }
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
                    // The request's id, so how it ended reads against it (`approval.*` lines).
                    started.as_ref().ok().and_then(|started| {
                        pairing_request_ref(&pairing_runtime, &started.pending_id)
                    }),
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
        // A phone signing in over SSH asks to pair through that login (CDXC:RemotePairing
        // 2026-11-08 at `begin_ssh_pairing`). Only the token file asks: its SSH login is how a
        // phone reads it; an agent session's own token, a peer or a paired phone has no business here.
        "/api/pairOverSsh" => {
            if !matches!(caller, Principal::LocalAgent) {
                return RemoteAccessHttpError {
                    status: StatusCode::FORBIDDEN,
                    code: "forbidden",
                    message: "Only a phone signing in over SSH asks to pair this way.".to_string(),
                }
                .into_response(endpoint_path, request_id);
            }
            let params = match parse_ssh_pairing_params(&params) {
                Ok(params) => params,
                Err(error) => {
                    return RemoteAccessHttpError::from(error)
                        .into_response(endpoint_path, request_id)
                }
            };
            tokio::task::spawn_blocking(move || {
                let device_name = params.device_name.clone();
                let started = begin_ssh_pairing(&pairing_runtime, &security, params);
                // The same request handed back again is no new ask: the log keeps the first one.
                if !matches!(&started, Ok(started) if started.reused) {
                    crate::audit::record(
                        crate::audit::Action::new(
                            "pairing.request",
                            "A phone that signs in over SSH asked to pair with this computer",
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
                        started.as_ref().ok().and_then(|started| {
                            pairing_request_ref(&pairing_runtime, &started.started.pending_id)
                        }),
                    );
                }
                let SshPairingStarted { started, nonce, .. } = started?;
                Ok(json!({
                    "status": PairingStatus::Pending,
                    "pendingId": started.pending_id,
                    "nonce": nonce,
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
                /*
                CDXC:RemotePairing 2026-11-09 WHY:
                A phone pairing over its own SSH login (Tailscale, the local network) never comes
                through Easy Connect, yet each of its asks held the allow-list lifted, so every
                launch of an unpaired Tailscale phone opened Easy Connect to any key for as long as
                it waited and restarted the helpers, dropping paired phones' links twice. Only a
                phone or computer pairing through Easy Connect lifts it now.
                */
                let pending = pairing_runtime.pending(&ask.id, crate::security::now_ms());
                let over_ssh = pending.as_ref().is_some_and(|pending| pending.over_ssh);
                let tailcat_client_key = pending.and_then(|pending| pending.tailcat_client_key);
                let answer = pairing_status(&context, ask)?;
                if answer.status == PairingStatus::Pending && !over_ssh {
                    hold_tailcat_pairing_window_open(&paths, &tailcat_runtime);
                }
                let mut result = json!({ "status": answer.status });
                if let (Some(object), Some(Value::Object(approved))) =
                    (result.as_object_mut(), answer.approved)
                {
                    object.extend(approved);
                    // The approval may just have started the helpers again with the new key.
                    tailcat_runtime.wait_for_main_addresses(MAIN_ADDRESSES_TIMEOUT);
                    let status = read_tailcat_status_payload_for_paths(&paths, &tailcat_runtime);
                    object.insert(
                        "easyConnectAddresses".to_string(),
                        json!(addresses_for_key(
                            &status,
                            &tailcat_runtime,
                            tailcat_client_key.as_deref()
                        )),
                    );
                }
                Ok(result)
            })
            .await
        }
        // The connecting computer's side of pairing through Easy Connect (computer_pairing.rs).
        "/api/pairComputer" => {
            let code = match super::computer_pairing::parse_computer_pairing_code(&params) {
                Ok(code) => code,
                Err(error) => {
                    return RemoteAccessHttpError::bad_request(format!("{error:#}"))
                        .into_response(endpoint_path, request_id)
                }
            };
            tokio::task::spawn_blocking(move || {
                let name = code.name.clone();
                let started = super::computer_pairing::start_computer_pairing(&paths, code);
                crate::audit::record(
                    crate::audit::Action::new(
                        "pairing.computerRequest",
                        "Asked another computer to let this one connect through Easy Connect",
                    )
                    .target(name),
                    actor,
                    match &started {
                        Ok(_) => crate::audit::Outcome::Pending,
                        Err(error) => crate::audit::Outcome::Failed(format!("{error:#}")),
                    },
                    None,
                );
                started.map_err(|error| RemoteAccessHttpError::bad_request(format!("{error:#}")))
            })
            .await
        }
        "/api/computerPairingStatus" | "/api/cancelComputerPairing" => {
            let Some(pending_id) = params
                .get("pendingId")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return RemoteAccessHttpError::bad_request("pendingId is required.")
                    .into_response(endpoint_path, request_id);
            };
            let cancel = endpoint_path == "/api/cancelComputerPairing";
            tokio::task::spawn_blocking(move || {
                if cancel {
                    super::computer_pairing::cancel_computer_pairing(&pending_id);
                    return Ok(json!({ "status": "cancelled" }));
                }
                super::computer_pairing::computer_pairing_status(&paths, &pending_id)
                    .map_err(|error| RemoteAccessHttpError::bad_request(format!("{error:#}")))
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

/// The devices waiting for approval whose request still waits: one denied on an approval card (or
/// lapsed) leaves the list at once, before the device itself hears it.
pub(crate) fn waiting_devices(
    pairing_runtime: &RemotePairingRuntime,
    security: &crate::security::SecurityState,
) -> Vec<ghostex_gx_protocol::pairing::PendingDevice> {
    let now = crate::security::now_ms();
    pairing_runtime
        .list_waiting(now)
        .into_iter()
        .filter(|device| {
            security
                .approvals
                .read(&device.approval_id, &Principal::Anonymous, now)
                .is_some_and(|request| {
                    request.status == ghostex_gx_protocol::approval::ApprovalStatus::Pending
                })
        })
        .collect()
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

/// How long the first ask of a pairing window waits for the door's helper to publish its address
/// (a second or two; every later ask finds it running).
const DOOR_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How long an approved device's answer waits for helpers that are just starting to publish
/// their addresses.
const MAIN_ADDRESSES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The key a paired device dials this computer with: the one it registered for this computer,
/// else the one it paired with.
fn device_dial_key(db: &rusqlite::Connection, device_id: &str) -> Option<String> {
    let repository = super::repository::RemotePairingRepository::new(db);
    repository
        .device_link_key(device_id)
        .ok()
        .flatten()
        .or_else(|| repository.find_device(device_id).ok()??.tailcat_client_key)
}

/// A device's Easy Connect addresses in the order that lets its `key` in soonest; the plain list
/// when the key is unknown.
fn addresses_for_key(
    status: &crate::tailcat::TailcatStatusPayload,
    tailcat_runtime: &crate::tailcat::TailcatRuntime,
    key: Option<&str>,
) -> Vec<EasyConnectAddress> {
    match key {
        Some(key) => easy_connect_addresses_for(
            status,
            tailcat_runtime.lanes_admitting(key),
            tailcat_runtime.door_address(),
        ),
        None => easy_connect_addresses(status),
    }
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

/// Only you get a pairing code (the desktop app with its owner session or the token file, a paired
/// phone); an agent's session token can't mint one (CDXC:Security 2026-11-10 in security/policy.rs).
pub(crate) fn may_show_pairing_code(caller: &Principal) -> bool {
    caller.is_owner_device()
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

/// The approval request a waiting pairing waits on, for its `pairing.request` log line.
fn pairing_request_ref(
    runtime: &super::RemotePairingRuntime,
    pending_id: &str,
) -> Option<crate::audit::ApprovalRef> {
    runtime
        .pending(pending_id, crate::security::now_ms())
        .map(|pending| crate::audit::ApprovalRef {
            id: pending.approval_id,
            signer: None,
        })
}
