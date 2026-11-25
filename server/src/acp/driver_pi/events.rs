//! Pi RPC stdout reader and event/response dispatch.

use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::state::{PiActivityReporter, PiReaderExitGuard, PiState};
use super::transport::{
    extract_usage_tokens, read_pi_streaming_behavior, summarize_pi_tool_input, HeldPrompt,
    PiRpcTransport,
};
use super::ui_requests::handle_pi_extension_ui_request;
use super::{append_bounded, now_iso_rfc3339, now_ms, LIVE_STREAM_MAX_CHARS};

pub(super) fn spawn_pi_stdout_reader(
    stdout: std::process::ChildStdout,
    transport: Arc<PiRpcTransport>,
    state: Arc<Mutex<PiState>>,
    activity: PiActivityReporter,
) {
    thread::spawn(move || {
        let _exit_guard = PiReaderExitGuard {
            state: state.clone(),
            activity: activity.clone(),
        };
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let Ok(read_bytes) = reader.read_line(&mut line) else {
                break;
            };
            if read_bytes == 0 {
                break;
            }
            let handle_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let trimmed = line.trim_end_matches(['\r', '\n']).trim();
                if trimmed.is_empty() {
                    return;
                }
                let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                    return;
                };
                transport.spool_event(&msg);
                {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.last_event_at_ms = now_ms();
                }
                let msg_type = msg.get("type").and_then(Value::as_str).unwrap_or_default();
                match msg_type {
                    "response" => {
                        handle_pi_response(&msg, &transport, &state, &activity);
                    }
                    "agent_start" => {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            s.last_error = None;
                            s.last_turn_interrupted = false;
                        }
                        activity.sync();
                    }
                    "message_start" => {
                        if msg.pointer("/message/role").and_then(Value::as_str) == Some("assistant")
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                        }
                    }
                    "message_update" => {
                        handle_pi_message_update(&msg, &state);
                    }
                    "message_end" => {
                        handle_pi_message_end(&msg, &state);
                    }
                    "tool_execution_start" => {
                        let tool_name = msg
                            .get("toolName")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .unwrap_or("Tool")
                            .to_string();
                        let detail = msg
                            .get("args")
                            .map(|args| summarize_pi_tool_input(&tool_name, args))
                            .filter(|d| !d.is_empty() && d != &tool_name);
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.live_tool_label = Some(tool_name);
                        s.live_tool_detail = detail;
                        if s.live_tool_started_at.is_none() {
                            s.live_tool_started_at = Some(now_iso_rfc3339());
                        }
                    }
                    "tool_execution_end" => {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.live_tool_label = None;
                        s.live_tool_detail = None;
                        s.live_tool_started_at = None;
                    }
                    "compaction_start" => {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.is_compacting = true;
                            s.working = true;
                            s.compacting_started_at = Some(now_iso_rfc3339());
                        }
                        activity.sync();
                    }
                    "compaction_end" => {
                        let held_next = {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.is_compacting = false;
                            s.compacting_started_at = None;
                            let next = s.held_prompts.pop_front();
                            if next.is_none() && !s.auto_retrying {
                                s.working = false;
                                s.clear_live_turn_output();
                            }
                            next
                        };
                        if let Some(held) = held_next {
                            let behavior = read_pi_streaming_behavior();
                            if let Err(err) =
                                transport.send_prompt_async(&held.text, held.images, Some(behavior))
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.last_error = Some((err, None));
                                s.working = false;
                            } else {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.working = true;
                            }
                        }
                        activity.sync();
                    }
                    "auto_retry_start" => {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.auto_retrying = true;
                        s.working = true;
                    }
                    "auto_retry_end" => {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.auto_retrying = false;
                        if msg.get("success").and_then(Value::as_bool) == Some(false) {
                            if let Some(err) = msg
                                .get("finalError")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                            {
                                s.last_error = Some((err.to_string(), None));
                            }
                        }
                    }
                    "agent_end" | "agent_settled" => {
                        let settled = {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            if msg_type == "agent_settled" || (!s.is_compacting && !s.auto_retrying)
                            {
                                s.working = false;
                                s.auto_retrying = false;
                                s.clear_live_turn_output();
                                true
                            } else {
                                false
                            }
                        };
                        if settled {
                            println!("[Native] Turn finished.");
                            let _ = std::io::stdout().flush();
                            activity.sync();
                        }
                    }
                    "extension_ui_request" => {
                        handle_pi_extension_ui_request(&msg, &transport, &state, &activity);
                    }
                    "extension_error" => {
                        if let Some(err) = msg
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                        {
                            eprintln!("[Native] Pi extension error: {err}");
                        }
                    }
                    _ => {}
                }
            }));
            if handle_res.is_err() {
                eprintln!(
                    "[Native] Warning: recovered from panic while handling Pi RPC stdout message."
                );
            }
        }
    });
}

