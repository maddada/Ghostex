//! Dictate's settings and local models API, for the desktop's Voice page and the phone's
//! Settings › Voice (contract: issues-1027/speech-to-text/api-contract.md):
//!
//! - `/api/readDictateSettings`, `/api/setDictateSettings` `{patch}`
//! - `/api/readDictateEngine`: what will transcribe, for the mic's tooltip
//! - `hex` in `/api/readDictateSettings`: where the user stands with Hex 3 (`hex.rs`)
//! - `/api/listSpeechModels`, `/api/downloadSpeechModel` `{id}`,
//!   `/api/cancelSpeechModelDownload` `{id}`, `/api/deleteSpeechModel` `{id}`
//! - `/api/skipDictationPolish` `{id}`

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::{catalog, cleanup, dictate, hex, models, settings};
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

fn model_param(
    params: &Map<String, Value>,
) -> Result<&'static catalog::ModelSpec, DomainStateError> {
    params
        .get("id")
        .and_then(Value::as_str)
        .and_then(catalog::model)
        .ok_or_else(|| failure("invalidParams", "id must name a local model."))
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
        "/api/listSpeechModels" => {
            let settings = {
                let paths = paths.clone();
                blocking(move || Ok(dictate::current_settings(&paths))).await?
            };
            blocking(move || Ok(models_answer(&paths, &settings.local_model))).await
        }
        "/api/downloadSpeechModel" => {
            let spec = model_param(&params)?;
            let logger = state.logger.clone();
            let server_id = state.metadata.server_id.clone();
            models::start_download(&paths, spec, move |result| {
                if let Err(message) = result {
                    super::super::log_warn_error(
                        &logger,
                        &server_id,
                        "voiceAgent.speechModelDownloadFailed",
                        &message,
                        json!({"model": spec.id}),
                    );
                }
            });
            Ok(json!({"model": model_view(&paths, spec)}))
        }
        "/api/cancelSpeechModelDownload" => {
            let spec = model_param(&params)?;
            models::cancel_download(&paths, spec);
            Ok(json!({"model": model_view(&paths, spec)}))
        }
        "/api/deleteSpeechModel" => {
            let spec = model_param(&params)?;
            {
                let paths = paths.clone();
                blocking(move || {
                    models::delete(&paths, spec)
                        .map_err(|message| failure("internalError", message))
                })
                .await?;
            }
            Ok(json!({"model": model_view(&paths, spec)}))
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

fn model_view(paths: &crate::paths::GxserverPaths, spec: &catalog::ModelSpec) -> Value {
    let in_use = dictate::current_settings(paths).local_model == spec.id;
    models::view(paths, spec, in_use)
}

fn models_answer(paths: &crate::paths::GxserverPaths, in_use: &str) -> Value {
    let folder = models::folder(paths);
    json!({
        "models": models::list(paths, in_use),
        "modelsFolder": folder.to_string_lossy(),
        "modelsDiskBytes": models::disk_bytes(&folder),
    })
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
    let mut answer = {
        let paths = paths.clone();
        let in_use = settings.local_model.clone();
        blocking(move || Ok(models_answer(&paths, &in_use))).await?
    };
    let object = answer.as_object_mut().expect("models_answer is an object");
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
    object.insert("localAvailable".to_string(), json!(true));
    object.insert("projectWords".to_string(), json!(project_words));
    object.insert(
        "polishAvailable".to_string(),
        json!(cleanup::polish_available(state)),
    );
    object.insert(
        "languages".to_string(),
        Value::Array(
            catalog::LANGUAGE_LABELS
                .iter()
                .map(|(code, label)| json!({"code": code, "label": label}))
                .collect(),
        ),
    );
    Ok(answer)
}
