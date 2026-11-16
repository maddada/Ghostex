//! The gxserver requests the chat brain makes, and what comes back.
//!
//! Every one is a `POST /api/<method>` whose body is the params object and whose reply is the
//! envelope `packages/gx-protocol/src/rpc.rs` describes. The core never performs the call: it asks
//! for it with [`crate::Effect::SendRpc`] and the host answers with
//! [`crate::Event::RpcSettled`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use ghostex_gx_protocol::RpcErrorCode;

/// A gxserver method the chat brain calls.
///
/// Open: an unknown name is kept verbatim so a newer host can drive a method this build does not
/// know.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChatRpcMethod {
    /// The transcript read, also used for history pages and subagent transcripts.
    ReadSessionChat,
    ReadSessionChatSkills,
    ReadSessionChatFiles,
    ReadSessionChatImage,
    /// The whole text of a tool output gxserver cut (`transcript::output_cut`).
    ReadSessionChatToolOutput,
    /// One form, page or chart of the session's resource index, read back by its offsets
    /// (`server/src/session_chat_resources.rs`).
    ReadSessionChatResource,
    /// A published page's picture for its chat card (`server/src/visual_pages/thumbnail.rs`).
    ReadVisualPageThumbnail,
    /// A published page's state for the dock: how many of its questions the reader's draft
    /// answers and how many it asks (`server/src/visual_pages/source.rs`).
    ReadVisualPageSource,
    /// Answers brought back from an exported page (`questions/page_import.rs`).
    ImportVisualPageAnswers,
    /// A page as its exported file, for the phone's Export (`questions/page_export.rs`).
    ExportVisualPage,
    SendSessionChatMessage,
    InterruptSessionChat,
    AnswerSessionChatPrompt,
    RewindSessionChat,
    SelectSessionChatModel,
    QueueSessionChatPrompt,
    UpdateSessionChatQueuedPrompt,
    RemoveSessionChatQueuedPrompt,
    ReorderSessionChatQueue,
    SendSessionChatQueuedPrompt,
    SetSessionChatDraft,
    /// A chat's view pick, kept by the session's computer (`transcript/view_sync.rs`).
    SetSessionChatView,
    AcknowledgeSessionChatDraftHandoff,
    ReadSessionTerminalTail,
    /// Copy Report's text about this session's terminal (`/api/readSessionChatReport`).
    ReadSessionChatReport,
    SessionForkBranches,
    SwitchDraftAgent,
    /// Where a draft will run (`/api/draftRunLocation`), read and changed by the Run on row.
    DraftRunLocation,
    /// A draft's worktree tray and its move (`/api/draftWorktree`).
    DraftWorktree,
    AgentAccounts,
    ReadSessionAgentNote,
    SaveSessionAgentNote,
    ListStashedPrompts,
    SaveStashedPrompt,
    ImportNativeAttachments,
    /// The composer field's current text, which the HOST answers rather than gxserver.
    ReadNativeComposer,
    RunProjectDocsAction,
    /// Every thread of a coordinator, behind its Threads panel's "N more" row.
    ReadCoordinatorThreads,
    /// Resumes a coordinator's closed thread before one of its Threads panel rows opens it.
    OpenCoordinatorThread,
    /// Moves a row of a coordinator's board to Removed, or restores it
    /// (server/src/coordinators/board.rs).
    CoordinatorBoard,
    /// Autopilot's choices (`/api/autopilot`, server/src/server/autopilot_http.rs): the pill.
    Autopilot,
    /// Ghostex Collab's owner calls (`/api/collab`, forwarded by server/src/collab_shim/http.rs to the Collab component): the answer
    /// to a guest's message.
    Collab,
    /// A shared chat's chat box is being typed into (`/api/collabTyping`).
    CollabTyping,
    /// Forks the session into a side chat that starts with a question (server/src/side_chats/).
    StartSideChat,
    /// Sends what a side chat said to its main session.
    BringBackSideChat,
    /// Stops a side chat and removes it.
    DeleteSideChat,
    /// The user's answer to an agent's secret request (server/src/secret_requests/).
    AnswerSecretRequest,
    /// Takes a message the agent CLI queued back out of its queue, for the composer.
    TakeBackSessionChatQueuedMessage,
    /// A short summary of where the session stands (`composer/catch_up.rs`).
    CatchUpSession,
    /// Every attachment, file and link the session and its threads mentioned
    /// (`composer/files_links.rs`).
    ReadSessionFilesLinks,
    /// What the Session details panel shows (`extras/session_details/`).
    ReadSessionDetails,
    /// Ends one of the session's background commands (the Session details panel's stop button).
    StopSessionBackgroundTask,
    /// The Mode menu's "Restart in Bypass when idle" switch (server/src/session_chat_mode_restart.rs).
    RestartSessionInMode,
    /// Hold a message on THIS computer's gxserver for a session whose computer is offline
    /// (`session/computer.rs`). The host sends these three to its own gxserver, never the
    /// session's.
    HoldSessionChatSend,
    ListHeldSessionChatSends,
    CancelHeldSessionChatSend,
    /// A method this build does not know; kept verbatim.
    Other(String),
}

