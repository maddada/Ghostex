//! A box session's first prompt travels inside its launch command; the startup send a client
//! queues for the same prompt is answered here instead of being typed a second time.

use serde_json::{json, Value};

use super::session::patch_agentbox_record;

use crate::domain::{DomainRepository, DomainStateError};

/// `runtimeSettings.agentbox.launchPrompt`: `pending` while the prompt carried by the launch has
/// not been matched with its startup send, `delivered` after.
pub(crate) const LAUNCH_PROMPT_KEY: &str = "launchPrompt";

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Claims the startup send of the prompt a box session's launch already carried, so the caller
/// answers it without delivering it. `false` for every other send (later prompts, other sessions).
///
/// CDXC:AgentBox 2026-10-01 WHY: create flows store the prompt as `firstUserMessage` and then queue the same text as a startup send. A box launch hands that prompt to the agent itself (`agentbox <agent> … -- -- "<prompt>"`), so typing the startup send too would submit it twice. The prompt is not lost when the box never comes up: until `agentbox list` shows the box, every restore reruns the create command, prompt included (restore.rs). The claim is consume-once and keyed on the exact text, so any other message still goes through the normal queue.
pub(crate) fn claim_launch_prompt_echo(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<bool, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(false);
    };
    if session
        .pointer("/runtimeSettings/agentbox/launchPrompt")
        .and_then(Value::as_str)
        != Some("pending")
    {
        return Ok(false);
    }
    let first_prompt = session
        .pointer("/runtimeSettings/firstUserMessage")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if normalized(first_prompt).is_empty() || normalized(first_prompt) != normalized(text) {
        return Ok(false);
    }
    // Conditional on `pending`, so two concurrent startup sends cannot both be answered.
    patch_agentbox_record(
        repository.connection(),
        project_id,
        session_id,
        LAUNCH_PROMPT_KEY,
        &json!("delivered"),
        Some("pending"),
    )
}
