//! Send request ids: a chat send that is retried reaches the agent once.

use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicI64, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_LENGTH, Response, StatusCode},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use tokio::sync::watch;

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

/// The RPC param (and result field) that names one send.
pub(crate) const SEND_REQUEST_ID_PARAM: &str = "sendRequestId";
const MAX_SEND_REQUEST_ID_CHARS: usize = 128;
/// How long, and how many per session, sends are remembered.
const RETENTION_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_ROWS_PER_SESSION: i64 = 500;
const PRUNE_EVERY_MS: i64 = 10 * 60 * 1000;
/// A repeated id waits this long for the attempt it repeats; a send with images waits up to 40s
/// for a starting agent before it types.
const IN_FLIGHT_WAIT: Duration = Duration::from_secs(120);
const RESPONSE_MAX_BYTES: usize = 16 * 1024 * 1024;

const STATE_SENDING: &str = "sending";
const STATE_SENT: &str = "sent";
const STATE_REFUSED: &str = "refused";

/// Refusal for an id whose attempt was cut off by a gxserver restart and that the agent's
/// transcript does not show: it may still be in the agent's hands, so it is never typed again.
pub(crate) const SEND_OUTCOME_UNKNOWN: &str = "sendOutcomeUnknown";

/// Response extension naming the send, for the send-failure log.
#[derive(Clone)]
pub(crate) struct SendRequestIdExtension(pub String);

/// One send: the session it goes to and the id its client (or gxserver) gave it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SendRequestKey {
    pub project_id: String,
    pub session_id: String,
    pub send_request_id: String,
}

impl SendRequestKey {
    pub(crate) fn new(project_id: &str, session_id: &str, send_request_id: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            send_request_id: send_request_id.to_string(),
        }
    }
}

/// What the ledger says about an id when a send with it arrives.
pub(crate) enum Begin {
    /// Nothing with this id reached the agent: send now, then settle the attempt.
    Send(Attempt),
    /// An earlier attempt went through; this is the result it answered.
    Sent(Value),
    /// The same id is being sent right now; the receiver turns true when it settles.
    InFlight(watch::Receiver<bool>),
    /// An attempt was being sent when gxserver stopped.
    Interrupted { since_ms: i64, needles: Vec<String> },
}

/// How an attempt ended.
pub(crate) enum Outcome {
    /// It went through; the result a repeated id answers with.
    Sent(Value),
    /// Refused before anything was submitted, so the same id may try again.
    Refused,
    /// Failed in a way that does not prove nothing was submitted: the row stays `sending`, and a
    /// repeated id is settled against the transcript like an attempt a restart cut off.
    Uncertain,
}

/// Error codes the send paths raise only before anything reaches the agent (or, for an agent that
/// kept the message in its input box, after the send cleared it or left it unsubmitted).
pub(crate) fn submitted_nothing(code: &str) -> bool {
    matches!(
        code,
        "invalidParams"
            | "badRequest"
            | "invalidState"
            | "notFound"
            | "composerNotReady"
            | "composerNotCleared"
            | "sendCancelled"
            | "accountRecoveryNotReady"
            | "agentBusy"
            | "dependencyUnavailable"
            | "sessionInputFailed"
            | "sessionStarting"
    )
}

/// A send the ledger let through. Settling records its outcome; dropping it unsettled (a panic)
/// leaves the row `sending`, which the next attempt treats as interrupted.
pub(crate) struct Attempt {
    key: SendRequestKey,
    paths: GxserverPaths,
    done: watch::Sender<bool>,
}

