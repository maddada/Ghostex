//! Answers to an exported page brought back into the chat (server/src/visual_pages/exported.rs,
//! import.rs). An exported page's Export answers gives the reader a JSON object
//! (`{"ghostexPageAnswers": 1, "page", "title", "answers", …}`); the user drops that file on the
//! chat or pastes the text and presses Enter, and instead of going to the agent as text it goes to
//! gxserver's `importVisualPageAnswers`, which sends the agent the page's usual
//! `<ghostex-page-answers>` message and marks the page answered.
//!
//! CDXC:SessionChat 2026-11-13 SEE-ALSO:
//! The user's decision is on server/src/visual_pages/exported.rs. The desktop chat reads a dropped `.json` file and hands its text to `importPageAnswers` (apps/desktop/src/app/native_chat/page_import_drop.rs).

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The answers an exported page gives are at most this long; anything longer is not one.
const EXPORTED_MAX_BYTES: usize = 256 * 1024;

/// Whether `text` is the JSON an exported page's Export answers gave: one object with the
/// `ghostexPageAnswers` mark and an `answers` object.
pub fn is_exported_answers(text: &str) -> bool {
    let text = text.trim();
    if !text.starts_with('{')
        || text.len() > EXPORTED_MAX_BYTES
        || !text.contains("ghostexPageAnswers")
    {
        return false;
    }
    serde_json::from_str::<Value>(text).is_ok_and(|value| {
        value.get("ghostexPageAnswers").is_some()
            && value.get("answers").is_some_and(Value::is_object)
    })
}

/// `importPageAnswers {text}`, or a send whose text is exported answers (`from_composer`, which
/// clears the chat box once it is taken): hands them to gxserver. Text that is not exported answers
/// says so.
pub fn import(state: &mut ChatState, text: &str, from_composer: bool) -> Vec<Effect> {
    if !is_exported_answers(text) {
        return vec![Effect::Toast {
            level: "error".to_string(),
            message: "This isn't the answers of an exported Ghostex page.".to_string(),
        }];
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .page_imports
        .insert(request_id, text.to_string());
    let mut effects = Vec::new();
    if from_composer {
        effects.push(Effect::ClearComposerIfUnchanged {
            text: text.to_string(),
        });
    }
    effects.push(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ImportVisualPageAnswers,
        params: Box::new(json!({
            "exported": text.trim(),
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    });
    effects
}

/// The import's answer, or `None` when `request_id` is not one: a toast that says where the
/// answers went, or why they did not, with the text put back in the chat box to try again.
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let text = state.questions.page_imports.remove(&request_id)?;
    state.core.claim_refusal();
    let result = match outcome {
        RpcOutcome::Ok { result } => result,
        RpcOutcome::Err { message, .. } => {
            let message =
                if message.contains("importVisualPageAnswers") || message.contains("Unknown") {
                    "This computer's Ghostex can't import answers yet. Update it and try again."
                        .to_string()
                } else {
                    format!("The answers were not imported: {message}")
                };
            return Some(vec![
                Effect::Toast {
                    level: "error".to_string(),
                    message,
                },
                Effect::RestoreReturnedPrompt { text },
            ]);
        }
    };
    let title = result
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("the page");
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return Some(vec![Effect::Toast {
            level: "error".to_string(),
            message: format!("Answers to \"{title}\" were imported. {error}"),
        }]);
    }
    let elsewhere =
        result.get("sessionId").and_then(Value::as_str) != Some(state.identity.session_id.as_str());
    let message = if elsewhere {
        format!("Answers to \"{title}\" were sent to the chat that made the page.")
    } else {
        format!("Answers to \"{title}\" were sent to the agent.")
    };
    Some(vec![Effect::Toast {
        level: "info".to_string(),
        message,
    }])
}
