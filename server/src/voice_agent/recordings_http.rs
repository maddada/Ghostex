//! Dictate's recordings API (`recordings.rs` keeps them, `transcribe.rs` transcribes them):
//!
//! - `/api/saveDictationRecording` `{id, offset, chunk (base64), done?, createdAt, durationMs,
//!   client, dismissed?, language?}`: one piece of a recording's WAV (pieces under 1 MiB, so the
//!   ordinary body limit holds); answers `{received}` and, after `done`, `{dictation}`.
//! - `/api/transcribeDictation` `{id}` or `{latest: true}`: transcribes it (Transcribe again,
//!   Recover, Transcribe previous) and answers `{dictation}` with its text or why it failed.
//! - `/api/dismissDictation` `{id}`: the reader cancelled; the recording is kept as dismissed.
//! - `/api/listDictations` `{query?, limit?}`: the recordings, newest first, and
//!   `latestUntranscribed` (what Transcribe previous takes).
//! - `/api/readDictationAudio` `{id}`: the audio as base64, for a client on another device.
//! - `/api/deleteDictation` `{id}`.

use axum::http::StatusCode;
use base64::Engine as _;
use serde_json::{json, Map, Value};

use super::recordings::{self, Finished, Piece, Saved};
use super::transcribe;
use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

/// Base64 of a piece may not exceed the 1 MiB JSON limit.
const MAX_PIECE_BYTES: usize = 640 * 1024;

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

fn id_param(params: &Map<String, Value>) -> Result<String, DomainStateError> {
    params
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| recordings::valid_id(id))
        .map(str::to_string)
        .ok_or_else(|| failure("invalidParams", "id must name a recording."))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, DomainStateError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| failure("internalError", error.to_string()))?
        .map_err(|message| failure("internalError", message))
}

/// Every recordings route (the paths listed above).
pub(crate) async fn handle_dictation_recordings_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = route(state, &endpoint_path, body).await;
    respond(endpoint_path, request_id, result)
}

async fn route(state: &AppState, path: &str, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let paths = state.paths.clone();
    {
        let paths = paths.clone();
        blocking(move || {
            recordings::ensure_recovered(&paths);
            Ok(())
        })
        .await?;
    }
    match path {
        "/api/saveDictationRecording" => save(state, &params).await,
        "/api/transcribeDictation" => {
            let id = if params.get("latest").and_then(Value::as_bool) == Some(true) {
                let paths = paths.clone();
                blocking(move || Ok(recordings::latest_awaiting_text(&paths)))
                    .await?
                    .map(|recording| recording.id)
                    .ok_or_else(|| {
                        failure(
                            "notFound",
                            "There is no recording waiting to be transcribed.",
                        )
                    })?
            } else {
                id_param(&params)?
            };
            let recording = transcribe::transcribe(state, &id)
                .await
                .map_err(|message| failure("notFound", message))?;
            Ok(json!({"dictation": recording.to_client(&paths)}))
        }
        "/api/dismissDictation" => {
            let id = id_param(&params)?;
            let recording = {
                let paths = paths.clone();
                blocking(move || recordings::dismiss(&paths, &id)).await?
            };
            Ok(json!({"dictation": recording.to_client(&paths)}))
        }
        "/api/listDictations" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .map(|query| query.trim().to_lowercase())
                .unwrap_or_default();
            let all = {
                let paths = paths.clone();
                blocking(move || Ok(recordings::list(&paths))).await?
            };
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(usize::MAX, |limit| limit as usize);
            let latest = all.iter().find(|recording| recording.awaiting_text());
            let total_bytes: u64 = all.iter().map(|recording| recording.audio_bytes).sum();
            let shown: Vec<Value> = all
                .iter()
                .filter(|recording| {
                    query.is_empty()
                        || recording
                            .text
                            .as_deref()
                            .is_some_and(|text| text.to_lowercase().contains(&query))
                })
                .take(limit)
                .map(|recording| recording.to_client(&paths))
                .collect();
            Ok(json!({
                "dictations": shown,
                "latestUntranscribed": latest.map(|recording| recording.to_client(&paths)),
                "count": all.len(),
                "totalBytes": total_bytes,
                "folder": recordings::folder(&paths).to_string_lossy(),
            }))
        }
        "/api/readDictationAudio" => {
            let id = id_param(&params)?;
            let recording = recordings::read(&paths, &id)
                .ok_or_else(|| failure("notFound", "That recording no longer exists."))?;
            let audio = tokio::fs::read(recordings::folder(&paths).join(&recording.audio_file))
                .await
                .map_err(|error| failure("notFound", format!("The audio is missing: {error}")))?;
            let format = recording.to_client(&paths)["format"].clone();
            Ok(json!({
                "format": format,
                "audioBase64": base64::engine::general_purpose::STANDARD.encode(audio),
            }))
        }
        "/api/deleteDictation" => {
            let id = id_param(&params)?;
            blocking(move || recordings::delete(&paths, &id)).await?;
            Ok(json!({"deleted": true}))
        }
        _ => Err(failure("notFound", "Unknown dictation route.")),
    }
}

async fn save(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let id = id_param(params)?;
    let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let chunk = base64::engine::general_purpose::STANDARD
        .decode(
            params
                .get("chunk")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .map_err(|_| failure("invalidParams", "chunk must be base64."))?;
    if chunk.len() > MAX_PIECE_BYTES {
        return Err(failure(
            "invalidParams",
            format!("A piece may carry at most {MAX_PIECE_BYTES} bytes."),
        ));
    }
    let done = params
        .get("done")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        .then(|| Finished {
            created_at: params.get("createdAt").and_then(Value::as_i64).unwrap_or(0),
            duration_ms: params
                .get("durationMs")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            client: match params.get("client").and_then(Value::as_str) {
                Some("phone") => "phone",
                _ => "desktop",
            }
            .to_string(),
            dismissed: params.get("dismissed").and_then(Value::as_bool) == Some(true),
            language: params
                .get("language")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|language| {
                    !language.is_empty()
                        && language.len() <= 16
                        && language
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-')
                })
                .map(str::to_string),
        });
    let paths = state.paths.clone();
    let saved = {
        let paths = paths.clone();
        blocking(move || {
            recordings::save_piece(
                &paths,
                Piece {
                    id: &id,
                    offset,
                    bytes: &chunk,
                    done,
                },
            )
        })
        .await?
    };
    Ok(match saved {
        Saved::Partial(received) => json!({"received": received}),
        Saved::Complete(recording) => json!({
            "received": recording.audio_bytes,
            "dictation": recording.to_client(&paths),
        }),
    })
}
