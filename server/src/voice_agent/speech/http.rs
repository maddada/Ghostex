//! Dictate's settings API, for the desktop's Dictation page and the phone's Settings › Voice
//! (contract: issues-1027/speech-to-text/api-contract.md):
//!
//! - `/api/readDictateSettings`, `/api/setDictateSettings` `{patch}`
//! - `/api/readDictateEngine`: what will transcribe, for the mic's tooltip
//! - `hex` in `/api/readDictateSettings`: where the user stands with Hex 3 (`hex.rs`)
//! - `oldModels` in `/api/readDictateSettings` and `/api/deleteOldSpeechModels`: the models our
//!   retired local engine downloaded (`retired.rs`)
//! - `/api/skipDictationPolish` `{id}`

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::{cleanup, dictate, hex, languages, retired, settings};
use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::voice_agent::keys::{self, KeyState};
use crate::voice_agent::provider::Provider;

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

/// Every route listed above.
pub(crate) async fn handle_dictate_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = route(state, &endpoint_path, body).await;
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, DomainStateError> + Send + 'static,
) -> Result<T, DomainStateError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| failure("internalError", error.to_string()))?
}

async fn route(state: &AppState, path: &str, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let paths = state.paths.clone();
    match path {
        "/api/readDictateSettings" => read_all(state).await,
        "/api/setDictateSettings" => {
            let patch = params
                .get("patch")
                .and_then(Value::as_object)
                .cloned()
                .ok_or_else(|| failure("invalidParams", "Send the changes as patch."))?;
            {
                let paths = paths.clone();
                blocking(move || {
                    settings::update(&paths, &patch, || dictate::saved_keys(&paths))
                        .map(|_| ())
                        .map_err(|message| failure("badRequest", message))
                })
                .await?;
            }
            read_all(state).await
        }
        "/api/readDictateEngine" => {
            let (settings, hex_state) = blocking(move || {
                let settings = dictate::current_settings(&paths);
                let hex_state = dictate::hex_state_for(&paths, &settings);
                Ok((settings, hex_state))
            })
            .await?;
            Ok(dictate::engine_view(state, &settings, hex_state.as_ref()))
        }
        "/api/deleteOldSpeechModels" => {
            blocking(move || {
                retired::delete_old_models(&paths)
                    .map_err(|message| failure("internalError", message))
            })
            .await?;
            read_all(state).await
        }
        "/api/skipDictationPolish" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| failure("invalidParams", "id must name a recording."))?;
            Ok(json!({"skipped": cleanup::skip_polish(id)}))
        }
        _ => Err(failure("notFound", "Unknown Dictate route.")),
    }
}

/// `/api/readDictateSettings`' answer.
async fn read_all(state: &AppState) -> Result<Value, DomainStateError> {
    let paths = state.paths.clone();
    let settings = {
        let paths = paths.clone();
        blocking(move || Ok(dictate::current_settings(&paths))).await?
    };
    let project_words = dictate::project_words(state);
    let key_view = |provider: Provider| {
        let key = keys::key_state(&paths, provider);
        json!({"configured": key == KeyState::Saved, "unreadable": key == KeyState::Unreadable})
    };
    let old_models = {
        let paths = paths.clone();
        blocking(move || Ok(retired::old_models_view(&paths))).await?
    };
    let mut answer = json!({ "oldModels": old_models });
    let object = answer.as_object_mut().expect("a JSON object");
    // Hex is asked on every read: its row says where the user stands even before it is picked.
    let hex_state = {
        let home = paths.home_dir.clone();
        blocking(move || Ok(hex::state(&home))).await?
    };
    object.insert("settings".to_string(), json!(settings));
    object.insert(
        "engine".to_string(),
        dictate::engine_view(
            state,
            &settings,
            (settings.engine == settings::Engine::Hex).then_some(&hex_state),
        ),
    );
    object.insert(
        "hex".to_string(),
        hex::view(&hex_state, settings.hex_model.as_deref()),
    );
    object.insert(
        "keys".to_string(),
        json!({"xai": key_view(Provider::Xai), "openai": key_view(Provider::OpenAi)}),
    );
    object.insert(
        "localRetired".to_string(),
        if settings.local_retired_notice {
            json!({ "message": retired::NOTICE })
        } else {
            Value::Null
        },
    );
    object.insert("projectWords".to_string(), json!(project_words));
    object.insert(
        "polishAvailable".to_string(),
        json!(cleanup::polish_available(state)),
    );
    object.insert(
        "languages".to_string(),
        Value::Array(
            languages::LANGUAGE_LABELS
                .iter()
                .map(|(code, label)| json!({"code": code, "label": label}))
                .collect(),
        ),
    );
    Ok(answer)
}
