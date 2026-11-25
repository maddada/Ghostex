//! OpenCode 2 server process lifecycle, model/provider catalog loading, interrupt, and rewind.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use super::super::host::SpoolWriter;
use super::super::spec::is_allowed_connector_env_key;
use super::approvals::{build_session_rules, cancel_all_pending, extract_plan_agent_paths};
use super::events::{reconcile_session, spawn_sse_loop};
use super::http::{
    encode_query_component, generate_server_password, readable_provider_error_message,
    reserve_ephemeral_port, OpenCodeHttp,
};
use super::state::{parse_model_reference, OpenCodeActivityReporter, OpenCodeState};
use super::{HTTP_LONG_TIMEOUT, HTTP_TIMEOUT, SERVER_START_TIMEOUT};

pub(super) struct OpenCodeProcess {
    pub(super) child: Child,
    pub(super) lifetime: super::super::child_lifetime::ChildLifetime,
    pub(super) http: Arc<OpenCodeHttp>,
}

pub(super) struct OpenCodeSpawnConfig {
    pub(super) prog: String,
    pub(super) connector_envs: Vec<(String, String)>,
    pub(super) cwd: String,
    pub(super) spool_session_id: String,
    pub(super) record_target: Option<super::super::child_lifetime::ChildRecordTarget>,
}

pub(super) fn stop_opencode_gracefully(
    child: &mut Child,
    lifetime: &mut super::super::child_lifetime::ChildLifetime,
    child_pid: &AtomicU32,
) {
    let pid = child.id();
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                child_pid.store(0, Ordering::Release);
                lifetime.cleanup();
                return;
            }
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    super::super::child_lifetime::force_kill_child(child);
                    child_pid.store(0, Ordering::Release);
                    lifetime.cleanup();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                child_pid.store(0, Ordering::Release);
                lifetime.cleanup();
                return;
            }
        }
    }
}

