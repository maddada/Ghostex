//! Session chat frames, messages, and the `readSessionChat` result.
//!
//! The most error-prone part of the chat contract is what an absent key means, and it differs per
//! key and per frame type. [`ChatSideState`] documents each one. These types only carry the wire
//! facts (absent, `null`, value); folding them into a view is the store's job.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tri::Tri;

/// Routing and ordering keys shared by all four chat frame types.
///
/// `epoch` and `seq` live in memory per follower and restart from 1 after a daemon restart or a
/// follower respawn, so compare `server_id` before comparing epochs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFrameBase {
    pub project_id: String,
    pub session_id: String,
    /// Follower generation, starts at 1.
    pub epoch: i64,
    /// Frame counter inside the epoch; the first frame is 1.
    pub seq: i64,
    pub protocol_version: u64,
    pub server_id: String,
}

open_string_enum! {
    ChatRole {
        User => "user",
        Assistant => "assistant",
        Reasoning => "reasoning",
        Tool => "tool",
        System => "system",
    }
}

open_string_enum! {
    ChatSource {
        Transcript => "transcript",
        Hook => "hook",
        Client => "client",
    }
}

open_string_enum! {
    ChatStatus {
        Loading => "loading",
        Ready => "ready",
        Working => "working",
        Empty => "empty",
        Starting => "starting",
        Error => "error",
        Unsupported => "unsupported",
    }
}

open_string_enum! {
    TurnLifecycleState {
        Working => "working",
        Completed => "completed",
        Interrupted => "interrupted",
    }
}

/// One content block of a message, tagged by `type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool-call")]
    ToolCall {
        name: String,
        /// Any JSON; bounded to 64 KiB of characters by the server.
        #[serde(default)]
        input: Value,
        /// The agent's id for this call, which its result names (Claude, Codex); absent for agents
        /// that record none.
        #[serde(rename = "callId", default, skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
    },
    #[serde(rename = "tool-result")]
    ToolResult {
        output: String,
        #[serde(rename = "isError", default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        /// The id of the call this answers, when the agent records one.
        #[serde(rename = "callId", default, skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
    },
    #[serde(rename = "image-ref")]
    ImageRef {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
    },
    /// A block type this client does not know. Its content is dropped.
    #[serde(other, rename = "unknown")]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncQuestion {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

/// Stands in for the collapsed rows of an older completed turn. Snapshot and replaced frames
/// reduce each such turn to its user prompt row plus this key; expanding it is a `readSessionChat`
/// call with `historyMode: "detail"` and this `before_offset`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredWork {
    pub before_offset: u64,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub start_id: String,
    #[serde(default)]
    pub end_id: Option<String>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub message_count: u64,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub file_paths: Vec<String>,
}

/// One transcript row: the server's `SessionChatMessage` plus the client-only `deferredWork`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub blocks: Vec<ChatBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_questions: Option<Vec<AsyncQuestion>>,
    /// Epoch ms. Always on the wire, `null` when unknown. Order rows by `(timestamp, byte_offset)`,
    /// never by id.
    #[serde(default)]
    pub timestamp: Option<i64>,
    pub source: ChatSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Line offset in the transcript file; the tie-breaker for equal timestamps.
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub byte_offset: Option<u64>,
    /// A prompt sitting in the agent's own queue, not handed to the model yet. Omitted when false.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "is_false"
    )]
    pub queued: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_work: Option<DeferredWork>,
    /// A client's presentation of an accepted send still waiting for the terminal.
    ///
    /// Never on the wire: only a client-sourced row carries it, and the renderer draws the row's
    /// "Queued" or "Failed" label from it. Not the agent CLI's own queue, which is `queued`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_delivery: Option<StartupDelivery>,
}

/// Where an accepted send stands with the terminal, as the sending client sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDelivery {
    pub prompt_id: String,
    /// `queued`, `sending` or `failed`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnLifecycle {
    pub state: TurnLifecycleState,
    pub turn_id: String,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

