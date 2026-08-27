/*
CDXC:AnonymousAnalytics 2026-08-26:
The process-global telemetry handle and its bounded queue.

Global rather than a field on `AppState` for two reasons. First, emitters live in
places that have no `AppState` in scope (domain repository writes, extension
lifecycle, CLI-facing dispatch helpers); threading state to all of them would
mean touching dozens of signatures for a fire-and-forget side effect. Second, and
more importantly, "uninitialised = silent" is exactly the behaviour the spec
requires for one-shot `ghostex` CLI verbs: only the long-running server loop
calls `init`, so a `gx` subcommand that happens to run a shared code path emits
nothing and starts no task.

The queue is bounded at 1000 and drops the OLDEST event on overflow: during a
network outage the recent events are the ones worth keeping, and an unbounded
queue in a daemon that can run for weeks is a memory leak.
*/

use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

use super::{
    base::build_base_properties,
    client::PostHogEndpoint,
    gate,
    identity::{self, ResolvedIdentity},
    profile::{self, ProfileSnapshot},
    taxonomy::{self, PropertyValue},
};

pub const MAX_QUEUED_EVENTS: usize = 1000;
pub const MAX_BATCH_SIZE: usize = 50;
pub const MAX_BATCH_ATTEMPTS: u8 = 3;

static TELEMETRY: OnceLock<Telemetry> = OnceLock::new();

#[derive(Clone, Debug)]
pub(super) struct QueuedEvent {
    pub(super) event: String,
    pub(super) properties: Map<String, Value>,
    pub(super) timestamp: String,
    pub(super) attempts: u8,
}

pub struct Telemetry {
    pub(super) paths: GxserverPaths,
    pub(super) identity: ResolvedIdentity,
    pub(super) endpoint: PostHogEndpoint,
    pub(super) base_properties: Map<String, Value>,
    queue: Mutex<VecDeque<QueuedEvent>>,
    /// Rate limiters for the events whose taxonomy row specifies one
    /// (`client.connected`, `surface.opened`). Keyed by event + enum member.
    throttles: Mutex<HashMap<String, Instant>>,
    /// The DB-derived profile fields attached to every event. Refreshed at
    /// startup and once per heartbeat run — never per capture.
    profile: Mutex<ProfileSnapshot>,
}

/// Called exactly once, by the long-running server loop. Repeat calls are
/// ignored, which is what keeps a second client from ever existing.
///
/// `server_id` is only the LAST link of the `distinct_id` chain now; see
/// `telemetry::identity` for why a per-human id beats a per-install one.
pub fn init(paths: GxserverPaths, server_id: &str) -> &'static Telemetry {
    TELEMETRY.get_or_init(|| Telemetry {
        identity: identity::resolve(&paths, server_id),
        paths,
        endpoint: PostHogEndpoint::from_environment(),
        base_properties: build_base_properties(),
        queue: Mutex::new(VecDeque::with_capacity(64)),
        throttles: Mutex::new(HashMap::new()),
        profile: Mutex::new(ProfileSnapshot::default()),
    })
}

pub fn handle() -> Option<&'static Telemetry> {
    TELEMETRY.get()
}

impl Telemetry {
    pub fn paths(&self) -> &GxserverPaths {
        &self.paths
    }

