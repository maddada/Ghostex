//! What the chat shows before the transcript catches up: optimistic send echoes, slash-command
//! markers, and the lines read straight off the agent's terminal.
//!
//! Owned by family a. It is the half of the composed list that has no authoritative record yet,
//! and every entry here is retired by the transcript row that replaces it.

use ghostex_gx_protocol::{ChatMessage, StartupDelivery};

/// The optimistic half of the transcript.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PendingState {
    /// Sends accepted locally but not yet visible in the transcript. Bounded by
    /// `SESSION_CHAT_PENDING_SEND_LIMIT`.
    pub sends: Vec<PendingSend>,
    /// Slash-command and keystroke markers ("Ran /compact", "Interrupted the agent"). Bounded by
    /// `SESSION_CHAT_COMMAND_MARKER_LIMIT`.
    pub markers: Vec<CommandMarker>,
    /// Each distinct transient line the agent painted over its own status row, kept until a
    /// transcript row carries the same text.
    pub terminal_status_messages: Vec<ChatMessage>,
    /// The message the agent is painting right now, shown as the streaming bubble.
    pub terminal_stream: Option<TerminalStream>,
    /// The tool row read off the screen, and the hold that bridges the terminal's repaint gaps.
    pub terminal_tool: Option<ChatMessage>,
    /// When the hold expires, or `None` when no hold is armed.
    pub terminal_tool_hold_until_ms: Option<f64>,
    /// The counter behind a pending send's id. Monotonic, never reused, so the ids are
    /// deterministic and the core needs no random source.
    pub send_counter: u64,
}

/// One accepted send with no transcript row yet.
///
/// The fields exist to match an echo with the authoritative turn that replaces it, which is what
/// makes the swap invisible.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PendingSend {
    pub id: String,
    /// The queue row this send became, once the receipt names one.
    pub queued_prompt_id: Option<String>,
    /// The queue row's delivery state, for a send reconstructed at startup from the queue.
    pub startup_delivery: Option<StartupDelivery>,
    pub text: String,
    pub image_paths: Vec<String>,
    pub sent_at_ms: i64,
    /// The last authoritative message id when the send was issued; `None` means none.
    pub after_message_id: Option<String>,
    pub after_message_timestamp: Option<i64>,
    /// One-based among identical sends sharing a boundary.
    pub matching_occurrence: Option<u32>,
    pub matching_after_timestamp: Option<i64>,
    /// The agent was already mid-response, so the prompt sits in the agent's own queue rather than
    /// starting a turn. The echo carries `queued` then, matching the server's own queued row.
    pub sent_while_working: bool,
}

/// A command Ghostex typed into the agent, shown as its own row until the transcript covers it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandMarker {
    pub id: String,
    pub command: String,
    pub sent_at_ms: i64,
    /// Row text override. A keystroke dispatch is not a slash command, so "Ran /x" would read
    /// wrong.
    pub label: Option<String>,
    /// Compaction records the transcript already held when the marker was created. A `/compact`
    /// marker retires once that COUNT grows, deliberately not on a timestamp: a remote session's
    /// transcript stamps come off the remote host's clock, and any skew would either retire the
    /// marker on sight or strand it forever.
    pub compaction_records_before: Option<usize>,
    /// Server identities already visible when the send began, independent of clock skew.
    pub local_command_ids_before: Vec<String>,
}

/// The line the agent is painting right now.
///
/// CDXC:AgentScreenDetection 2026-09-11 DECISION:
/// User: while the agent streams a long reply, show the text as it comes in from the terminal
/// (chunks every second are enough) and switch to the transcript's message the moment it is saved
/// to the JSONL.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalStream {
    /// `detectedAt` of the run: stable for one message across probes.
    pub id: String,
    /// Epoch ms of `id`.
    pub started_at_ms: i64,
    /// The message as painted so far.
    pub text: String,
    /// Normalized first paragraph, the transcript match key.
    pub key: String,
    /// Still the newest thing on screen. Once false, the stream is held for the transcript to
    /// retire rather than dropped outright.
    pub live: bool,
}
