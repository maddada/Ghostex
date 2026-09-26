//! The two decisions a workspace tab's own Close, Sleep and Wake need from the store, now that the
//! host performs them instead of the old runtime (`terminal-lifecycle-queue.ts`).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/lifecycle_requests.rs.

use serde_json::Value;

use crate::core::Core;
use crate::keys::SessionKey;

use super::lifecycle::replacement_focus_for_transition;

/// The row that takes the focus when a tab's Close or Sleep did not name its own replacement:
/// the next running row of the project, in the list's order, and only when the closing row is
/// the focused one (`resolveLocalProjectListTransitionFocusTarget`).
pub fn terminal_lifecycle_fallback_focus(core: &Core, session: &SessionKey) -> Option<SessionKey> {
    let focused = core.focus().focused_session.clone();
    replacement_focus_for_transition(core, session, focused.as_ref())
}

/// Whether a `/api/transitionSession` answer lets the tab show the transition.
///
/// CDXC:Workarea 2026-06-26 WHY:
/// macOS close and sleep intentionally diverge after gxserver handles a provider transition. Close removes the local pane and sidebar row once `/api/transitionSession` returns a valid close result, even when the provider kill did not commit; sleep stays strict (the session's lifecycle matches the action, the provider is `missing`, and the kill did not explicitly fail) so GPUI does not show a cold sleeping placeholder while the zmx runtime is still live.
pub fn provider_transition_committed(result: &Value, action: &str) -> bool {
    let Some(session) = result.get("session").filter(|session| session.is_object()) else {
        return false;
    };
    if result.get("action").and_then(Value::as_str) != Some(action) {
        return false;
    }
    if action == "close" {
        return true;
    }
    let Some(provider) = session
        .get("providerState")
        .filter(|state| state.is_object())
    else {
        return false;
    };
    let expected = if action == "sleep" {
        "sleeping"
    } else {
        "stopped"
    };
    let kill_failed = result
        .get("transition")
        .filter(|transition| transition.is_object())
        .and_then(|transition| transition.get("kill"))
        .filter(|kill| kill.is_object())
        .and_then(|kill| kill.get("killed"))
        .and_then(Value::as_bool)
        == Some(false);
    session.get("lifecycleState").and_then(Value::as_str) == Some(expected)
        && provider.get("lifecycleState").and_then(Value::as_str) == Some("missing")
        && !kill_failed
}
