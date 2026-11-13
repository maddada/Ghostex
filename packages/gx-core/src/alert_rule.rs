//! The one rule for when something a session did plays a sound and shows a notification: a system
//! notification on the computer, a push or local notification on the phone, the sound on either.
//! gxserver applies it when it writes a notification row and sends a push, and stamps its verdict on
//! the presentation (`attention.alerts`, `stuckHandledBy`, `blockedSince`); the desktop, the web
//! build and the phone read those stamps, and fall back to this same rule for an older daemon. The
//! bell keeps a row either way; a row that does not alert is written already read, so it never
//! lights the bell.
//!
//! CDXC:Notifications 2026-11-08 DECISION:
//! User: "please don't show notifications or play sound for worker threads under a coordinator
//! finishing ever. also for a coordinator, we should always only show notification and play sound
//! when he needs you (pink dot basically or blocked red dot) not when he finishes. red dot on any
//! session needs to notify if autopilot isn't enabled and we don't resolve it ourselves. also same
//! for getting blocked cuz account ran out of usage or api errors or stuff like that we should
//! notify and play a sound always (consider this blocked red dot one)". So:
//! - A thread (a session under a coordinator) finishing a turn never alerts, whatever its handover
//!   says: it reports to its coordinator.
//! - A coordinator alerts only when it needs the user: a question or other wait (the pink dot), a
//!   finished turn whose handover has a Blocked or Needs you section, or a red dot. A plain
//!   finished turn is quiet.
//! - A red dot (an approval only the user can grant, a session not moving, a session Ghostex could
//!   not restart) alerts unless Autopilot covers the session and handles it (it answers approvals
//!   on Everything with approvals on; it interrupts, reloads and restarts a stuck session until it
//!   leaves it for the user, which is when the alert comes), or Ghostex healed it itself first.
//! - Blocked (every account out of usage; an API, login, overload or rate-limit error account
//!   recovery could not get past) is a red dot that always alerts, Autopilot or not, unless
//!   Ghostex fixed it right away (an automatic switch to an account with room, a quick retry that
//!   got through), in which case it never became blocked.
//! - Every other session keeps its finished-turn alert, under Alert me for (All, or Needs you only).
//! Supersedes the parts of the 2026-11-07 Alert me for decision (attention.rs) that let a thread's
//! or a coordinator's finished turn alert under All.
//!
//! CDXC:Notifications 2026-11-13 DECISION:
//! User: "we're getting way too many notifications from the coordinator when it finishes. I don't
//! think anyone cares about when the coordinator finishes. Please disable those. Just show for new
//! needs-you, blocked or approval: those need user attention." A coordinator's handover ends with a
//! Needs you section nearly every turn, so for a coordinator gxserver no longer reads it: its
//! `handover_needs_you` means the turn put something new on its board for the user (an item ask,
//! an item moved into Needs you or Blocked; server/src/coordinators/work.rs
//! `raised_for_user_since`). A plain finished turn never alerts, whatever its handover says. Its
//! open question forms, approvals and red dots alert as before (they are not `Finished`).
//! SEE-ALSO: server/src/presentation/attention_alerts.rs (the stamps), server/src/notification_feed/producer.rs
//! (rows and pushes), apps/desktop/src/app/gx_store/attention/host.rs (sound and banner),
//! apps/mobile/app/src/notifications/attentionAlerts.ts (the phone).

use ghostex_gx_protocol::{AttentionKind, PresentationSession, SessionActivity};

use crate::attention::AttentionAlerts;

/// What the session is to its coordinator, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertRole {
    /// A session under a coordinator (`coordinatorRole` `thread`, or a coordinator link).
    Thread,
    /// A coordinator that is not itself a thread.
    Coordinator,
    /// Anything else.
    Session,
}

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertTrigger {
    /// A turn ended with nothing waiting on the user. `handover_needs_you`: its handover has a
    /// Blocked or Needs you section; for a coordinator, the turn raised a new ask, Needs you item
    /// or Blocked item on its board (CDXC:Notifications 2026-11-13).
    Finished { handover_needs_you: bool },
    /// A question card, open question, bell, connector card or other wait: the pink dot.
    NeedsInput,
    /// A permission prompt or approval card only the user can grant: the red dot.
    Approval,
    /// Not moving for a while, or stopped and Ghostex could not restart it: the red dot.
    Stuck,
    /// Out of usage on every account, or an error recovery could not get past: the red dot.
    Blocked,
}

/// Who else is dealing with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlertHandling {
    /// Autopilot covers the session and will settle this itself (answer the approval, heal the
    /// stuck session) instead of leaving it for the user.
    pub autopilot_handles: bool,
    /// Ghostex already fixed it itself (an automatic account switch, the restart after a crash,
    /// the nudge that got it moving), so nothing waits on the user.
    pub healed: bool,
}

