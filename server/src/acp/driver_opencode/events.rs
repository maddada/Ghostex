//! OpenCode 2 SSE event stream loop, event mapping, and full HTTP reconciliation.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use super::super::host::{
    append_bounded, now_iso_rfc3339, now_ms, SpoolWriter, LIVE_STREAM_MAX_CHARS,
};
use super::approvals::{asked_form_fields, handle_incoming_form, handle_incoming_permission};
use super::http::{is_auth_error, readable_provider_error_message, OpenCodeHttp};
use super::mirror::{extract_usage_from_messages, sync_child_mirror, tokens_total};
use super::state::{extract_model_from_info_or_messages, OpenCodeActivityReporter, OpenCodeState};
use super::{HTTP_TIMEOUT, MIRROR_DELTA_DEBOUNCE};

pub(super) fn reconcile_session(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    sync_children: bool,
) {
    let Some(sid) = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .session_id
        .clone()
    else {
        return;
    };

    let info = http
        .session(&sid, "", "GET", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").cloned());
    let messages = http.fetch_messages(&sid).ok();
    let active_map = http
        .request("GET", "/api/session/active", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let perms = http
        .session(&sid, "/permission", "GET", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let forms = http
        .session(&sid, "/form", "GET", None, HTTP_TIMEOUT)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let children_arr = http
        .request(
            "GET",
            &format!("/api/session?parentID={sid}&limit=200"),
            None,
            HTTP_TIMEOUT,
        )
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default();

    let mut active_set: HashSet<String> = active_map.keys().cloned().collect();
    let mut child_ids = Vec::new();
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(ref info_val) = info {
            s.session_info = info_val.clone();
            if let Some(title) = info_val
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                s.session_title = Some(title.to_string());
            }
            if let Some(agent_name) = info_val
                .get("agent")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|a| !a.is_empty())
            {
                s.current_agent = agent_name.to_string();
            }
        }
        if let Some(ref msgs) = messages {
            s.live_parts.prune(msgs);
            s.messages = msgs.clone();
            if let Some(used) = extract_usage_from_messages(msgs) {
                s.usage_used_tokens = Some(used);
            }
            let running_shells: HashSet<String> = msgs
                .iter()
                .filter(|m| m["type"] == "shell" && m["status"] == "running")
                .filter_map(|m| m["shellID"].as_str().map(str::to_string))
                .collect();
            s.running_shells = running_shells;
        }
        let (detected_model, detected_variant) =
            extract_model_from_info_or_messages(&s.session_info, &s.messages);
        if let Some(m) = detected_model {
            s.current_model = Some(m);
            s.current_effort = detected_variant;
            s.update_window_for_current_model();
        }
        for child in &children_arr {
            if let Some(cid) = child.get("id").and_then(Value::as_str) {
                s.child_sessions.insert(cid.to_string(), child.clone());
                child_ids.push((cid.to_string(), child.clone()));
            }
        }
        // Keep active set aligned with OpenCode server.
        let lead_active = active_set.contains(&sid) || !s.running_shells.is_empty();
        if !lead_active && s.working && !s.compacting {
            s.working = false;
            s.clear_live_turn_output();
            s.live_parts.clear();
        } else if lead_active {
            s.working = true;
        }
        s.active_sessions = std::mem::take(&mut active_set);
        // Prune any pending main-session permissions/forms that OpenCode no longer lists.
        let open_perm_ids: HashSet<&str> = perms
            .iter()
            .filter_map(|p| p.get("id").and_then(Value::as_str))
            .collect();
        let open_form_ids: HashSet<&str> = forms
            .iter()
            .filter_map(|f| f.get("id").and_then(Value::as_str))
            .collect();
        s.pending_requests.retain(|req| {
            if req.session_id != sid {
                return true;
            }
            if req.is_question() {
                open_form_ids.contains(req.request_id.as_str())
            } else {
                open_perm_ids.contains(req.request_id.as_str())
            }
        });
        if forms.iter().all(|f| asked_form_fields(f).is_some()) {
            s.external_form_url = None;
        }
        s.write_mirror_now();
    }

    for p in &perms {
        handle_incoming_permission(http, state, activity, p, &sid, false);
    }
    for f in &forms {
        handle_incoming_form(state, activity, f, &sid, false);
    }

    if sync_children {
        let active_snapshot = state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .active_sessions
            .clone();
        for (cid, cinfo) in &child_ids {
            sync_child_mirror(http, &sid, cinfo, active_snapshot.contains(cid));
            if let Ok(cperms) = http.session(cid, "/permission", "GET", None, HTTP_TIMEOUT) {
                for p in cperms
                    .get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    handle_incoming_permission(http, state, activity, p, cid, true);
                }
            }
            if let Ok(cforms) = http.session(cid, "/form", "GET", None, HTTP_TIMEOUT) {
                for f in cforms
                    .get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    handle_incoming_form(state, activity, f, cid, true);
                }
            }
        }
    }
    activity.sync();
}

