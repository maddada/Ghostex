//! Secret requests (server/src/secret_requests/): the private cards above the composer where the
//! user answers an agent's `ghostex secret request`, and their Save and Decline.
//!
//! The typed value never lives here: the renderer keeps it in its own masked field and hands it
//! over once, in the `answerSecretRequest` action, which this module turns straight into the RPC
//! without storing it. The document carries only what the agent asked and the card's state.
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! An agent asks for a password, key or token with `ghostex secret request`; the card shows its label and reason, a masked field, and Save privately / Decline. The value goes from the field to gxserver, which writes it to an owner-only file the agent's commands read, so it never reaches the transcript, the composer draft, client storage or the model. A request whose agent stopped waiting shows as ended and refuses answers.
//! SEE-ALSO: apps/desktop/src/app/native_chat/secret_request.rs (desktop and web), apps/mobile/app/src/chat/native/cards/SecretRequestCard.tsx (phone), server/src/secret_requests/ (the store and the CLI's wait).

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

const DEFAULT_PLACEHOLDER: &str = "Paste the secret";
const REFERENCE_PLACEHOLDER: &str = "Paste the secret or an op:// reference";
const PRIVACY_NOTE: &str =
    "Kept out of the chat. Saved to a private file only this agent's commands can read, deleted after 24 hours.";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A frame's `secretRequests`: kept as sent, and a card that is no longer open forgets its last
/// refusal.
pub fn receive(state: &mut ChatState, requests: Value) {
    let open: Vec<String> = requests
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|row| text(row, "status") == "pending")
        .map(|row| text(row, "id").to_string())
        .collect();
    state
        .extras
        .panels
        .secret_request_errors
        .retain(|id, _| open.contains(id));
    state.session.secret_requests = Some(requests);
}

/// The cards, oldest first: `[{ id, label, reason, placeholder, status, open, busy, error, note,
/// closedLabel }]`. Answered cards linger briefly so a second client sees them close.
pub fn project(state: &ChatState) -> Value {
    let panels = &state.extras.panels;
    let rows = state
        .session
        .secret_requests
        .as_ref()
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let cards: Vec<Value> = rows
        .iter()
        .filter(|row| !text(row, "id").is_empty() && !text(row, "label").is_empty())
        .map(|row| {
            let id = text(row, "id");
            let status = text(row, "status");
            let open = status == "pending";
            let busy = panels
                .secret_request_calls
                .values()
                .any(|pending| pending == id);
            let accepts_reference = row.get("acceptsReference") == Some(&Value::Bool(true));
            let placeholder = match text(row, "placeholder") {
                "" if accepts_reference => REFERENCE_PLACEHOLDER,
                "" => DEFAULT_PLACEHOLDER,
                placeholder => placeholder,
            };
            let closed_label = match status {
                "saved" => "Saved privately",
                "declined" => "Declined",
                "timedOut" => "Timed out: the agent stopped waiting",
                "cancelled" => "Ended: the agent stopped waiting",
                _ => "",
            };
            json!({
                "id": id,
                "title": "Secret requested",
                "label": text(row, "label"),
                "reason": text(row, "reason"),
                "placeholder": placeholder,
                "status": status,
                "open": open,
                "busy": busy,
                "error": panels.secret_request_errors.get(id).cloned().unwrap_or_default(),
                "note": PRIVACY_NOTE,
                "saveLabel": if busy { "Saving…" } else { "Save privately" },
                "declineLabel": "Decline",
                "closedLabel": closed_label,
                "acceptsReference": accepts_reference,
            })
        })
        .collect();
    Value::Array(cards)
}

