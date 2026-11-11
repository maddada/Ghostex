//! `/api/registerLiveActivity`: a phone hands this computer its Live Activity tokens, takes them back, or asks
//! whether this computer can push its banner and for which computers.
//!
//! - `{action: "status", machineId}`: this computer's server id, the computers it can speak for (connected,
//!   with Agent access on and no approval waiting), and whether it holds this phone's tokens (`holds`) and
//!   pushes with them (`confirmed`). The phone picks its one banner computer from these answers.
//! - `{action: "register", machineId, activityToken?, pushToStartToken?, environment, sessionNames, computers}`:
//!   this computer pushes the phone's banner from now on. `computers` are the phone's ids for its other
//!   computers (`serverId`, `machineId`), so a row of a connected computer opens that session on the phone.
//! - `{action: "clear", machineId}`: this computer stops at once and forgets the tokens.
//!
//! CDXC:Notifications 2026-11-11 DECISION: User, on which computer pushes the banner: "u should push from 1
//! machine ... so u can actually push status of both machines from 1 main machine." The phone picks one
//! computer and clears the others first (apps/mobile/app/src/notifications/liveActivityPush.ts). Like the push
//! registration (push/registration.rs), a phone calling with its own paired-device token registers and clears
//! only as itself: an entry registered by another phone is refused, and unpairing the phone ends its entry.
//! Messages never repeat what the caller sent: a token pasted in the wrong field must not come back or reach a
//! log.

use std::collections::HashSet;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    paths::GxserverPaths,
    protocol::{rpc_error, rpc_success},
    push::{is_printable_id, MACHINE_ID_MAX},
    remote_access::RemotePairingRepository,
    server::{routed_json, AppState, RoutedResponse},
    storage::open_gxserver_database,
};

use super::store::{
    is_live_activity_token, LiveActivityEnvironment, LiveActivityPhone, LiveActivityStore,
    PhoneComputer, MAX_PHONE_COMPUTERS,
};

pub(crate) const REGISTER_LIVE_ACTIVITY_ENDPOINT: &str = "/api/registerLiveActivity";
/// The capability a computer's `ghostex server endpoint` lists when it can push the phone's banner.
pub(crate) const LIVE_ACTIVITY_PUSH_CAPABILITY: &str = "liveActivityPush";

const STATUS_FIELDS: &[&str] = &["action", "machineId"];
const REGISTER_FIELDS: &[&str] = &[
    "action",
    "machineId",
    "activityToken",
    "pushToStartToken",
    "environment",
    "sessionNames",
    "computers",
];
const CLEAR_FIELDS: &[&str] = &["action", "machineId"];

struct NewTokens {
    activity_token: Option<String>,
    push_to_start_token: Option<String>,
    environment: LiveActivityEnvironment,
    session_names: bool,
    computers: Vec<PhoneComputer>,
}

enum Request {
    Status {
        machine_id: String,
    },
    Register {
        machine_id: String,
        tokens: NewTokens,
    },
    Clear {
        machine_id: String,
    },
}

pub(crate) struct RegistrationError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl RegistrationError {
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

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internalError",
            message: message.into(),
        }
    }
}

