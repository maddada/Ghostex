//! Sleep and wake: the call, the facts read before it, and what the answer means.
//!
//! CDXC:SessionSleep 2026-09-20 WHY:
//! This is the first action whose port has to survive being WRONG for a moment. The TypeScript
//! does not guess: it calls the daemon, waits, asks the answer two questions
//! (`gxserverSleepWasDeclined`, and for `/api/transitionSession` `didGpuiGxserverProviderTransitionCommit`),
//! and only then shows the row as sleeping. An optimistic value applied before the call would
//! publish a state the daemon declined, which is what the KeepAwake fix of 2026-08-19 removed.
//! So the port is two halves with the round trip between them, and both halves are pure: the host
//! makes the call and hands the answer back.
//!
//! The half after the answer is where the echo guard lives, and the guard is already in the store:
//! [`crate::SessionPatch`] is an overlay with a recorded base, and `StoredPatch::verdict` answers
//! the three cases the daemon can produce. It is NOT the focus stamp M3 built, and the difference
//! is the point: focus has exactly one owner and a total order, so a stamp decides it; a session's
//! lifecycle has one owner (the daemon) and the client's value is a prediction of one transition.
//! A stamp cannot express "the daemon moved somewhere I did not predict", which is the case that
//! matters here, and the base-recorded overlay does: it stops overlaying the moment the row leaves
//! the value it was predicting FROM.
//!
//! Ported from `setSessionSleeping` (`gxserver-runtime/auto-sleep.ts`),
//! `resolveLocalProjectListTransitionFocusTarget` and `localProjectTransitionSessionIds`
//! (`gxserver-runtime/sessions-and-focus.ts`), and `gxserverSleepWasDeclined` and
//! `focusMovedElsewhereDuringWake` (`gxserver-runtime/helpers/auto-sleep.ts`), all deleted with
//! QuickJS on 2026-09-25 (see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_lifecycle.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::{MachineId, SessionKey};
use crate::overlay::SessionPatch;
use crate::selectors::QUICK_AUTOMATIONS_PROJECT_ID;

use super::resolve::text_field;

use ghostex_gx_protocol::{LifecycleState, SessionSurface};

/// How long an accepted transition may stay overlaid before the daemon's own row must have
/// arrived.
///
/// The TypeScript has no expiry at all: it writes the value into its copy of the daemon row, where
/// it lives until that row is sent again. This crate refuses that (M1: "an overlay that never ends
/// would hide the daemon's state for good"), so the port picks a bound instead. It is generous on
/// purpose. The overlay is only ever applied AFTER the daemon accepted the call, so the daemon has
/// already done the work and publishes the row in the same breath; a minute is three orders of
/// magnitude more than that round trip and still bounds a stream that never answers.
pub const LIFECYCLE_PATCH_TTL_MS: u64 = 60_000;

/// Which call a `setSessionSleeping` payload asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleCall {
    Sleep,
    Wake,
}

impl LifecycleCall {
    pub fn rpc_path(self) -> &'static str {
        match self {
            Self::Sleep => "/api/sleepSession",
            Self::Wake => "/api/wakeSession",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Wake => "wake",
        }
    }
}

/// Everything the host needs to make the call, plus the two facts that must be read BEFORE it
/// because the answer arrives in another frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LifecycleRequest {
    pub session: SessionKey,
    pub call: LifecycleCall,
    pub rpc_path: &'static str,
    pub rpc_params: Value,
    /// The running row that takes the focus when the row being slept holds it. Resolved before the
    /// call, exactly as `resolveLocalProjectListTransitionFocusTarget` is, because by the time the
    /// answer lands this row is already the one the list would skip.
    pub replacement_focus: Option<SessionKey>,
    /// Focus as it stood when the call left. A wake that lands after the user moved on must not
    /// take the focus back (`focusMovedElsewhereDuringWake`).
    pub focused_before: Option<SessionKey>,
    /// The `options` the TypeScript hands `focusLocalWorkspaceSession` for THIS row, which are the
    /// caller's and not the payload's: Full Reload's second leg asks for a remount, and Split Right
    /// asks for the new pane. They never reach the replacement focus, which the TypeScript selects
    /// with no options at all.
    pub focus_options: FocusOptions,
}

/// The two `focusLocalWorkspaceSession` options a sidebar action can ask for.
///
/// CDXC:Sessions 2026-09-21 WHY:
/// Both exist because the wake and the pane are one step for the user and two for the app. A Full
/// Reload really cycles the provider, so the local terminal the workspace still holds is dead by
/// the time the wake lands and `forceRemount` is what tears it down synchronously instead of
/// re-selecting it (`CDXC:CefRuntime 2026-07-12`). Split Right is a focus with a placement, so the
/// same wake has to carry where the pane goes or the session opens in the tab it already had.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FocusOptions {
    pub force_remount: bool,
    pub split_right: bool,
}

