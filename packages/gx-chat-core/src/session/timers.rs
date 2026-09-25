//! The core's timer table: what every family uses instead of `setTimeout`.
//!
//! The core reads no clock and owns no threads, so a timer here is a row in a table, not a
//! callback. A family arms one by key, the host wakes the core with [`crate::Event::Tick`] when the
//! frame's `nextWakeMs` is due, and the keys that came due are handed back on
//! `state.core.fired_timers` for that one dispatch.
//!
//! Ported from the `timers` map at the top of
//! `packages/shared/session-chat-controller/native-host.ts` together with its `tick` and the
//! `nextWakeMs` half of `take`. The firing rules are the TypeScript's exactly: a row is due when
//! its deadline is at or before `now`, rows fire in the order they were armed, a one-shot is
//! removed before its handler runs and an interval is re-armed from `now` rather than from its own
//! deadline, so a late wake never fires the same interval twice.

/// The timers whose TypeScript callback begins with a `Date.now()` of its own.
///
/// `tick()` reads the clock once to decide what is due and then runs each due callback; the ones
/// listed here read it again as their first statement (`const now = Date.now()` in the stall
/// watchdog, `setNow(Date.now())` inside the meter, activity and fleet intervals,
/// `readStartedAt = Date.now()` at the top of a seed retry), so on a recorded turn their value is
/// the NEXT read after the tick's, in fire order. The drain in `crate::dispatch::events` assigns
/// those reads; a key not listed here sees the tick's clock. SEE-ALSO: the timer keys in
/// `session/constants.rs`, `menus/lifecycle.rs` and `extras/settle.rs`.
pub fn callback_reads_clock(key: &str) -> bool {
    matches!(
        key,
        "a:stall"
            | "a:seed-retry"
            | "menus.contextMeter"
            | "extras.activityClock"
            | "extras.fleetClock"
            | "menus.switchClock"
    )
}

/// One armed timer.
#[derive(Clone, Debug, PartialEq)]
pub struct TimerEntry {
    /// The family's own name for this timer. Arming the same key again moves the deadline rather
    /// than adding a second row, which is what the TypeScript's `if (timerRef.current !== null)
    /// return` guards achieved by hand.
    pub key: String,
    /// When it comes due, in the host's epoch milliseconds.
    pub due_at_ms: f64,
    /// Set for a repeating timer, which re-arms itself on every fire.
    pub interval_ms: Option<f64>,
}

/// Every timer the core has armed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimerTable {
    entries: Vec<TimerEntry>,
}

impl TimerTable {
    /// Arms a one-shot `delay_ms` from now, replacing an existing row with the same key.
    pub fn arm(&mut self, key: &str, now_ms: f64, delay_ms: f64) {
        self.insert(key, now_ms + delay_ms, None);
    }

    /// Arms a repeating timer, first due one interval from now.
    pub fn arm_interval(&mut self, key: &str, now_ms: f64, interval_ms: f64) {
        self.insert(key, now_ms + interval_ms, Some(interval_ms));
    }

    /// Arms a one-shot only when the key is not armed already, and says whether it did.
    ///
    /// This is the shape most of the ported backoffs want: the TypeScript checks its timer ref for
    /// `null` before scheduling, so a second failure while one retry is pending changes nothing.
    pub fn arm_once(&mut self, key: &str, now_ms: f64, delay_ms: f64) -> bool {
        if self.is_armed(key) {
            return false;
        }
        self.insert(key, now_ms + delay_ms, None);
        true
    }

    fn insert(&mut self, key: &str, due_at_ms: f64, interval_ms: Option<f64>) {
        match self.entries.iter_mut().find(|entry| entry.key == key) {
            Some(entry) => {
                entry.due_at_ms = due_at_ms;
                entry.interval_ms = interval_ms;
            }
            None => self.entries.push(TimerEntry {
                key: key.to_string(),
                due_at_ms,
                interval_ms,
            }),
        }
    }

    /// Drops a timer. True when one was armed.
    pub fn cancel(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key != key);
        self.entries.len() != before
    }

    /// Drops every timer whose key starts with `prefix`, for a family that names a group of them.
    pub fn cancel_prefix(&mut self, prefix: &str) {
        self.entries.retain(|entry| !entry.key.starts_with(prefix));
    }

    /// Whether this key is armed.
    pub fn is_armed(&self, key: &str) -> bool {
        self.entries.iter().any(|entry| entry.key == key)
    }

    /// Whether anything at all is armed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The keys due at `now_ms`, in the order they were armed.
    ///
    /// One-shots are removed and intervals re-armed before the caller acts on them, exactly as the
    /// host's `tick` deletes or re-stamps a row before calling its callback: a handler that arms
    /// the same key again must win over the bookkeeping of the fire that ran it.
    pub fn due(&mut self, now_ms: f64) -> Vec<String> {
        let mut fired = Vec::new();
        let mut keep = Vec::with_capacity(self.entries.len());
        for entry in std::mem::take(&mut self.entries) {
            if entry.due_at_ms > now_ms {
                keep.push(entry);
                continue;
            }
            fired.push(entry.key.clone());
            if let Some(interval) = entry.interval_ms {
                keep.push(TimerEntry {
                    due_at_ms: now_ms + interval,
                    ..entry
                });
            }
        }
        self.entries = keep;
        fired
    }

    /// Milliseconds until the earliest deadline, or `None` when nothing is armed.
    ///
    /// Never negative: an overdue timer asks for an immediate wake, which is what
    /// `Math.max(0, …)` does on the TypeScript side.
    pub fn next_wake_ms(&self, now_ms: f64) -> Option<u64> {
        self.entries
            .iter()
            .map(|entry| entry.due_at_ms)
            .fold(None, |earliest: Option<f64>, due| {
                Some(match earliest {
                    Some(current) if current <= due => current,
                    _ => due,
                })
            })
            .map(|due| (due - now_ms).max(0.0).round() as u64)
    }

    /// The armed rows, for a host that inspects the table.
    pub fn entries(&self) -> &[TimerEntry] {
        &self.entries
    }
}
