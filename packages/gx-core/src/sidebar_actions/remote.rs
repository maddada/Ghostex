//! The remote legs: what a session action does when the row belongs to a remote machine.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! A remote row's action is a call down that machine's tunnel and nothing else. The old runtime
//! patched nothing, moved no focus and hid no row for one: it asked the machine, waited or did not,
//! and let that machine's presentation stream say what happened. So a remote leg has no overlay, no
//! echo guard and no replacement focus to port, and a port that gave remote rows the local
//! treatment would show the user a state the machine never reported.
//!
//! What a remote leg DOES have is a mode, and the mode is behaviour. `requestRemoteGxserver` waits
//! for the answer and lets its caller decide what a failure means (a fork's toast, a snooze's
//! toast, and the sleep that follows an accepted snooze or a park that also sleeps), while
//! `postRemoteGxserverSidebarRequest` fires and forgets and the Rust bridge toasts its failure. A
//! close and a pin are the second kind. Getting that backwards either makes the user wait on a call
//! nobody reads, or makes a park race its own sleep, which is exactly what the shipped comment on
//! `setSessionParked` says the wait is there to prevent.
//!
//! The legs of one action run in order, and a failed WAITED leg stops the rest, which is what the
//! TypeScript's chain of `await`s did: a park whose update failed does not sleep, a snooze the
//! machine refused does not sleep, and a reload whose sleep failed does not wake.
//!
//! Ported from the deleted `gxserver-runtime/remote-machines.ts` (`requestRemoteGxserver`,
//! `postRemoteGxserverSidebarRequest`), `sessions-and-focus.ts` and `auto-sleep.ts` (every remote
//! leg answered here); its parity gate, `tooling/gx-core/remote-action-parity.ts`, is deleted too.
//!
//! SEE-ALSO: apps/desktop/src/app/remote_conn/sidebar_rpc.rs (the one function the bridge and this
//! path both call), apps/desktop/src/app/gx_store/sidebar_remote.rs (the host).

use serde_json::{json, Value};

use crate::keys::SessionKey;

use super::flags::session_flags_of;
use super::fork::fork_params;
use super::lifecycle::{lifecycle_message, lifecycle_params, LifecycleCall};
use super::plan::ToastLevel;
use super::reload::{owns_reload_message, reload_leg_messages};
use super::resolve::text_field;
use super::snooze::{snooze_request, SnoozeCall, LIFECYCLE_FAILURE_DESCRIPTION};

/// `requestRemoteGxserver`'s default `timeoutMs`, which every waited remote leg is sent with.
pub const REMOTE_AWAITED_TIMEOUT_MS: u64 = 20_000;
/// The bridge's timeout for a request that names none, which is every fire-and-forget one
/// (`gpui_remote_sidebar_request_timeout`).
pub const REMOTE_FIRE_AND_FORGET_TIMEOUT_MS: u64 = 15_000;

/// Every payload type a remote leg answers. Split Right is deliberately absent: see the refusal on
/// [`plan_remote_session_action`].
pub const REMOTE_SESSION_MESSAGE_TYPES: [&str; 11] = [
    "setSessionSleeping",
    "closeSession",
    "forkSession",
    "setSessionPinned",
    "setSessionParked",
    "setSessionTag",
    "setSessionFavorite",
    "snoozeSession",
    "unsnoozeSession",
    "fullReloadSession",
    "restartSession",
];

/// Whether the caller waits for the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteCallMode {
    /// `requestRemoteGxserver`: the next leg, or the failure toast, depends on the answer.
    Awaited,
    /// `postRemoteGxserverSidebarRequest`: nobody waits, and the bridge toasts a failure. Always the
    /// last leg of its plan.
    FireAndForget,
}

impl RemoteCallMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Awaited => "awaited",
            Self::FireAndForget => "fireAndForget",
        }
    }
}

/// What the user sees when a waited leg fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFailureToast {
    pub level: ToastLevel,
    pub title: &'static str,
    /// `None` is the text the call failed with. Through this bridge that is always the bridge's own
    /// fixed sentence, never the machine's body, so it names no path and no session.
    pub description: Option<&'static str>,
}

/// One call down the machine's tunnel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteLeg {
    pub path: &'static str,
    /// The RAW ids that machine's daemon accepts. A machine-scoped string must never be sent.
    pub params: Value,
    pub mode: RemoteCallMode,
    pub timeout_ms: u64,
    /// `None` is silence, which is what a rejected `await` with no `catch` gives the user.
    pub on_failure: Option<RemoteFailureToast>,
}