impl FocusOptions {
    pub fn to_json(self) -> Value {
        json!({ "forceRemount": self.force_remount, "splitRight": self.split_right })
    }

    /// What the message carries. `forceRemount` and `placement` are the two fields the store's own
    /// compositions set when they build a `setSessionSleeping` message for a row the single-session
    /// path then answers; a message from the renderer carries neither.
    pub fn from_message(message: &Value) -> Self {
        Self {
            force_remount: message
                .get("forceRemount")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            split_right: text_field(message, "placement") == Some("splitRight"),
        }
    }
}

impl LifecycleRequest {
    /// The request in the shape the parity gate compared while the TypeScript ran: the call and its payload, which is the
    /// half a list comparison cannot see.
    pub fn to_json(&self) -> Value {
        json!({
            "call": self.call.as_str(),
            "rpc": { "path": self.rpc_path, "params": self.rpc_params },
            "replacementFocus": self
                .replacement_focus
                .as_ref()
                .map(SessionKey::to_sidebar_session_id),
            "focusedBefore": self
                .focused_before
                .as_ref()
                .map(SessionKey::to_sidebar_session_id),
            "focusOptions": self.focus_options.to_json(),
        })
    }
}

/// What the daemon said, reduced to the one question the TypeScript asks of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleAnswer {
    /// The call went through.
    Accepted,
    /// `/api/sleepSession` answered with a `declined` field: the session is still running, and
    /// nothing local may say otherwise.
    Declined,
    /// Transport, envelope or timeout. The TypeScript lets the rejection propagate out of
    /// `setSessionSleeping`, so nothing local happens either.
    Failed,
}

impl LifecycleAnswer {
    /// `gxserverSleepWasDeclined`: the presence of the field, whatever it holds.
    pub fn read(result: Result<&Value, &str>) -> Self {
        match result {
            Err(_) => Self::Failed,
            Ok(value) => match value.get("declined").is_some() {
                true => Self::Declined,
                false => Self::Accepted,
            },
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Failed => "failed",
        }
    }
}

/// What the host does once the answer is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecycleFollowUp {
    /// Overlay the transition the daemon accepted, under the store's own echo guard.
    Patch {
        session: SessionKey,
        patch: SessionPatch,
    },
    /// Select a session in the workspace, exactly as `focusLocalWorkspaceSession` does.
    Focus {
        session: SessionKey,
        options: FocusOptions,
    },
}

impl LifecycleFollowUp {
    pub fn to_json(&self) -> Value {
        match self {
            Self::Patch { session, patch } => json!({
                "follow": "patch",
                "session": session.to_sidebar_session_id(),
                "lifecycleState": patch.lifecycle_state.as_ref().map(LifecycleState::as_str),
            }),
            Self::Focus { session, options } => json!({
                "follow": "focus",
                "session": session.to_sidebar_session_id(),
                "options": options.to_json(),
            }),
        }
    }
}

/// The `setSessionSleeping` payload, or `None` when this file does not own it.
///
/// Two shapes are deliberately not answered here: a browser row (`gpui-browser:`) is an app tab
/// and not a daemon session, and a REMOTE row is answered by `remote.rs`, whose leg is a call down
/// that machine's tunnel with none of the local overlay, focus or declined handling. The bulk
/// payloads (`setSessionsSleeping`, `setGroupSleeping`) are `bulk.rs`'s. The Quick Automations
/// project is owned here, because what it does is nothing and a gate can compare that.
pub fn plan_lifecycle_request(core: &Core, message: &Value) -> Option<LifecycleRequest> {
    let (call, sidebar_session_id) = lifecycle_message(message)?;
    if sidebar_session_id.starts_with("gpui-browser:") {
        return None;
    }
    let session = SessionKey::parse_sidebar_session_id(sidebar_session_id)?;
    if !session.machine.is_local() {
        return None;
    }
    let sleeping = call == LifecycleCall::Sleep;
    let focused_before = core.focus().focused_session.clone();
    // The Quick Automations row returns before the call on either side, so it is a request with
    // no RPC rather than a refusal: the gate then compares "nothing happens" instead of skipping.
    let quick = session.project_id == QUICK_AUTOMATIONS_PROJECT_ID;
    Some(LifecycleRequest {
        rpc_path: match quick {
            true => "",
            false => call.rpc_path(),
        },
        rpc_params: match quick {
            true => Value::Null,
            false => lifecycle_params(&session),
        },
        // Only a sleep moves the focus off the row, and only when that row holds it.
        replacement_focus: match quick || !sleeping {
            true => None,
            false => replacement_focus_for_transition(core, &session, focused_before.as_ref()),
        },
        focus_options: FocusOptions::from_message(message),
        focused_before,
        session,
        call,
    })
}

