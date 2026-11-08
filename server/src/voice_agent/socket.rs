//! `/api/voice`: the WebSocket every client opens for a voice conversation or a dictation. Binary
//! frames are 24 kHz mono PCM16 audio both ways; text frames are JSON control and events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ws::{
    rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade,
};
use axum::extract::State;
use axum::http::{HeaderMap, Response, StatusCode, Uri};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::conversation::FOCUS_NOT_FOUND;
use super::conversation::{ClientFrame, Conversation, Input};
use super::focus::FocusScope;
use super::provider::{self, ConnectError, Provider};
use super::{announcements, dictation, keys, prompt, registry, tools};
use crate::auth::{is_authorized_session_client_headers, is_authorized_session_client_token};
use crate::protocol::{protocol_mismatch_error, rpc_error};
use crate::server::{
    is_expected_protocol_version, json_response, query_value, read_protocol_version, request_id,
    AppState,
};

static NEXT_ATTACH: AtomicU64 = AtomicU64::new(1);

pub(crate) async fn handle_voice(
    State(state): State<Arc<AppState>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    uri: Uri,
) -> Response<Body> {
    let request_id = request_id(&headers);
    let Ok(ws) = ws else {
        return json_response(
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                "A voice WebSocket is required.",
                Some(request_id),
            ),
        );
    };
    let query_token = query_value(&uri, "authToken");
    let authorized = is_authorized_session_client_headers(
        &headers,
        &state.auth_token,
        &state.device_api_tokens,
        &state.security.session_tokens,
    ) || query_token.as_deref().is_some_and(|token| {
        is_authorized_session_client_token(
            token,
            &state.auth_token,
            &state.device_api_tokens,
            &state.security.session_tokens,
        )
    });
    if !authorized {
        return json_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(
                "unauthorized",
                "gxserver auth token is required for this endpoint.",
                Some(request_id),
            ),
        );
    }
    // The voice agent's tools act with this token, not the token file (`loopback::ToolCaller`).
    let caller_token = crate::auth::bearer_token(&headers)
        .map(str::to_string)
        .or_else(|| query_token.clone())
        .unwrap_or_default();
    let version = read_protocol_version(&headers, &uri, None);
    if !is_expected_protocol_version(version.as_ref()) {
        return json_response(
            StatusCode::UPGRADE_REQUIRED,
            protocol_mismatch_error(version, Some(request_id)),
        );
    }
    let mode = query_value(&uri, "mode").unwrap_or_else(|| "conversation".to_string());
    let conversation_id = query_value(&uri, "conversationId").filter(|id| !id.trim().is_empty());
    let provider = query_value(&uri, "provider").and_then(|id| Provider::from_id(&id));
    // `callMode=pushToTalk|regular` overrides `voiceAgentCallMode` for this call (`ghostex voice`).
    let push_to_talk = query_value(&uri, "callMode").and_then(|mode| match mode.as_str() {
        "pushToTalk" => Some(true),
        "regular" => Some(false),
        _ => None,
    });
    let focus = query_value(&uri, "focus")
        .map(|focus| focus.trim().to_string())
        .filter(|focus| !focus.is_empty());
    let client_remote = query_value(&uri, "clientRemote").as_deref() == Some("1");
    let language = query_value(&uri, "language")
        .map(|language| language.trim().to_string())
        .filter(|language| {
            !language.is_empty()
                && language.len() <= 16
                && language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    ws.on_upgrade(move |socket| async move {
        match mode.as_str() {
            "dictation" => dictation::run(&state, socket, language).await,
            // Refused before anything is billed, and a reattach too: the watcher is ending the
            // calls that were open when it was turned off (`feature.rs`).
            "conversation" if !super::feature::voice_agent_on(&state.paths) => {
                fail(
                    &mut { socket },
                    super::feature::TURNED_OFF,
                    &super::feature::voice_agent_off_message(),
                )
                .await
            }
            "conversation" => {
                let start = Start {
                    provider,
                    push_to_talk,
                    focus,
                    client_remote,
                };
                run_conversation_socket(state, socket, conversation_id, start, caller_token).await
            }
            _ => {
                fail(
                    &mut { socket },
                    "badRequest",
                    "mode must be conversation or dictation.",
                )
                .await
            }
        }
    })
}

/// Sends a fatal error and closes the socket.
pub(super) async fn fail(socket: &mut WebSocket, code: &str, message: &str) {
    let _ = socket
        .send(Message::Text(
            json!({"type": "error", "code": code, "message": message, "fatal": true})
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket
        .send(Message::Text(
            json!({"type": "ended", "reason": "error", "message": message})
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket.send(Message::Close(None)).await;
}

/// How a new conversation starts, from the socket's query.
struct Start {
    provider: Option<Provider>,
    push_to_talk: Option<bool>,
    /// `focus=<projectId>:<sessionId>`: a call about one session (`focus.rs`).
    focus: Option<String>,
    /// `clientRemote=1`: the client is not on this computer.
    client_remote: bool,
}

async fn run_conversation_socket(
    state: Arc<AppState>,
    mut socket: WebSocket,
    conversation_id: Option<String>,
    start: Start,
    caller_token: String,
) {
    let (tx, resumed) = match conversation_id {
        Some(id) => match registry::sender(&id) {
            Some(tx) => (tx, true),
            None => {
                fail(
                    &mut socket,
                    "notFound",
                    "That voice conversation has ended. Start a new one.",
                )
                .await;
                return;
            }
        },
        None => match start_conversation(&state, &mut socket, start).await {
            Some(tx) => (tx, false),
            None => return,
        },
    };
    let _ = tx.send(Input::Caller(caller_token));
    let attach_id = NEXT_ATTACH.fetch_add(1, Ordering::Relaxed);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientFrame>();
    if tx
        .send(Input::Attach {
            attach_id,
            out: out_tx,
            resumed,
        })
        .is_err()
    {
        fail(
            &mut socket,
            "notFound",
            "That voice conversation has ended. Start a new one.",
        )
        .await;
        return;
    }
    let (mut client_sink, mut client_stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            let message = match frame {
                ClientFrame::Json(value) => Message::Text(value.to_string().into()),
                ClientFrame::Audio(bytes) => Message::Binary(bytes),
                ClientFrame::Close => {
                    let _ = client_sink.send(Message::Close(None)).await;
                    break;
                }
            };
            if client_sink.send(message).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = client_stream.next().await {
        let input = match message {
            Message::Binary(bytes) => Input::Audio(bytes),
            Message::Text(text) => {
                let value: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                let field = |key: &str| {
                    value
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                match value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
                    "text" => Input::Text(field("text")),
                    "interrupt" => Input::Interrupt,
                    "playback" => Input::Playback {
                        item_id: field("itemId"),
                        played_ms: value.get("playedMs").and_then(Value::as_u64).unwrap_or(0),
                    },
                    "end" => Input::End,
                    "continueMemory" => Input::ContinueMemory,
                    "forgetMemory" => {
                        super::memory::forget(&state);
                        continue;
                    }
                    "voice" => Input::Voice(field("voice")),
                    "talkStart" => Input::TalkStart,
                    "talkEnd" => Input::TalkEnd,
                    "talkCancel" => Input::TalkCancel,
                    "mute" => match value.get("muted").and_then(Value::as_bool) {
                        Some(muted) => Input::Mute(muted),
                        None => Input::BadRequest("mute needs muted: true or false.".to_string()),
                    },
                    "focus" => match value.get("session") {
                        Some(Value::Null) => Input::Focus(None),
                        Some(Value::String(session))
                            if session.trim().split_once(':').is_some_and(|(project, id)| {
                                !project.is_empty() && !id.is_empty()
                            }) =>
                        {
                            Input::Focus(Some(session.trim().to_string()))
                        }
                        _ => Input::BadRequest(
                            "focus needs session: \"<projectId>:<sessionId>\" or null.".to_string(),
                        ),
                    },
                    "confirm" => match (value.get("id").and_then(Value::as_str), value.get("session")) {
                        (Some(id), Some(Value::String(session))) => Input::Confirm {
                            id: id.to_string(),
                            session: Some(session.trim().to_string()),
                        },
                        (Some(id), Some(Value::Null) | None) => Input::Confirm {
                            id: id.to_string(),
                            session: None,
                        },
                        _ => Input::BadRequest(
                            "confirm needs id, and session: \"<projectId>:<sessionId>\" for Stop or null for Cancel.".to_string(),
                        ),
                    },
                    "playbackRate" => match value.get("rate").and_then(Value::as_f64) {
                        Some(rate) if rate.is_finite() => Input::PlaybackRate(rate),
                        _ => continue,
                    },
                    _ => continue,
                }
            }
            Message::Close(_) => break,
            _ => continue,
        };
        if tx.send(input).is_err() {
            break;
        }
    }
    let _ = tx.send(Input::Detach { attach_id });
    // The conversation closes the writer when it ends; a detached one just stops writing here.
    writer.abort();
}

/// Opens the provider session and starts the conversation task, or tells the client why not.
async fn start_conversation(
    state: &Arc<AppState>,
    socket: &mut WebSocket,
    start: Start,
) -> Option<mpsc::UnboundedSender<Input>> {
    let Start {
        provider,
        push_to_talk,
        focus,
        client_remote,
    } = start;
    let provider = provider.unwrap_or_else(|| keys::selected_provider(&state.paths));
    let push_to_talk = push_to_talk.unwrap_or_else(|| keys::push_to_talk_calls(&state.paths));
    // A call about a session that is not open fails before anything is billed.
    let focus = match focus {
        None => None,
        Some(reference) => {
            let sessions = super::sessions::list_sessions_async(state)
                .await
                .unwrap_or_default();
            match FocusScope::from_sessions(&sessions, &reference) {
                Some(scope) => Some(scope),
                None => {
                    fail(socket, "focusNotFound", FOCUS_NOT_FOUND).await;
                    return None;
                }
            }
        }
    };
    let key = match keys::read_key_async(&state.paths, provider).await {
        Ok(Some(key)) => key,
        Ok(None) => {
            fail(socket, "noKey", &no_key_message(provider, client_remote)).await;
            return None;
        }
        Err(message) if client_remote => {
            let message = message.replace(
                "Settings > Voice",
                "Ghostex Settings > Voice on that computer",
            );
            fail(
                socket,
                "noKey",
                &format!("On the computer this session runs on: {message}"),
            )
            .await;
            return None;
        }
        Err(message) => {
            fail(socket, "noKey", &message).await;
            return None;
        }
    };
    let _ = socket
        .send(Message::Text(
            json!({"type": "state", "state": "connecting"})
                .to_string()
                .into(),
        ))
        .await;
    let mut upstream = match provider::connect_realtime(provider, &key).await {
        Ok(upstream) => upstream,
        Err(ConnectError::BadKey) => {
            fail(socket, "badKey", &bad_key_message(provider, client_remote)).await;
            return None;
        }
        Err(ConnectError::Refused(message)) => {
            super::warn_error(
                state,
                "voiceAgent.providerRefused",
                &message,
                json!({"provider": provider.id()}),
            );
            fail(socket, "providerRefused", &message).await;
            return None;
        }
        Err(ConnectError::Unavailable(message)) => {
            super::warn(
                state,
                "voiceAgent.providerUnavailable",
                json!({"provider": provider.id(), "message": message}),
            );
            fail(
                socket,
                "providerUnavailable",
                &format!("Couldn't reach {}: {message}", provider.label()),
            )
            .await;
            return None;
        }
    };
    let voice = keys::selected_voice(&state.paths, provider);
    let keyterms = keyterms(state).await;
    let update = provider.session_update(
        &voice,
        &prompt::instructions(focus.as_ref()),
        &tools::definitions(focus.as_ref()),
        &keyterms,
        push_to_talk,
    );
    if upstream
        .send(WsMessage::Text(update.to_string().into()))
        .await
        .is_err()
    {
        fail(
            socket,
            "providerUnavailable",
            &format!("{} closed the connection.", provider.label()),
        )
        .await;
        return None;
    }
    // Wait for the provider to accept the configuration, so a bad voice or model fails here.
    let configured = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(frame) = upstream.next().await {
            let Ok(WsMessage::Text(text)) = frame else {
                if frame.is_err() {
                    return Err(None);
                }
                continue;
            };
            let event: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
            match event.get("type").and_then(Value::as_str) {
                Some("session.updated") => return Ok(()),
                Some("error") => return Err(Some(event)),
                _ => {}
            }
        }
        Err(None)
    })
    .await;
    match configured {
        Ok(Ok(())) => {}
        Ok(Err(None)) => {
            super::warn(
                state,
                "voiceAgent.sessionRefused",
                json!({"provider": provider.id(), "eventType": "closed"}),
            );
            fail(
                socket,
                "providerError",
                &format!("{} closed the connection.", provider.label()),
            )
            .await;
            return None;
        }
        Ok(Err(Some(event))) => {
            let report =
                super::warn_provider_error(state, "voiceAgent.sessionRefused", provider, &event);
            fail(socket, "providerError", &report.message).await;
            return None;
        }
        Err(_) => {
            super::warn(
                state,
                "voiceAgent.sessionRefused",
                json!({"provider": provider.id(), "eventType": "timeout"}),
            );
            fail(
                socket,
                "providerUnavailable",
                &format!("{} did not answer within 10 seconds.", provider.label()),
            )
            .await;
            return None;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::unbounded_channel();
    registry::insert(&id, tx.clone(), provider);
    announcements::ensure_running(state);
    let conversation = Conversation::new(
        id,
        state.clone(),
        provider,
        voice,
        update,
        push_to_talk,
        focus,
        client_remote,
        upstream,
        tx.clone(),
        rx,
    );
    tokio::spawn(conversation.run());
    Some(tx)
}

/// CDXC:VoiceMode 2026-10-23 DECISION: User, asked "On the computer, what about sessions running on another computer you are connected to?", picked "Build it now: the call runs on that other computer instead (that computer needs its own voice key)". The desktop opens the call on that computer's gxserver with `clientRemote=1`, so a missing or refused key is that computer's, said in general words, and sessions are shown in the client's app, not on that computer's screen (the tool frame's `show`).
pub(super) fn no_key_message(provider: Provider, client_remote: bool) -> String {
    if client_remote {
        format!(
            "The computer this session runs on has no {} API key for the voice agent. Add it in Ghostex Settings > Voice on that computer.",
            provider.label()
        )
    } else {
        format!("Add your {} API key in Settings > Voice.", provider.label())
    }
}

pub(super) fn bad_key_message(provider: Provider, client_remote: bool) -> String {
    if client_remote {
        format!(
            "The {label} API key on the computer this session runs on was refused by {label}. Check it in Ghostex Settings > Voice on that computer.",
            label = provider.label()
        )
    } else {
        format!(
            "{} refused the API key. Check it in Settings > Voice.",
            provider.label()
        )
    }
}

/// Names the recognizer should expect, most useful first: Ghostex, the agents, then the user's
/// project names. Each provider keeps what its limit allows (`Provider::session_update`).
async fn keyterms(state: &Arc<AppState>) -> Vec<String> {
    let mut terms: Vec<String> = ["Ghostex", "Claude", "Codex", "Cursor", "OpenCode", "Gemini"]
        .iter()
        .map(|term| term.to_string())
        .collect();
    if let Ok(projects) = super::sessions::list_projects_async(state).await {
        for project in projects {
            let name = project.name.trim().to_string();
            if !name.is_empty() && !name.starts_with("Chat 20") && !terms.contains(&name) {
                terms.push(name);
            }
        }
    }
    terms
}
