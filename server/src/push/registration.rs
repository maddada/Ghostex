use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    paths::GxserverPaths,
    protocol::{rpc_error, rpc_success},
    remote_access::RemotePairingRepository,
    server::{routed_json, AppState, RoutedResponse},
    storage::open_gxserver_database,
};

use super::store::{PushKinds, PushRegistration, PushRegistrationStore, PushStoreError};

pub(crate) const REGISTER_PUSH_TOKEN_ENDPOINT: &str = "/api/registerPushToken";
pub(crate) const MACHINE_ID_MAX: usize = 64;
pub(crate) const DEVICE_ID_MAX: usize = 128;
pub(crate) const SESSION_ID_MAX: usize = 64;
pub(crate) const NOTIFICATION_ID_MAX: usize = 128;

const REGISTER_FIELDS: &[&str] = &[
    "action",
    "token",
    "machineId",
    "kinds",
    "deviceId",
    "sessionNames",
];
const UNREGISTER_FIELDS: &[&str] = &["action", "machineId"];

enum PushTokenRequest {
    Register {
        token: String,
        machine_id: String,
        kinds: PushKinds,
        device_id: Option<String>,
        session_names: bool,
    },
    Unregister {
        machine_id: String,
    },
}

/// `ExpoPushToken[…]` or `ExponentPushToken[…]` around 8 to 64 URL-safe characters.
pub(crate) fn is_expo_push_token(value: &str) -> bool {
    let Some(inner) = value
        .strip_prefix("ExponentPushToken[")
        .or_else(|| value.strip_prefix("ExpoPushToken["))
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return false;
    };
    (8..=64).contains(&inner.len())
        && inner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// 1 to `max` printable ASCII characters without spaces.
pub(crate) fn is_printable_id(value: &str, max: usize) -> bool {
    (1..=max).contains(&value.len()) && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// `caller_token_hash` is the SHA-256 of the paired phone's own API token when that token, not the
/// computer's main token, authorized the request (the dispatcher reads it from the request).
pub(crate) async fn handle_register_push_token_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    caller_token_hash: Option<String>,
) -> RoutedResponse {
    let request = match parse_push_token_request(body) {
        Ok(request) => request,
        Err(error) => return error.into_response(endpoint_path, request_id),
    };
    let paths = state.paths.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let caller = match caller_token_hash {
            Some(hash) => Some(paired_device_with_token(&paths, &hash)?),
            None => None,
        };
        apply_push_token_request(&paths, request, caller.as_deref())
    })
    .await
    .unwrap_or_else(|_| {
        Err(PushTokenError::internal(
            "The push registration task stopped before it finished.",
        ))
    });
    match outcome {
        Ok(()) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({})),
        ),
        Err(error) => error.into_response(endpoint_path, request_id),
    }
}

// Messages name the rule a field broke and never repeat what the caller sent: a token pasted
// into the wrong field must not come back in a response or a log.
fn parse_push_token_request(body: &Value) -> Result<PushTokenRequest, PushTokenError> {
    let params =
        read_domain_rpc_params(body).map_err(|error| PushTokenError::bad_request(error.message))?;
    match params.get("action").and_then(Value::as_str) {
        Some("register") => {
            reject_unknown_fields(&params, REGISTER_FIELDS)?;
            let token = params
                .get("token")
                .and_then(Value::as_str)
                .filter(|token| is_expo_push_token(token))
                .ok_or_else(|| PushTokenError::bad_request("token must be an Expo push token."))?;
            let machine_id = read_machine_id(&params)?;
            let kinds = params
                .get("kinds")
                .and_then(|kinds| serde_json::from_value::<PushKinds>(kinds.clone()).ok())
                .ok_or_else(|| {
                    PushTokenError::bad_request(
                        "kinds must hold exactly the booleans finished, waiting and error.",
                    )
                })?;
            let device_id = match params.get("deviceId") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .filter(|device_id| is_printable_id(device_id, DEVICE_ID_MAX))
                        .ok_or_else(|| {
                            PushTokenError::bad_request(
                                "deviceId must be 1 to 128 printable characters without spaces.",
                            )
                        })?
                        .to_string(),
                ),
            };
            // The phone's "Show session names" switch; a phone that sends none keeps names off.
            let session_names = match params.get("sessionNames") {
                None => false,
                Some(value) => value.as_bool().ok_or_else(|| {
                    PushTokenError::bad_request("sessionNames must be a boolean.")
                })?,
            };
            Ok(PushTokenRequest::Register {
                token: token.to_string(),
                machine_id,
                kinds,
                device_id,
                session_names,
            })
        }
        Some("unregister") => {
            reject_unknown_fields(&params, UNREGISTER_FIELDS)?;
            Ok(PushTokenRequest::Unregister {
                machine_id: read_machine_id(&params)?,
            })
        }
        _ => Err(PushTokenError::bad_request(
            "action must be register or unregister.",
        )),
    }
}

fn reject_unknown_fields(
    params: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), PushTokenError> {
    if params.keys().all(|key| allowed.contains(&key.as_str())) {
        Ok(())
    } else {
        Err(PushTokenError::bad_request(format!(
            "params may hold only {}.",
            allowed.join(", ")
        )))
    }
}

fn read_machine_id(params: &Map<String, Value>) -> Result<String, PushTokenError> {
    params
        .get("machineId")
        .and_then(Value::as_str)
        .filter(|machine_id| is_printable_id(machine_id, MACHINE_ID_MAX))
        .map(str::to_string)
        .ok_or_else(|| {
            PushTokenError::bad_request(
                "machineId must be 1 to 64 printable characters without spaces.",
            )
        })
}

