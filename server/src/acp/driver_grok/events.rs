//! Inbound Grok stdout JSON-RPC message handling (reverse requests, xAI notifications, ACP updates).

use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::acp::approval::{evaluate_permission, AcpApprovalLevel, PermissionDecision};
use crate::acp::host::{
    append_bounded, end_live_segment, now_iso_rfc3339, now_ms, LIVE_STREAM_MAX_CHARS,
};

use super::approvals::{
    parse_grok_permission_request, parse_xai_ask_user_question_request,
    parse_xai_exit_plan_mode_request,
};
use super::protocol::{
    extract_grok_plan_markdown_from_update, extract_xai_child_session_id,
    extract_xai_monitor_task_id, is_xai_async_spawn_ack, is_xai_get_output_tool,
    is_xai_spawn_or_task_tool, is_xai_uuid, non_empty_str, normalize_grok_reasoning_effort,
    resolve_xai_tool_title, xai_tool_output_text,
};
use super::state::{
    GrokBackgroundTaskEntry, GrokLaunchMode, GrokPromptOutcome, GrokState, GrokToolInfo,
};
use super::transport::{GrokActivityReporter, GrokTransport};
use super::{
    XAI_RATE_LIMITED_ERROR_CODE, XAI_SUBAGENT_COMPLETED_PROMPT_ID_PREFIX,
    XAI_TASK_COMPLETED_PROMPT_ID_PREFIX,
};

pub(super) fn parse_turn_completed_usage(update: &Value) -> Option<u64> {
    let usage = update.get("usage")?;
    usage
        .get("inputTokens")
        .or_else(|| usage.get("totalTokens"))
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
}

pub(super) fn extract_subagent_end_notice_from_text(text: &str) -> Option<(String, bool)> {
    let lower = text.to_ascii_lowercase();
    let pos = lower.find("background subagent ")?;
    let rest = &text[pos + "background subagent ".len()..];
    let trimmed = rest.trim_start_matches(['"', '\'', ' ']);
    let uuid: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    if !is_xai_uuid(&uuid) {
        return None;
    }
    let after_uuid = trimmed[uuid.len()..].trim_start_matches(['"', '\'', ' ']);
    // Skip balanced descriptor parentheses so nested titles cannot supply false outcome verbs.
    let tail = if after_uuid.starts_with('(') {
        let mut depth = 0usize;
        let mut cut = 0usize;
        for (i, ch) in after_uuid.char_indices() {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    cut = i + 1;
                    break;
                }
            }
        }
        after_uuid[cut..].trim_start().to_ascii_lowercase()
    } else {
        after_uuid.to_ascii_lowercase()
    };
    if tail.contains("completed successfully")
        || tail.contains("completed")
        || tail.contains("finished")
        || tail.contains("ended")
    {
        return Some((uuid, true));
    }
    if tail.contains("failed")
        || tail.contains("error")
        || tail.contains("crashed")
        || tail.contains("cancelled")
        || tail.contains("canceled")
        || tail.contains("interrupted")
        || tail.contains("timed out")
    {
        return Some((uuid, false));
    }
    None
}

pub(super) fn is_wake_prompt_id(prompt_id: &str) -> bool {
    prompt_id.starts_with(XAI_TASK_COMPLETED_PROMPT_ID_PREFIX)
        || prompt_id.starts_with(XAI_SUBAGENT_COMPLETED_PROMPT_ID_PREFIX)
}