impl RemoteLeg {
    fn awaited(path: &'static str, params: Value, on_failure: Option<RemoteFailureToast>) -> Self {
        Self {
            path,
            params,
            mode: RemoteCallMode::Awaited,
            timeout_ms: REMOTE_AWAITED_TIMEOUT_MS,
            on_failure,
        }
    }

    fn fire_and_forget(path: &'static str, params: Value) -> Self {
        Self {
            path,
            params,
            mode: RemoteCallMode::FireAndForget,
            timeout_ms: REMOTE_FIRE_AND_FORGET_TIMEOUT_MS,
            on_failure: None,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "path": self.path,
            "params": self.params,
            "mode": self.mode.as_str(),
            "timeoutMs": self.timeout_ms,
            "onFailure": self.on_failure.as_ref().map(|toast| json!({
                "level": toast.level.as_str(),
                "title": toast.title,
                "description": toast.description,
            })),
        })
    }
}

/// Which action a plan is, for the counters and the record line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteActionKind {
    Sleep,
    Wake,
    Close,
    Fork,
    Flags,
    ParkThenSleep,
    Snooze,
    Unsnooze,
    Reload,
}

impl RemoteActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Wake => "wake",
            Self::Close => "close",
            Self::Fork => "fork",
            Self::Flags => "flags",
            Self::ParkThenSleep => "parkThenSleep",
            Self::Snooze => "snooze",
            Self::Unsnooze => "unsnooze",
            Self::Reload => "reload",
        }
    }
}

/// One remote action, as the calls it makes in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSessionPlan {
    pub session: SessionKey,
    pub kind: RemoteActionKind,
    pub legs: Vec<RemoteLeg>,
}

/// What happens once a leg has come back, or has been posted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteStep {
    /// Run this leg next.
    Next(usize),
    /// The action is complete.
    Done,
    /// A waited leg failed: the legs after it do not run, and this is what the user sees.
    Stopped { toast: Option<RemoteFailureToast> },
}

impl RemoteSessionPlan {
    /// The machine the calls go to, exactly as the sidebar id spells it. The host normalizes it the
    /// way the bridge does, through the same function.
    pub fn machine_id(&self) -> &str {
        self.session.machine.remote_id().unwrap_or_default()
    }