/// State keys shared by snapshot, replaced, and state frames, and by the `readSessionChat` result.
///
/// What an absent key means:
///
/// | Key | Absent means |
/// | --- | --- |
/// | `prompt`, `terminalNotice`, `terminalActivity`, `agentFleet`, `agentTasks` | CLEARED |
/// | `selectedOptions`, `draft`, `appCommands`, `returnedPrompt` | UNCHANGED |
/// | `queue` | the daemon has no queue support (present even when empty otherwise) |
/// | `screenProbed` | not probed yet (sticky once true) |
/// | `agentSessionId` | snapshot or replaced: cleared; state: unchanged |
/// | `accountSwitch`, `pendingModelSelection`, `asyncQuestionsSince` | a daemon without queue support; otherwise always written, `null` meaning none |
/// | `retiredAsyncQuestionIds` | a daemon without queue support; otherwise always written |
///
/// Payload shapes are kept loose (`Value`) until the native chat needs each one typed; every one
/// has a TypeScript interface in `packages/shared/session-chat.ts`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSideState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_options: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_notice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_activity: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_fleet: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_tasks: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_probed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_commands: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returned_prompt: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub account_switch: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub pending_model_selection: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub async_questions_since: Tri<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_async_question_ids: Option<Vec<String>>,
}

/// `sessionChatSnapshot` and `sessionChatReplaced`: an authoritative tail window.
///
/// A snapshot is the first frame of a follower generation and the answer to every subscribe by
/// any client, so expect unsolicited snapshots with a new epoch at any time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshotFrame {
    #[serde(flatten)]
    pub base: ChatFrameBase,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub messages: Vec<ChatMessage>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub has_more: bool,
    /// `true` marks `has_more` as exact; older daemons omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more_exact: Option<bool>,
    /// Byte offset into the transcript file: the cursor for the next older page.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub before_offset: u64,
    pub status: ChatStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The three read-only draft-agent fields an ordinary daemon frame does NOT own.
    ///
    /// A host that synthesizes snapshots from reads (the mobile SSH host, and the retained-session
    /// store's own `snapshotEvent`) sets them as own properties deliberately, including a cleared
    /// one on promotion. Absent means the frame does not own them and the folded value stands;
    /// present means it does, and an explicit `null` clears the field. `controller.ts` tests
    /// exactly that with `'sessionAgentId' in event`; JSON has no `undefined`, so a host that
    /// crosses a JSON boundary spells "owned and cleared" as `null`. Every gxserver socket frame
    /// omits all three.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub session_agent_id: Tri<String>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub available_agents: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub switchable_agents: Tri<Value>,
    /// Absent on a snapshot or replaced frame means cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<TurnLifecycle>,
    #[serde(flatten)]
    pub state: ChatSideState,
}

/// `sessionChatAppended`: new transcript rows only. Never carries queue, draft, prompt, or
/// screen state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAppendedFrame {
    #[serde(flatten)]
    pub base: ChatFrameBase,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub messages: Vec<ChatMessage>,
    /// Absent means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<TurnLifecycle>,
    /// Omitted on the wire when empty.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub superseded_message_ids: Vec<String>,
}

/// `sessionChatState`: a state change without new rows.
///
/// `status` is not taken verbatim by a client: a state frame must not downgrade a loaded
/// transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStateFrame {
    #[serde(flatten)]
    pub base: ChatFrameBase,
    pub status: ChatStatus,
    /// Absent means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working: Option<bool>,
    /// Absent means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<TurnLifecycle>,
    #[serde(flatten)]
    pub state: ChatSideState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatForkInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub ancestor_ids: Vec<String>,
}

/// Result of `POST /api/readSessionChat`: a snapshot without the routing keys.
///
/// `(epoch, seq)` is `(0, 0)` when no follower exists, and can be older than the live stream: a
/// read must never roll a newer live tail back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadSessionChatResult {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub messages: Vec<ChatMessage>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more_exact: Option<bool>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub before_offset: u64,
    #[serde(default)]
    pub epoch: i64,
    #[serde(default)]
    pub seq: i64,
    pub status: ChatStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_agents: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switchable_agents: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_info: Option<ChatForkInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<TurnLifecycle>,
    #[serde(flatten)]
    pub state: ChatSideState,
}

fn is_false(value: &bool) -> bool {
    !*value
}
