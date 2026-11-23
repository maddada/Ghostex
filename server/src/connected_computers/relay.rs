//! `/api/computers/<serverId>/relay/api/...`: an agent's request for another computer, forwarded
//! over the connection this computer already has to it.

use crate::session_chat_send::sender_mark::AsUserLog;
use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use serde_json::json;

use crate::{
    constants::GXSERVER_PROTOCOL_HEADER,
    paths::GxserverPaths,
    protocol::rpc_error,
    security::Principal,
    server::{json_response, routed_json, RoutedResponse},
};

use super::registry;

pub(crate) const RELAY_ROUTE_PREFIX: &str = "/api/computers/";
const RELAY_SEGMENT: &str = "/relay";
const RELAY_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// The computer and the request path of a relay URL, or `None` when the path is not one.
pub(crate) fn parse_relay_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix(RELAY_ROUTE_PREFIX)?;
    let (server_id, forwarded) = rest.split_once(RELAY_SEGMENT)?;
    (registry::is_server_id(server_id)
        && forwarded.starts_with("/api/")
        && !forwarded.contains(".."))
    .then_some((server_id, forwarded))
}

/// Whether the relay forwards `forwarded`: only the endpoints another computer's agents may use,
/// and never another relay, so a request never chains from A through B to C.
///
/// `/api/gbotsRemote` names the session asking by its ids, which only this computer's gxserver
/// may vouch for, so it is sent by gxserver itself (gbots/computer/home.rs) and never relayed.
pub(crate) fn relay_forwards(forwarded: &str) -> bool {
    !forwarded.starts_with(RELAY_ROUTE_PREFIX)
        && forwarded != crate::gbots::GBOTS_REMOTE_ENDPOINT
        && super::scope::is_peer_allowed(forwarded)
}

/// The relay base a CLI sends every request of that computer to.
pub(crate) fn relay_base_url(local_base_url: &str, server_id: &str) -> String {
    format!("{local_base_url}{RELAY_ROUTE_PREFIX}{server_id}{RELAY_SEGMENT}")
}