pub(super) fn populate_catalog_and_metadata(
    http: &OpenCodeHttp,
    cwd: &str,
    state: &Arc<Mutex<OpenCodeState>>,
) {
    let encoded_dir = encode_query_component(cwd);
    // Cold OpenCode servers may briefly return an empty model list; retry up to 3 times.
    let mut models_resp = Value::Null;
    for attempt in 0..3 {
        if let Ok(resp) = http.request(
            "GET",
            &format!("/api/model?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        ) {
            let count = resp
                .get("data")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            models_resp = resp;
            if count > 0 || attempt == 2 {
                break;
            }
        }
        thread::sleep(Duration::from_millis(350));
    }

    let lineup = crate::session_chat_opencode::lineup_entry(&models_resp, || {
        http.request(
            "GET",
            &format!("/api/provider?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
    });

    let mut available_models = Vec::new();
    let mut model_variants = HashMap::new();
    let mut model_context_windows = HashMap::new();
    let mut model_catalog = None;

    if let Some(entry) = lineup {
        if let Some(rows) = entry.get("models").and_then(Value::as_array) {
            for row in rows {
                let Some(val) = row.get("value").and_then(Value::as_str) else {
                    continue;
                };
                let label = row.get("label").and_then(Value::as_str).unwrap_or(val);
                let efforts: Vec<String> = row
                    .get("efforts")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let ctx = row.get("contextWindow").and_then(Value::as_u64);
                if let Some(w) = ctx.filter(|w| *w > 0) {
                    model_context_windows.insert(val.to_string(), w);
                }
                model_variants.insert(val.to_string(), efforts.clone());
                available_models.push(json!({
                    "modelId": val,
                    "name": label,
                    "supportedReasoningEfforts": efforts,
                    "contextWindow": ctx,
                }));
            }
        }
        model_catalog = Some(json!({
            "schemaVersion": 1,
            "updatedAt": "2026-09-25",
            "effortLabels": {},
            "agents": { "opencode": entry },
        }));
    }

    let plan_paths = http
        .request(
            "GET",
            &format!("/api/agent?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .map(|resp| extract_plan_agent_paths(&resp))
        .unwrap_or_default();

    let known_commands: HashSet<String> = http
        .request(
            "GET",
            &format!("/api/command?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .and_then(|resp| resp.get("data").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter_map(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .map(|s| s.trim().trim_start_matches('/').to_string())
                .filter(|s| !s.is_empty())
        })
        .collect();

    let known_skills: HashSet<String> = http
        .request(
            "GET",
            &format!("/api/skill?directory={encoded_dir}"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .and_then(|resp| resp.get("data").and_then(Value::as_array).cloned())
        .into_iter()
        .flatten()
        .filter_map(|s| {
            s.get("id")
                .or_else(|| s.get("name"))
                .and_then(Value::as_str)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .collect();

    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
    s.available_models = Value::Array(available_models);
    s.model_catalog = model_catalog;
    s.model_variants = model_variants;
    s.model_context_windows = model_context_windows;
    s.plan_agent_paths = plan_paths;
    s.known_commands = known_commands;
    s.known_skills = known_skills;
    s.update_window_for_current_model();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_and_init_opencode(
    cfg: &OpenCodeSpawnConfig,
    resume_session_id: Option<&str>,
    fork_session: bool,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    child_pid: &Arc<AtomicU32>,
    process_gen: &Arc<AtomicU64>,
    my_gen: u64,
) -> Result<OpenCodeProcess, String> {
    let port = reserve_ephemeral_port()?;
    let password = generate_server_password();

    let mut cmd = Command::new(&cfg.prog);
    super::super::child_lifetime::end_with_host(&mut cmd);
    cmd.env_remove(super::super::channel::ACP_CHANNEL_SECRET_ENV)
        .env_remove("OPENCODE_SERVER_PASSWORD")
        .env_remove("GHOSTEX_SESSION_ID")
        .env("OPENCODE_PASSWORD", &password);
    for (k, v) in &cfg.connector_envs {
        if is_allowed_connector_env_key(k) {
            cmd.env(k, v);
        }
    }
    cmd.args(["serve", "--hostname=127.0.0.1", &format!("--port={port}")])
        .current_dir(&cfg.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|err| {
        format!(
            "Failed to start `{} serve --hostname=127.0.0.1 --port={port}`: {err}",
            cfg.prog
        )
    })?;
    let mut lifetime =
        super::super::child_lifetime::ChildLifetime::attach(&child, cfg.record_target.as_ref());
    let pid = child.id();
    child_pid.store(pid, Ordering::Release);
    state.lock().unwrap_or_else(|p| p.into_inner()).child_pid = pid;

    let Some(stdout) = child.stdout.take() else {
        super::super::child_lifetime::force_kill_child(&mut child);
        lifetime.cleanup();
        child_pid.store(0, Ordering::Release);
        return Err("OpenCode server stdout unavailable.".to_string());
    };
    let stderr = child.stderr.take();
    let spool = Arc::new(Mutex::new(SpoolWriter::open(&cfg.spool_session_id)));
    let stderr_tail = Arc::new(Mutex::new(VecDeque::<String>::new()));

    if let Some(stderr) = stderr {
        let sp = spool.clone();
        let tail = stderr_tail.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(mut w) = sp.lock() {
                        w.append(&json!({ "stderr": trimmed }));
                    }
                    let mut t = tail.lock().unwrap_or_else(|p| p.into_inner());
                    if t.len() >= 16 {
                        t.pop_front();
                    }
                    t.push_back(trimmed.to_string());
                }
            }
        });
    }

    let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
    {
        let sp = spool.clone();
        let st_exit = state.clone();
        let gen_exit = process_gen.clone();
        let tail = stderr_tail.clone();
        thread::spawn(move || {
            let mut sent_ready = false;
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(mut w) = sp.lock() {
                        w.append(&json!({ "stdout": trimmed }));
                    }
                    if !sent_ready {
                        if let Some(idx) = trimmed.find("listening on http://") {
                            let url = trimmed[idx + "listening on ".len()..].trim().to_string();
                            sent_ready = true;
                            let _ = ready_tx.send(Ok(url));
                        }
                    }
                }
            }
            if !sent_ready {
                let tail_text = tail
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | ");
                let msg = if tail_text.is_empty() {
                    "OpenCode server exited before reporting its listen URL.".to_string()
                } else {
                    format!("OpenCode server exited during startup: {tail_text}")
                };
                let _ = ready_tx.send(Err(msg));
            }
            if gen_exit.load(Ordering::Acquire) == my_gen {
                let mut s = st_exit.lock().unwrap_or_else(|p| p.into_inner());
                s.child_exited = true;
                s.working = false;
            }
        });
    }

    let base_url = match ready_rx.recv_timeout(SERVER_START_TIMEOUT) {
        Ok(Ok(url)) => url,
        Ok(Err(err)) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            return Err(err);
        }
        Err(_) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            return Err("Timed out waiting for OpenCode server to start.".to_string());
        }
    };

    let http = Arc::new(OpenCodeHttp::new(base_url, &password));
    // Verify `/api/info` responds before subscribing.
    if let Err(err) = http.request("GET", "/api/info", None, HTTP_TIMEOUT) {
        process_gen.fetch_add(1, Ordering::AcqRel);
        stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
        return Err(format!("OpenCode server health check failed: {err}"));
    }

    // Load models, providers, agents (plan paths), commands, and skills.
    populate_catalog_and_metadata(&http, &cfg.cwd, state);

    // Subscribe to `/api/event` BEFORE creating/resuming/prompting so no events are missed.
    spawn_sse_loop(
        http.clone(),
        state.clone(),
        activity.clone(),
        spool.clone(),
        process_gen.clone(),
        my_gen,
    );

    // Create, resume, or fork the OpenCode session.
    let (approval_level, read_only, plan_paths, grants, req_model, req_effort, dev_instr) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (
            s.approval_level,
            s.read_only,
            s.plan_agent_paths.clone(),
            s.session_grants.clone(),
            s.current_model.clone(),
            s.current_effort.clone(),
            s.developer_instructions.clone(),
        )
    };
    let rules = build_session_rules(approval_level, read_only, &plan_paths, &grants);

    let validated_model_obj = req_model.as_deref().and_then(|m| {
        let (provider, id) = parse_model_reference(m)?;
        let variant = state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .validated_variant(m, req_effort.as_deref());
        let mut obj = json!({ "providerID": provider, "id": id });
        if let Some(v) = variant {
            obj["variant"] = json!(v);
        }
        Some(obj)
    });

    let open_or_create = || -> Result<Value, String> {
        if let Some(resume_id) = resume_session_id
            .map(str::trim)
            .filter(|s| !s.is_empty() && crate::session_chat_opencode::safe_id(s))
        {
            if fork_session {
                let forked =
                    http.session(resume_id, "/fork", "POST", Some(&json!({})), HTTP_TIMEOUT)?;
                return Ok(forked.get("data").cloned().unwrap_or(Value::Null));
            }
            let (status, existing) = http.request_raw(
                "GET",
                &format!("/api/session/{resume_id}"),
                None,
                None,
                HTTP_TIMEOUT,
            )?;
            if status == 200 {
                return Ok(existing.get("data").cloned().unwrap_or(Value::Null));
            }
            if status != 404 {
                let (_, msg) = readable_provider_error_message(&existing);
                return Err(if msg.is_empty() {
                    format!("Failed to load OpenCode session {resume_id} (HTTP {status}).")
                } else {
                    msg
                });
            }
            // Pre-minted id that has not been created on the server yet: create it with that id.
            let mut body = json!({
                "id": resume_id,
                "location": { "directory": cfg.cwd },
                "permissions": rules,
            });
            if let Some(ref model_obj) = validated_model_obj {
                body["model"] = model_obj.clone();
            }
            let created = http.request("POST", "/api/session", Some(&body), HTTP_TIMEOUT)?;
            return Ok(created.get("data").cloned().unwrap_or(Value::Null));
        }

        let mut body = json!({
            "location": { "directory": cfg.cwd },
            "permissions": rules,
        });
        if let Some(ref model_obj) = validated_model_obj {
            body["model"] = model_obj.clone();
        }
        let created = http.request("POST", "/api/session", Some(&body), HTTP_TIMEOUT)?;
        Ok(created.get("data").cloned().unwrap_or(Value::Null))
    };

    let session_data = match open_or_create() {
        Ok(data) => data,
        Err(err) => {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            return Err(err);
        }
    };

    let session_id = session_data
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| crate::session_chat_opencode::safe_id(s))
        .map(str::to_string)
        .ok_or_else(|| {
            process_gen.fetch_add(1, Ordering::AcqRel);
            stop_opencode_gracefully(&mut child, &mut lifetime, child_pid);
            "OpenCode did not return a valid session id.".to_string()
        })?;

    // Relocate session directory if cwd changed since last run.
    let existing_dir = session_data
        .pointer("/location/directory")
        .and_then(Value::as_str);
    if existing_dir.is_some_and(|d| d != cfg.cwd) {
        let _ = http.session(
            &session_id,
            "/move",
            "POST",
            Some(&json!({ "directory": cfg.cwd })),
            HTTP_TIMEOUT,
        );
    }

    // Apply Ghostex approval rules to resumed/forked sessions too.
    let _ = http.session(
        &session_id,
        "",
        "PATCH",
        Some(&json!({ "permissions": rules })),
        HTTP_TIMEOUT,
    );

    // Apply agent mode (`plan` for `--read-only`, otherwise keep existing or `build`).
    if read_only {
        let _ = http.session(
            &session_id,
            "/agent",
            "POST",
            Some(&json!({ "agent": "plan" })),
            HTTP_TIMEOUT,
        );
    }

    // Apply explicitly requested model/variant if present.
    if let Some(ref model_obj) = validated_model_obj {
        let _ = http.session(
            &session_id,
            "/model",
            "POST",
            Some(&json!({ "model": model_obj })),
            HTTP_TIMEOUT,
        );
    }

    // Apply Ghostex developer/coordinator/workspace instructions if provided.
    if let Some(instr) = dev_instr.as_deref().filter(|s| !s.trim().is_empty()) {
        let _ = http.session(
            &session_id,
            "/instructions/ghostex",
            "PUT",
            Some(&json!({ "text": instr })),
            HTTP_TIMEOUT,
        );
    }

    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.session_id = Some(session_id);
        s.session_info = session_data;
        s.child_exited = false;
        s.fatal_error = None;
    }

    reconcile_session(&http, state, activity, true);

    Ok(OpenCodeProcess {
        child,
        lifetime,
        http,
    })
}

