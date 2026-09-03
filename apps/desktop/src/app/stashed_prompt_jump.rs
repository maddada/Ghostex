/*
CDXC:SavedPrompts 2026-08-24:
Per-concern module for the Saved Prompts "Go to session" jump. The modal host
window posts `jumpToStashedPromptSession` with the raw gxserver ids a stash row
carries plus the durable provider conversation id (`agentSessionId`); Rust only
bounds those strings and hands them to the sidebar runtime, which owns the
present → restore → resume routing (the same machinery the Project Board's
conversation links use). Rust deliberately resolves nothing here: session
lifecycle state lives in the runtime's presentation, not in the app shell.
*/

use crate::app::consts::*;
use crate::*;

pub(crate) fn gpui_stashed_prompt_session_jump_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onStashedPromptSessionJump==='function'){{bridge.onStashedPromptSessionJump(payload);}}else{{const pending=Array.isArray(bridge.pendingStashedPromptSessionJumps)?bridge.pendingStashedPromptSessionJumps:[];pending.push(payload);bridge.pendingStashedPromptSessionJumps=pending;}}}})(); undefined;"
    )
}

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

impl GhostexGpuiApp {
    pub(crate) fn dispatch_gpui_stashed_prompt_session_jump(
        &mut self,
        project_id: Option<&str>,
        session_id: Option<&str>,
        agent_session_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let project_id = gpui_stashed_prompt_jump_bounded_id(project_id);
        let session_id = gpui_stashed_prompt_jump_bounded_id(session_id);
        let agent_session_id = gpui_stashed_prompt_jump_bounded_id(agent_session_id);
        if project_id.is_none() && session_id.is_none() && agent_session_id.is_none() {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let mut message = serde_json::Map::new();
        if let Some(agent_session_id) = agent_session_id {
            message.insert(
                "agentSessionId".to_string(),
                serde_json::Value::String(agent_session_id),
            );
        }
        if let Some(project_id) = project_id {
            message.insert(
                "projectId".to_string(),
                serde_json::Value::String(project_id),
            );
        }
        if let Some(session_id) = session_id {
            message.insert(
                "sessionId".to_string(),
                serde_json::Value::String(session_id),
            );
        }
        message.insert(
            "type".to_string(),
            serde_json::Value::String(
                GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_TYPE.to_string(),
            ),
        );
        message.insert(
            "version".to_string(),
            serde_json::json!(GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_VERSION),
        );
        let script = gpui_stashed_prompt_session_jump_script(&serde_json::Value::Object(message));
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }
}
