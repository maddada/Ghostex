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
/// User: "Please for permissions let's show a new red dot so user knows stuff is stuck and waiting on him to approve ... This sub thread for outdated dependeincies audit was stuck on this for a long time and i didn't know!!". A running session waits for approval while it holds the approval card a PermissionRequest hook stored (Claude, Codex, ZCode, Mastra, Droid, Kimi, Devin and the other hook agents) and its agent is not working, the same rule that puts a coordinator thread in `waiting`; a working title means the user already approved and the tool runs. An approval that exists only on the screen (a permission prompt that holds the agent's input: Copilot, Antigravity, Grok, Empryo, Cursor, and an OMP approval its hooks never announce) counts for every running session, also while its hooks say working: a coordinator's thread from the supervisor's screen reading, any session from the screen marker (`screen_shows_approval`). The flag is `awaitingApproval` on the presentation session: its row draws the red dot ahead of every other status, and its orchestrator's crew badge turns red.
/// SEE-ALSO: server/src/coordinators/state.rs (`classify_thread_session`, `screen_waits_on_approval`), server/src/session_chat_options/detector.rs (sets the screen marker), server/src/session_chat_compacting.rs (`refresh_screen_approval_marker_from_screen`), packages/gx-core/src/sidebar_view/view.rs (`CoordinatorBadgeTone::Approval`), apps/desktop/src/app/native_sidebar/status.rs (`APPROVAL_COLOR`).
pub fn session_awaits_approval(session: &Value, generated_at: &str) -> bool {
    session_awaits_reported_approval(session, generated_at)
        || (effective_lifecycle_state(session) == "running" && screen_shows_approval(session))
}

