//! Native OpenCode 2 server driver for `ghostex acp-host --agent opencode`.
//!
//! Spawns a local `opencode serve --hostname=127.0.0.1 --port=<free>` process with a
//! per-host random `OPENCODE_PASSWORD` (OpenCode 2 only), subscribes to its `/api/event`
//! SSE stream, maps Ghostex's four live approval levels (`Ask`, `Auto-edits`, `Auto`,
//! `Full access`) and `--read-only` to OpenCode 2 session permission rules (keeping
//! session-scoped grants in memory and never writing persistent `"always"` approvals),
//! handles permissions, forms/questions, model + variant (effort) catalog and selection,
//! mid-turn steering, slash commands, skills, `!shell` commands, image attachments,
//! subagent fleet + child transcript mirrors, `todowrite` tasks, conversation rewind,
//! `gxserver` reload survival inside `zmx`, and in-place Switch to Terminal
//! (`opencode --session <id>`).

mod approvals;
mod commands;
mod events;
mod http;
mod mirror;
mod process;
mod state;

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub(crate) use mirror::{is_mirror_path, mirror_path, resolve_subagent_mirror};

use approvals::apply_approval_level;
use commands::dispatch_command;
use http::resolve_opencode_binary;
use process::{spawn_and_init_opencode, stop_opencode_gracefully, OpenCodeSpawnConfig};
use state::{OpenCodeActivityReporter, OpenCodeState};

use super::approval::{canonicalize_writable_roots, AcpApprovalLevel};
use super::host::{
    exec_interactive_cli, now_ms, GxserverEndpoint, HostArgs, TerminationSignalGuard,
    POLL_RETRY_DELAY,
};
use super::spec::{validated_ghostex_prompt_file, AcpAgentSpec};

const SERVER_START_TIMEOUT: Duration = Duration::from_secs(20);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const HTTP_LONG_TIMEOUT: Duration = Duration::from_secs(60);
const SHELL_TIMEOUT: Duration = Duration::from_secs(3600);
const SSE_SILENT_TIMEOUT: Duration = Duration::from_secs(45);
const MIRROR_DELTA_DEBOUNCE: Duration = Duration::from_millis(120);

const DECLINED_PERMISSION_MESSAGE: &str = "The user declined this request.";
const DECLINED_SYNTHETIC_NOTE: &str =
    "The user declined this request. Do not retry it unless asked.";
const DISMISSED_FORM_MESSAGE: &str = "The user dismissed this prompt.";

