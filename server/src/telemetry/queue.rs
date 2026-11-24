/*
CDXC:Telemetry 2026-08-26:
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
    gate, identity,
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
    /// Random per-install UUIDv4 (`telemetry::identity`), loaded lazily and
    /// persisted only when analytics are enabled and ready to flush.
    distinct_id: Mutex<Option<String>>,
    pub(super) endpoint: PostHogEndpoint,
    pub(super) base_properties: Map<String, Value>,
    queue: Mutex<VecDeque<QueuedEvent>>,
    /// Rate limiters for the events whose taxonomy row specifies one
    /// (`client.connected`, `surface.opened`). Keyed by event + enum member.
    throttles: Mutex<HashMap<String, Instant>>,
    /// The DB-derived profile fields attached to every event. Refreshed at
    /// startup and once per heartbeat run (never per capture).
    profile: Mutex<ProfileSnapshot>,
}

/// Called exactly once, by the long-running server loop. Repeat calls are
/// ignored, which is what keeps a second client from ever existing.
pub fn init(paths: GxserverPaths) -> &'static Telemetry {
    TELEMETRY.get_or_init(|| {
        let existing_id = identity::read_existing_install_id(&paths);
        Telemetry {
            paths,
            distinct_id: Mutex::new(existing_id),
            endpoint: PostHogEndpoint::from_environment(),
            base_properties: build_base_properties(),
            queue: Mutex::new(VecDeque::with_capacity(64)),
            throttles: Mutex::new(HashMap::new()),
            profile: Mutex::new(ProfileSnapshot::default()),
        }
    })
}

/// A private `Telemetry` for unit tests: never the process-global handle, a
/// fixed UUIDv4, and an endpoint that is never flushed, so a test cannot reach
/// PostHog.
#[cfg(test)]
pub(super) fn telemetry_for_test(paths: GxserverPaths) -> Telemetry {
    Telemetry {
        paths,
        distinct_id: Mutex::new(Some("00000000-0000-4000-8000-000000000001".to_string())),
        endpoint: PostHogEndpoint {
            batch_url: "http://127.0.0.1:9/never-flushed".to_string(),
            api_key: "test".to_string(),
        },
        base_properties: build_base_properties(),
        queue: Mutex::new(VecDeque::new()),
        throttles: Mutex::new(HashMap::new()),
        profile: Mutex::new(ProfileSnapshot::default()),
    }
}

#[cfg(test)]
impl Telemetry {
    pub(super) fn queued_for_test(&self) -> Vec<(String, Map<String, Value>)> {
        self.lock_queue()
            .iter()
            .map(|queued| (queued.event.clone(), queued.properties.clone()))
            .collect()
    }
}

pub fn handle() -> Option<&'static Telemetry> {
    TELEMETRY.get()
}

impl Telemetry {
    pub fn paths(&self) -> &GxserverPaths {
        &self.paths
    }

    pub fn endpoint_url(&self) -> &str {
        &self.endpoint.batch_url
    }

    pub fn queued_len(&self) -> usize {
        self.lock_queue().len()
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

    pub(crate) fn read_profile(&self) -> ProfileSnapshot {
        match self.profile.lock() {
            Ok(profile) => *profile,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    /// Replace the DB-derived half of the profile. Called at startup and once
    /// per heartbeat run (see `telemetry::profile` for why not per capture).
    pub fn refresh_profile(&self, snapshot: ProfileSnapshot) {
        match self.profile.lock() {
            Ok(mut profile) => *profile = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
    }

    /// Return the random per-install UUIDv4, generating one if needed. When
    /// `persist` is `true`, ensures it is written to `analytics-install-id`.
    /// When `persist` is `false` (e.g. preview while onboarding is pending),
    /// keeps the UUIDv4 in memory without writing to disk.
    pub(crate) fn ensure_distinct_id(&self, persist: bool) -> String {
        let mut guard = self
            .distinct_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let id = match guard.clone() {
            Some(existing) => existing,
            None => {
                let resolved = identity::read_existing_install_id(&self.paths)
                    .unwrap_or_else(identity::generate_install_id);
                *guard = Some(resolved.clone());
                resolved
            }
        };
        if persist && identity::read_existing_install_id(&self.paths).is_none() {
            identity::persist_install_id(&self.paths, &id);
        }
        id
    }

    /// Read the distinct id for read-only inspection (`POST /api/telemetryPreview`).
    /// Returns `None` when disabled and no install id file exists on disk.
    pub(crate) fn distinct_id_for_preview(&self) -> Option<String> {
        if !self.is_enabled() {
            return identity::read_existing_install_id(&self.paths);
        }
        Some(self.ensure_distinct_id(false))
    }

    pub(super) fn clear_cached_identity(&self) {
        let mut guard = self
            .distinct_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
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
        path: `evaluate` hands back the opt-out answer and the settings-derived
        profile field from the same mtime-keyed cached read.
        */
        let settings = gate::evaluate(&self.paths);
        if !settings.analytics_enabled {
            /*
            Disabled means DROP and WIPE, not defer: a queue or persisted state
            that survives an opt-out would ship pre-opt-out activity if the user
            ever opted back in.
            */
            self.drop_queue();
            self.clear_cached_identity();
            super::task::purge_on_opt_out_if_needed(&self.paths);
            return;
        }
        super::task::reset_opt_out_wiped();
        /*
        Profile properties are prepended, not spliced into the base map, so they
        go through exactly the same validator as the call site's own properties.
        An event that names one of these keys itself (the heartbeat re-reads
        `default_agent` from the DB when it fires) wins, because it comes later
        in the list and `Map::insert` overwrites.
        */
        let mut merged_properties = profile::profile_properties(&settings, &self.read_profile());
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

    pub(super) fn encode_batch(&self, batch: &[QueuedEvent], distinct_id: &str) -> Vec<Value> {
        batch
            .iter()
            .map(|queued| {
                json!({
                    "event": queued.event,
                    "distinct_id": distinct_id,
                    "properties": Value::Object(queued.properties.clone()),
                    "timestamp": queued.timestamp,
                })
            })
            .collect()
    }
}

/// Build the exact sanitized PostHog JSON payload for one event without
/// mutating the queue, SQLite, or disk. Used by `POST /api/telemetryPreview`.
pub fn build_preview_event(
    paths: &GxserverPaths,
    profile_snapshot: &ProfileSnapshot,
    distinct_id: Option<&str>,
    event: &str,
    properties: &[(&'static str, PropertyValue)],
) -> Option<Value> {
    let settings = gate::settings_profile(paths);
    let mut merged_properties = profile::profile_properties(&settings, profile_snapshot);
    merged_properties.extend_from_slice(properties);
    let validated = taxonomy::validate(event, &merged_properties).ok()?;
    let mut merged = handle()
        .map(|telemetry| telemetry.base_properties.clone())
        .unwrap_or_else(build_base_properties);
    for (key, value) in validated {
        merged.insert(key, value);
    }
    Some(json!({
        "event": event,
        "distinct_id": distinct_id,
        "properties": Value::Object(merged),
    }))
}

/// Replace the DB-derived profile fields. A no-op in a process that never
/// initialised telemetry, like every one-shot CLI verb.
pub fn refresh_profile(snapshot: ProfileSnapshot) {
    if let Some(telemetry) = handle() {
        telemetry.refresh_profile(snapshot);
    }
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
        telemetry.clear_cached_identity();
        super::task::purge_on_opt_out_if_needed(telemetry.paths());
        return;
    }
    if !telemetry.claim_throttle(format!("{event}:{throttle_key}"), window) {
        return;
    }
    telemetry.enqueue(event, properties);
}
