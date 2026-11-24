//! Shared single-flight async section cache and update notification for the Session details panel.
//!
//! Each slow probe (git diff, `gh pr view`, `ps`, `codex mcp list`, `grok inspect`, skill scans,
//! connectors, OpenCode HTTP) caches its own result here with stale-while-revalidate and single-flight
//! deduplication. When a background probe finishes it bumps the revision counter and wakes any
//! progressive `/api/readSessionDetails` request waiting on `afterRevision`, so every card renders
//! as soon as its own data arrives without blocking any other card.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

static DETAILS_REVISION: AtomicU64 = AtomicU64::new(1);

fn update_notify() -> &'static Notify {
    static NOTIFY: OnceLock<Notify> = OnceLock::new();
    NOTIFY.get_or_init(Notify::new)
}

/// Current monotonic revision of background section completions.
pub(super) fn current_revision() -> u64 {
    DETAILS_REVISION.load(Ordering::Acquire)
}

/// Bumps the revision and wakes any progressive readers waiting for a section to land.
pub(super) fn notify_update() {
    DETAILS_REVISION.fetch_add(1, Ordering::AcqRel);
    update_notify().notify_waiters();
}

/// Waits up to `timeout` for the revision to move past `after`.
pub(super) async fn wait_for_revision_change(after: u64, timeout: Duration) {
    if current_revision() != after {
        return;
    }
    let notified = update_notify().notified();
    tokio::pin!(notified);
    // Register the waiter before re-checking the revision so no wakeup between check and await is lost.
    notified.as_mut().enable();
    if current_revision() != after {
        return;
    }
    let _ = tokio::time::timeout(timeout, notified).await;
}

/// Result of a non-blocking section lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SectionLoad<T> {
    /// Data is ready right now (fresh, or stale while a background refresh runs).
    Ready(T),
    /// No cached data yet; a deduplicated background probe is running and will call `notify_update()`.
    Pending,
}

impl<T> SectionLoad<T> {
    pub(super) fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    pub(super) fn into_ready(self) -> Option<T> {
        match self {
            Self::Ready(value) => Some(value),
            Self::Pending => None,
        }
    }
}

struct Entry<V> {
    value: Option<V>,
    fetched_at: Option<Instant>,
    in_flight: bool,
    waiters: Arc<(Mutex<u64>, Condvar)>,
}

impl<V> Default for Entry<V> {
    fn default() -> Self {
        Self {
            value: None,
            fetched_at: None,
            in_flight: false,
            waiters: Arc::new((Mutex::new(0), Condvar::new())),
        }
    }
}

/// Single-flight, stale-while-revalidate cache keyed by `K`.
pub(super) struct AsyncSectionCache<K, V> {
    entries: Mutex<HashMap<K, Entry<V>>>,
    max_entries: usize,
}

impl<K, V> AsyncSectionCache<K, V>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
{
    pub(super) fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    /// Returns a fresh value immediately, returns a stale value immediately while refreshing in the
    /// background, or starts a background probe and returns `SectionLoad::Pending` on a cold miss.
    pub(super) fn get_or_start<F>(
        &'static self,
        key: K,
        fresh_for: Duration,
        force_stale: bool,
        compute: F,
    ) -> SectionLoad<V>
    where
        F: FnOnce() -> V + Send + 'static,
    {
        let (result, should_spawn, waiters) = {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if entries.len() >= self.max_entries && !entries.contains_key(&key) {
                if let Some(oldest) = entries
                    .iter()
                    .filter(|(_, entry)| !entry.in_flight)
                    .min_by_key(|(_, entry)| entry.fetched_at)
                    .map(|(k, _)| k.clone())
                {
                    entries.remove(&oldest);
                }
            }
            let entry = entries.entry(key.clone()).or_default();
            let is_fresh =
                !force_stale && entry.fetched_at.is_some_and(|at| at.elapsed() < fresh_for);
            if is_fresh {
                if let Some(value) = entry.value.clone() {
                    return SectionLoad::Ready(value);
                }
            }
            let should_spawn = !entry.in_flight;
            if should_spawn {
                entry.in_flight = true;
            }
            let result = match entry.value.clone() {
                Some(stale) => SectionLoad::Ready(stale),
                None => SectionLoad::Pending,
            };
            (result, should_spawn, entry.waiters.clone())
        };

        if should_spawn {
            self.spawn_refresh(key, waiters, compute);
        }
        result
    }

