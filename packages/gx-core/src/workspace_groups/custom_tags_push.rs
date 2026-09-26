//! The custom session tag catalog's write-through to gxserver: a debounced push with a retry, as a
//! state machine the host drives with its clock.
//!
//! CDXC:Sessions 2026-09-25 WHY:
//! Settings edits the tag catalog in an app-modal window, and the old runtime's
//! `queueCustomSessionTagsServerSync` / `pushCustomSessionTagsToGxserver` pushed it: every edit
//! replaces the pending catalog and re-arms a 400 ms timer, a failed push re-arms a 5 s retry only
//! while the catalog is still owed and no timer is armed, and a push whose catalog was replaced
//! while it was in flight leaves the catalog owed so the newer one goes out. This is that machine,
//! step for step; the app store does not edit the catalog locally, so there is no echo to refuse
//! here: the sidebar keeps drawing the daemon's copy, exactly as it did.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/custom_tags_sync.rs (the host),
//! packages/shared/gxserver-protocol.ts (`GxserverCustomSessionTagsState`).

use serde_json::Value;

/// `GPUI_CUSTOM_SESSION_TAGS_SERVER_SYNC_DELAY_MS`.
pub const CUSTOM_SESSION_TAGS_SYNC_DELAY_MS: u64 = 400;
/// `GPUI_CUSTOM_SESSION_TAGS_SERVER_SYNC_RETRY_DELAY_MS`.
pub const CUSTOM_SESSION_TAGS_SYNC_RETRY_DELAY_MS: u64 = 5_000;

/// What the host does next.
#[derive(Clone, Debug, PartialEq)]
pub enum CustomTagsPushEffect {
    /// Arm the timer, replacing any armed one; when it fires, call `timer_fired(booking)`.
    Arm { booking: u64, delay_ms: u64 },
    /// Send `/api/updateCustomSessionTags` with `{ state }`; report `push_finished(revision, ok)`.
    Push { revision: u64, state: Value },
}

/// The catalog owed to the local daemon and the one armed timer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomTagsPush {
    latest: Option<(u64, Value)>,
    revision: u64,
    pending: bool,
    booking: u64,
    armed: bool,
}

impl CustomTagsPush {
    /// `queueCustomSessionTagsServerSync(state)`.
    pub fn queue(&mut self, state: Value) -> CustomTagsPushEffect {
        self.revision += 1;
        self.latest = Some((self.revision, state));
        self.pending = true;
        self.arm(CUSTOM_SESSION_TAGS_SYNC_DELAY_MS)
    }

    /// The armed timer fired. `None` for a timer that was replaced (`clearTimeout`), or when
    /// nothing is owed, which is the TypeScript's `if (!pushed) return`.
    pub fn timer_fired(&mut self, booking: u64) -> Option<CustomTagsPushEffect> {
        if booking != self.booking || !self.armed {
            return None;
        }
        self.armed = false;
        let (revision, state) = self.latest.clone()?;
        Some(CustomTagsPushEffect::Push { revision, state })
    }

    /// The push came back. A success clears the debt only when no newer catalog was queued since;
    /// a failure re-arms the retry only while the debt stands and no timer is armed.
    pub fn push_finished(&mut self, revision: u64, ok: bool) -> Option<CustomTagsPushEffect> {
        if ok {
            if self
                .latest
                .as_ref()
                .is_some_and(|(latest, _)| *latest == revision)
            {
                self.pending = false;
            }
            return None;
        }
        if self.armed || !self.pending {
            return None;
        }
        Some(self.arm(CUSTOM_SESSION_TAGS_SYNC_RETRY_DELAY_MS))
    }

    /// Whether a catalog is owed to the daemon.
    pub fn pending(&self) -> bool {
        self.pending
    }

    fn arm(&mut self, delay_ms: u64) -> CustomTagsPushEffect {
        self.booking += 1;
        self.armed = true;
        CustomTagsPushEffect::Arm {
            booking: self.booking,
            delay_ms,
        }
    }
}
