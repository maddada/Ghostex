//! Native `cursor-agent acp` JSON-RPC driver for `ghostex acp-host --agent cursor`.
//!
//! Speaks Cursor Agent CLI's structured ACP stdio server directly in Rust, isolating
//! `CURSOR_CONFIG_DIR` from the user's `~/.cursor/cli-config.json` (read-only) so Ghostex's four
//! live approval levels (`Ask`, `Auto-edits`, `Auto`, `Full permissions`) stay authoritative
//! without mutating the user's Cursor config, supporting `cursor/ask_question` interactive
//! questions, parameterized model + effort selection, live streaming/thinking/tool/diff/plan/todo
//! and subagent cards, context usage read from Cursor's checkpoint store, `gxserver` reload
//! survival inside `zmx`, and bidirectional Terminal-based <-> Native switching.

mod config_and_store;
mod permissions;
mod reader;
mod subagents;
mod transcript;
mod transport_and_state;

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

pub(crate) use config_and_store::{
    cursor_native_transcript_dir, cursor_native_transcript_meta_path,
    cursor_native_transcript_path, user_cli_default_level,
};
pub(crate) use subagents::resolve_cursor_subagent;

use config_and_store::{
    is_cursor_auth_error, is_cursor_usage_limit_error, read_cursor_acp_meta_title,
    resolve_cursor_workspace_mcp_servers, sync_store_from_acp_to_cli, sync_store_from_cli_to_acp,
    CursorIsolatedConfig,
};
use permissions::{
    build_cursor_ask_question_response, cancel_all_pending_requests, resolve_acp_permission_option,
    CursorRequestKind, PendingCursorRequest,
};
use transport_and_state::{
    apply_cursor_model_and_effort, build_prompt_blocks, parse_cursor_mode_slash,
    stop_child_gracefully, CursorActivityReporter, CursorState, CursorTransport,
};

use super::approval::{canonicalize_writable_roots, AcpApprovalLevel};
use super::host::{
    exec_interactive_cli, now_ms, GxserverEndpoint, HostArgs, TerminationSignalGuard,
    POLL_RETRY_DELAY,
};
use super::spec::{build_acp_server_argv, AcpAgentSpec};

pub(super) const CURSOR_RPC_TIMEOUT: Duration = Duration::from_secs(30);
const CURSOR_AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const CURSOR_LOAD_TIMEOUT: Duration = Duration::from_secs(90);

/* ----------------------------------------------------------- entry point */

