//! `/api/computers/<serverId>/relay/api/...`: an agent's request for another computer, forwarded
//! over the connection this computer already has to it.

use crate::session_chat_send::sender_mark::AsUserLog;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use serde_json::json;

use crate::{
    constants::GXSERVER_PROTOCOL_HEADER,
    logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel},
    protocol::rpc_error,
    security::Principal,
    server::{json_response, routed_json, AppState, RoutedResponse},
};

use super::registry;
use super::relay_watch::{self, CompletedRelayCall};

pub(crate) const RELAY_ROUTE_PREFIX: &str = "/api/computers/";
const RELAY_SEGMENT: &str = "/relay";
const RELAY_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
/// A relayed call that took this long is logged even when routine request logging is off.
const SLOW_RELAY_CALL_WARN_MS: u128 = 5_000;

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

/// The two requests about a relayed call rather than for the other computer (relay_watch.rs).
fn relay_control(path: &str) -> Option<(&str, &str)> {
    let (server_id, control) = path
        .strip_prefix(RELAY_ROUTE_PREFIX)?
        .split_once(RELAY_SEGMENT)?;
    (registry::is_server_id(server_id) && matches!(control, "/status" | "/watchStart"))
        .then_some((server_id, control))
}

/// One relayed call, for its log line and its entry in relay_watch.rs.
///
/// CDXC:RemoteMachines 2026-11-28 WHY:
/// GitHub #789: relayed calls left no trace of how long they took or where they waited. A call the CLI stopped waiting for wrote nothing at all, because the request's own log line (`apiRequest`, server/http_dispatch.rs) is written by a future that ends with the connection. Each call now writes a `computerRelay` line when the other computer's answer arrives, from the thread that waited for it, which outlives the caller: the endpoint, the computer by its server id, the time taken and the outcome. A call that never got an answer, or took [`SLOW_RELAY_CALL_WARN_MS`], is always kept; a quick one (answered or refused) is routine, since `apiRequest` already records a failed status.
#[derive(Clone)]
struct RelayCall {
    state: Arc<AppState>,
    method: Method,
    forwarded: String,
    server_id: String,
    request_id: String,
    started_at: Instant,
    /// A chat read that waits on the other computer on purpose (`--wait-ms`).
    long_poll: bool,
    /// Whether relay_watch.rs holds an entry for it under `request_id`.
    tracked: bool,
}

impl RelayCall {
    fn finish(
        &self,
        status: u16,
        outcome: &'static str,
        code: Option<&str>,
        error: Option<String>,
        created_session: Option<relay_watch::CreatedSessionInfo>,
    ) {
        let duration_ms = self.started_at.elapsed().as_millis();
        let slow = !self.long_poll && duration_ms >= SLOW_RELAY_CALL_WARN_MS;
        let (level, log_error) = match outcome {
            "ok" | "remoteError" | "refused" if slow => (LogLevel::Warn, None),
            "ok" | "remoteError" | "refused" => (LogLevel::Info, None),
            _ => (LogLevel::Error, code),
        };
        let _ = self.state.logger.log_routine(
            DiagnosticLogScenario::ApiRequests,
            GxserverLogInput {
                level,
                event: "computerRelay".to_string(),
                server_id: Some(self.state.metadata.server_id.clone()),
                request_id: Some(self.request_id.clone()),
                client: None,
                duration_ms: Some(duration_ms),
                error: log_error.map(str::to_string),
                details: Some(json!({
                    "method": self.method.as_str(),
                    "route": self.forwarded,
                    "targetServerId": self.server_id,
                    "outcome": outcome,
                    "statusCode": status,
                    "errorCode": code,
                })),
            },
        );
        if self.tracked {
            relay_watch::complete(
                &self.state,
                &self.request_id,
                CompletedRelayCall {
                    status,
                    outcome,
                    error,
                    created_session,
                },
            );
        }
    }

    /// A refusal made here, before anything left for the other computer.
    fn refuse(&self, status: StatusCode, code: &str, message: String) -> RoutedResponse {
        self.finish(
            status.as_u16(),
            "refused",
            Some(code),
            Some(message.clone()),
            None,
        );
        refusal(status, code, message, self.request_id.clone())
    }