pub(super) fn handle_grok_stdout_message(
    msg: &Value,
    transport: &Arc<GrokTransport>,
    state: &Arc<Mutex<GrokState>>,
    activity: &GrokActivityReporter,
) {
    transport.spool_event(msg);
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.last_event_at_ms = now_ms();
        if s.replaying_load {
            s.last_replay_activity_ms = Some(s.last_event_at_ms);
        }
    }

    let method = msg.get("method").and_then(Value::as_str);
    let has_id = msg.get("id").is_some();

    // 1. JSON-RPC response to our outgoing request.
    if method.is_none() && has_id {
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            if let Some(tx) = transport
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id)
            {
                let _ = tx.send(msg.clone());
            }
        }
        return;
    }

    // 2. Incoming reverse JSON-RPC request from Grok agent.
    if let (Some(method), true) = (method, has_id) {
        let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "session/request_permission" => {
                let mut req = parse_grok_permission_request(rpc_id, &params);
                let (level, spawned_mode, roots, session_cwd) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    (
                        s.approval_level,
                        s.spawned_launch_mode,
                        s.writable_roots.clone(),
                        s.cwd.clone(),
                    )
                };
                // Auto prompts from `--permission-mode auto` represent classifier refusals
                // and must always reach the user.
                if spawned_mode != GrokLaunchMode::Auto && level != AcpApprovalLevel::Auto {
                    match evaluate_permission(level, &params, &req.options, &roots, &session_cwd) {
                        PermissionDecision::AutoApprove { option_id } => {
                            println!(
                                "\r\n[Native] Auto-approved ({}): {} ({})",
                                level.label(),
                                req.tool_name,
                                req.summary
                            );
                            let _ = std::io::stdout().flush();
                            let _ = transport.respond_to_agent(
                                req.rpc_id,
                                json!({
                                    "outcome": {
                                        "outcome": "selected",
                                        "optionId": option_id,
                                    }
                                }),
                            );
                            return;
                        }
                        PermissionDecision::AskUser { reason } => {
                            req.reason = reason;
                        }
                    }
                }
                println!(
                    "\r\n[Native] Waiting for approval: {} ({})",
                    req.tool_name, req.summary
                );
                let _ = std::io::stdout().flush();
                let was_empty = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let empty = s.pending_requests.is_empty();
                    s.pending_requests.push_back(req);
                    empty
                };
                if was_empty {
                    activity.sync();
                }
            }
            "x.ai/ask_user_question" | "_x.ai/ask_user_question" => {
                let req = parse_xai_ask_user_question_request(rpc_id, &params);
                println!("\r\n[Native] Waiting for question answer: {}", req.summary);
                let _ = std::io::stdout().flush();
                let was_empty = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let empty = s.pending_requests.is_empty();
                    s.pending_requests.push_back(req);
                    empty
                };
                if was_empty {
                    activity.sync();
                }
            }
            "x.ai/exit_plan_mode" | "_x.ai/exit_plan_mode" => {
                let fallback = state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .last_proposed_plan_markdown
                    .clone();
                let req = parse_xai_exit_plan_mode_request(rpc_id, &params, fallback);
                println!("\r\n[Native] Waiting for plan approval...");
                let _ = std::io::stdout().flush();
                let was_empty = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let empty = s.pending_requests.is_empty();
                    s.pending_requests.push_back(req);
                    empty
                };
                if was_empty {
                    activity.sync();
                }
            }
            _ => {
                transport.respond_error_to_agent(
                    rpc_id,
                    -32601,
                    &format!("Unsupported client method: {method}"),
                );
            }
        }
        return;
    }

    // 3. Incoming JSON-RPC notifications from Grok agent.
    let Some(method) = method else {
        return;
    };
    let params = msg.get("params").unwrap_or(&Value::Null);

    match method {
        "x.ai/session/prompt_complete" | "_x.ai/session/prompt_complete" => {
            let sid = non_empty_str(params.get("sessionId")).unwrap_or_default();
            let prompt_id = non_empty_str(params.get("promptId"));
            let stop_reason = non_empty_str(params.get("stopReason")).unwrap_or("end_turn");
            let agent_result = non_empty_str(params.get("agentResult"));
            let outcome = match stop_reason {
                "rate_limit" => GrokPromptOutcome::Error {
                    code: XAI_RATE_LIMITED_ERROR_CODE,
                    message: "Grok usage limit reached. Try again later.".to_string(),
                    is_usage_limit: true,
                },
                "error" => GrokPromptOutcome::Error {
                    code: -32603,
                    message: agent_result
                        .unwrap_or("Grok ended the turn with an error.")
                        .to_string(),
                    is_usage_limit: false,
                },
                other => GrokPromptOutcome::Success {
                    stop_reason: other.to_string(),
                    usage: None,
                },
            };
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.resolve_prompt_waiter(sid, prompt_id, outcome);
            return;
        }
        "x.ai/task_backgrounded"
        | "_x.ai/task_backgrounded"
        | "x.ai/task_completed"
        | "_x.ai/task_completed" => {
            let is_completed = method.ends_with("task_completed");
            if let Some(update) = params.get("update") {
                let snapshot = update.get("task_snapshot");
                let task_id = non_empty_str(snapshot.and_then(|s| s.get("task_id")))
                    .or_else(|| non_empty_str(update.get("task_id")))
                    .or_else(|| non_empty_str(update.get("tool_call_id")));
                if let Some(tid) = task_id {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if is_completed {
                        if let Some(entry) = s.background_tasks.get_mut(tid) {
                            entry.running = false;
                        }
                    } else {
                        s.background_tasks
                            .insert(tid.to_string(), GrokBackgroundTaskEntry { running: true });
                    }
                }
            }
            return;
        }
        "x.ai/session_notification" | "_x.ai/session_notification" | "_x.ai/session/update" => {
            let sid = non_empty_str(params.get("sessionId")).unwrap_or_default();
            let Some(update) = params.get("update") else {
                return;
            };
            let kind = update
                .get("sessionUpdate")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match kind {
                "model_changed" => {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if s.native_session_id
                        .as_deref()
                        .is_none_or(|root| root == sid)
                    {
                        if let Some(m) = non_empty_str(update.get("model_id"))
                            .or_else(|| non_empty_str(update.get("modelId")))
                        {
                            s.current_model = Some(m.to_string());
                            if let Some(window) = s.model_context_window(Some(m)) {
                                s.usage_window_size = Some(window);
                            }
                        }
                        if let Some(eff) = normalize_grok_reasoning_effort(
                            update
                                .get("reasoning_effort")
                                .or_else(|| update.get("reasoningEffort"))
                                .and_then(Value::as_str),
                        ) {
                            s.current_effort = Some(eff);
                        }
                    }
                }
                "subagent_spawned" => {
                    let child_id = non_empty_str(update.get("child_session_id"))
                        .or_else(|| non_empty_str(update.get("subagent_id")));
                    if let Some(child_id) = child_id {
                        let name = non_empty_str(update.get("subagent_type"));
                        let task = non_empty_str(update.get("description"));
                        let model = non_empty_str(update.get("model"));
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.upsert_subagent(
                            child_id,
                            name,
                            task,
                            model,
                            Some(true),
                            Some(false),
                            None,
                            None,
                        );
                    }
                }
                "subagent_progress" => {
                    let child_id = non_empty_str(update.get("child_session_id"))
                        .or_else(|| non_empty_str(update.get("subagent_id")));
                    if let Some(child_id) = child_id {
                        let elapsed = update
                            .get("duration_ms")
                            .and_then(Value::as_u64)
                            .map(|ms| ms / 1000);
                        let tokens = update.get("tokens_used").and_then(Value::as_u64);
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.upsert_subagent(
                            child_id,
                            None,
                            None,
                            None,
                            Some(true),
                            Some(false),
                            elapsed,
                            tokens,
                        );
                    }
                }
                "subagent_finished" => {
                    let child_id = non_empty_str(update.get("child_session_id"))
                        .or_else(|| non_empty_str(update.get("subagent_id")));
                    if let Some(child_id) = child_id {
                        let elapsed = update
                            .get("duration_ms")
                            .and_then(Value::as_u64)
                            .map(|ms| ms / 1000);
                        let tokens = update.get("tokens_used").and_then(Value::as_u64);
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.upsert_subagent(
                            child_id,
                            None,
                            None,
                            None,
                            Some(false),
                            Some(true),
                            elapsed,
                            tokens,
                        );
                    }
                    activity.sync();
                }
                "turn_completed" => {
                    let prompt_id = non_empty_str(update.get("prompt_id"))
                        .or_else(|| non_empty_str(update.get("promptId")));
                    let stop_reason = non_empty_str(update.get("stop_reason"))
                        .or_else(|| non_empty_str(update.get("stopReason")))
                        .unwrap_or("end_turn");
                    let agent_result = non_empty_str(update.get("agent_result"));
                    let used_tokens = parse_turn_completed_usage(update);

                    let mut sync_after_wake = false;
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let is_root = s
                            .native_session_id
                            .as_deref()
                            .is_none_or(|root| root == sid);
                        if is_root {
                            if s.usage_used_tokens.is_none() {
                                if let Some(u) = used_tokens {
                                    s.usage_used_tokens = Some(u);
                                }
                            }
                            if let Some(pid) = prompt_id {
                                if is_wake_prompt_id(pid) {
                                    if s.wake_prompt_id.as_deref() == Some(pid) {
                                        s.wake_prompt_id = None;
                                        if s.pending_prompt_waiters.is_empty() {
                                            s.working = false;
                                            s.clear_live_turn_output();
                                            sync_after_wake = true;
                                        }
                                    }
                                } else {
                                    let outcome = match stop_reason {
                                        "rate_limit" => GrokPromptOutcome::Error {
                                            code: XAI_RATE_LIMITED_ERROR_CODE,
                                            message: "Grok usage limit reached. Try again later."
                                                .to_string(),
                                            is_usage_limit: true,
                                        },
                                        "error" => GrokPromptOutcome::Error {
                                            code: -32603,
                                            message: agent_result
                                                .unwrap_or("Grok ended the turn with an error.")
                                                .to_string(),
                                            is_usage_limit: false,
                                        },
                                        other => GrokPromptOutcome::Success {
                                            stop_reason: other.to_string(),
                                            usage: update.get("usage").cloned(),
                                        },
                                    };
                                    s.resolve_prompt_waiter(sid, Some(pid), outcome);
                                }
                            }
                        }
                    }
                    if sync_after_wake {
                        activity.sync();
                    }
                }
                _ => {}
            }
            return;
        }
        "session/update" => {}
        _ => return,
    }

    // Standard ACP `session/update` notification.
    let sid = non_empty_str(params.get("sessionId")).unwrap_or_default();
    let Some(update) = params.get("update") else {
        return;
    };
    let meta_prompt_id = non_empty_str(params.pointer("/_meta/promptId"));
    let meta_total_tokens = params
        .pointer("/_meta/totalTokens")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0);
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let (is_root_session, replaying) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let is_root = s
            .native_session_id
            .as_deref()
            .is_none_or(|root| root == sid);
        if is_root {
            if let Some(tokens) = meta_total_tokens {
                s.usage_used_tokens = Some(tokens);
            }
        }
        (is_root, s.replaying_load)
    };

    if !is_root_session {
        // Child subagent update: never leak child chunks into the root live stream!
        if !sid.is_empty() {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = s.subagents.get_mut(sid) {
                if !entry.done {
                    entry.working = true;
                }
            }
        }
        return;
    }

    // If a background wake continuation starts streaming on the root session while idle, mark working.
    let mut started_wake = false;
    if !replaying {
        if let Some(pid) = meta_prompt_id.filter(|p| is_wake_prompt_id(p)) {
            if matches!(
                kind,
                "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update"
            ) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if !s.working && s.pending_prompt_waiters.is_empty() {
                    s.working = true;
                    s.wake_prompt_id = Some(pid.to_string());
                    s.last_turn_interrupted = false;
                    s.clear_live_turn_output();
                    started_wake = true;
                }
            }
        }
    }
    if started_wake {
        activity.sync();
    }

    match kind {
        "config_option_update" => {
            if let Some(opts) = update.get("configOptions") {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.config_options = opts.clone();
                if let Some(arr) = opts.as_array() {
                    for opt in arr {
                        if opt.get("category").and_then(Value::as_str) == Some("thought_level") {
                            if let Some(val) = normalize_grok_reasoning_effort(
                                opt.get("currentValue").and_then(Value::as_str),
                            ) {
                                s.current_effort = Some(val);
                            }
                        }
                    }
                }
            }
        }
        "user_message_chunk" => {
            if let Some(text) = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
            {
                if let Some((child_id, _ok)) = extract_subagent_end_notice_from_text(text) {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if s.subagents.contains_key(&child_id) {
                        s.upsert_subagent(
                            &child_id,
                            None,
                            None,
                            None,
                            Some(false),
                            Some(true),
                            None,
                            None,
                        );
                    }
                }
            }
        }
        "agent_message_chunk" if !replaying => {
            if let Some(text) = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_stream_started_at.is_none() {
                    s.live_stream_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_stream_text, text, LIVE_STREAM_MAX_CHARS);
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
            }
        }
        "agent_thought_chunk" if !replaying => {
            if let Some(text) = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_thought_started_at.is_none() {
                    s.live_thought_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_thought_text, text, LIVE_STREAM_MAX_CHARS);
            }
        }
        "tool_call" | "tool_call_update" => {
            let call_id = non_empty_str(update.get("toolCallId"))
                .unwrap_or_default()
                .to_string();
            let raw_status = update
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();

            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if kind == "tool_call" && !replaying {
                end_live_segment!(s);
            }
            if let Some(plan_md) = extract_grok_plan_markdown_from_update(update) {
                s.last_proposed_plan_markdown = Some(plan_md);
            }

            let existing = if !call_id.is_empty() {
                s.active_tools.get(&call_id).cloned()
            } else {
                None
            };
            let raw_input = update
                .get("rawInput")
                .filter(|v| !v.is_null())
                .cloned()
                .or_else(|| existing.as_ref().map(|e| e.raw_input.clone()))
                .unwrap_or(Value::Null);
            let raw_output = update
                .get("rawOutput")
                .filter(|v| !v.is_null())
                .cloned()
                .or_else(|| existing.as_ref().map(|e| e.raw_output.clone()))
                .unwrap_or(Value::Null);
            let raw_title = update
                .get("title")
                .or_else(|| update.get("name"))
                .or_else(|| update.get("kind"))
                .and_then(Value::as_str)
                .or_else(|| existing.as_ref().map(|e| e.title.as_str()));
            let resolved_title =
                resolve_xai_tool_title(raw_title, update.get("_meta"), &raw_input, &raw_output)
                    .unwrap_or_else(|| "Tool".to_string());
            let output_text = xai_tool_output_text(&raw_output, update.get("content"));

            let bg_task_id = extract_xai_monitor_task_id(
                &resolved_title,
                &raw_input,
                &raw_output,
                output_text.as_deref(),
            );

            // Track background shell / monitor starts.
            if let Some(ref tid) = bg_task_id {
                s.background_tasks
                    .entry(tid.clone())
                    .or_insert_with(|| GrokBackgroundTaskEntry { running: true });
            }

            // Track subagent spawn / get_output / kill from tool calls.
            if is_xai_spawn_or_task_tool(&resolved_title, &raw_input) {
                if let Some(child_id) = output_text
                    .as_deref()
                    .and_then(extract_xai_child_session_id)
                {
                    let async_ack = output_text.as_deref().is_some_and(is_xai_async_spawn_ack);
                    let working = if raw_status == "failed" {
                        false
                    } else if async_ack {
                        true
                    } else {
                        raw_status != "completed"
                    };
                    let done = !working;
                    s.upsert_subagent(
                        &child_id,
                        non_empty_str(raw_input.get("subagent_type")),
                        non_empty_str(raw_input.get("description")),
                        non_empty_str(raw_input.get("model")),
                        Some(working),
                        Some(done),
                        None,
                        None,
                    );
                }
            } else if is_xai_get_output_tool(&resolved_title, &raw_input, &raw_output) {
                if let Some(child_id) = output_text
                    .as_deref()
                    .and_then(extract_xai_child_session_id)
                {
                    let result_obj = raw_output
                        .get("Result")
                        .or_else(|| raw_output.get("result"));
                    let structured_status = non_empty_str(result_obj.and_then(|r| r.get("status")))
                        .map(|v| v.to_ascii_lowercase());
                    let is_terminal = matches!(
                        structured_status.as_deref(),
                        Some("completed" | "success" | "failed" | "error")
                    ) || result_obj
                        .and_then(|r| r.get("exit_code"))
                        .and_then(Value::as_i64)
                        .is_some();
                    if is_terminal && s.subagents.contains_key(&child_id) {
                        s.upsert_subagent(
                            &child_id,
                            None,
                            None,
                            None,
                            Some(false),
                            Some(true),
                            None,
                            None,
                        );
                    }
                }
            } else if raw_status == "completed" {
                let lower_title = resolved_title.to_ascii_lowercase();
                let variant =
                    non_empty_str(raw_input.get("variant")).map(|v| v.to_ascii_lowercase());
                let is_kill = lower_title.contains("kill_command_or_subagent")
                    || variant.as_deref() == Some("kill")
                    || variant.as_deref() == Some("killtask")
                    || non_empty_str(raw_output.get("type")) == Some("KillTask");
                if is_kill {
                    let mut ids: Vec<String> = Vec::new();
                    if let Some(arr) = raw_input.get("task_ids").and_then(Value::as_array) {
                        for entry in arr {
                            if let Some(id) = non_empty_str(Some(entry)) {
                                ids.push(id.to_string());
                            }
                        }
                    }
                    if let Some(id) = non_empty_str(raw_input.get("task_id"))
                        .or_else(|| non_empty_str(raw_input.get("taskId")))
                    {
                        ids.push(id.to_string());
                    }
                    for tid in ids {
                        if let Some(bg) = s.background_tasks.get_mut(&tid) {
                            bg.running = false;
                        }
                        if s.subagents.contains_key(&tid) {
                            s.upsert_subagent(
                                &tid,
                                None,
                                None,
                                None,
                                Some(false),
                                Some(true),
                                None,
                                None,
                            );
                        }
                    }
                }
            }

            if !call_id.is_empty() {
                s.active_tools.insert(
                    call_id,
                    GrokToolInfo {
                        title: resolved_title.clone(),
                        raw_input: raw_input.clone(),
                        raw_output: raw_output.clone(),
                    },
                );
            }

            if !replaying {
                if matches!(raw_status, "completed" | "failed" | "cancelled") {
                    s.live_tool_label = None;
                    s.live_tool_detail = None;
                    s.live_tool_started_at = None;
                } else {
                    s.live_tool_label = Some(resolved_title);
                    if s.live_tool_started_at.is_none() {
                        s.live_tool_started_at = Some(now_iso_rfc3339());
                    }
                    let detail = non_empty_str(update.pointer("/locations/0/path"))
                        .or_else(|| non_empty_str(raw_input.get("command")))
                        .or_else(|| non_empty_str(raw_input.get("file_path")))
                        .or_else(|| non_empty_str(raw_input.get("path")))
                        .or_else(|| non_empty_str(raw_input.get("url")))
                        .or_else(|| non_empty_str(raw_input.get("query")))
                        .map(str::to_string);
                    if detail.is_some() {
                        s.live_tool_detail = detail;
                    }
                }
            }
        }
        "usage_update" => {
            let used = update.get("used").and_then(Value::as_u64);
            let size = update.get("size").and_then(Value::as_u64);
            if used.is_some() || size.is_some() {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(u) = used.filter(|n| *n > 0) {
                    s.usage_used_tokens = Some(u);
                }
                if let Some(sz) = size.filter(|n| *n > 0) {
                    s.usage_window_size = Some(sz);
                }
            }
        }
        "session_info_update" => {
            if let Some(title) = non_empty_str(update.get("title")) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.session_title = Some(title.to_string());
            }
        }
        _ => {}
    }
}
