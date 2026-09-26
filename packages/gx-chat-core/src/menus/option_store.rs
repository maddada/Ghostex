//! The optimistic option store: a pill shows the user's choice at once and reconciles with what
//! the agent turns out to be running.
//!
//! Port of `packages/shared/session-chat-controller/option-state.ts`. The TypeScript owned a
//! `setTimeout` for the grace window; here the deadline is a number the core reports through
//! [`OptionStore::next_wake_ms`] and the host turns into `Effect::SetTimer`, so nothing in this
//! file reads a clock or holds a closure.
//!
//! CDXC:AgentScreenDetection 2026-09-05 DECISION:
//! User: model, effort, Fast and Plan selections update optimistically, then reconcile with the
//! agent; this supersedes waiting for footer detection before showing Fast or Plan. Each change
//! owns only its fields, so an old failure cannot roll back a newer selection or another session.

use std::collections::BTreeMap;

use crate::menus::option_catalog::SessionOptionCatalog;
use crate::menus::option_values::{
    apply_detected_options, reconcile_options_from_command, seed_option_state, DetectedOptions,
    OptionSource, OptionState, OptionValue, SESSION_CHAT_DISPATCH_GRACE_MS,
};
use crate::menus::time::{iso_from_millis, parse_iso_millis};

/// One optimistic change still waiting for the agent to confirm it.
#[derive(Clone, Debug, Default, PartialEq)]
struct PendingOptionChange {
    /// The generation that owns these fields, so an old failure cannot roll back a newer one.
    id: u64,
    /// Descriptor id to the value this change asked for.
    values: BTreeMap<String, String>,
    /// The state before the change, restored when it is rolled back.
    previous: OptionState,
    started_at: i64,
    delivered_at: Option<i64>,
}

/// A receipt for one dispatch, handed back so the caller can complete or roll it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchReceipt(u64);

/// The pill values of one session, and the changes still in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionStore {
    /// The storage key this store reads and writes, `undefined` for a session with none.
    pub storage_key: Option<String>,
    state: OptionState,
    /// What the agent was last seen running, independent of the optimistic overlay.
    detected_state: OptionState,
    pending: Vec<PendingOptionChange>,
    next_change_id: u64,
    /// Set when a grace window expired without confirmation, which the host reads once.
    unconfirmed: bool,
    /// Every state published since the host last wrote, oldest first. The TypeScript called
    /// `persistence.write` on EVERY publish, so a turn that publishes twice (a detection and a
    /// receipt completing on the same frame) makes two `optionWrite` round trips, not one.
    published: Vec<OptionState>,
}

impl OptionStore {
    /// A store seeded from what was persisted for this session.
    ///
    /// `createSessionChatOptionState`: still-pending local intent survives the reseed, and a value
    /// that was already dispatched keeps its place in the pending table so its grace window
    /// continues across a remount.
    pub fn seeded(
        catalog: Option<&SessionOptionCatalog>,
        storage_key: Option<String>,
        stored: &OptionState,
        now_ms: i64,
    ) -> Self {
        let state = match catalog {
            Some(catalog) => seed_option_state(catalog, stored, now_ms),
            None => OptionState::new(),
        };
        let mut store = Self {
            storage_key,
            state,
            ..Self::default()
        };
        let seeded: Vec<(String, OptionValue)> = store
            .state
            .iter()
            .map(|(id, value)| (id.clone(), value.clone()))
            .collect();
        for (id, value) in seeded {
            let dispatched_at = value.dispatched_at.as_deref().and_then(parse_iso_millis);
            if value.source == OptionSource::Dispatched {
                if let Some(dispatched_at) = dispatched_at {
                    store.next_change_id += 1;
                    store.pending.push(PendingOptionChange {
                        id: store.next_change_id,
                        values: BTreeMap::from([(id.clone(), value.value.clone())]),
                        previous: OptionState::new(),
                        started_at: dispatched_at,
                        delivered_at: Some(dispatched_at),
                    });
                }
            }
        }
        store
    }

    /// `getSnapshot()`.
    pub fn state(&self) -> &OptionState {
        &self.state
    }

    /// The state to persist, and whether it changed since the host last wrote it.
    pub fn take_dirty(&mut self) -> Option<OptionState> {
        std::mem::take(&mut self.published).pop()
    }

    /// Every state published since the host last wrote, oldest first: one write each.
    pub fn take_published(&mut self) -> Vec<OptionState> {
        std::mem::take(&mut self.published)
    }