    /// Blocking lookup with single-flight deduplication and a hard time-box (for legacy / CLI callers).
    /// Returns cached/stale immediately if fresh or if `timeout` elapses while a cold probe is still running.
    pub(super) fn get_or_wait<F>(
        &'static self,
        key: K,
        fresh_for: Duration,
        force_stale: bool,
        timeout: Duration,
        compute: F,
    ) -> Option<V>
    where
        F: FnOnce() -> V + Send + 'static,
    {
        let (immediate, should_spawn, waiters, start_gen) = {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if entries.len() >= self.max_entries && !entries.contains_key(&key) {
                if let Some(oldest) = entries
                    .iter()
                    .filter(|(_, entry)| !entry.in_flight)
                    .min_by_key(|(_, entry)| entry.fetched_at)
                    .map(|(k, _)| k.clone())
                {
                    entries.remove(&oldest);
                }
            }
            let entry = entries.entry(key.clone()).or_default();
            let is_fresh =
                !force_stale && entry.fetched_at.is_some_and(|at| at.elapsed() < fresh_for);
            if is_fresh {
                if let Some(value) = entry.value.clone() {
                    return Some(value);
                }
            }
            let should_spawn = !entry.in_flight;
            if should_spawn {
                entry.in_flight = true;
            }
            let immediate = entry.value.clone();
            let start_gen = *entry
                .waiters
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            (immediate, should_spawn, entry.waiters.clone(), start_gen)
        };

        if should_spawn {
            self.spawn_refresh(key.clone(), waiters.clone(), compute);
        }

        // If we already have a stale value, return it right away while the background refresh runs.
        if let Some(stale) = immediate {
            return Some(stale);
        }

        // Cold miss: wait up to `timeout` for the in-flight probe to complete.
        let deadline = Instant::now() + timeout;
        let (lock, condvar) = &*waiters;
        let mut generation = lock.lock().unwrap_or_else(|error| error.into_inner());
        while *generation == start_gen {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next_guard, wait_result) = condvar
                .wait_timeout(generation, remaining)
                .unwrap_or_else(|error| error.into_inner());
            generation = next_guard;
            if wait_result.timed_out() {
                break;
            }
        }
        drop(generation);

        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(&key).and_then(|entry| entry.value.clone()))
    }

    /// Seeds a cold entry without overwriting an already-cached value.
    pub(super) fn seed_if_empty(&'static self, key: K, value: V, age: Duration) {
        if let Ok(mut entries) = self.entries.lock() {
            let entry = entries.entry(key).or_default();
            if entry.value.is_none() {
                entry.value = Some(value);
                entry.fetched_at = Instant::now().checked_sub(age);
            }
        }
    }

    /// Reads the last stored value without triggering any probe.
    #[allow(dead_code)]
    pub(super) fn peek(&'static self, key: &K) -> Option<V> {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(key).and_then(|entry| entry.value.clone()))
    }

    fn spawn_refresh<F>(&'static self, key: K, waiters: Arc<(Mutex<u64>, Condvar)>, compute: F)
    where
        F: FnOnce() -> V + Send + 'static,
    {
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(compute));
            {
                let mut entries = self
                    .entries
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let entry = entries.entry(key).or_default();
                entry.in_flight = false;
                if let Ok(value) = result {
                    entry.value = Some(value);
                    entry.fetched_at = Some(Instant::now());
                }
            }
            let (lock, condvar) = &*waiters;
            {
                let mut generation = lock.lock().unwrap_or_else(|error| error.into_inner());
                *generation = generation.wrapping_add(1);
            }
            condvar.notify_all();
            notify_update();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn cold_miss_is_pending_then_ready_and_deduplicated() {
        static CACHE: OnceLock<AsyncSectionCache<String, usize>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| AsyncSectionCache::new(8));
        static CALLS: AtomicUsize = AtomicUsize::new(0);

        let rev_before = current_revision();
        let first = cache.get_or_start("k1".to_string(), Duration::from_secs(60), false, || {
            std::thread::sleep(Duration::from_millis(40));
            CALLS.fetch_add(1, Ordering::SeqCst) + 1
        });
        assert_eq!(first, SectionLoad::Pending);

        // A second concurrent call joins the same in-flight work instead of spawning another.
        let second = cache.get_or_start("k1".to_string(), Duration::from_secs(60), false, || {
            CALLS.fetch_add(1, Ordering::SeqCst) + 100
        });
        assert_eq!(second, SectionLoad::Pending);

        // Wait for the background thread to land.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut landed = None;
        while Instant::now() < deadline {
            if let SectionLoad::Ready(v) =
                cache.get_or_start("k1".to_string(), Duration::from_secs(60), false, || 999)
            {
                landed = Some(v);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(landed, Some(1));
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        assert!(current_revision() > rev_before);
    }

    #[test]
    fn stale_entry_serves_immediately_while_refreshing() {
        static CACHE: OnceLock<AsyncSectionCache<String, usize>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| AsyncSectionCache::new(8));
        cache.seed_if_empty("k2".to_string(), 10, Duration::from_secs(120));

        // Returns stale 10 right away while refreshing to 20 in background.
        let stale = cache.get_or_start("k2".to_string(), Duration::from_secs(10), false, || {
            std::thread::sleep(Duration::from_millis(20));
            20
        });
        assert_eq!(stale, SectionLoad::Ready(10));

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut fresh = 10;
        while Instant::now() < deadline {
            if let SectionLoad::Ready(v) =
                cache.get_or_start("k2".to_string(), Duration::from_secs(60), false, || 999)
            {
                if v == 20 {
                    fresh = v;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fresh, 20);
    }
}
