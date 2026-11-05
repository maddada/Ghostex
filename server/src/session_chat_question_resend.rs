//! When an agent's finished turn holds a ```questions block the chat cannot read, gxserver asks
//! the agent to send those questions again in the one shape the form reads.
//!
//! CDXC:SessionChat 2026-10-17 DECISION:
//! User: "i think for 2nd one u can tell the agent the error faced when the question list isn't right instead of supporting 1 million variations ... u should tell it to reask the question only and not write any other task but with fixed format". The questions block keeps its one canonical shape (no near-miss spellings; superseded 2026-11-05 by the near-misses `parse_form` in packages/gx-visual/src/questions.rs now reads). Once the agent's turn has ended, gxserver queues it a message (`ghostex_gx_visual::questions::resend_message`) with the parser's exact error, a small example of the shape, and the instruction to re-ask only those questions with the same Q-numbers, checked with `ghostex visual check`, and nothing else. At most once per broken block and at most two in a row for a session: a third broken retry keeps its error card and nothing more is sent.
//! SEE-ALSO: packages/gx-visual/src/questions.rs (the parser, the message and its tag), packages/gx-chat-core/src/transcript/question_forms.rs (marks the broken form as asked again) and transcript/noise.rs (folds the message), apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx (the "asked again" line).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use ghostex_gx_chat_core::transcript::question_forms::{form_fences, parse_form, resend_form_id};
use ghostex_gx_visual::questions::{resend_message, QuestionForm};
use serde_json::{json, Map, Value};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::presentation::read_runtime_text;
use crate::server::AppState;
use crate::session_chat::{
    SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatTailPage,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;

/// How many resend messages may follow one another before gxserver stops asking.
const MAX_IN_A_ROW: usize = 2;
/// Messages read from the transcript tail: the finished turn and the requests before it.
const SCAN_LIMIT: usize = 60;
/// How many of the latest turns are read for a broken block ([`form_to_resend`]).
const TURNS_READ: usize = 3;
/// The agent's last words reach its transcript a moment after the turn-end hook.
const SETTLE_DELAY: Duration = Duration::from_millis(1_500);

/// The form ids already asked about in this run, so a turn end seen twice sends once.
fn asked() -> &'static Mutex<HashSet<String>> {
    static ASKED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ASKED.get_or_init(Default::default)
}

/// Called from every place an agent's activity changes (beside the notification feed): when the
/// change is a finished turn, checks it on a background thread.
pub(crate) fn observe_turn_end(state: &AppState, params: &Map<String, Value>, result: &Value) {
    if result.get("enteredAttention").and_then(Value::as_bool) != Some(true)
        || result
            .pointer("/activity/attentionSource")
            .and_then(Value::as_str)
            != Some(TURN_COMPLETE_ATTENTION_SOURCE)
    {
        return;
    }
    let Some(session) = result.get("session").cloned() else {
        return;
    };
    let id = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| params.get(key).and_then(Value::as_str))
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (id("projectId"), id("sessionId")) else {
        return;
    };
    let state = state.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE_DELAY);
        check_turn(&state, &session, &project_id, &session_id);
    });
}

fn text_of(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn transcript_tail(session: &Value) -> Option<Vec<SessionChatMessage>> {
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(
        session_chat_agent_for_session(session).as_deref(),
    )?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    match crate::session_chat::read_session_chat_tail_page(agent, &path, SCAN_LIMIT, None).ok()? {
        SessionChatTailPage::Page { messages, .. } => Some(messages),
        _ => None,
    }
}

/// The broken ```questions block to ask the agent about, from its transcript's tail: the newest
/// one in its last few turns that gxserver has not asked about yet and that no readable form has
/// followed. `None` once [`MAX_IN_A_ROW`] resend messages started the latest turns, so an agent
/// that keeps failing is asked no more.
///
/// CDXC:SessionChat 2026-11-05 WHY:
/// It used to look only after the newest user message. A coordinator's queue hands it the next
/// thread report as soon as its turn ends, often inside the 1.5 s this waits for the transcript to
/// settle, so the broken block was already a turn back and was never asked about: the user saw
/// "These questions could not be shown" and had to paste the error back. The last few turns are
/// read instead, and a block the agent has since re-asked in a readable form is left alone.
pub(crate) fn form_to_resend(messages: &[SessionChatMessage]) -> Option<QuestionForm> {
    let user_turns: Vec<(usize, String)> = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == SessionChatRole::User && !message.queued)
        .map(|(index, message)| (index, text_of(message)))
        .collect();
    // The requests that started the latest turns, newest first: resend after resend means the
    // agent keeps failing, and gxserver stops after MAX_IN_A_ROW.
    let in_a_row = user_turns
        .iter()
        .rev()
        .take_while(|(_, text)| resend_form_id(text).is_some())
        .count();
    if in_a_row >= MAX_IN_A_ROW {
        return None;
    }
    let already_asked: Vec<&str> = user_turns
        .iter()
        .filter_map(|(_, text)| resend_form_id(text))
        .collect();
    let from = user_turns
        .iter()
        .rev()
        .nth(TURNS_READ)
        .map_or(0, |(index, _)| index + 1);
    let forms: Vec<QuestionForm> = messages[from..]
        .iter()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .flat_map(|message| form_fences(&text_of(message)))
        .map(|(_, body)| parse_form(&body))
        .collect();
    let last_readable = forms
        .iter()
        .rposition(|form| form.error.is_empty() && !form.questions.is_empty());
    forms
        .into_iter()
        .enumerate()
        .rev()
        .find(|(index, form)| {
            !form.error.is_empty()
                && last_readable.is_none_or(|readable| readable < *index)
                && !already_asked.contains(&form.id.as_str())
        })
        .map(|(_, form)| form)
}

/// Claims `form_id` for one resend in this run, so a turn end seen twice (or a block still in the
/// tail at the next turn end, before the queued resend reaches the transcript) asks once.
pub(crate) fn claim(form_id: &str) -> bool {
    asked()
        .lock()
        .map(|mut asked| asked.insert(form_id.to_string()))
        .unwrap_or(false)
}

fn check_turn(state: &AppState, session: &Value, project_id: &str, session_id: &str) {
    let Some(messages) = transcript_tail(session) else {
        return;
    };
    let broken = form_to_resend(&messages);
    let Some(form) = broken else {
        return;
    };
    if !claim(&form.id) {
        return;
    }
    // The queue sends it once the agent is idle and nothing waits on its screen, so it never
    // interrupts a turn, with or without a chat open.
    let outcome = crate::server::queue_session_chat_prompt(
        state,
        project_id,
        session_id,
        &resend_message(&form),
        false,
    );
    if let Err(error) = outcome {
        let _ = state.logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "sessionChat.questionsResendFailed".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(error.message),
            details: Some(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "formId": form.id,
            })),
        });
    }
}

#[cfg(test)]
#[path = "session_chat_question_resend_tests.rs"]
mod tests;