    /// Whether a grace window expired without the agent confirming the change. Read once.
    pub fn take_unconfirmed(&mut self) -> bool {
        std::mem::replace(&mut self.unconfirmed, false)
    }

    /// When the next grace window runs out, for the host's timer. `None` while nothing is pending.
    pub fn next_wake_ms(&self) -> Option<i64> {
        self.pending
            .iter()
            .filter_map(|change| {
                change
                    .delivered_at
                    .map(|at| at + SESSION_CHAT_DISPATCH_GRACE_MS)
            })
            .min()
    }

    /// `publish(next)`.
    ///
    /// The TypeScript's guard is `if (state === next) return`, an IDENTITY test: every caller but
    /// one builds a fresh object, so an unchanged value still notifies its listeners and still
    /// persists. Comparing contents here instead lost the `optionWrite` round trips the replay
    /// paired its storage answers against, so the guard is the caller's (`begin_dispatch` is the
    /// one that can hand back `state` itself, when it was asked for no values at all).
    fn publish(&mut self, next: OptionState) {
        self.published.push(next.clone());
        self.state = next;
    }

    fn change(&self, id: u64) -> Option<&PendingOptionChange> {
        self.pending.iter().find(|change| change.id == id)
    }

    /// Which change currently owns a descriptor, which is the TypeScript's `pending.get(id)`.
    fn owner_of(&self, descriptor_id: &str) -> Option<u64> {
        self.pending
            .iter()
            .rev()
            .find(|change| change.values.contains_key(descriptor_id))
            .map(|change| change.id)
    }

    fn release(&mut self, descriptor_id: &str, change_id: u64) {
        if let Some(change) = self
            .pending
            .iter_mut()
            .find(|change| change.id == change_id)
        {
            change.values.remove(descriptor_id);
        }
        self.pending.retain(|change| !change.values.is_empty());
    }

    /// `beginDispatch(values)`: shows the values at once and starts their grace window.
    pub fn begin_dispatch(
        &mut self,
        values: impl IntoIterator<Item = (String, String)>,
        now_ms: i64,
    ) -> DispatchReceipt {
        let values: BTreeMap<String, String> = values.into_iter().collect();
        self.next_change_id += 1;
        let change = PendingOptionChange {
            id: self.next_change_id,
            values: values.clone(),
            previous: self.state.clone(),
            started_at: now_ms,
            delivered_at: None,
        };
        let mut next = self.state.clone();
        for (id, value) in &values {
            next.insert(
                id.clone(),
                OptionValue {
                    value: value.clone(),
                    source: OptionSource::Dispatched,
                    label: None,
                    dispatched_at: Some(iso_from_millis(now_ms)),
                    detected_source: None,
                    detected_at: None,
                },
            );
        }
        // A descriptor this change claims leaves whatever change held it before.
        for id in values.keys() {
            if let Some(previous) = self.owner_of(id) {
                self.release(id, previous);
            }
        }
        self.pending.push(change);
        // `let next = state; for (…) next = {...next, …}`: with no values there is no spread and
        // `next` is still `state` itself, which is the one publish the TypeScript skips.
        if !values.is_empty() {
            self.publish(next);
        }
        DispatchReceipt(self.next_change_id)
    }

    /// `receipt.complete()`: the command reached the agent, so the grace window starts now.
    ///
    /// The receipt's closure holds its change even once a detection confirmed every value and took
    /// it out of the pending table, and it publishes a fresh object either way: one `optionWrite`
    /// per call, owned values or not.
    pub fn complete(&mut self, receipt: DispatchReceipt, now_ms: i64) {
        let change = self.change(receipt.0).cloned().unwrap_or_default();
        if let Some(entry) = self
            .pending
            .iter_mut()
            .find(|pending| pending.id == receipt.0)
        {
            entry.delivered_at = Some(now_ms);
        }
        let mut next = self.state.clone();
        for id in change.values.keys() {
            if self.owner_of(id) == Some(receipt.0) {
                if let Some(entry) = next.get_mut(id) {
                    if entry.source == OptionSource::Dispatched {
                        entry.dispatched_at = Some(iso_from_millis(now_ms));
                    }
                }
            }
        }
        self.publish(next);
    }