/// Forwards the request. The caller has already checked it carries this gxserver's own token: a
/// token another computer was given never relays onward. The other computer is reached with the
/// token it gave this one, which its gxserver limits to the same allowlist.
pub(crate) async fn relay(
    method: Method,
    path: &str,
    headers: HeaderMap,
    body: Body,
    request_id: String,
    caller: &Principal,
    paths: &GxserverPaths,
    server_id_here: &str,
) -> RoutedResponse {
    let endpoint_path = Some(RELAY_ROUTE_PREFIX.to_string());
    let Some((server_id, forwarded)) = parse_relay_path(path) else {
        return refusal(
            StatusCode::NOT_FOUND,
            "notFound",
            format!("{path} is not a relay path."),
            request_id,
        );
    };
    if !relay_forwards(forwarded) {
        return refusal(
            StatusCode::FORBIDDEN,
            "forbidden",
            format!("Agents can't use {forwarded} on another computer."),
            request_id,
        );
    }
    let Some(computer) = registry::find(server_id) else {
        return refusal(
            StatusCode::NOT_FOUND,
            "computerNotConnected",
            "That computer isn't connected to this one. Connect it in Settings > Remote > Remote machines, then try again.".to_string(),
            request_id,
        );
    };
    if let Some(message) = computer.waiting_message() {
        return refusal(
            StatusCode::FORBIDDEN,
            "approvalPending",
            message,
            request_id,
        );
    }
    if !computer.agent_access {
        return refusal(
            StatusCode::FORBIDDEN,
            "agentAccessOff",
            access_off_message(&computer),
            request_id,
        );
    }
    if method != Method::POST && !(method == Method::GET && forwarded == "/api/health/server") {
        return refusal(
            StatusCode::METHOD_NOT_ALLOWED,
            "methodNotAllowed",
            format!("{forwarded} requires POST."),
            request_id,
        );
    }
    let bytes = match to_bytes(body, RELAY_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return refusal(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payloadTooLarge",
                format!(
                    "A request to another computer can be at most {RELAY_BODY_LIMIT_BYTES} bytes."
                ),
                request_id,
            );
        }
    };
    // Whose thread a session is only this computer knows (thread_scope.rs).
    let refused = {
        let db = crate::storage::open_gxserver_database(paths).ok();
        super::thread_scope::refusal(db.as_deref(), caller, server_id, forwarded, &bytes)
    };
    if let Some(message) = refused {
        return refusal(
            StatusCode::FORBIDDEN,
            "forbidden",
            message.to_string(),
            request_id,
        );
    }
    // An agent's message to another computer says who sent it before it leaves; the other
    // computer passes it on as it came (session_chat_send/sender_mark.rs). One of the user's own
    // callers' messages leaves marked as the user's (`userSend`), so the other computer records it
    // as theirs (quotes of earlier messages, chat_message_numbers.rs); nobody else's carries it.
    // The security log entry of a send as the user, written once the other computer took it.
    let mut as_user_log = None;
    let bytes = if forwarded == crate::session_chat_send::HELD_WITH_NEXT_ENDPOINT {
        // Who lists or withdraws a held note there is the session calling here, or no session.
        match stamped_sender(caller, &bytes) {
            Ok(stamped) => stamped.into(),
            Err(message) => {
                return refusal(
                    StatusCode::BAD_REQUEST,
                    "badRequest",
                    message.to_string(),
                    request_id,
                )
            }
        }
    } else if crate::session_chat_send::sender_mark::MESSAGE_ROUTES.contains(&forwarded) {
        match marked_message_body_logged(paths, server_id_here, server_id, caller, &bytes) {
            Ok((marked, log)) => {
                as_user_log = log;
                marked.into()
            }
            Err(message) => {
                return refusal(
                    StatusCode::BAD_REQUEST,
                    "badRequest",
                    message.to_string(),
                    request_id,
                )
            }
        }
    } else if forwarded == "/api/createAgentSession" {
        match marked_create_body(paths, server_id_here, server_id, caller, &bytes) {
            Ok((marked, log)) => {
                as_user_log = log;
                marked.into()
            }
            Err(message) => {
                return refusal(
                    StatusCode::BAD_REQUEST,
                    "badRequest",
                    message.to_string(),
                    request_id,
                )
            }
        }
    } else {
        bytes
    };
    // A message to a done thread of a coordinator here reopens it once the other computer took it
    // (coordinators/remote_threads.rs `reopen_messaged_remote_thread`).
    let reopen_target = (forwarded == "/api/sendSessionChatMessage" && is_chat_message(&bytes))
        .then(|| super::thread_scope::session_ids(&bytes))
        .flatten()
        .map(|(project_id, session_id)| {
            (
                (server_id.to_string(), project_id, session_id),
                crate::coordinators::now_iso(),
            )
        });
    // The mark rides in a header only this relay sets, so an older sender's body can't pass it.
    let user_send = crate::session_chat_send::sender_mark::MESSAGE_ROUTES.contains(&forwarded)
        && crate::session_chat_send::sender_mark::is_users_caller(caller);
    let relayed_non_user_device = !caller.is_user_device();
    // Defense-in-depth on the sending relay too (protects older receiving servers as well):
    // refuse shift-tab approval cycling and elevated session creates from non-user-device callers.
    if relayed_non_user_device {
        if let Ok(parsed_body) = forwarded_body(&bytes) {
            if let Some(params) = parsed_body.get("params").and_then(serde_json::Value::as_object) {
                if forwarded == "/api/sendSessionChatMessage" {
                    if let Some(key) = params.get("key").and_then(serde_json::Value::as_str) {
                        let norm = key.trim().to_ascii_lowercase();
                        if matches!(
                            norm.as_str(),
                            "shift-tab" | "shift+tab" | "backtab" | "\u{1b}[z"
                        ) {
                            return refusal(
                                StatusCode::FORBIDDEN,
                                "forbidden",
                                "Only your own devices can cycle approval mode with Shift+Tab."
                                    .to_string(),
                                request_id,
                            );
                        }
                    }
                }
                if matches!(
                    forwarded,
                    "/api/createSession" | "/api/createAgentSession" | "/api/createWorktreeSession"
                ) {
                    let level_elevated = params
                        .get("runtimeSettings")
                        .and_then(serde_json::Value::as_object)
                        .and_then(|rt| rt.get(crate::acp::SESSION_ACP_APPROVAL_LEVEL_KEY))
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .is_some_and(|raw| {
                            crate::acp::AcpApprovalLevel::parse(raw)
                                != Some(crate::acp::AcpApprovalLevel::Ask)
                        });
                    let accept_all_elevated = params
                        .get("launchSettings")
                        .and_then(serde_json::Value::as_object)
                        .and_then(|l| l.get("acceptAllMode"))
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|v| crate::acp::approval::is_accept_all_enabled_value(Some(v)));
                    let raw_plan_on_create = forwarded == "/api/createSession"
                        && params
                            .get("launchSettings")
                            .and_then(serde_json::Value::as_object)
                            .is_some_and(|l| {
                                l.get("agentLaunchPlan").is_some_and(|v| !v.is_null())
                                    || l.get("startupText")
                                        .and_then(serde_json::Value::as_str)
                                        .is_some_and(|s| !s.trim().is_empty())
                            });
                    if level_elevated || accept_all_elevated || raw_plan_on_create {
                        return refusal(
                            StatusCode::FORBIDDEN,
                            "forbidden",
                            "Only your own devices can start a session with elevated approval or a raw launch plan."
                                .to_string(),
                            request_id,
                        );
                    }
                }
            }
        }
    }
    let protocol = header_text(&headers, GXSERVER_PROTOCOL_HEADER);
    let content_type = header_text(&headers, header::CONTENT_TYPE.as_str());
    let url = format!("{}{forwarded}", computer.base_url);
    let token = computer.token().to_string();
    let name = computer.name.clone();
    let forwarded_request_id = request_id.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        // The grant goes only to a listener of this account: the tunnel's port, once freed, could
        // be someone else's (ghostex_paths::loopback_owner).
        computer.check_listener(&url).map(|()| {
            forward(
                &method,
                &url,
                &token,
                protocol.as_deref(),
                content_type.as_deref(),
                &forwarded_request_id,
                user_send,
                relayed_non_user_device,
                bytes.to_vec(),
            )
        })
    })
    .await;
    match outcome {
        Ok(Err(foreign)) => refusal(
            StatusCode::BAD_GATEWAY,
            ghostex_paths::loopback_owner::FOREIGN_LISTENER_CODE,
            foreign.to_string(),
            request_id,
        ),
        Ok(Ok(Ok((status, content_type, body)))) => {
            if let Some(log) = as_user_log {
                log.record(relayed_send_outcome(&name, status, &body));
            }
            if let Some((key, sent_at)) = reopen_target.filter(|_| (200..300).contains(&status)) {
                let (paths, caller) = (paths.clone(), caller.clone());
                let _ = tokio::task::spawn_blocking(move || {
                    let db = crate::storage::open_gxserver_database(&paths).ok()?;
                    crate::coordinators::reopen_messaged_remote_thread(&db, &key, &caller, &sent_at)
                        .ok()
                })
                .await;
            }
            let mut response = Response::new(Body::from(body));
            *response.status_mut() =
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            if let Some(value) = content_type.and_then(|value| HeaderValue::from_str(&value).ok()) {
                response.headers_mut().insert(header::CONTENT_TYPE, value);
            }
            RoutedResponse {
                endpoint_path,
                response,
            }
        }
        _ => refusal(
            StatusCode::BAD_GATEWAY,
            "computerUnreachable",
            format!(
                "{name} didn't answer. Ghostex reconnects it on its own, so try again in a moment."
            ),
            request_id,
        ),
    }
}