    /// The other computer answered, or the call to it failed: what the caller is told, by the
    /// same code as before (`computerUnreachable`, which clients hold a send on:
    /// gx-chat-core session/computer.rs `holds_refusal`, held_sends.rs), now saying which side.
    fn finish_forward(
        &self,
        name: &str,
        result: &Result<(u16, Option<String>, Vec<u8>), ureq::Error>,
    ) -> Option<String> {
        let forwarded = &self.forwarded;
        match result {
            Ok((status, _, body)) => {
                let error = relay_watch::remote_error_message(*status, body);
                self.finish(
                    *status,
                    if error.is_some() { "remoteError" } else { "ok" },
                    error.as_ref().map(|_| "remoteError"),
                    error,
                    relay_watch::created_session(forwarded, &self.server_id, *status, body),
                );
                None
            }
            Err(error) => {
                let timed_out = matches!(error, ureq::Error::Timeout(_));
                let (outcome, message) = if crate::http_agent::never_connected(error) {
                    (
                        "unreachable",
                        format!("{name} didn't answer. Ghostex reconnects it on its own, so try again in a moment."),
                    )
                } else if timed_out {
                    (
                        "computerTimeout",
                        format!("{name} took {forwarded} but did not finish it in time. It may still be working on it, so check there before trying again."),
                    )
                } else {
                    (
                        "connectionLost",
                        format!("The connection to {name} dropped before it answered {forwarded}. It may still have done it, so check there before trying again."),
                    )
                };
                self.finish(
                    StatusCode::BAD_GATEWAY.as_u16(),
                    outcome,
                    Some("computerUnreachable"),
                    Some(message.clone()),
                    None,
                );
                Some(message)
            }
        }
    }
}

