//! Whether a session's agent still has a background command running, read from the same
//! transcript facts as the panel's Background tasks section. Native chats have no terminal footer,
//! so the `backgroundWorkDetectedAt` marker (read from a Terminal-based agent's footer) never
//! covers them; Native idle release asks here instead. BLOCKING.

use serde_json::Value;

use crate::server::AppState;

/// A turn the transcript shows running, a Claude background shell or monitor, or a Codex
/// background command, still running. Other agents' background work is not recorded where this
/// can read it, so they answer by their transcript's turn alone.
pub(crate) fn session_has_running_background_work(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
) -> bool {
    let Ok(resolution) = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    ) else {
        return false;
    };
    // The transcript shows a turn still running, whatever the stored activity says.
    if resolution.working() {
        return true;
    }
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
    let Some(transcript) = resolution.transcript().1.map(std::path::Path::to_path_buf) else {
        return false;
    };
    match agent.as_deref() {
        Some("claude") => super::scan::scan_claude_transcript(&transcript)
            .is_ok_and(|facts| facts.has_running_background()),
        Some("codex") => super::codex::scan_rollout(&transcript)
            .is_ok_and(|facts| facts.has_running_background()),
        _ => false,
    }
}
