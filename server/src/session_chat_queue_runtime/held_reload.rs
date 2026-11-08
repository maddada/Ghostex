//! The input-box ladder's last step: a message held because the agent's input box is not on screen
//! gets the agent reloaded the way the user's Full Reload does, once the repaints have failed.
//!
//! CDXC:SessionChat 2026-11-08 WHY:
//! Five thread reports sat "Waiting for agent" on the idle coordinator S90:P3lv0:G9jt7 from 06:47
//! to about 10:40 UTC, the first one saying the input box had not been on screen for 226 minutes,
//! until the user's Full Reload delivered them at once. The ladder (wait_reason.rs) read the
//! screen fresh, sent Claude's Ctrl+L 206 times and resized the terminal 104 times, and every one
//! drew the same frame: a cause no repaint reaches (here a rating survey Ghostex did not recognise,
//! session_chat_claude_rating_survey.rs) holds a message for as long as the agent lives. So after
//! `INPUT_BOX_RELOAD_AFTER_MS` (wait_reason.rs) the scheduler hands the row here, under its delivery claim so no
//! send races the reload. One more fresh read after a moment must still show the box missing with
//! nothing in its place (no notice, dialog, question, panel a send closes, compaction or starting
//! CLI: the same guard as the resize step, so nothing ever types into a real prompt), the session
//! must be idle by its hooks, and send_heal.rs refuses a turn in progress, a pending question,
//! background tasks and a second automatic restart within ten minutes. Then the session sleeps and
//! wakes on its own conversation, the row becomes a startup send so it waits for the new input box
//! instead of being marked Not sent, and the reload or its refusal is logged with the screen
//! (`sessionChatQueueReloadedAgent`, `sessionChatQueueReloadRefused` in
//! session-chat-send-failures.jsonl). Once per held row; a refusal that passes (a turn, the
//! ten-minute limit) is tried again two minutes later.
//! SEE-ALSO: scheduler.rs (when the step is due), repaint_nudge.rs (the guard), send_heal.rs
//! (`reload_idle_agent_for_input_box`).

use std::{sync::Arc, time::Duration};

use super::*;

/// How long the last look waits, so a frame the ladder's resize or Ctrl+L is still painting is not
/// taken for a missing box.
const SETTLE_BEFORE_LAST_LOOK: Duration = Duration::from_secs(1);

/// Reloads the agent of a session whose head row `prompt_id` has waited on a missing input box
/// past `INPUT_BOX_RELOAD_AFTER_MS` (wait_reason.rs), when a fresh read still shows nothing but a missing box.
pub(super) async fn reload_for_held_message(
    state: Arc<AppState>,
    project_id: String,
    session_id: String,
    prompt_id: String,
) -> bool {
    let look = {
        let state = state.clone();
        let (project_id, session_id) = (project_id.clone(), session_id.clone());
        tokio::task::spawn_blocking(move || {
            std::thread::sleep(SETTLE_BEFORE_LAST_LOOK);
            let session = super::held_redraw::read_session(&state, &project_id, &session_id)?;
            let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
            let detection = SessionChatOptionDetector::new(&state).detect_blocking(
                &project_id,
                &session_id,
                agent.as_deref(),
                true,
            );
            Some((session, agent, detection))
        })
        .await
        .ok()
        .flatten()
    };
    let Some((session, agent, detection)) = look else {
        return false;
    };
    // The box is back, or something a person must answer is on screen: nothing to heal here.
    if !super::repaint_nudge::screen_wants_repaint(agent.as_deref(), &detection) {
        return false;
    }
    let screen = &detection.composer.screen_tail;
    let missing = detection
        .composer
        .reason
        .as_deref()
        .unwrap_or("The agent's input box is not on screen.");
    let refused = |reason: &str| {
        crate::session_chat_send_diagnostics::record_send_recovery(
            &state,
            "sessionChatQueueReloadRefused",
            &project_id,
            &session_id,
            &format!("{missing} The agent was not reloaded: {reason}."),
            screen,
        );
    };
    if !super::held_redraw::session_is_idle(&session) {
        refused("it is not idle by its hooks");
        return false;
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    let Ok(target) = resolve_session_chat_send_target(&state, &params, "queueInputBoxReload")
    else {
        refused("the session could not be read");
        return false;
    };
    if let Err(refusal) = super::send_heal::reload_idle_agent_for_input_box(&state, &target).await {
        refused(refusal.reason);
        return refusal.lasting;
    }
    // A row the queue would mark Not sent after two minutes of no box waits for the reloaded
    // agent's input box instead, like any message held for a starting agent.
    crate::session_chat_queue::hold_queued_prompt_for_wake(
        &state.paths,
        &project_id,
        &session_id,
        &prompt_id,
    );
    broadcast_session_chat_queue_state(&state, &project_id, &session_id);
    crate::session_chat_send_diagnostics::record_send_recovery(
        &state,
        "sessionChatQueueReloadedAgent",
        &project_id,
        &session_id,
        &format!(
            "{missing} The repaints did not bring it back on an idle agent, so the agent was reloaded on its own conversation (sleep and wake, as Full Reload does); the message waits for its new input box."
        ),
        screen,
    );
    true
}
