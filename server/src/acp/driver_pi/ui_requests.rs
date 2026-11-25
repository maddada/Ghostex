//! Pi `extension_ui_request` approval/question bridging and model/thinking updates.

use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::state::{PendingPiRequest, PiActivityReporter, PiRequestKind, PiState};
use super::transport::{
    normalize_pi_thinking_level, pi_tool_acp_kind, summarize_pi_tool_input, PiRpcTransport,
};
use super::{
    evaluate_permission, AcpPermissionOption, PermissionDecision, PI_RPC_TIMEOUT,
    PI_RPIV_CUSTOM_OPTION, PI_RPIV_CUSTOM_TTL,
};

pub(super) fn handle_pi_extension_ui_request(
    msg: &Value,
    transport: &PiRpcTransport,
    state: &Mutex<PiState>,
    activity: &PiActivityReporter,
) {
    let request_id = msg
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let method = msg
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Fire-and-forget UI notifications require no response.
    match method {
        "setTitle" => {
            if let Some(title) = msg
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.session_title = Some(title.to_string());
            }
            return;
        }
        "notify" => {
            if msg.get("notifyType").and_then(Value::as_str) == Some("error") {
                if let Some(text) = msg
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.last_error = Some((text.to_string(), None));
                }
            }
            return;
        }
        "setStatus" | "setWidget" | "set_editor_text" => {
            return;
        }
        _ => {}
    }

    if request_id.is_empty() {
        return;
    }

    match method {
        "confirm" => {
            let title = msg
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Confirm")
                .to_string();
            let raw_message = msg
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();

            // Check if this is Ghostex's structured tool-approval gate.
            if let Ok(parsed) = serde_json::from_str::<Value>(raw_message) {
                if parsed.get("ghostexToolApproval").and_then(Value::as_bool) == Some(true) {
                    let tool_name = parsed
                        .get("toolName")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let tool_call_id = parsed
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| Some(request_id.clone()));
                    let input = parsed.get("input").cloned().unwrap_or(json!({}));
                    let acp_kind = pi_tool_acp_kind(&tool_name);
                    let summary = summarize_pi_tool_input(&tool_name, &input);

                    let (level, read_only, roots, cwd, already_allowed) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.approval_level,
                            s.read_only,
                            s.writable_roots.clone(),
                            s.cwd.clone(),
                            s.session_approvals.contains(&tool_name),
                        )
                    };

                    if read_only && acp_kind != "read" {
                        println!("\r\n[Native] Denied in read-only mode: {tool_name} ({summary})");
                        let _ = std::io::stdout().flush();
                        let _ = transport
                            .respond_extension_ui(&request_id, json!({ "confirmed": false }));
                        return;
                    }

                    if already_allowed {
                        let _ = transport
                            .respond_extension_ui(&request_id, json!({ "confirmed": true }));
                        return;
                    }

                    let acp_params = json!({
                        "toolCall": {
                            "toolCallId": tool_call_id,
                            "name": tool_name,
                            "title": tool_name,
                            "kind": acp_kind,
                            "rawInput": input,
                        }
                    });
                    let eval_options = vec![
                        AcpPermissionOption {
                            option_id: "allow_once".to_string(),
                            name: "Allow".to_string(),
                            kind: "allow_once".to_string(),
                        },
                        AcpPermissionOption {
                            option_id: "reject_once".to_string(),
                            name: "Deny".to_string(),
                            kind: "reject_once".to_string(),
                        },
                    ];

                    match evaluate_permission(level, &acp_params, &eval_options, &roots, &cwd) {
                        PermissionDecision::AutoApprove { .. } => {
                            println!(
                                "\r\n[Native] Auto-approved ({}): {tool_name} ({summary})",
                                level.label()
                            );
                            let _ = std::io::stdout().flush();
                            let _ = transport
                                .respond_extension_ui(&request_id, json!({ "confirmed": true }));
                            return;
                        }
                        PermissionDecision::AskUser { reason } => {
                            println!("\r\n[Native] Waiting for approval: {tool_name} ({summary})");
                            let _ = std::io::stdout().flush();
                            let pending = PendingPiRequest {
                                request_id,
                                kind: PiRequestKind::ToolApproval {
                                    tool_name: tool_name.clone(),
                                },
                                tool_name,
                                summary,
                                tool_use_id: tool_call_id,
                                reason,
                            };
                            let was_empty = {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                let empty = s.pending_requests.is_empty();
                                s.pending_requests.push_back(pending);
                                empty
                            };
                            if was_empty {
                                activity.sync();
                            }
                            return;
                        }
                    }
                }
            }

            // Third-party extension `ctx.ui.confirm(...)`.
            let summary = if raw_message.trim().is_empty() {
                title.clone()
            } else {
                raw_message.trim().to_string()
            };
            let pending = PendingPiRequest {
                request_id: request_id.clone(),
                kind: PiRequestKind::ExtensionConfirm,
                tool_name: title,
                summary,
                tool_use_id: Some(request_id),
                reason: None,
            };
            let was_empty = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                let empty = s.pending_requests.is_empty();
                s.pending_requests.push_back(pending);
                empty
            };
            if was_empty {
                activity.sync();
            }
        }
        "select" => {
            let title = msg
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Choose an option.")
                .to_string();
            let raw_options: Vec<String> = msg
                .get("options")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let mut options = Vec::with_capacity(raw_options.len());
            let mut rpiv_custom_option = None;
            for opt in raw_options {
                if opt.trim().eq_ignore_ascii_case(PI_RPIV_CUSTOM_OPTION) {
                    rpiv_custom_option = Some(opt);
                } else {
                    options.push(opt);
                }
            }
            let pending = PendingPiRequest {
                request_id: request_id.clone(),
                kind: PiRequestKind::Select {
                    title: title.clone(),
                    options,
                    rpiv_custom_option,
                },
                tool_name: title.clone(),
                summary: title,
                tool_use_id: Some(request_id),
                reason: None,
            };
            let was_empty = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                let empty = s.pending_requests.is_empty();
                s.pending_requests.push_back(pending);
                empty
            };
            if was_empty {
                activity.sync();
            }
        }
        "input" | "editor" => {
            // Check for a staged custom text answer from a preceding rpiv `select` prompt.
            let staged_custom = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                match s.pending_rpiv_custom_input.take() {
                    Some((text, at)) if at.elapsed() <= PI_RPIV_CUSTOM_TTL => Some(text),
                    _ => None,
                }
            };
            if let Some(custom_text) = staged_custom {
                let _ =
                    transport.respond_extension_ui(&request_id, json!({ "value": custom_text }));
                return;
            }
            let title = msg
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Enter a response.")
                .to_string();
            let placeholder = msg
                .get("placeholder")
                .or_else(|| msg.get("prefill"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let pending = PendingPiRequest {
                request_id: request_id.clone(),
                kind: PiRequestKind::TextInput {
                    title: title.clone(),
                    placeholder,
                },
                tool_name: title.clone(),
                summary: title,
                tool_use_id: Some(request_id),
                reason: None,
            };
            let was_empty = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                let empty = s.pending_requests.is_empty();
                s.pending_requests.push_back(pending);
                empty
            };
            if was_empty {
                activity.sync();
            }
        }
        _ => {
            let _ = transport.respond_extension_ui(&request_id, json!({ "cancelled": true }));
        }
    }
}

