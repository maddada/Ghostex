//! The pure helpers the renderer asks for one gesture and uses at once.
//!
//! They are not events: the composer asks `composerReferences` on every paint and a right-click
//! asks `transcriptMenu` before it can open a menu, so both need an answer in the same turn rather
//! than a frame later. The deleted QuickJS brain exposed them beside `action` on the same global
//! (`native-host.ts`); the Rust core exposes them as plain functions over `&ChatState`, so the
//! worker can answer one without touching the core's state at all.
//!
//! The answers are serialized straight from the typed values, never through a `serde_json::Value`:
//! the view reads them by key and a detour through a map would re-order the keys a menu row list
//! is built in.

use ghostex_gx_chat_core::composer::keys::ComposerKeyEvent;
use ghostex_gx_chat_core::composer::queries::{
    composer_key_intent, composer_references, reference_menu, send_blocked_toast, transcript_menu,
};
use ghostex_gx_chat_core::query::{answer_attachments_edit, chat_reference_removal};
use ghostex_gx_chat_core::{ChatContext, ChatState};
use serde_json::Value;

/// One helper's answer, or `None` for a name this build does not know.
pub(super) fn answer(
    state: &ChatState,
    context: ChatContext,
    method: &str,
    arguments: &[Value],
) -> Option<Value> {
    let first = arguments.first();
    Some(match method {
        "composerReferences" => to_value(&composer_references(first.and_then(Value::as_str)?)),
        "composerKeyIntent" => {
            let event: ComposerKeyEvent = serde_json::from_value(first?.clone()).ok()?;
            let platform = arguments.get(1).and_then(Value::as_str);
            to_value(&composer_key_intent(&event, platform))
        }
        "referenceMenu" => to_value(&reference_menu(first.and_then(Value::as_str)?)),
        "transcriptMenu" => {
            let request = first?;
            let href = request.get("href").and_then(Value::as_str);
            let selection = request
                .get("selection")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let question_active = request.get("questionActive") == Some(&Value::Bool(true));
            to_value(&transcript_menu(href, selection, question_active))
        }
        // The view passes the reason it already drew; the brain falls back to its own send gate
        // when it does not, which is what `sendBlockedToast()` with no argument does.
        "sendBlockedToast" => {
            let reason = match first.and_then(Value::as_str) {
                Some(reason) => reason.to_string(),
                None => ghostex_gx_chat_core::composer::document::send_blocked(state, &context)?,
            };
            to_value(&send_blocked_toast(&reason))
        }
        "insertAnswerAttachments" => to_value(&answer_attachments_edit(arguments)?),
        "removeChatReference" => to_value(&chat_reference_removal(arguments)?),
        _ => return None,
    })
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
