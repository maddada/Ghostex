//! Model catalog ingestion, reasoning effort validation, and live model/effort switching.

use std::sync::Mutex;

use serde_json::{json, Value};

use super::protocol::{non_empty_str, normalize_grok_reasoning_effort};
use super::state::GrokState;
use super::transport::GrokTransport;
use super::{GROK_DEFAULT_MODEL_SLUG, GROK_RPC_TIMEOUT};

pub(super) fn apply_grok_model_state(state: &mut GrokState, models: &Value) {
    use ghostex_gx_chat_core::menus::catalog::{
        is_placeholder_model_label, is_placeholder_model_value,
    };
    if let Some(cur) = models
        .get("currentModelId")
        .or_else(|| models.get("currentModel"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !is_placeholder_model_value(m) && *m != GROK_DEFAULT_MODEL_SLUG)
    {
        state.current_model = Some(cur.to_string());
    }
    if let Some(avail) = models.get("availableModels").and_then(Value::as_array) {
        let filtered: Vec<Value> = avail
            .iter()
            .filter(|entry| {
                let id = entry
                    .get("modelId")
                    .or_else(|| entry.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let name = entry
                    .get("name")
                    .or_else(|| entry.get("displayName"))
                    .and_then(Value::as_str)
                    .unwrap_or(id);
                !is_placeholder_model_value(id) && !is_placeholder_model_label(name)
            })
            .cloned()
            .collect();
        state.available_models = Value::Array(filtered);
    }
    if let Some(cur_model) = state.current_model.as_deref() {
        if let Some(entry) = state.available_models.as_array().and_then(|arr| {
            arr.iter().find(|e| {
                e.get("modelId")
                    .or_else(|| e.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    == Some(cur_model)
            })
        }) {
            if state.current_effort.is_none() {
                if let Some(eff) = normalize_grok_reasoning_effort(
                    entry
                        .pointer("/_meta/reasoningEffort")
                        .and_then(Value::as_str),
                ) {
                    state.current_effort = Some(eff);
                }
            }
            if state.usage_window_size.is_none() {
                if let Some(window) = entry
                    .pointer("/_meta/totalContextTokens")
                    .and_then(Value::as_u64)
                    .filter(|n| *n > 0)
                {
                    state.usage_window_size = Some(window);
                }
            }
        }
    }
}

pub(super) fn apply_grok_session_metadata(state: &mut GrokState, result: &Value) {
    if let Some(sid) = non_empty_str(result.get("sessionId")) {
        state.native_session_id = Some(sid.to_string());
    }
    if let Some(models) = result.get("models") {
        apply_grok_model_state(state, models);
    } else if let Some(models) = result.pointer("/_meta/modelState") {
        apply_grok_model_state(state, models);
    }
    if let Some(configs) = result.get("configOptions") {
        state.config_options = configs.clone();
        if let Some(arr) = configs.as_array() {
            for opt in arr {
                if opt.get("category").and_then(Value::as_str) == Some("thought_level") {
                    if let Some(val) = normalize_grok_reasoning_effort(
                        opt.get("currentValue").and_then(Value::as_str),
                    ) {
                        state.current_effort = Some(val);
                    }
                }
            }
        }
    }
}

pub(super) fn grok_model_offered_efforts(available_models: &Value, model_id: &str) -> Vec<String> {
    let Some(entry) = available_models.as_array().and_then(|arr| {
        arr.iter().find(|e| {
            e.get("modelId")
                .or_else(|| e.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                == Some(model_id)
        })
    }) else {
        return Vec::new();
    };
    if entry
        .pointer("/_meta/supportsReasoningEffort")
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Vec::new();
    }
    let Some(efforts) = entry
        .pointer("/_meta/reasoningEfforts")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    efforts
        .iter()
        .filter_map(|item| {
            let raw = non_empty_str(item.get("value")).or_else(|| non_empty_str(item.get("id")))?;
            normalize_grok_reasoning_effort(Some(raw))
        })
        .collect()
}

pub(super) fn find_thought_level_config_option_id(config_options: &Value) -> Option<String> {
    config_options.as_array()?.iter().find_map(|opt| {
        let is_select = opt
            .get("type")
            .and_then(Value::as_str)
            .is_none_or(|t| t == "select");
        let cat = opt.get("category").and_then(Value::as_str).unwrap_or("");
        let id = opt.get("id").and_then(Value::as_str)?;
        (is_select && (cat == "thought_level" || id.contains("effort") || id.contains("thinking")))
            .then(|| id.to_string())
    })
}

pub(super) fn apply_grok_model_and_effort(
    transport: &GrokTransport,
    state: &Mutex<GrokState>,
    requested_model: Option<&str>,
    requested_effort: Option<&str>,
) -> Result<(), String> {
    let (sid, proto_ver, current_model, avail, config_opts) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (
            s.native_session_id.clone(),
            s.protocol_version,
            s.current_model.clone(),
            s.available_models.clone(),
            s.config_options.clone(),
        )
    };
    let Some(sid) = sid else {
        return Err("Grok Native session is not initialized yet.".to_string());
    };
    let clean_model = requested_model
        .map(str::trim)
        .filter(|m| {
            !m.is_empty()
                && *m != GROK_DEFAULT_MODEL_SLUG
                && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
        })
        .map(str::to_string);
    let clean_effort = normalize_grok_reasoning_effort(requested_effort);
    let target_model = clean_model.clone().or(current_model.clone());

    if proto_ver <= 1 {
        if let Some(target) = target_model {
            let offered = grok_model_offered_efforts(&avail, &target);
            let valid_effort = clean_effort
                .as_ref()
                .filter(|eff| offered.is_empty() || offered.iter().any(|o| o == *eff))
                .cloned();
            let mut params = json!({
                "sessionId": sid,
                "modelId": target,
            });
            if let Some(ref eff) = valid_effort {
                params["_meta"] = json!({ "reasoningEffort": eff });
            }
            let res = transport.call("session/set_model", params, GROK_RPC_TIMEOUT);
            if let Err(err) = res {
                if valid_effort.is_some() {
                    // Retry without reasoningEffort metadata if Grok rejected the effort.
                    transport.call(
                        "session/set_model",
                        json!({ "sessionId": sid, "modelId": target }),
                        GROK_RPC_TIMEOUT,
                    )?;
                } else {
                    return Err(err);
                }
            }
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.current_model = Some(target.clone());
            if let Some(window) = s.model_context_window(Some(&target)) {
                s.usage_window_size = Some(window);
            }
            if let Some(eff) = valid_effort.or(clean_effort.clone()) {
                s.current_effort = Some(eff);
            }
        }
        // Also apply `thought_level` config option when exposed by Grok 1.0.93+.
        if let Some(ref eff) = clean_effort {
            if let Some(cfg_id) = find_thought_level_config_option_id(&config_opts) {
                let _ = transport.call(
                    "session/set_config_option",
                    json!({
                        "sessionId": sid,
                        "configId": cfg_id,
                        "value": eff,
                    }),
                    GROK_RPC_TIMEOUT,
                );
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.current_effort = Some(eff.clone());
            }
        }
        return Ok(());
    }

    // Protocol v2+: model switch via session/set_model, effort via thought_level config option.
    if let Some(target) = clean_model {
        transport.call(
            "session/set_model",
            json!({ "sessionId": sid, "modelId": target }),
            GROK_RPC_TIMEOUT,
        )?;
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.current_model = Some(target.clone());
        if let Some(window) = s.model_context_window(Some(&target)) {
            s.usage_window_size = Some(window);
        }
    }
    if let Some(eff) = clean_effort {
        let cfg_id = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            find_thought_level_config_option_id(&s.config_options)
        };
        if let Some(cfg_id) = cfg_id {
            let _ = transport.call(
                "session/set_config_option",
                json!({
                    "sessionId": sid,
                    "configId": cfg_id,
                    "value": eff,
                }),
                GROK_RPC_TIMEOUT,
            );
        }
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.current_effort = Some(eff);
    }
    Ok(())
}