/// CDXC:SessionStatus 2026-11-21 WHY:
/// The rule before the screen marker: a stored hook approval card, or a coordinator thread's screen wait. The Notifications list reads this one, not `session_awaits_approval`: its overlay turns the session's newest row into an unread "Waiting for your approval" row while an approval waits, so a screen-only approval would have relabelled an older, already read row (a finished turn) as an unread approval with no row of its own. A screen-only approval gives the red dot and the sorting; the bell shows what the agent's hooks reported, as before.
/// SEE-ALSO: server/src/notification_feed/store.rs (`approval_waits`), server/src/notification_feed/producer.rs.
pub fn session_awaits_reported_approval(session: &Value, generated_at: &str) -> bool {
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

/// Runtime-settings key holding the `detectedAt` of the approval the session's screen shows.
pub(crate) const SESSION_CHAT_SCREEN_APPROVAL_DETECTED_AT_KEY: &str =
    "sessionChatScreenApprovalDetectedAt";

/// CDXC:SessionStatus 2026-11-21 WHY:
/// OMP asks for an eval cell's tool calls (`commit`, any prelude helper) with the same "Allow tool" dialog, but that path raises no `tool_approval_requested`, so Ghostex's OMP extension sends no PermissionRequest; its hooks stay at the eval call's working and a standalone session kept its orange working dot while the chat's notice read "Allow this? (permissionPrompt)" (Ghostex 11.13.0, OMP 19.0.1). Only coordinator threads had a screen record, so the screen reading now leaves a marker on any session, set and cleared by the one funnel every capture passes through, like the compaction marker; a marker only the screen can clear is read again by the stall tick until a capture shows the prompt gone.
pub(crate) fn screen_shows_approval(session: &Value) -> bool {
    session
        .pointer(&format!(
            "/runtimeSettings/{SESSION_CHAT_SCREEN_APPROVAL_DETECTED_AT_KEY}"
        ))
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

/// What one capture does to the screen marker: `(projected, write)`. `projected` is the cache's new
/// verdict, `write` is `Some(Some(detectedAt))` to set the marker, `Some(None)` to clear it, `None`
/// to leave it. Only a whole capture changes the verdict; a failed one keeps the last.
pub(crate) fn screen_approval_transition(
    previous: Option<bool>,
    captured: bool,
    notice: Option<&crate::session_chat_notice::SessionChatTerminalNotice>,
) -> (Option<bool>, Option<Option<String>>) {
    let approval = notice.filter(|notice| notice.is_screen_approval());
    let projected = if captured {
        Some(approval.is_some())
    } else {
        previous
    };
    let write = (projected != previous)
        .then(|| projected.map(|_| approval.map(|notice| notice.detected_at.clone())))
        .flatten();
    (projected, write)
}

/// CDXC:SessionStatus 2026-10-31 WHY:
/// A running session holds the question card a hook stored for an ask-user tool call (Claude's AskUserQuestion and the agents that share its hook contract). The card lives from the asking call's PreToolUse, which maps to working, until that call's own completion, so unlike the approval rule above a working title does not end it. `pendingQuestionCount` still counts open question forms and pages separately; desktop question status and sleep guards combine that count with this flag (CDXC:SessionStatus 2026-11-03 in packages/gx-protocol/src/presentation.rs). A sleeping or stopped session's leftover card does not count; a question that exists only in a transcript or on the screen is not covered.
/// SEE-ALSO: `awaitingAnswer` in packages/gx-protocol/src/presentation.rs and packages/shared/gxserver-protocol-presentation.ts, server/src/ghostex_cli/sessions/cli_session.rs and mobile_summary.rs (forwarding).
pub fn session_awaits_answer(session: &Value) -> bool {
    effective_lifecycle_state(session) == "running"
        && session_chat_prompt_setting(session)
            .and_then(|stored| parse_stored_session_chat_prompt(&stored))
            .is_some_and(|prompt| matches!(prompt, SessionChatInteractivePrompt::Question { .. }))
}

/// The question state over stored session data, matching the presentation row's `has_question`.
pub fn session_has_question(session: &Value) -> bool {
    crate::session_chat_async_questions::pending_question_count(session) > 0
        || crate::session_chat_open_questions::open_question_count(session) > 0
        || session_awaits_answer(session)
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

    /// OMP's approval for a `commit` call from an eval cell, as Ghostex read it on 2026-11-21.
    const OMP_EVAL_APPROVAL_SCREEN: &str =
        include_str!("session_chat_tests/fixtures/omp-eval-commit-approval.txt");

    #[test]
    fn an_approval_on_the_screen_waits_for_approval_while_hooks_say_working() {
        let mut waiting = session("running", "working", None);
        waiting["runtimeSettings"][SESSION_CHAT_SCREEN_APPROVAL_DETECTED_AT_KEY] =
            json!("2026-11-21T11:30:00.000Z");
        assert!(session_awaits_approval(&waiting, NOW));
        waiting["lifecycleState"] = json!("sleeping");
        assert!(!session_awaits_approval(&waiting, NOW));
    }

    #[test]
    fn the_omp_approval_on_screen_sets_the_mark_and_a_clean_capture_clears_it() {
        let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
            Some("omp"),
            OMP_EVAL_APPROVAL_SCREEN,
        )
        .expect("the approval notice");
        assert!(notice.is_screen_approval());
        assert_eq!(
            screen_approval_transition(None, true, Some(&notice)),
            (Some(true), Some(Some(notice.detected_at.clone())))
        );
        // The same approval read again, or a capture that failed, writes nothing.
        assert_eq!(
            screen_approval_transition(Some(true), true, Some(&notice)),
            (Some(true), None)
        );
        assert_eq!(
            screen_approval_transition(Some(true), false, None),
            (Some(true), None)
        );
        // A whole capture without it clears the mark.
        assert_eq!(
            screen_approval_transition(Some(true), true, None),
            (Some(false), Some(None))
        );
        // A permission prompt Ghostex answers itself is not the user's.
        let mut auto = notice.clone();
        auto.auto_trust = true;
        assert_eq!(
            screen_approval_transition(None, true, Some(&auto)),
            (Some(false), Some(None))
        );
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
