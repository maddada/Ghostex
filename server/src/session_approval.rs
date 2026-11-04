//! Whether a session is stuck on an approval only the user can grant, decided once in gxserver for
//! every client (the sidebar's red dot, the orchestrator's crew badge, the notifications list,
//! `ghostex sessions`).

use serde_json::Value;

use crate::agents::session_chat_prompt_setting;
use crate::presentation::{effective_lifecycle_state, presentation_activity};
use crate::session_chat_interactive::{
    parse_stored_session_chat_prompt, SessionChatInteractivePrompt,
};

/// CDXC:SessionStatus 2026-10-27 DECISION:
/// User: "Please for permissions let's show a new red dot so user knows stuff is stuck and waiting on him to approve ... This sub thread for outdated dependeincies audit was stuck on this for a long time and i didn't know!!". A running session waits for approval while it holds the approval card a PermissionRequest hook stored (Claude, Codex, ZCode, Mastra, Droid, Kimi, Devin and the other hook agents) and its agent is not working, the same rule that puts a coordinator thread in `waiting`; a working title means the user already approved and the tool runs. A thread whose approval exists only on its screen (Copilot, Antigravity, Grok, Empryo, Cursor) counts from the supervisor's screen reading, also while its hooks say working. The flag is `awaitingApproval` on the presentation session: its row draws the red dot ahead of every other status, and its orchestrator's crew badge turns red.
/// SEE-ALSO: server/src/coordinators/state.rs (`classify_thread_session`, `screen_waits_on_approval`), packages/gx-core/src/sidebar_view/view.rs (`CoordinatorBadgeTone::Approval`), apps/desktop/src/app/native_sidebar/status.rs (`APPROVAL_COLOR`).
pub fn session_awaits_approval(session: &Value, generated_at: &str) -> bool {
    if effective_lifecycle_state(session) != "running" {
        return false;
    }
    if crate::coordinators::screen_waits_on_approval(session) {
        return true;
    }
    presentation_activity(session, generated_at) != "working" && stored_approval_card(session)
}

fn stored_approval_card(session: &Value) -> bool {
    session_chat_prompt_setting(session)
        .and_then(|stored| parse_stored_session_chat_prompt(&stored))
        .is_some_and(|prompt| matches!(prompt, SessionChatInteractivePrompt::Approval { .. }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const NOW: &str = "2026-10-27T00:00:00.000Z";

    fn session(lifecycle: &str, activity: &str, prompt: Option<Value>) -> Value {
        let mut agent_activity = json!({ "activity": activity });
        if let Some(prompt) = prompt {
            agent_activity["sessionChatPrompt"] = json!(prompt.to_string());
        }
        json!({
            "projectId": "p-approval",
            "sessionId": "s-approval",
            "lifecycleState": lifecycle,
            "runtimeSettings": {
                "activity": activity,
                "agentActivity": agent_activity,
            },
        })
    }

    fn approval() -> Value {
        json!({ "kind": "approval", "tool": "Bash", "summary": "rm -rf build" })
    }

    fn question() -> Value {
        json!({
            "kind": "question",
            "questions": [{ "question": "Which one?", "options": [{ "label": "A" }] }],
        })
    }

    #[test]
    fn a_stored_approval_card_waits_for_approval_until_the_agent_works_again() {
        assert!(session_awaits_approval(
            &session("running", "attention", Some(approval())),
            NOW
        ));
        // A title that settled to idle does not answer the card.
        assert!(session_awaits_approval(
            &session("running", "idle", Some(approval())),
            NOW
        ));
        // Working again: the user approved and the tool runs.
        assert!(!session_awaits_approval(
            &session("running", "working", Some(approval())),
            NOW
        ));
    }

    #[test]
    fn a_question_a_finished_turn_or_a_sleeping_session_is_not_an_approval() {
        assert!(!session_awaits_approval(
            &session("running", "attention", Some(question())),
            NOW
        ));
        assert!(!session_awaits_approval(
            &session("running", "attention", None),
            NOW
        ));
        assert!(!session_awaits_approval(
            &session("sleeping", "attention", Some(approval())),
            NOW
        ));
    }
}
