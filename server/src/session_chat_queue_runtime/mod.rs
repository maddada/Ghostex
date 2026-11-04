//! Session Chat prompt-queue runtime, split by concern. Every submodule is glob re-exported
//! here, so `crate::session_chat_queue_runtime::*` paths are unchanged.

/*
CDXC:SessionChat 2026-08-21:
The scheduler half of the Ghostex chat prompt queue. `session_chat_queue.rs`
owns storage, the endpoints and the frame carriage; this module owns the clock
that decides WHEN a queued row is allowed to reach the agent.

The daemon owns this decision rather than any client, so a queue drains with
every client closed, the phone locked, or the desktop app quit.

Shape is deliberately `delayed_sends.rs`'s: a 1s tick, a non-working stability
window tracked per session, a guarded claim so two ticks cannot double-fire, and
restart recovery that never silently re-sends. The differences are the readiness
rule and the drain rate:

  - Ready = the session is idle, or sits in the attention a hook's Stop
    entered (`attentionSource: turnComplete`) with no question/approval card
    pending, AND the chat transcript lifecycle is not `Working`, held for
    SESSION_CHAT_QUEUE_STABILITY_MS.
  - Every other `attention` NEVER releases the queue. A prompt fired while the
    agent sits on a permission/approval prompt would be swallowed as the ANSWER
    to that prompt. Late delivery is harmless; early delivery corrupts a turn.
  - ONE prompt per idle window. Delivering the head makes the agent work again,
    so the clock restarts from zero after every attempt and row #2 waits for the
    next stop. This is never a "drain the whole queue" loop.
  - An input-blocking notice or unresolved quota, authentication, or agent error is not a delivery opportunity: the head
    row is marked `failed` with the notice title and the drain stops until the
    user retries or deletes it. The text is never lost. Queue eligibility uses
    `session_chat_notice/rules.rs`'s own predicate, NOT `severity == error`: a trust
    dialog or a first-run setup screen is only catalogued `Warning`/`Info` and
    would still eat a prompt as the ANSWER to itself.

Cost discipline: only sessions that actually hold a pending row are considered,
so a daemon with no queues anywhere does one indexed SQLite query per second and
nothing else. Transcripts are never walked for a session with an empty queue.
*/

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, schedule_presentation_session_delta,
    session_observer_key, AppState, RoutedResponse,
};
use crate::session_chat_composer::SessionChatComposerReadiness;
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_options::{
    cached_session_chat_composer_readiness, cached_session_chat_screen_state,
    cached_session_chat_terminal_notice, emit_session_chat_options_state_frame,
    schedule_session_chat_option_redetect, session_chat_terminal_notice_publisher,
    session_chat_watchdog_state_reader, SessionChatOptionDetector,
};
use crate::session_chat_send::resolve_session_chat_send_target;
use crate::{
    domain::DomainRepository,
    paths::GxserverPaths,
    presentation::presentation_activity,
    session_chat::{
        read_session_chat_tail_page, resolve_session_chat_transcript_agent,
        resolve_session_chat_transcript_path, SessionChatTailPage, SessionChatTurnLifecycleState,
    },
    session_chat_notice::SessionChatTerminalNotice,
    session_chat_queue::{
        deliver_session_chat_queued_prompt, fail_session_chat_queued_prompt,
        list_sessions_with_pending_queue, read_session_chat_queue_snapshot_with,
        SessionChatQueuePublisherFactory, SessionChatQueueSenderFactory,
    },
    session_status::is_turn_complete_attention,
    storage::open_gxserver_database,
};
use axum::http::StatusCode;
use serde_json::{json, Map};

mod box_first_send;
mod cloud_send;
mod endpoints;
mod held_redraw;
mod mid_turn;
mod post_send;
mod repaint_nudge;
mod scheduler;
mod send;
mod send_heal;
mod wait_reason;
mod wiring;

pub(crate) use endpoints::*;
pub(crate) use held_redraw::repair_unreadable_frame;
pub(crate) use mid_turn::*;
pub(crate) use post_send::*;
pub use scheduler::*;
pub(crate) use send::*;
pub(crate) use send_heal::*;
pub(crate) use wiring::*;
