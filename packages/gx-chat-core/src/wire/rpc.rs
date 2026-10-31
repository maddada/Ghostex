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
    /// One form, page or chart of the session's resource index, read back by its offsets
    /// (`server/src/session_chat_resources.rs`).
    ReadSessionChatResource,
    /// A published page's picture for its chat card (`server/src/visual_pages/thumbnail.rs`).
    ReadVisualPageThumbnail,
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
    AcknowledgeSessionChatDraftHandoff,
    ReadSessionTerminalTail,
    /// Copy Report's text about this session's terminal (`/api/readSessionChatReport`).
    ReadSessionChatReport,
    SessionForkBranches,
    SwitchDraftAgent,
    /// Where a draft will run (`/api/draftRunLocation`), read and changed by the Run on row.
    DraftRunLocation,
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
            Self::ReadSessionChatResource => "readSessionChatResource",
            Self::ReadVisualPageThumbnail => "readVisualPageThumbnail",
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
            Self::AcknowledgeSessionChatDraftHandoff => "acknowledgeSessionChatDraftHandoff",
            Self::ReadSessionTerminalTail => "readSessionTerminalTail",
            Self::ReadSessionChatReport => "readSessionChatReport",
            Self::SessionForkBranches => "sessionForkBranches",
            Self::SwitchDraftAgent => "switchDraftAgent",
            Self::DraftRunLocation => "draftRunLocation",
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
            Self::TakeBackSessionChatQueuedMessage => "takeBackSessionChatQueuedMessage",
            Self::CatchUpSession => "catchUpSession",
            Self::ReadSessionFilesLinks => "readSessionFilesLinks",
            Self::ReadSessionDetails => "readSessionDetails",
            Self::StopSessionBackgroundTask => "stopSessionBackgroundTask",
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
            "readSessionChatResource" => Self::ReadSessionChatResource,
            "readVisualPageThumbnail" => Self::ReadVisualPageThumbnail,
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
            "acknowledgeSessionChatDraftHandoff" => Self::AcknowledgeSessionChatDraftHandoff,
            "readSessionTerminalTail" => Self::ReadSessionTerminalTail,
            "readSessionChatReport" => Self::ReadSessionChatReport,
            "sessionForkBranches" => Self::SessionForkBranches,
            "switchDraftAgent" => Self::SwitchDraftAgent,
            "draftRunLocation" => Self::DraftRunLocation,
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
            "takeBackSessionChatQueuedMessage" => Self::TakeBackSessionChatQueuedMessage,
            "catchUpSession" => Self::CatchUpSession,
            "readSessionFilesLinks" => Self::ReadSessionFilesLinks,
            "readSessionDetails" => Self::ReadSessionDetails,
            "stopSessionBackgroundTask" => Self::StopSessionBackgroundTask,
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