impl Attempt {
    /// Records how the attempt ended.
    pub(crate) fn settle(self, outcome: Outcome) {
        let result = match &outcome {
            Outcome::Sent(result) => Some(result),
            Outcome::Refused => None,
            Outcome::Uncertain => return,
        };
        if let Ok(db) = open_gxserver_database(&self.paths) {
            let _ = db.execute(
                r#"
                UPDATE session_chat_send_requests
                SET state = ?4, response = ?5, updatedAtMs = ?6
                WHERE projectId = ?1 AND sessionId = ?2 AND sendRequestId = ?3
                "#,
                params![
                    self.key.project_id,
                    self.key.session_id,
                    self.key.send_request_id,
                    if result.is_some() {
                        STATE_SENT
                    } else {
                        STATE_REFUSED
                    },
                    result.map(Value::to_string),
                    now_ms(),
                ],
            );
        }
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        in_flight_sends()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
        let _ = self.done.send(true);
    }
}

fn in_flight_sends() -> &'static Mutex<HashMap<SendRequestKey, watch::Receiver<bool>>> {
    static IN_FLIGHT: OnceLock<Mutex<HashMap<SendRequestKey, watch::Receiver<bool>>>> =
        OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Looks the id up and, when nothing with it reached the agent, claims it for a new attempt.