/// `setSessionSleeping`'s own fields: which call, and the sidebar session id it names. One parse for
/// both machines, so the local planner and the remote leg cannot read `sleeping` two ways.
pub(super) fn lifecycle_message(message: &Value) -> Option<(LifecycleCall, &str)> {
    if text_field(message, "type")? != "setSessionSleeping" {
        return None;
    }
    let call = match message.get("sleeping")?.as_bool()? {
        true => LifecycleCall::Sleep,
        false => LifecycleCall::Wake,
    };
    Some((call, text_field(message, "sessionId")?))
}

/// The body of `/api/sleepSession` and `/api/wakeSession`, which is the same on either machine: the
/// RAW ids that machine's daemon accepts and the reason. `sleepTrigger` is never set here, because
/// only the automatic sleep sets it and no sidebar payload is one.
pub(super) fn lifecycle_params(session: &SessionKey) -> Value {
    json!({
        "projectId": session.project_id,
        "reason": "gpui-sidebar",
        "sessionId": session.session_id,
    })
}

/// What to do with the answer. An empty list is a real answer: the daemon declined, or the call
/// failed, and nothing local may move.
pub fn apply_lifecycle_answer(
    request: &LifecycleRequest,
    answer: LifecycleAnswer,
    focused_now: Option<&SessionKey>,
    now_ms: u64,
) -> Vec<LifecycleFollowUp> {
    if request.rpc_path.is_empty() || answer != LifecycleAnswer::Accepted {
        return Vec::new();
    }
    let expires_at_ms = now_ms.saturating_add(LIFECYCLE_PATCH_TTL_MS);
    let mut follow_ups = vec![LifecycleFollowUp::Patch {
        session: request.session.clone(),
        patch: SessionPatch::lifecycle(
            match request.call {
                LifecycleCall::Sleep => LifecycleState::Sleeping,
                LifecycleCall::Wake => LifecycleState::Running,
            },
            expires_at_ms,
        ),
    }];
    match request.call {
        LifecycleCall::Sleep => {
            if let Some(replacement) = &request.replacement_focus {
                // The replacement is selected with NO options: it is not the row the caller asked
                // about, so a Full Reload's remount and a Split Right's pane belong to the row
                // being reloaded or split and never to whichever row inherits the focus.
                follow_ups.push(LifecycleFollowUp::Focus {
                    session: replacement.clone(),
                    options: FocusOptions::default(),
                });
            }
        }
        // `focusMovedElsewhereDuringWake`: the user picked another session while the daemon was
        // waking this one. It is running and will attach when it is selected again, but it does
        // not take the focus from what the user chose in the meantime.
        LifecycleCall::Wake => {
            let moved_elsewhere = focused_now != request.focused_before.as_ref()
                && focused_now != Some(&request.session);
            if !moved_elsewhere {
                follow_ups.push(LifecycleFollowUp::Focus {
                    session: request.session.clone(),
                    options: request.focus_options,
                });
            }
        }
    }
    follow_ups
}

/// `resolveLocalProjectListTransitionFocusTarget`: the next RUNNING row of the same project, in
/// the order the list draws it, wrapping past the end, and only when the row being slept or closed
/// is the focused one.
///
/// Shared with `close.rs` because the TypeScript shares it: `transitionSession` resolves it the
/// same way for both actions and from the same place, before the row leaves the list.
pub(super) fn replacement_focus_for_transition(
    core: &Core,
    session: &SessionKey,
    focused: Option<&SessionKey>,
) -> Option<SessionKey> {
    if focused != Some(session) {
        return None;
    }
    let ordered = transition_session_ids(core, session);
    let removed_index = ordered.iter().position(|id| *id == session.session_id);
    let rotated: Vec<&String> = match removed_index {
        Some(index) => ordered[index + 1..]
            .iter()
            .chain(&ordered[..index])
            .collect(),
        None => ordered.iter().collect(),
    };
    rotated
        .into_iter()
        .find(|candidate| **candidate != session.session_id && is_running(core, session, candidate))
        .map(|candidate| SessionKey {
            machine: session.machine.clone(),
            project_id: session.project_id.clone(),
            session_id: candidate.clone(),
        })
}

