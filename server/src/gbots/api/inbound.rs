//! The GBot API's own routes, answered before gxserver's token gate (route_http/dispatch.rs) and
//! opened only by the GBot's own key (`Authorization: Bearer gxb_…`):
//!
//! | Route | Does |
//! | --- | --- |
//! | `POST /hooks/gbots/bots/<botId>/messages` `{message, chat?: "new", wait?: seconds}` | Sends the message; `200 replied` with the reply, or `202 working` / `202 needsApproval` with a `poll` address |
//! | `GET /hooks/gbots/bots/<botId>/messages/<messageId>?wait=<seconds>` | The same answer for a message sent before |
//! | `POST /hooks/gbots/bots/<botId>/routines/<routineId>/run` | Starts one of the GBot's routines now (a webhook); nothing the caller sends reaches the GBot |
//!
//! Refusals: 400 (no message), 401 (no key, a wrong, revoked or other GBot's key), 404, 409
//! (GBots off, the GBot paused, its chat still on the key's earlier message), 410 (the chat was
//! closed), 413 (over 48,000 characters), 429 (more than 10 calls a minute with one key).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! Coordinator brief (M11), on the user's behalf: decide "how the message reaches gxserver from outside" and "never open a new port or loosen Easy Connect". gxserver listens on 127.0.0.1 only, so the GBot API reaches it from apps and scripts on this computer, and from devices that already reach this computer over SSH (a paired phone through Easy Connect or Tailscale, another computer with its own tunnel); nothing else. The key opens these routes for its own GBot and nothing else in gxserver: it is checked here, never by the token gate, so it can never pass for any other caller.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{HeaderMap, Method, StatusCode};
use serde_json::{json, Value};

use super::chat::{send, SendRefusal, MESSAGE_MAX_CHARS};
use super::keys::{key_for, touch, BotKey};
use super::reply::{read_message, reply_state, ApiMessage, ReplyState};
use crate::domain::DomainRepository;
use crate::gbots::record::{read_bot, Bot};
use crate::server::{routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

/// Every GBot API route starts with this.
pub(crate) const ROUTE_PREFIX: &str = "/hooks/gbots/";
pub(crate) const BOTS_ROUTE_PREFIX: &str = "/hooks/gbots/bots/";
/// Calls one key may make in a minute.
const CALLS_PER_MINUTE: usize = 10;
/// The longest an app may wait for a reply in one call.
const WAIT_MAX_SECS: u64 = 300;
/// How long a message call waits when the app doesn't say.
const WAIT_DEFAULT_SECS: u64 = 60;
/// A body bigger than this is refused before it is read (a 48,000-character message in UTF-8
/// and its JSON fit well within it).
const BODY_MAX_BYTES: usize = 256 * 1024;
const POLL_EVERY: Duration = Duration::from_millis(1500);

enum Route {
    Send,
    Poll(String),
    RunRoutine(String),
}

fn parse(method: &Method, path: &str) -> Option<(String, Route)> {
    let rest = path.strip_prefix(BOTS_ROUTE_PREFIX)?;
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    match (method, parts.as_slice()) {
        (&Method::POST, [bot, "messages"]) => Some((bot.to_string(), Route::Send)),
        (&Method::GET, [bot, "messages", message]) => {
            Some((bot.to_string(), Route::Poll(message.to_string())))
        }
        (&Method::POST, [bot, "routines", routine, "run"]) => {
            Some((bot.to_string(), Route::RunRoutine(routine.to_string())))
        }
        _ => None,
    }
}

fn answer(status: StatusCode, body: Value) -> RoutedResponse {
    routed_json(Some(ROUTE_PREFIX.to_string()), status, body)
}

fn refuse(status: StatusCode, error: &str, message: impl Into<String>) -> RoutedResponse {
    answer(
        status,
        json!({ "ok": false, "error": error, "message": message.into() }),
    )
}

fn unauthorized() -> RoutedResponse {
    refuse(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Send this GBot's API key as `Authorization: Bearer gxb_…`. Its key is in Ghostex, GBot Settings > Bot API; a key stops working once its GBot gets a new one or its API is turned off.",
    )
}

/// Calls each key made in the last minute.
fn calls() -> &'static Mutex<HashMap<String, VecDeque<Instant>>> {
    static CALLS: OnceLock<Mutex<HashMap<String, VecDeque<Instant>>>> = OnceLock::new();
    CALLS.get_or_init(Default::default)
}

/// Counts a call; `Some(seconds to wait)` once the key made its 10 calls this minute.
fn rate_limited(key_id: &str) -> Option<u64> {
    let mut calls = calls()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let window = Duration::from_secs(60);
    let made = calls.entry(key_id.to_string()).or_default();
    while made
        .front()
        .is_some_and(|at| now.duration_since(*at) >= window)
    {
        made.pop_front();
    }
    if made.len() >= CALLS_PER_MINUTE {
        let oldest = *made.front()?;
        return Some(
            window
                .saturating_sub(now.duration_since(oldest))
                .as_secs()
                .max(1),
        );
    }
    made.push_back(now);
    None
}

