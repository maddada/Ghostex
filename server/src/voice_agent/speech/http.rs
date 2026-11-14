//! Dictate's settings API, for the desktop's Dictation page and the phone's Settings › Voice
//! (contract: issues-1027/speech-to-text/api-contract.md):
//!
//! - `/api/readDictateSettings`, `/api/setDictateSettings` `{patch}`
//! - `/api/readDictateEngine` `{prepare?}`: what will transcribe and whether it can, for the mic
//!   (greyed when it can't); `prepare` (the mic was pressed) opens a closed Hex on macOS and
//!   starts Superwhisper's service
//! - `hex` in `/api/readDictateSettings`: where the user stands with Hex 3 (`hex.rs`)
//! - `superwhisper` in `/api/readDictateSettings`: whether Superwhisper is installed (`superwhisper.rs`)
//! - `/api/deleteOldSpeechModels` and `/api/skipDictationPolish` `{id}`: kept for phones at 11.2.1
//!   and desktops at 11.2.2, which may still call them; gxserver deletes the old models on its own
//!   (`retired.rs`) and nothing polishes any more (CDXC:Dictate 2026-11-10 in transcribe.rs)

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::{dictate, hex, languages, retired, settings, superwhisper};
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
            // Sent when the microphone is pressed: a closed Hex opens while the user speaks.
            let prepare = params
                .get("prepare")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let (settings, hex_state) = blocking(move || {
                let settings = dictate::current_settings(&paths);
                let hex_state = dictate::hex_state_for(&paths, &settings);
                if prepare && matches!(hex_state, Some(hex::State::NotRunning)) {
                    hex::open_in_background(&paths.home_dir);
                }
                if prepare && settings.engine == settings::Engine::Superwhisper {
                    superwhisper::prepare_in_background(&paths.home_dir);
                }
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
        "/api/skipDictationPolish" => Ok(json!({"skipped": false})),
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
    let key_view = |provider: Provider| {
        let key = keys::key_state(&paths, provider);
        json!({"configured": key == KeyState::Saved, "unreadable": key == KeyState::Unreadable})
    };
    // Phones and desktops from before the local engine was retired read these without a guard.
    let mut answer = json!({
        "models": [],
        "modelsDiskBytes": 0,
        "localAvailable": false,
    });
    let object = answer.as_object_mut().expect("a JSON object");
    // Hex is asked on every read: its row says where the user stands even before it is picked.
    let hex_state = {
        let home = paths.home_dir.clone();
        blocking(move || Ok(hex::state(&home))).await?
    };
    let mut settings_view = json!(settings);
    // Phones at 11.2.1 read these without a guard, and desktops at 11.2.2 take a missing switch as
    // on: both show an empty dictionary and their filler and AI polish switches off, with polish
    // unavailable (`polishAvailable` below). Their changes to them are ignored (`settings.rs`).
    if let Some(view) = settings_view.as_object_mut() {
        view.insert("dictionary".to_string(), json!([]));
        view.insert("addProjectNames".to_string(), Value::Bool(false));
        view.insert("removeFillers".to_string(), Value::Bool(false));
        view.insert(
            "polish".to_string(),
            json!({"enabled": false, "model": "claude-sonnet-5-5", "prompt": null}),
        );
    }
    object.insert("settings".to_string(), settings_view);
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
    // Offered on the engine picker only where it is installed (Linux).
    object.insert(
        "superwhisper".to_string(),
        superwhisper::view(&superwhisper::state(&paths.home_dir)),
    );
    object.insert(
        "keys".to_string(),
        json!({"xai": key_view(Provider::Xai), "openai": key_view(Provider::OpenAi)}),
    );
    // Older clients hide the retired engine's notice and Old speech models row when these are null.
    object.insert("localRetired".to_string(), Value::Null);
    object.insert("oldModels".to_string(), Value::Null);
    object.insert("projectWords".to_string(), json!([]));
    object.insert("polishAvailable".to_string(), Value::Bool(false));
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
