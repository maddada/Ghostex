//! Main `/api/agentChatChannel` event loop and prompt watcher for `ghostex acp-host --agent grok`.

use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use crate::acp::approval::{canonicalize_writable_roots, AcpApprovalLevel};
use crate::acp::host::{
    exec_interactive_cli, now_ms, GxserverEndpoint, HostArgs, TerminationSignalGuard,
    POLL_RETRY_DELAY,
};
use crate::acp::spec::{build_acp_server_argv, AcpAgentSpec};

use super::approvals::{
    build_xai_ask_user_question_response, cancel_all_pending_requests,
    respond_pending_request_cancelled,
};
use super::models::apply_grok_model_and_effort;
use super::process::{spawn_and_initialize_grok, stop_grok_process};
use super::protocol::{
    build_grok_interject_image_blocks, build_grok_prompt_blocks, non_empty_str,
    rewrite_leading_grok_skill_mention, CachedGrokSkills,
};
use super::state::{
    GrokLaunchMode, GrokPendingRequest, GrokPromptOutcome, GrokState, PendingPromptKind,
};
use super::transport::{GrokActivityReporter, GrokProcess, GrokTransport};
use super::{GROK_RPC_TIMEOUT, XAI_INTERJECT_METHOD, XAI_RATE_LIMITED_ERROR_CODE};

