//! Spawning, initializing, authenticating, loading/resuming, and stopping `grok agent stdio`.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use crate::acp::host::{now_ms, HostArgs};
use crate::acp::spec::{build_acp_server_argv, is_allowed_connector_env_key, AcpAgentSpec};

use super::events::handle_grok_stdout_message;
use super::models::{apply_grok_model_state, apply_grok_session_metadata};
use super::protocol::resolve_grok_auth_method_id;
use super::state::{GrokLaunchMode, GrokState};
use super::transport::{GrokActivityReporter, GrokProcess, GrokReaderExitGuard, GrokTransport};
use super::{
    GROK_INITIALIZE_TIMEOUT, GROK_LOAD_REPLAY_IDLE_GAP, GROK_LOAD_TIMEOUT,
    GROK_OAUTH2_REFERRER_ENV, GROK_OAUTH2_REFERRER_VALUE, GROK_RPC_TIMEOUT,
};

pub(super) fn spawn_and_initialize_grok(
    spec: &AcpAgentSpec,
    parsed: &HostArgs,
    launch_mode: GrokLaunchMode,
    resume_session_id: Option<&str>,
    cwd: &str,
    spool_session_id: &str,
    record_target: Option<&crate::acp::child_lifetime::ChildRecordTarget>,
    child_pid: &Arc<AtomicU32>,
    state: &Arc<Mutex<GrokState>>,
    activity: &GrokActivityReporter,
    intentional_stop: &Arc<AtomicBool>,
) -> Result<GrokProcess, String> {
    let (mut acp_envs, prog, _) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
            .ok_or_else(|| format!("Invalid Native host command for {}", spec.family))?;
    acp_envs.push((
        GROK_OAUTH2_REFERRER_ENV.to_string(),
        GROK_OAUTH2_REFERRER_VALUE.to_string(),
    ));
    for (k, v) in &parsed.profile.connector_envs {
        if is_allowed_connector_env_key(k) {
            acp_envs.push((k.clone(), v.clone()));
        }
    }
    if launch_mode == GrokLaunchMode::Default {
        // Prevent Grok CLI from seeding Auto permission mode from ~/.claude/settings.json
        // defaultMode when Ghostex explicitly launched Ask or Auto-edits.
        acp_envs.push(("GROK_AUTO_PERMISSION_MODE".to_string(), "0".to_string()));
        acp_envs.push((
            "GROK_DEFAULT_PERMISSION_MODE".to_string(),
            "ask".to_string(),
        ));
    }
    let argv = launch_mode.spawn_args();

    let mut child = crate::acp::child_lifetime::end_with_host(&mut Command::new(&prog))
        .env_remove(crate::acp::channel::ACP_CHANNEL_SECRET_ENV)
        .envs(acp_envs)
        .args(&argv)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to start `{prog} {}`: {err}", argv.join(" ")))?;

    let mut child_lifetime =
        crate::acp::child_lifetime::ChildLifetime::attach(&child, record_target);
    child_pid.store(child.id(), Ordering::Release);

    let Some(stdin) = child.stdin.take() else {
        crate::acp::child_lifetime::force_kill_child(&mut child);
        child_pid.store(0, Ordering::Release);
        child_lifetime.cleanup();
        return Err("Failed to open stdin pipe to Grok Native host.".to_string());
    };
    let Some(stdout) = child.stdout.take() else {
        crate::acp::child_lifetime::force_kill_child(&mut child);
        child_pid.store(0, Ordering::Release);
        child_lifetime.cleanup();
        return Err("Failed to open stdout pipe from Grok Native host.".to_string());
    };
    let stderr = child.stderr.take();

    let transport = Arc::new(GrokTransport::new(stdin, spool_session_id));

    // Drain stderr into spool.
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
                    crate::acp::startup_error::record_stderr_head_line(&head, trimmed);
                }
            }
        });
    }

    // Stdout JSON-RPC reader loop.
    {
        let t_out = transport.clone();
        let st_out = state.clone();
        let act_out = activity.clone();
        let stop_out = intentional_stop.clone();
        thread::spawn(move || {
            let _guard = GrokReaderExitGuard {
                state: st_out.clone(),
                intentional_stop: stop_out,
            };
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                        return;
                    };
                    handle_grok_stdout_message(&msg, &t_out, &st_out, &act_out);
                }));
                if res.is_err() {
                    eprintln!("[Grok Native] Warning: recovered from panic handling stdout line.");
                }
            }
        });
    }

    let cleanup_on_err = |transport: &GrokTransport,
                          child: &mut Child,
                          lifetime: &mut crate::acp::child_lifetime::ChildLifetime,
                          child_pid: &AtomicU32| {
        transport.close_stdin();
        crate::acp::child_lifetime::force_kill_child(child);
        child_pid.store(0, Ordering::Release);
        lifetime.cleanup();
    };

    // 1. Initialize ACP with xAI `clientType: "extension"`.
    let init_params = json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
            "session": { "notices": {}, "compaction": {} },
        },
        "clientInfo": crate::acp::client_info(),
        "_meta": {
            "clientType": "extension",
        },
    });
    let init_res = match transport.call("initialize", init_params, GROK_INITIALIZE_TIMEOUT) {
        Ok(res) => res,
        Err(err) => {
            let exited = child.try_wait().ok().flatten();
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(crate::acp::startup_error::startup_failure_message(
                "Grok Native",
                &err,
                exited,
                &stderr_head,
            ));
        }
    };
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.protocol_version = init_res
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if let Some(model_state) = init_res.pointer("/_meta/modelState") {
            apply_grok_model_state(&mut s, model_state);
        }
    }

    // 2. Authenticate when advertised.
    let auth_methods = init_res
        .get("authMethods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !auth_methods.is_empty() {
        let preferred = resolve_grok_auth_method_id();
        let default_method = init_res
            .pointer("/_meta/defaultAuthMethodId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let method_id = if auth_methods
            .iter()
            .any(|m| m.get("id").and_then(Value::as_str) == Some(preferred))
        {
            Some(preferred)
        } else if let Some(default_id) = default_method.filter(|id| {
            auth_methods
                .iter()
                .any(|m| m.get("id").and_then(Value::as_str) == Some(*id))
        }) {
            Some(default_id)
        } else {
            None
        };
        let Some(method_id) = method_id else {
            let detail = auth_methods
                .first()
                .and_then(|m| {
                    m.get("description")
                        .or_else(|| m.get("name"))
                        .and_then(Value::as_str)
                })
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Authentication required.");
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.fatal_error = Some(detail.to_string());
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(detail.to_string());
        };
        if let Err(err) = transport.call(
            "authenticate",
            json!({ "methodId": method_id }),
            GROK_RPC_TIMEOUT,
        ) {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.fatal_error = Some(err.clone());
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(err);
        }
    }

    // 3. Open or resume session (with replay-idle readiness for `session/load`).
    let session_result = if let Some(resume_id) = resume_session_id.filter(|s| !s.trim().is_empty())
    {
        let load_params = json!({
            "sessionId": resume_id,
            "cwd": cwd,
            "mcpServers": [],
        });
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.replaying_load = true;
            s.last_replay_activity_ms = None;
        }
        let load_res = match transport.start_call("session/load", load_params.clone()) {
            Ok((rpc_id, rx)) => {
                let deadline = Instant::now() + GROK_LOAD_TIMEOUT;
                let outcome = loop {
                    match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(resp) => {
                            if let Some(error) = resp.get("error") {
                                let msg = error
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("session/load failed");
                                break Err(msg.to_string());
                            }
                            let mut res = resp.get("result").cloned().unwrap_or_else(|| json!({}));
                            if res.get("sessionId").is_none() {
                                if let Some(obj) = res.as_object_mut() {
                                    obj.insert("sessionId".to_string(), json!(resume_id));
                                }
                            }
                            break Ok(res);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            break Err("Grok Native host exited during session/load.".to_string());
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Grok's session/load can stay unanswered after replay finishes.
                            let last_replay = state
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .last_replay_activity_ms;
                            if let Some(last_ms) = last_replay {
                                if now_ms().saturating_sub(last_ms)
                                    >= GROK_LOAD_REPLAY_IDLE_GAP.as_millis() as i64
                                {
                                    transport
                                        .pending
                                        .lock()
                                        .unwrap_or_else(|p| p.into_inner())
                                        .remove(&rpc_id);
                                    break Ok(json!({ "sessionId": resume_id }));
                                }
                            }
                            if Instant::now() >= deadline {
                                transport
                                    .pending
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .remove(&rpc_id);
                                break Err("Grok Native session/load timed out.".to_string());
                            }
                        }
                    }
                };
                outcome
            }
            Err(err) => Err(err),
        };
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.replaying_load = false;
            s.last_replay_activity_ms = None;
        }
        match load_res {
            Ok(res) => Ok(res),
            Err(_) => transport
                .call("session/resume", load_params, GROK_RPC_TIMEOUT)
                .map(|mut res| {
                    if res.get("sessionId").is_none() {
                        if let Some(obj) = res.as_object_mut() {
                            obj.insert("sessionId".to_string(), json!(resume_id));
                        }
                    }
                    res
                }),
        }
    } else {
        transport.call(
            "session/new",
            json!({ "cwd": cwd, "mcpServers": [] }),
            GROK_RPC_TIMEOUT,
        )
    };

    let session_result = match session_result {
        Ok(res) => res,
        Err(err) => {
            cleanup_on_err(&transport, &mut child, &mut child_lifetime, child_pid);
            return Err(format!("Grok Native session open failed: {err}"));
        }
    };

    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.spawned_launch_mode = launch_mode;
        s.child_exited = false;
        s.fatal_error = None;
        apply_grok_session_metadata(&mut s, &session_result);
    }

    Ok(GrokProcess {
        child,
        child_lifetime,
        transport,
    })
}

pub(super) fn stop_grok_process(
    proc: &mut GrokProcess,
    child_pid: &AtomicU32,
    intentional_stop: &AtomicBool,
    force_immediately: bool,
) {
    intentional_stop.store(true, Ordering::Release);
    proc.transport.close_stdin();
    if force_immediately {
        crate::acp::child_lifetime::force_kill_child(&mut proc.child);
        child_pid.store(0, Ordering::Release);
        proc.child_lifetime.cleanup();
        return;
    }
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                child_pid.store(0, Ordering::Release);
                proc.child_lifetime.cleanup();
                return;
            }
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    crate::acp::child_lifetime::force_kill_child(&mut proc.child);
                    child_pid.store(0, Ordering::Release);
                    proc.child_lifetime.cleanup();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                child_pid.store(0, Ordering::Release);
                proc.child_lifetime.cleanup();
                return;
            }
        }
    }
}