pub(crate) fn begin(
    paths: &GxserverPaths,
    key: &SendRequestKey,
    endpoint: &str,
    needles: &[String],
) -> Result<Begin, DomainStateError> {
    let mut in_flight = in_flight_sends()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(done) = in_flight.get(key) {
        return Ok(Begin::InFlight(done.clone()));
    }
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    prune(&db, key)?;
    let row = db
        .query_row(
            r#"
            SELECT state, response, needles, createdAtMs FROM session_chat_send_requests
            WHERE projectId = ?1 AND sessionId = ?2 AND sendRequestId = ?3
            "#,
            params![key.project_id, key.session_id, key.send_request_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    match row {
        Some((state, response, _, _)) if state == STATE_SENT => Ok(Begin::Sent(
            response
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_else(|| json!({})),
        )),
        Some((state, _, stored_needles, since_ms)) if state == STATE_SENDING => {
            Ok(Begin::Interrupted {
                since_ms,
                needles: serde_json::from_str(&stored_needles).unwrap_or_default(),
            })
        }
        _ => start_attempt(&db, &mut in_flight, paths, key, endpoint, needles).map(Begin::Send),
    }
}

/// Claims an id whose earlier attempt was interrupted, for a send the user asked for again
/// knowing it may have arrived (a queued prompt they retried after a restart).
pub(crate) fn begin_again(
    paths: &GxserverPaths,
    key: &SendRequestKey,
    endpoint: &str,
    needles: &[String],
) -> Result<Begin, DomainStateError> {
    let mut in_flight = in_flight_sends()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(done) = in_flight.get(key) {
        return Ok(Begin::InFlight(done.clone()));
    }
    let db = open_gxserver_database(paths).map_err(internal_error)?;
    start_attempt(&db, &mut in_flight, paths, key, endpoint, needles).map(Begin::Send)
}

fn start_attempt(
    db: &Connection,
    in_flight: &mut HashMap<SendRequestKey, watch::Receiver<bool>>,
    paths: &GxserverPaths,
    key: &SendRequestKey,
    endpoint: &str,
    needles: &[String],
) -> Result<Attempt, DomainStateError> {
    let now = now_ms();
    db.execute(
        r#"
        INSERT INTO session_chat_send_requests (
          projectId, sessionId, sendRequestId, endpoint, state, needles, response,
          createdAtMs, updatedAtMs
        )
        VALUES (?1, ?2, ?3, ?4, 'sending', ?5, NULL, ?6, ?6)
        ON CONFLICT (projectId, sessionId, sendRequestId) DO UPDATE SET
          endpoint = excluded.endpoint, state = 'sending', needles = excluded.needles,
          response = NULL, createdAtMs = excluded.createdAtMs, updatedAtMs = excluded.updatedAtMs
        "#,
        params![
            key.project_id,
            key.session_id,
            key.send_request_id,
            endpoint,
            json!(needles).to_string(),
            now,
        ],
    )
    .map_err(sql_error)?;
    let (done, waiting) = watch::channel(false);
    in_flight.insert(key.clone(), waiting);
    Ok(Attempt {
        key: key.clone(),
        paths: paths.clone(),
        done,
    })
}

/// Whether a send with this id already went out or is going out now: a caller retrying an
/// interrupting send uses it to skip the interrupt the first attempt already made.
pub(crate) fn was_attempted(paths: &GxserverPaths, key: &SendRequestKey) -> bool {
    if in_flight_sends()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(key)
    {
        return true;
    }
    open_gxserver_database(paths).is_ok_and(|db| {
        db.query_row(
            r#"
            SELECT state FROM session_chat_send_requests
            WHERE projectId = ?1 AND sessionId = ?2 AND sendRequestId = ?3
            "#,
            params![key.project_id, key.session_id, key.send_request_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .is_some_and(|state| state != STATE_REFUSED)
    })
}

/// When the attempt behind this id began (ms), for a replay that must be judged by the time of the
/// original send (coordinators/reopen.rs).
pub(crate) fn send_request_created_ms(paths: &GxserverPaths, key: &SendRequestKey) -> Option<i64> {
    let db = open_gxserver_database(paths).ok()?;
    db.query_row(
        r#"
        SELECT createdAtMs FROM session_chat_send_requests
        WHERE projectId = ?1 AND sessionId = ?2 AND sendRequestId = ?3
        "#,
        params![key.project_id, key.session_id, key.send_request_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// The result stored for a settled send, as a replay of it answers.
pub(crate) fn stored_send_result(
    paths: &GxserverPaths,
    key: &SendRequestKey,
) -> Option<Map<String, Value>> {
    let db = open_gxserver_database(paths).ok()?;
    let text: String = db
        .query_row(
            r#"
            SELECT response FROM session_chat_send_requests
            WHERE projectId = ?1 AND sessionId = ?2 AND sendRequestId = ?3 AND state = 'sent'
            "#,
            params![key.project_id, key.session_id, key.send_request_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten()?;
    serde_json::from_str::<Value>(&text)
        .ok()?
        .as_object()
        .cloned()
}

/// Settles an interrupted attempt the transcript proved arrived.
pub(crate) fn mark_sent(paths: &GxserverPaths, key: &SendRequestKey, result: &Value) {
    if let Ok(db) = open_gxserver_database(paths) {
        let _ = db.execute(
            r#"
            UPDATE session_chat_send_requests SET state = 'sent', response = ?4, updatedAtMs = ?5
            WHERE projectId = ?1 AND sessionId = ?2 AND sendRequestId = ?3
            "#,
            params![
                key.project_id,
                key.session_id,
                key.send_request_id,
                result.to_string(),
                now_ms()
            ],
        );
    }
}

/// Whether the session's transcript records a user turn carrying the text since the interrupted
/// attempt began. False when it cannot tell (no text to match, no transcript yet).
pub(crate) async fn interrupted_send_arrived(
    paths: &GxserverPaths,
    server_id: &str,
    key: &SendRequestKey,
    since_ms: i64,
    needles: &[String],
) -> bool {
    if needles.is_empty() {
        return false;
    }
    let paths = paths.clone();
    let server_id = server_id.to_string();
    let key = key.clone();
    let needles = needles.to_vec();
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).ok()?;
        let session = DomainRepository::new(&db, server_id.as_str())
            .get_session(&key.project_id, &key.session_id)
            .ok()??;
        crate::coordinators::transcript_records_message(&session, &needles, since_ms, &mut None)
    })
    .await
    .ok()
    .flatten()
        == Some(true)
}

/// CDXC:SessionChat 2026-10-05 WHY:
/// A retried send must never deliver the same message twice. Every send RPC (`sendSessionChatMessage`, `queueSessionChatPrompt`, `answerSessionChatPrompt`) carries a client-made `sendRequestId`, and gxserver keeps a persisted ledger of the ids it took per session (24 hours, newest 500). A repeated id returns the first attempt's result with `duplicate: true` instead of typing again; a repeat that arrives while the first attempt still runs waits for it; an attempt a refusal ended (nothing was submitted) may run again under the same id. The attempt runs in its own task, so a client that drops the connection mid-send cannot cut it off and leave its retry guessing. An attempt a gxserver restart cut off is settled against the agent's transcript: shown means sent, otherwise the retry is refused with `sendOutcomeUnknown` rather than typed again, because the agent may still have it. A client that sends no id (older desktop, phone or CLI builds) gets one generated here, which names the send in the result and the failure log but cannot recognise that client's own retries.
pub(crate) async fn send_once<F, Fut>(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    send: F,
) -> RoutedResponse
where
    F: FnOnce(AppState, String, String, Value) -> Fut + Send + 'static,
    Fut: Future<Output = RoutedResponse> + Send + 'static,
{
    let Ok(params) = read_domain_rpc_params(body) else {
        return send(state.clone(), endpoint_path, request_id, body.clone()).await;
    };
    let project_id = trimmed(&params, "projectId");
    let session_id = trimmed(&params, "sessionId");
    if project_id.is_empty() || session_id.is_empty() {
        return send(state.clone(), endpoint_path, request_id, body.clone()).await;
    }
    let send_request_id = match read_send_request_id(&params) {
        Ok(id) => id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let mut body = body.clone();
    body["params"][SEND_REQUEST_ID_PARAM] = json!(send_request_id);
    let key = SendRequestKey::new(&project_id, &session_id, &send_request_id);
    let needles = params
        .get("text")
        .and_then(Value::as_str)
        .map(crate::coordinators::delivery_needles)
        .unwrap_or_default();
    loop {
        match begin(&state.paths, &key, &endpoint_path, &needles) {
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
            Ok(Begin::Send(attempt)) => {
                let task_state = state.clone();
                let task_endpoint = endpoint_path.clone();
                let task_request_id = request_id.clone();
                let id = send_request_id.clone();
                let attempt_task = tokio::spawn(async move {
                    let response = send(task_state, task_endpoint, task_request_id, body).await;
                    let (response, outcome) = name_send_in_response(response, &id).await;
                    attempt.settle(outcome);
                    response
                });
                return match attempt_task.await {
                    Ok(response) => response,
                    Err(_) => with_send_request_id(
                        domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: "internalError",
                                message: "The send stopped unexpectedly. Read the chat before sending it again.".to_string(),
                            },
                        ),
                        &send_request_id,
                    ),
                };
            }
            Ok(Begin::Sent(result)) => {
                return replay(endpoint_path, request_id, result, &send_request_id)
            }
            Ok(Begin::InFlight(mut done)) => {
                if tokio::time::timeout(IN_FLIGHT_WAIT, done.wait_for(|done| *done))
                    .await
                    .is_err()
                {
                    return with_send_request_id(
                        domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: "sendInProgress",
                                message: "This message is still being sent. Read the chat before sending it again.".to_string(),
                            },
                        ),
                        &send_request_id,
                    );
                }
            }
            Ok(Begin::Interrupted { since_ms, needles }) => {
                if interrupted_send_arrived(
                    &state.paths,
                    state.metadata.server_id.as_str(),
                    &key,
                    since_ms,
                    &needles,
                )
                .await
                {
                    let result = json!({ "queued": true, "recoveredAfterRestart": true });
                    mark_sent(&state.paths, &key, &result);
                    return replay(endpoint_path, request_id, result, &send_request_id);
                }
                return with_send_request_id(
                    domain_error_response(
                        endpoint_path,
                        request_id,
                        DomainStateError {
                            code: SEND_OUTCOME_UNKNOWN,
                            message: "gxserver restarted while this message was being sent, and the agent's chat does not show it yet. Read the chat; if it never arrived, send it again as a new message.".to_string(),
                        },
                    ),
                    &send_request_id,
                );
            }
        }
    }
}

/// The client's `sendRequestId`, if it sent one.
pub(crate) fn read_send_request_id(
    params: &Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    match params.get(SEND_REQUEST_ID_PARAM) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id))
            if !id.trim().is_empty() && id.trim().chars().count() <= MAX_SEND_REQUEST_ID_CHARS =>
        {
            Ok(Some(id.trim().to_string()))
        }
        Some(_) => Err(DomainStateError {
            code: "invalidParams",
            message: format!(
                "sendRequestId must be a string of 1 to {MAX_SEND_REQUEST_ID_CHARS} characters."
            ),
        }),
    }
}

