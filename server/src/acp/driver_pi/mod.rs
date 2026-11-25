//! Native `pi --mode rpc` JSONL driver for `ghostex acp-host --agent pi`.
//!
//! Speaks Pi's structured RPC protocol directly in Rust while injecting a small Ghostex-owned
//! Pi extension (`ghostex-pi-native-extension.ts`) for live tool-call approvals, `$skill`
//! expansion, and OpenRouter output-token capping. Enforces Ghostex's four live approval levels
//! (`Ask`, `Auto-edits`, `Auto`, `Full permissions`) plus `--read-only`, supports interactive
//! questions (`select`, `input`, `editor` including custom follow-up prompts), `/compact`,
//! local image attachments, models and thinking levels, live streaming/thinking/tool/compaction
//! progress, token usage, rewind (`fork`), `gxserver` reload survival inside `zmx`, and Switch
//! to Terminal (`pi --session <file>`).

mod events;
mod extension;
mod rewind;
mod state;
mod transport;
mod ui_requests;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use super::approval::{
    canonicalize_writable_roots, evaluate_permission, AcpApprovalLevel, AcpPermissionOption,
    PermissionDecision,
};
use super::host::{
    append_bounded, exec_interactive_cli, now_iso_rfc3339, now_ms, GxserverEndpoint, HostArgs,
    PendingPermission, SpoolWriter, TerminationSignalGuard, LIVE_STREAM_MAX_CHARS,
    POLL_RETRY_DELAY,
};
use super::spec::{build_acp_server_argv, transcript_path_for_session, AcpAgentSpec};

use events::spawn_pi_stdout_reader;
use extension::materialize_pi_extension;
use state::{PiActivityReporter, PiRequestKind, PiState};
use transport::{
    cancel_all_pending_requests, encode_pi_images, parse_pi_compact_command,
    read_pi_streaming_behavior, resolve_pi_launch_session_args, spawn_pi_process,
    stop_pi_child_gracefully, HeldPrompt, PiRpcTransport,
};
use ui_requests::apply_model_and_effort;

pub(super) const PI_RPC_TIMEOUT: Duration = Duration::from_secs(30);
const PI_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const PI_CLEAR_QUEUE_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const PI_RPIV_CUSTOM_OPTION: &str = "Type something.";
pub(super) const PI_RPIV_CUSTOM_TTL: Duration = Duration::from_secs(15);