pub(super) fn handle_pi_response(
    msg: &Value,
    transport: &Arc<PiRpcTransport>,
    state: &Arc<Mutex<PiState>>,
    activity: &PiActivityReporter,
) {
    let id = msg.get("id").and_then(Value::as_str).unwrap_or_default();
    if !id.is_empty() {
        if let Some(tx) = transport
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id)
        {
            let _ = tx.send(msg.clone());
            return;
        }
    }
    let command = msg
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let in_flight = if !id.is_empty() {
        transport
            .in_flight_prompts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id)
    } else {
        None
    };
    let success = msg.get("success").and_then(Value::as_bool).unwrap_or(true);
    if !success {
        let err_text = msg
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Pi command failed")
            .to_string();
        if command == "prompt"
            && err_text
                .to_ascii_lowercase()
                .contains("compaction is in progress")
        {
            if let Some(prompt) = in_flight {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.is_compacting = true;
                s.held_prompts.push_back(HeldPrompt {
                    text: prompt.text,
                    images: prompt.images,
                });
                return;
            }
        }
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.last_error = Some((err_text, None));
            if matches!(command, "prompt" | "compact") && !s.is_compacting {
                s.working = false;
                s.clear_live_turn_output();
            }
        }
        activity.sync();
        return;
    }

    // Command-only slash commands ack with `disposition: "handled"` and emit no `agent_end` /
    // `agent_settled`. Probe `get_state` briefly so the turn settles cleanly.
    if command == "prompt"
        && msg.pointer("/data/disposition").and_then(Value::as_str) == Some("handled")
    {
        let t_probe = transport.clone();
        let st_probe = state.clone();
        let act_probe = activity.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            if let Ok(data) = t_probe.call("get_state", json!({}), Duration::from_secs(5)) {
                let is_streaming = data
                    .get("isStreaming")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let is_compacting = data
                    .get("isCompacting")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let pending_count = data
                    .get("pendingMessageCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let mut should_sync = false;
                {
                    let mut s = st_probe.lock().unwrap_or_else(|p| p.into_inner());
                    s.apply_get_state(&data);
                    if !is_streaming && !is_compacting && pending_count == 0 && s.working {
                        s.working = false;
                        s.clear_live_turn_output();
                        should_sync = true;
                    }
                }
                if should_sync {
                    act_probe.sync();
                }
            }
        });
    }
}

pub(super) fn handle_pi_message_update(msg: &Value, state: &Mutex<PiState>) {
    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(usage) = msg.pointer("/message/usage") {
        if let Some(tokens) = extract_usage_tokens(usage) {
            s.usage_used_tokens = Some(tokens);
        }
    }
    let Some(event) = msg.get("assistantMessageEvent") else {
        return;
    };
    let kind = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "text_delta" => {
            if let Some(delta) = event
                .get("delta")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                if s.live_stream_started_at.is_none() {
                    s.live_stream_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_stream_text, delta, LIVE_STREAM_MAX_CHARS);
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
            }
        }
        "thinking_delta" => {
            if let Some(delta) = event
                .get("delta")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                if s.live_thought_started_at.is_none() {
                    s.live_thought_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_thought_text, delta, LIVE_STREAM_MAX_CHARS);
            }
        }
        "toolcall_start" => {
            if let Some(name) = event
                .pointer("/toolCall/name")
                .or_else(|| event.get("toolName"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                s.live_tool_label = Some(name.to_string());
                if s.live_tool_started_at.is_none() {
                    s.live_tool_started_at = Some(now_iso_rfc3339());
                }
            }
        }
        _ => {}
    }
}

pub(super) fn handle_pi_message_end(msg: &Value, state: &Mutex<PiState>) {
    let Some(message) = msg.get("message").and_then(Value::as_object) else {
        return;
    };
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(provider) = message
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        s.current_provider = Some(provider.to_string());
    }
    if let Some(model) = message
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        s.current_model_id = Some(model.to_string());
    }
    if let Some(usage) = message.get("usage") {
        if let Some(tokens) = extract_usage_tokens(usage) {
            s.usage_used_tokens = Some(tokens);
        }
    }
    let stop_reason = message
        .get("stopReason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if stop_reason.eq_ignore_ascii_case("error") {
        let err = message
            .get("errorMessage")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("Pi returned an error.")
            .to_string();
        s.last_error = Some((err, None));
    } else if stop_reason.eq_ignore_ascii_case("aborted") {
        s.last_turn_interrupted = true;
    }
}
