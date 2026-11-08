//! Session Chat send-delivery watchdog, split by concern. Every submodule is glob re-exported
//! here, so `crate::session_chat_watchdog::*` paths are unchanged.

/*
CDXC:AgentScreenDetection 2026-08-19:
Chat sends are fire-and-forget: `write_session_chat_payload` succeeds the moment
zmx accepts the bytes, so a message typed into a dead login screen, a trust
dialog or a shell where the agent already exited disappears without a trace.

This module is the other half of `session_chat_notice/`: instead of reading
what the screen SAYS, it checks what the agent RECORDED. Every text send samples
the session transcript's byte length first, and after the send completes a
watchdog task watches the bytes appended past that offset for the message it
just sent. Delivery proven ⇒ silence. Nothing after 10s ⇒ one terminal capture,
classified, and published as a `terminalNotice` so the user sees the login
screen or the crashed CLI that swallowed the message.

Hard boundaries, because this runs behind every single chat send:
  - it NEVER retries a send, never writes to the terminal, and never delays or
    blocks the send path (it is spawned after the send has already resolved);
  - it spawns at most ONE `zmx history` capture, and only at the timeout;
  - a new send supersedes the previous watchdog for that session, so a fast
    typist has one watchdog, not five.

Delivery is checked in two tiers because neither alone is complete:
  a. decoded User-role messages from the shared incremental reader — the normal
     case for both agents;
  b. a JSON-escaped substring scan of the appended raw bytes — catches Claude's
     `queue-operation` enqueue rows (typed while a turn runs) and Codex's
     `response_item` message lane, neither of which the decoders surface.

CDXC:AgentScreenDetection 2026-08-24: a third tier answers the opposite
question — not "can delivery be proven?" but "was something ELSE submitted in
its place?" (`observe_mismatched_input`). It is the only tier with affirmative
evidence, so it does not wait out the deadline and it is not silenced by the
"the agent was already working" suppression, which is precisely what used to
swallow an empty submit.
*/

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use serde_json::{Map, Value};

use crate::session_chat::{
    is_noise_message, parse_json_object, read_incremental_transcript_messages,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    session_chat_line_decoder, text_block, SessionChatBlock, SessionChatIncrementalState,
    SessionChatLineDecoder, SessionChatMessage, SessionChatRole, SessionChatSource,
    SessionChatTranscriptAgent,
};
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, clear_session_chat_watchdog_notice,
    session_chat_delivery_mismatch_notice, session_chat_notice_key,
    session_chat_screen_shows_queued_input, session_chat_terminal_screen_tail,
    session_chat_watchdog_notice, set_session_chat_watchdog_notice, SessionChatTerminalNotice,
    SessionChatTerminalNoticeAction, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource, SESSION_CHAT_NOTICE_DELIVERY_FAILED,
    SESSION_CHAT_NOTICE_QUEUED_INPUT,
};
use crate::session_chat_options::session_chat_option_agent;

mod claude_liveness;
mod delivery_match;
mod probe;
mod task;

use claude_liveness::*;
use delivery_match::*;
pub(crate) use delivery_match::{note_ghostex_typed, transcript_line_records_prompt};
pub use probe::*;
use task::*;