pub(super) fn run(spec: &'static AcpAgentSpec, parsed: HostArgs) -> i32 {
    let cwd_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd = cwd_path.to_string_lossy().to_string();
    let writable_roots = canonicalize_writable_roots(&cwd_path, &parsed.writable_roots);
    let endpoint = GxserverEndpoint::from_env_or_secret_file(parsed.channel_secret_file.as_deref());
    let spool_session_id = endpoint
        .as_ref()
        .map(|ep| ep.session_id.clone())
        .unwrap_or_else(|| format!("local-pi-{}", std::process::id()));

    println!("Ghostex Native Host · {} · Starting...", spec.display_name);
    let _ = std::io::stdout().flush();

    let gx_paths = crate::paths::get_gxserver_paths(None);
    let extension_path = match materialize_pi_extension(&gx_paths) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("{err}");
            super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, &err, false);
            return 1;
        }
    };

    let signal_guard = TerminationSignalGuard::install();
    let Some((acp_envs, prog, base_argv)) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
    else {
        let msg = "Invalid Native host command for pi";
        eprintln!("{msg}");
        super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, msg, false);
        return 1;
    };
    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);

    let (initial_session_args, used_existing_session_flag) =
        resolve_pi_launch_session_args(parsed.resume_session_id.as_deref());

    let initial_approval = parsed
        .approval_level
        .or_else(|| parsed.read_only.then_some(AcpApprovalLevel::Ask))
        .unwrap_or(AcpApprovalLevel::Ask);

    // Spawn Pi RPC subprocess and run initial `get_state` handshake. If resuming a stale/missing
    // `--session` file fails before RPC answers, retry once cleanly without `--session`.
    let (mut child, mut child_lifetime, transport, state, activity) = {
        let mut attempt_args = initial_session_args;
        let mut allow_retry = used_existing_session_flag;
        loop {
            let (mut child, mut lifetime) = match spawn_pi_process(
                &prog,
                &base_argv,
                &acp_envs,
                &extension_path,
                &attempt_args,
                &cwd,
                record_target.as_ref(),
            ) {
                Ok(pair) => pair,
                Err(err) => {
                    eprintln!("{err}");
                    super::host::report_driver_startup_error(
                        endpoint.as_ref(),
                        spec.family,
                        &err,
                        false,
                    );
                    return 1;
                }
            };
            let pid = child.id();
            let Some(stdin) = child.stdin.take() else {
                super::child_lifetime::force_kill_child(&mut child);
                lifetime.cleanup();
                let msg = "Failed to open stdin pipe to Pi RPC.";
                eprintln!("{msg}");
                super::host::report_driver_startup_error(
                    endpoint.as_ref(),
                    spec.family,
                    msg,
                    false,
                );
                return 1;
            };
            let Some(stdout) = child.stdout.take() else {
                super::child_lifetime::force_kill_child(&mut child);
                lifetime.cleanup();
                let msg = "Failed to open stdout pipe from Pi RPC.";
                eprintln!("{msg}");
                super::host::report_driver_startup_error(
                    endpoint.as_ref(),
                    spec.family,
                    msg,
                    false,
                );
                return 1;
            };
            let stderr = child.stderr.take();
            let transport = Arc::new(PiRpcTransport::new(stdin, &spool_session_id));
            let state = Arc::new(Mutex::new(PiState::new(
                initial_approval,
                parsed.read_only,
                cwd_path.clone(),
                writable_roots.clone(),
                pid,
            )));
            let activity = PiActivityReporter::start(endpoint.clone(), state.clone());

            if let Some(stderr) = stderr {
                let t_err = transport.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().map_while(Result::ok) {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            t_err.spool_event(&json!({ "stderr": trimmed }));
                        }
                    }
                });
            }

            spawn_pi_stdout_reader(stdout, transport.clone(), state.clone(), activity.clone());

            match transport.call("get_state", json!({}), PI_STARTUP_TIMEOUT) {
                Ok(data) => {
                    state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .apply_get_state(&data);
                    break (child, lifetime, transport, state, activity);
                }
                Err(err) if allow_retry => {
                    allow_retry = false;
                    eprintln!(
                        "[Native] Pi resume handshake failed ({err}); starting a fresh Pi session..."
                    );
                    let pid_atomic = AtomicU32::new(pid);
                    stop_pi_child_gracefully(&transport, &mut child, &mut lifetime, &pid_atomic);
                    attempt_args = parsed
                        .resume_session_id
                        .as_deref()
                        .filter(|id| crate::agents::pi_session_id::is_pi_session_id(id))
                        .map(|id| vec!["--session-id".to_string(), id.to_string()])
                        .unwrap_or_default();
                    continue;
                }
                Err(err) => {
                    eprintln!("Pi RPC startup failed: {err}");
                    super::host::report_driver_startup_error(
                        endpoint.as_ref(),
                        spec.family,
                        &err,
                        super::startup_error::looks_like_auth_error(&err),
                    );
                    let pid_atomic = AtomicU32::new(pid);
                    stop_pi_child_gracefully(&transport, &mut child, &mut lifetime, &pid_atomic);
                    return 1;
                }
            }
        }
    };

    let child_pid = Arc::new(AtomicU32::new(child.id()));
    let shutting_down = Arc::new(AtomicBool::new(false));

    #[cfg(unix)]
    {
        let t_sig = transport.clone();
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if super::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
                    t_sig.close_stdin();
                    let pid = pid_sig.swap(0, Ordering::AcqRel);
                    super::child_lifetime::terminate_child_on_host_signal(pid, target_sig.as_ref());
                    if let Some(ep) = ep_sig.as_ref() {
                        ep.close_channel();
                    }
                    std::process::exit(143);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    // Fetch Pi's available models so model slug resolution and contextWindow lookups work.
    if let Ok(models_data) = transport.call("get_available_models", json!({}), PI_RPC_TIMEOUT) {
        state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .apply_available_models(&models_data);
    }

    // Apply requested initial `--model` and `--effort`.
    if parsed.model.is_some() || parsed.effort.is_some() {
        if let Err(err) = apply_model_and_effort(
            &transport,
            &state,
            parsed.model.as_deref(),
            parsed.effort.as_deref(),
        ) {
            eprintln!("[Native] Warning: could not apply initial Pi model/effort: {err}");
        }
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

    let capabilities = json!([
        "acp",
        "send",
        "steer",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "rewind",
        "switch_to_cli",
        "mid_turn_send",
    ]);
    let mut switch_to_cli_requested = false;
    let mut reconnecting = false;

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            println!("\r\n[Native] Pi process exited.");
            break;
        }
        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let mut poll_params = json!({
            "action": "poll",
            "sessionRef": ep.session_ref,
            "agent": "acp:pi",
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
                    state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .approval_level = desired;
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
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if s.working || s.is_compacting {
                            s.last_turn_interrupted = true;
                        }
                        s.held_prompts.clear();
                    }
                    cancel_all_pending_requests(&transport, &state);
                    let _ = transport.call("clear_queue", json!({}), PI_CLEAR_QUEUE_TIMEOUT);
                    let _ = transport.call("abort", json!({}), Duration::from_secs(5));
                    let wait_until = SystemTime::now() + Duration::from_secs(2);
                    while SystemTime::now() < wait_until {
                        if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = false;
                        s.is_compacting = false;
                        s.clear_live_turn_output();
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
                                .position(|p| {
                                    !p.is_question() && p.tool_use_id.as_deref() == Some(wanted)
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
                    let normalized = explicit.unwrap_or(decision).trim().to_ascii_lowercase();
                    let (confirmed, remember_session) = match normalized.as_str() {
                        "allow" | "allow_once" | "1" | "y" | "yes" => (true, false),
                        "always" | "allow_always" | "2" => (true, true),
                        _ => (false, false),
                    };
                    if confirmed && remember_session {
                        if let PiRequestKind::ToolApproval { ref tool_name } = req.kind {
                            state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .session_approvals
                                .insert(tool_name.clone());
                        }
                    }
                    match transport
                        .respond_extension_ui(&req.request_id, json!({ "confirmed": confirmed }))
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
                                .position(|p| {
                                    p.is_question() && p.tool_use_id.as_deref() == Some(wanted)
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
                    let first = selections.first();
                    let other_text = first
                        .and_then(|sel| sel.get("other"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    let first_idx = first
                        .and_then(|sel| sel.get("indices"))
                        .and_then(Value::as_array)
                        .and_then(|arr| arr.first())
                        .and_then(Value::as_u64)
                        .map(|n| n as usize);

                    let response_fields = match &req.kind {
                        PiRequestKind::Select {
                            options,
                            rpiv_custom_option,
                            ..
                        } => {
                            if let Some(idx) = first_idx {
                                if let Some(chosen) = options.get(idx) {
                                    json!({ "value": chosen })
                                } else {
                                    json!({ "cancelled": true })
                                }
                            } else if let Some(custom) = other_text {
                                if let Some(matched) =
                                    options.iter().find(|opt| opt.eq_ignore_ascii_case(&custom))
                                {
                                    json!({ "value": matched })
                                } else if let Some(rpiv_opt) = rpiv_custom_option {
                                    state
                                        .lock()
                                        .unwrap_or_else(|p| p.into_inner())
                                        .pending_rpiv_custom_input = Some((custom, Instant::now()));
                                    json!({ "value": rpiv_opt })
                                } else {
                                    json!({ "value": custom })
                                }
                            } else {
                                json!({ "cancelled": true })
                            }
                        }
                        PiRequestKind::TextInput { .. } => {
                            if let Some(custom) = other_text {
                                json!({ "value": custom })
                            } else {
                                json!({ "cancelled": true })
                            }
                        }
                        _ => json!({ "cancelled": true }),
                    };

                    match transport.respond_extension_ui(&req.request_id, response_fields) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let model = params.get("model").and_then(Value::as_str);
                    let effort = params.get("effort").and_then(Value::as_str);
                    match apply_model_and_effort(&transport, &state, model, effort) {
                        Ok(()) => {
                            let snap = state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .snapshot_json();
                            ep.ack(id, true, snap, None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
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
                    let snap = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.approval_level = level;
                        s.snapshot_json()
                    };
                    ep.ack(id, true, snap, None);
                }
                "steer" => {
                    let text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    match transport.send_prompt_async(text, Vec::new(), Some("steer")) {
                        Ok(_) => {
                            let sid = {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.working = true;
                                s.last_error = None;
                                s.native_session_id.clone()
                            };
                            ep.ack(
                                id,
                                true,
                                json!({ "accepted": true, "midTurn": true, "sessionId": sid }),
                                None,
                            );
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "send" => {
                    let text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let image_paths: Vec<String> = params
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

                    let (is_working, is_compacting, has_pending, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            s.is_compacting,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                        )
                    };
                    if !interrupt && has_pending {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }
                    let encoded_images = match encode_pi_images(&gx_paths, &image_paths) {
                        Ok(imgs) => imgs,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    // `/compact [instructions]` maps directly to Pi's RPC `compact` command.
                    if encoded_images.is_empty() {
                        if let Some(instructions) = parse_pi_compact_command(&text) {
                            match transport.send_compact_async(instructions.as_deref()) {
                                Ok(_) => {
                                    {
                                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                        s.is_compacting = true;
                                        s.working = true;
                                        s.compacting_started_at = Some(now_iso_rfc3339());
                                        s.last_error = None;
                                    }
                                    activity.sync();
                                    ep.ack(
                                        id,
                                        true,
                                        json!({ "accepted": true, "sessionId": sid }),
                                        None,
                                    );
                                }
                                Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                            }
                            continue;
                        }
                    }

                    // Hold prompts while compaction is running (Pi rejects prompts mid-compaction).
                    if is_compacting && !interrupt {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .held_prompts
                            .push_back(HeldPrompt {
                                text,
                                images: encoded_images,
                            });
                        ep.ack(
                            id,
                            true,
                            json!({ "accepted": true, "midTurn": true, "sessionId": sid }),
                            None,
                        );
                        continue;
                    }

                    let is_mid_turn = is_working && !interrupt;
                    if is_working && interrupt {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        cancel_all_pending_requests(&transport, &state);
                        let _ = transport.call("clear_queue", json!({}), PI_CLEAR_QUEUE_TIMEOUT);
                        let _ = transport.call("abort", json!({}), Duration::from_secs(5));
                        let wait_until = SystemTime::now() + Duration::from_secs(3);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    if !is_mid_turn {
                        cancel_all_pending_requests(&transport, &state);
                    }

                    let streaming_behavior = is_mid_turn.then(read_pi_streaming_behavior);
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.working = true;
                        s.last_error = None;
                        s.last_turn_interrupted = false;
                        if !is_mid_turn {
                            s.clear_live_turn_output();
                        }
                    }
                    match transport.send_prompt_async(&text, encoded_images, streaming_behavior) {
                        Ok(_) => {
                            if is_mid_turn {
                                println!("\r\n[Native] Delivered mid-turn message...");
                            } else {
                                println!("\r\n[Native] Turn started...");
                                activity.sync();
                            }
                            let _ = std::io::stdout().flush();
                            ep.ack(
                                id,
                                true,
                                json!({
                                    "accepted": true,
                                    "midTurn": is_mid_turn,
                                    "sessionId": sid,
                                }),
                                None,
                            );
                        }
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                if !is_mid_turn {
                                    s.working = false;
                                }
                                s.last_error = Some((err.clone(), None));
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                        }
                    }
                }
                "rewind" => {
                    let message_id = params
                        .get("messageId")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    match rewind::rewind_pi_session(&transport, &state, message_id) {
                        Ok(result) => {
                            activity.sync();
                            ep.ack(id, true, result, None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
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
                            s.working || s.is_compacting,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            s.current_model_slug(),
                            s.current_effort.clone(),
                        )
                    };
                    if (is_working || has_pending) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("The agent is currently working on a turn. Interrupt it or wait for it to finish before switching to Terminal."),
                        );
                        continue;
                    }
                    cancel_all_pending_requests(&transport, &state);
                    if is_working {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        let _ = transport.call("clear_queue", json!({}), PI_CLEAR_QUEUE_TIMEOUT);
                        let _ = transport.call("abort", json!({}), Duration::from_secs(3));
                    }
                    shutting_down.store(true, Ordering::Release);
                    stop_pi_child_gracefully(
                        &transport,
                        &mut child,
                        &mut child_lifetime,
                        &child_pid,
                    );
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
                        Some(&format!("Unsupported Native host command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (resume_target, approval_level, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            let target = s
                .session_file
                .as_deref()
                .filter(|p| Path::new(p).is_file())
                .map(str::to_string)
                .or_else(|| s.native_session_id.clone());
            (
                target,
                s.approval_level,
                s.current_model_slug(),
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            resume_target.as_deref(),
            approval_level,
            parsed.read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    stop_pi_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