pub(super) fn run(spec: &AcpAgentSpec, parsed: HostArgs) -> i32 {
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
    // Keep default (no `--approval-level`): start at the user's own `cli-config.json` level
    // (CDXC:NativeApprovals 2026-11-25 in `acp/cli_default.rs`).
    let keep_cli_default = parsed.approval_level.is_none() && !parsed.read_only;
    let isolated_config = match CursorIsolatedConfig::create(&gx_paths, keep_cli_default) {
        Ok(cfg) => cfg,
        Err(err) => {
            let msg = format!("Failed preparing Cursor Native config: {err}");
            eprintln!("{msg}");
            super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, &msg, false);
            return 1;
        }
    };

    if let Some(resume_id) = parsed.resume_session_id.as_deref() {
        sync_store_from_cli_to_acp(&cwd_path, resume_id);
    }

    let signal_guard = TerminationSignalGuard::install();
    let Some((mut acp_envs, prog, argv)) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
    else {
        let msg = format!("Invalid Native host command for {}", spec.family);
        eprintln!("{msg}");
        super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, &msg, false);
        return 1;
    };
    acp_envs.push((
        "CURSOR_CONFIG_DIR".to_string(),
        isolated_config.dir.to_string_lossy().into_owned(),
    ));

    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);
    let mut child = match super::child_lifetime::end_with_host(&mut Command::new(&prog))
        .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .envs(acp_envs)
        .args(&argv)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let msg = format!("Failed to start `{prog} {}`: {err}", argv.join(" "));
            eprintln!("{msg}");
            super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, &msg, false);
            return 1;
        }
    };
    let mut child_lifetime =
        super::child_lifetime::ChildLifetime::attach(&child, record_target.as_ref());
    let child_pid_num = child.id();
    let child_pid = Arc::new(AtomicU32::new(child_pid_num));
    let Some(stdin) = child.stdin.take() else {
        let msg = "Failed to open stdin pipe to Cursor Native host.";
        eprintln!("{msg}");
        super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, msg, false);
        super::child_lifetime::force_kill_child(&mut child);
        child_lifetime.cleanup();
        return 1;
    };
    let Some(stdout) = child.stdout.take() else {
        let msg = "Failed to open stdout pipe from Cursor Native host.";
        eprintln!("{msg}");
        super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, msg, false);
        super::child_lifetime::force_kill_child(&mut child);
        child_lifetime.cleanup();
        return 1;
    };
    let stderr = child.stderr.take();

    let transport = Arc::new(CursorTransport::new(stdin, &spool_session_id));
    let initial_level = if parsed.read_only {
        AcpApprovalLevel::Ask
    } else {
        parsed
            .approval_level
            .or_else(|| super::cli_default::cli_default_level("cursor"))
            .unwrap_or(AcpApprovalLevel::Ask)
    };
    let state = Arc::new(Mutex::new(CursorState::new(
        initial_level,
        parsed.read_only,
        cwd_path.clone(),
        writable_roots,
        child_pid_num,
    )));
    let activity = CursorActivityReporter::start(endpoint.clone(), state.clone());
    let shutting_down = Arc::new(AtomicBool::new(false));

    #[cfg(unix)]
    {
        let t_sig = transport.clone();
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
        let cfg_dir_sig = isolated_config.dir.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if super::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
                    t_sig.close_stdin();
                    let pid = pid_sig.swap(0, Ordering::AcqRel);
                    super::child_lifetime::terminate_child_on_host_signal(pid, target_sig.as_ref());
                    let _ = fs::remove_dir_all(&cfg_dir_sig);
                    if let Some(ep) = ep_sig.as_ref() {
                        ep.close_channel();
                    }
                    std::process::exit(143);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    let stderr_head: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = stderr {
        let t_err = transport.clone();
        let head = stderr_head.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    t_err.spool_event(&json!({ "stderr": trimmed }));
                    super::startup_error::record_stderr_head_line(&head, trimmed);
                }
            }
        });
    }

    reader::spawn_stdout_reader(stdout, transport.clone(), state.clone(), activity.clone());

    // 1. Initialize ACP with parameterized model picker & subagents enabled.
    let init_params = json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
            "_meta": {
                "parameterizedModelPicker": true,
                "subagents": true,
            },
        },
        "clientInfo": crate::acp::client_info(),
    });
    if let Err(err) = transport.call("initialize", init_params, CURSOR_RPC_TIMEOUT) {
        let exited = child.try_wait().ok().flatten();
        let message = super::startup_error::startup_failure_message(
            "Cursor Native",
            &err,
            exited,
            &stderr_head,
        );
        eprintln!("Cursor Native initialize failed: {message}");
        super::host::report_driver_startup_error(
            endpoint.as_ref(),
            spec.family,
            &message,
            is_cursor_auth_error(&message),
        );
        stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
        return 1;
    }

    // 2. Open or resume Cursor ACP session (retrying once after `authenticate` if required).
    let workspace_mcp_servers = resolve_cursor_workspace_mcp_servers(&cwd_path);
    let open_session = |attempt_resume: Option<&str>| -> Result<(String, Value), String> {
        if let Some(resume_id) = attempt_resume {
            // Bind transcript BEFORE `session/load` so replay either populates an empty file or
            // gets suppressed if we already seeded from Terminal/previous Native turns.
            let suppress = {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                let populated = s.transcript.bind(resume_id);
                s.transcript.suppress_writes = populated;
                s.subagents.loading = true;
                populated
            };
            let res = transport.call(
                "session/load",
                json!({
                    "sessionId": resume_id,
                    "cwd": cwd,
                    "mcpServers": workspace_mcp_servers,
                }),
                CURSOR_LOAD_TIMEOUT,
            );
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if !suppress {
                    s.transcript.flush_thought();
                    s.transcript.flush_text();
                }
                s.transcript.suppress_writes = false;
                s.subagents.loading = false;
            }
            res.map(|val| (resume_id.to_string(), val))
        } else {
            let val = transport.call(
                "session/new",
                json!({ "cwd": cwd, "mcpServers": workspace_mcp_servers }),
                CURSOR_RPC_TIMEOUT,
            )?;
            let sid = val
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Cursor session/new returned no sessionId.".to_string())?
                .to_string();
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                let _ = s.transcript.bind(&sid);
            }
            Ok((sid, val))
        }
    };

    let open_res = match open_session(parsed.resume_session_id.as_deref()) {
        Ok(pair) => Ok(pair),
        Err(err) if is_cursor_auth_error(&err) => {
            match transport.call(
                "authenticate",
                json!({ "methodId": "cursor_login" }),
                CURSOR_AUTH_TIMEOUT,
            ) {
                Ok(_) => open_session(parsed.resume_session_id.as_deref()),
                Err(_) => Err(err),
            }
        }
        Err(err) => {
            // If resuming an un-saved/deleted session id failed, fall back to a fresh session so
            // the host stays usable rather than exiting immediately.
            if parsed.resume_session_id.is_some() {
                eprintln!("[Native] Cursor session/load failed ({err}); starting fresh session.");
                open_session(None)
            } else {
                Err(err)
            }
        }
    };

    let (native_session_id, session_result) = match open_res {
        Ok(pair) => pair,
        Err(err) => {
            eprintln!("Cursor Native session open failed: {err}");
            let is_auth = is_cursor_auth_error(&err);
            super::host::report_driver_startup_error(endpoint.as_ref(), spec.family, &err, is_auth);
            if is_auth {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.last_auth_error = Some(err.clone());
                s.last_error = Some((Some("unauthorized".to_string()), err));
            }
            stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
            return 1;
        }
    };

    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.native_session_id = Some(native_session_id.clone());
        if let Some(modes) = session_result.get("modes") {
            if let Some(cur) = modes.get("currentModeId").and_then(Value::as_str) {
                s.current_mode = Some(cur.to_string());
            }
            if let Some(avail) = modes.get("availableModes") {
                s.available_modes = avail.clone();
            }
        }
        if let Some(configs) = session_result.get("configOptions") {
            s.sync_from_config_options(configs);
        }
        if s.session_title.is_none() {
            s.session_title = read_cursor_acp_meta_title(&native_session_id);
        }
        s.refresh_context_usage_from_store();
    }

    // Lock read-only sessions into Cursor's read-only `plan` mode.
    if parsed.read_only {
        if transport
            .call(
                "session/set_mode",
                json!({ "sessionId": native_session_id, "modeId": "plan" }),
                CURSOR_RPC_TIMEOUT,
            )
            .is_ok()
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.current_mode = Some("plan".to_string());
        }
    }

    // Apply requested initial model and/or effort if provided.
    let requested_model = parsed
        .model
        .as_deref()
        .filter(|m| !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m));
    let requested_effort = parsed
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    if requested_model.is_some() || requested_effort.is_some() {
        if let Err(err) =
            apply_cursor_model_and_effort(&transport, &state, requested_model, requested_effort)
        {
            eprintln!("[Native] Warning: could not apply initial Cursor model/effort: {err}");
        }
    }

    println!(
        "Ghostex Native Host · {} · Ready (conversation: {})",
        spec.display_name, native_session_id
    );
    println!("Use Chat View to talk with this agent, or switch to Terminal to open Terminal mode.");
    let _ = std::io::stdout().flush();

    activity.sync();

    // 3. Main `/api/agentChatChannel` poll loop.
    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
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

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            println!("\r\n[Native] Cursor agent process exited.");
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
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if !s.read_only {
                        s.approval_level = desired;
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
                    let snap = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if !s.working {
                            s.refresh_context_usage_from_store();
                        }
                        s.snapshot_json()
                    };
                    ep.ack(id, true, snap, None);
                }
                "abort" => {
                    let sid = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if s.working {
                            s.last_turn_interrupted = true;
                        }
                        s.native_session_id.clone()
                    };
                    if let Some(sid) = sid {
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                    }
                    cancel_all_pending_requests(&transport, &state);
                    let wait_until = SystemTime::now() + Duration::from_secs(2);
                    while SystemTime::now() < wait_until {
                        if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = false;
                        s.transcript.finish_turn("aborted", None);
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
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|r| {
                                    !r.perm.is_question
                                        && r.perm.tool_use_id.as_deref() == Some(wanted_id)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|r| !r.perm.is_question)
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
                    let outcome = if decision.eq_ignore_ascii_case("escape")
                        || decision.eq_ignore_ascii_case("cancel")
                    {
                        json!({ "outcome": "cancelled" })
                    } else if let Some(opt_id) =
                        resolve_acp_permission_option(&req.perm.options, explicit, decision)
                    {
                        json!({ "outcome": "selected", "optionId": opt_id })
                    } else {
                        json!({ "outcome": "cancelled" })
                    };
                    match transport.respond_to_agent(req.perm.rpc_id, json!({ "outcome": outcome }))
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
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_requests
                                .iter()
                                .position(|r| {
                                    r.perm.is_question
                                        && r.perm.tool_use_id.as_deref() == Some(wanted_id)
                                })
                                .and_then(|idx| s.pending_requests.remove(idx))
                        } else {
                            s.pending_requests
                                .iter()
                                .position(|r| r.perm.is_question)
                                .and_then(|idx| s.pending_requests.remove(idx))
                        }
                    };
                    let Some(req) = req else {
                        ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                        continue;
                    };
                    let response_payload = match &req.kind {
                        CursorRequestKind::CursorAskQuestion { questions } => {
                            build_cursor_ask_question_response(questions, &selections)
                        }
                        CursorRequestKind::AcpPermission => {
                            let first_idx = selections
                                .first()
                                .and_then(|s| s.get("indices"))
                                .and_then(Value::as_array)
                                .and_then(|arr| arr.first())
                                .and_then(Value::as_u64);
                            if let Some(opt) =
                                first_idx.and_then(|idx| req.perm.options.get(idx as usize))
                            {
                                json!({ "outcome": { "outcome": "selected", "optionId": opt.option_id } })
                            } else {
                                json!({ "outcome": { "outcome": "cancelled" } })
                            }
                        }
                    };
                    match transport.respond_to_agent(req.perm.rpc_id, response_payload) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let model = params
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let effort = params
                        .get("effort")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    if let Err(err) =
                        apply_cursor_model_and_effort(&transport, &state, model, effort)
                    {
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
                    {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if !s.read_only {
                            s.approval_level = level;
                        }
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "send" => {
                    let raw_text = params
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
                    let (is_working, has_pending_perm, sid, read_only) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            s.read_only,
                        )
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Cursor Native session not initialized yet."),
                        );
                        continue;
                    };
                    if !interrupt && has_pending_perm {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }
                    if is_working && !interrupt {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                        continue;
                    }

                    // Support `/plan`, `/ask`, and `/agent` mode switches directly over ACP.
                    let mut prompt_text = raw_text.clone();
                    if images.is_empty() {
                        if let Some((target_mode, remainder)) = parse_cursor_mode_slash(&raw_text) {
                            let effective_mode = if read_only { "plan" } else { target_mode };
                            if let Err(err) = transport.call(
                                "session/set_mode",
                                json!({ "sessionId": sid, "modeId": effective_mode }),
                                CURSOR_RPC_TIMEOUT,
                            ) {
                                ep.ack(id, false, Value::Null, Some(&err));
                                continue;
                            }
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                s.current_mode = Some(effective_mode.to_string());
                            }
                            if remainder.is_empty() {
                                ep.ack(
                                    id,
                                    true,
                                    json!({
                                        "accepted": true,
                                        "mode": effective_mode,
                                        "sessionId": sid,
                                    }),
                                    None,
                                );
                                continue;
                            }
                            prompt_text = remainder.to_string();
                        }
                    }

                    let blocks = match build_prompt_blocks(&gx_paths, &prompt_text, &images) {
                        Ok(blocks) => blocks,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };

                    if is_working && interrupt {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        cancel_all_pending_requests(&transport, &state);
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                        let wait_until = SystemTime::now() + Duration::from_secs(4);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    cancel_all_pending_requests(&transport, &state);

                    let (prev_working, prev_gen, turn_gen) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let prev_working = s.working;
                        let prev_gen = s.turn_generation;
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.last_turn_interrupted = false;
                        s.last_turn_stop_error = None;
                        s.last_error = None;
                        s.clear_live_turn_output();
                        let display_text = if !prompt_text.trim().is_empty() {
                            prompt_text.clone()
                        } else {
                            "[Image]".to_string()
                        };
                        s.transcript.write_user_message(&display_text);
                        s.subagents.begin_turn();
                        (prev_working, prev_gen, s.turn_generation)
                    };

                    let (rpc_id, rx) = match transport.start_call(
                        "session/prompt",
                        json!({
                            "sessionId": sid,
                            "prompt": blocks,
                        }),
                    ) {
                        Ok(pair) => pair,
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                if s.turn_generation == turn_gen {
                                    s.turn_generation = prev_gen;
                                    s.working = prev_working;
                                    s.transcript.finish_turn("error", Some(&err));
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

                    let t_turn = transport.clone();
                    let st_turn = state.clone();
                    let act_turn = activity.clone();
                    thread::spawn(move || {
                        let res = t_turn.finish_call(
                            rpc_id,
                            "session/prompt",
                            rx,
                            Duration::from_secs(60 * 60),
                        );
                        let (stop_reason, rpc_err) = match &res {
                            Ok(val) => {
                                if let Some(total) = val
                                    .pointer("/usage/totalTokens")
                                    .or_else(|| val.pointer("/usage/inputTokens"))
                                    .and_then(Value::as_u64)
                                    .filter(|u| *u > 0)
                                {
                                    let mut s = st_turn.lock().unwrap_or_else(|p| p.into_inner());
                                    if s.usage_used_tokens.is_none() {
                                        s.usage_used_tokens = Some(total);
                                    }
                                }
                                (
                                    val.get("stopReason")
                                        .and_then(Value::as_str)
                                        .unwrap_or("end_turn")
                                        .to_string(),
                                    None,
                                )
                            }
                            Err(err) => (format!("error: {err}"), Some(err.clone())),
                        };
                        let was_cancelled = matches!(
                            stop_reason.to_ascii_lowercase().as_str(),
                            "cancelled" | "canceled" | "interrupted" | "aborted"
                        );
                        let (is_current_turn, orphaned_reqs) = {
                            let mut s = st_turn.lock().unwrap_or_else(|p| p.into_inner());
                            if s.turn_generation == turn_gen {
                                s.working = false;
                                let transport_err = s.transport_failure.failure();
                                let turn_err = rpc_err.or(transport_err);
                                if was_cancelled || s.last_turn_interrupted {
                                    s.last_turn_interrupted = true;
                                    s.transcript.finish_turn("aborted", None);
                                } else if let Some(ref err_msg) = turn_err {
                                    let code = if is_cursor_auth_error(err_msg) {
                                        s.last_auth_error = Some(err_msg.clone());
                                        Some("unauthorized".to_string())
                                    } else if is_cursor_usage_limit_error(err_msg) {
                                        Some("rate_limit".to_string())
                                    } else {
                                        Some("provider_error".to_string())
                                    };
                                    s.last_turn_stop_error = code.clone();
                                    s.last_error = Some((code, err_msg.clone()));
                                    s.transcript.finish_turn("error", Some(err_msg));
                                } else {
                                    s.transcript.finish_turn("success", None);
                                }
                                s.refresh_context_usage_from_store();
                                s.clear_live_turn_output();
                                let drained: Vec<PendingCursorRequest> =
                                    s.pending_requests.drain(..).collect();
                                (true, drained)
                            } else {
                                (false, Vec::new())
                            }
                        };
                        for req in orphaned_reqs {
                            match req.kind {
                                CursorRequestKind::AcpPermission => {
                                    let _ = t_turn.respond_to_agent(
                                        req.perm.rpc_id,
                                        json!({ "outcome": { "outcome": "cancelled" } }),
                                    );
                                }
                                CursorRequestKind::CursorAskQuestion { .. } => {
                                    let _ = t_turn.respond_to_agent(
                                        req.perm.rpc_id,
                                        json!({ "outcome": { "outcome": "cancelled", "reason": "Turn ended" } }),
                                    );
                                }
                            }
                        }
                        if is_current_turn {
                            println!("[Native] Turn finished ({stop_reason}).");
                            let _ = std::io::stdout().flush();
                            act_turn.sync();
                        }
                    });
                }
                "switch_to_cli" => {
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let (is_working, has_pending_perm, sid, carry_model, carry_effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let m = s.current_model.as_deref().map(|raw| {
                            if raw == "default" {
                                "auto".to_string()
                            } else {
                                raw.to_string()
                            }
                        });
                        (
                            s.working,
                            !s.pending_requests.is_empty(),
                            s.native_session_id.clone(),
                            m,
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
                    cancel_all_pending_requests(&transport, &state);
                    if is_working {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        if let Some(sid_ref) = sid.as_deref() {
                            let _ =
                                transport.notify("session/cancel", json!({ "sessionId": sid_ref }));
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
                    stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
                    // Sync the flushed ACP store into `~/.cursor/chats/<md5(cwd)>/<id>/` so
                    // `cursor-agent --resume <id>` resumes the exact Native conversation.
                    if let Some(sid_ref) = sid.as_deref() {
                        sync_store_from_acp_to_cli(&cwd_path, sid_ref);
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
                    switch_to_cli_requested = true;
                    break;
                }
                other => {
                    ep.ack(
                        id,
                        false,
                        Value::Null,
                        Some(&format!("Unsupported Cursor Native command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        isolated_config.cleanup();
        let (sid, approval_level, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            let m = s.current_model.as_deref().map(|raw| {
                if raw == "default" {
                    "auto".to_string()
                } else {
                    raw.to_string()
                }
            });
            (
                s.native_session_id.clone(),
                s.approval_level,
                m,
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
    stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
    if let Some(sid_ref) = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .native_session_id
        .clone()
    {
        sync_store_from_acp_to_cli(&cwd_path, &sid_ref);
    }
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