/// Entry point for `ghostex acp-host --agent grok`.
pub(in crate::acp) fn run(spec: &AcpAgentSpec, parsed: HostArgs) -> i32 {
    let cwd_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd = cwd_path.to_string_lossy().to_string();
    let writable_roots = canonicalize_writable_roots(&cwd_path, &parsed.writable_roots);
    let endpoint = GxserverEndpoint::from_env_or_secret_file(parsed.channel_secret_file.as_deref());
    let spool_session_id = endpoint
        .as_ref()
        .map(|ep| ep.session_id.clone())
        .unwrap_or_else(|| format!("local-{}", std::process::id()));

    println!("Ghostex Native Host · {} · Starting...", spec.display_name);
    let _ = std::io::stdout().flush();

    let gx_paths = crate::paths::get_gxserver_paths(None);
    let signal_guard = TerminationSignalGuard::install();

    // Keep default (no `--approval-level`): start in the mode Grok's own `[ui] permission_mode`
    // starts its TUI in, since `grok agent` alone always starts in ask (CDXC:NativeApprovals
    // 2026-11-25 in `acp/cli_default.rs`).
    let initial_level = parsed
        .approval_level
        .or_else(|| parsed.read_only.then_some(AcpApprovalLevel::Ask))
        .or_else(|| crate::acp::cli_default::cli_default_level(spec.family))
        .unwrap_or(AcpApprovalLevel::Ask);
    let initial_launch_mode = GrokLaunchMode::from_approval_level(initial_level, parsed.read_only);

    let state = Arc::new(Mutex::new(GrokState::new(
        spec.family,
        initial_level,
        initial_launch_mode,
        parsed.read_only,
        cwd_path.clone(),
        writable_roots,
    )));
    let activity =
        GrokActivityReporter::start(endpoint.clone(), state.clone(), spec.family.to_string());
    let record_target = endpoint
        .as_ref()
        .map(crate::acp::child_lifetime::ChildRecordTarget::from_endpoint);
    let child_pid = Arc::new(AtomicU32::new(0));
    let shutting_down = Arc::new(AtomicBool::new(false));
    let intentional_stop = Arc::new(AtomicBool::new(false));
    let skills_cache = Mutex::new(None::<CachedGrokSkills>);
    let grok_bin = build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
        .map(|(_, prog, _)| prog)
        .unwrap_or_else(|| "grok".to_string());

    let mut proc = match spawn_and_initialize_grok(
        spec,
        &parsed,
        initial_launch_mode,
        parsed.resume_session_id.as_deref(),
        &cwd,
        &spool_session_id,
        record_target.as_ref(),
        &child_pid,
        &state,
        &activity,
        &intentional_stop,
    ) {
        Ok(proc) => proc,
        Err(err) => {
            eprintln!("{err}");
            crate::acp::host::report_driver_startup_error(
                endpoint.as_ref(),
                spec.family,
                &err,
                crate::acp::startup_error::looks_like_auth_error(&err),
            );
            return 1;
        }
    };

    // Apply initial model / effort selection if requested.
    if parsed.model.is_some() || parsed.effort.is_some() {
        if let Err(err) = apply_grok_model_and_effort(
            &proc.transport,
            &state,
            parsed.model.as_deref(),
            parsed.effort.as_deref(),
        ) {
            eprintln!("[Grok Native] Warning: initial model selection failed: {err}");
        }
    }

    // Watch for SIGTERM/SIGHUP/SIGINT so the Grok subprocess is stopped promptly.
    #[cfg(unix)]
    {
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if crate::acp::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
                    let pid = pid_sig.swap(0, Ordering::AcqRel);
                    crate::acp::child_lifetime::terminate_child_on_host_signal(
                        pid,
                        target_sig.as_ref(),
                    );
                    if let Some(ep) = ep_sig.as_ref() {
                        ep.close_channel();
                    }
                    std::process::exit(143);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    let native_session_id = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .native_session_id
        .clone();
    println!(
        "Ghostex Native Host · {} · Ready (conversation: {})",
        spec.display_name,
        native_session_id.as_deref().unwrap_or("pending")
    );
    println!("Use Chat View to talk with this agent, or switch to Terminal to open Terminal mode.");
    let _ = std::io::stdout().flush();

    activity.sync();

    let next_prompt_seq = AtomicU64::new(1);
    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
        "steer",
        "mid_turn_send",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "switch_to_cli",
    ]);
    let channel_agent = format!("acp:{}", spec.family);
    let mut reconnecting = false;

    let recycle_grok_subprocess = |proc: &mut GrokProcess,
                                   target_launch_mode: GrokLaunchMode,
                                   force_kill: bool|
     -> Result<(), String> {
        let (sid, cur_model, cur_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.native_session_id.clone(),
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        cancel_all_pending_requests(&proc.transport, &state);
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.abort_all_prompt_waiters();
            s.working = false;
            s.wake_prompt_id = None;
            s.clear_live_turn_output();
        }
        stop_grok_process(proc, &child_pid, &intentional_stop, force_kill);
        intentional_stop.store(false, Ordering::Release);
        let new_proc = spawn_and_initialize_grok(
            spec,
            &parsed,
            target_launch_mode,
            sid.as_deref(),
            &cwd,
            &spool_session_id,
            record_target.as_ref(),
            &child_pid,
            &state,
            &activity,
            &intentional_stop,
        )?;
        *proc = new_proc;
        if cur_model.is_some() || cur_effort.is_some() {
            let _ = apply_grok_model_and_effort(
                &proc.transport,
                &state,
                cur_model.as_deref(),
                cur_effort.as_deref(),
            );
        }
        Ok(())
    };

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            println!("\r\n[Native] Agent process exited.");
            break;
        }
        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };

        let mut poll_params = json!({
            "action": "poll",
            "sessionRef": ep.session_ref,
            "agent": channel_agent,
            "pid": std::process::id(),
            "instance": ep.instance,
            "version": 1,
            "capabilities": capabilities,
        });
        if let Some(secret) = ep.channel_secret.as_deref() {
            poll_params["channelSecret"] = json!(secret);
        }
        let poll_body = json!({
            "requestId": format!("acp-poll-{}", now_ms()),
            "params": poll_params,
        });

        let resp = match ep.post_json("/api/agentChatChannel", poll_body, Duration::from_secs(28)) {
            Ok(resp) => {
                let newly_registered = resp
                    .pointer("/result/newlyRegistered")
                    .or_else(|| resp.get("newlyRegistered"))
                    .and_then(Value::as_bool)
                    == Some(true);
                let mut had_desired_approval = false;
                if let Some(desired) = resp
                    .pointer("/result/desiredApprovalLevel")
                    .or_else(|| resp.get("desiredApprovalLevel"))
                    .and_then(Value::as_str)
                    .and_then(AcpApprovalLevel::parse)
                {
                    had_desired_approval = true;
                    let (current, is_working, spawned_mode, read_only) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.approval_level,
                            s.working,
                            s.spawned_launch_mode,
                            s.read_only,
                        )
                    };
                    if desired != current {
                        let target_launch = GrokLaunchMode::from_approval_level(desired, read_only);
                        if target_launch == spawned_mode {
                            state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .approval_level = desired;
                        } else if !is_working {
                            if let Err(err) =
                                recycle_grok_subprocess(&mut proc, target_launch, false)
                            {
                                eprintln!(
                                    "[Grok Native] Warning: failed to recycle for approval level {}: {err}",
                                    desired.label()
                                );
                            } else {
                                state
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .approval_level = desired;
                            }
                        }
                    }
                }
                if reconnecting || newly_registered || had_desired_approval {
                    reconnecting = false;
                    activity.sync();
                }
                resp
            }
            Err(_) => {
                reconnecting = true;
                thread::sleep(POLL_RETRY_DELAY);
                continue;
            }
        };

        let commands = resp
            .pointer("/result/commands")
            .or_else(|| resp.get("commands"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for cmd in commands {
            let id = cmd.get("id").and_then(Value::as_str).unwrap_or_default();
            let kind = cmd.get("kind").and_then(Value::as_str).unwrap_or_default();
            let params = cmd.get("params").cloned().unwrap_or(Value::Null);

            match kind {
                "state" => {
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "abort" => {
                    let (was_working, sid, spawned_mode) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let w = s.working;
                        if w {
                            s.last_turn_interrupted = true;
                        }
                        (w, s.native_session_id.clone(), s.spawned_launch_mode)
                    };
                    if let Some(sid_ref) = sid.as_deref() {
                        // Grok Ctrl+C blocks stale background wakes until the next genuine user turn.
                        let _ = proc.transport.notify(
                            "session/cancel",
                            json!({
                                "sessionId": sid_ref,
                                "_meta": { "cancelTrigger": "ctrl_c" },
                            }),
                        );
                    }
                    cancel_all_pending_requests(&proc.transport, &state);
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.abort_all_prompt_waiters();
                    }
                    if was_working {
                        let wait_until = SystemTime::now() + Duration::from_secs(2);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                        // User Stop while active restarts the Grok process group to terminate any
                        // stuck foreground tools cleanly and resumes the same conversation.
                        let still_working = state.lock().unwrap_or_else(|p| p.into_inner()).working;
                        if still_working {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.turn_generation = s.turn_generation.wrapping_add(1);
                            s.working = false;
                            s.wake_prompt_id = None;
                            s.clear_live_turn_output();
                        }
                        if let Err(err) = recycle_grok_subprocess(&mut proc, spawned_mode, true) {
                            eprintln!("[Grok Native] Warning: restart after stop failed: {err}");
                        }
                    }
                    activity.sync();
                    ep.ack(id, true, json!({ "aborted": true }), None);
                }
                "approve" => {
                    let decision = params
                        .get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("allow");
                    let explicit = params.get("optionId").and_then(Value::as_str);
                    let target_tool_use_id = non_empty_str(params.get("toolUseId"));
                    let req = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|p| {
                                    !p.is_question() && p.tool_use_id.as_deref() == Some(wanted_id)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|p| !p.is_question())
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        let msg = if target_tool_use_id.is_some() {
                            "That Native approval request is no longer pending."
                        } else {
                            "No pending Native approval."
                        };
                        ep.ack(id, false, Value::Null, Some(msg));
                        continue;
                    };
                    let response_payload = match &req.kind {
                        PendingPromptKind::ExitPlanMode => {
                            let normalized = decision.trim().to_ascii_lowercase();
                            let approved = matches!(
                                normalized.as_str(),
                                "allow"
                                    | "allow_once"
                                    | "always"
                                    | "allow_always"
                                    | "approved"
                                    | "accept"
                                    | "1"
                                    | "y"
                                    | "yes"
                            ) || explicit == Some("approved");
                            if approved {
                                json!({ "outcome": "approved" })
                            } else {
                                json!({
                                    "outcome": "abandoned",
                                    "feedback": "The user did not approve the plan yet. Wait for their feedback before implementing.",
                                })
                            }
                        }
                        PendingPromptKind::Permission => {
                            let outcome = if decision.eq_ignore_ascii_case("escape")
                                || decision.eq_ignore_ascii_case("cancel")
                            {
                                json!({ "outcome": "cancelled" })
                            } else if let Some(opt_id) =
                                req.resolve_permission_option_id(explicit, decision)
                            {
                                json!({ "outcome": "selected", "optionId": opt_id })
                            } else {
                                json!({ "outcome": "cancelled" })
                            };
                            json!({ "outcome": outcome })
                        }
                        PendingPromptKind::AskUserQuestion { .. } => {
                            json!({ "outcome": "cancelled" })
                        }
                    };
                    match proc
                        .transport
                        .respond_to_agent(req.rpc_id, response_payload)
                    {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "answer_question" => {
                    let selections = params
                        .get("selections")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let target_tool_use_id = non_empty_str(params.get("toolUseId"));
                    let req = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|p| {
                                    p.is_question() && p.tool_use_id.as_deref() == Some(wanted_id)
                                })
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
                        continue;
                    };
                    let payload = match &req.kind {
                        PendingPromptKind::AskUserQuestion { raw_questions } => {
                            build_xai_ask_user_question_response(raw_questions, &selections)
                        }
                        _ => json!({ "outcome": "cancelled" }),
                    };
                    match proc.transport.respond_to_agent(req.rpc_id, payload) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let model = non_empty_str(params.get("model")).map(str::to_string);
                    let effort = non_empty_str(params.get("effort")).map(str::to_string);
                    if let Err(err) = apply_grok_model_and_effort(
                        &proc.transport,
                        &state,
                        model.as_deref(),
                        effort.as_deref(),
                    ) {
                        ep.ack(id, false, Value::Null, Some(&err));
                        continue;
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "mode" => {
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
                        continue;
                    };
                    let (is_working, spawned_mode, read_only) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.spawned_launch_mode, s.read_only)
                    };
                    let target_launch = GrokLaunchMode::from_approval_level(level, read_only);
                    if target_launch != spawned_mode {
                        if is_working {
                            ep.ack(
                                id,
                                false,
                                Value::Null,
                                Some(
                                    "Wait for the current turn to finish before changing Grok's permission mode.",
                                ),
                            );
                            continue;
                        }
                        if let Err(err) = recycle_grok_subprocess(&mut proc, target_launch, false) {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.approval_level = level;
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "steer" => {
                    let raw_text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let text = rewrite_leading_grok_skill_mention(
                        raw_text,
                        &grok_bin,
                        &cwd_path,
                        &skills_cache,
                    );
                    let (is_working, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (s.working, s.native_session_id.clone())
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Grok Native session is not initialized yet."),
                        );
                        continue;
                    };
                    if is_working {
                        match proc.transport.call(
                            XAI_INTERJECT_METHOD,
                            json!({
                                "sessionId": sid,
                                "text": text,
                            }),
                            GROK_RPC_TIMEOUT,
                        ) {
                            Ok(res)
                                if res.get("status").and_then(Value::as_str) == Some("queued")
                                    || res.pointer("/result/status").and_then(Value::as_str)
                                        == Some("queued") =>
                            {
                                println!(
                                    "\r\n[Native] Delivered mid-turn steer via _x.ai/interject..."
                                );
                                let _ = std::io::stdout().flush();
                                ep.ack(id, true, json!({ "steered": true }), None);
                                continue;
                            }
                            Ok(_) => {
                                ep.ack(
                                    id,
                                    false,
                                    Value::Null,
                                    Some("Grok did not queue the interjection."),
                                );
                                continue;
                            }
                            Err(err) => {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                        }
                    }
                    // If the turn already settled, start a normal turn with the message.
                    let blocks = match build_grok_prompt_blocks(&gx_paths, &text, &[]) {
                        Ok(b) => b,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    let prompt_id = format!(
                        "gx-xai-prompt-{}",
                        next_prompt_seq.fetch_add(1, Ordering::Relaxed)
                    );
                    let (turn_gen, fallback_rx) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.wake_prompt_id = None;
                        s.last_turn_interrupted = false;
                        s.last_error = None;
                        s.clear_live_turn_output();
                        let rx = s.register_prompt_waiter(sid.clone(), prompt_id.clone());
                        (s.turn_generation, rx)
                    };
                    match proc.transport.start_call(
                        "session/prompt",
                        json!({
                            "sessionId": sid,
                            "prompt": blocks,
                            "_meta": {
                                "promptId": prompt_id,
                                "requestId": prompt_id,
                            },
                        }),
                    ) {
                        Ok((rpc_id, rpc_rx)) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "steered": true, "sessionId": sid }), None);
                            spawn_grok_prompt_watcher(
                                proc.transport.clone(),
                                state.clone(),
                                activity.clone(),
                                rpc_id,
                                rpc_rx,
                                fallback_rx,
                                prompt_id,
                                turn_gen,
                            );
                        }
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.remove_prompt_waiter(&prompt_id);
                                if s.turn_generation == turn_gen {
                                    s.working = false;
                                }
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                        }
                    }
                }
                "send" => {
                    let raw_text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let text = rewrite_leading_grok_skill_mention(
                        raw_text,
                        &grok_bin,
                        &cwd_path,
                        &skills_cache,
                    );
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

                    let (is_working, has_pending_perm, sid, spawned_mode) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            s.spawned_launch_mode,
                        )
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Grok Native session is not initialized yet."),
                        );
                        continue;
                    };
                    if !interrupt && has_pending_perm {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }

                    // Mid-turn follow-up without interrupt: steer via `_x.ai/interject`.
                    if is_working && !interrupt {
                        let image_blocks =
                            match build_grok_interject_image_blocks(&gx_paths, &images) {
                                Ok(blocks) => blocks,
                                Err(err) => {
                                    ep.ack(id, false, Value::Null, Some(&err));
                                    continue;
                                }
                            };
                        let mut interject_params = json!({
                            "sessionId": sid,
                            "text": text,
                        });
                        if !image_blocks.is_empty() {
                            interject_params["content"] = Value::Array(image_blocks);
                        }
                        match proc.transport.call(
                            XAI_INTERJECT_METHOD,
                            interject_params,
                            GROK_RPC_TIMEOUT,
                        ) {
                            Ok(res)
                                if res.get("status").and_then(Value::as_str) == Some("queued")
                                    || res.pointer("/result/status").and_then(Value::as_str)
                                        == Some("queued") =>
                            {
                                println!("\r\n[Native] Delivered mid-turn message...");
                                let _ = std::io::stdout().flush();
                                ep.ack(
                                    id,
                                    true,
                                    json!({
                                        "accepted": true,
                                        "midTurn": true,
                                        "sessionId": sid,
                                    }),
                                    None,
                                );
                                continue;
                            }
                            _ => {
                                // If the turn finished while we called interject, fall through to
                                // a normal `session/prompt`; otherwise report busy so Ghostex queues.
                                if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                    ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                                    continue;
                                }
                            }
                        }
                    }

                    let blocks = match build_grok_prompt_blocks(&gx_paths, &text, &images) {
                        Ok(blocks) => blocks,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    if is_working && interrupt {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.last_turn_interrupted = true;
                            s.abort_all_prompt_waiters();
                        }
                        cancel_all_pending_requests(&proc.transport, &state);
                        let _ = proc.transport.notify(
                            "session/cancel",
                            json!({
                                "sessionId": sid,
                                "_meta": { "cancelTrigger": "ctrl_c" },
                            }),
                        );
                        let wait_until = SystemTime::now() + Duration::from_secs(3);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                        if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            if let Err(err) = recycle_grok_subprocess(&mut proc, spawned_mode, true)
                            {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                        }
                    } else {
                        cancel_all_pending_requests(&proc.transport, &state);
                    }

                    let prompt_id = format!(
                        "gx-xai-prompt-{}",
                        next_prompt_seq.fetch_add(1, Ordering::Relaxed)
                    );
                    let (prev_working, prev_gen, turn_gen, fallback_rx) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let prev_w = s.working;
                        let prev_g = s.turn_generation;
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.wake_prompt_id = None;
                        s.last_turn_interrupted = false;
                        s.last_error = None;
                        s.clear_live_turn_output();
                        let rx = s.register_prompt_waiter(sid.clone(), prompt_id.clone());
                        (prev_w, prev_g, s.turn_generation, rx)
                    };

                    let (rpc_id, rpc_rx) = match proc.transport.start_call(
                        "session/prompt",
                        json!({
                            "sessionId": sid,
                            "prompt": blocks,
                            "_meta": {
                                "promptId": prompt_id,
                                "requestId": prompt_id,
                            },
                        }),
                    ) {
                        Ok(pair) => pair,
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.remove_prompt_waiter(&prompt_id);
                                if s.turn_generation == turn_gen {
                                    s.turn_generation = prev_gen;
                                    s.working = prev_working;
                                }
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    println!("\r\n[Native] Turn started...");
                    let _ = std::io::stdout().flush();
                    activity.sync();
                    ep.ack(
                        id,
                        true,
                        json!({
                            "accepted": true,
                            "midTurn": false,
                            "sessionId": sid,
                        }),
                        None,
                    );

                    spawn_grok_prompt_watcher(
                        proc.transport.clone(),
                        state.clone(),
                        activity.clone(),
                        rpc_id,
                        rpc_rx,
                        fallback_rx,
                        prompt_id,
                        turn_gen,
                    );
                }
                "switch_to_cli" => {
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let (is_working, has_pending_perm, sid, carry_model, carry_effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            s.current_model.clone(),
                            s.current_effort.clone(),
                        )
                    };
                    if (is_working || has_pending_perm) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("The agent is currently working on a turn. Interrupt it or wait for it to finish before switching to Terminal."),
                        );
                        continue;
                    }
                    cancel_all_pending_requests(&proc.transport, &state);
                    if is_working {
                        {
                            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                            s.last_turn_interrupted = true;
                            s.abort_all_prompt_waiters();
                        }
                        if let Some(sid_ref) = sid.as_deref() {
                            let _ = proc.transport.notify(
                                "session/cancel",
                                json!({
                                    "sessionId": sid_ref,
                                    "_meta": { "cancelTrigger": "ctrl_c" },
                                }),
                            );
                        }
                        let wait_until = SystemTime::now() + Duration::from_millis(1500);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    shutting_down.store(true, Ordering::Release);
                    stop_grok_process(&mut proc, &child_pid, &intentional_stop, false);
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
                    switch_to_cli_requested = true;
                    break;
                }
                other => {
                    ep.ack(
                        id,
                        false,
                        Value::Null,
                        Some(&format!("Unsupported Grok Native host command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, approval_level, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.native_session_id.clone(),
                s.approval_level,
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            approval_level,
            parsed.read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    stop_grok_process(&mut proc, &child_pid, &intentional_stop, false);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_grok_prompt_watcher(
    transport: Arc<GrokTransport>,
    state: Arc<Mutex<GrokState>>,
    activity: GrokActivityReporter,
    rpc_id: u64,
    rpc_rx: mpsc::Receiver<Value>,
    fallback_rx: mpsc::Receiver<GrokPromptOutcome>,
    prompt_id: String,
    turn_gen: u64,
) {
    thread::spawn(move || {
        // Race the JSON-RPC `session/prompt` response against xAI's completion notifications
        // (`x.ai/session/prompt_complete` or `turn_completed`), matching the xAI ACP extension.
        let deadline = Instant::now() + Duration::from_secs(60 * 60);
        let outcome = loop {
            if let Ok(fallback) = fallback_rx.try_recv() {
                transport
                    .pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&rpc_id);
                break fallback;
            }
            match rpc_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(resp) => {
                    if let Some(err) = resp.get("error") {
                        let code = err.get("code").and_then(Value::as_i64).unwrap_or(-32603);
                        let message = err
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Grok request failed")
                            .to_string();
                        break GrokPromptOutcome::Error {
                            code,
                            message,
                            is_usage_limit: code == XAI_RATE_LIMITED_ERROR_CODE,
                        };
                    }
                    let result = resp.get("result").cloned().unwrap_or(Value::Null);
                    let stop_reason = result
                        .get("stopReason")
                        .and_then(Value::as_str)
                        .unwrap_or("end_turn")
                        .to_string();
                    break GrokPromptOutcome::Success {
                        stop_reason,
                        usage: result.get("usage").cloned(),
                    };
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // Check if fallback already arrived right before disconnect.
                    if let Ok(fallback) = fallback_rx.try_recv() {
                        break fallback;
                    }
                    break GrokPromptOutcome::Error {
                        code: -32603,
                        message: "Grok Native host disconnected during turn.".to_string(),
                        is_usage_limit: false,
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        transport
                            .pending
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&rpc_id);
                        break GrokPromptOutcome::Error {
                            code: -32603,
                            message: "Grok Native turn timed out.".to_string(),
                            is_usage_limit: false,
                        };
                    }
                }
            }
        };

        let (stop_reason_label, was_cancelled, stop_error_code, last_err_pair) = match &outcome {
            GrokPromptOutcome::Success { stop_reason, .. } => {
                let cancelled = matches!(
                    stop_reason.to_ascii_lowercase().as_str(),
                    "cancelled" | "canceled" | "interrupted" | "aborted"
                );
                (stop_reason.clone(), cancelled, None, None)
            }
            GrokPromptOutcome::Error {
                code,
                message,
                is_usage_limit,
            } => {
                let err_code = if *is_usage_limit {
                    "rate_limit".to_string()
                } else {
                    format!("acp_{code}")
                };
                (
                    format!("error: {message}"),
                    false,
                    Some(err_code.clone()),
                    Some((message.clone(), Some(err_code))),
                )
            }
        };

        let (is_current_turn, orphaned_reqs) = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.remove_prompt_waiter(&prompt_id);
            s.remember_completed_prompt_id(&prompt_id);
            if s.usage_used_tokens.is_none() {
                if let GrokPromptOutcome::Success {
                    usage: Some(ref u), ..
                } = outcome
                {
                    if let Some(total) = u
                        .get("inputTokens")
                        .or_else(|| u.get("totalTokens"))
                        .and_then(Value::as_u64)
                        .filter(|n| *n > 0)
                    {
                        s.usage_used_tokens = Some(total);
                    }
                }
            }
            if s.turn_generation == turn_gen {
                s.working = false;
                if was_cancelled {
                    s.last_turn_interrupted = true;
                }
                if let Some(pair) = last_err_pair {
                    s.last_error = Some(pair);
                }
                s.clear_live_turn_output();
                let drained: Vec<GrokPendingRequest> = s.pending_requests.drain(..).collect();
                (true, drained)
            } else {
                (false, Vec::new())
            }
        };

        for req in orphaned_reqs {
            respond_pending_request_cancelled(&transport, req);
        }

        if is_current_turn {
            println!("[Native] Turn finished ({stop_reason_label}).");
            let _ = std::io::stdout().flush();
            if let Some(code) = stop_error_code {
                activity.sync_with_error(code);
            } else {
                activity.sync();
            }
        }
    });
}