impl ChatRpcMethod {
    /// The wire spelling, which is also the `/api/` path segment.
    pub fn as_str(&self) -> &str {
        match self {
            Self::ReadSessionChat => "readSessionChat",
            Self::ReadSessionChatSkills => "readSessionChatSkills",
            Self::ReadSessionChatFiles => "readSessionChatFiles",
            Self::ReadSessionChatImage => "readSessionChatImage",
            Self::ReadSessionChatToolOutput => "readSessionChatToolOutput",
            Self::ReadSessionChatResource => "readSessionChatResource",
            Self::ReadVisualPageThumbnail => "readVisualPageThumbnail",
            Self::ReadVisualPageSource => "readVisualPageSource",
            Self::ImportVisualPageAnswers => "importVisualPageAnswers",
            Self::ExportVisualPage => "exportVisualPage",
            Self::SendSessionChatMessage => "sendSessionChatMessage",
            Self::InterruptSessionChat => "interruptSessionChat",
            Self::AnswerSessionChatPrompt => "answerSessionChatPrompt",
            Self::RewindSessionChat => "rewindSessionChat",
            Self::SelectSessionChatModel => "selectSessionChatModel",
            Self::QueueSessionChatPrompt => "queueSessionChatPrompt",
            Self::UpdateSessionChatQueuedPrompt => "updateSessionChatQueuedPrompt",
            Self::RemoveSessionChatQueuedPrompt => "removeSessionChatQueuedPrompt",
            Self::ReorderSessionChatQueue => "reorderSessionChatQueue",
            Self::SendSessionChatQueuedPrompt => "sendSessionChatQueuedPrompt",
            Self::SetSessionChatDraft => "setSessionChatDraft",
            Self::SetSessionChatView => "setSessionChatView",
            Self::AcknowledgeSessionChatDraftHandoff => "acknowledgeSessionChatDraftHandoff",
            Self::ReadSessionTerminalTail => "readSessionTerminalTail",
            Self::ReadSessionChatReport => "readSessionChatReport",
            Self::SessionForkBranches => "sessionForkBranches",
            Self::SwitchDraftAgent => "switchDraftAgent",
            Self::DraftRunLocation => "draftRunLocation",
            Self::DraftWorktree => "draftWorktree",
            Self::AgentAccounts => "agentAccounts",
            Self::ReadSessionAgentNote => "readSessionAgentNote",
            Self::SaveSessionAgentNote => "saveSessionAgentNote",
            Self::ListStashedPrompts => "listStashedPrompts",
            Self::SaveStashedPrompt => "saveStashedPrompt",
            Self::ImportNativeAttachments => "importNativeAttachments",
            Self::ReadNativeComposer => "readNativeComposer",
            Self::RunProjectDocsAction => "runProjectDocsAction",
            Self::ReadCoordinatorThreads => "readCoordinatorThreads",
            Self::OpenCoordinatorThread => "openCoordinatorThread",
            Self::CoordinatorBoard => "coordinatorBoard",
            Self::Autopilot => "autopilot",
            Self::Collab => "collab",
            Self::CollabTyping => "collabTyping",
            Self::StartSideChat => "startSideChat",
            Self::BringBackSideChat => "bringBackSideChat",
            Self::DeleteSideChat => "deleteSideChat",
            Self::AnswerSecretRequest => "answerSecretRequest",
            Self::TakeBackSessionChatQueuedMessage => "takeBackSessionChatQueuedMessage",
            Self::CatchUpSession => "catchUpSession",
            Self::ReadSessionFilesLinks => "readSessionFilesLinks",
            Self::ReadSessionDetails => "readSessionDetails",
            Self::StopSessionBackgroundTask => "stopSessionBackgroundTask",
            Self::RestartSessionInMode => "restartSessionInMode",
            Self::HoldSessionChatSend => "holdSessionChatSend",
            Self::ListHeldSessionChatSends => "listHeldSessionChatSends",
            Self::CancelHeldSessionChatSend => "cancelHeldSessionChatSend",
            Self::Other(name) => name.as_str(),
        }
    }

