use std::path::PathBuf;

use crate::agent_hooks::read_claude_hook_surface_records;
use crate::resume_lookup::expand_home;

fn written_transcript(path: Option<&str>) -> Option<PathBuf> {
    path.map(expand_home).filter(|path| path.is_file())
}

/// CDXC:SessionIdentity 2026-09-24 WHY:
/// A Claude that starts in a pane and never receives a prompt reports a conversation id through its hooks but never writes a transcript, and before the transcript guard such an id replaced the pane's real conversation (session G3aa6 on 2026-09-21). Resuming it fails with "No conversation found", so a stored id with no transcript on disk resumes the newest conversation this pane wrote instead; its next hook event stores the repaired id.
/// CDXC:SessionIdentity 2026-11-09 WHY:
/// GitHub #521: a Claude session that kept no id at all woke by its title, and the title lookup failed ("Unable to find restorable Claude session id"), although this pane's hooks had named its conversation. A session with no id now takes the newest conversation this pane's hooks named and wrote, the same record the repair above trusts, before falling back to the title.
pub(super) fn written_claude_identity(
    surface_id: Option<&str>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    set_aside_before: Option<f64>,
) -> (Option<String>, Option<String>) {
    let Some(surface_id) = surface_id else {
        return (agent_session_id, agent_session_path);
    };
    // Start fresh sets aside every conversation the pane named before it (`forget_conversation`
    // in session_chat_send_failure.rs).
    let read_records = || {
        let mut records =
            read_claude_hook_surface_records(&crate::paths::get_gxserver_paths(None), surface_id);
        records.retain(|record| set_aside_before.is_none_or(|cutoff| record.updated_at > cutoff));
        records
    };
    let Some(stored_id) = agent_session_id.as_deref() else {
        if written_transcript(agent_session_path.as_deref()).is_some() {
            return (agent_session_id, agent_session_path);
        }
        let records = read_records();
        return match records
            .iter()
            .rev()
            .find(|record| written_transcript(record.agent_session_path.as_deref()).is_some())
        {
            Some(record) => (
                Some(record.agent_session_id.clone()),
                record.agent_session_path.clone(),
            ),
            None => (agent_session_id, agent_session_path),
        };
    };
    if written_transcript(agent_session_path.as_deref()).is_some() {
        return (agent_session_id, agent_session_path);
    }
    let records = read_records();
    if records.iter().any(|record| {
        record.agent_session_id == stored_id
            && written_transcript(record.agent_session_path.as_deref()).is_some()
    }) {
        return (agent_session_id, agent_session_path);
    }
    let Some(replacement) = records.iter().rev().find(|record| {
        record.agent_session_id != stored_id
            && written_transcript(record.agent_session_path.as_deref()).is_some()
    }) else {
        return (agent_session_id, agent_session_path);
    };
    if crate::agent_transcripts::find_claude_transcript(stored_id).is_some() {
        return (agent_session_id, agent_session_path);
    }
    (
        Some(replacement.agent_session_id.clone()),
        replacement.agent_session_path.clone(),
    )
}