/// Puts the id on the response (in `result` on success, beside `error` on a refusal) and
/// returns how the attempt ended.
async fn name_send_in_response(
    routed: RoutedResponse,
    send_request_id: &str,
) -> (RoutedResponse, Outcome) {
    let RoutedResponse {
        endpoint_path,
        response,
    } = routed;
    let (mut parts, body) = response.into_parts();
    let succeeded = parts.status.is_success();
    let bytes = to_bytes(body, RESPONSE_MAX_BYTES).await.unwrap_or_default();
    let mut envelope: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let mut result = None;
    let mut refused = false;
    if let Some(object) = envelope.as_object_mut() {
        if succeeded {
            if let Some(Value::Object(fields)) = object.get_mut("result") {
                fields.insert(SEND_REQUEST_ID_PARAM.to_string(), json!(send_request_id));
            }
            result = object.get("result").cloned();
        } else {
            refused = object
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(submitted_nothing);
            object.insert(SEND_REQUEST_ID_PARAM.to_string(), json!(send_request_id));
        }
    }
    let bytes = if envelope.is_null() {
        bytes.to_vec()
    } else {
        envelope.to_string().into_bytes()
    };
    parts.headers.remove(CONTENT_LENGTH);
    parts
        .extensions
        .insert(SendRequestIdExtension(send_request_id.to_string()));
    (
        RoutedResponse {
            endpoint_path,
            response: Response::from_parts(parts, Body::from(bytes)),
        },
        if succeeded {
            // A success without a result object still counts as sent.
            Outcome::Sent(result.unwrap_or_else(|| json!({})))
        } else if refused {
            Outcome::Refused
        } else {
            Outcome::Uncertain
        },
    )
}

