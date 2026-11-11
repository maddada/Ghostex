use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::registration::{is_printable_id, SESSION_ID_MAX};

/// Alerts one phone may receive per rolling window.
pub(crate) const ALERTS_PER_WINDOW: usize = 8;
pub(crate) const ALERT_WINDOW_MS: i64 = 60_000;
/// One alert per session and phone in this span.
pub(crate) const SESSION_GAP_MS: i64 = 20_000;
/// Background-only start messages one phone may receive per rolling hour.
pub(crate) const SILENT_PER_WINDOW: usize = 2;
pub(crate) const SILENT_WINDOW_MS: i64 = 3_600_000;

/// CDXC:Notifications 2026-10-31 WHY:
/// One phone gets at most 8 alert requests in any 60 s, at most one new alert per session in 20 s, and at most 2 background-only start messages in any hour; the two budgets are separate, so a start message never takes an alert's place. Every request to Expo, retries included, uses a slot of its budget, so the phone never gets more requests than the rule allows; the 20 s session rule applies when a new alert is admitted, and a retry of that same alert waits for a free slot within its expiry instead.
/// The times are wall-clock milliseconds kept in the private store, so a restart does not reset the budget. A time later than now (the clock was set back) keeps counting until the window after it has passed, so turning the clock back never frees slots early.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PhoneBudget {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    alert_sends: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    silent_sends: Vec<i64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    session_alerts: BTreeMap<String, i64>,
}

impl PhoneBudget {
    /// When an alert request may go out: `now` when a slot is free, else the moment the request
    /// that frees one leaves the window.
    pub(crate) fn alert_slot_at(&self, now: i64) -> i64 {
        slot_at(&self.alert_sends, ALERTS_PER_WINDOW, ALERT_WINDOW_MS, now)
    }

    pub(crate) fn silent_slot_free(&self, now: i64) -> bool {
        slot_at(&self.silent_sends, SILENT_PER_WINDOW, SILENT_WINDOW_MS, now) <= now
    }

    /// Whether a new alert of `session_id` may be admitted.
    pub(crate) fn session_free(&self, session_id: &str, now: i64) -> bool {
        self.session_alerts
            .get(session_id)
            .is_none_or(|admitted| *admitted <= now - SESSION_GAP_MS)
    }

    /// Whether an alert went to this phone within the last `span_ms`.
    pub(crate) fn alerted_within(&self, now: i64, span_ms: i64) -> bool {
        self.alert_sends.iter().any(|sent| *sent > now - span_ms)
    }

    pub(crate) fn record_alert(&mut self, now: i64, admitted_session: Option<&str>) {
        self.prune(now);
        self.alert_sends.push(now);
        if let Some(session_id) = admitted_session {
            self.session_alerts.insert(session_id.to_string(), now);
        }
    }

    pub(crate) fn record_silent(&mut self, now: i64) {
        self.prune(now);
        self.silent_sends.push(now);
    }

    fn is_empty(&self) -> bool {
        self.alert_sends.is_empty()
            && self.silent_sends.is_empty()
            && self.session_alerts.is_empty()
    }

    /// Drops times that no longer count.
    fn prune(&mut self, now: i64) {
        self.alert_sends
            .retain(|sent| *sent > now - ALERT_WINDOW_MS);
        self.silent_sends
            .retain(|sent| *sent > now - SILENT_WINDOW_MS);
        self.session_alerts
            .retain(|_, admitted| *admitted > now - SESSION_GAP_MS);
    }

    /// A budget only this module wrote: a few times per list and printable session ids.
    pub(crate) fn is_valid(&self) -> bool {
        self.alert_sends.len() <= ALERTS_PER_WINDOW * 4
            && self.silent_sends.len() <= SILENT_PER_WINDOW * 4
            && self.session_alerts.len() <= ALERTS_PER_WINDOW * 4
            && self
                .session_alerts
                .keys()
                .all(|session_id| is_printable_id(session_id, SESSION_ID_MAX))
    }
}

/// `now` when fewer than `limit` sends count in the window, else when the send that frees a slot
/// leaves it. The lists hold a handful of times, so the quadratic rank search stays trivial.
fn slot_at(sends: &[i64], limit: usize, window: i64, now: i64) -> i64 {
    let counted = || {
        sends
            .iter()
            .copied()
            .filter(move |sent| *sent > now - window)
    };
    let count = counted().count();
    if count < limit {
        return now;
    }
    let rank = count - limit;
    counted()
        .find(|candidate| {
            counted().filter(|other| other < candidate).count() <= rank
                && counted().filter(|other| other <= candidate).count() > rank
        })
        .map_or(now, |freeing| freeing + window)
}

/// Prunes every phone's budget and forgets phones with nothing left to count.
pub(crate) fn prune_budgets(budgets: &mut BTreeMap<String, PhoneBudget>, now: i64) {
    budgets.retain(|_, budget| {
        budget.prune(now);
        !budget.is_empty()
    });
}