pub(super) fn stop_session_shells(http: &OpenCodeHttp, session_id: &str, directory: &str) {
    let mut shell_ids = Vec::new();
    if let Ok(messages) = http.fetch_messages(session_id) {
        for m in messages {
            if m["type"] == "shell" && m["status"] == "running" {
                if let Some(sh) = m["shellID"]
                    .as_str()
                    .filter(|id| crate::session_chat_opencode::safe_id(id))
                {
                    if !shell_ids.iter().any(|e| e == sh) {
                        shell_ids.push(sh.to_string());
                    }
                }
            }
        }
    }
    if let Ok(shells) = http.request_in("GET", "/api/shell", directory, None, HTTP_TIMEOUT) {
        for sh in shells
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if sh.pointer("/metadata/sessionID").and_then(Value::as_str) == Some(session_id)
                && sh["status"] == "running"
            {
                if let Some(id) = sh["id"]
                    .as_str()
                    .filter(|id| crate::session_chat_opencode::safe_id(id))
                {
                    if !shell_ids.iter().any(|e| e == id) {
                        shell_ids.push(id.to_string());
                    }
                }
            }
        }
    }
    for sh in shell_ids {
        let _ = http.request_in(
            "DELETE",
            &format!("/api/shell/{sh}"),
            directory,
            None,
            HTTP_TIMEOUT,
        );
    }
}

