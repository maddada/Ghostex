//! A row's Close After Done state, read from the session itself: gxserver owns the timer since
//! 2026-09-25 (server/src/close_after_done.rs) and publishes `closeAfterDone` and, while the
//! countdown runs, `closeAfterDoneDeadlineAt`. The labels count down from the deadline against
//! the host's clock, as before.

use ghostex_gx_protocol::PresentationSession;

use super::inputs::CloseAfterDoneInput;

impl CloseAfterDoneInput {
    /// `None` when the session is not armed.
    pub fn from_session(session: &PresentationSession) -> Option<Self> {
        session.close_after_done.then(|| Self {
            armed: true,
            deadline_at: session.close_after_done_deadline_at.clone(),
            remaining_label: None,
            remaining_ms: None,
        })
    }
}
