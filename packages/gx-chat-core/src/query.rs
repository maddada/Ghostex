//! The pure helpers a host asks the core directly (the phone's `MobileChatCore::query`): they
//! read state and return an answer, and change nothing.
//!
//! Serialized straight from the typed answer, never through a [`Value`]: a detour through
//! `serde_json::Value` would re-order an object's keys.

use serde_json::Value;

use crate::composer::keys::ComposerKeyEvent;
use crate::composer::queries::{
    composer_key_intent, composer_references, insert_answer_attachments, reference_menu,
    remove_chat_reference, send_blocked_toast, transcript_menu,
};
use crate::composer::references::ComposerEdit;
use crate::state::{ChatContext, ChatState};

/// Answers one pure helper, serialized as JSON text.
///
/// `None` means the arguments were not the shape the helper takes, which is a refusal rather than
/// an empty answer.
pub fn answer_query(
    state: &ChatState,
    context: &ChatContext,
    query: Query,
    arguments: &[Value],
) -> Option<String> {
    let first = arguments.first();
    Some(match query {
        Query::ComposerReferences => to_json(&composer_references(first.and_then(Value::as_str)?)),
        Query::ComposerKeyIntent => {
            let event: ComposerKeyEvent = serde_json::from_value(first?.clone()).ok()?;
            let platform = arguments.get(1).and_then(Value::as_str);
            to_json(&composer_key_intent(&event, platform))
        }
        Query::ReferenceMenu => to_json(&reference_menu(first.and_then(Value::as_str)?)),
        Query::TranscriptMenu => {
            let request = first?;
            let href = request.get("href").and_then(Value::as_str);
            let selection = request
                .get("selection")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let question_active = request.get("questionActive") == Some(&Value::Bool(true));
            to_json(&transcript_menu(href, selection, question_active))
        }
        Query::SendBlockedToast => {
            // The renderer passes the reason it already has; when it passes none, the helper asks
            // the composer for the current one.
            let reason = match first.and_then(Value::as_str) {
                Some(reason) => reason.to_string(),
                None => crate::composer::document::send_blocked(state, context)?,
            };
            to_json(&send_blocked_toast(&reason))
        }
        Query::InsertAnswerAttachments => to_json(&answer_attachments_edit(arguments)?),
        Query::RemoveChatReference => to_json(&chat_reference_removal(arguments)?),
    })
}

/// `insertAnswerAttachments(current, paths, original, start, end)`, offsets in UTF-16 code units.
pub fn answer_attachments_edit(arguments: &[Value]) -> Option<ComposerEdit> {
    let paths: Vec<&str> = arguments
        .get(1)?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    Some(insert_answer_attachments(
        arguments.first()?.as_str()?,
        &paths,
        arguments.get(2)?.as_str()?,
        usize_argument(arguments, 3)?,
        usize_argument(arguments, 4)?,
    ))
}

/// `removeChatReference(current, start, end)`, offsets in UTF-16 code units.
pub fn chat_reference_removal(arguments: &[Value]) -> Option<ComposerEdit> {
    Some(remove_chat_reference(
        arguments.first()?.as_str()?,
        usize_argument(arguments, 1)?,
        usize_argument(arguments, 2)?,
    ))
}

fn usize_argument(arguments: &[Value], index: usize) -> Option<usize> {
    arguments.get(index)?.as_u64().map(|value| value as usize)
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// The pure helpers. They read state and return an answer; they change nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Query {
    /// The reference pills a draft's text carries.
    ComposerReferences,
    /// What a keystroke in the composer means.
    ComposerKeyIntent,
    /// The context menu for a reference pill.
    ReferenceMenu,
    /// The context menu for a transcript selection.
    TranscriptMenu,
    /// The toast text for a send the composer refused.
    SendBlockedToast,
    /// A question answer with the references for the files a paste uploaded.
    InsertAnswerAttachments,
    /// A draft without the reference an attachment pill stands for.
    RemoveChatReference,
}

impl Query {
    /// The wire spelling, which is the method name a host passes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ComposerReferences => "composerReferences",
            Self::ComposerKeyIntent => "composerKeyIntent",
            Self::ReferenceMenu => "referenceMenu",
            Self::TranscriptMenu => "transcriptMenu",
            Self::SendBlockedToast => "sendBlockedToast",
            Self::InsertAnswerAttachments => "insertAnswerAttachments",
            Self::RemoveChatReference => "removeChatReference",
        }
    }

    /// Maps a method name to a helper, or `None` when it is not one of them.
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "composerReferences" => Self::ComposerReferences,
            "composerKeyIntent" => Self::ComposerKeyIntent,
            "referenceMenu" => Self::ReferenceMenu,
            "transcriptMenu" => Self::TranscriptMenu,
            "sendBlockedToast" => Self::SendBlockedToast,
            "insertAnswerAttachments" => Self::InsertAnswerAttachments,
            "removeChatReference" => Self::RemoveChatReference,
            _ => return None,
        })
    }
}
