//! "For later": a question form or question page the reader put off. It stays answerable, but it
//! no longer counts as waiting: no pink dot on the session row, no count or ping on the ? pill, no
//! notification. gxserver keeps the list (`laterQuestionIds`, server/src/session_chat_open_questions.rs)
//! so every client agrees and it survives restarts; this side shows a change at once and keeps it
//! shown until the wire agrees.
//!
//! CDXC:SessionStatus 2026-10-27 DECISION:
//! User: "i wish i could just mark an inline html as "For later" so it isn't bothering me with pink dot or saying "unanswered" but then i can go back to it any time to answer it". A page card and a question form offer "For later" while they wait, and "Back to unanswered" once put off. A For-later question leaves the session's question count (the sidebar's pink dot, `answer requested (n)`), the ? pill's count, ping and the turn-end notification, and is listed under For later in the ? pill's list. Answering it moves it to Answered; gxserver then forgets the mark.
//! SEE-ALSO: server/src/session_chat_open_questions.rs (the stored list and the count), server/src/session_chat_send/answer_http.rs (`questionForLater`), questions/open_questions.rs (the pill and its list), apps/desktop/src/app/native_chat/page_card.rs and question_form.rs, apps/mobile/app/src/chat/native/transcript/PageCard.tsx and QuestionForm.tsx.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The marks set here that the wire has not confirmed yet, by form or page id: whether it is For
/// later, and the call that set it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaterState {
    pub pending: BTreeMap<String, (bool, u64)>,
    /// The removals set here that the wire has not confirmed yet, by id, and the call that set them.
    pub removing: BTreeMap<String, u64>,
}

/// The forms and pages put off For later: gxserver's list with this client's changes on top.
pub fn later_ids(state: &ChatState) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = state.session.later_question_ids.iter().cloned().collect();
    for (id, (later, _)) in &state.questions.later.pending {
        if *later {
            ids.insert(id.clone());
        } else {
            ids.remove(id);
        }
    }
    ids
}

/// The forms and pages the reader removed without answering: gxserver's list with this client's
/// removals on top. They no longer wait, count or notify, and the list shows them under Earlier,
/// dimmed, as Canceled.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User, of the Chat Resources list's For later rows: "when i hover on it then we should show the restore button and a new x button to remove a question without answering it". The X removes a form or page for good: it leaves the unanswered and For later counts, the ? pill's stepping and the header's "N for later", moves to Earlier, is kept across restarts in gxserver beside the For later list, and sends nothing to the agent. Then (2026-10-30, Q797): "Canceled" is the word, on the row in the list and on the question's card in the chat in place of For later or Waiting; the form still opens to be read but takes no answer (no picks, notes, Skip all or Send), and the phone's rows and form card do the same. Supersedes the row being marked like a withdrawn question.
pub fn removed_ids(state: &ChatState) -> BTreeSet<String> {
    state
        .session
        .removed_question_ids
        .iter()
        .chain(state.questions.later.removing.keys())
        .cloned()
        .collect()
}

/// `questionRemove {id}`: removes a form or page without answering it.
pub fn remove(state: &mut ChatState, id: &str) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() || removed_ids(state).contains(id) {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .removing
        .insert(id.to_string(), request_id);
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(json!({
            "kind": "questionRemove",
            "questionId": id,
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    }]
}

/// `questionForLater {id, later}`: puts a form or page off For later, or back to unanswered.
pub fn set(state: &mut ChatState, id: &str, later: bool) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() || later_ids(state).contains(id) == later {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .pending
        .insert(id.to_string(), (later, request_id));
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(json!({
            "kind": "questionForLater",
            "questionId": id,
            "later": later,
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    }]
}

/// `snoozeQuestion {id, hours}`: puts a form or page off For later for `hours` (1, 6 or 24); gxserver
/// brings it back by itself at the end (server/src/session_chat_open_questions.rs `LATER_UNTIL_KEY`).
/// A snooze again on one already put off moves its end.
///
/// CDXC:SessionChat 2026-11-10 DECISION:
/// User: "for any ask i should be able to click 'Snooze' and snooze it for 1h 6h 24h or cancel it".
/// Snooze is For later with an end, not a second kind of putting off; Cancel is the removal
/// (`remove`) that also tells the agent (`cancel`).
pub fn snooze(state: &mut ChatState, id: &str, hours: u64) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .pending
        .insert(id.to_string(), (true, request_id));
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(json!({
            "kind": "questionForLater",
            "questionId": id,
            "later": true,
            "hours": hours.clamp(1, 24 * 14),
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    }]
}

/// `cancelQuestion {id}`: removes a form or page for good (`remove`) and tells the agent which
/// questions the user cancelled, as an answer is sent back.
pub fn cancel(state: &mut ChatState, id: &str) -> Vec<Effect> {
    let tags: Vec<String> = state
        .transcript_view
        .askers
        .iter()
        .find(|asker| asker.id == id)
        .map(|asker| asker.tags.clone())
        .unwrap_or_default();
    let mut effects = remove(state, id);
    if effects.is_empty() {
        return effects;
    }
    let text = if tags.is_empty() {
        "I dismissed your questions there: no need to ask them again.".to_string()
    } else {
        format!(
            "I dismissed {}: no need to ask {} again.",
            tags.join(", "),
            if tags.len() == 1 { "it" } else { "them" }
        )
    };
    let request_id = state.core.allocate_request_id();
    effects.push(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(json!({
            "text": text,
            "sendRequestId": format!("question-cancel-{}-{id}", state.identity.session_id),
        })),
    });
    effects
}

/// `questionsReply {id, text}`: the reader's own words on a whole form or page, from its dock
/// row's Reply, sent to the agent as a message without touching the chat box. `text` comes already
/// tagged with the questions it still asks ("Q12 Q13: words", `extras::work_cards`), as a work
/// item ask's Reply comes as "W7 Q12: words". The form or page stays open: the words answer none
/// of its questions by themselves.
pub fn reply(state: &mut ChatState, id: &str, text: &str) -> Vec<Effect> {
    let text = text.trim();
    if id.is_empty() || text.is_empty() {
        return Vec::new();
    }
    // The same words on the same form or page are one send on every client and run, so a second
    // press is dropped by the send ledger (FNV-1a, as `coordinator_board`'s answers are).
    let hash = format!("{id}\n{text}")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    let session: String = state.identity.session_id.chars().take(40).collect();
    let request_id = state.core.allocate_request_id();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(json!({
            "text": text,
            "sendRequestId": format!("questions-reply-{session}-{hash:016x}"),
        })),
    }]
}

