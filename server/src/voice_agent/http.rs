//! `/api/readVoiceAgent`, `/api/setVoiceAgentKey` and `/api/forgetVoiceAgentMemory`: what Settings
//! and the clients need to know about the voice agent, saving a provider's key, and forgetting the
//! remembered last conversation. A key is never sent back.

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::keys::{self, KeyCheck, KeyState};
use super::memory;
use super::provider::Provider;
use super::registry;
use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

fn respond(
    endpoint_path: String,
    request_id: String,
    result: Result<Value, DomainStateError>,
) -> RoutedResponse {
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) async fn handle_read_voice_agent_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
) -> RoutedResponse {
    let paths = &state.paths;
    let provider = keys::selected_provider(paths);
    let voices = |provider: Provider| json!(provider.voices());
    // `configured` means usable, so a client that knows only it treats an unreadable key as none.
    let key_view = |provider: Provider| {
        let state = keys::key_state(paths, provider);
        json!({
            "configured": state == KeyState::Saved,
            "unreadable": state == KeyState::Unreadable,
            "reference": keys::key_reference(paths, provider).map(|reference| reference.reference),
        })
    };
    let result = json!({
        "provider": provider.id(),
        "voices": {
            "xai": keys::selected_voice(paths, Provider::Xai),
            "openai": keys::selected_voice(paths, Provider::OpenAi),
        },
        "voiceOptions": {"xai": voices(Provider::Xai), "openai": voices(Provider::OpenAi)},
        "keys": {"xai": key_view(Provider::Xai), "openai": key_view(Provider::OpenAi)},
        "secretsSealedWith": crate::vault::vault_backend(paths),
        // Off on Settings > Extensions: `/api/voice` refuses conversations (`feature.rs`); the
        // keys stay readable because Dictate uses the xAI or OpenAI key.
        "enabled": super::feature::voice_agent_on(paths),
        "conversations": registry::summary(),
        // Calls about one session (`focus` on `/api/voice`) are understood here.
        "focusedCalls": true,
        // Calls with a GBot (`gbot=` or `focus=gbot:<botId>`, `gbot_call/`) are understood here.
        "gbotCalls": true,
        // `/api/voiceAgentClip` takes a recorded turn for a call whose client can't stream (`clip.rs`).
        "clipTurns": true,
        "idleHangupMs": super::conversation::IDLE_HANGUP.as_millis() as u64,
        "speed": keys::selected_speed(paths),
        "callMode": if keys::push_to_talk_calls(paths) { "pushToTalk" } else { "regular" },
        // Saving the call mode switches every running call at once (`callMode` frames, as `ready`'s
        // `callModeLive` says), so Settings can say so before any call.
        "callModeLive": true,
        "speedOptions": keys::SPEEDS,
        "memory": memory::client_view(paths),
        "memorySummaryPending": memory::summary_pending(paths),
    });
    respond(endpoint_path, request_id, Ok(result))
}

pub(crate) async fn handle_forget_voice_agent_memory_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
) -> RoutedResponse {
    memory::forget(state);
    respond(endpoint_path, request_id, Ok(json!({"memory": null})))
}

pub(crate) async fn handle_set_voice_agent_key_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = set_key(state, body).await;
    respond(endpoint_path, request_id, result)
}

async fn set_key(state: &AppState, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let provider = params
        .get("provider")
        .and_then(Value::as_str)
        .and_then(Provider::from_id)
        .ok_or_else(|| failure("invalidParams", "provider must be xai or openai."))?;
    let key = params
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let paths = state.paths.clone();
    if key.is_empty() {
        tokio::task::spawn_blocking(move || keys::write_key(&paths, provider, None))
            .await
            .map_err(|error| failure("internalError", error.to_string()))?
            .map_err(|message| failure("internalError", message))?;
        return Ok(json!({"provider": provider.id(), "configured": false}));
    }
    // An `op://…` reference is kept as the reference; the key is read through 1Password, checked
    // with the provider once here, and read again each time it is used.
    if let Some(reference) = crate::secret_providers::typed_reference(&state.paths, &key) {
        let reference = reference.map_err(|message| failure("invalidParams", message))?;
        return tokio::task::spawn_blocking(move || {
            let read = crate::secret_providers::resolve(&paths, &reference)
                .map_err(|message| failure("badKey", message))?;
            match keys::check_key(provider, read.trim()) {
                KeyCheck::Valid => {}
                KeyCheck::Refused => {
                    return Err(failure(
                        "badKey",
                        format!("{} refused the key 1Password gave.", provider.label()),
                    ))
                }
                KeyCheck::Unreachable(message) => {
                    return Err(failure("providerUnavailable", message))
                }
            }
            keys::write_key_reference(&paths, provider, &reference)
                .map_err(|message| failure("internalError", message))
        })
        .await
        .map_err(|error| failure("internalError", error.to_string()))?
        .map(|()| json!({"provider": provider.id(), "configured": true, "reference": true}));
    }
    if key.len() > 512 || key.chars().any(char::is_whitespace) {
        return Err(failure(
            "invalidParams",
            "That does not look like an API key.",
        ));
    }
    tokio::task::spawn_blocking(move || {
        match keys::check_key(provider, &key) {
            KeyCheck::Valid => {}
            KeyCheck::Refused => {
                return Err(failure(
                    "badKey",
                    format!("{} refused that API key.", provider.label()),
                ))
            }
            KeyCheck::Unreachable(message) => return Err(failure("providerUnavailable", message)),
        }
        keys::write_key(&paths, provider, Some(&key))
            .map_err(|message| failure("internalError", message))
    })
    .await
    .map_err(|error| failure("internalError", error.to_string()))??;
    Ok(json!({"provider": provider.id(), "configured": true}))
}
