//! Session Chat send path, split by concern. Every submodule is glob re-exported here,
//! so `crate::session_chat_send::*` paths are unchanged.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, oneshot};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::session_chat::{SessionChatQuestion, SessionChatQuestionSelection};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_options::schedule_session_chat_option_redetect;
use crate::session_chat_question_row_align::QuestionRowAction;
use crate::session_chat_queue_runtime::SessionChatMessageSource;
use crate::storage::open_gxserver_database;
use axum::http::StatusCode;
use serde_json::{json, Map, Value};

/*
CDXC:SessionChat 2026-07-31:
Session Chat send path (upstream chat spec §7/§8 port). The agent is a TUI, so sending is
writing bytes to its pty via `zmx send` stdin. Delivery uses an agent-specific,
screen-verified composer clear, a
bracketed-paste body with ESC sanitized and newlines normalized to CR, and a
SEPARATE Enter write (a trailing \r inside the paste burst is read as newline
text and the message stays staged). What is NOT preserved is the blind 500ms
settle that used to precede that Enter: the Enter now waits until a screen
capture proves the composer actually took the body, and is never written when
the body is provably absent (CDXC:Clipboard below). A per-session
queue serializes sequences: each send owns the input line from its clear
until its Enter fires. HTTP handlers enqueue and return immediately.
*/

#[path = "../session_chat_grok_draft.rs"]
mod grok_draft;

#[path = "../session_chat_input_replace.rs"]
mod input_replace;
pub use input_replace::clear_session_chat_composer;
pub(crate) use input_replace::handle_replace_session_chat_draft_http;

#[path = "../session_chat_composer_repaint.rs"]
mod composer_repaint;
pub(crate) use composer_repaint::claude_composer_needs_redraw;

mod answer_http;
mod ask_answer_keys;
mod busy_input_wait;
mod claude_queue_pull;
mod constants;
mod draft_capture;
mod grok_queue_pull;
mod guarded_write;
mod handoff_http;
mod input_bytes;
mod launch_command;
mod paste_verification;
mod queue;
mod queued_prompt_escape;
mod send_http;
mod send_reopen;
mod single_paste;
mod steps;
#[cfg(test)]
pub(crate) mod test_terminal;
#[cfg(test)]
mod tests;
mod worker;

pub(crate) use answer_http::*;
pub use ask_answer_keys::*;
pub use busy_input_wait::*;
pub(crate) use claude_queue_pull::*;
pub use constants::*;
pub use draft_capture::*;
pub(crate) use grok_queue_pull::*;
pub use guarded_write::InputGuard;
pub(crate) use guarded_write::*;
pub(crate) use handoff_http::*;
pub use input_bytes::*;
pub(crate) use paste_verification::*;
pub use queue::*;
pub use queued_prompt_escape::*;
pub(crate) use send_http::*;
pub(crate) use send_reopen::*;
pub(crate) use single_paste::*;
pub use steps::*;
use worker::*;
