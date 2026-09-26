/*
CDXC:SavedPrompts 2026-08-24:
"Go to session" on a Saved Prompts row. A stash row remembers the raw gxserver ids it was written
from AND the provider conversation id (`agentSessionId`) that outlives them, so the target is
resolved in that order of durability: the session that currently OWNS the conversation, then the
exact session the prompt was stashed from, then the daemon's restore/resume contract for a session
that is gone. Since 2026-09-25 gxserver resolves it (`/api/openConversation`); Rust only bounds the
strings and focuses the answer (gx_store/terminal_lifecycle/open_conversation.rs).
*/

use crate::app::consts::*;
use crate::app::gx_store::terminal_lifecycle::open_conversation::ConversationTarget;
use crate::*;

/// Bound one optional first-party id the same way the command-palette session
/// focus dispatch does: non-empty, length-capped, and free of control
/// characters. Anything else is dropped rather than forwarded.
fn gpui_stashed_prompt_jump_bounded_id(value: Option<&str>) -> Option<String> {
    let value = value.map(str::trim)?;
    if value.is_empty()
        || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_string())
}

/// Project and session ids are gxserver's own ids, which never contain `:`; a combined
/// presentation id is dropped rather than half-resolved.
fn gpui_stashed_prompt_jump_gxserver_id(value: Option<&str>) -> Option<String> {
    gpui_stashed_prompt_jump_bounded_id(value).filter(|id| !id.contains(':'))
}

impl GhostexGpuiApp {
    pub(crate) fn dispatch_gpui_stashed_prompt_session_jump(
        &mut self,
        project_id: Option<&str>,
        session_id: Option<&str>,
        agent_session_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let project_id = gpui_stashed_prompt_jump_gxserver_id(project_id);
        let session_id = gpui_stashed_prompt_jump_gxserver_id(session_id);
        let agent_session_id = gpui_stashed_prompt_jump_bounded_id(agent_session_id);
        if agent_session_id.is_none() && (project_id.is_none() || session_id.is_none()) {
            return false;
        }
        self.gx_store_open_conversation(
            ConversationTarget {
                agent_session_id,
                project_id,
                session_id,
                restore_reason: "stashedPromptJumpToSessionRestore",
                resume_reason: "stashedPromptJumpToSessionResume",
                failure_title: "Couldn't open the session for this prompt",
            },
            cx,
        );
        true
    }
}