/// Whether `trigger` on a session of `role` plays a sound and shows a notification.
pub fn should_alert(
    role: AlertRole,
    trigger: AlertTrigger,
    handling: AlertHandling,
    alerts_for: AttentionAlerts,
) -> bool {
    if handling.healed {
        return false;
    }
    match trigger {
        AlertTrigger::Blocked | AlertTrigger::NeedsInput => true,
        AlertTrigger::Approval | AlertTrigger::Stuck => !handling.autopilot_handles,
        AlertTrigger::Finished { handover_needs_you } => match role {
            AlertRole::Thread => false,
            AlertRole::Coordinator => handover_needs_you,
            AlertRole::Session => match alerts_for {
                AttentionAlerts::All => true,
                AttentionAlerts::NeedsYou => handover_needs_you,
            },
        },
    }
}

/// The role a session's coordinator fields give it (`coordinatorRole`, `coordinatorSessionId`):
/// anything linked to a coordinator is a thread, a coordinator included.
pub fn alert_role(
    coordinator_role: Option<&str>,
    coordinator_session_id: Option<&str>,
) -> AlertRole {
    let linked = coordinator_session_id.is_some_and(|id| !id.trim().is_empty());
    match coordinator_role {
        _ if linked => AlertRole::Thread,
        Some("thread") => AlertRole::Thread,
        Some("coordinator") => AlertRole::Coordinator,
        _ => AlertRole::Session,
    }
}

/// The trigger an attention stands for: a finished turn (`attention.kind` `finished`), else an
/// approval while one waits, else any other wait.
pub fn attention_trigger(
    finished: bool,
    handover_needs_you: bool,
    awaiting_approval: bool,
) -> AlertTrigger {
    if finished {
        AlertTrigger::Finished { handover_needs_you }
    } else if awaiting_approval {
        AlertTrigger::Approval
    } else {
        AlertTrigger::NeedsInput
    }
}

/// The role a presentation row gives its session.
pub fn presentation_role(session: &PresentationSession) -> AlertRole {
    alert_role(
        session.coordinator_role.as_deref(),
        session.coordinator_session_id.as_deref(),
    )
}

/// The trigger a row in attention stands for, or `None` outside attention.
pub fn presentation_attention_trigger(session: &PresentationSession) -> Option<AlertTrigger> {
    if session.activity != SessionActivity::Attention {
        return None;
    }
    let attention = session.attention.as_ref()?;
    Some(attention_trigger(
        attention.kind == Some(AttentionKind::Finished),
        attention.handover_needs_you,
        session.awaiting_approval,
    ))
}

/// Whether a row's attention alerts on this client: gxserver's verdict (`attention.alerts`), else
/// the rule applied to the row as an older daemon sends it (nobody handling it), then Alert me for
/// on top. An attention from a daemon too old to name its kind alerts as before.
pub fn attention_alerts(session: &PresentationSession, alerts_for: AttentionAlerts) -> bool {
    let Some(attention) = session.attention.as_ref() else {
        return false;
    };
    let trigger = presentation_attention_trigger(session);
    let server = attention.alerts.unwrap_or_else(|| {
        trigger.is_none_or(|trigger| {
            should_alert(
                presentation_role(session),
                trigger,
                AlertHandling::default(),
                AttentionAlerts::All,
            )
        })
    });
    if !server {
        return false;
    }
    match (alerts_for, trigger) {
        (AttentionAlerts::All, _) | (_, None) => true,
        (AttentionAlerts::NeedsYou, Some(trigger)) => should_alert(
            presentation_role(session),
            trigger,
            AlertHandling::default(),
            AttentionAlerts::NeedsYou,
        ),
    }
}

/// Whether a stuck row's red dot alerts: stuck, and nobody (Autopilot) is healing it.
pub fn stuck_alerts(session: &PresentationSession) -> bool {
    session.stuck_since.is_some()
        && should_alert(
            presentation_role(session),
            AlertTrigger::Stuck,
            AlertHandling {
                autopilot_handles: session
                    .stuck_handled_by
                    .as_deref()
                    .is_some_and(|by| !by.trim().is_empty()),
                healed: false,
            },
            AttentionAlerts::All,
        )
}

/// Whether a row is blocked (and so alerts).
pub fn blocked_alerts(session: &PresentationSession) -> bool {
    session.blocked_since.is_some()
        && should_alert(
            presentation_role(session),
            AlertTrigger::Blocked,
            AlertHandling::default(),
            AttentionAlerts::All,
        )
}