fn replay(
    endpoint_path: String,
    request_id: String,
    mut result: Value,
    send_request_id: &str,
) -> RoutedResponse {
    if let Some(fields) = result.as_object_mut() {
        fields.insert(SEND_REQUEST_ID_PARAM.to_string(), json!(send_request_id));
        fields.insert("duplicate".to_string(), json!(true));
    }
    let mut response = routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    );
    response
        .response
        .extensions_mut()
        .insert(SendRequestIdExtension(send_request_id.to_string()));
    response
}

fn with_send_request_id(mut response: RoutedResponse, send_request_id: &str) -> RoutedResponse {
    response
        .response
        .extensions_mut()
        .insert(SendRequestIdExtension(send_request_id.to_string()));
    response
}

fn prune(db: &Connection, key: &SendRequestKey) -> Result<(), DomainStateError> {
    static LAST_PRUNED_MS: AtomicI64 = AtomicI64::new(0);
    let now = now_ms();
    if now - LAST_PRUNED_MS.load(Ordering::Relaxed) >= PRUNE_EVERY_MS {
        LAST_PRUNED_MS.store(now, Ordering::Relaxed);
        db.execute(
            "DELETE FROM session_chat_send_requests WHERE updatedAtMs < ?1",
            params![now - RETENTION_MS],
        )
        .map_err(sql_error)?;
    }
    db.execute(
        r#"
        DELETE FROM session_chat_send_requests
        WHERE projectId = ?1 AND sessionId = ?2 AND state <> 'sending'
          AND sendRequestId NOT IN (
            SELECT sendRequestId FROM session_chat_send_requests
            WHERE projectId = ?1 AND sessionId = ?2
            ORDER BY updatedAtMs DESC LIMIT ?3
          )
        "#,
        params![key.project_id, key.session_id, MAX_ROWS_PER_SESSION],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn trimmed(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn internal_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    internal_error(format!("SQLite gxserver state error: {error}"))
}
