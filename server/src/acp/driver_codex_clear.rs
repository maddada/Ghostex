//! `/clear` for a Native Codex chat: a new thread in the same session, as `/clear` does in the
//! Codex terminal app.

use super::*;

/// Whether a chat send is Codex's `/clear` (which the app-server does not know: sent as a prompt,
/// it reached the model as words).
pub(super) fn is_clear_command(text: &str, images: &[String]) -> bool {
    images.is_empty() && text.trim().eq_ignore_ascii_case("/clear")
}

/// CDXC:SessionChat 2026-11-25 WHY:
/// GitHub #755: `ghostex clear-command` refused every Native session (it reads the agent's input
/// box off the terminal, which a Native session's terminal never shows), and a lead that reuses one
/// worker for routine jobs could not start it over. Codex's app-server has no clear request, so the
/// host stops the app-server and starts it again with no thread to resume
/// (`restart_codex_process` with `thread_id` cleared): `thread/start` then opens the new thread with
/// the same folder, approvals, sandbox, profile and instructions the session started with, and each
/// turn still names the model and effort picked in the chat. The new thread id reaches gxserver
/// through the next activity sync, like a resume does. The chat's own title is kept.
/// SEE-ALSO: server/src/server/session_clear_http.rs (the route), driver_claude.rs (Claude's own
/// `/clear` over stream-json needs nothing from the host).
#[allow(clippy::too_many_arguments)]
pub(super) fn start_new_thread(
    proc_opt: &mut Option<CodexProcess>,
    cfg: &CodexSpawnConfig,
    state: &Arc<Mutex<CodexState>>,
    activity: &CodexActivityReporter,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<CodexTransport>>>>,
    process_gen: &Arc<AtomicU64>,
) -> Result<Value, String> {
    let previous = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working || !s.pending_requests.is_empty() {
            return Err("ACP_AGENT_BUSY".to_string());
        }
        s.usage_used_tokens = None;
        s.last_turn_interrupted = false;
        s.last_turn_stop_error = None;
        s.last_error = None;
        s.thread_path = None;
        s.thread_id.take()
    };
    if let Err(err) = restart_codex_process(
        proc_opt,
        cfg,
        state,
        activity,
        child_pid,
        active_transport,
        process_gen,
    ) {
        // The old thread stays the session's: a failed start must not leave it without one.
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.thread_id.is_none() {
            s.thread_id = previous;
        }
        return Err(err);
    }
    let thread_id = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .thread_id
        .clone()
        .ok_or_else(|| "Codex did not open a new thread.".to_string())?;
    println!("\r\n[Codex Native] Started a new conversation.");
    let _ = std::io::stdout().flush();
    Ok(json!({ "accepted": true, "cleared": true, "sessionId": thread_id }))
}