/// `localProjectTransitionSessionIds`: the rows the projection built for this project first (which
/// is each of its groups\' own `sessionIds`, locally hidden rows left out), then every row the
/// presentation holds for it, then the row being slept, each id once.
///
/// It is NOT the drawn list. `latestGroups` is the projection\'s inventory and its per-group
/// `sessions` are in the group\'s `sessionIds` order, before the sidebar sorts them into sections;
/// reading the sorted rows here would pick a different successor whenever the sort mode or a
/// section heading moved a row.
fn transition_session_ids(core: &Core, session: &SessionKey) -> Vec<String> {
    let mut ordered: Vec<String> = Vec::new();
    let Some(entry) = core.presentation().machine(&session.machine) else {
        ordered.push(session.session_id.clone());
        return ordered;
    };
    let push = |ordered: &mut Vec<String>, session_id: &str| {
        if !session_id.is_empty() && !ordered.iter().any(|held| held == session_id) {
            ordered.push(session_id.to_string());
        }
    };
    if let Some(loaded) = entry.loaded() {
        for group in loaded
            .groups()
            .iter()
            .filter(|group| group.project_id == session.project_id)
        {
            for session_id in &group.session_ids {
                // `createGxserverPresentationSessionsByProjectFromGroups` leaves out three kinds
                // of row: one the daemon does not list in the sidebar by default, one on the
                // commands surface, and one hidden locally. The second leg below then adds them
                // back at the end, which is what decides the successor when every drawn row of
                // the project is asleep.
                let listed = loaded
                    .server_session(&session.project_id, session_id)
                    .is_some_and(|row| {
                        row.visible_in_sidebar_by_default && row.surface != SessionSurface::Commands
                    });
                if listed && !entry.is_session_hidden(&session.project_id, session_id) {
                    push(&mut ordered, session_id);
                }
            }
        }
        // The second leg is `this.presentation.sessions`, which is the daemon's own array order,
        // and the daemon orders rows by the byte order of `sortKey` (loaded.rs says so for the
        // same reason). The store keeps rows by id, so the order is rebuilt from that key rather
        // than read off a list: it decides the successor whenever the row being slept is one the
        // projection filtered out, which is every stopped and every sidebar-invisible row, and a
        // store built from deltas has no array order to read anyway.
        let mut tail: Vec<(&str, &str)> = loaded
            .server_sessions()
            .filter(|row| row.project_id == session.project_id)
            .map(|row| (row.sort_key.as_str(), row.session_id.as_str()))
            .collect();
        tail.sort_unstable();
        for (_, session_id) in tail {
            push(&mut ordered, session_id);
        }
    }
    push(&mut ordered, &session.session_id);
    ordered
}

/// `isRunningLocalPresentationSession`: the row of `this.presentation`, which on the TypeScript
/// side is the daemon row with the optimistic patch written into it. A locally hidden row is still
/// in that array, so the hide is deliberately NOT consulted here even though the leg above honours
/// it.
fn is_running(core: &Core, session: &SessionKey, session_id: &str) -> bool {
    shown_lifecycle(core, &session.machine, &session.project_id, session_id)
        == Some(LifecycleState::Running)
}

/// The lifecycle the CLIENT shows for a row, which is the daemon's with this store's own overlay on
/// top, and `None` for a row it holds nothing for.
///
/// One function rather than one per caller. This is what `this.presentation` answers in the
/// TypeScript, because `patchPresentationSession` writes the optimistic value straight into that
/// mirror, so every question of the form "is this row awake" has to be asked of the overlay too or
/// a Full Reload's own wake would read the row it just slept as still running.
pub(super) fn shown_lifecycle(
    core: &Core,
    machine: &MachineId,
    project_id: &str,
    session_id: &str,
) -> Option<LifecycleState> {
    let entry = core.presentation().machine(machine)?;
    let server = entry
        .loaded()
        .and_then(|loaded| loaded.server_session(project_id, session_id))?;
    Some(match entry.session_patch(project_id, session_id) {
        Some(patch) => patch
            .lifecycle_state
            .clone()
            .unwrap_or_else(|| server.lifecycle_state.clone()),
        None => server.lifecycle_state.clone(),
    })
}

/// Whether this payload is one this file answers, without building anything. The host asks it
/// before it decides to keep a command out of the old runtime.
pub fn owns_lifecycle_message(message: &Value) -> bool {
    text_field(message, "type") == Some("setSessionSleeping")
}