    fn lock_queue(&self) -> std::sync::MutexGuard<'_, VecDeque<QueuedEvent>> {
        match self.queue.lock() {
            Ok(queue) => queue,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn lock_throttles(&self) -> std::sync::MutexGuard<'_, HashMap<String, Instant>> {
        match self.throttles.lock() {
            Ok(throttles) => throttles,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn read_profile(&self) -> ProfileSnapshot {
        match self.profile.lock() {
            Ok(profile) => *profile,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    /// Replace the DB-derived half of the profile. Called at startup and once
    /// per heartbeat run — see `telemetry::profile` for why not per capture.
    pub fn refresh_profile(&self, snapshot: ProfileSnapshot) {
        match self.profile.lock() {
            Ok(mut profile) => *profile = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
    }

    /// Which link of the identity chain produced this process's `distinct_id`,
    /// as a taxonomy member.
    pub fn identity_source(&self) -> &'static str {
        self.identity.source.as_enum()
    }

    pub fn is_enabled(&self) -> bool {
        gate::is_enabled(&self.paths)
    }

    /// `true` when this `(event, key)` pair has not fired inside `window`, and
    /// claims the window when it answers `true`.
    pub(super) fn claim_throttle(&self, key: String, window: Duration) -> bool {
        let mut throttles = self.lock_throttles();
        let now = Instant::now();
        throttles.retain(|_, last| now.duration_since(*last) < window);
        match throttles.get(&key) {
            Some(last) if now.duration_since(*last) < window => false,
            _ => {
                throttles.insert(key, now);
                true
            }
        }
    }

    pub(super) fn enqueue(&self, event: &str, properties: &[(&'static str, PropertyValue)]) {
        /*
        ONE gate evaluation, which is also the ONE settings-file stat on this
        path: `evaluate` hands back the opt-out answer and the two
        settings-derived profile fields from the same mtime-keyed cached read.
        Calling `is_enabled()` here and then reading the settings again for the
        profile would have doubled the syscalls on the hottest telemetry path.
        */
        let settings = gate::evaluate(&self.paths);
        if !settings.analytics_enabled {
            /*
            Disabled means DROP, not defer: a queue that survives an opt-out
            would ship the user's pre-opt-out activity the moment they opted
            back in, which is not what "off" means.
            */
            self.drop_queue();
            return;
        }
        /*
        Profile properties are prepended, not spliced into the base map, so they
        go through exactly the same validator as the call site's own properties.
        An event that names one of these keys itself (the heartbeat re-reads
        `default_agent` from the DB when it fires) wins, because it comes later
        in the list and `Map::insert` overwrites.
        */
        let mut merged_properties = profile::profile_properties(
            &settings,
            &self.read_profile(),
            self.identity.source.as_enum(),
        );
        merged_properties.extend_from_slice(properties);
        let validated = match taxonomy::validate(event, &merged_properties) {
            Ok(validated) => validated,
            Err(reason) => {
                super::debug_log(format!("telemetry capture dropped: {reason}"));
                return;
            }
        };
        let mut merged = self.base_properties.clone();
        for (key, value) in validated {
            merged.insert(key, value);
        }
        let queued = QueuedEvent {
            event: event.to_string(),
            properties: merged,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            attempts: 0,
        };
        let mut queue = self.lock_queue();
        while queue.len() >= MAX_QUEUED_EVENTS {
            queue.pop_front();
        }
        queue.push_back(queued);
    }

    pub(super) fn drop_queue(&self) {
        self.lock_queue().clear();
    }

    pub(super) fn take_batch(&self) -> Vec<QueuedEvent> {
        let mut queue = self.lock_queue();
        let take = queue.len().min(MAX_BATCH_SIZE);
        queue.drain(..take).collect()
    }

    pub(super) fn queue_is_empty(&self) -> bool {
        self.lock_queue().is_empty()
    }

    /// Put a failed batch back at the FRONT so ordering survives a retry, minus
    /// whatever has burned its attempt cap.
    pub(super) fn requeue_front(&self, batch: Vec<QueuedEvent>) {
        let mut queue = self.lock_queue();
        for queued in batch.into_iter().rev() {
            if queued.attempts >= MAX_BATCH_ATTEMPTS {
                continue;
            }
            if queue.len() >= MAX_QUEUED_EVENTS {
                queue.pop_back();
            }
            queue.push_front(queued);
        }
    }

    pub(super) fn encode_batch(&self, batch: &[QueuedEvent]) -> Vec<Value> {
        batch
            .iter()
            .map(|queued| {
                json!({
                    "event": queued.event,
                    "distinct_id": self.identity.distinct_id,
                    "properties": Value::Object(queued.properties.clone()),
                    "timestamp": queued.timestamp,
                })
            })
            .collect()
    }
}

/// Replace the DB-derived profile fields. A no-op in a process that never
/// initialised telemetry, like every one-shot CLI verb.
pub fn refresh_profile(snapshot: ProfileSnapshot) {
    if let Some(telemetry) = handle() {
        telemetry.refresh_profile(snapshot);
    }
}

/// The `identity_source` member for this process, or `None` when telemetry was
/// never initialised.
pub fn identity_source() -> Option<&'static str> {
    handle().map(Telemetry::identity_source)
}

/// The one public emitter. A no-op when telemetry was never initialised (every
/// one-shot CLI verb) and when the gate says no.
pub fn capture(event: &str, properties: &[(&'static str, PropertyValue)]) {
    let Some(telemetry) = handle() else {
        return;
    };
    telemetry.enqueue(event, properties);
}

/// `capture`, but only if this `(event, throttle_key)` has not fired inside
/// `window`. Used by the taxonomy rows that specify a dedup window.
pub fn capture_throttled(
    event: &str,
    throttle_key: &str,
    window: Duration,
    properties: &[(&'static str, PropertyValue)],
) {
    let Some(telemetry) = handle() else {
        return;
    };
    /*
    The gate is checked before the throttle is claimed so an opted-out process
    never burns a window it will not use.
    */
    if !telemetry.is_enabled() {
        telemetry.drop_queue();
        return;
    }
    if !telemetry.claim_throttle(format!("{event}:{throttle_key}"), window) {
        return;
    }
    telemetry.enqueue(event, properties);
}