fn query_wait(query: Option<&str>) -> Option<u64> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("wait="))
        .and_then(|value| value.parse().ok())
}

fn poll_path(bot_id: &str, message_id: &str) -> String {
    format!("{BOTS_ROUTE_PREFIX}{bot_id}/messages/{message_id}")
}

/// The key and its GBot, or the refusal (BLOCKING).
fn authorize(
    state: &AppState,
    bot_id: &str,
    presented: Option<&str>,
) -> Result<(BotKey, Bot), RoutedResponse> {
    let presented = presented.ok_or_else(unauthorized)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internalError",
            format!("Ghostex could not read its state: {error}"),
        )
    })?;
    let key = key_for(&db, presented)
        .ok()
        .flatten()
        .filter(|key| key.bot_id == bot_id)
        .ok_or_else(unauthorized)?;
    let bot = read_bot(&db, bot_id)
        .ok()
        .flatten()
        .ok_or_else(unauthorized)?;
    if !crate::gbots::gbots_on(&state.paths) {
        return Err(refuse(
            StatusCode::CONFLICT,
            "turnedOff",
            "GBots are turned off on this computer.",
        ));
    }
    Ok((key, bot))
}

pub(crate) async fn serve(
    state: Arc<AppState>,
    method: Method,
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    body: Body,
) -> RoutedResponse {
    let Some((bot_id, route)) = parse(&method, &path) else {
        return refuse(
            StatusCode::NOT_FOUND,
            "notFound",
            "The GBot API has POST /hooks/gbots/bots/<bot>/messages, GET /hooks/gbots/bots/<bot>/messages/<message> and POST /hooks/gbots/bots/<bot>/routines/<routine>/run.",
        );
    };
    let presented = crate::auth::bearer_token(&headers).map(str::to_string);
    let checked = {
        let state = state.clone();
        let bot_id = bot_id.clone();
        tokio::task::spawn_blocking(move || authorize(&state, &bot_id, presented.as_deref())).await
    };
    let (key, bot) = match checked {
        Ok(Ok(found)) => found,
        Ok(Err(refusal)) => return refusal,
        Err(_) => {
            return refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internalError",
                "Ghostex could not check the key.",
            )
        }
    };
    if let Some(seconds) = rate_limited(&key.key_id) {
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "rateLimited",
            format!("This key made {CALLS_PER_MINUTE} calls in the last minute. Try again in {seconds} seconds."),
        );
    }
    match route {
        Route::Send => send_message(state, key, bot, query, body).await,
        Route::Poll(message_id) => poll_message(state, key, bot, message_id, query).await,
        Route::RunRoutine(routine) => run_routine(state, key, bot, routine).await,
    }
}

async fn send_message(
    state: Arc<AppState>,
    key: BotKey,
    bot: Bot,
    query: Option<String>,
    body: Body,
) -> RoutedResponse {
    let bytes = match axum::body::to_bytes(body, BODY_MAX_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return refuse(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payloadTooLarge",
                format!("A message is at most {MESSAGE_MAX_CHARS} characters."),
            )
        }
    };
    let Ok(request) = serde_json::from_slice::<Value>(&bytes) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "Send JSON: {\"message\": \"…\"}.",
        );
    };
    let text = request["message"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if text.is_empty() {
        return refuse(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "Send the message as {\"message\": \"…\"}.",
        );
    }
    if text.chars().count() > MESSAGE_MAX_CHARS {
        return refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payloadTooLarge",
            format!("A message is at most {MESSAGE_MAX_CHARS} characters."),
        );
    }
    if bot.paused_at.is_some() {
        return refuse(
            StatusCode::CONFLICT,
            "paused",
            format!("{} is paused. The user resumes it in Ghostex.", bot.name),
        );
    }
    let wait = request["wait"]
        .as_u64()
        .or_else(|| query_wait(query.as_deref()))
        .unwrap_or(WAIT_DEFAULT_SECS)
        .min(WAIT_MAX_SECS);
    let new_chat = request["chat"].as_str() == Some("new");
    let sent = {
        let state = state.clone();
        let (key, bot) = (key.clone(), bot.clone());
        tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&state.paths).map_err(|error| {
                SendRefusal::Failed(crate::domain::DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                })
            })?;
            let repository = DomainRepository::new(&db, &state.metadata.server_id);
            let sent = send(&state, &db, &repository, &bot, &key, &text, new_chat);
            if sent.is_ok() {
                touch(&db, &key.key_id);
            }
            sent
        })
        .await
    };
    let message = match sent {
        Ok(Ok(message)) => message,
        Ok(Err(SendRefusal::Busy(earlier))) => {
            return answer(
                StatusCode::CONFLICT,
                json!({
                    "ok": false,
                    "error": "busy",
                    "message": format!("{} is still on this key's last message. Wait for its reply, or send with \"chat\": \"new\".", bot.name),
                    "messageId": earlier,
                    "poll": poll_path(&bot.bot_id, &earlier),
                }),
            )
        }
        Ok(Err(SendRefusal::Limited(message))) => {
            return refuse(StatusCode::TOO_MANY_REQUESTS, "limited", message)
        }
        Ok(Err(SendRefusal::Failed(error))) => {
            return refuse(StatusCode::INTERNAL_SERVER_ERROR, error.code, error.message)
        }
        Err(_) => {
            return refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internalError",
                "Ghostex could not send the message.",
            )
        }
    };
    let state_now = await_reply(&state, &message, wait).await;
    message_answer(&state, &bot, &message, state_now)
}