pub(super) fn apply_model_and_effort(
    transport: &PiRpcTransport,
    state: &Mutex<PiState>,
    requested_model: Option<&str>,
    requested_effort: Option<&str>,
) -> Result<(), String> {
    let target_model = requested_model.map(str::trim).filter(|m| {
        !m.is_empty() && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
    });
    let target_effort = requested_effort.and_then(normalize_pi_thinking_level);

    let (resolved_model, preserved_effort) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let resolved = target_model.and_then(|m| s.resolve_model_target(m));
        (resolved, s.current_effort.clone())
    };

    if let Some((provider, model_id)) = resolved_model {
        let data = transport.call(
            "set_model",
            json!({ "provider": provider, "modelId": model_id }),
            PI_RPC_TIMEOUT,
        )?;
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(obj) = data.as_object() {
            s.apply_model_object(obj);
        } else {
            s.current_provider = Some(provider);
            s.current_model_id = Some(model_id);
        }
        // `set_model` resets Pi's thinking level to the model's default; re-apply the requested
        // or previously active thinking level right after.
        let follow_up_effort = target_effort.clone().or(preserved_effort);
        drop(s);
        if let Some(level) = follow_up_effort {
            if transport
                .call(
                    "set_thinking_level",
                    json!({ "level": level }),
                    PI_RPC_TIMEOUT,
                )
                .is_ok()
            {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .current_effort = Some(level);
            }
        }
    } else if let Some(level) = target_effort {
        transport.call(
            "set_thinking_level",
            json!({ "level": level }),
            PI_RPC_TIMEOUT,
        )?;
        state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .current_effort = Some(level);
    }

    // Refresh authoritative state from Pi (picks up clamped thinkingLevel + contextWindow).
    if let Ok(data) = transport.call("get_state", json!({}), Duration::from_secs(10)) {
        state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .apply_get_state(&data);
    }
    Ok(())
}