/// A chat message, not a raw key (`params.key`, send_http.rs), which never reopens a thread.
fn is_chat_message(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .is_some_and(|body| {
            body.pointer("/params/key")
                .is_none_or(serde_json::Value::is_null)
        })
}

/// Why nothing reaches `computer` while its Agent access is off, naming whose switch it is.
pub(crate) fn access_off_message(computer: &super::ConnectedComputer) -> String {
    match computer.off_by {
        Some(super::registry::OffBy::ThisComputer) | None => format!(
            "Agent access to {name} is off. Turn on Agent access for {name} in Settings > Remote > Remote machines.",
            name = computer.name
        ),
        Some(super::registry::OffBy::OtherComputer) => format!(
            "{name} has Agent access for this computer turned off. It is turned back on in {name}'s Settings > Remote > Remote machines.",
            name = computer.name
        ),
    }
}

/// Relay-only header marking that the originating caller on the sending computer was NOT a
/// user device (`Principal::Session`, guest, extension). Receiving servers honor this only on
/// authenticated `Principal::Peer` requests to keep non-user-device approval/launch guards active.
pub(crate) const RELAYED_NON_USER_DEVICE_HEADER: &str = "x-ghostex-relayed-non-user-device";

fn forward(
    method: &Method,
    url: &str,
    token: &str,
    protocol: Option<&str>,
    content_type: Option<&str>,
    request_id: &str,
    user_send: bool,
    relayed_non_user_device: bool,
    body: Vec<u8>,
) -> Result<(u16, Option<String>, Vec<u8>), ureq::Error> {
    let agent = crate::http_agent::answering_agent();
    let authorization = format!("Bearer {token}");
    let response = if *method == Method::GET {
        let mut request = agent
            .get(url)
            .header("authorization", &authorization)
            .header("x-request-id", request_id);
        if let Some(protocol) = protocol {
            request = request.header(GXSERVER_PROTOCOL_HEADER, protocol);
        }
        if relayed_non_user_device {
            request = request.header(RELAYED_NON_USER_DEVICE_HEADER, "1");
        }
        request
            .config()
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .call()?
    } else {
        let mut request = agent
            .post(url)
            .header("authorization", &authorization)
            .header("x-request-id", request_id)
            .header("content-type", content_type.unwrap_or("application/json"));
        if let Some(protocol) = protocol {
            request = request.header(GXSERVER_PROTOCOL_HEADER, protocol);
        }
        if user_send {
            request = request.header(
                crate::session_chat_send::sender_mark::RELAYED_USER_SEND_HEADER,
                "1",
            );
        }
        if relayed_non_user_device {
            request = request.header(RELAYED_NON_USER_DEVICE_HEADER, "1");
        }
        // Long polls (`read-session-chat --wait-ms`) wait on the other computer, so the relay
        // leaves the time limit to the CLI that asked.
        request
            .config()
            .timeout_global(Some(Duration::from_secs(15 * 60)))
            .build()
            .send(&body[..])?
    };
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut response.into_body().into_reader(), &mut bytes)
        .map_err(ureq::Error::Io)?;
    Ok((status, content_type, bytes))
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// A message route's body as it leaves for the other computer: one of the user's own callers'
/// marked as theirs, an agent's session's signed. Refused unless it is a JSON object with an
/// object `params`, read with the same whitespace rule as the receiving gxserver
/// (`server/http_infra.rs`), so nothing reaches the other computer unmarked.
///
/// CDXC:ServerLinks 2026-11-20 FIX:
/// Fable's review of 809e881f4 found the relay passed a body it could not parse on untouched:
/// serde_json takes only space, tab, LF and CR around a document while the receiver trims every
/// Unicode space first, so an agent's body wrapped in a form feed or NBSP reached the other
/// computer neither signed nor stripped of `userSend`.
#[cfg(test)]
pub(crate) fn marked_message_body(
    paths: &crate::paths::GxserverPaths,
    server_id_here: &str,
    target_server_id: &str,
    caller: &Principal,
    bytes: &[u8],
) -> Result<Vec<u8>, &'static str> {
    marked_message_body_logged(paths, server_id_here, target_server_id, caller, bytes)
        .map(|(body, _)| body)
}