pub(super) fn run(spec: &AcpAgentSpec, parsed: HostArgs) -> i32 {
    let cwd_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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

    let prog = resolve_opencode_binary();
    let initial_approval_level = if parsed.read_only {
        AcpApprovalLevel::Ask
    } else {
        parsed.approval_level.unwrap_or(AcpApprovalLevel::Ask)
    };

    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);
    let spawn_cfg = OpenCodeSpawnConfig {
        prog,
        connector_envs: parsed.profile.connector_envs.clone(),
        cwd: cwd.clone(),
        spool_session_id,
        record_target: record_target.clone(),
    };

    let file_instructions =
        validated_ghostex_prompt_file(parsed.profile.append_system_prompt_file.as_deref())
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|text| text.trim().to_string())
            .filter(|s| !s.is_empty());
    let combined_developer_instructions = match (
        parsed
            .profile
            .developer_instructions
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        file_instructions.as_deref(),
    ) {
        (Some(inline), Some(file_text)) => Some(format!("{inline}\n\n{file_text}")),
        (Some(inline), None) => Some(inline.to_string()),
        (None, Some(file_text)) => Some(file_text.to_string()),
        (None, None) => None,
    };

    let state = Arc::new(Mutex::new(OpenCodeState::new(
        initial_approval_level,
        parsed.read_only,
        combined_developer_instructions,
        cwd_path.clone(),
        writable_roots,
        parsed.model.clone(),
        parsed.effort.clone(),
    )));
    let activity = OpenCodeActivityReporter::start(endpoint.clone(), state.clone());
    let shutting_down = Arc::new(AtomicBool::new(false));
    let child_pid = Arc::new(AtomicU32::new(0));
    let process_gen = Arc::new(AtomicU64::new(1));

    let mut proc_opt = match spawn_and_init_opencode(
        &spawn_cfg,
        parsed.resume_session_id.as_deref(),
        parsed.fork_session,
        &state,
        &activity,
        &child_pid,
        &process_gen,
        1,
    ) {
        Ok(proc) => Some(proc),
        Err(err) => {
            eprintln!("[OpenCode Native] Startup failed: {err}");
            return 1;
        }
    };

    #[cfg(unix)]
    {
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if super::host::termination_requested() {
                    down_sig.store(true, Ordering::Release);
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

    let initial_sid = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .session_id
        .clone();
    println!(
        "Ghostex Native Host · {} · Ready (session: {})",
        spec.display_name,
        initial_sid.as_deref().unwrap_or("pending")
    );
    let _ = std::io::stdout().flush();
    activity.sync();

    let mut switch_to_cli_requested = false;
    let capabilities = json!([
        "acp",
        "send",
        "mid_turn_send",
        "steer",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "switch_to_cli",
        "rewind"
    ]);
    let channel_agent = format!("acp:{}", spec.family);
    let mut reconnecting = false;
    let mut crash_restarts: VecDeque<Instant> = VecDeque::new();

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            let now = Instant::now();
            while crash_restarts
                .front()
                .is_some_and(|at| now.duration_since(*at) > Duration::from_secs(60))
            {
                crash_restarts.pop_front();
            }
            if crash_restarts.len() >= 3 {
                println!("\r\n[OpenCode Native] Server exited repeatedly; stopping host.");
                break;
            }
            crash_restarts.push_back(now);
            println!("\r\n[OpenCode Native] Server exited unexpectedly; restarting...");
            let _ = std::io::stdout().flush();
            if let Some(mut old) = proc_opt.take() {
                stop_opencode_gracefully(&mut old.child, &mut old.lifetime, &child_pid);
            }
            thread::sleep(Duration::from_millis(500));
            let next_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
            let resume_id = state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .session_id
                .clone();
            match spawn_and_init_opencode(
                &spawn_cfg,
                resume_id.as_deref(),
                false,
                &state,
                &activity,
                &child_pid,
                &process_gen,
                next_gen,
            ) {
                Ok(proc) => {
                    proc_opt = Some(proc);
                    println!("\r\n[OpenCode Native] Server restarted and session resumed.");
                    let _ = std::io::stdout().flush();
                    activity.sync();
                    continue;
                }
                Err(err) => {
                    eprintln!("[OpenCode Native] Failed to restart server: {err}");
                    break;
                }
            }
        }

        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let is_working_now = state.lock().unwrap_or_else(|p| p.into_inner()).working;
        let mut poll_params = json!({
            "action": "poll",
            "sessionRef": ep.session_ref,
            "agent": channel_agent,
            "pid": std::process::id(),
            "instance": ep.instance,
            "version": 1,
            "capabilities": capabilities,
        });
        if is_working_now {
            poll_params["holdMs"] = json!(1500);
        }
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
                    let current = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .approval_level;
                    if desired != current {
                        if let Some(proc) = proc_opt.as_ref() {
                            if let Err(err) =
                                apply_approval_level(&proc.http, &state, &activity, desired)
                            {
                                eprintln!(
                                    "[OpenCode Native] Warning: could not sync approval level: {err}"
                                );
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
            if dispatch_command(
                ep,
                id,
                kind,
                params,
                &mut proc_opt,
                &state,
                &activity,
                &gx_paths,
                &cwd,
                &shutting_down,
                &process_gen,
                &child_pid,
            ) {
                switch_to_cli_requested = true;
                break;
            }
        }
        if switch_to_cli_requested {
            break;
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, approval_level, read_only, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            (
                s.session_id.clone(),
                s.approval_level,
                s.read_only,
                s.current_model.clone(),
                s.current_effort.clone(),
            )
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            approval_level,
            read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    process_gen.fetch_add(1, Ordering::AcqRel);
    if let Some(mut old) = proc_opt.take() {
        stop_opencode_gracefully(&mut old.child, &mut old.lifetime, &child_pid);
    }
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
