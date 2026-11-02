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
///
/// Shared with the projection cache's model (`state::ProjectedMessage`): a row changes its own
/// copy through `Arc::make_mut`, and it serializes as the bare object.
pub type ProjectedMessage = std::sync::Arc<Value>;

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
        /// The running turn's "Active work" fold starts open (`summary_mode_turns` in
        /// `transcript/turns.rs`). Its key is the turn's own, so a reader who closes it keeps it
        /// closed for the rest of that turn without touching the reply fold that follows.
        #[serde(default)]
        work_open: bool,
        /// The running turn's rows behind its "Active work" fold. The agent's newest row carries
        /// `toolsOpen: true`: its tool calls start open, so the running one shows. A call waiting
        /// on an approval carries `awaitingApproval: true` and starts open itself
        /// (`transcript/approval_wait.rs`), in every transcript mode.
        work: Vec<ProjectedMessage>,
        /// What ended a turn the agent wrote no reply to: a slash command's output ("Set effort
        /// level to High") or the "Agent was interrupted" marker. Drawn under the prompt without a fold.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        outcome: Vec<ProjectedMessage>,
        /// The newest turn that has a reply: its "Agent reply" fold starts open.
        #[serde(default)]
        latest_reply: bool,
        /// The agent's messages in the turn that hold a page card, a question form or a chart,
        /// drawn in full under the prompt; the folds leave them out (`content_rows` in
        /// `transcript/turns.rs`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cards: Vec<ProjectedMessage>,
        /// The pictures the running turn's work opened from disk, drawn as a strip under its
        /// "Active work" fold while the fold is closed (`transcript/read_images.rs`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        read_images: Vec<Value>,
    },
    /// A finished turn in verbose mode: its file changes collapse into one fold.
    #[serde(rename = "completed-work", rename_all = "camelCase")]
    CompletedWork {
        id: String,
        /// "Worked for 2m 5s".
        label: String,
        /// The counts beside the label, `{kind, label}` each (`transcript/work_summary.rs`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stats: Vec<Value>,
        /// The file change card's header over `files` (`file_change_card` in
        /// `transcript/file_change_rows.rs`).
        #[serde(default)]
        files_card: Value,
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
        /// Cards for the messages the turn's work sent other agents, hoisted out of the fold.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sent_messages: Vec<Value>,
        /// The Workflow call ids of the folded rows, whose cards (`workflowCards`) are drawn
        /// outside the fold, hoisted like the sent messages.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        workflows: Vec<Value>,
        artifacts: Vec<ProjectedMessage>,
        /// Omitted, not null, when the turn has no reply.
        #[serde(rename = "final", default, skip_serializing_if = "Option::is_none")]
        final_message: Option<ProjectedMessage>,
        /// The pictures the turn's folded work opened from disk, its rows' and those of rows the
        /// server still holds back, drawn as a strip under the fold while it is closed
        /// (`transcript/read_images.rs`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        read_images: Vec<Value>,
    },
    /// A kind this build does not know, kept verbatim so one new row cannot drop the list.
    #[serde(untagged)]
    Unknown(Value),
}

// A host may hand the rows to another thread, so a row stays `Send + Sync`, and cloning one shares
// its models.
const _: fn() = || {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<TranscriptItem>();
};
