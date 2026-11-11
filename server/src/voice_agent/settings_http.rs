//! `/api/setVoiceAgentSettings`: the voice picker and speed buttons of a voice screen on another
//! device (the phone) save the computer's `voiceAgentXaiVoice` / `voiceAgentOpenaiVoice`,
//! `voiceAgentSpeed` and `voiceAgentCallMode`.
//!
//! CDXC:VoiceMode 2026-10-20 WHY:
//! The voice and the speed are the computer's settings, shared by every client, so the phone saves them here instead of keeping its own. The write goes through the running desktop app's `updateSettingsPatch` (CDXC:Settings 2026-09-09: settings writes go through the app, never a direct file write); this endpoint only checks the values first, so a remote client can change these two settings and nothing else.

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::keys;
use super::provider::Provider;
use crate::domain::read_domain_rpc_params;
use crate::protocol::{rpc_error, rpc_success};
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

const RENDERER_TIMEOUT_MS: u64 = 5_000;

fn bad_request(endpoint_path: String, request_id: String, message: String) -> RoutedResponse {
    routed_json(
        Some(endpoint_path),
        StatusCode::BAD_REQUEST,
        rpc_error("badRequest", message, Some(request_id)),
    )
}

/// `{provider?, voice?, speed?}`: `voice` is one of `provider`'s voices (the selected provider
/// when omitted), `speed` one of 1, 1.2, 1.4, 1.6, 1.8, 2, `callMode` `regular` or
/// `pushToTalk` (switches the running calls at once, too). Answers the saved values.
pub(crate) async fn handle_set_voice_agent_settings_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let provider = match params.get("provider").and_then(Value::as_str) {
        Some(id) => match Provider::from_id(id) {
            Some(provider) => provider,
            None => {
                return bad_request(
                    endpoint_path,
                    request_id,
                    "provider must be xai or openai.".to_string(),
                )
            }
        },
        None => keys::selected_provider(&state.paths),
    };
    let mut patch = Map::new();
    if let Some(voice) = params.get("voice").and_then(Value::as_str) {
        let voice = voice.trim().to_ascii_lowercase();
        if !provider.voices().contains(&voice.as_str()) {
            return bad_request(
                endpoint_path,
                request_id,
                format!("{voice} is not a {} voice.", provider.label()),
            );
        }
        patch.insert(keys::voice_setting(provider).to_string(), json!(voice));
    }
    if let Some(speed) = params.get("speed") {
        let wanted = speed
            .as_f64()
            .or_else(|| speed.as_str().and_then(|text| text.trim().parse().ok()));
        let Some(speed) = wanted.and_then(|wanted| {
            keys::SPEEDS
                .into_iter()
                .find(|speed| (speed - wanted).abs() < 0.01)
        }) else {
            return bad_request(
                endpoint_path,
                request_id,
                "speed must be 1, 1.2, 1.4, 1.6, 1.8 or 2.".to_string(),
            );
        };
        // Stored the way Settings stores it: "1", "1.2", … "2".
        let text = format!("{speed:.1}");
        let text = text.strip_suffix(".0").unwrap_or(&text).to_string();
        patch.insert(keys::SPEED_SETTING.to_string(), json!(text));
    }
    if let Some(mode) = params.get("callMode") {
        let Some(mode) = mode
            .as_str()
            .filter(|mode| matches!(*mode, "regular" | "pushToTalk"))
        else {
            return bad_request(
                endpoint_path,
                request_id,
                "callMode must be regular or pushToTalk.".to_string(),
            );
        };
        patch.insert(keys::CALL_MODE_SETTING.to_string(), json!(mode));
    }
    if patch.is_empty() {
        return bad_request(
            endpoint_path,
            request_id,
            "Send a voice, a speed or a callMode.".to_string(),
        );
    }
    let mut payload = Map::new();
    payload.insert("patch".to_string(), Value::Object(patch.clone()));
    payload.insert("source".to_string(), json!("voiceAgent:remote"));
    match state
        .event_hub
        .dispatch_renderer_command(
            "updateSettingsPatch".to_string(),
            payload,
            RENDERER_TIMEOUT_MS,
        )
        .await
    {
        Ok(_) => {
            // Every running call follows the new mode at once (`Conversation::set_call_mode`).
            if let Some(mode) = patch.get(keys::CALL_MODE_SETTING).and_then(Value::as_str) {
                let push_to_talk = mode == "pushToTalk";
                for tx in super::registry::senders() {
                    let _ = tx.send(super::conversation::Input::CallMode(push_to_talk));
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({"provider": provider.id(), "saved": Value::Object(patch)}),
                ),
            )
        }
        Err(error) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}