/// Save (`{ requestId, secret }`) or Decline (`{ requestId, decline: true }`) of one card. An
/// empty Save does nothing; the renderer keeps its Save button off until the field has text.
pub fn answer(state: &mut ChatState, action: &UserAction, request_id: u64) -> Option<Effect> {
    let id = action
        .param("requestId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())?
        .to_string();
    let open = state
        .session
        .secret_requests
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| text(row, "id") == id && text(row, "status") == "pending")
        });
    let panels = &mut state.extras.panels;
    if !open
        || panels
            .secret_request_calls
            .values()
            .any(|pending| *pending == id)
    {
        return None;
    }
    let decline = action.param("decline") == Some(&Value::Bool(true));
    let answer = if decline {
        json!({ "type": "decline" })
    } else {
        let secret = action.param("secret").and_then(Value::as_str)?;
        if secret.trim().is_empty() {
            return None;
        }
        json!({ "type": "save", "secret": secret })
    };
    panels.secret_request_errors.remove(&id);
    panels.secret_request_calls.insert(request_id, id.clone());
    Some(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSecretRequest,
        params: Box::new(json!({ "requestId": id, "answer": answer })),
    })
}

/// Takes the answer to [`answer`]: a refusal shows on its card (the field keeps the text, so the
/// user can fix it and save again); success waits for gxserver's frame to close the card. `false`
/// when `request_id` is not one of these calls.
pub fn settle(state: &mut ChatState, request_id: u64, answer: Result<&Value, String>) -> bool {
    let panels = &mut state.extras.panels;
    let Some(id) = panels.secret_request_calls.remove(&request_id) else {
        return false;
    };
    if let Err(message) = answer {
        let message = if message.trim().is_empty() {
            "Could not answer the request. Try again.".to_string()
        } else {
            message
        };
        panels.secret_request_errors.insert(id, message);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{ActionKind, UserAction};

    fn state_with(rows: Value) -> ChatState {
        let mut state = ChatState::default();
        receive(&mut state, rows);
        state
    }

    fn row(status: &str) -> Value {
        json!([{
            "id": "r1", "label": "Example token", "reason": "For the example CLI.",
            "status": status, "acceptsReference": true,
        }])
    }

    fn action(params: Value) -> UserAction {
        UserAction {
            kind: ActionKind::AnswerSecretRequest,
            params: params.as_object().cloned().unwrap_or_default(),
        }
    }

    #[test]
    fn a_save_sends_the_value_once_and_never_keeps_it() {
        let mut state = state_with(row("pending"));
        let effect = answer(
            &mut state,
            &action(json!({ "requestId": "r1", "secret": "not-a-real-token" })),
            7,
        )
        .expect("an open card sends its answer");
        let Effect::SendRpc { method, params, .. } = effect else {
            panic!("expected an RPC");
        };
        assert_eq!(method, ChatRpcMethod::AnswerSecretRequest);
        assert_eq!(params["answer"]["secret"], "not-a-real-token");
        let card = project(&state);
        assert_eq!(card[0]["busy"], true);
        assert!(!card.to_string().contains("not-a-real-token"));
        assert!(!format!("{:?}", state.extras.panels).contains("not-a-real-token"));
        // A second press while the first is in flight does nothing.
        assert!(answer(
            &mut state,
            &action(json!({ "requestId": "r1", "secret": "again" })),
            8
        )
        .is_none());
        assert!(settle(&mut state, 7, Err("Could not save".to_string())));
        assert_eq!(project(&state)[0]["error"], "Could not save");
        assert_eq!(project(&state)[0]["busy"], false);
    }

    #[test]
    fn closed_or_empty_answers_send_nothing() {
        let mut state = state_with(row("pending"));
        assert!(answer(
            &mut state,
            &action(json!({ "requestId": "r1", "secret": "  " })),
            1
        )
        .is_none());
        let mut closed = state_with(row("timedOut"));
        assert!(answer(
            &mut closed,
            &action(json!({ "requestId": "r1", "decline": true })),
            1
        )
        .is_none());
        let card = project(&closed);
        assert_eq!(card[0]["open"], false);
        assert_eq!(
            card[0]["closedLabel"],
            "Timed out: the agent stopped waiting"
        );
        assert_eq!(project(&state)[0]["placeholder"], REFERENCE_PLACEHOLDER);
    }
}
