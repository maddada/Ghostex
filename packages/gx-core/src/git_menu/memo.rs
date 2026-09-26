//! The Git state leases, ported from packages/shared/sidebar-git-state-memo.ts.
//!
//! CDXC:Git 2026-07-29:
//! Switching projects used to re-run the whole Git read every single time, because only the last
//! project refreshed was remembered, so switching A -> B -> A paid the full cost again and starved
//! the terminal-attach calls sharing the same daemon. This is the caching policy for that read: a
//! bounded, time-to-live keyed memo with least-recently-used eviction. It reads no clock; every
//! entry point takes `now_ms`.

/// How long a computed local Git state stays publishable without re-reading. 45s is short enough
/// that a returning user sees near-live counters and long enough that rapid project switching
/// costs zero calls.
pub const GIT_STATE_MEMO_TTL_MS: u64 = 45 * 1000;

/// How long GitHub CLI results (`gh --version`, `gh pr view`) stay publishable: `gh pr view` is a
/// network round trip and pull-request state changes on a human timescale.
pub const GIT_HUB_MEMO_TTL_MS: u64 = 5 * 60 * 1000;

/// Upper bound on remembered projects.
pub const GIT_MEMO_MAX_ENTRIES: usize = 50;

#[derive(Clone, Debug)]
struct Entry<V> {
    key: String,
    stored_at_ms: u64,
    value: V,
}

/// Bounded key/value memo with a fixed TTL and least-recently-used eviction. Reading an entry
/// never extends its TTL: freshness is decided purely by when the value was stored.
#[derive(Clone, Debug)]
pub struct GitTtlMemo<V> {
    /// Oldest touched first.
    entries: Vec<Entry<V>>,
    max_entries: usize,
    ttl_ms: u64,
}

impl<V: Clone> GitTtlMemo<V> {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: GIT_MEMO_MAX_ENTRIES,
            ttl_ms,
        }
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.key == key)
    }

    fn is_fresh(&self, entry: &Entry<V>, now_ms: u64) -> bool {
        now_ms.saturating_sub(entry.stored_at_ms) < self.ttl_ms
    }

    /// The fresh value for `key`, or `None` when the key is unknown or expired. An expired entry
    /// is dropped so a later [`GitTtlMemo::peek`] cannot revive it.
    pub fn get(&mut self, key: &str, now_ms: u64) -> Option<V> {
        let index = self.position(key)?;
        let entry = self.entries.remove(index);
        if !self.is_fresh(&entry, now_ms) {
            return None;
        }
        let value = entry.value.clone();
        self.entries.push(entry);
        Some(value)
    }

    /// The last stored value regardless of age, without touching recency: the stale-while-revalidate
    /// GitHub path shows the previous pull-request badge while a fresh probe is in flight.
    pub fn peek(&self, key: &str) -> Option<&V> {
        self.position(key).map(|index| &self.entries[index].value)
    }

    pub fn is_fresh_key(&self, key: &str, now_ms: u64) -> bool {
        self.position(key)
            .is_some_and(|index| self.is_fresh(&self.entries[index], now_ms))
    }

    pub fn set(&mut self, key: &str, value: V, now_ms: u64) {
        if let Some(index) = self.position(key) {
            self.entries.remove(index);
        }
        self.entries.push(Entry {
            key: key.to_string(),
            stored_at_ms: now_ms,
            value,
        });
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn delete(&mut self, key: &str) {
        if let Some(index) = self.position(key) {
            self.entries.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