/// The call's answer, or `None` when it is not one of these. A refusal drops the change and says
/// why; a success keeps it shown until the wire carries it (`settle`).
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let marked = state
        .questions
        .later
        .pending
        .iter()
        .find(|(_, (_, request))| *request == request_id)
        .map(|(id, _)| id.clone());
    let removed = state
        .questions
        .later
        .removing
        .iter()
        .find(|(_, request)| **request == request_id)
        .map(|(id, _)| id.clone());
    if marked.is_none() && removed.is_none() {
        return None;
    }
    let RpcOutcome::Err { message, .. } = outcome else {
        return Some(Vec::new());
    };
    let what = if let Some(id) = marked {
        state.questions.later.pending.remove(&id);
        "change For later"
    } else {
        if let Some(id) = removed {
            state.questions.later.removing.remove(&id);
        }
        "remove the question"
    };
    state.core.request_publish();
    Some(vec![Effect::Toast {
        level: "error".to_string(),
        message: format!("Couldn't {what}: {message}"),
    }])
}

/// Forgets the changes the wire now carries.
pub fn settle(state: &mut ChatState) {
    let wire = &state.session.later_question_ids;
    let removed = &state.session.removed_question_ids;
    // Runs on every event: the state is only borrowed mutably when a mark actually settles, so its
    // stamp (`state::Stamped`) stays put otherwise.
    let later = &state.questions.later;
    if !later
        .pending
        .iter()
        .any(|(id, (later, _))| wire.contains(id) == *later)
        && !later.removing.keys().any(|id| removed.contains(id))
    {
        return;
    }
    let later = &mut state.questions.later;
    later
        .pending
        .retain(|id, (later, _)| wire.contains(id) != *later);
    later.removing.retain(|id, _| !removed.contains(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_shows_at_once_and_holds_until_the_wire_agrees() {
        let mut state = ChatState::default();
        let effects = set(&mut state, "page-a", true);
        assert_eq!(effects.len(), 1, "one call to gxserver");
        assert!(later_ids(&state).contains("page-a"));
        // A frame from before the write still says nothing: the mark stays.
        settle(&mut state);
        assert!(later_ids(&state).contains("page-a"));
        *state.session.later_question_ids = vec!["page-a".to_string()];
        settle(&mut state);
        assert!(state.questions.later.pending.is_empty());
        assert!(later_ids(&state).contains("page-a"));
        // Back to unanswered.
        set(&mut state, "page-a", false);
        assert!(!later_ids(&state).contains("page-a"));
        assert!(
            set(&mut state, "page-a", false).is_empty(),
            "already unanswered"
        );
    }

    #[test]
    fn a_refused_mark_is_dropped() {
        let mut state = ChatState::default();
        set(&mut state, "form-a", true);
        let request = state.questions.later.pending["form-a"].1;
        let effects = rpc_settled(
            &mut state,
            request,
            &RpcOutcome::Err {
                code: None,
                message: "no".to_string(),
                endpoint: None,
            },
        );
        assert!(matches!(effects.as_deref(), Some([Effect::Toast { .. }])));
        assert!(!later_ids(&state).contains("form-a"));
    }

    #[test]
    fn a_removal_shows_at_once_holds_until_the_wire_agrees_and_a_refusal_drops_it() {
        let mut state = ChatState::default();
        let effects = remove(&mut state, "page-a");
        assert_eq!(effects.len(), 1);
        assert!(removed_ids(&state).contains("page-a"));
        settle(&mut state);
        assert!(
            removed_ids(&state).contains("page-a"),
            "a frame from before the write"
        );
        *state.session.removed_question_ids = vec!["page-a".to_string()];
        settle(&mut state);
        assert!(state.questions.later.removing.is_empty());
        assert!(
            removed_ids(&state).contains("page-a"),
            "kept from the wire's list"
        );

        remove(&mut state, "form-a");
        let request = state.questions.later.removing["form-a"];
        let effects = rpc_settled(
            &mut state,
            request,
            &RpcOutcome::Err {
                code: None,
                message: "no".to_string(),
                endpoint: None,
            },
        );
        assert!(matches!(effects.as_deref(), Some([Effect::Toast { .. }])));
        assert!(!removed_ids(&state).contains("form-a"));
    }
}