/// `caller_token_hash`: the SHA-256 of the paired phone's own API token when that token authorized the request.
pub(crate) async fn handle_register_live_activity_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    caller_token_hash: Option<String>,
) -> RoutedResponse {
    let request = match parse(body) {
        Ok(request) => request,
        Err(error) => return error_response(error, endpoint_path, request_id),
    };
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let machine_id = match &request {
        Request::Status { machine_id }
        | Request::Register { machine_id, .. }
        | Request::Clear { machine_id } => machine_id.clone(),
    };
    let registering = matches!(request, Request::Register { .. });
    let clearing = matches!(request, Request::Clear { .. });
    let outcome = tokio::task::spawn_blocking(move || {
        let caller = match caller_token_hash {
            Some(hash) => Some(paired_device_with_token(&paths, &hash)?),
            None => None,
        };
        apply(&paths, request, caller.as_deref())
    })
    .await
    .unwrap_or_else(|_| {
        Err(RegistrationError::internal(
            "The Live Activity registration stopped before it finished.",
        ))
    });
    match outcome {
        Ok(holds) => {
            if registering {
                state.live_activity.confirm(&machine_id, true);
            } else if clearing {
                state.live_activity.confirm(&machine_id, false);
            }
            let computers = speaks_for(&server_id);
            let result = json!({
                "serverId": server_id,
                "computers": computers,
                "holds": holds,
                "confirmed": holds && state.live_activity.is_confirmed(&machine_id),
            });
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => error_response(error, endpoint_path, request_id),
    }
}

/// The computers this one can read for the banner: connected, Agent access on, no approval waiting.
fn speaks_for(server_id: &str) -> Value {
    let computers: Vec<Value> = crate::connected_computers::list()
        .into_iter()
        .filter(|computer| {
            computer.agent_access
                && computer.waiting.is_none()
                && !computer.base_url.is_empty()
                && computer.server_id != server_id
        })
        .map(|computer| json!({ "serverId": computer.server_id, "name": computer.name }))
        .collect();
    Value::Array(computers)
}

fn error_response(
    error: RegistrationError,
    endpoint_path: String,
    request_id: String,
) -> RoutedResponse {
    routed_json(
        Some(endpoint_path),
        error.status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

fn reject_unknown_fields(
    params: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), RegistrationError> {
    if params.keys().all(|key| allowed.contains(&key.as_str())) {
        Ok(())
    } else {
        Err(RegistrationError::bad_request(format!(
            "params may hold only {}.",
            allowed.join(", ")
        )))
    }
}

fn read_machine_id(params: &Map<String, Value>) -> Result<String, RegistrationError> {
    params
        .get("machineId")
        .and_then(Value::as_str)
        .filter(|machine_id| is_printable_id(machine_id, MACHINE_ID_MAX))
        .map(str::to_string)
        .ok_or_else(|| {
            RegistrationError::bad_request(
                "machineId must be 1 to 64 printable characters without spaces.",
            )
        })
}

fn read_token(params: &Map<String, Value>, key: &str) -> Result<Option<String>, RegistrationError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .filter(|token| is_live_activity_token(token))
            .map(|token| Some(token.to_ascii_lowercase()))
            .ok_or_else(|| {
                RegistrationError::bad_request(format!(
                    "{key} must be a token in hex (64 to 512 characters)."
                ))
            }),
    }
}

fn parse(body: &Value) -> Result<Request, RegistrationError> {
    let params = read_domain_rpc_params(body)
        .map_err(|error| RegistrationError::bad_request(error.message))?;
    match params.get("action").and_then(Value::as_str) {
        Some("status") => {
            reject_unknown_fields(&params, STATUS_FIELDS)?;
            Ok(Request::Status {
                machine_id: read_machine_id(&params)?,
            })
        }
        Some("clear") => {
            reject_unknown_fields(&params, CLEAR_FIELDS)?;
            Ok(Request::Clear {
                machine_id: read_machine_id(&params)?,
            })
        }
        Some("register") => {
            reject_unknown_fields(&params, REGISTER_FIELDS)?;
            let machine_id = read_machine_id(&params)?;
            let activity_token = read_token(&params, "activityToken")?;
            let push_to_start_token = read_token(&params, "pushToStartToken")?;
            if activity_token.is_none() && push_to_start_token.is_none() {
                return Err(RegistrationError::bad_request(
                    "A registration needs activityToken or pushToStartToken; clear takes them back.",
                ));
            }
            let environment = match params.get("environment").and_then(Value::as_str) {
                None | Some("production") => LiveActivityEnvironment::Production,
                Some("sandbox") => LiveActivityEnvironment::Sandbox,
                Some(_) => {
                    return Err(RegistrationError::bad_request(
                        "environment must be production or sandbox.",
                    ))
                }
            };
            let session_names = match params.get("sessionNames") {
                None => false,
                Some(value) => value.as_bool().ok_or_else(|| {
                    RegistrationError::bad_request("sessionNames must be a boolean.")
                })?,
            };
            let computers = match params.get("computers") {
                None => Vec::new(),
                Some(value) => serde_json::from_value::<Vec<PhoneComputer>>(value.clone())
                    .ok()
                    .filter(|computers| {
                        computers.len() <= MAX_PHONE_COMPUTERS
                            && computers.iter().all(|computer| {
                                crate::connected_computers::is_server_id(&computer.server_id)
                                    && is_printable_id(&computer.machine_id, MACHINE_ID_MAX)
                            })
                    })
                    .ok_or_else(|| {
                        RegistrationError::bad_request(
                            "computers must be up to 32 objects with serverId and machineId.",
                        )
                    })?,
            };
            Ok(Request::Register {
                machine_id,
                tokens: NewTokens {
                    activity_token,
                    push_to_start_token,
                    environment,
                    session_names,
                    computers,
                },
            })
        }
        _ => Err(RegistrationError::bad_request(
            "action must be status, register or clear.",
        )),
    }
}

/// Applies the request under the store's lock; returns whether this computer now holds the phone's tokens.
fn apply(
    paths: &GxserverPaths,
    request: Request,
    caller: Option<&str>,
) -> Result<bool, RegistrationError> {
    let store = LiveActivityStore::open(paths);
    let mut phones = store
        .read()
        .map_err(|error| RegistrationError::internal(error.to_string()))?;
    let owned_by_another = |phones: &[LiveActivityPhone], machine_id: &str| {
        caller.is_some()
            && phones
                .iter()
                .any(|phone| phone.machine_id == machine_id && phone.device_id.as_deref() != caller)
    };
    match request {
        Request::Status { machine_id } => Ok(phones.iter().any(|phone| {
            phone.machine_id == machine_id
                && (caller.is_none() || phone.device_id.as_deref() == caller)
        })),
        Request::Clear { machine_id } => {
            if owned_by_another(&phones, &machine_id) {
                return Err(RegistrationError::forbidden(
                    "This computer id holds another phone's Live Activity.",
                ));
            }
            let before = phones.len();
            phones.retain(|phone| phone.machine_id != machine_id);
            if phones.len() != before {
                store
                    .write(&phones)
                    .map_err(|error| RegistrationError::internal(error.to_string()))?;
            }
            Ok(false)
        }
        Request::Register { machine_id, tokens } => {
            if owned_by_another(&phones, &machine_id) {
                return Err(RegistrationError::forbidden(
                    "This computer id holds another phone's Live Activity.",
                ));
            }
            if let Some(device_id) = caller {
                if !paired_device_exists(paths, device_id)? {
                    return Err(RegistrationError::forbidden(
                        "This phone is no longer paired with this computer.",
                    ));
                }
            }
            // One entry per phone: the same phone under another computer id (it added this computer again)
            // replaces that one.
            phones.retain(|phone| {
                phone.machine_id != machine_id
                    && (caller.is_none() || phone.device_id.as_deref() != caller)
            });
            let mut seen = HashSet::new();
            let computers = tokens
                .computers
                .into_iter()
                .filter(|computer| seen.insert(computer.server_id.clone()))
                .collect();
            phones.push(LiveActivityPhone {
                machine_id,
                activity_token: tokens.activity_token,
                push_to_start_token: tokens.push_to_start_token,
                environment: tokens.environment,
                seen_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                computers,
                device_id: caller.map(str::to_string),
                session_names: tokens.session_names,
            });
            store
                .write(&phones)
                .map_err(|error| RegistrationError::internal(error.to_string()))?;
            Ok(true)
        }
    }
}

/// The paired phone whose own API token hashes to `token_hash` (as push/registration.rs).
fn paired_device_with_token(
    paths: &GxserverPaths,
    token_hash: &str,
) -> Result<String, RegistrationError> {
    let unreadable = |_| RegistrationError::internal("The paired phones could not be read.");
    let db = open_gxserver_database(paths).map_err(unreadable)?;
    RemotePairingRepository::new(&db)
        .list_devices()
        .map_err(unreadable)?
        .into_iter()
        .find(|device| device.api_token_hash.as_deref() == Some(token_hash))
        .map(|device| device.id)
        .ok_or_else(|| {
            RegistrationError::forbidden("This phone is no longer paired with this computer.")
        })
}

fn paired_device_exists(paths: &GxserverPaths, device_id: &str) -> Result<bool, RegistrationError> {
    let unreadable = |_| RegistrationError::internal("The paired phones could not be read.");
    let db = open_gxserver_database(paths).map_err(unreadable)?;
    RemotePairingRepository::new(&db)
        .find_device(device_id)
        .map(|device| device.is_some())
        .map_err(unreadable)
}
