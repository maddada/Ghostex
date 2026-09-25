//! Split Right: a focus that carries where the pane goes.
//!
//! CDXC:Workarea 2026-09-21 WHY:
//! `CDXC:Workarea 2026-09-04 DECISION` says what this is: Advanced > Split Right opens the session
//! in a new pane to the right of the focused agents pane, and it is a sidebar focus with a
//! placement rather than an action of its own. So the plan here is which of two things the row
//! gets, and the placement rides on both: a sleeping row is woken with the placement, exactly like
//! a click, and an awake row is simply selected with it.
//!
//! **"What does the user see if the call fails after the pane has moved?" has the same answer fork
//! gave: the case does not exist.** For an awake row there is no call at all, only a local
//! selection. For a sleeping row the pane is placed inside the wake's accepted leg, after the
//! daemon has answered and after the declined check, so there is no window where a pane has moved
//! and the call then fails, nothing to reverse and no optimistic row to take back. The one thing
//! that can happen is the opposite: the wake lands, the user has meanwhile selected another
//! session, and `focusMovedElsewhereDuringWake` skips the focus, so the session is awake and no
//! pane opens. That is the shipped behaviour and it is preserved rather than fixed, because taking
//! the pane anyway would move the user away from the row they chose in the meantime.
//!
//! **The attention acknowledgement is not planned here.** It is a subsystem, with a minimum
//! visible window and its own timers keyed per session (the old runtime's until 2026-09-25, now
//! packages/gx-core/src/attention.rs); M3 built the queue a local selection feeds, so a split
//! queues the acknowledgement the same way a click does rather than growing a second copy.
//!
//! Ported from `splitSessionRight` in the deleted `gxserver-runtime/sessions-and-focus.ts` (see
//! git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_reload.rs, apps/desktop/src/app/gx_store/burst.rs
//! (`gx_store_queue_attention_acknowledge`).

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::SessionKey;
use crate::selectors::QUICK_AUTOMATIONS_PROJECT_ID;

use super::lifecycle::shown_lifecycle;
use super::resolve::text_field;

use ghostex_gx_protocol::LifecycleState;

/// What a Split Right does to the row it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SplitAction {
    /// The Quick Automations row, where the TypeScript returns before it does anything. Owned
    /// rather than refused so the gate compares "nothing happens" instead of skipping it.
    Nothing,
    /// A sleeping row: the wake message the single-session path answers, carrying the placement.
    Wake(Value),
    /// An awake row: select it with the placement, which is all the TypeScript does.
    Focus,
}

/// A Split Right, resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPlan {
    pub session: SessionKey,
    /// Queued for the old runtime, which owns the attention timers.
    pub acknowledge_attention: bool,
    pub action: SplitAction,
}

impl SplitPlan {
    pub fn to_json(&self) -> Value {
        let (action, wake) = match &self.action {
            SplitAction::Nothing => ("nothing", Value::Null),
            SplitAction::Wake(message) => ("wake", message.clone()),
            SplitAction::Focus => ("focus", Value::Null),
        };
        json!({
            "session": self.session.to_sidebar_session_id(),
            "acknowledgeAttention": self.acknowledge_attention,
            "action": action,
            "wake": wake,
        })
    }
}

pub fn owns_split_message(message: &Value) -> bool {
    text_field(message, "type") == Some("splitSessionRight")
}

/// The Split Right plan, or `None` when this file does not own the payload.
///
/// Refused, with the reason at each refusal:
///
/// - **A remote row**, which is not a wake and a selection at all but one open through the native
///   project-path action bridge. Since 2026-09-21 `remote_focus.rs` owns that open, including this
///   payload's placement, so a remote Split Right is planned there rather than here; the gate
///   asserts the TypeScript still acts on one, which is what makes the hand-off a hand-off.
/// - **A browser row and an id that does not parse**, which are the TypeScript's own early return.
///
/// A row the store holds no lifecycle for is NOT refused: see the note at the read below.
pub fn plan_split_right(core: &Core, message: &Value) -> Option<SplitPlan> {
    if !owns_split_message(message) {
        return None;
    }
    let sidebar_session_id = text_field(message, "sessionId")?;
    if sidebar_session_id.starts_with("gpui-browser:") {
        return None;
    }
    let session = SessionKey::parse_sidebar_session_id(sidebar_session_id)?;
    if !session.machine.is_local() {
        return None;
    }
    // The Quick Automations row returns BEFORE the acknowledgement, not after it: the guard it
    // fails is the same `if` that rejects an unparseable id, and `acknowledgeSessionAttention` is
    // the line under it. Getting that order backwards acknowledges the attention of a row the
    // action then refuses to touch, which takes the "waiting on you" mark off a session nobody
    // opened. The first cut had it backwards and the gate reported it.
    if session.project_id == QUICK_AUTOMATIONS_PROJECT_ID {
        return Some(SplitPlan {
            session,
            acknowledge_attention: false,
            action: SplitAction::Nothing,
        });
    }
    // The row as the CLIENT shows it, overlay included, which is what `this.presentation` answers
    // after a `patchPresentationSession`. Asked through the one function that owns that read.
    //
    // A row the store holds NOTHING for is not refused and is not sleeping: `isSleepingLocal`
    // PresentationSession` answers false for a row it cannot find, so the TypeScript selects it
    // with the placement and lets the workspace decide. A port that refused here would silently do
    // nothing where the shipped code still asks, which is the rule piece 3c settled.
    let lifecycle = shown_lifecycle(
        core,
        &session.machine,
        &session.project_id,
        &session.session_id,
    )
    .unwrap_or(LifecycleState::Running);
    let action = match lifecycle == LifecycleState::Sleeping {
        true => SplitAction::Wake(json!({
            "type": "setSessionSleeping",
            "sessionId": sidebar_session_id,
            "sleeping": false,
            "placement": "splitRight",
        })),
        false => SplitAction::Focus,
    };
    Some(SplitPlan {
        session,
        acknowledge_attention: true,
        action,
    })
}