pub(super) fn interrupt_active_turn(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    cwd: &str,
) {
    cancel_all_pending(http, state);
    let (sid_opt, child_ids) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working {
            s.last_turn_interrupted = true;
        }
        let active_children: Vec<String> = s
            .active_sessions
            .iter()
            .filter(|id| Some(id.as_str()) != s.session_id.as_deref())
            .cloned()
            .collect();
        (s.session_id.clone(), active_children)
    };
    for cid in child_ids {
        let _ = http.session(&cid, "/interrupt", "POST", None, Duration::from_secs(5));
    }
    if let Some(sid) = sid_opt.as_deref() {
        let _ = http.session(sid, "/interrupt", "POST", None, Duration::from_secs(5));
        stop_session_shells(http, sid, cwd);
    }
    let wait_until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < wait_until {
        if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.working = false;
        s.compacting = false;
        s.compacting_started_at = None;
        s.running_shells.clear();
        s.clear_live_turn_output();
    }
}

pub(super) fn handle_rewind(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    message_id: &str,
) -> Result<Value, String> {
    let (sid, is_working) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (s.session_id.clone(), s.working)
    };
    if is_working {
        return Err("ACP_AGENT_BUSY".to_string());
    }
    let Some(sid) = sid else {
        return Err("REWIND_NOT_FOUND".to_string());
    };
    let messages = http.fetch_messages(&sid)?;
    if !messages
        .iter()
        .any(|m| m["id"].as_str() == Some(message_id) && m["type"] == "user")
    {
        return Err("REWIND_NOT_FOUND".to_string());
    }
    if let Err(err) = http.session(
        &sid,
        "/revert/stage",
        "POST",
        Some(&json!({ "messageID": message_id, "files": false })),
        HTTP_LONG_TIMEOUT,
    ) {
        let _ = http.session(&sid, "/revert/clear", "POST", None, HTTP_TIMEOUT);
        return Err(err);
    }
    if let Err(err) = http.session(&sid, "/revert/commit", "POST", None, HTTP_LONG_TIMEOUT) {
        let _ = http.session(&sid, "/revert/clear", "POST", None, HTTP_TIMEOUT);
        return Err(err);
    }
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.usage_used_tokens = None;
        s.last_error = None;
        s.last_turn_stop_error = None;
        s.last_turn_interrupted = false;
    }
    reconcile_session(http, state, activity, false);
    Ok(json!({
        "ok": true,
        "targetMessageId": message_id,
        "warning": null,
    }))
}