async fn poll_message(
    state: Arc<AppState>,
    key: BotKey,
    bot: Bot,
    message_id: String,
    query: Option<String>,
) -> RoutedResponse {
    let read = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&state.paths).ok()?;
            read_message(&db, &message_id).ok().flatten()
        })
        .await
        .ok()
        .flatten()
    };
    let Some(message) = read.filter(|message| message.key_id == key.key_id) else {
        return refuse(
            StatusCode::NOT_FOUND,
            "notFound",
            "There is no such message for this key.",
        );
    };
    let wait = query_wait(query.as_deref()).unwrap_or(0).min(WAIT_MAX_SECS);
    let state_now = await_reply(&state, &message, wait).await;
    message_answer(&state, &bot, &message, state_now)
}

/// Waits up to `wait` seconds while the GBot works on `message`.
async fn await_reply(state: &Arc<AppState>, message: &ApiMessage, wait: u64) -> ReplyState {
    let deadline = Instant::now() + Duration::from_secs(wait);
    loop {
        let current = {
            let state = state.clone();
            let message = message.clone();
            tokio::task::spawn_blocking(move || {
                let db = open_gxserver_database(&state.paths).ok()?;
                let repository = DomainRepository::new(&db, &state.metadata.server_id);
                Some(reply_state(&db, &repository, &message))
            })
            .await
            .ok()
            .flatten()
            .unwrap_or(ReplyState::Working)
        };
        if current != ReplyState::Working || Instant::now() + POLL_EVERY > deadline {
            return current;
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
}

fn message_answer(
    state: &AppState,
    bot: &Bot,
    message: &ApiMessage,
    reply: ReplyState,
) -> RoutedResponse {
    let chat = crate::ids::create_global_session_ref(
        &state.metadata.server_id,
        &message.project_id,
        &message.session_id,
    );
    let poll = poll_path(&bot.bot_id, &message.message_id);
    match reply {
        ReplyState::Replied(text) => answer(
            StatusCode::OK,
            json!({ "ok": true, "status": "replied", "messageId": message.message_id, "reply": text, "chat": chat }),
        ),
        ReplyState::Working => answer(
            StatusCode::ACCEPTED,
            json!({
                "ok": true,
                "status": "working",
                "messageId": message.message_id,
                "poll": poll,
                "message": format!("{} is still on it. GET the poll address (add ?wait=60 to wait for it).", bot.name),
                "chat": chat,
            }),
        ),
        ReplyState::NeedsApproval(waiting_on) => answer(
            StatusCode::ACCEPTED,
            json!({
                "ok": true,
                "status": "needsApproval",
                "messageId": message.message_id,
                "poll": poll,
                "waitingOn": waiting_on,
                "message": format!("{} waits for the user to answer in Ghostex. Poll again later.", bot.name),
                "chat": chat,
            }),
        ),
        ReplyState::Gone => refuse(
            StatusCode::GONE,
            "chatClosed",
            format!(
                "{}'s chat for this message was closed before it replied. Send the message again.",
                bot.name
            ),
        ),
    }
}

async fn run_routine(
    state: Arc<AppState>,
    key: BotKey,
    bot: Bot,
    routine: String,
) -> RoutedResponse {
    let started = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
        let view = crate::automations::routine_view(&db, &routine)
            .ok()
            .flatten()
            .filter(|view| view["botId"].as_str() == Some(bot.bot_id.as_str()))
            .ok_or_else(|| format!("{} has no routine {routine}.", bot.name))?;
        if view["paused"] == true || bot.paused_at.is_some() {
            return Err(format!(
                "{}'s routine \"{}\" is paused.",
                bot.name,
                view["name"].as_str().unwrap_or_default()
            ));
        }
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let run = crate::automations::run_routine_now(
            &state.automation_runtime,
            &db,
            &repository,
            &routine,
        )
        .map_err(|error| error.message)?;
        touch(&db, &key.key_id);
        Ok(json!({ "ok": true, "status": "started", "routine": view["name"], "run": run }))
    })
    .await;
    match started {
        Ok(Ok(body)) => answer(StatusCode::ACCEPTED, body),
        Ok(Err(message)) => refuse(StatusCode::CONFLICT, "notStarted", message),
        Err(_) => refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internalError",
            "Ghostex could not start the routine.",
        ),
    }
}
