//! CDXC:Sessions 2026-09-11 DECISION:
//! User: add "Unpark after sending a message" (on by default), which unparks a parked session when the user sends an actual message there.
//! gxserver owns it because it is the one place that sees prompts from every surface: chat sends from the desktop, web and mobile clients all go through the chat queue runtime, and terminal-typed prompts arrive as agent hook events.
//! Only explicit prompt-submit hook events count as a message. The first user message rides on every later hook event as cached sidecar state, so its presence says nothing about this event, and tool or turn events fire long after the user last typed.
//! Option commands Ghostex types on the user's behalf and raw keys are not messages either.
//! SEE-ALSO: server/src/server/agent_http.rs, server/src/session_chat_queue_runtime.rs, packages/shared/ghostex-settings/defaults.ts.

use std::path::Path;

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";
const SETTING_KEY: &str = "unparkAfterSendingMessage";

/// Absent file, unparseable file, or absent key all mean the shipped default: on.
/// Only an explicit `false` turns the behaviour off.
pub(crate) fn unpark_after_sending_message_enabled(app_config_dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(app_config_dir.join(SETTINGS_FILE_NAME)) else {
        return true;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&text) else {
        return true;
    };
    settings.get(SETTING_KEY).and_then(Value::as_bool) != Some(false)
}

/// Whether an agent hook event is the user submitting a prompt: Claude's and
/// Mastra's `UserPromptSubmit`, or Cursor's `beforeSubmitPrompt`. Codex has no
/// prompt-submit hook, so a prompt typed into a Codex terminal is not seen here;
/// its chat sends still reach the queue runtime.
pub(crate) fn is_user_prompt_submit_hook_event(params: &Map<String, Value>) -> bool {
    params
        .get("eventName")
        .or_else(|| params.get("rawEventName"))
        .and_then(Value::as_str)
        .map(|event_name| event_name.trim().to_ascii_lowercase())
        .is_some_and(|event_name| {
            matches!(
                event_name.as_str(),
                "userpromptsubmit" | "beforesubmitprompt"
            )
        })
}

/// Clears `isParked` on a session the user just sent a message to. Returns
/// whether the row changed so the caller publishes a presentation delta. The
/// parked flag is read before the settings file so the common case, a session
/// that is not parked, costs no file read.
pub(crate) fn unpark_session_after_user_message(
    repository: &DomainRepository<'_>,
    app_config_dir: &Path,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(false);
    };
    if session.get("isParked").and_then(Value::as_bool) != Some(true) {
        return Ok(false);
    }
    if !unpark_after_sending_message_enabled(app_config_dir) {
        return Ok(false);
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert("isParked".to_string(), Value::Bool(false));
    repository.update_session(&update)?;
    Ok(true)
}
