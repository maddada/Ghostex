//! Close: the one action whose optimistic update takes a row AWAY.
//!
//! CDXC:Sessions 2026-09-20 DECISION:
//! User (through the port's coordinator): a row the user cannot get back by retrying is the
//! failure the user would report as data loss even when nothing was lost, so the two choices must
//! be made deliberately rather than inherited. This port RESTORES the row when the daemon does not
//! confirm the close, which the TypeScript does not.
//!
//! What the TypeScript does: it removes the row, calls `/api/transitionSession`, and swallows
//! every failure with `.catch(() => undefined)`. The removal is also written into
//! `localFirstHiddenPresentationSessionKeys`, which nothing clears for the life of the page, and
//! its `rpc` is a bare `fetch` with no timeout and no abort. So a daemon that accepts the
//! connection and never answers leaves the row gone until the app is restarted, while the session
//! is still alive and still listed by the daemon. Retrying cannot help, because there is no row
//! left to retry on.
//!
//! Why the restore is the right difference rather than parity: the session is not gone, only the
//! client forgot it, so showing it again shows the truth. The cost is a row that flickers away and
//! comes back, which reads as "that did not work, try again" and is recoverable. The cost of
//! matching is a row that is silently missing, which is not. The boundary is exactly the one the
//! TypeScript's `.catch` draws, so nothing else moves: an answer that arrives keeps the removal,
//! and only what that `catch` would have swallowed (transport, HTTP status, a bad envelope, and
//! now also a timeout this client has and that one does not) brings the row back.
//!
//! Ported from `transitionSession` (`gxserver-runtime/sessions-and-focus.ts`) and
//! `removePresentationSession` / `hideLocalPresentationSession`
//! (`gxserver-runtime/presentation-stream.ts`), deleted with QuickJS on 2026-09-25.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_lifecycle.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::SessionKey;
use crate::selectors::QUICK_AUTOMATIONS_PROJECT_ID;

use super::lifecycle::replacement_focus_for_transition;
use super::resolve::text_field;

/// What the host must call, and the row that takes the focus when the closing row holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    pub session: SessionKey,
    pub rpc_path: &'static str,
    pub rpc_params: Value,
    pub replacement_focus: Option<SessionKey>,
}

impl CloseRequest {
    pub fn to_json(&self) -> Value {
        json!({
            "rpc": { "path": self.rpc_path, "params": self.rpc_params },
            "replacementFocus": self
                .replacement_focus
                .as_ref()
                .map(SessionKey::to_sidebar_session_id),
        })
    }
}

/// What the daemon said about a close.
///
/// `NeverAnswered` is its own case and not a kind of failure, because it is the one the two
/// clients answer differently: this one is bounded by its own timeout and takes the removal back,
/// the TypeScript's `fetch` has no timeout at all and the row stays gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAnswer {
    /// A success envelope came back. Whatever it says about the provider, the daemon handled the
    /// request, and its own removal delta follows.
    Accepted,
    /// Transport, HTTP status, or an envelope that is not a success: everything the TypeScript's
    /// `.catch(() => undefined)` swallows.
    Failed,
    /// The call never returned. In this client that is the timeout firing.
    NeverAnswered,
}

impl CloseAnswer {
    pub fn read(result: Result<&Value, &str>) -> Self {
        match result {
            Ok(_) => Self::Accepted,
            Err(_) => Self::Failed,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::NeverAnswered => "neverAnswered",
        }
    }
}

/// What the host does, in the order it does it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseFollowUp {
    /// Take the row off the list now (`Intent::HideSession`).
    Hide {
        session: SessionKey,
    },
    /// Put it back (`Intent::UnhideSession`): the daemon never confirmed the close.
    Unhide {
        session: SessionKey,
    },
    Focus {
        session: SessionKey,
    },
}

impl CloseFollowUp {
    pub fn to_json(&self) -> Value {
        let (follow, session) = match self {
            Self::Hide { session } => ("hide", session),
            Self::Unhide { session } => ("unhide", session),
            Self::Focus { session } => ("focus", session),
        };
        json!({ "follow": follow, "session": session.to_sidebar_session_id() })
    }
}

/// The `closeSession` payload, or `None` when this file does not own it.
///
/// Refused, each one a subsystem rather than a branch: a browser row closes an app tab through the
/// browser bridge; a REMOTE row is `remote.rs`'s, where a close is `/api/killSession` down that
/// machine's tunnel with no hide and no replacement focus; the Quick Automations row is not a
/// close at all but a teardown of four pieces of the old runtime's own state (its overview flag,
/// its visible set, its focus and its active project), none of which the store holds; and
/// `closeSessions` is the bulk payload.
pub fn plan_close_request(core: &Core, message: &Value) -> Option<CloseRequest> {
    if text_field(message, "type")? != "closeSession" {
        return None;
    }
    let sidebar_session_id = text_field(message, "sessionId")?;
    if sidebar_session_id.starts_with("gpui-browser:") {
        return None;
    }
    let session = SessionKey::parse_sidebar_session_id(sidebar_session_id)?;
    if !session.machine.is_local() || session.project_id == QUICK_AUTOMATIONS_PROJECT_ID {
        return None;
    }
    let focused = core.focus().focused_session.clone();
    Some(CloseRequest {
        rpc_path: "/api/transitionSession",
        rpc_params: json!({
            "action": "close",
            "projectId": session.project_id,
            "reason": "gpui-sidebar",
            "sessionId": session.session_id,
        }),
        // Resolved before the row goes, exactly as the TypeScript resolves it before
        // `removePresentationSession`: afterwards the row is no longer in the list it counts from.
        replacement_focus: replacement_focus_for_transition(core, &session, focused.as_ref()),
        session,
    })
}

/// What happens the moment the user clicks, before anything is called.
pub fn close_optimistic_follow_ups(request: &CloseRequest) -> Vec<CloseFollowUp> {
    let mut follow_ups = vec![CloseFollowUp::Hide {
        session: request.session.clone(),
    }];
    if let Some(replacement) = &request.replacement_focus {
        follow_ups.push(CloseFollowUp::Focus {
            session: replacement.clone(),
        });
    }
    follow_ups
}

/// What happens when the answer arrives, or does not. An empty list is the ordinary case: the
/// daemon took the close and its own removal delta retires the hide.
pub fn apply_close_answer(request: &CloseRequest, answer: CloseAnswer) -> Vec<CloseFollowUp> {
    match answer {
        CloseAnswer::Accepted => Vec::new(),
        CloseAnswer::Failed | CloseAnswer::NeverAnswered => vec![CloseFollowUp::Unhide {
            session: request.session.clone(),
        }],
    }
}

/// Whether this payload is one this file answers.
pub fn owns_close_message(message: &Value) -> bool {
    text_field(message, "type") == Some("closeSession")
}
