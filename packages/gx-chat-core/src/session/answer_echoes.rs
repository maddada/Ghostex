//! Answers a reader sent (from a question page's popup window or a ```questions form) shown in the
//! transcript the moment they leave, as an echo of the message gxserver will type: the "You
//! answered" card with its delivery state, while the form or page already reads Answered.
//!
//! The answers travel through the session's chat queue (server/src/visual_pages/answers.rs), which
//! types them once the agent has been idle for a moment, so the recorded message arrives seconds
//! later. A queued answers row becomes an echo the way a startup send does
//! (`session/startup_sends.rs`), so it keeps its card through the gap between delivery and the
//! transcript, and a form's own send draws one before gxserver has even answered. The echo goes
//! when the transcript carries that page's answers.
//!
//! CDXC:SessionChat 2026-10-14 DECISION:
//! User: "When I answered the question in the chat view (in the HTML visual modal), it took about 5 seconds for the answer to show in the chat. I want it as fast as possible. Could be an optimistic update with a spinner at the bottom left of the answers card until confirmed, but I want the user to see that the action had an effect faster." The answers card shows at once with a spinner at its bottom left, the form or page reads Answered, the confirmed card takes its place without a jump, and a failed delivery turns the card into an error with Retry that keeps the answers.

use ghostex_gx_protocol::{ChatMessage, StartupDelivery};

use crate::session::text::normalize_pending_text;
use crate::state::{ChatContext, ChatState, PendingSend};
use crate::transcript::page_answers::{answers_page_id, page_answers_in};
use crate::transcript::question_forms::answers_message;

/// The id prefix of the echo a form's own send draws before its queue row exists.
const FORM_ECHO_PREFIX: &str = "answers:";

/// Whether [`settle`] has anything to do: a pending send that carries a page's answers, or a form
/// whose send is in flight. Otherwise it would keep every send and add none.
pub fn waiting(state: &ChatState) -> bool {
    state
        .pending
        .sends
        .iter()
        .any(|entry| answers_page_id(&entry.text).is_some())
        || state
            .questions
            .forms
            .values()
            .any(|form| form.send.is_some())
}

/// Keeps the answer echoes in step: one for each form send in flight, none for answers the
/// transcript already carries.
pub fn settle(state: &mut ChatState, context: &ChatContext, transcript: &[ChatMessage]) {
    let confirmed: Vec<String> = page_answers_in(transcript)
        .into_iter()
        .map(|answers| answers.page_id)
        .collect();
    // gxserver's index saw the answers in the transcript, even when the loaded rows no longer hold
    // them or the file has since lost them. Only a row that left counts: a failed row's Retry keeps
    // its card whatever an earlier send of the same page did.
    let indexed: Vec<&str> = state
        .session
        .resource_index
        .iter()
        .filter(|entry| entry.get("status").and_then(serde_json::Value::as_str) == Some("answered"))
        .filter_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
        .collect();
    let mut sends = std::mem::take(&mut state.pending.sends);
    sends.retain(|entry| {
        let Some(page_id) = answers_page_id(&entry.text) else {
            return true;
        };
        if confirmed.contains(&page_id)
            || (entry.left_queue_at_ms.is_some() && indexed.contains(&page_id.as_str()))
        {
            return false;
        }
        // A form's echo lives while its send is in flight, and on as the queue row it became.
        match entry.id.strip_prefix(FORM_ECHO_PREFIX) {
            Some(form) if entry.queued_prompt_id.is_none() => state
                .questions
                .forms
                .get(form)
                .is_some_and(|form| form.send.is_some()),
            _ => true,
        }
    });
    for (id, form_state) in &state.questions.forms {
        let Some(send) = form_state.send.as_ref() else {
            continue;
        };
        if confirmed.contains(id) {
            continue;
        }
        let Some(form) = state
            .transcript_view
            .question_forms
            .iter()
            .find(|form| &form.id == id)
        else {
            continue;
        };
        let text = answers_message(form, &send.answers);
        let key = normalize_pending_text(&text);
        // While gxserver is away the echo says it is reconnecting (`questions/unreachable_retry.rs`).
        let delivery = if send.retry.reconnecting {
            "reconnecting"
        } else {
            "sending"
        };
        if let Some(entry) = sends
            .iter_mut()
            .find(|entry| normalize_pending_text(&entry.text) == key)
        {
            if entry.id.starts_with(FORM_ECHO_PREFIX) {
                if let Some(echo) = entry.startup_delivery.as_mut() {
                    echo.state = delivery.to_string();
                }
            }
            continue;
        }
        sends.push(PendingSend {
            id: format!("{FORM_ECHO_PREFIX}{id}"),
            startup_delivery: Some(StartupDelivery {
                prompt_id: String::new(),
                state: delivery.to_string(),
                error_message: None,
                wait_reason: None,
            }),
            text,
            sent_at_ms: context.now_millis(),
            ..PendingSend::default()
        });
    }
    state.pending.sends = sends;
}