pub(super) fn spawn_sse_loop(
    http: Arc<OpenCodeHttp>,
    state: Arc<Mutex<OpenCodeState>>,
    activity: OpenCodeActivityReporter,
    spool: Arc<Mutex<SpoolWriter>>,
    process_gen: Arc<AtomicU64>,
    my_gen: u64,
) {
    thread::spawn(move || {
        let mut backoff = Duration::from_millis(250);
        let mut ever_connected = false;
        while process_gen.load(Ordering::Acquire) == my_gen {
            match http.open_event_stream() {
                Ok(resp) => {
                    let was_reconnect = ever_connected;
                    ever_connected = true;
                    backoff = Duration::from_millis(250);
                    if was_reconnect {
                        reconcile_session(&http, &state, &activity, true);
                    }
                    let mut reader = BufReader::new(resp.into_body().into_reader());
                    let mut line = String::new();
                    loop {
                        if process_gen.load(Ordering::Acquire) != my_gen {
                            return;
                        }
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) => break,
                            Ok(_) => {
                                let trimmed = line.trim();
                                if let Some(data_str) = trimmed.strip_prefix("data:") {
                                    let payload = data_str.trim();
                                    if let Ok(event) = serde_json::from_str::<Value>(payload) {
                                        if let Ok(mut sp) = spool.lock() {
                                            sp.append(&event);
                                        }
                                        let res = std::panic::catch_unwind(
                                            std::panic::AssertUnwindSafe(|| {
                                                handle_sse_event(&http, &state, &activity, &event);
                                            }),
                                        );
                                        if res.is_err() {
                                            eprintln!(
                                                "[OpenCode Native] Warning: recovered from panic in SSE handler."
                                            );
                                        }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(_) => {}
            }
            if process_gen.load(Ordering::Acquire) != my_gen {
                return;
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    });
}

fn handle_sse_event(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    event: &Value,
) {
    let Some(kind) = event.get("type").and_then(Value::as_str) else {
        return;
    };
    let data = event.get("data").unwrap_or(&Value::Null);
    let event_sid = data
        .get("sessionID")
        .or_else(|| data.pointer("/session/id"))
        .or_else(|| data.pointer("/form/sessionID"))
        .or_else(|| data.pointer("/shell/metadata/sessionID"))
        .and_then(Value::as_str);

    let (main_sid, is_child) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.last_event_at_ms = now_ms();
        let main = s.session_id.clone();
        // Check if this is a newly created descendant session.
        if kind == "session.created" {
            let created = data.get("session").unwrap_or(data);
            if let (Some(cid), Some(parent)) = (
                created.get("id").and_then(Value::as_str),
                created.get("parentID").and_then(Value::as_str),
            ) {
                if main.as_deref() == Some(parent) || s.child_sessions.contains_key(parent) {
                    s.child_sessions.insert(cid.to_string(), created.clone());
                }
            }
        }
        let child = event_sid.is_some_and(|esid| s.child_sessions.contains_key(esid));
        (main, child)
    };

    let Some(main_sid) = main_sid else {
        return;
    };
    let is_main = event_sid == Some(main_sid.as_str());
    if !is_main && !is_child {
        return;
    }

    if is_child {
        let cid = event_sid.unwrap_or_default();
        match kind {
            "session.execution.started" => {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .active_sessions
                    .insert(cid.to_string());
            }
            "session.execution.succeeded"
            | "session.execution.failed"
            | "session.execution.interrupted" => {
                let cinfo = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.active_sessions.remove(cid);
                    s.pending_requests.retain(|r| r.session_id != cid);
                    s.child_sessions.get(cid).cloned()
                };
                if let Some(info) = cinfo {
                    sync_child_mirror(http, &main_sid, &info, false);
                }
                activity.sync();
            }
            "permission.asked" => {
                handle_incoming_permission(http, state, activity, data, cid, true);
            }
            "permission.replied" => {
                if let Some(req_id) = data
                    .get("requestID")
                    .or_else(|| data.get("id"))
                    .and_then(Value::as_str)
                {
                    let changed = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let before = s.pending_requests.len();
                        s.pending_requests.retain(|r| r.request_id != req_id);
                        s.pending_requests.len() != before
                    };
                    if changed {
                        activity.sync();
                    }
                }
            }
            "form.created" => {
                let form = data.get("form").unwrap_or(data);
                handle_incoming_form(state, activity, form, cid, true);
            }
            "form.replied" | "form.cancelled" => {
                if let Some(req_id) = data
                    .get("formID")
                    .or_else(|| data.get("id"))
                    .and_then(Value::as_str)
                {
                    let changed = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let before = s.pending_requests.len();
                        s.pending_requests.retain(|r| r.request_id != req_id);
                        s.pending_requests.len() != before
                    };
                    if changed {
                        activity.sync();
                    }
                }
            }
            "session.text.ended" | "session.tool.success" | "session.tool.failed" => {
                let (cinfo, active) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    (
                        s.child_sessions.get(cid).cloned(),
                        s.active_sessions.contains(cid),
                    )
                };
                if let Some(info) = cinfo {
                    sync_child_mirror(http, &main_sid, &info, active);
                }
            }
            _ => {}
        }
        return;
    }

    // Main session events
    match kind {
        "session.execution.started" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.working = true;
                s.active_sessions.insert(main_sid.clone());
                s.last_turn_interrupted = false;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
            }
            activity.sync();
        }
        "session.execution.succeeded" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.active_sessions.remove(&main_sid);
                s.working = !s.running_shells.is_empty();
                s.compacting = false;
                s.compacting_started_at = None;
                s.last_turn_interrupted = false;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
                s.clear_live_turn_output();
                s.live_parts.clear();
                s.pending_requests.retain(|r| r.session_id != main_sid);
            }
            println!("[OpenCode Native] Turn finished.");
            let _ = std::io::stdout().flush();
            reconcile_session(http, state, activity, true);
        }
        "session.execution.interrupted" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.active_sessions.remove(&main_sid);
                s.working = false;
                s.compacting = false;
                s.compacting_started_at = None;
                s.last_turn_interrupted = true;
                s.last_turn_stop_error = None;
                s.last_error = None;
                s.last_auth_error = None;
                s.clear_live_turn_output();
                s.live_parts.clear();
                s.pending_requests.retain(|r| r.session_id != main_sid);
            }
            reconcile_session(http, state, activity, true);
        }
        "session.execution.failed" => {
            let err_val = data.get("error").unwrap_or(data);
            let (code, msg) = readable_provider_error_message(err_val);
            let msg = if msg.is_empty() {
                "OpenCode failed to complete the turn.".to_string()
            } else {
                msg
            };
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.active_sessions.remove(&main_sid);
                s.working = false;
                s.compacting = false;
                s.compacting_started_at = None;
                s.clear_live_turn_output();
                s.live_parts.clear();
                s.pending_requests.retain(|r| r.session_id != main_sid);
                if is_auth_error(code.as_deref(), &msg) {
                    s.last_auth_error = Some(msg.clone());
                }
                s.last_turn_stop_error = Some(code.clone().unwrap_or_else(|| "error".to_string()));
                s.last_error = Some((code, msg));
            }
            reconcile_session(http, state, activity, true);
        }
        "session.text.delta" => {
            if let Some(delta) = data.get("delta").and_then(Value::as_str) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_stream_started_at.is_none() {
                    s.live_stream_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_stream_text, delta, LIVE_STREAM_MAX_CHARS);
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
                if let (Some(mid), Some(ord)) = (
                    data.get("assistantMessageID").and_then(Value::as_str),
                    data.get("ordinal").and_then(Value::as_u64),
                ) {
                    s.live_parts.append(mid, false, ord, delta);
                    if s.last_mirror_write.elapsed() >= MIRROR_DELTA_DEBOUNCE {
                        s.write_mirror_now();
                    }
                }
            }
        }
        "session.reasoning.delta" => {
            if let Some(delta) = data.get("delta").and_then(Value::as_str) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.live_thought_started_at.is_none() {
                    s.live_thought_started_at = Some(now_iso_rfc3339());
                }
                append_bounded(&mut s.live_thought_text, delta, LIVE_STREAM_MAX_CHARS);
                if let (Some(mid), Some(ord)) = (
                    data.get("assistantMessageID").and_then(Value::as_str),
                    data.get("ordinal").and_then(Value::as_u64),
                ) {
                    s.live_parts.append(mid, true, ord, delta);
                    if s.last_mirror_write.elapsed() >= MIRROR_DELTA_DEBOUNCE {
                        s.write_mirror_now();
                    }
                }
            }
        }
        "session.text.started" | "session.reasoning.started" => {
            // Fetch message skeleton so live deltas attach to the new assistant message row.
            if let Ok(msgs) = http.fetch_messages(&main_sid) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.messages = msgs;
                s.write_mirror_now();
            }
        }
        "session.text.ended" | "session.reasoning.ended" => {
            if let Ok(msgs) = http.fetch_messages(&main_sid) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.live_parts.prune(&msgs);
                s.messages = msgs;
                s.write_mirror_now();
            }
        }
        "session.tool.input.started" | "session.tool.called" | "session.tool.progress" => {
            let name = data
                .get("name")
                .or_else(|| data.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let detail = data
                .pointer("/input/command")
                .or_else(|| data.pointer("/input/filePath"))
                .or_else(|| data.pointer("/input/path"))
                .or_else(|| data.pointer("/input/pattern"))
                .or_else(|| data.pointer("/input/url"))
                .or_else(|| data.pointer("/input/query"))
                .or_else(|| data.pointer("/input/description"))
                .or_else(|| data.get("title"))
                .and_then(Value::as_str)
                .map(str::to_string);
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.live_tool_label = Some(name.to_string());
                s.live_tool_detail = detail;
                s.live_tool_started_at = Some(now_iso_rfc3339());
            }
            if kind == "session.tool.called" {
                if let Ok(msgs) = http.fetch_messages(&main_sid) {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.messages = msgs;
                    s.write_mirror_now();
                }
            }
        }
        "session.tool.success" | "session.tool.failed" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.live_tool_label = None;
                s.live_tool_detail = None;
                s.live_tool_started_at = None;
            }
            if let Ok(msgs) = http.fetch_messages(&main_sid) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.messages = msgs;
                s.write_mirror_now();
            }
        }
        "session.compaction.started" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.working = true;
                s.compacting = true;
                s.compacting_started_at = Some(now_iso_rfc3339());
            }
            activity.sync();
        }
        "session.compaction.ended" | "session.compaction.failed" => {
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.compacting = false;
                s.compacting_started_at = None;
                if kind == "session.compaction.ended" {
                    s.usage_used_tokens = None;
                }
                if !s.active_sessions.contains(&main_sid) && s.running_shells.is_empty() {
                    s.working = false;
                }
            }
            reconcile_session(http, state, activity, false);
        }
        "session.step.ended" => {
            if let Some(tokens) = data.get("tokens") {
                if let Some(used) = tokens_total(tokens) {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.usage_used_tokens = Some(used);
                    s.update_window_for_current_model();
                }
            }
        }
        "session.step.failed" => {
            if let Some(err) = data.get("error").filter(|v| !v.is_null()) {
                let (code, msg) = readable_provider_error_message(err);
                let is_abort = code
                    .as_deref()
                    .is_some_and(|c| c.to_ascii_lowercase().contains("abort"))
                    || msg.to_ascii_lowercase().contains("aborted");
                if !msg.is_empty() && !is_abort {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if is_auth_error(code.as_deref(), &msg) {
                        s.last_auth_error = Some(msg.clone());
                    }
                    s.last_error = Some((code, msg));
                }
            }
        }
        "session.retry.scheduled" => {
            let attempt = data.get("attempt").and_then(Value::as_u64);
            let msg = data
                .get("message")
                .or_else(|| data.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("Retrying request");
            let label = match attempt {
                Some(n) => format!("Retrying (attempt {n})"),
                None => "Retrying".to_string(),
            };
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.live_tool_label = Some(label);
            s.live_tool_detail = Some(msg.to_string());
            s.live_tool_started_at = Some(now_iso_rfc3339());
        }
        "session.model.selected" => {
            let model_obj = data.get("model").unwrap_or(data);
            if let (Some(provider), Some(id)) = (
                model_obj.get("providerID").and_then(Value::as_str),
                model_obj
                    .get("id")
                    .or_else(|| model_obj.get("modelID"))
                    .and_then(Value::as_str),
            ) {
                let full = format!("{provider}/{id}");
                let variant = model_obj
                    .get("variant")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("default"))
                    .map(str::to_string);
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.current_model = Some(full);
                s.current_effort = variant;
                s.update_window_for_current_model();
            }
        }
        "session.updated" => {
            let session_obj = data.get("session").unwrap_or(data);
            if let Some(title) = session_obj
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .session_title = Some(title.to_string());
            }
        }
        "permission.asked" => {
            handle_incoming_permission(http, state, activity, data, &main_sid, false);
        }
        "permission.replied" => {
            if let Some(req_id) = data
                .get("requestID")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let changed = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let before = s.pending_requests.len();
                    s.pending_requests.retain(|r| r.request_id != req_id);
                    s.pending_requests.len() != before
                };
                if changed {
                    activity.sync();
                }
            }
        }
        "form.created" => {
            let form = data.get("form").unwrap_or(data);
            handle_incoming_form(state, activity, form, &main_sid, false);
        }
        "form.replied" | "form.cancelled" => {
            if let Some(req_id) = data
                .get("formID")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let changed = {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let before = s.pending_requests.len();
                    s.pending_requests.retain(|r| r.request_id != req_id);
                    s.pending_requests.len() != before
                };
                if changed {
                    activity.sync();
                }
            }
        }
        "shell.created" => {
            if let Some(shell_id) = data
                .pointer("/shell/id")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.running_shells.insert(shell_id.to_string());
                s.working = true;
            }
            activity.sync();
        }
        "shell.exited" | "shell.deleted" => {
            if let Some(shell_id) = data
                .pointer("/shell/id")
                .or_else(|| data.get("id"))
                .and_then(Value::as_str)
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.running_shells.remove(shell_id);
                if !s.active_sessions.contains(&main_sid) && s.running_shells.is_empty() {
                    s.working = false;
                }
            }
            reconcile_session(http, state, activity, false);
        }
        _ => {}
    }
}
