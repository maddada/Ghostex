//! A session on a Cloud Box that is paused or stopped.
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1013): "On sending a message there. Not on opening the session." A host refuses every
//! call to a sleeping box with the code [`CLOUD_BOX_ASLEEP_CODE`] instead of waking it, and wakes
//! it only for a call that delivers a message. So a refused transcript read is not a failed chat:
//! the chat is ready with what it already holds (its cached transcript, or none), says
//! [`ASLEEP_PLACEHOLDER`] in the composer, and reads again every [`ASLEEP_READ_RETRY_MS`] (each
//! answered at once by the host, without the network) until the box is back. A submission goes
//! on past a refused draft push, so its send reaches the host and wakes the box.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/cloud_boxes.rs and
//! apps/gpui-web/src/app/gx_store/cloud_boxes.rs (`cloud_box_send_target`, the refusal and the
//! wake), apps/mobile/app/src/chat/ (the phone's host speaks the same code).

use ghostex_gx_protocol::ChatStatus;

use crate::session::constants::{TIMER_RESYNC_RETRY, TIMER_SEED_RETRY};
use crate::state::{ChatContext, ChatState, OutstandingRead, ReadKind};

/// The error code a host refuses a call to a paused or stopped box with.
pub const CLOUD_BOX_ASLEEP_CODE: &str = "cloudBoxAsleep";

/// What the composer says while the box sleeps.
pub const ASLEEP_PLACEHOLDER: &str = "Stopped \u{b7} sending wakes it";

/// How often a sleeping box's transcript is asked for again.
pub const ASLEEP_READ_RETRY_MS: u64 = 3_000;

/// What an asleep chat with no cached transcript says in place of the new-session welcome.
pub fn asleep_empty_state() -> crate::document::EmptyState {
    crate::document::EmptyState {
        title: "This cloud box is stopped".to_string(),
        detail: "Its conversation shows once the box is back. Sending a message wakes it."
            .to_string(),
        action: None,
    }
}

/// Whether a refusal says the session's box is asleep.
pub fn is_asleep(code: Option<&str>) -> bool {
    code == Some(CLOUD_BOX_ASLEEP_CODE)
}

/// A transcript read refused because the box sleeps: the chat settles as ready with what it holds
/// and the lane reads again later. Never an error, never the loading hold.
pub fn read_refused_asleep(state: &mut ChatState, context: &ChatContext, read: &OutstandingRead) {
    if read.generation != state.messages.generation {
        return;
    }
    if !state.session.cloud_box_asleep {
        state.session.cloud_box_asleep = true;
        state.core.request_render();
    }
    state.session.error = None;
    if matches!(
        state.session.server_status,
        ChatStatus::Loading | ChatStatus::Starting | ChatStatus::Error
    ) {
        state.session.server_status = ChatStatus::Ready;
    }
    let delay = ASLEEP_READ_RETRY_MS as f64;
    match read.kind {
        ReadKind::Seed => {
            if !state.messages.position.frame_arrived {
                state
                    .core
                    .timers
                    .arm(TIMER_SEED_RETRY, context.now_ms, delay);
            }
        }
        ReadKind::Resync => {
            state.messages.resync.in_flight = false;
            state
                .core
                .timers
                .arm(TIMER_RESYNC_RETRY, context.now_ms, delay);
        }
        ReadKind::Page => {
            if state
                .messages
                .load_earlier_request
                .as_ref()
                .is_some_and(|request| request.before_offset == read.before_offset.unwrap_or(0))
            {
                state.messages.load_earlier_request = None;
                state.messages.loading_earlier = false;
            }
        }
    }
    state.core.request_publish();
}

/// A read answered: the box is up again.
pub fn note_awake(state: &mut ChatState) {
    if state.session.cloud_box_asleep {
        state.session.cloud_box_asleep = false;
        state.core.request_render();
    }
}
