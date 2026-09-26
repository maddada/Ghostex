//! The transcript list the renderer walks: one entry per drawn card group.
//!
//! The producer was `projectChatTranscript` plus `NativeChatPresentation.update`
//! (`packages/shared/session-chat-controller/native-presentation.ts`). The consumer dispatches on
//! `kind` alone (`apps/desktop/src/app/native_chat/transcript.rs`), so the discriminant spelling
//! is part of the contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A projected message: the message plus everything the renderer needs to draw it without
/// re-parsing markdown.
///
/// Free-form for now. It is the single largest shape in the document (markdown marks, tool rows,
/// file cards, question exchanges, images, system cards, reasoning, agent messages) and it belongs
/// to family b, which types it as it ports the builders.
pub type ProjectedMessage = Value;

/// One row group in the transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TranscriptItem {
    /// A single message card.
    #[serde(rename = "message")]
    Message { message: ProjectedMessage },
    /// One user turn in summary mode: the prompt, the reply, and the work between them.
    #[serde(rename = "summary", rename_all = "camelCase")]
    Summary {
        id: String,
        user: ProjectedMessage,
        /// The turn's reply, or `null` while the turn is still running.
        #[serde(rename = "final")]
        final_message: Option<ProjectedMessage>,
        /// The replies before `final` in the same turn, oldest first: one per stretch a harness
        /// row (a background task finishing) broke the turn into. Drawn above `final`.
        #[serde(default)]
        earlier_replies: Vec<ProjectedMessage>,
        active: bool,
        work: Vec<ProjectedMessage>,
        /// The newest turn that has a reply: its "Agent reply" fold starts open.
        #[serde(default)]
        latest_reply: bool,
    },
    /// A finished turn in verbose mode: its file changes collapse into one fold.
    #[serde(rename = "completed-work", rename_all = "camelCase")]
    CompletedWork {
        id: String,
        /// "Worked for Xs".
        label: String,
        files: Vec<Value>,
        files_label: String,
        simple_files_label: String,
        expandable: bool,
        /// The turn's off-screen work, when the host has to read it back on demand.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deferred: Option<Value>,
        work: Vec<ProjectedMessage>,
        /// Answered question cards, hoisted out of the fold.
        questions: Vec<Value>,
        artifacts: Vec<ProjectedMessage>,
        /// Omitted, not null, when the turn has no reply.
        #[serde(rename = "final", default, skip_serializing_if = "Option::is_none")]
        final_message: Option<ProjectedMessage>,
    },
    /// A kind this build does not know, kept verbatim so one new row cannot drop the list.
    #[serde(untagged)]
    Unknown(Value),
}