#[cfg(test)]
mod tests {
    use crate::session::startup_sends::pending_with_startup_sends;
    use serde_json::json;

    const ANSWERS: &str = "<ghostex-page-answers page=\"0123456789abcdef0123456789abcdef\" title=\"Mockups\">\n{\n  \"Which layout?\": \"tabs\"\n}\n</ghostex-page-answers>";

    #[test]
    fn queued_answers_become_echoes_with_their_delivery_state() {
        let queue = vec![
            json!({"id": "a", "text": ANSWERS, "state": "queued", "createdAt": "2026-10-14T01:00:00.000Z"}),
            json!({"id": "b", "text": "an ordinary queued prompt", "state": "queued"}),
        ];
        let echoes = pending_with_startup_sends(&[], &queue, Some("claude"));
        assert_eq!(
            echoes.len(),
            1,
            "an ordinary queued prompt stays in the strip"
        );
        assert_eq!(echoes[0].queued_prompt_id.as_deref(), Some("a"));
        assert_eq!(echoes[0].startup_delivery.as_ref().unwrap().state, "queued");
    }

    #[test]
    fn a_waiting_row_carries_why_it_waits() {
        let reason = "Waiting: the agent's input box can't be read yet, trying again.";
        let queue = vec![json!({
            "id": "a", "text": ANSWERS, "state": "queued", "waitReason": reason
        })];
        let echoes = pending_with_startup_sends(&[], &queue, Some("claude"));
        let delivery = echoes[0].startup_delivery.as_ref().unwrap();
        assert_eq!(delivery.wait_reason.as_deref(), Some(reason));
        assert_eq!(
            serde_json::to_value(delivery).unwrap()["waitReason"],
            json!(reason)
        );
        // The reason moves on with the ladder, and leaves when gxserver drops it.
        let next = vec![json!({"id": "a", "text": ANSWERS, "state": "queued"})];
        let echoes = pending_with_startup_sends(&echoes, &next, Some("claude"));
        assert_eq!(
            echoes[0].startup_delivery.as_ref().unwrap().wait_reason,
            None
        );
        // The queue strip's row keeps it too.
        let row: crate::document::QueuedPrompt = serde_json::from_value(
            json!({"id": "b", "text": "x", "state": "queued", "createdAt": "", "updatedAt": "", "waitReason": reason}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&row).unwrap()["waitReason"],
            json!(reason)
        );
    }

    #[test]
    fn a_failed_delivery_keeps_its_error_for_retry() {
        let queue = vec![json!({
            "id": "a", "text": ANSWERS, "state": "failed", "errorMessage": "The agent is not running."
        })];
        let echoes = pending_with_startup_sends(&[], &queue, Some("claude"));
        let delivery = echoes[0].startup_delivery.as_ref().unwrap();
        assert_eq!(delivery.state, "failed");
        assert_eq!(
            delivery.error_message.as_deref(),
            Some("The agent is not running.")
        );
        assert!(
            pending_with_startup_sends(&echoes, &[], Some("claude")).is_empty(),
            "a failed row the reader removed takes its card with it"
        );
    }
}
