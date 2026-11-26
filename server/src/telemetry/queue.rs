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
    gate::{self, TelemetryMode},
    identity,
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
    distinct_id: Mutex<Option<(TelemetryMode, String)>>,
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
    TELEMETRY.get_or_init(|| Telemetry {
        paths,
        distinct_id: Mutex::new(None),
        endpoint: PostHogEndpoint::from_environment(),
        base_properties: build_base_properties(),
        queue: Mutex::new(VecDeque::with_capacity(64)),
        throttles: Mutex::new(HashMap::new()),
        profile: Mutex::new(ProfileSnapshot::default()),
    })
}

/// A private `Telemetry` for unit tests: never the process-global handle, an
/// endpoint that is never flushed, so a test cannot reach
/// PostHog.
#[cfg(test)]
pub(super) fn telemetry_for_test(paths: GxserverPaths) -> Telemetry {
    Telemetry {
        paths,
        distinct_id: Mutex::new(None),
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

fn load_id(paths: &GxserverPaths, filename: &str) -> String {
    identity::read_install_id(paths, filename).unwrap_or_else(identity::generate_install_id)
}

pub(super) fn retire_other_ids(paths: &GxserverPaths, mode: TelemetryMode) {
    for filename in [identity::FULL_ID_FILE, identity::WEEKLY_ID_FILE] {
        if identity::file_for_mode(mode) != Some(filename) {
            identity::delete_install_id(paths, filename);
        }
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

    /// Resolve an identity for the requested mode. Preview keeps a generated ID in
    /// memory so the eventual send uses the same value without preview writing to disk.
    pub(super) fn distinct_id(&self, mode: TelemetryMode, persist: bool) -> Option<String> {
        let filename = identity::file_for_mode(mode)?;
        let mut cached = self.distinct_id.lock().unwrap_or_else(|p| p.into_inner());
        if persist {
            // Settings may have changed while the flush was preparing the batch.
            if gate::mode(&self.paths) != mode {
                return None;
            }
            retire_other_ids(&self.paths, mode);
        }
        if cached
            .as_ref()
            .is_none_or(|(cached_mode, _)| *cached_mode != mode)
        {
            *cached = Some((mode, load_id(&self.paths, filename)));
        }
        let id = cached.as_ref().expect("resolved identity").1.clone();
        if persist && identity::read_install_id(&self.paths, filename).as_deref() != Some(&id) {
            identity::persist_install_id(&self.paths, filename, &id);
        }
        Some(id)
    }

    pub(crate) fn distinct_id_for_preview(&self, mode: TelemetryMode) -> Option<String> {
        self.distinct_id(mode, false)
    }

    pub(super) fn retire_other_ids(&self, mode: TelemetryMode) {
        let mut cached = self.distinct_id.lock().unwrap_or_else(|p| p.into_inner());
        if gate::mode(&self.paths) != mode {
            return;
        }
        retire_other_ids(&self.paths, mode);
        if cached
            .as_ref()
            .is_some_and(|(cached_mode, _)| *cached_mode != mode)
        {
            *cached = None;
        }
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
            Disabled means DROP and WIPE product events immediately, not defer:
            a queue or persisted state that survives an opt-out would ship
            pre-opt-out activity if the user ever opted back in.
            */
            if gate::is_hard_opted_out(&self.paths) {
                self.drop_queue();
                self.retire_other_ids(TelemetryMode::Off);
            } else {
                self.drop_product_events();
            }
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

    /// Enqueue the minimal disclosed weekly active install count (`install.weekly_count`).
    /// Contains strictly `analytics_enabled` and `$process_person_profile: false` with zero
    /// base, profile, OS, agent, feature, account, or settings properties (Q29).
    /// Strictly blocked on hard opt-out (`DO_NOT_TRACK=1`, `GHOSTEX_TELEMETRY_DISABLED=1`,
    /// `--no-analytics`, or remote helper).
    pub(super) fn enqueue_weekly_count(&self, analytics_enabled: bool) {
        if gate::is_hard_opted_out(&self.paths) {
            self.drop_queue();
            self.retire_other_ids(TelemetryMode::Off);
            super::task::purge_on_opt_out_if_needed(&self.paths);
            return;
        }
        let validated = match taxonomy::validate(
            taxonomy::EVENT_INSTALL_WEEKLY_COUNT,
            &[
                ("analytics_enabled", PropertyValue::Bool(analytics_enabled)),
                (
                    taxonomy::PROCESS_PERSON_PROFILE_KEY,
                    PropertyValue::Bool(false),
                ),
            ],
        ) {
            Ok(validated) => validated,
            Err(reason) => {
                super::debug_log(format!("telemetry weekly count dropped: {reason}"));
                return;
            }
        };
        let queued = QueuedEvent {
            event: taxonomy::EVENT_INSTALL_WEEKLY_COUNT.to_string(),
            properties: validated,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            attempts: 0,
        };
        let mut queue = self.lock_queue();
        queue.retain(|item| item.event != taxonomy::EVENT_INSTALL_WEEKLY_COUNT);
        while queue.len() >= MAX_QUEUED_EVENTS {
            queue.pop_front();
        }
        queue.push_back(queued);
    }

    pub(super) fn drop_queue(&self) {
        self.lock_queue().clear();
    }

    /// Drops all queued product events while preserving a pending minimal
    /// `install.weekly_count` event (if any) when the UI toggle is OFF.
    pub(super) fn drop_product_events(&self) {
        self.lock_queue()
            .retain(|queued| queued.event == taxonomy::EVENT_INSTALL_WEEKLY_COUNT);
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

/// Build the exact minimal `install.weekly_count` preview payload without
/// mutating the queue, SQLite, or disk. Used by `POST /api/telemetryPreview`.
pub fn build_weekly_count_preview_event(
    distinct_id: Option<&str>,
    analytics_enabled: bool,
) -> Option<Value> {
    let validated = taxonomy::validate(
        taxonomy::EVENT_INSTALL_WEEKLY_COUNT,
        &[
            ("analytics_enabled", PropertyValue::Bool(analytics_enabled)),
            (
                taxonomy::PROCESS_PERSON_PROFILE_KEY,
                PropertyValue::Bool(false),
            ),
        ],
    )
    .ok()?;
    Some(json!({
        "event": taxonomy::EVENT_INSTALL_WEEKLY_COUNT,
        "distinct_id": distinct_id,
        "properties": Value::Object(validated),
    }))
}

/// Replace the DB-derived profile fields. A no-op in a process that never
/// initialised telemetry, like every one-shot CLI verb.
pub fn refresh_profile(snapshot: ProfileSnapshot) {
    if let Some(telemetry) = handle() {
        telemetry.refresh_profile(snapshot);
    }
}

/// Enqueue `install.weekly_count` (Q29). A no-op when uninitialized or hard opted out.
pub fn capture_weekly_count(analytics_enabled: bool) {
    let Some(telemetry) = handle() else {
        return;
    };
    telemetry.enqueue_weekly_count(analytics_enabled);
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
        if gate::is_hard_opted_out(telemetry.paths()) {
            telemetry.drop_queue();
            telemetry.retire_other_ids(TelemetryMode::Off);
        } else {
            telemetry.drop_product_events();
        }
        super::task::purge_on_opt_out_if_needed(telemetry.paths());
        return;
    }
    if !telemetry.claim_throttle(format!("{event}:{throttle_key}"), window) {
        return;
    }
    telemetry.enqueue(event, properties);
}
