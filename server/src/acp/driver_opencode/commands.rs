//! Host channel command handlers (`state`, `abort`, `approve`, `answer_question`, `model`, `mode`, `rewind`, `send`/`steer`, `switch_to_cli`).

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::super::approval::AcpApprovalLevel;
use super::super::host::{now_iso_rfc3339, GxserverEndpoint};
use super::approvals::{
    apply_approval_level, build_form_reply_answers, build_session_rules, cancel_all_pending,
    PendingOpenCodeKind,
};
use super::events::reconcile_session;
use super::http::is_auth_error;
use super::process::{
    handle_rewind, interrupt_active_turn, stop_opencode_gracefully, OpenCodeProcess,
};
use super::state::{
    build_opencode_file_attachments, extract_skill_references, parse_model_reference,
    OpenCodeActivityReporter, OpenCodeState,
};
use super::{
    DECLINED_PERMISSION_MESSAGE, DECLINED_SYNTHETIC_NOTE, HTTP_LONG_TIMEOUT, HTTP_TIMEOUT,
    SHELL_TIMEOUT,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_command(
    ep: &GxserverEndpoint,
    id: &str,
    kind: &str,
    params: Value,
    proc_opt: &mut Option<OpenCodeProcess>,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    gx_paths: &crate::paths::GxserverPaths,
    cwd: &str,
    shutting_down: &Arc<AtomicBool>,
    process_gen: &Arc<AtomicU64>,
    child_pid: &Arc<AtomicU32>,
) -> bool {
    match kind {
        "state" => {
            let snap = state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .snapshot_json();
            ep.ack(id, true, snap, None);
        }
        "abort" => {
            if let Some(proc) = proc_opt.as_ref() {
                interrupt_active_turn(&proc.http, &state, &cwd);
                reconcile_session(&proc.http, &state, &activity, true);
            }
            activity.sync();
            ep.ack(id, true, json!({ "aborted": true }), None);
        }
        "approve" => {
            let Some(proc) = proc_opt.as_ref() else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode server is not running."),
                );
                return false;
            };
            let decision = params
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("allow");
            let target_tool_use_id = params
                .get("toolUseId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let req = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(wanted) = target_tool_use_id {
                    s.pending_requests
                        .iter()
                        .position(|p| !p.is_question() && p.request_id == wanted)
                        .and_then(|idx| s.pending_requests.remove(idx))
                } else {
                    s.pending_requests
                        .iter()
                        .position(|p| !p.is_question())
                        .and_then(|idx| s.pending_requests.remove(idx))
                }
            };
            let Some(req) = req else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("No pending OpenCode approval."),
                );
                return false;
            };
            let norm = decision.trim().to_ascii_lowercase();
            let is_cancel = matches!(norm.as_str(), "escape" | "cancel" | "\x1b");
            let is_deny = matches!(
                norm.as_str(),
                "deny" | "reject" | "reject_once" | "reject_always" | "decline" | "n" | "no"
            );
            let is_session_grant = matches!(
                norm.as_str(),
                "always" | "allow_always" | "accept_for_session" | "2"
            );

            let res = if is_cancel {
                proc.http.session(
                    &req.session_id,
                    &format!("/permission/{}/reply", req.request_id),
                    "POST",
                    Some(&json!({ "decision": "reject" })),
                    HTTP_TIMEOUT,
                )
            } else if is_deny {
                // Steer a synthetic declined note before rejecting so the model continues
                // gracefully without retrying the declined tool call.
                let _ = proc.http.session(
                    &req.session_id,
                    "/synthetic",
                    "POST",
                    Some(&json!({
                        "text": DECLINED_SYNTHETIC_NOTE,
                        "delivery": "steer",
                        "resume": false,
                    })),
                    HTTP_TIMEOUT,
                );
                proc.http.session(
                    &req.session_id,
                    &format!("/permission/{}/reply", req.request_id),
                    "POST",
                    Some(&json!({
                        "decision": "reject",
                        "message": DECLINED_PERMISSION_MESSAGE,
                    })),
                    HTTP_TIMEOUT,
                )
            } else {
                if is_session_grant {
                    if let PendingOpenCodeKind::Permission {
                        ref action,
                        ref resources,
                    } = req.kind
                    {
                        let (main_sid, rules) = {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            if resources.is_empty() {
                                s.session_grants.push((action.clone(), "*".to_string()));
                            } else {
                                for r in resources {
                                    if !s
                                        .session_grants
                                        .iter()
                                        .any(|(a, res)| a == action && res == r)
                                    {
                                        s.session_grants.push((action.clone(), r.clone()));
                                    }
                                }
                            }
                            let rules = build_session_rules(
                                s.approval_level,
                                s.read_only,
                                &s.plan_agent_paths,
                                &s.session_grants,
                            );
                            (s.session_id.clone(), rules)
                        };
                        if let Some(msid) = main_sid {
                            let _ = proc.http.session(
                                &msid,
                                "",
                                "PATCH",
                                Some(&json!({ "permissions": rules })),
                                HTTP_TIMEOUT,
                            );
                        }
                    }
                }
                // Always reply "once" (never OpenCode's persistent "always") so no
                // project-wide permission is silently saved to disk.
                proc.http.session(
                    &req.session_id,
                    &format!("/permission/{}/reply", req.request_id),
                    "POST",
                    Some(&json!({ "decision": "once" })),
                    HTTP_TIMEOUT,
                )
            };

            match res {
                Ok(_) => {
                    activity.sync();
                    ep.ack(id, true, json!({ "answered": true }), None);
                }
                Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
            }
        }
        "answer_question" => {
            let Some(proc) = proc_opt.as_ref() else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode server is not running."),
                );
                return false;
            };
            let selections = params
                .get("selections")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let target_tool_use_id = params
                .get("toolUseId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let req = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(wanted) = target_tool_use_id {
                    s.pending_requests
                        .iter()
                        .position(|p| p.is_question() && p.request_id == wanted)
                        .and_then(|idx| s.pending_requests.remove(idx))
                } else {
                    s.pending_requests
                        .iter()
                        .position(|p| p.is_question())
                        .and_then(|idx| s.pending_requests.remove(idx))
                }
            };
            let Some(req) = req else {
                ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                return false;
            };
            let PendingOpenCodeKind::Form { ref form } = req.kind else {
                ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                return false;
            };
            let body = match build_form_reply_answers(form, &selections) {
                Ok(body) => body,
                Err(err) => {
                    state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .pending_requests
                        .push_front(req);
                    ep.ack(id, false, Value::Null, Some(&err));
                    return false;
                }
            };
            match proc.http.session(
                &req.session_id,
                &format!("/form/{}/reply", req.request_id),
                "POST",
                Some(&body),
                HTTP_TIMEOUT,
            ) {
                Ok(_) => {
                    activity.sync();
                    ep.ack(id, true, json!({ "answered": true }), None);
                }
                Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
            }
        }
        "model" => {
            let Some(proc) = proc_opt.as_ref() else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode server is not running."),
                );
                return false;
            };
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let effort = params
                .get("effort")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let sid = state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .session_id
                .clone();
            let Some(sid) = sid else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode session not initialized."),
                );
                return false;
            };
            let effective_model = if !model.is_empty() {
                model
            } else {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .current_model
                    .clone()
                    .unwrap_or_default()
            };
            let Some((provider_id, model_id)) = parse_model_reference(&effective_model) else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("An OpenCode model must include its provider (provider/model)."),
                );
                return false;
            };
            let validated_variant = {
                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                let req_eff = if effort.is_empty() {
                    None
                } else {
                    Some(effort.as_str())
                };
                s.validated_variant(&effective_model, req_eff)
            };
            let mut model_ref = json!({
                "providerID": provider_id,
                "id": model_id,
            });
            if let Some(ref v) = validated_variant {
                model_ref["variant"] = json!(v);
            }
            match proc.http.session(
                &sid,
                "/model",
                "POST",
                Some(&json!({ "model": model_ref })),
                HTTP_TIMEOUT,
            ) {
                Ok(_) => {
                    let snap = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.current_model = Some(effective_model);
                        s.current_effort = validated_variant;
                        s.update_window_for_current_model();
                        s.snapshot_json()
                    };
                    ep.ack(id, true, snap, None);
                }
                Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
            }
        }
        "mode" => {
            let Some(proc) = proc_opt.as_ref() else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode server is not running."),
                );
                return false;
            };
            let mode = params
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(level) = AcpApprovalLevel::parse(mode) else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some(&format!("Unsupported approval level: {mode}")),
                );
                return false;
            };
            if let Err(err) = apply_approval_level(&proc.http, &state, &activity, level) {
                ep.ack(id, false, Value::Null, Some(&err));
                return false;
            }
            let snap = state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .snapshot_json();
            ep.ack(id, true, snap, None);
        }
        "rewind" => {
            let Some(proc) = proc_opt.as_ref() else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode server is not running."),
                );
                return false;
            };
            let message_id = params
                .get("messageId")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            if message_id.is_empty() {
                ep.ack(id, false, Value::Null, Some("REWIND_NOT_FOUND"));
                return false;
            }
            match handle_rewind(&proc.http, &state, &activity, &message_id) {
                Ok(result) => ep.ack(id, true, result, None),
                Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
            }
        }
        "steer" | "send" => {
            let Some(proc) = proc_opt.as_ref() else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode server is not running."),
                );
                return false;
            };
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let images: Vec<String> = params
                .get("images")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let interrupt = params
                .get("interrupt")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let files = match build_opencode_file_attachments(&gx_paths, &text, &images) {
                Ok(f) => f,
                Err(err) => {
                    ep.ack(id, false, Value::Null, Some(&err));
                    return false;
                }
            };

            let (sid, is_working, has_pending, known_commands, known_skills) = {
                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                (
                    s.session_id.clone(),
                    s.working,
                    !s.pending_requests.is_empty(),
                    s.known_commands.clone(),
                    s.known_skills.clone(),
                )
            };
            let Some(sid) = sid else {
                ep.ack(
                    id,
                    false,
                    Value::Null,
                    Some("OpenCode session not initialized."),
                );
                return false;
            };

            if has_pending && !interrupt {
                ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                return false;
            }

            let skills = extract_skill_references(&text, &known_skills);

            // Mid-turn steer without cancelling the active turn.
            if is_working && !interrupt {
                let mut steer_body = json!({
                    "text": text,
                    "files": files,
                    "delivery": "steer",
                });
                if !skills.is_empty() {
                    steer_body["skills"] = Value::Array(skills.clone());
                }
                match proc
                    .http
                    .session(&sid, "/prompt", "POST", Some(&steer_body), HTTP_TIMEOUT)
                {
                    Ok(_) => {
                        println!("\r\n[OpenCode Native] Steered active turn...");
                        let _ = std::io::stdout().flush();
                        ep.ack(
                            id,
                            true,
                            json!({
                                "accepted": true,
                                "midTurn": true,
                                "steered": true,
                                "sessionId": sid,
                            }),
                            None,
                        );
                        return false;
                    }
                    Err(err) => {
                        // Wait briefly in case the turn just finished; otherwise report busy.
                        thread::sleep(Duration::from_millis(250));
                        let still_working = state.lock().unwrap_or_else(|p| p.into_inner()).working;
                        if still_working {
                            eprintln!(
                                "[OpenCode Native] Warning: steer failed while active: {err}"
                            );
                            ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                            return false;
                        }
                    }
                }
            }

            if is_working && interrupt {
                interrupt_active_turn(&proc.http, &state, &cwd);
            }
            cancel_all_pending(&proc.http, &state);

            let trimmed = text.trim();

            // `!cmd` runs an OpenCode shell command asynchronously so the poll loop stays
            // responsive to `abort` and `state`.
            if let Some(shell_cmd) = text.strip_prefix('!') {
                let shell_cmd = shell_cmd.trim_start().to_string();
                if shell_cmd.is_empty() {
                    ep.ack(
                        id,
                        false,
                        Value::Null,
                        Some("Enter a shell command after !."),
                    );
                    return false;
                }
                {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.working = true;
                    s.last_turn_interrupted = false;
                    s.last_turn_stop_error = None;
                    s.last_error = None;
                    s.clear_live_turn_output();
                }
                activity.sync();
                let http_bg = proc.http.clone();
                let sid_bg = sid.clone();
                let state_bg = state.clone();
                let act_bg = activity.clone();
                thread::spawn(move || {
                    let res = http_bg.session(
                        &sid_bg,
                        "/shell",
                        "POST",
                        Some(&json!({ "command": shell_cmd })),
                        SHELL_TIMEOUT,
                    );
                    if let Err(err) = res {
                        let mut s = state_bg.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = false;
                        s.last_turn_stop_error = Some("error".to_string());
                        s.last_error = Some((None, err));
                    }
                    reconcile_session(&http_bg, &state_bg, &act_bg, false);
                });
                ep.ack(
                    id,
                    true,
                    json!({ "accepted": true, "sessionId": sid }),
                    None,
                );
                return false;
            }

            // Local slash commands: `/compact`, `/model`, `/agent`, or server-listed commands.
            if let Some((command, argument)) =
                crate::session_chat_local_command::parse_session_chat_local_command(&text)
            {
                let cmd_name = command.trim_start_matches('/');
                match cmd_name {
                    "compact" if images.is_empty() => {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            s.compacting = true;
                            s.compacting_started_at = Some(now_iso_rfc3339());
                            s.last_turn_interrupted = false;
                            s.last_turn_stop_error = None;
                            s.last_error = None;
                            s.clear_live_turn_output();
                        }
                        activity.sync();
                        println!("\r\n[OpenCode Native] Compacting session...");
                        let _ = std::io::stdout().flush();
                        match proc.http.session(
                            &sid,
                            "/compact",
                            "POST",
                            Some(&json!({})),
                            HTTP_LONG_TIMEOUT,
                        ) {
                            Ok(_) => {
                                ep.ack(
                                            id,
                                            true,
                                            json!({ "accepted": true, "compacting": true, "sessionId": sid }),
                                            None,
                                        );
                            }
                            Err(err) => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.working = false;
                                    s.compacting = false;
                                    s.compacting_started_at = None;
                                    s.last_turn_stop_error = Some("error".to_string());
                                    s.last_error = Some((None, err.clone()));
                                }
                                activity.sync();
                                ep.ack(id, false, Value::Null, Some(&err));
                            }
                        }
                        return false;
                    }
                    "model" if images.is_empty() => {
                        let mut words = argument.split_whitespace();
                        let raw_model = words.next().unwrap_or_default();
                        let raw_variant = words.next();
                        if raw_model.is_empty() || words.next().is_some() {
                            ep.ack(
                                        id,
                                        false,
                                        Value::Null,
                                        Some("Use /model provider/model [variant], or pick from the model menu."),
                                    );
                            return false;
                        }
                        let Some((provider_id, model_id)) = parse_model_reference(raw_model) else {
                            ep.ack(
                                id,
                                false,
                                Value::Null,
                                Some(
                                    "An OpenCode model must include its provider (provider/model).",
                                ),
                            );
                            return false;
                        };
                        let variant = state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .validated_variant(raw_model, raw_variant);
                        let mut model_ref = json!({
                            "providerID": provider_id,
                            "id": model_id,
                        });
                        if let Some(ref v) = variant {
                            model_ref["variant"] = json!(v);
                        }
                        match proc.http.session(
                            &sid,
                            "/model",
                            "POST",
                            Some(&json!({ "model": model_ref })),
                            HTTP_TIMEOUT,
                        ) {
                            Ok(_) => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.current_model = Some(raw_model.to_string());
                                    s.current_effort = variant;
                                    s.update_window_for_current_model();
                                }
                                ep.ack(
                                    id,
                                    true,
                                    json!({ "accepted": true, "sessionId": sid }),
                                    None,
                                );
                            }
                            Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                        }
                        return false;
                    }
                    "agent" if images.is_empty() => {
                        let target_agent = argument.trim();
                        if target_agent.is_empty() {
                            ep.ack(
                                id,
                                false,
                                Value::Null,
                                Some("Use /agent build or /agent plan."),
                            );
                            return false;
                        }
                        match proc.http.session(
                            &sid,
                            "/agent",
                            "POST",
                            Some(&json!({ "agent": target_agent })),
                            HTTP_TIMEOUT,
                        ) {
                            Ok(_) => {
                                let rules = {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.current_agent = target_agent.to_string();
                                    s.read_only = target_agent.eq_ignore_ascii_case("plan");
                                    build_session_rules(
                                        s.approval_level,
                                        s.read_only,
                                        &s.plan_agent_paths,
                                        &s.session_grants,
                                    )
                                };
                                let _ = proc.http.session(
                                    &sid,
                                    "",
                                    "PATCH",
                                    Some(&json!({ "permissions": rules })),
                                    HTTP_TIMEOUT,
                                );
                                ep.ack(
                                    id,
                                    true,
                                    json!({ "accepted": true, "sessionId": sid }),
                                    None,
                                );
                            }
                            Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                        }
                        return false;
                    }
                    other if known_commands.contains(other) => {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.working = true;
                            s.compacting = false;
                            s.compacting_started_at = None;
                            s.last_turn_interrupted = false;
                            s.last_turn_stop_error = None;
                            s.last_error = None;
                            s.last_auth_error = None;
                            s.clear_live_turn_output();
                        }
                        activity.sync();
                        let body = json!({
                            "name": other,
                            "text": argument,
                            "files": files,
                        });
                        match proc.http.session(
                            &sid,
                            "/command",
                            "POST",
                            Some(&body),
                            HTTP_LONG_TIMEOUT,
                        ) {
                            Ok(_) => {
                                reconcile_session(&proc.http, &state, &activity, false);
                                ep.ack(
                                    id,
                                    true,
                                    json!({ "accepted": true, "sessionId": sid }),
                                    None,
                                );
                            }
                            Err(err) => {
                                {
                                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                    s.working = false;
                                    s.last_turn_stop_error = Some("error".to_string());
                                    s.last_error = Some((None, err.clone()));
                                }
                                activity.sync();
                                ep.ack(id, false, Value::Null, Some(&err));
                            }
                        }
                        return false;
                    }
                    _ => {}
                }
            }

            // Standard prompt delivery.
            if trimmed.is_empty() && files.is_empty() {
                ep.ack(id, false, Value::Null, Some("Prompt cannot be empty."));
                return false;
            }
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.working = true;
                s.compacting = false;
                s.compacting_started_at = None;
                s.last_turn_interrupted = false;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
                s.clear_live_turn_output();
            }
            activity.sync();
            println!("\r\n[OpenCode Native] Turn started...");
            let _ = std::io::stdout().flush();

            let mut prompt_body = json!({
                "text": text,
                "files": files,
            });
            if !skills.is_empty() {
                prompt_body["skills"] = Value::Array(skills);
            }
            match proc.http.session(
                &sid,
                "/prompt",
                "POST",
                Some(&prompt_body),
                HTTP_LONG_TIMEOUT,
            ) {
                Ok(_) => {
                    if let Ok(msgs) = proc.http.fetch_messages(&sid) {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.messages = msgs;
                        s.write_mirror_now();
                    }
                    ep.ack(
                        id,
                        true,
                        json!({ "accepted": true, "sessionId": sid }),
                        None,
                    );
                }
                Err(err) => {
                    let is_auth = is_auth_error(None, &err);
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = false;
                        if is_auth {
                            s.last_auth_error = Some(err.clone());
                        }
                        s.last_turn_stop_error = Some("error".to_string());
                        s.last_error = Some((None, err.clone()));
                    }
                    activity.sync();
                    ep.ack(id, false, Value::Null, Some(&err));
                }
            }
        }
        "switch_to_cli" => {
            let interrupt = params
                .get("interrupt")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let (is_working, has_pending, sid, carry_model, carry_effort) = {
                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                (
                    s.working,
                    !s.pending_requests.is_empty(),
                    s.session_id.clone(),
                    s.current_model.clone(),
                    s.current_effort.clone(),
                )
            };
            if (is_working || has_pending) && !interrupt {
                ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("OpenCode is working on a turn. Wait or interrupt before switching to Terminal."),
                        );
                return false;
            }
            if let Some(proc) = proc_opt.as_ref() {
                cancel_all_pending(&proc.http, &state);
                if is_working {
                    interrupt_active_turn(&proc.http, &state, &cwd);
                }
            }
            shutting_down.store(true, Ordering::Release);
            process_gen.fetch_add(1, Ordering::AcqRel);
            if let Some(mut old) = proc_opt.take() {
                stop_opencode_gracefully(&mut old.child, &mut old.lifetime, &child_pid);
            }
            ep.ack(
                id,
                true,
                json!({
                    "switched": true,
                    "nativeSessionId": sid,
                    "model": carry_model,
                    "effort": carry_effort,
                }),
                None,
            );
            ep.close_channel();
            return true;
        }
        other => {
            ep.ack(
                id,
                false,
                Value::Null,
                Some(&format!("Unsupported OpenCode Native command: {other}")),
            );
        }
    }
    false
}
