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
/// `epoch` and `seq` live in memory per follower. Each follower generation takes a new epoch that is
/// higher than every epoch gxserver handed out before it, a restarted gxserver included (its
/// `server_id` stays the same), and `seq` starts again inside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFrameBase {
    pub project_id: String,
    pub session_id: String,
    /// Follower generation: rises with every new generation and across gxserver restarts.
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
        /// The file edit this result made, as the agent recorded it; absent for every other tool.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        patch: Option<ToolPatch>,
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

/// What a file edit changed, as the agent wrote it in its transcript (Claude's `structuredPatch`).
///
/// The tool call only holds the text it replaced, so this is the one place the file's own line
/// numbers and surrounding lines come from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPatch {
    /// The edit created the file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub created: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub hunks: Vec<PatchHunk>,
}

/// One unified-diff hunk: its first line in the old and new file and its lines, each prefixed
/// with `' '`, `'-'` or `'+'`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchHunk {
    #[serde(default)]
    pub old_start: u32,
    #[serde(default)]
    pub new_start: u32,
    /// The nearest line above the hunk that starts a declaration, the way `git diff` heads a hunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub lines: Vec<String>,
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
    /// The agent's own record that its API call failed, with the code it recorded
    /// (`server_error`, `rate_limit`, …). Omitted on every other row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_error: Option<String>,
    /// On a user row that has `@` mentions: the ones gxserver found on disk, in the spelling
    /// `bare_file_paths` gives them; only these become file pills
    /// (server/src/session_chat_mention_paths.rs). Absent on every other row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention_paths: Option<Vec<String>>,
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
    /// Why gxserver still holds it, in the user's words; set once the row has waited about a
    /// minute (server/src/session_chat_queue_runtime/wait_reason.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_reason: Option<String>,
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
/// | `selectedOptions`, `draft`, `appCommands`, `returnedPrompt`, `coordinatorThreads`, `agentMessageDeliveries` | UNCHANGED |
/// | `queue` | the daemon has no queue support (present even when empty otherwise) |
/// | `screenProbed` | not probed yet (sticky once true) |
/// | `agentSessionId` | snapshot or replaced: cleared; state: unchanged |
/// | `accountSwitch`, `apiRetry`, `pendingModelSelection`, `asyncQuestionsSince`, `forkedAt` | a daemon without queue support; otherwise always written, `null` meaning none |
/// | `retiredAsyncQuestionIds`, `laterQuestionIds`, `removedQuestionIds` | a daemon without queue support; otherwise always written |
/// | `resourceIndex` | UNCHANGED (a daemon before 2026-10-30 never writes it) |
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
    /// Claude's Workflow runs (`{ runs: [...] }`, `server/src/session_chat_claude_workflow_cards.rs`),
    /// one per Workflow call, for the chat's workflow cards. Cleared on omission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflows: Option<Value>,
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
    /// A coordinator's threads (`{ threads: [...], doneCount }`), carried only for a coordinator
    /// session; the Threads panel above its composer. Absent means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_threads: Option<Value>,
    /// How the messages this session's agent sent with `ghostex agents send` and left waiting
    /// ended, by `sendRequestId` (`{ "<id>": { state: "delivered" | "failed", failure? } }`;
    /// server/src/agent_message_deliveries.rs). Absent means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_message_deliveries: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub account_switch: Tri<Value>,
    /// Ghostex's automatic retry after the agent's API call failed on a connection or server
    /// error (server/src/accounts/api_errors.rs), or `null` when none is running.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub api_retry: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub pending_model_selection: Tri<Value>,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub async_questions_since: Tri<i64>,
    /// When this session was forked from another, in epoch ms, or `null` when it is no fork: the
    /// questions its transcript carries from before then were asked in the parent.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub forked_at: Tri<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_async_question_ids: Option<Vec<String>>,
    /// The question forms and pages the user put off "For later" (form or page ids,
    /// server/src/session_chat_open_questions.rs): still answerable, but not counted as waiting.
    /// Absent means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_question_ids: Option<Vec<String>>,
    /// The question forms and pages the user removed without answering (form or page ids,
    /// server/src/session_chat_open_questions.rs): no longer counted as waiting. Absent means
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_question_ids: Option<Vec<String>>,
    /// The session's resource index (`ResourceEntry` records, gx-chat-core
    /// `questions/resource_index.rs`; server/src/session_chat_resources.rs): every question form,
    /// page and chart the whole conversation held, so the Resources list and count cover more than
    /// the loaded rows. A coordinator's also lists the pages its threads posted. Absent means
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_index: Option<Vec<Value>>,
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
    /// Whether the session runs on its own login and offers no account; reads alone carry it,
    /// and `None` (an older daemon, or a frame) leaves what the core already knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses_own_login: Option<bool>,
    /// Whether the Catch me up setting is on; reads alone carry it, and `None` (an older daemon,
    /// or a frame) leaves what the core already knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catch_up: Option<bool>,
    /// Whether the Session details panel built-in extension is on, the same way as `catch_up`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_details: Option<bool>,
    /// Whether the Send when you pick an answer setting is on, the same way as `catch_up`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_on_pick: Option<bool>,
    /// Whether the Show all effort levels setting is on, the same way as `catch_up`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_orchestrator_efforts: Option<bool>,
    /// The Handover heading color strength setting (0 to 100), the same way as `catch_up`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marked_heading_strength: Option<f64>,
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
