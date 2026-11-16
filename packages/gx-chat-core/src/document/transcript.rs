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
        /// The closed line above the newest reply, or above a running turn's "Active work", that
        /// holds `earlierReplies`: "2 earlier replies · 1 report" (`turns::earlier_label`).
        /// Empty when there is none.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        earlier_label: String,
        /// The quiet lines the open "Active work" draws over `work`, by row index
        /// (`quiet_view::summary_work_lines`): a row inside a line shows only while it is open.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        work_lines: Vec<crate::transcript::quiet_view::QuietStretch>,
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
        /// "GBot added" cards the folded rows' commands printed, hoisted out of the fold
        /// (`transcript/gbot_cards.rs`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        gbot_cards: Vec<Value>,
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
        /// The turn's messages between agents and answers fold into it (`transcript/folded_cards.rs`):
        /// the `artifacts` marked `foldedIntoWork`, then `questions`, then `sentMessages` are drawn
        /// at the top of the opened row instead of after it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cards_in_fold: bool,
        /// What a Reduced line adds up when it folds this turn in (`transcript/grouped_work.rs`).
        /// Never on the wire; boxed so the row stays no bigger than the others need.
        #[serde(skip)]
        tally: Box<WorkTally>,
    },
    /// Agent work set to Reduced (or Quiet): the finished turns the agent ran for one message of yours, folded
    /// into one line (`transcript/grouped_work.rs`). The fields are those of `completed-work`
    /// added up over every merged turn; what each turn keeps out of the fold is in `turns`.
    #[serde(rename = "grouped-work", rename_all = "camelCase")]
    GroupedWork {
        /// The first merged turn's id, as its `completed-work` row had it.
        id: String,
        /// "Worked for 1m 16s": the merged turns' times added up.
        label: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stats: Vec<Value>,
        #[serde(default)]
        files_card: Value,
        files: Vec<Value>,
        files_label: String,
        simple_files_label: String,
        expandable: bool,
        /// Every merged turn's folded rows, in turn order.
        work: Vec<ProjectedMessage>,
        /// Each merged turn's tail, in turn order, drawn after the line.
        turns: Vec<GroupedTurn>,
        /// The pictures the merged work opened from disk, each once, drawn as a strip under the
        /// line while it is closed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        read_images: Vec<Value>,
        /// What the reply's work button says under Agent work Quiet, which draws no line:
        /// "24s · 5 tools · 2 files" (`grouped_work::work_button`).
        #[serde(default)]
        work_button: String,
        /// Agent work Quiet: the id of the reply whose hover row carries the work button, the
        /// row's last reply with actions; absent when none has them (the button then stands on
        /// its own row after the tails) and under Reduced.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_carrier: Option<String>,
        /// As on `completed-work`, for every merged turn: each turn's `artifacts` marked
        /// `foldedIntoWork`, its `questions` and its `sentMessages` are drawn at the top of the
        /// opened line instead of in its tail (`transcript/folded_cards.rs`).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cards_in_fold: bool,
        /// The merged turns' numbers added up, as on `completed-work`. Never on the wire.
        #[serde(skip)]
        tally: Box<WorkTally>,
    },
    /// A kind this build does not know, kept verbatim so one new row cannot drop the list.
    #[serde(untagged)]
    Unknown(Value),
}

/// One turn a `grouped-work` line merged: what stays out of the line, shaped like the same-named
/// `completed-work` fields, and the turn's own id and `deferred`, which opening the line reads back
/// (`{type: 'loadWork', id, work: deferred}`, one per turn that has it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupedTurn {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred: Option<Value>,
    pub questions: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sent_messages: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gbot_cards: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<Value>,
    pub artifacts: Vec<ProjectedMessage>,
    #[serde(rename = "final", default, skip_serializing_if = "Option::is_none")]
    pub final_message: Option<ProjectedMessage>,
    /// Where the turn's rows start in the line's `work`, which a Quiet View line marks
    /// (`quiet_view::QuietTurnMark`). Never on the wire.
    #[serde(skip)]
    pub work_start: usize,
    /// The turn's own "Worked for 22s", as its `completed-work` row had it. Never on the wire.
    #[serde(skip)]
    pub label: String,
}

/// A finished turn's numbers, kept beside its `completed-work` row so a Reduced line can add them
/// up without reading its labels back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkTally {
    /// When the turn's opener arrived, which starts a line the turn opens.
    pub started_ms: Option<i64>,
    /// When the turn ended (`presentation::turn_end`), which ends a line the turn opens.
    pub ended_ms: Option<i64>,
    /// How far the turn carries a line it does not open: an OMP wake-up's reply only, the way a
    /// prompt's own label runs on through them (`presentation::turn_worked_label`), else its end.
    pub extends_to_ms: Option<i64>,
    /// Its tool calls, as `work_summary::work_stats` counts them.
    pub tool_calls: usize,
    /// How many of `tool_calls` sent another agent a message, which a Quiet View line counts as
    /// messages sent instead (`work_summary::sent_call_count`).
    pub sent_calls: usize,
    /// The paths its "N files changed" counts, the held-back ones included.
    pub changed: Vec<String>,
    /// The messages between agents and the answers folded into it (`transcript/folded_cards.rs`).
    pub messages: usize,
    pub answers: usize,
    /// The turns' sections still on the server, by their prompt's id (`startId`), each with the
    /// tool calls gxserver counted in it (`deferredWork.toolCallCount`): a Quiet View line counts
    /// those instead of the turns' own counts, which cannot see the held-back rows
    /// (`transcript/quiet_view.rs`).
    pub held: Vec<(String, usize)>,
}

// A host may hand the rows to another thread, so a row stays `Send + Sync`, and cloning one shares
// its models.
const _: fn() = || {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<TranscriptItem>();
};