    /// The step after leg `index`. `ok` is read for a waited leg only: nobody waits for a
    /// fire-and-forget one, so its answer cannot change what happens next.
    pub fn step_after(&self, index: usize, ok: bool) -> RemoteStep {
        let Some(leg) = self.legs.get(index) else {
            return RemoteStep::Done;
        };
        if leg.mode == RemoteCallMode::Awaited && !ok {
            return RemoteStep::Stopped {
                toast: leg.on_failure.clone(),
            };
        }
        match index + 1 < self.legs.len() {
            true => RemoteStep::Next(index + 1),
            false => RemoteStep::Done,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "machine": self.machine_id(),
            "kind": self.kind.as_str(),
            "legs": self.legs.iter().map(RemoteLeg::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Whether this payload names a remote row this file answers, without building anything.
pub fn owns_remote_session_message(message: &Value) -> bool {
    text_field(message, "type").is_some_and(|kind| REMOTE_SESSION_MESSAGE_TYPES.contains(&kind))
        && text_field(message, "sessionId")
            .and_then(SessionKey::parse_remote_scoped_session_id)
            .is_some()
}

/// The remote leg of a per-session payload, or `None` when this file does not answer it.
///
/// A LOCAL row is never answered here: `parse_remote_scoped_session_id` accepts only
/// `remote:<machine>:session:<project>:<session>`, which is `parseGpuiRemotePresentationSessionId`'s
/// pattern. Refused, with the reason at each refusal:
///
/// - **Split Right**, whose remote leg is not a call at all: it acknowledges the row's attention,
///   posts `openRemoteSessionTerminal` through the native project-path bridge and moves the old
///   runtime's REMOTE focus, and remote focus is not the store's yet (a remote row click is not
///   either). Handed over whole rather than performed in part.
/// - **A payload whose own fields do not parse** (a sleep with no `sleeping`, a pin with no
///   `pinned`), for the reason the local planners refuse them: the TypeScript would spread an
///   `undefined` into a real call, and a port that guessed a default would send a different one.
///   The old runtime still sends its call, so this is a hand-off of real work.
pub fn plan_remote_session_action(
    message: &Value,
    sleep_session_when_parking: bool,
) -> Option<RemoteSessionPlan> {
    let kind = text_field(message, "type")?;
    let session = SessionKey::parse_remote_scoped_session_id(text_field(message, "sessionId")?)?;
    let (kind, legs) = match kind {
        "setSessionSleeping" => {
            let (call, _) = lifecycle_message(message)?;
            let kind = match call {
                LifecycleCall::Sleep => RemoteActionKind::Sleep,
                LifecycleCall::Wake => RemoteActionKind::Wake,
            };
            (kind, vec![lifecycle_leg(&session, call)])
        }
        // `transitionSession(id, 'close')`: `/api/killSession`, posted and not waited for. The
        // local close hides the row first and restores it when the call fails; a remote close does
        // neither, and the machine's stream is what takes the row away.
        "closeSession" => (
            RemoteActionKind::Close,
            vec![RemoteLeg::fire_and_forget(
                "/api/killSession",
                json!({
                    "projectId": session.project_id,
                    "reason": "gpui-sidebar",
                    "sessionId": session.session_id,
                }),
            )],
        ),
        // Waited, with a toast on failure, and no activation and no pane afterwards: the machine
        // makes the fork and its refreshed presentation draws it without moving the user
        // (`CDXC:SessionFork 2026-07-10`).
        "forkSession" => (
            RemoteActionKind::Fork,
            vec![RemoteLeg::awaited(
                "/api/forkSession",
                fork_params(&session),
                Some(RemoteFailureToast {
                    level: ToastLevel::Error,
                    title: "Remote fork failed",
                    description: None,
                }),
            )],
        ),
        "setSessionPinned" | "setSessionParked" | "setSessionTag" | "setSessionFavorite" => {
            let (flags, then_sleep) = session_flags_of(message, sleep_session_when_parking)?;
            let params = flags.to_params(&session);
            match then_sleep {
                // "A fire-and-forget remote update can race a response-backed sleep request", so a
                // park that also sleeps WAITS for the flag before it sleeps, and a failed flag stops
                // the sleep.
                true => (
                    RemoteActionKind::ParkThenSleep,
                    vec![
                        RemoteLeg::awaited("/api/updateSession", params, None),
                        lifecycle_leg(&session, LifecycleCall::Sleep),
                    ],
                ),
                false => (
                    RemoteActionKind::Flags,
                    vec![RemoteLeg::fire_and_forget("/api/updateSession", params)],
                ),
            }
        }
        "snoozeSession" | "unsnoozeSession" => {
            let request = snooze_request(message)?;
            let mut legs = vec![RemoteLeg::awaited(
                request.rpc_path,
                request.rpc_params,
                Some(RemoteFailureToast {
                    level: ToastLevel::Warning,
                    title: request.call.failure_title(),
                    description: Some(LIFECYCLE_FAILURE_DESCRIPTION),
                }),
            )];
            // A snoozed session is always asleep (`CDXC:Sessions 2026-09-12 DECISION`), and the
            // sleep follows only an ACCEPTED snooze.
            let kind = match request.call {
                SnoozeCall::Snooze => {
                    legs.push(lifecycle_leg(&session, LifecycleCall::Sleep));
                    RemoteActionKind::Snooze
                }
                SnoozeCall::Unsnooze => RemoteActionKind::Unsnooze,
            };
            (kind, legs)
        }
        _ if owns_reload_message(message) => {
            // The same two legs the local reload is made of, each one a waited call down the
            // tunnel. `forceRemount` rides on the wake message and means nothing here: a remote
            // wake has no local terminal to tear down, and `setSessionSleeping`'s remote leg never
            // reads its options.
            let legs = reload_leg_messages(text_field(message, "sessionId")?)
                .iter()
                .map(|leg| lifecycle_message(leg).map(|(call, _)| lifecycle_leg(&session, call)))
                .collect::<Option<Vec<_>>>()?;
            (RemoteActionKind::Reload, legs)
        }
        _ => return None,
    };
    Some(RemoteSessionPlan {
        session,
        kind,
        legs,
    })
}

/// `setSessionSleeping`'s remote leg: waited, silent on failure, the same body the local call sends.
fn lifecycle_leg(session: &SessionKey, call: LifecycleCall) -> RemoteLeg {
    RemoteLeg::awaited(call.rpc_path(), lifecycle_params(session), None)
}