const NOT_A_MESSAGE: &str = "A message must be a JSON object with an object `params`.";

/// A forwarded body read as the receiving gxserver reads it (see `marked_message_body`).
fn forwarded_body(bytes: &[u8]) -> Result<serde_json::Value, &'static str> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| NOT_A_MESSAGE)?
        .trim();
    let body: serde_json::Value = serde_json::from_str(text).map_err(|_| NOT_A_MESSAGE)?;
    if !body.get("params").is_some_and(serde_json::Value::is_object) {
        return Err(NOT_A_MESSAGE);
    }
    Ok(body)
}

/// Whether the other computer's answer to a relayed send says it went through: a success status
/// and not `ok: false`; otherwise why not (its message, or the status).
fn relayed_send_outcome(computer: &str, status: u16, body: &[u8]) -> Result<(), String> {
    let answer = serde_json::from_slice::<serde_json::Value>(body).ok();
    let refused = answer
        .as_ref()
        .is_some_and(|answer| answer.get("ok") == Some(&serde_json::Value::Bool(false)));
    if (200..300).contains(&status) && !refused {
        return Ok(());
    }
    Err(answer
        .as_ref()
        .and_then(|answer| answer.get("message").or_else(|| answer.get("error")))
        .and_then(serde_json::Value::as_str)
        .map(|message| format!("{computer}: {message}"))
        .unwrap_or_else(|| format!("{computer} answered {status}.")))
}

