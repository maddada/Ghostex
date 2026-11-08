//! The answers of a loaded question form whose answers message is outside the loaded rows: a jump
//! read's window ends at the form's turn, before the answers that came next. The session's
//! resource index records where those answers start (`answersOffset`), so they are read back once
//! by the form's id (`readSessionChatResource`, the Resources modal's read) and the form card shows
//! the picks that were sent instead of an Answered card with no picks.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::ChatState;
use crate::transcript::page_answers::page_answers_in;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The read-backs, by form id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordedAnswers {
    /// The forms already asked for, one read each.
    pub asked: BTreeSet<String>,
    /// The reads in flight: request id to form id.
    pub reads: BTreeMap<u64, String>,
    /// The answers read back.
    pub answers: BTreeMap<String, Value>,
}

/// The answers read back for this form.
pub fn answers<'a>(state: &'a ChatState, id: &str) -> Option<&'a Value> {
    state.questions.recorded_answers.answers.get(id)
}

/// Asks once for the answers of each loaded form the index records answered in the transcript when
/// neither the loaded rows, the Resources modal nor this client's own send carries them.
pub fn sync(state: &mut ChatState) -> Vec<Effect> {
    let index = state.session.resource_index.view();
    let missing: Vec<String> = state
        .transcript_view
        .question_forms
        .iter()
        .filter(|form| form.error.is_empty())
        .map(|form| form.id.clone())
        .filter(|id| !state.questions.recorded_answers.asked.contains(id))
        .filter(|id| index.answered_in_transcript(id))
        .filter(|id| {
            !state
                .transcript_view
                .page_answers
                .iter()
                .any(|answers| &answers.page_id == id)
        })
        .filter(|id| crate::questions::resource_modal::fetched_answers(state, id).is_none())
        .filter(|id| {
            state
                .questions
                .forms
                .get(id)
                .is_none_or(|form| form.send.is_none() && form.sent.is_none())
        })
        .collect();
    let mut effects = Vec::new();
    for id in missing {
        let request_id = state.core.allocate_request_id();
        let recorded = &mut state.questions.recorded_answers;
        recorded.asked.insert(id.clone());
        recorded.reads.insert(request_id, id.clone());
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::ReadSessionChatResource,
            params: Box::new(json!({
                "projectId": state.identity.project_id,
                "sessionId": state.identity.session_id,
                "id": id,
            })),
        });
    }
    effects
}

/// A read-back settled, or `None` when the id is not one. A read that fails or finds no answers
/// leaves the form Answered without its picks; it is not asked again.
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let id = state.questions.recorded_answers.reads.remove(&request_id)?;
    if let RpcOutcome::Ok { result } = outcome {
        let messages: Vec<ghostex_gx_protocol::ChatMessage> = result
            .get("messages")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        if let Some(found) = page_answers_in(&messages)
            .into_iter()
            .find(|answers| answers.page_id == id)
        {
            state
                .questions
                .recorded_answers
                .answers
                .insert(id, found.answers);
            state.core.request_publish();
        }
    }
    Some(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostex_gx_protocol::ChatMessage;

    const FORM: &str = "```questions\n{\"title\":\"Pick\",\"questions\":[{\"tag\":\"Q1\",\"title\":\"Which?\",\"choices\":[\"A\",\"B\"]}]}\n```";

    fn reply(id: &str, text: &str) -> ChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": "assistant", "source": "transcript", "timestamp": 1,
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    /// A chat whose loaded rows end at a form's turn, with the index recording its answers later.
    fn window_without_the_answers() -> (ChatState, String) {
        let mut state = ChatState::default();
        state.transcript_view.question_forms =
            crate::transcript::question_forms::question_forms(&[reply("m", FORM)]);
        let id = state.transcript_view.question_forms[0].id.clone();
        state.session.resource_index = vec![json!({
            "id": id, "kind": "form", "status": "answered", "offset": 100, "answersOffset": 200
        })]
        .into();
        (state, id)
    }

    #[test]
    fn a_loaded_form_reads_back_its_answers_from_outside_the_window() {
        let (mut state, id) = window_without_the_answers();
        let form = &crate::questions::forms::project(&state)[&id];
        assert_eq!(form["state"], "answered", "settled before the read lands");
        assert_eq!(form["canSend"], false);
        let effects = sync(&mut state);
        let request_id = match &effects[..] {
            [Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::ReadSessionChatResource,
                ..
            }] => *request_id,
            other => panic!("{other:?}"),
        };
        assert!(sync(&mut state).is_empty(), "one read per form");
        let answers = crate::transcript::question_forms::answers_message(
            &state.transcript_view.question_forms[0],
            &json!({"Which?": "B"}),
        );
        let result = json!({"id": id, "messages": [
            serde_json::to_value(reply("m", FORM)).unwrap(),
            {"id": "a", "role": "user", "source": "transcript", "timestamp": 2,
                "blocks": [{"type": "text", "text": answers}]},
        ]});
        assert!(rpc_settled(&mut state, request_id, &RpcOutcome::Ok { result }).is_some());
        let form = &crate::questions::forms::project(&state)[&id];
        assert_eq!(form["state"], "answered");
        assert_eq!(
            form["drafts"][0]["indices"],
            json!([1]),
            "the sent pick shows"
        );
        // It opens and closes like any answered form.
        crate::questions::forms::toggle(&mut state, &id);
        assert_eq!(
            crate::questions::forms::project(&state)[&id]["collapsed"],
            false
        );
    }

    #[test]
    fn answers_only_in_the_queue_are_not_read_back() {
        let (mut state, id) = window_without_the_answers();
        state.session.resource_index[0]
            .as_object_mut()
            .unwrap()
            .remove("answersOffset");
        assert!(sync(&mut state).is_empty());
        assert!(answers(&state, &id).is_none());
    }
}