    /// Maps a wire spelling to a method; never fails.
    pub fn from_wire(name: &str) -> Self {
        match name {
            "readSessionChat" => Self::ReadSessionChat,
            "readSessionChatSkills" => Self::ReadSessionChatSkills,
            "readSessionChatFiles" => Self::ReadSessionChatFiles,
            "readSessionChatImage" => Self::ReadSessionChatImage,
            "readSessionChatToolOutput" => Self::ReadSessionChatToolOutput,
            "readSessionChatResource" => Self::ReadSessionChatResource,
            "readVisualPageThumbnail" => Self::ReadVisualPageThumbnail,
            "readVisualPageSource" => Self::ReadVisualPageSource,
            "importVisualPageAnswers" => Self::ImportVisualPageAnswers,
            "exportVisualPage" => Self::ExportVisualPage,
            "sendSessionChatMessage" => Self::SendSessionChatMessage,
            "interruptSessionChat" => Self::InterruptSessionChat,
            "answerSessionChatPrompt" => Self::AnswerSessionChatPrompt,
            "rewindSessionChat" => Self::RewindSessionChat,
            "selectSessionChatModel" => Self::SelectSessionChatModel,
            "queueSessionChatPrompt" => Self::QueueSessionChatPrompt,
            "updateSessionChatQueuedPrompt" => Self::UpdateSessionChatQueuedPrompt,
            "removeSessionChatQueuedPrompt" => Self::RemoveSessionChatQueuedPrompt,
            "reorderSessionChatQueue" => Self::ReorderSessionChatQueue,
            "sendSessionChatQueuedPrompt" => Self::SendSessionChatQueuedPrompt,
            "setSessionChatDraft" => Self::SetSessionChatDraft,
            "setSessionChatView" => Self::SetSessionChatView,
            "acknowledgeSessionChatDraftHandoff" => Self::AcknowledgeSessionChatDraftHandoff,
            "readSessionTerminalTail" => Self::ReadSessionTerminalTail,
            "readSessionChatReport" => Self::ReadSessionChatReport,
            "sessionForkBranches" => Self::SessionForkBranches,
            "switchDraftAgent" => Self::SwitchDraftAgent,
            "draftRunLocation" => Self::DraftRunLocation,
            "draftWorktree" => Self::DraftWorktree,
            "agentAccounts" => Self::AgentAccounts,
            "readSessionAgentNote" => Self::ReadSessionAgentNote,
            "saveSessionAgentNote" => Self::SaveSessionAgentNote,
            "listStashedPrompts" => Self::ListStashedPrompts,
            "saveStashedPrompt" => Self::SaveStashedPrompt,
            "importNativeAttachments" => Self::ImportNativeAttachments,
            "readNativeComposer" => Self::ReadNativeComposer,
            "runProjectDocsAction" => Self::RunProjectDocsAction,
            "readCoordinatorThreads" => Self::ReadCoordinatorThreads,
            "openCoordinatorThread" => Self::OpenCoordinatorThread,
            "coordinatorBoard" => Self::CoordinatorBoard,
            "autopilot" => Self::Autopilot,
            "collab" => Self::Collab,
            "collabTyping" => Self::CollabTyping,
            "startSideChat" => Self::StartSideChat,
            "bringBackSideChat" => Self::BringBackSideChat,
            "deleteSideChat" => Self::DeleteSideChat,
            "answerSecretRequest" => Self::AnswerSecretRequest,
            "takeBackSessionChatQueuedMessage" => Self::TakeBackSessionChatQueuedMessage,
            "catchUpSession" => Self::CatchUpSession,
            "readSessionFilesLinks" => Self::ReadSessionFilesLinks,
            "readSessionDetails" => Self::ReadSessionDetails,
            "stopSessionBackgroundTask" => Self::StopSessionBackgroundTask,
            "restartSessionInMode" => Self::RestartSessionInMode,
            "holdSessionChatSend" => Self::HoldSessionChatSend,
            "listHeldSessionChatSends" => Self::ListHeldSessionChatSends,
            "cancelHeldSessionChatSend" => Self::CancelHeldSessionChatSend,
            other => Self::Other(other.to_string()),
        }
    }
}

impl Serialize for ChatRpcMethod {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ChatRpcMethod {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_wire(&String::deserialize(deserializer)?))
    }
}

/// How a request the core asked for ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RpcOutcome {
    /// The result body, still free-form: the per-method result types are added by the family that
    /// ports the caller.
    Ok { result: Value },
    /// A refusal, with the code the composer keys its `composerNotReady` card off.
    Err {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        message: String,
        /// The `/api/` path the refusal came from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
    },
}
