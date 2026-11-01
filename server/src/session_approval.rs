//! Whether a session is stuck on an approval only the user can grant, decided once in gxserver for
//! every client (the sidebar's red dot, the orchestrator's crew badge, the notifications list,
//! `ghostex sessions`), and whether it waits on a question card only the user can answer.

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

/// CDXC:SessionStatus 2026-10-31 WHY:
/// The phone names a waiting session "has a question" from `awaitingAnswer`: a running session holds the question card a hook stored for an ask-user tool call (Claude's AskUserQuestion and the agents that share its hook contract). The card lives from the asking call's PreToolUse, which maps to working, until that call's own completion, so unlike the approval rule above a working title does not end it. It is a separate field because `pendingQuestionCount` (the pink dot, auto-sleep and the bulk sleep guard) counts open question forms and pages only and stays as it was. A sleeping or stopped session's leftover card does not count; a question that exists only in a transcript or on the screen is not covered.
/// SEE-ALSO: `awaitingAnswer` in packages/gx-protocol/src/presentation.rs and packages/shared/gxserver-protocol-presentation.ts, server/src/ghostex_cli/sessions/cli_session.rs and mobile_summary.rs (forwarding).
pub fn session_awaits_answer(session: &Value) -> bool {
    effective_lifecycle_state(session) == "running"
        && session_chat_prompt_setting(session)
            .and_then(|stored| parse_stored_session_chat_prompt(&stored))
            .is_some_and(|prompt| matches!(prompt, SessionChatInteractivePrompt::Question { .. }))
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

    /// The card the PreToolUse hook of Claude's AskUserQuestion stores (the real derivation).
    fn asked_question() -> Value {
        let prompt = crate::session_chat_interactive::derive_session_chat_prompt(
            Some("AskUserQuestion"),
            Some(&json!({
                "questions": [{
                    "question": "Which one?", "header": "Pick", "multiSelect": false,
                    "options": [{ "label": "A", "description": "First" }, { "label": "B", "description": "Second" }],
                }],
            })),
            Some("PreToolUse"),
        )
        .expect("a question card");
        serde_json::to_value(prompt).expect("stored card")
    }

    #[test]
    fn a_stored_question_card_awaits_an_answer_while_the_agent_works() {
        // The asking call's PreToolUse stored the card and set working; it still waits.
        for activity in ["working", "attention", "idle"] {
            assert!(
                session_awaits_answer(&session("running", activity, Some(asked_question()))),
                "{activity}"
            );
        }
    }

    #[test]
    fn an_approval_no_card_or_a_sleeping_session_does_not_await_an_answer() {
        assert!(!session_awaits_answer(&session(
            "running",
            "attention",
            Some(approval())
        )));
        assert!(!session_awaits_answer(&session(
            "running",
            "attention",
            None
        )));
        for lifecycle in ["sleeping", "stopped"] {
            assert!(
                !session_awaits_answer(&session(lifecycle, "working", Some(asked_question()))),
                "{lifecycle}"
            );
        }
    }
}
