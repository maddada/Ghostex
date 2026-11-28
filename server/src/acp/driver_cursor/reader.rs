//! Stdout JSON-RPC reader loop handling Cursor ACP responses, incoming permission/question/plan
//! requests, and streaming session update notifications.

use std::io::{BufRead, BufReader, Write};
use std::process::ChildStdout;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

use super::permissions::{parse_cursor_ask_question_request, parse_cursor_permission_request};
use super::transport_and_state::{
    CursorActivityReporter, CursorState, CursorTransport, ReaderExitGuard,
};
use crate::acp::approval::{evaluate_permission, AcpApprovalLevel, PermissionDecision};
use crate::acp::host::{
    append_bounded, end_live_segment, now_iso_rfc3339, now_ms, LIVE_STREAM_MAX_CHARS,
};

pub(super) fn spawn_stdout_reader(
    stdout: ChildStdout,
    t_out: Arc<CursorTransport>,
    st_out: Arc<Mutex<CursorState>>,
    act_out: CursorActivityReporter,
) {
    thread::spawn(move || {
        let _exit_guard = ReaderExitGuard(st_out.clone());
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let handle_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return;
                }
                let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                    return;
                };
                t_out.spool_event(&msg);
                {
                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                    s.last_event_at_ms = now_ms();
                }
                let has_method = msg.get("method").and_then(Value::as_str).is_some();
                let has_id = msg.get("id").is_some();

                // Response to our outgoing JSON-RPC call.
                if !has_method && has_id {
                    if let Some(id) = msg.get("id").and_then(Value::as_u64) {
                        if let Some(tx) = t_out
                            .pending
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&id)
                        {
                            let _ = tx.send(msg);
                        }
                    }
                    return;
                }

                // Incoming JSON-RPC request from Cursor ACP server.
                if has_method && has_id {
                    let method = msg
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
                    let params = msg.get("params").cloned().unwrap_or(Value::Null);
                    match method {
                        "session/request_permission" => {
                            let (mut req, eval_params) =
                                parse_cursor_permission_request(rpc_id, &params);
                            if !req.perm.is_question {
                                let (level, roots, session_cwd, read_only) = {
                                    let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    (
                                        s.approval_level,
                                        s.writable_roots.clone(),
                                        s.cwd.clone(),
                                        s.read_only,
                                    )
                                };
                                let effective_level = if read_only {
                                    AcpApprovalLevel::Ask
                                } else {
                                    level
                                };
                                match evaluate_permission(
                                    effective_level,
                                    &eval_params,
                                    &req.perm.options,
                                    &roots,
                                    &session_cwd,
                                ) {
                                    PermissionDecision::AutoApprove { option_id } => {
                                        println!(
                                            "\r\n[Native] Auto-approved ({}): {} ({})",
                                            effective_level.label(),
                                            req.perm.tool_name,
                                            req.perm.summary
                                        );
                                        let _ = std::io::stdout().flush();
                                        let _ = t_out.respond_to_agent(
                                            req.perm.rpc_id,
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
                                        req.perm.reason = reason;
                                    }
                                }
                            }
                            println!(
                                "\r\n[Native] Waiting for {}: {} ({})",
                                if req.perm.is_question {
                                    "question answer"
                                } else {
                                    "approval"
                                },
                                req.perm.tool_name,
                                req.perm.summary
                            );
                            let _ = std::io::stdout().flush();
                            let was_empty = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                let empty = s.pending_requests.is_empty();
                                s.pending_requests.push_back(req);
                                empty
                            };
                            if was_empty {
                                act_out.sync();
                            }
                        }
                        "cursor/ask_question" => {
                            let req = parse_cursor_ask_question_request(rpc_id, &params);
                            println!(
                                "\r\n[Native] Waiting for question answer: {}",
                                req.perm.tool_name
                            );
                            let _ = std::io::stdout().flush();
                            let was_empty = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                let empty = s.pending_requests.is_empty();
                                s.pending_requests.push_back(req);
                                empty
                            };
                            if was_empty {
                                act_out.sync();
                            }
                        }
                        "cursor/create_plan" => {
                            if let Some(plan) = params
                                .get("plan")
                                .and_then(Value::as_str)
                                .filter(|p| !p.trim().is_empty())
                            {
                                let name = params.get("name").and_then(Value::as_str);
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.transcript.write_plan_markdown(name, plan);
                            }
                            // Returning -32601 makes Cursor write the local plan file and
                            // complete the CreatePlan tool call cleanly.
                            t_out.respond_error_to_agent(
                                rpc_id,
                                -32601,
                                "Method not supported: cursor/create_plan",
                            );
                        }
                        _ => {
                            t_out.respond_error_to_agent(
                                rpc_id,
                                -32601,
                                &format!("Unsupported client method: {method}"),
                            );
                        }
                    }
                    return;
                }

                // JSON-RPC notification from Cursor ACP server.
                if let Some(update) = msg.pointer("/params/update") {
                    let kind = update
                        .get("sessionUpdate")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    match kind {
                        "current_mode_update" => {
                            if let Some(mode_id) = update
                                .get("currentModeId")
                                .or_else(|| update.get("modeId"))
                                .and_then(Value::as_str)
                            {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.current_mode = Some(mode_id.to_string());
                            }
                        }
                        "config_option_update" => {
                            if let Some(opts) = update.get("configOptions") {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.sync_from_config_options(opts);
                            }
                        }
                        "user_message_chunk" => {
                            if let Some(text) = update
                                .pointer("/content/text")
                                .or_else(|| update.get("text"))
                                .and_then(Value::as_str)
                                .filter(|t| !t.trim().is_empty())
                            {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.transcript.write_user_message(text);
                            }
                        }
                        "agent_message_chunk" => {
                            if let Some(text) = update
                                .pointer("/content/text")
                                .or_else(|| update.get("text"))
                                .and_then(Value::as_str)
                                .filter(|t| !t.is_empty())
                            {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if s.live_stream_started_at.is_none() {
                                    s.live_stream_started_at = Some(now_iso_rfc3339());
                                }
                                s.transport_failure.push(text);
                                append_bounded(
                                    &mut s.live_stream_text,
                                    text,
                                    LIVE_STREAM_MAX_CHARS,
                                );
                                s.live_tool_label = None;
                                s.live_tool_detail = None;
                                s.live_tool_started_at = None;
                                s.transcript.push_text(text);
                            }
                        }
                        "agent_thought_chunk" => {
                            if let Some(text) = update
                                .pointer("/content/text")
                                .or_else(|| update.get("text"))
                                .and_then(Value::as_str)
                                .filter(|t| !t.is_empty())
                            {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if s.live_thought_started_at.is_none() {
                                    s.live_thought_started_at = Some(now_iso_rfc3339());
                                }
                                append_bounded(
                                    &mut s.live_thought_text,
                                    text,
                                    LIVE_STREAM_MAX_CHARS,
                                );
                                s.transcript.push_thought(text);
                            }
                        }
                        "tool_call" | "tool_call_update" => {
                            let status = update
                                .get("status")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            if kind == "tool_call" {
                                end_live_segment!(s);
                            }
                            s.transcript.handle_tool_update(update);
                            if matches!(status, "completed" | "failed" | "cancelled") {
                                s.live_tool_label = None;
                                s.live_tool_detail = None;
                                s.live_tool_started_at = None;
                            } else {
                                let label = update
                                    .get("title")
                                    .or_else(|| update.get("name"))
                                    .or_else(|| update.get("kind"))
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|t| !t.is_empty())
                                    .map(str::to_string);
                                if let Some(label) = label {
                                    s.live_tool_label = Some(label);
                                }
                                if s.live_tool_started_at.is_none() {
                                    s.live_tool_started_at = Some(now_iso_rfc3339());
                                }
                                let detail = update
                                    .pointer("/locations/0/path")
                                    .or_else(|| update.pointer("/rawInput/command"))
                                    .or_else(|| update.pointer("/rawInput/path"))
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|t| !t.is_empty())
                                    .map(str::to_string);
                                if detail.is_some() {
                                    s.live_tool_detail = detail;
                                }
                            }
                        }
                        "plan" => {
                            if let Some(entries) = update.get("entries").and_then(Value::as_array) {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.transcript.write_plan_todos(entries);
                            }
                        }
                        "subagent_spawned" => {
                            let name = update
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("Subagent");
                            let task = update.get("task").and_then(Value::as_str).unwrap_or("");
                            let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                            s.live_tool_label = Some(format!("Subagent: {name}"));
                            if !task.trim().is_empty() {
                                s.live_tool_detail = Some(task.chars().take(120).collect());
                            }
                            if s.live_tool_started_at.is_none() {
                                s.live_tool_started_at = Some(now_iso_rfc3339());
                            }
                        }
                        "usage_update" => {
                            let used = update.get("used").and_then(Value::as_u64);
                            let size = update.get("size").and_then(Value::as_u64);
                            if used.is_some() || size.is_some() {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if let Some(u) = used {
                                    s.usage_used_tokens = Some(u);
                                }
                                if let Some(sz) = size {
                                    s.usage_window_size = Some(sz);
                                }
                            }
                        }
                        "session_info_update" => {
                            if let Some(title) = update
                                .get("title")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|t| !t.is_empty())
                            {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                s.session_title = Some(title.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }));
            if handle_res.is_err() {
                eprintln!(
                    "[Native] Warning: recovered from panic while handling Cursor stdout message."
                );
            }
        }
    });
}