/// Forwards the request. The caller has already checked it carries this gxserver's own token: a
/// token another computer was given never relays onward. The other computer is reached with the
/// token it gave this one, which its gxserver limits to the same allowlist.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn relay(
    state: Arc<AppState>,
    method: Method,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    body: Body,
    request_id: String,
    caller: &Principal,
) -> RoutedResponse {
    match relay_control(path) {
        Some((server_id, "/status")) => {
            return relay_watch::handle_status(&method, caller, server_id, query, request_id).await;
        }
        Some((server_id, _)) => {
            return relay_watch::handle_watch_start(
                state, &method, caller, server_id, body, request_id,
            )
            .await;
        }
        None => {}
    }
    let paths = &state.paths;
    let server_id_here = state.metadata.server_id.as_str();
    let endpoint_path = Some(RELAY_ROUTE_PREFIX.to_string());
    let Some((server_id, forwarded)) = parse_relay_path(path) else {
        return refusal(
            StatusCode::NOT_FOUND,
            "notFound",
            format!("{path} is not a relay path."),
            request_id,
        );
    };
    let mut call = RelayCall {
        state: state.clone(),
        method: method.clone(),
        forwarded: forwarded.to_string(),
        server_id: server_id.to_string(),
        request_id: request_id.clone(),
        started_at: Instant::now(),
        long_poll: false,
        tracked: false,
    };
    if !relay_forwards(forwarded) {
        return call.refuse(
            StatusCode::FORBIDDEN,
            "forbidden",
            format!("Agents can't use {forwarded} on another computer."),
        );
    }
    let Some(computer) = registry::find(server_id) else {
        return call.refuse(
            StatusCode::NOT_FOUND,
            "computerNotConnected",
            "That computer isn't connected to this one. Open Settings > Remote > Remote machines (`ghostex settings open --tab remote`) so the user can connect it, then try again.".to_string(),
        );
    };
    call.tracked = relay_watch::track(&request_id, caller, server_id, &computer.name, forwarded);
    if let Some(message) = computer.waiting_message() {
        return call.refuse(StatusCode::FORBIDDEN, "approvalPending", message);
    }
    if !computer.agent_access {
        return call.refuse(
            StatusCode::FORBIDDEN,
            "agentAccessOff",
            access_off_message(&computer),
        );
    }
    if method != Method::POST && !(method == Method::GET && forwarded == "/api/health/server") {
        return call.refuse(
            StatusCode::METHOD_NOT_ALLOWED,
            "methodNotAllowed",
            format!("{forwarded} requires POST."),
        );
    }
    let bytes = match to_bytes(body, RELAY_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return call.refuse(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payloadTooLarge",
                format!(
                    "A request to another computer can be at most {RELAY_BODY_LIMIT_BYTES} bytes."
                ),
            );
        }
    };
    // Whose thread a session is only this computer knows (thread_scope.rs).
    let refused = {
        let db = crate::storage::open_gxserver_database(paths).ok();
        super::thread_scope::refusal(db.as_deref(), caller, server_id, forwarded, &bytes)
    };
    if let Some(message) = refused {
        return call.refuse(StatusCode::FORBIDDEN, "forbidden", message.to_string());
    }
    call.long_poll = is_long_poll(forwarded, &bytes);
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
                return call.refuse(StatusCode::BAD_REQUEST, "badRequest", message.to_string());
            }
        }
    } else if crate::session_chat_send::sender_mark::MESSAGE_ROUTES.contains(&forwarded) {
        match marked_message_body_logged(paths, server_id_here, server_id, caller, &bytes) {
            Ok((marked, log)) => {
                as_user_log = log;
                marked.into()
            }
            Err(message) => {
                return call.refuse(StatusCode::BAD_REQUEST, "badRequest", message.to_string());
            }
        }
    } else if forwarded == "/api/createAgentSession" {
        match marked_create_body(paths, server_id_here, server_id, caller, &bytes) {
            Ok((marked, log)) => {
                as_user_log = log;
                marked.into()
            }
            Err(message) => {
                return call.refuse(StatusCode::BAD_REQUEST, "badRequest", message.to_string());
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
    let protocol = header_text(&headers, GXSERVER_PROTOCOL_HEADER);
    let content_type = header_text(&headers, header::CONTENT_TYPE.as_str());
    let url = format!("{}{forwarded}", computer.base_url);
    let token = computer.token().to_string();
    let name = computer.name.clone();
    let blocking_call = call.clone();
    let blocking_name = name.clone();
    // This thread waits for the other computer whether or not the caller still does, so the
    // call's log line and its watch (relay_watch.rs) are settled here, not in the request's
    // future, which ends when the caller hangs up.
    let outcome = tokio::task::spawn_blocking(move || {
        let call = blocking_call;
        // The grant goes only to a listener of this account: the tunnel's port, once freed, could
        // be someone else's (ghostex_paths::loopback_owner).
        if let Err(foreign) = computer.check_listener(&url) {
            let message = foreign.to_string();
            call.finish(
                StatusCode::BAD_GATEWAY.as_u16(),
                "foreignListener",
                Some(ghostex_paths::loopback_owner::FOREIGN_LISTENER_CODE),
                Some(message.clone()),
                None,
            );
            return Err(message);
        }
        if call.tracked {
            relay_watch::mark_forwarding(&call.request_id);
        }
        let result = forward(
            &call.method,
            &url,
            &token,
            protocol.as_deref(),
            content_type.as_deref(),
            &call.request_id,
            user_send,
            bytes.to_vec(),
        );
        let failure = call.finish_forward(&blocking_name, &result);
        Ok(result.map_err(|_| failure.unwrap_or_default()))
    })
    .await;
    match outcome {
        Ok(Err(foreign)) => refusal(
            StatusCode::BAD_GATEWAY,
            ghostex_paths::loopback_owner::FOREIGN_LISTENER_CODE,
            foreign,
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
        Ok(Ok(Err(message))) => refusal(
            StatusCode::BAD_GATEWAY,
            "computerUnreachable",
            message,
            request_id,
        ),
        // The forwarding thread itself failed, so nothing above settled the call.
        Err(_) => {
            let message = format!(
                "This computer's Ghostex failed while passing {forwarded} on to {name}. Try again in a moment."
            );
            call.finish(
                StatusCode::BAD_GATEWAY.as_u16(),
                "internalError",
                Some("computerUnreachable"),
                Some(message.clone()),
                None,
            );
            refusal(
                StatusCode::BAD_GATEWAY,
                "computerUnreachable",
                message,
                request_id,
            )
        }
    }
}

/// A chat read that waits on the other computer on purpose (`read-session-chat --wait-ms`), so a
/// long one is not logged as slow.
fn is_long_poll(forwarded: &str, body: &[u8]) -> bool {
    forwarded == "/api/readSessionChat"
        && body.len() <= 64 * 1024
        && serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|body| {
                body.pointer("/params/waitMs")
                    .and_then(|wait| wait.as_u64())
            })
            .is_some_and(|wait_ms| wait_ms > 0)
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

fn forward(
    method: &Method,
    url: &str,
    token: &str,
    protocol: Option<&str>,
    content_type: Option<&str>,
    request_id: &str,
    user_send: bool,
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
