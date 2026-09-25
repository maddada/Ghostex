//! Full Reload: the one action whose whole content is two other actions, in order.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! `fullReloadSession` sleeps the session and wakes it again, and that is all it is. Nothing here
//! calls the daemon: the plan is the two `setSessionSleeping` messages the single-session path
//! already owns, in the order the TypeScript awaits them, and the host runs them ONE AT A TIME
//! through that path. A port that made its own two calls would have written a second copy of the
//! declined leg, the replacement focus and the echo guard for a payload that adds neither.
//!
//! **The order is the behaviour, not an implementation detail.** The TypeScript awaits the sleep
//! before it starts the wake, so the daemon has really killed the provider before it is asked to
//! respawn it; issuing both at once would race the wake against a session that is still running and
//! leave the user with a reload that reloaded nothing. The host therefore waits for each leg to
//! come home, which is a different wait from the bulk sleep's 350 ms timer and is why it is a
//! sequence here rather than a paced fan-out. It also READS what came home: a sleep whose call
//! failed ends the reload (`reload_continues_after`). The first host awaited the sleep and sent
//! the wake whatever it said, which woke a session whose sleep never reached the daemon and
//! remounted its terminal for nothing.
//!
//! **`forceRemount` rides on the second leg** and is the reason the wake is not an ordinary one.
//! `/api/sleepSession` zmx-kills the daemon and the CLI inside it, so the local terminal the
//! workspace still holds is dead; without the remount the wake focus can re-select that dead
//! terminal, which is `CDXC:CefRuntime 2026-07-12`. It reaches the workspace through the request's
//! `focus_options`, so the single-session path applies it on exactly the leg that asked for it.
//!
//! Ported from `fullReloadSession` in the deleted `gxserver-runtime/sessions-and-focus.ts`.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/lifecycle.rs,
//! apps/desktop/src/app/gx_store/sidebar_reload.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::SessionKey;

use super::lifecycle::{plan_lifecycle_request, LifecycleAnswer};
use super::resolve::text_field;

/// The two legs of a Full Reload, in the order they go out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadPlan {
    pub session: SessionKey,
    /// `setSessionSleeping` messages the single-session path answers, first the sleep and then the
    /// wake that carries the remount.
    pub legs: Vec<Value>,
}

impl ReloadPlan {
    pub fn to_json(&self) -> Value {
        json!({
            "session": self.session.to_sidebar_session_id(),
            "legs": self.legs,
        })
    }
}

/// The payloads this file answers. `restartSession` is the same arm in `handleSidebarMessage`.
pub const RELOAD_MESSAGE_TYPES: [&str; 2] = ["fullReloadSession", "restartSession"];

pub fn owns_reload_message(message: &Value) -> bool {
    text_field(message, "type").is_some_and(|kind| RELOAD_MESSAGE_TYPES.contains(&kind))
}

/// The Full Reload plan, or `None` when this file does not own the payload.
///
/// Refused, with the reason at each refusal:
///
/// - **A remote row**, which `remote.rs` answers with the same two legs
///   (`reload_leg_messages`), each one a call down that machine's tunnel.
/// - **A browser row and an id that does not parse**, which are the TypeScript's own early return
///   (`!remoteSession && (!reference || !this.client)`): it does nothing at all for either, and the
///   gate asserts that it makes no call, so the hand-off costs the user nothing.
/// - **A row whose first leg the single-session path will not answer.** The legs are not a second
///   list of rules: the plan asks `plan_lifecycle_request` itself, so a shape that path refuses
///   refuses the whole reload rather than half of it. The Quick Automations row is the one that
///   matters, and it is OWNED, because both of its legs return before the call on both sides and
///   the gate can compare that nothing happens.
pub fn plan_full_reload(core: &Core, message: &Value) -> Option<ReloadPlan> {
    if !owns_reload_message(message) {
        return None;
    }
    let [sleep, wake] = reload_leg_messages(text_field(message, "sessionId")?);
    // Asked rather than re-derived. Every refusal of the single-session path is a refusal here, and
    // the one place that decides them stays one place.
    let session = plan_lifecycle_request(core, &sleep)?.session;
    Some(ReloadPlan {
        session,
        legs: vec![sleep, wake],
    })
}

/// The two `setSessionSleeping` messages a Full Reload is, in order, for either machine.
pub(super) fn reload_leg_messages(sidebar_session_id: &str) -> [Value; 2] {
    [
        json!({
            "type": "setSessionSleeping",
            "sessionId": sidebar_session_id,
            "sleeping": true,
        }),
        json!({
            "type": "setSessionSleeping",
            "sessionId": sidebar_session_id,
            "sleeping": false,
            "forceRemount": true,
        }),
    ]
}

/// Whether the wake may follow a sleep that came back with this answer.
///
/// `fullReloadSession` is two `await`s, so a sleep whose call REJECTS stops the reload before the
/// wake is asked for, while a sleep the daemon DECLINED returns normally and the wake still goes
/// out. The rule is here so the host and the gate ask the same question (see the module comment).
pub fn reload_continues_after(answer: LifecycleAnswer) -> bool {
    answer != LifecycleAnswer::Failed
}
