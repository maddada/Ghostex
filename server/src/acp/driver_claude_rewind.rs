//! Rewind for a Native Claude chat: restart Claude Code at the message before the chosen prompt.

use super::*;

/// CDXC:SessionChat 2026-11-24 WHY:
/// A Native Claude chat runs Claude Code in stream-json (print) mode, which honours `--resume <id> --resume-session-at <message uuid>`: the conversation loads only up to that row and the next prompt is written with it as its parent, which is exactly what Terminal's /rewind "Restore conversation" leaves behind (same id, files untouched). Until that next prompt lands, every restart of this Claude process (crash, credential refresh, a slow interrupt, the host starting again after sleep) must resume at the same row, or Claude would remember the turns the chat hides. The rewind gxserver records for the chat readers (session_chat_rewind_state.rs) is that truth on disk, so a host started later reads it too.
/// SEE-ALSO: server/src/session_chat_rewind_native.rs (sets the record, then asks for this restart).
pub(super) fn claude_resume_flags(state: &Mutex<ClaudeState>, resume_id: &str) -> Vec<String> {
    let (fresh, held) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        (s.fresh_session_id.clone(), s.resume_session_at.clone())
    };
    // A rewind to before the first prompt starts an empty conversation under an id gxserver
    // already adopted; until Claude writes its first row there is nothing to resume.
    if fresh.as_deref() == Some(resume_id)
        && crate::agent_transcripts::find_claude_transcript(resume_id).is_none()
    {
        return vec!["--session-id".to_string(), resume_id.to_string()];
    }
    let mut flags = vec!["--resume".to_string(), resume_id.to_string()];
    let at = held.or_else(|| {
        let path = transcript_path_for_session("claude", resume_id)?;
        crate::session_chat_rewind_state::persisted_session_chat_pending_rewind(&path)?.leaf_id
    });
    if let Some(at) = at {
        flags.push("--resume-session-at".to_string());
        flags.push(at);
    }
    flags
}

/// The next turn finished, so its prompt is in the transcript and a plain `--resume` now loads
/// the rewound branch.
pub(super) fn forget_rewind_resume_point(state: &mut ClaudeState) {
    state.resume_session_at = None;
    state.fresh_session_id = None;
}

/// The `rewind` command: `leafId` is the row to resume at, or absent to start an empty
/// conversation (a rewind to the first prompt, which has no row before it).
#[allow(clippy::too_many_arguments)]
pub(super) fn rewind_claude(
    proc: &mut ClaudeProcess,
    spec: &AcpAgentSpec,
    parsed: &HostArgs,
    cwd: &str,
    spool_session_id: &str,
    record_target: Option<&crate::acp::child_lifetime::ChildRecordTarget>,
    state: &Arc<Mutex<ClaudeState>>,
    activity: &ClaudeActivityReporter,
    mode_sync_lock: &Arc<Mutex<()>>,
    child_pid: &Arc<AtomicU32>,
    active_transport: &Arc<Mutex<Option<Arc<ClaudeTransport>>>>,
    process_gen: &Arc<AtomicU64>,
    leaf_id: Option<String>,
) -> Result<Value, String> {
    let previous = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.working || !s.pending_requests.is_empty() {
            return Err("ACP_AGENT_BUSY".to_string());
        }
        let previous = (
            s.native_session_id.clone(),
            s.resume_session_at.clone(),
            s.fresh_session_id.clone(),
        );
        let Some(current) = s.native_session_id.clone() else {
            return Err("REWIND_NOT_FOUND".to_string());
        };
        match &leaf_id {
            Some(leaf) => {
                s.resume_session_at = Some(leaf.clone());
                s.fresh_session_id = None;
                s.native_session_id = Some(current);
            }
            None => {
                let fresh = uuid::Uuid::new_v4().to_string();
                s.resume_session_at = None;
                s.fresh_session_id = Some(fresh.clone());
                s.native_session_id = Some(fresh);
            }
        }
        previous
    };
    let restart = |proc: &mut ClaudeProcess| -> Result<(), String> {
        let new_gen = process_gen.fetch_add(1, Ordering::AcqRel) + 1;
        *active_transport.lock().unwrap_or_else(|p| p.into_inner()) = None;
        stop_claude_gracefully(
            &proc.transport,
            &mut proc.child,
            &mut proc.lifetime,
            child_pid,
        );
        let (resume_id, model, effort, read_only, level) = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.child_pid = 0;
            s.child_exited = false;
            s.usage_used_tokens = None;
            s.clear_live_turn_output();
            (
                s.native_session_id.clone(),
                s.current_model.clone(),
                s.current_effort.clone(),
                s.read_only,
                s.approval_level,
            )
        };
        *proc = spawn_and_init_claude(
            spec,
            parsed,
            cwd,
            spool_session_id,
            record_target,
            resume_id.as_deref(),
            false,
            model.as_deref(),
            effort.as_deref(),
            read_only,
            level,
            state,
            activity,
            mode_sync_lock,
            child_pid,
            active_transport,
            process_gen,
            new_gen,
        )?;
        Ok(())
    };
    if let Err(err) = restart(proc) {
        // Put Claude back where it was, so a refused rewind changes nothing.
        {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.native_session_id = previous.0;
            s.resume_session_at = previous.1;
            s.fresh_session_id = previous.2;
        }
        if let Err(recover) = restart(proc) {
            state.lock().unwrap_or_else(|p| p.into_inner()).child_exited = true;
            eprintln!("[Claude Native] Could not restart Claude after a refused rewind: {recover}");
        }
        activity.sync();
        return Err(format!("REWIND_FAILED: {err}"));
    }
    let s = state.lock().unwrap_or_else(|p| p.into_inner());
    Ok(json!({
        "sessionId": s.native_session_id,
        "leafId": leaf_id,
        "childPid": s.child_pid,
    }))
}