/// CDXC:Notifications 2026-11-07 DECISION:
/// User, approving PR #473's iPhone push: "yes. fix pairing also": a phone removed from Paired devices must stop getting pushes. A phone that calls with its own API token (`caller_device`, every phone paired since per-device tokens) registers and unregisters only as itself: its registration is bound to its own pairing whatever `deviceId` it sends (another phone's id is refused), so removing it deletes that registration, and it cannot unregister another phone's computer id. This supersedes the 2026-10-31 rule that took the device id from the request body.
/// CDXC:Notifications 2026-10-31 WHY:
/// A caller with the computer's main token proves no phone, so the computer id and device id it sends are names, not proof of which phone is asking. A registration therefore never takes over what another binding holds: a token or a paired device already registered under another computer id is refused, and a computer id keeps the paired device it was registered with until it is unregistered. The phone unregisters and registers again to change either.
/// A device id must name a phone paired with this computer, checked while the store lock is held; unpairing holds the same lock from deleting that phone's registration until its pairing row is gone, so a registration racing an unpair cannot leave a token behind.
/// SEE-ALSO: server/src/remote_access/paired_devices.rs (unpair deletes the linked registration).
fn apply_push_token_request(
    paths: &GxserverPaths,
    request: PushTokenRequest,
    caller_device: Option<&str>,
) -> Result<(), PushTokenError> {
    let store = PushRegistrationStore::open(paths);
    let mut data = store.read()?;
    let registrations = &mut data.registrations;
    match request {
        PushTokenRequest::Unregister { machine_id } => {
            if let Some(caller) = caller_device {
                if registrations.iter().any(|registration| {
                    registration.machine_id == machine_id
                        && registration.device_id.as_deref() != Some(caller)
                }) {
                    return Err(PushTokenError::forbidden(
                        "This computer id is registered by another phone.",
                    ));
                }
            }
            let before = registrations.len();
            registrations.retain(|registration| registration.machine_id != machine_id);
            if registrations.len() == before {
                return Ok(());
            }
        }
        PushTokenRequest::Register {
            token,
            machine_id,
            kinds,
            device_id,
            session_names,
        } => {
            let device_id = match (caller_device, device_id) {
                (Some(caller), Some(sent)) if sent != caller => {
                    return Err(PushTokenError::forbidden(
                        "deviceId must name this phone's own pairing.",
                    ));
                }
                (Some(caller), _) => Some(caller.to_string()),
                (None, sent) => sent,
            };
            let existing = registrations
                .iter()
                .position(|registration| registration.machine_id == machine_id);
            if existing.is_some_and(|index| registrations[index].device_id != device_id) {
                return Err(PushTokenError::conflict(
                    "This computer id is registered with a different pairing. Unregister it, then register again.",
                ));
            }
            let held_by_another_id = |holds: &dyn Fn(&PushRegistration) -> bool| {
                registrations.iter().any(|registration| {
                    registration.machine_id != machine_id && holds(registration)
                })
            };
            if held_by_another_id(&|registration| registration.token == token) {
                return Err(PushTokenError::conflict(
                    "This push token is registered under another computer id. Unregister that one first.",
                ));
            }
            if let Some(device_id) = device_id.as_deref() {
                if held_by_another_id(&|registration| {
                    registration.device_id.as_deref() == Some(device_id)
                }) {
                    return Err(PushTokenError::conflict(
                        "This paired phone is registered under another computer id. Unregister that one first.",
                    ));
                }
                if !paired_device_exists(paths, device_id)? {
                    return Err(PushTokenError {
                        status: StatusCode::NOT_FOUND,
                        code: "notFound",
                        message: "deviceId does not name a phone paired with this computer."
                            .to_string(),
                    });
                }
            }
            let registration = PushRegistration {
                token,
                machine_id,
                kinds,
                device_id,
                session_names,
                registered_at: chrono::Utc::now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            };
            match existing {
                Some(index) => registrations[index] = registration,
                None => registrations.push(registration),
            }
        }
    }
    store.write(&data)?;
    Ok(())
}

/// The paired phone whose own API token hashes to `token_hash`.
fn paired_device_with_token(
    paths: &GxserverPaths,
    token_hash: &str,
) -> Result<String, PushTokenError> {
    let unreadable = |_| PushTokenError::internal("The paired phones could not be read.");
    let db = open_gxserver_database(paths).map_err(unreadable)?;
    RemotePairingRepository::new(&db)
        .list_devices()
        .map_err(unreadable)?
        .into_iter()
        .find(|device| device.api_token_hash.as_deref() == Some(token_hash))
        .map(|device| device.id)
        .ok_or_else(|| {
            PushTokenError::forbidden("This phone is no longer paired with this computer.")
        })
}

fn paired_device_exists(paths: &GxserverPaths, device_id: &str) -> Result<bool, PushTokenError> {
    let unreadable = |_| PushTokenError::internal("The paired phones could not be read.");
    let db = open_gxserver_database(paths).map_err(unreadable)?;
    RemotePairingRepository::new(&db)
        .find_device(device_id)
        .map(|device| device.is_some())
        .map_err(unreadable)
}

struct PushTokenError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl PushTokenError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "badRequest",
            message: message.into(),
        }
    }

    fn forbidden(message: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: message.to_string(),
        }
    }

    fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict",
            message: message.to_string(),
        }
    }

    fn internal(message: &str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internalError",
            message: message.to_string(),
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

impl From<PushStoreError> for PushTokenError {
    fn from(error: PushStoreError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internalError",
            message: error.to_string(),
        }
    }
}