    /// `receipt.rollback()`: the command failed, so the pill goes back to what it showed.
    ///
    /// Publishes even when no value is still its own, as [`OptionStore::complete`] does.
    pub fn rollback(&mut self, receipt: DispatchReceipt) {
        let change = self.change(receipt.0).cloned().unwrap_or_default();
        let mut next = self.state.clone();
        for id in change.values.keys() {
            if self.owner_of(id) != Some(receipt.0) {
                continue;
            }
            self.release(id, receipt.0);
            match self
                .detected_state
                .get(id)
                .or_else(|| change.previous.get(id))
            {
                Some(previous) => {
                    next.insert(id.clone(), previous.clone());
                }
                None => {
                    next.remove(id);
                }
            }
        }
        self.publish(next);
    }

    /// `recordDispatched(id, value)`: a change that is already on the wire.
    pub fn record_dispatched(&mut self, id: &str, value: &str, now_ms: i64) {
        let receipt = self.begin_dispatch([(id.to_string(), value.to_string())], now_ms);
        self.complete(receipt, now_ms);
    }

    /// `reconcileTypedCommand(text)`: a command the user typed moves the pills too.
    pub fn reconcile_typed_command(
        &mut self,
        catalog: Option<&SessionOptionCatalog>,
        text: &str,
        now_ms: i64,
    ) {
        let Some(catalog) = catalog else { return };
        let next = reconcile_options_from_command(catalog, &self.state, text, now_ms);
        let changed: Vec<(String, String)> = next
            .iter()
            .filter(|(id, entry)| self.state.get(*id) != Some(*entry))
            .map(|(id, entry)| (id.clone(), entry.value.clone()))
            .collect();
        if changed.is_empty() {
            return;
        }
        let receipt = self.begin_dispatch(changed, now_ms);
        self.complete(receipt, now_ms);
    }

    /// `applyDetected(detected)`: gxserver's reading of the agent's screen.
    pub fn apply_detected(
        &mut self,
        catalog: Option<&SessionOptionCatalog>,
        detected: Option<&DetectedOptions>,
    ) {
        let (Some(catalog), Some(detected)) = (catalog, detected) else {
            return;
        };
        self.detected_state = apply_detected_options(catalog, &self.detected_state, Some(detected));
        let mut next = apply_detected_options(catalog, &self.state, Some(detected));
        let owned: Vec<(String, u64, String)> = self
            .pending
            .iter()
            .flat_map(|change| {
                change
                    .values
                    .iter()
                    .map(|(id, value)| (id.clone(), change.id, value.clone()))
            })
            .collect();
        for (id, change_id, wanted) in owned {
            let Some(change) = self.change(change_id).cloned() else {
                continue;
            };
            let actual = self.detected_state.get(&id);
            let fresh = actual
                .and_then(|entry| entry.detected_at.as_deref())
                .and_then(parse_iso_millis)
                .is_some_and(|at| at >= change.started_at);
            match actual {
                Some(actual) if fresh && actual.value == wanted => {
                    next.insert(id.clone(), actual.clone());
                    self.release(&id, change_id);
                }
                _ => {
                    if let Some(current) = self.state.get(&id) {
                        next.insert(id.clone(), current.clone());
                    }
                }
            }
        }
        self.publish(next);
    }

    /// The grace-window sweep the TypeScript ran on a timer: a change that was delivered more
    /// than [`SESSION_CHAT_DISPATCH_GRACE_MS`] ago and never confirmed gives its pill back.
    ///
    /// An unchanged footer is still fresh evidence after a rejected CLI toggle, so a detection
    /// taken since the change started replaces the value rather than clearing it.
    pub fn expire(&mut self, now_ms: i64) {
        let due: Vec<u64> = self
            .pending
            .iter()
            .filter(|change| {
                change
                    .delivered_at
                    .is_some_and(|at| now_ms >= at + SESSION_CHAT_DISPATCH_GRACE_MS)
            })
            .map(|change| change.id)
            .collect();
        if due.is_empty() {
            return;
        }
        let mut next = self.state.clone();
        let mut expired = false;
        for change_id in due {
            let Some(change) = self.change(change_id).cloned() else {
                continue;
            };
            for id in change.values.keys() {
                self.release(id, change_id);
                match self.detected_state.get(id) {
                    Some(detected)
                        if detected
                            .detected_at
                            .as_deref()
                            .and_then(parse_iso_millis)
                            .is_some_and(|at| at >= change.started_at) =>
                    {
                        next.insert(id.clone(), detected.clone());
                    }
                    _ => {
                        next.remove(id);
                    }
                }
            }
            expired = true;
        }
        if expired {
            self.publish(next);
            self.unconfirmed = true;
        }
    }
}