/// `marked_message_body`, with the security log entry of a send as the user, for the relay to
/// write once the other computer took it.
fn marked_message_body_logged(
    paths: &crate::paths::GxserverPaths,
    server_id_here: &str,
    target_server_id: &str,
    caller: &Principal,
    bytes: &[u8],
) -> Result<(Vec<u8>, Option<AsUserLog>), &'static str> {
    let mut body = forwarded_body(bytes)?;
    let params = body
        .get_mut("params")
        .and_then(|params| params.as_object_mut())
        .ok_or(NOT_A_MESSAGE)?;
    crate::session_chat_send::sender_mark::mark_relayed_send(caller, params);
    stamp_sender(caller, params);
    // `--as-user --computer`: the other computer drops `via`, so the label and the security log
    // entry are written here, as for a chat on this computer (Fable's review of 809e881f4).
    let via = params
        .remove("via")
        .and_then(|value| value.as_str().map(str::to_string));
    let log = crate::session_chat_send::sender_mark::label_as_user_send(
        target_server_id,
        caller,
        via.as_deref(),
        params,
    )
    .map(|(_, log)| log);
    if let Principal::Session {
        project_id,
        session_id,
        ..
    } = caller
    {
        crate::session_chat_send::sender_mark::sign_agent_message(
            paths,
            server_id_here,
            project_id,
            session_id,
            params,
        );
    }
    Ok((serde_json::to_vec(&body).map_err(|_| NOT_A_MESSAGE)?, log))
}

/// A coordinator create for another computer: its first request is labelled as the user's, or
/// signed as the calling agent's, here before it leaves
/// (sender_mark.rs `label_coordinator_first_request`), since the other computer strips `via` from
/// a peer and holds no record of this computer's sessions.
///
/// CDXC:Security 2026-11-20 WHY (Fable's review of 4e4dbf7c6): `agents send --as-user
/// --main-coordinator --computer <name> --if-none create` lost its label and log entry, and an
/// agent's create went there unsigned.
fn marked_create_body(
    paths: &crate::paths::GxserverPaths,
    server_id_here: &str,
    target_server_id: &str,
    caller: &Principal,
    bytes: &[u8],
) -> Result<(Vec<u8>, Option<AsUserLog>), &'static str> {
    let mut body = forwarded_body(bytes)?;
    let params = body
        .get_mut("params")
        .and_then(|params| params.as_object_mut())
        .ok_or(NOT_A_MESSAGE)?;
    let log = crate::session_chat_send::sender_mark::label_coordinator_first_request(
        paths,
        server_id_here,
        target_server_id,
        caller,
        params,
    );
    Ok((serde_json::to_vec(&body).map_err(|_| NOT_A_MESSAGE)?, log))
}

/// Names the calling session as the sender (`senderProjectId`, `senderSessionId`) and drops any
/// the request carried: the other computer takes a held note's sender from these
/// (session_chat_send/with_next.rs `HeldSender`), so only the relay may say them.
///
/// CDXC:Security 2026-11-20 WHY (Fable's re-review of #677): an agent here could claim another
/// session here and withdraw its held notes there, or have its "Not delivered" go to that session.
fn stamp_sender(caller: &Principal, params: &mut serde_json::Map<String, serde_json::Value>) {
    params.remove("senderProjectId");
    params.remove("senderSessionId");
    if let Principal::Session {
        project_id,
        session_id,
        ..
    } = caller
    {
        params.insert("senderProjectId".to_string(), json!(project_id));
        params.insert("senderSessionId".to_string(), json!(session_id));
    }
}

/// [`stamp_sender`] on a request that is not a message (`/api/heldWithNext`).
fn stamped_sender(caller: &Principal, bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    const NOT_A_REQUEST: &str = "A request must be a JSON object with an object `params`.";
    let mut body: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| NOT_A_REQUEST)?;
    let params = body
        .get_mut("params")
        .and_then(|params| params.as_object_mut())
        .ok_or(NOT_A_REQUEST)?;
    stamp_sender(caller, params);
    serde_json::to_vec(&body).map_err(|_| NOT_A_REQUEST)
}

pub(crate) fn refusal(
    status: StatusCode,
    error: &str,
    message: String,
    request_id: String,
) -> RoutedResponse {
    routed_json(
        Some(RELAY_ROUTE_PREFIX.to_string()),
        status,
        rpc_error(error, message, Some(request_id)),
    )
}

/// A refusal for a token another computer holds, written for the agent on that computer.
pub(crate) fn peer_refusal(error: &str, message: String, request_id: String) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: json_response(
            StatusCode::FORBIDDEN,
            json!({
                "ok": false,
                "error": error,
                "message": message,
                "requestId": request_id,
            }),
        ),
    }
}
