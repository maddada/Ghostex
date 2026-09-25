//! Whether the account-switch card is on screen, and the clock its countdowns are measured
//! against.
//!
//! Port of `packages/shared/session-chat-controller/account-switch.ts`. The TypeScript kept
//! `dismissed`, `now` and `observed` in React state and drove them from effects; here they are
//! fields the core advances from the host's clock, so the same sequence of frames produces the
//! same card.
//!
//! CDXC:AgentProviders 2026-09-15 DECISION:
//! User: the account-switch card must not go away until the switch is actually complete and the
//! second account is ready. gxserver's `success` phase only proves the new CLI process is up, so
//! the chat keeps the card, blocks sends, and shows the last step as active until the caller
//! reports `ready`: the session's account read names the target account and no queued model
//! change is still pending. Only the brief success acknowledgement uses a timer, and it starts
//! once `ready` holds; every in-flight step comes from gxserver.

use crate::menus::accounts_presentation::SwitchProgress;
use crate::menus::time::parse_iso_millis;

/// How long a success stays on screen once the account is ready.
pub const SWITCH_ACKNOWLEDGEMENT_MS: i64 = 1_800;

/// A completed switch older than this is history, not something to announce again.
pub const SWITCH_RECENT_MS: i64 = 5_000;

/// The re-render clock the card's countdowns read.
pub const SWITCH_CLOCK_INTERVAL_MS: i64 = 30_000;

/// What the card machine remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountSwitchState {
    /// The switch the user has seen through to the end, so it never comes back.
    pub dismissed: Option<String>,
    /// The switch this chat watched happen, which is what tells a live success from an old one.
    pub observed: Option<String>,
    /// The clock the card's relative times are measured against. Latched rather than live, so a
    /// once-a-second status frame does not rewrite every countdown.
    pub now_ms: Option<i64>,
    /// The id whose acknowledgement timer is running, and when it is due.
    pub acknowledge: Option<(String, i64)>,
    /// The visible card the clock effect last ran for (`[visible?.id]`).
    pub clock_for: Option<String>,
    /// When the card's 30 second `setInterval` next fires, or `None` while no card is up.
    pub clock_due_ms: Option<i64>,
    /// The progress effect's dependencies (`[progress?.id, progress?.phase, progress?.updatedAt,
    /// ready]`) as it last ran, so the core knows when it runs again and reads the clock.
    pub progress_deps: Option<(String, String, String, bool)>,
}

/// The card to draw, the clock it reads, and whether sending is held.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountSwitchStatus {
    /// The switch on screen, or `None`.
    pub visible: Option<SwitchProgress>,
    pub now_ms: i64,
    /// A switch is in flight, so sending is held.
    pub busy: bool,
}

impl AccountSwitchState {
    /// The effect body of `computeAccountSwitchStatus`: advances the machine for this frame.
    ///
    /// Returns when the core next needs a tick, so the host can arm one timer for the
    /// acknowledgement and the card's 30 second clock.
    pub fn observe(
        &mut self,
        progress: Option<&SwitchProgress>,
        ready: bool,
        now_ms: i64,
    ) -> Option<i64> {
        // `useState(Date.now)`: the clock is read once, when the chat first computes its switch
        // status, and then only while a card is up.
        if self.now_ms.is_none() {
            self.now_ms = Some(now_ms);
        }
        let progress = progress?;
        if progress.phase != "success" {
            self.observed = Some(progress.id.clone());
            self.acknowledge = None;
        } else {
            // An old completed switch must not reappear when reopening a conversation.
            let recent = parse_iso_millis(&progress.updated_at)
                .is_some_and(|at| now_ms - at < SWITCH_RECENT_MS);
            if self.observed.as_deref() != Some(progress.id.as_str()) && !recent {
                self.dismissed = Some(progress.id.clone());
                self.acknowledge = None;
            } else if !ready {
                self.acknowledge = None;
            } else {
                match &self.acknowledge {
                    Some((id, due)) if id == &progress.id => {
                        if now_ms >= *due {
                            self.dismissed = Some(progress.id.clone());
                            self.acknowledge = None;
                        }
                    }
                    _ => {
                        self.acknowledge =
                            Some((progress.id.clone(), now_ms + SWITCH_ACKNOWLEDGEMENT_MS));
                    }
                }
            }
        }
        // The card's own clock is `crate::menus::lifecycle`'s: its effect and interval each take
        // a read of their own, which only the caller holding the turn's clock reads can pick.
        self.acknowledge.as_ref().map(|(_, due)| *due)
    }

    /// `visible`: the switch to draw, or nothing.
    pub fn visible_id<'a>(
        &self,
        progress: &'a SwitchProgress,
        now_ms: i64,
    ) -> Option<&'a SwitchProgress> {
        if progress.phase == "cancelled" {
            return None;
        }
        if self.dismissed.as_deref() == Some(progress.id.as_str()) {
            return None;
        }
        if progress.phase == "success"
            && self.observed.as_deref() != Some(progress.id.as_str())
            && parse_iso_millis(&progress.updated_at)
                .is_none_or(|at| now_ms - at >= SWITCH_RECENT_MS)
        {
            return None;
        }
        Some(progress)
    }

    /// `computeAccountSwitchStatus`'s return value for this frame.
    pub fn status(
        &self,
        progress: Option<&SwitchProgress>,
        ready: bool,
        now_ms: i64,
    ) -> AccountSwitchStatus {
        let visible = progress.and_then(|progress| self.visible_id(progress, now_ms));
        let busy = progress.is_some_and(|progress| {
            matches!(
                progress.phase.as_str(),
                "switching" | "resuming" | "continuing"
            ) || (progress.phase == "success" && !ready && visible.is_some())
        });
        AccountSwitchStatus {
            visible: visible.cloned(),
            // `useState(Date.now)`: the clock is read once when the chat opens and then only while
            // a card is up, which is why an idle chat publishes the same number every frame.
            now_ms: self.now_ms.unwrap_or(now_ms),
            busy,
        }
    }
}
